//! Each input resource has an independent lifetime. Configuration transactions
//! enqueue coalesced work; they never install, stop or wait for native hooks.
use super::*;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

static CORE_NEEDED: AtomicBool = AtomicBool::new(false);
static CAPABILITY_NEEDED: AtomicBool = AtomicBool::new(false);
static KEYBOARD_ACCEPTING_SHORTCUTS: AtomicBool = AtomicBool::new(false);
static REFRESH_QUEUED: AtomicBool = AtomicBool::new(false);
static REFRESH_DIRTY: AtomicBool = AtomicBool::new(false);
static WORKER: Mutex<InputResources> = Mutex::new(InputResources {
    keyboard: None, mouse: None, events: None,
});
static EVENT_TX: Mutex<Option<SyncSender<&'static str>>> = Mutex::new(None);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

struct HookWorker {
    thread_id: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
    handle: JoinHandle<()>,
    done: Receiver<()>,
    stopping: bool,
}

struct EventWorker {
    handle: JoinHandle<()>,
    done: Receiver<()>,
    stopping: bool,
}

struct InputResources {
    keyboard: Option<HookWorker>,
    mouse: Option<HookWorker>,
    events: Option<EventWorker>,
}

impl InputResources {
    fn is_empty(&self) -> bool {
        self.keyboard.is_none() && self.mouse.is_none() && self.events.is_none()
    }
}

struct Demand {
    keyboard: bool,
    mouse: bool,
    forward_events: bool,
}

impl Demand {
    fn current() -> Self {
        let pet = BONGO_CAT_INPUT_ENABLED.load(Ordering::Acquire)
            && BONGO_CAT_INPUT_VISIBLE.load(Ordering::Acquire)
            && OPTIONAL_SHORTCUTS_ALLOWED.load(Ordering::Acquire);
        let stats_overlay = STATS_OVERLAY_MINI_INPUT_ENABLED.load(Ordering::Acquire);
        Self {
            keyboard: CORE_NEEDED.load(Ordering::Acquire) || pet
                || (OPTIONAL_SHORTCUTS_ALLOWED.load(Ordering::Acquire)
                    && CAPABILITY_NEEDED.load(Ordering::Acquire)),
            mouse: pet || stats_overlay,
            forward_events: pet || stats_overlay,
        }
    }
}

extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn PostThreadMessageW(thread_id: u32, message: u32, wparam: usize, lparam: isize) -> i32;
    fn PeekMessageW(msg: *mut MSG, window: isize, min: u32, max: u32, remove: u32) -> i32;
}

pub(super) fn set_core_input_needed(needed: bool) {
    CORE_NEEDED.store(needed, Ordering::Release);
}

pub(super) fn set_capability_input_needed(needed: bool) {
    CAPABILITY_NEEDED.store(needed, Ordering::Release);
}

pub(super) fn keyboard_accepts_shortcuts() -> bool {
    KEYBOARD_ACCEPTING_SHORTCUTS.load(Ordering::Acquire)
}

pub(super) fn initialize() {
    INITIALIZED.store(true, Ordering::Release);
    request_refresh();
}

pub(super) fn request_refresh() {
    if !INITIALIZED.load(Ordering::Acquire) { return; }
    let demand = Demand::current();
    if !demand.keyboard && !demand.mouse
        && WORKER.try_lock().is_ok_and(|worker| worker.is_empty())
        && !REFRESH_QUEUED.load(Ordering::Acquire)
    {
        return;
    }
    REFRESH_DIRTY.store(true, Ordering::Release);
    if REFRESH_QUEUED.swap(true, Ordering::AcqRel) { return; }
    tauri::async_runtime::spawn_blocking(|| {
        loop {
            REFRESH_DIRTY.store(false, Ordering::Release);
            if let Err(error) = refresh() {
                crate::logger::log_msg("ERROR", "Input", &error);
            }
            if !REFRESH_DIRTY.load(Ordering::Acquire) { break; }
        }
        REFRESH_QUEUED.store(false, Ordering::Release);
        if REFRESH_DIRTY.load(Ordering::Acquire) { request_refresh(); }
    });
}

pub(super) fn emit_input_event(event: &'static str) -> bool {
    if let Ok(sender) = EVENT_TX.try_lock() {
        if let Some(sender) = sender.as_ref() {
            return sender.try_send(event).is_ok();
        }
    }
    false
}

fn refresh() -> Result<(), String> {
    let mut resources = WORKER.lock().map_err(|_| "输入服务状态不可用".to_string())?;
    let demand = Demand::current();
    // If a key-down was consumed, keep the keyboard hook until its matching
    // key-up arrives. That callback requests the final cleanup itself.
    let keyboard_needed = {
        let handled = active_handled_shortcut_keys().lock();
        // Serialize retirement with key-down dispatch so a final key cannot
        // be swallowed between checking the set and stopping its hook.
        KEYBOARD_ACCEPTING_SHORTCUTS.store(demand.keyboard, Ordering::Release);
        demand.keyboard || !handled.is_empty()
    };
    let mut errors = Vec::new();
    // Both the pet and mini statistics overlay consume forwarded events.
    // Prepare delivery before installing a new hook that can produce them.
    if let Err(error) = reconcile_events(&mut resources.events, demand.forward_events) {
        errors.push(error);
    }
    if let Err(error) = reconcile_hook(
        &mut resources.keyboard, keyboard_needed, WH_KEYBOARD_LL,
        Some(keyboard_hook_proc), &KEYBOARD_HOOK,
    ) {
        errors.push(error);
    }
    if let Err(error) = reconcile_hook(
        &mut resources.mouse, demand.mouse, WH_MOUSE_LL,
        Some(mouse_hook_proc), &MOUSE_HOOK,
    ) {
        errors.push(error);
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("；")) }
}

