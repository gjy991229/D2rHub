//! Passive desktop-pet input, modeled on rdev's shared keyboard/mouse listener.
//! Both hooks share one message thread and always pass input onward.
use super::*;
use std::sync::{mpsc, Arc};
use std::sync::atomic::AtomicU32;
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

#[link(name = "kernel32")]
extern "system" { fn GetCurrentThreadId() -> u32; }

const TIMEOUT: Duration = Duration::from_secs(3);
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static QUEUED: AtomicBool = AtomicBool::new(false);
static DIRTY: AtomicBool = AtomicBool::new(false);
static WORKER: Mutex<Option<PetListener>> = Mutex::new(None);
static EVENT_TX: Mutex<Option<mpsc::SyncSender<&'static str>>> = Mutex::new(None);

struct PetListener {
    thread_id: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
    done: mpsc::Receiver<()>,
    handle: JoinHandle<()>,
    events: JoinHandle<()>,
    stopping: bool,
}
struct Hook(HHOOK);
impl Drop for Hook {
    fn drop(&mut self) { let _ = unsafe { UnhookWindowsHookEx(self.0) }; }
}
struct Completion(mpsc::SyncSender<()>);
impl Drop for Completion {
    fn drop(&mut self) {
        // End the forwarder even if the native message loop exits unexpectedly.
        if let Ok(mut sender) = EVENT_TX.lock() { sender.take(); }
        let _ = self.0.send(());
    }
}

fn needed() -> bool {
    INITIALIZED.load(Ordering::Acquire)
        && BONGO_CAT_INPUT_ENABLED.load(Ordering::Acquire)
        && BONGO_CAT_INPUT_VISIBLE.load(Ordering::Acquire)
        && OPTIONAL_SHORTCUTS_ALLOWED.load(Ordering::Acquire)
}

fn emit(event: &'static str) {
    if !needed() { return; }
    if let Ok(sender) = EVENT_TX.try_lock() {
        if let Some(sender) = sender.as_ref() { let _ = sender.try_send(event); }
    }
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
        let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if event.dwExtraInfo != crate::infrastructure::physical_input::INPUT_TAG { emit("Keyboard"); }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}
unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wparam.0 as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN) {
        let event = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        if event.dwExtraInfo != crate::infrastructure::physical_input::INPUT_TAG {
            emit(if wparam.0 as u32 == WM_LBUTTONDOWN { "MouseLeft" } else { "MouseRight" });
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

pub(super) fn initialize() {
    INITIALIZED.store(true, Ordering::Release);
    request_refresh();
}
pub(super) fn request_refresh() {
    if !INITIALIZED.load(Ordering::Acquire) { return; }
    DIRTY.store(true, Ordering::Release);
    if QUEUED.swap(true, Ordering::AcqRel) { return; }
    tauri::async_runtime::spawn_blocking(|| {
        loop {
            DIRTY.store(false, Ordering::Release);
            if let Err(error) = reconcile() { crate::logger::log_msg("ERROR", "PetInput", &error); }
            if !DIRTY.load(Ordering::Acquire) { break; }
        }
        QUEUED.store(false, Ordering::Release);
        if DIRTY.load(Ordering::Acquire) { request_refresh(); }
    });
}

fn reconcile() -> Result<(), String> {
    let mut slot = WORKER.lock().map_err(|_| "桌宠输入服务状态不可用")?;
    if slot.as_ref().is_some_and(|worker| needed() && !worker.stopping && !worker.handle.is_finished()) {
        return Ok(());
    }
    if let Some(worker) = slot.as_mut() {
        EVENT_TX.lock().map_err(|_| "桌宠事件队列不可用")?.take();
        worker.stopping = true;
        worker.cancelled.store(true, Ordering::Release);
        if !worker.handle.is_finished() {
            let id = worker.thread_id.load(Ordering::Acquire);
            if id != 0 {
                unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) }
                    .map_err(|error| format!("停止桌宠监听失败：{error}"))?;
            }
            if matches!(worker.done.recv_timeout(TIMEOUT), Err(mpsc::RecvTimeoutError::Timeout)) {
                return Err("桌宠输入监听尚未停止，已保留线程所有权".into());
            }
        }
    }
    if let Some(worker) = slot.take() {
        let _ = worker.handle.join();
        let _ = worker.events.join();
    }
    if !needed() { return Ok(()); }
    let app = APP_HANDLE.lock().ok().and_then(|app| app.clone()).ok_or("应用尚未激活")?;
    let (sender, receiver) = mpsc::sync_channel(64);
    let events = std::thread::Builder::new().name("pet-input-events".into()).spawn(move || {
        while let Ok(event) = receiver.recv() { let _ = app.emit("global-input-event", event); }
    }).map_err(|error| format!("启动桌宠事件转发失败：{error}"))?;
    *EVENT_TX.lock().map_err(|_| "桌宠事件队列不可用")? = Some(sender);
    let thread_id = Arc::new(AtomicU32::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (done_tx, done) = mpsc::sync_channel(1);
    let (ready_tx, ready) = mpsc::sync_channel(1);
    let id = thread_id.clone();
    let stop = cancelled.clone();
    let handle = std::thread::Builder::new().name("pet-input-listener".into()).spawn(move || {
        let _done = Completion(done_tx);
        let mut msg = MSG::default();
        unsafe { PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_NOREMOVE); }
        id.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
        if stop.load(Ordering::Acquire) { return; }
        let install = || -> Result<(Hook, Hook), String> {
            let keyboard = Hook(unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), HINSTANCE::default(), 0) }
                .map_err(|error| format!("安装桌宠键盘监听失败：{error}"))?);
            let mouse = Hook(unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), HINSTANCE::default(), 0) }
                .map_err(|error| format!("安装桌宠鼠标监听失败：{error}"))?);
            Ok((keyboard, mouse))
        };
        let _hooks = match install() {
            Ok(hooks) => hooks,
            Err(error) => { let _ = ready_tx.send(Err(error)); return; }
        };
        if stop.load(Ordering::Acquire) || ready_tx.send(Ok(())).is_err() { return; }
        while unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.0 > 0 {
            if stop.load(Ordering::Acquire) { break; }
            unsafe { TranslateMessage(&msg); DispatchMessageW(&msg); }
        }
    });
    let handle = match handle {
        Ok(handle) => handle,
        Err(error) => {
            EVENT_TX.lock().map_err(|_| "桌宠事件队列不可用")?.take();
            let _ = events.join();
            return Err(format!("启动桌宠输入监听失败：{error}"));
        }
    };
    *slot = Some(PetListener { thread_id, cancelled, done, handle, events, stopping: false });
    match ready.recv_timeout(TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        result => {
            if let Some(worker) = slot.as_mut() {
                worker.stopping = true;
                worker.cancelled.store(true, Ordering::Release);
                let _ = unsafe { PostThreadMessageW(worker.thread_id.load(Ordering::Acquire), WM_QUIT, WPARAM(0), LPARAM(0)) };
            }
            EVENT_TX.lock().map_err(|_| "桌宠事件队列不可用")?.take();
            match result {
                Ok(Err(error)) => Err(error),
                _ => Err("桌宠输入监听启动超时".into()),
            }
        }
    }
}

pub(super) fn shutdown() {
    INITIALIZED.store(false, Ordering::Release);
    if let Err(error) = reconcile() { crate::logger::log_msg("ERROR", "PetInput", &error); }
}
