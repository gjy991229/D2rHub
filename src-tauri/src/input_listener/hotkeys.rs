//! One Win32 message thread owns every business hotkey. No keyboard hooks.
use super::*;
use std::sync::atomic::{AtomicU32, AtomicUsize};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, PM_REMOVE, WM_APP, WM_HOTKEY,
    WM_QUIT,
};

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
}

const WORK_MESSAGE: u32 = WM_APP + 41;
const TIMEOUT: Duration = Duration::from_secs(3);
type Reply = mpsc::SyncSender<Result<(), String>>;
enum Request {
    Refresh {
        generation: u64,
        keys: Vec<String>,
        reply: Option<Reply>,
    },
    Validate(Vec<String>, Reply),
}
struct Worker {
    sender: mpsc::Sender<Request>,
    thread_id: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}
static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(1);
static SUSPENSIONS: AtomicUsize = AtomicUsize::new(0);

pub(super) fn initialize(app: AppHandle) -> Result<(), String> {
    let mut slot = WORKER.lock().map_err(|_| "系统热键服务状态不可用")?;
    if let Some(worker) = slot.as_ref().filter(|worker| !worker.handle.is_finished()) {
        return if worker.cancelled.load(Ordering::Acquire) {
            Err("系统热键线程仍在停止中".into())
        } else {
            Ok(())
        };
    }
    if let Some(worker) = slot.take() {
        let _ = worker.handle.join();
    }
    let (sender, receiver) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let thread_id = Arc::new(AtomicU32::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));
    let id = thread_id.clone();
    let stop = cancelled.clone();
    let handle = std::thread::Builder::new()
        .name("global-hotkeys".into())
        .spawn(move || {
            let mut msg = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_NOREMOVE);
            }
            id.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
            if stop.load(Ordering::Acquire) || ready_tx.send(()).is_err() {
                return;
            }
            let mut registrations = Registrations::default();
            let mut latest = 0;
            while unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.0 > 0 {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if msg.message == WORK_MESSAGE {
                    while let Ok(request) = receiver.try_recv() {
                        match request {
                            Request::Refresh {
                                generation,
                                keys,
                                reply,
                            } => {
                                let result = if generation >= latest {
                                    latest = generation;
                                    let keys = if blocked() { Vec::new() } else { keys };
                                    registrations.replace(&keys)
                                } else {
                                    Ok(())
                                };
                                if let Err(error) = &result {
                                    report_error(&app, error);
                                }
                                if let Some(reply) = reply {
                                    let _ = reply.send(result);
                                }
                            }
                            Request::Validate(keys, reply) => {
                                let result = if is_suspended() {
                                    Err("自动输入进行中，请在完成后修改快捷键".into())
                                } else {
                                    registrations.validate(&keys)
                                };
                                let _ = reply.send(result);
                            }
                        }
                    }
                } else if msg.message == WM_HOTKEY && !blocked() {
                    if let Some(key) = registrations.keys.get(&(msg.wParam.0 as i32)).cloned() {
                        // Never run window operations or configuration reads in this pump.
                        let app = app.clone();
                        std::thread::spawn(move || dispatch_registered_shortcut(&app, &key));
                    }
                }
            }
            // RAII unregisters on this same owning thread, including failures.
        })
        .map_err(|error| format!("启动系统热键线程失败：{error}"))?;
    *slot = Some(Worker {
        sender,
        thread_id,
        cancelled,
        handle,
    });
    if ready_rx.recv_timeout(TIMEOUT).is_err() {
        if let Some(worker) = slot.as_ref() {
            worker.cancelled.store(true, Ordering::Release);
            let _ = unsafe {
                PostThreadMessageW(
                    worker.thread_id.load(Ordering::Acquire),
                    WM_QUIT,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        }
        return Err("系统热键线程启动超时".into());
    }
    crate::logger::log_msg(
        "INFO",
        "Shortcut",
        "RegisterHotKey / WM_HOTKEY 系统热键服务已启动",
    );
    Ok(())
}

fn report_error(app: &AppHandle, error: &str) {
    crate::logger::log_msg("ERROR", "Shortcut", error);
    let _ = app.emit("global-hotkey-error", error);
}

fn send(request: Request) -> Result<bool, String> {
    let slot = WORKER.lock().map_err(|_| "系统热键服务状态不可用")?;
    let Some(worker) = slot.as_ref() else {
        return Ok(false);
    };
    if worker.handle.is_finished() || worker.cancelled.load(Ordering::Acquire) {
        return Err("系统热键线程已停止".into());
    }
    worker
        .sender
        .send(request)
        .map_err(|_| "系统热键线程已退出")?;
    unsafe {
        PostThreadMessageW(
            worker.thread_id.load(Ordering::Acquire),
            WORK_MESSAGE,
            WPARAM(0),
            LPARAM(0),
        )
    }
    .map_err(|error| format!("唤醒系统热键线程失败：{error}"))?;
    Ok(true)
}

fn refresh_request(reply: Option<Reply>) -> Request {
    let generation = GENERATION.fetch_add(1, Ordering::AcqRel);
    Request::Refresh {
        generation,
        keys: registered_shortcuts(),
        reply,
    }
}

pub(super) fn refresh() {
    if let Err(error) = send(refresh_request(None)) {
        if let Some(app) = APP_HANDLE.lock().ok().and_then(|app| app.clone()) {
            report_error(&app, &error);
        }
    }
}

pub(super) fn refresh_sync() -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel(1);
    if !send(refresh_request(Some(tx)))? {
        return Ok(());
    }
    rx.recv_timeout(TIMEOUT)
        .map_err(|_| "更新系统热键超时".to_string())?
}