fn reconcile_hook(
    worker: &mut Option<HookWorker>,
    needed: bool,
    kind: i32,
    callback: HOOKPROC,
    slot: &'static AtomicPtr<std::ffi::c_void>,
) -> Result<(), String> {
    if worker.as_ref().is_some_and(|worker| needed && !worker.stopping && !worker.handle.is_finished()) {
        return Ok(());
    }
    if let Some(current) = worker.as_mut() {
        if !current.handle.is_finished() {
            let thread_id = current.thread_id.load(Ordering::Acquire);
            if thread_id != 0 && unsafe { PostThreadMessageW(thread_id, 0x0012, 0, 0) } == 0 {
                return Err(format!("无法停止输入服务 {kind}，已保留其生命周期状态"));
            }
            current.cancelled.store(true, Ordering::Release);
            current.stopping = true;
            if matches!(current.done.recv_timeout(STOP_TIMEOUT), Err(mpsc::RecvTimeoutError::Timeout)) {
                return Err(format!("输入服务 {kind} 尚未停止，已保留线程句柄供后续清理"));
            }
        }
        if let Some(previous) = worker.take() { let _ = previous.handle.join(); }
        if kind == WH_KEYBOARD_LL {
            active_handled_shortcut_keys().lock().clear();
        }
    }
    if needed {
        start_hook(worker, kind, callback, slot)?;
    }
    Ok(())
}

fn start_hook(
    owner: &mut Option<HookWorker>,
    kind: i32,
    callback: HOOKPROC,
    slot: &'static AtomicPtr<std::ffi::c_void>,
) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let thread_id = Arc::new(AtomicU32::new(0));
    let worker_id = thread_id.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let handle = std::thread::Builder::new().name(format!("input-hook-{kind}"))
        .spawn(move || unsafe {
            let mut msg = std::mem::zeroed::<MSG>();
            PeekMessageW(&mut msg, 0, 0, 0, 0);
            worker_id.store(GetCurrentThreadId(), Ordering::Release);
            if worker_cancelled.load(Ordering::Acquire) { return; }
            let hook = SetWindowsHookExW(kind, callback, std::ptr::null_mut(), 0);
            if hook.is_null() {
                let _ = ready_tx.send(Err(format!("无法注册输入服务 {kind}: {}", std::io::Error::last_os_error())));
                return;
            }
            slot.store(hook, Ordering::SeqCst);
            let guard = HookGuard::new(hook, slot);
            if worker_cancelled.load(Ordering::Acquire) || ready_tx.send(Ok(())).is_err() { return; }
            while GetMessageW(&mut msg as *mut MSG as *mut _, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg as *const MSG as *const _);
                DispatchMessageW(&msg as *const MSG as *const _);
            }
            drop(guard);
            let _ = done_tx.send(());
        }).map_err(|error| format!("无法启动输入服务 {kind}: {error}"))?;
    // Publish ownership before waiting. On a timeout the late worker remains
    // tracked, preventing a second worker from replacing its global hook slot.
    *owner = Some(HookWorker { thread_id, cancelled, handle, done: done_rx, stopping: false });
    match ready_rx.recv_timeout(STOP_TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        result => {
            if let Some(worker) = owner.as_mut() {
                worker.cancelled.store(true, Ordering::Release);
                worker.stopping = true;
                let id = worker.thread_id.load(Ordering::Acquire);
                if id != 0 { unsafe { PostThreadMessageW(id, 0x0012, 0, 0); } }
            }
            Err(match result {
                Ok(Err(error)) => error,
                Err(error) => format!("输入服务 {kind} 启动未完成: {error}"),
                Ok(Ok(())) => unreachable!(),
            })
        }
    }
}

fn reconcile_events(worker: &mut Option<EventWorker>, needed: bool) -> Result<(), String> {
    if worker.as_ref().is_some_and(|worker| needed && !worker.stopping && !worker.handle.is_finished()) {
        return Ok(());
    }
    if let Some(current) = worker.as_mut() {
        EVENT_TX.lock().map_err(|_| "输入事件状态不可用".to_string())?.take();
        current.stopping = true;
        if !current.handle.is_finished()
            && matches!(current.done.recv_timeout(STOP_TIMEOUT), Err(mpsc::RecvTimeoutError::Timeout))
        {
            return Err("输入事件转发尚未停止，已保留线程句柄供后续清理".to_string());
        }
        if let Some(previous) = worker.take() { let _ = previous.handle.join(); }
    }
    if needed {
        let app = APP_HANDLE.lock().ok().and_then(|app| app.clone())
            .ok_or_else(|| "应用尚未激活".to_string())?;
        let (sender, receiver) = mpsc::sync_channel(64);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let handle = std::thread::Builder::new().name("optional-input-events".to_string())
            .spawn(move || {
                while let Ok(event) = receiver.recv() {
                    let _ = app.emit("global-input-event", event);
                }
                let _ = done_tx.send(());
            }).map_err(|error| format!("无法启动输入事件转发: {error}"))?;
        *worker = Some(EventWorker { handle, done: done_rx, stopping: false });
        *EVENT_TX.lock().map_err(|_| "输入事件状态不可用".to_string())? = Some(sender);
    }
    Ok(())
}