pub(super) fn validate(keys: Vec<String>) -> Result<(), String> {
    for key in &keys {
        if !key.trim().is_empty() {
            parse(key)?;
        }
    }
    let (tx, rx) = mpsc::sync_channel(1);
    if !send(Request::Validate(keys, tx))? {
        return Ok(());
    }
    rx.recv_timeout(TIMEOUT)
        .map_err(|_| "检查系统热键占用超时".to_string())?
}

fn blocked() -> bool {
    SHORTCUT_CAPTURE_ACTIVE.load(Ordering::Acquire) || is_suspended()
}
pub(crate) fn is_suspended() -> bool {
    SUSPENSIONS.load(Ordering::Acquire) != 0
}

/// RegisterHotKey cannot distinguish tagged SendInput from physical input.
/// Unregister before automation injects input; restore only after it releases it.
pub(crate) struct Pause;
pub(crate) fn suspend() -> Result<Pause, String> {
    SUSPENSIONS.fetch_add(1, Ordering::AcqRel);
    if let Err(error) = refresh_sync() {
        SUSPENSIONS.fetch_sub(1, Ordering::AcqRel);
        refresh();
        return Err(error);
    }
    Ok(Pause)
}
impl Drop for Pause {
    fn drop(&mut self) {
        if SUSPENSIONS.fetch_sub(1, Ordering::AcqRel) == 1 {
            refresh();
        }
    }
}

pub(super) fn shutdown() {
    let worker = WORKER.lock().ok().and_then(|mut slot| slot.take());
    if let Some(worker) = worker {
        worker.cancelled.store(true, Ordering::Release);
        let posted = unsafe {
            PostThreadMessageW(
                worker.thread_id.load(Ordering::Acquire),
                WM_QUIT,
                WPARAM(0),
                LPARAM(0),
            )
        };
        if posted.is_ok() || worker.handle.is_finished() {
            let _ = worker.handle.join();
        }
    }
}

fn parse(value: &str) -> Result<(u32, u32), String> {
    let normalized = crate::capabilities::room_automation::canonicalize_shortcut(value)
        .map_err(|error| format!("快捷键 {value} 无效：{error}"))?
        .to_ascii_lowercase();
    let mut key = normalized.as_str();
    let mut modifiers = 0x4000; // MOD_NOREPEAT
    for (prefix, flag) in [("ctrl+", 2), ("alt+", 1), ("shift+", 4)] {
        if let Some(rest) = key.strip_prefix(prefix) {
            modifiers |= flag;
            key = rest;
        }
    }
    let vk = (0x08..=0xFE)
        .find(|vk| vk_to_key_string(*vk).eq_ignore_ascii_case(key))
        .ok_or_else(|| format!("无法注册快捷键 {value}：不支持的按键"))?;
    if vk == 0x7B {
        return Err("F12 是 Windows 调试器保留键，请选择其他快捷键".into());
    }
    Ok((modifiers, vk))
}

#[derive(Default)]
struct Registrations {
    keys: HashMap<i32, String>,
    next_id: i32,
}
impl Registrations {
    fn add(&mut self, key: &str) -> Result<i32, String> {
        let (modifiers, vk) = parse(key)?;
        if self.keys.is_empty() {
            // After a full pause, discard retired notifications before reusing
            // IDs. Repeated room-input sessions must not exhaust the ID space.
            let mut msg = MSG::default();
            while unsafe {
                PeekMessageW(&mut msg, HWND::default(), WM_HOTKEY, WM_HOTKEY, PM_REMOVE)
            }
            .as_bool()
            {}
            self.next_id = 0;
        }
        // Do not immediately reuse retired IDs: queued WM_HOTKEY messages for
        // a former binding must not dispatch a new action after reconfiguration.
        self.next_id += 1;
        if self.next_id > 0xBFFF {
            return Err("系统热键标识已用尽，请重启 D2RHub".into());
        }
        unsafe {
            RegisterHotKey(
                HWND::default(),
                self.next_id,
                HOT_KEY_MODIFIERS(modifiers),
                vk,
            )
        }
        .map_err(|error| format!("注册系统热键 {key} 失败，可能已被其他程序占用：{error}"))?;
        self.keys.insert(self.next_id, key.to_string());
        Ok(self.next_id)
    }
    fn remove(&mut self, id: i32) -> Result<(), String> {
        unsafe { UnregisterHotKey(HWND::default(), id) }
            .map_err(|error| format!("注销系统热键失败：{error}"))?;
        self.keys.remove(&id);
        Ok(())
    }
    fn validate(&mut self, keys: &[String]) -> Result<(), String> {
        let mut temporary = Vec::new();
        let mut result = Ok(());
        for key in keys.iter().filter(|key| !key.trim().is_empty()) {
            let key = key.trim().to_ascii_lowercase();
            if self.keys.values().any(|current| *current == key) {
                continue;
            }
            match self.add(&key) {
                Ok(id) => temporary.push(id),
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        for id in temporary {
            if let Err(error) = self.remove(id) {
                result = Err(error);
            }
        }
        result
    }
    fn replace(&mut self, keys: &[String]) -> Result<(), String> {
        let desired: HashSet<_> = keys
            .iter()
            .map(|key| key.trim().to_ascii_lowercase())
            .filter(|key| !key.is_empty())
            .collect();
        let retired: Vec<_> = self
            .keys
            .iter()
            .filter(|(_, key)| !desired.contains(*key))
            .map(|(id, _)| *id)
            .collect();
        let mut errors = Vec::new();
        for id in retired {
            if let Err(error) = self.remove(id) {
                errors.push(error);
            }
        }
        for key in desired {
            if !self.keys.values().any(|current| *current == key) {
                if let Err(error) = self.add(&key) {
                    errors.push(error);
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("；"))
        }
    }
}
impl Drop for Registrations {
    fn drop(&mut self) {
        for id in self.keys.keys() {
            let _ = unsafe { UnregisterHotKey(HWND::default(), *id) };
        }
    }
}
