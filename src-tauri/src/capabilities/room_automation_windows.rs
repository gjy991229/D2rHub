//! Windows keyboard delivery adapter for room automation.
//!
//! The application runtime owns cancellation and task lifecycles; this module
//! only translates one already-validated room form operation into native
//! window messages. Every wait and key boundary consults the caller's cancel
//! signal so capability shutdown never leaves detached input work behind.

use crate::capabilities::room_automation::{FlowStrategy, RoomAutomationConfig};
use crate::infrastructure::physical_input::{modifiers_released, DesktopInput};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::FILETIME;

const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const SMTO_BLOCK: u32 = 0x0001;
const SMTO_ABORTIFHUNG: u32 = 0x0002;
const SMTO_ERRORONEXIT: u32 = 0x0020;
const VK_BACK: u16 = 0x08;
const VK_TAB: u16 = 0x09;
const VK_RETURN: u16 = 0x0D;
const VK_SHIFT: u16 = 0x10;
const VK_ESCAPE: u16 = 0x1B;
const VK_END: u16 = 0x23;
const VK_LEFT: u16 = 0x25;
const VK_RIGHT: u16 = 0x27;
const MAPVK_VK_TO_VSC: u32 = 0;
const EXTRA_PANEL_SETTLE_MS: u64 = 50;
const ROOM_FORM_SETTLE_MS: u64 = 200;
const GATEWAY_DIRECTION_REPETITIONS: usize = 2;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

struct EnteredPassword {
    process_created_at: u64,
    hwnd: isize,
    create: bool,
    password: String,
}

// Shared by create/join and retained across capability restarts. Never persisted
// or published in workflow status. Creation time guards against PID reuse.
static ENTERED_PASSWORDS: OnceLock<Mutex<HashMap<u32, EnteredPassword>>> = OnceLock::new();

struct PreparedForm {
    created: u64,
    hwnd: isize,
    room_name: String,
}

static DESKTOP: Mutex<()> = Mutex::new(());

static PREPARED_FORMS: OnceLock<Mutex<HashMap<u32, PreparedForm>>> = OnceLock::new();

/// Open every form concurrently, then share physical Ctrl with posted A/V.
/// Only the primary receives Enter here; followers keep their prepared forms.
pub(crate) fn prepare_background_room(
    config: &RoomAutomationConfig,
    primary_pid: u32,
    follower_pids: &[u32],
    room_name: &str,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    validate_text(room_name)?;
    validate_text(&config.password)?;
    let _desktop = loop {
        cancel.check()?;
        if let Some(guard) = DESKTOP.try_lock() {
            break guard;
        }
        wait(cancel, Duration::from_millis(25))?;
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while !modifiers_released() {
        if Instant::now() >= deadline {
            return Err("请松开快捷键及鼠标按键后重试".to_string());
        }
        wait(cancel, Duration::from_millis(25))?;
    }
    let mut targets = Vec::new();
    for pid in std::iter::once(primary_pid).chain(follower_pids.iter().copied()) {
        let hwnd = crate::infrastructure::system::find_game_hwnd(pid)
            .ok_or_else(|| format!("无法找到 D2R 窗口 (PID: {pid})"))?;
        validate_target(hwnd)?;
        let created = process_creation_time(pid).ok_or("无法确认游戏进程身份")?;
        targets.push((pid, hwnd, created));
    }
    if foreground_pid() != Some(primary_pid) {
        return Err("同步粘贴前请保持主号在前台".to_string());
    }
    let mut input = DesktopInput::acquire(targets[0].1, primary_pid)?;
    let strategy = BackgroundTextStrategy::from_value(&config.background_text_strategy);
    let flow = config.flow();
    let forms = PREPARED_FORMS.get_or_init(|| Mutex::new(HashMap::new()));
    let passwords = ENTERED_PASSWORDS.get_or_init(|| Mutex::new(HashMap::new()));
    let password_needed = {
        let cached = passwords.lock();
        targets.iter().any(|(pid, hwnd, created)| {
            !cached.get(pid).is_some_and(|entry| {
                entry.hwnd == *hwnd
                    && entry.process_created_at == *created
                    && entry.create == (*pid == primary_pid)
                    && entry.password == config.password
            })
        })
    };
    // A partial paste must not become a password-cache hit on the next attempt.
    if password_needed {
        let mut cached = passwords.lock();
        for (pid, _, _) in &targets {
            cached.remove(pid);
        }
    }
    let results = std::thread::scope(|scope| {
        let handles = targets
            .iter()
            .map(|&(pid, hwnd, created)| {
                scope.spawn(move || -> Result<(), String> {
                    let previous = forms.lock().remove(&pid);
                    if previous.is_some_and(|entry| entry.created == created && entry.hwnd == hwnd)
                    {
                        // A second create during manual waiting replaces the open
                        // join form, instead of navigating inside that old form.
                        deliver_key(
                            hwnd,
                            VK_ESCAPE,
                            false,
                            strategy,
                            flow.key_hold_ms,
                            550,
                            cancel,
                        )?;
                    }
                    open_room_form(hwnd, pid == primary_pid, strategy, flow, cancel)?;
                    forms.lock().insert(
                        pid,
                        PreparedForm {
                            created,
                            hwnd,
                            room_name: String::new(),
                        },
                    );
                    Ok(())
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("同步呼出房间表单线程异常退出".to_string()))
            })
            .collect::<Vec<_>>()
    });
    for result in results {
        result?;
    }
    let background = targets
        .iter()
        .skip(1)
        .map(|(_, hwnd, _)| *hwnd)
        .collect::<Vec<_>>();
    paste_group(&mut input, &background, room_name, strategy, flow, cancel)?;
    if password_needed {
        // Every participant uses the same password; refresh the group when any
        // participant lacks a valid cache, otherwise leave all passwords alone.
        input.key_down(VK_TAB)?;
        // Release the physical Tab before visiting followers: their cumulative
        // message waits must not turn the primary's single Tab into key repeat.
        wait(
            cancel,
            Duration::from_millis(flow.key_hold_ms.clamp(10, 250)),
        )?;
        input.release_all()?;
        for &hwnd in &background {
            deliver_key(hwnd, VK_TAB, false, strategy, flow.key_hold_ms, 20, cancel)?;
        }
        wait(cancel, Duration::from_millis(flow.step_delay_ms.max(120)))?;
        paste_group(
            &mut input,
            &background,
            &config.password,
            strategy,
            flow,
            cancel,
        )?;
    }
    input.check_target()?;
    for &(pid, hwnd, created) in &targets {
        if process_creation_time(pid) != Some(created)
            || crate::infrastructure::system::find_game_hwnd(pid) != Some(hwnd)
        {
            return Err("同步粘贴期间游戏进程或窗口已变化".to_string());
        }
    }
    // Consume the primary's prepared marker before the externally visible submit.
    forms.lock().remove(&primary_pid);
    deliver_key(
        targets[0].1,
        VK_RETURN,
        false,
        strategy,
        flow.key_hold_ms,
        20,
        cancel,
    )?;
    for &(pid, hwnd, created) in &targets {
        if password_needed {
            passwords.lock().insert(
                pid,
                EnteredPassword {
                    process_created_at: created,
                    hwnd,
                    password: config.password.clone(),
                    create: pid == primary_pid,
                },
            );
        }
        if pid != primary_pid {
            forms.lock().insert(
                pid,
                PreparedForm {
                    created,
                    hwnd,
                    room_name: room_name.to_string(),
                },
            );
        }
    }
    Ok(())
}

fn paste_group(
    input: &mut DesktopInput,
    background: &[isize],
    value: &str,
    strategy: BackgroundTextStrategy,
    flow: &FlowStrategy,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    if !value.is_empty() {
        input.clipboard_text(value)?;
    }
    group_chord(input, background, 0x41, true, strategy, flow, cancel)?;
    if value.is_empty() {
        group_chord(input, background, VK_BACK, false, strategy, flow, cancel)
    } else {
        input.check_clipboard()?;
        group_chord(input, background, 0x56, true, strategy, flow, cancel)
    }
}

fn group_chord(
    input: &mut DesktopInput,
    background: &[isize],
    key: u16,
    ctrl: bool,
    strategy: BackgroundTextStrategy,
    flow: &FlowStrategy,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    // Always release posted keys even on cancellation or partial delivery.
    let result = (|| {
        cancel.check()?;
        if ctrl {
            input.key_down(0x11)?;
            wait(cancel, Duration::from_millis(flow.step_delay_ms.max(120)))?;
            for &hwnd in background {
                deliver_key_message(hwnd, 0x11, true, strategy)?;
            }
        }
        input.key_down(key)?;
        wait(
            cancel,
            Duration::from_millis(flow.key_hold_ms.clamp(10, 250)),
        )?;
        // Keep physical Ctrl active for background consumers, but release A/V
        // before their waits so long delays cannot paste repeatedly up front.
        input.release_last()?;
        for &hwnd in background {
            validate_target(hwnd)?;
            deliver_key_message(hwnd, key, true, strategy)?;
        }
        // Keep physical Ctrl and the clipboard stable while background windows
        // consume the posted chord. This is a timing allowance, not an ACK.
        wait(cancel, Duration::from_millis(flow.step_delay_ms.max(200)))
    })();
    let mut cleanup = Ok(());
    for &hwnd in background {
        if let Err(error) = deliver_key_message(hwnd, key, false, strategy) {
            cleanup = Err(error);
        }
        if ctrl {
            if let Err(error) = deliver_key_message(hwnd, 0x11, false, strategy) {
                cleanup = Err(error);
            }
        }
    }
    let release = input.release_all();
    result?;
    cleanup?;
    release?;
    wait(cancel, Duration::from_millis(flow.step_delay_ms.max(200)))
}

pub(crate) fn submit_prepared_follower(
    config: &RoomAutomationConfig,
    pid: u32,
    room_name: &str,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    cancel.check()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while !modifiers_released() {
        if Instant::now() >= deadline {
            return Err("请松开跟随快捷键及鼠标按键后重试".to_string());
        }
        wait(cancel, Duration::from_millis(25))?;
    }
    let forms = PREPARED_FORMS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cached = forms.lock();
    let entry = cached
        .get(&pid)
        .ok_or("该小号没有预填表单，请重新触发创建快捷键")?;
    if entry.room_name != room_name
        || process_creation_time(pid) != Some(entry.created)
        || crate::infrastructure::system::find_game_hwnd(pid) != Some(entry.hwnd)
    {
        return Err("小号预填表单与当前房间或进程不匹配，请重新触发创建快捷键".to_string());
    }
    validate_target(entry.hwnd)?;
    let hwnd = entry.hwnd;
    // Delivery may be uncertain after an error. Never replay Enter against a
    // potentially submitted form or fall back to opening/refilling another one.
    cached.remove(&pid);
    drop(cached);
    deliver_key(
        hwnd,
        VK_RETURN,
        false,
        BackgroundTextStrategy::from_value(&config.background_text_strategy),
        config.flow().key_hold_ms,
        20,
        cancel,
    )
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit_handle: i32, pid: u32) -> *mut c_void;
    fn GetProcessTimes(
        process: *mut c_void,
        creation: *mut FILETIME,
        exit: *mut FILETIME,
        kernel: *mut FILETIME,
        user: *mut FILETIME,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

pub(super) fn process_creation_time(pid: u32) -> Option<u64> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let result = GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user);
        CloseHandle(process);
        (result != 0).then_some(
            (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
        )
    }
}

extern "system" {
    fn PostMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> i32;
    fn SendMessageTimeoutW(
        hWnd: isize,
        Msg: u32,
        wParam: usize,
        lParam: isize,
        fuFlags: u32,
        uTimeout: u32,
        lpdwResult: *mut usize,
    ) -> isize;
    fn MapVirtualKeyW(uCode: u32, uMapType: u32) -> u32;
    fn IsIconic(hWnd: isize) -> i32;
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
}

pub(crate) trait CancellationCheck: Send + Sync {
    fn check(&self) -> Result<(), String>;
    fn wait_cancelled(&self, duration: Duration) -> bool;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BackgroundTextStrategy {
    PostKeys,
    SendKeys,
}

impl BackgroundTextStrategy {
    fn from_value(value: &str) -> Self {
        if value == "send_keys" {
            Self::SendKeys
        } else {
            Self::PostKeys
        }
    }

    fn is_synchronous(self) -> bool {
        matches!(self, Self::SendKeys)
    }
}

pub(crate) fn foreground_pid() -> Option<u32> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd == 0 {
        return None;
    }
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    (pid != 0).then_some(pid)
}

fn open_room_form(
    hwnd: isize,
    create: bool,
    strategy: BackgroundTextStrategy,
    flow: &FlowStrategy,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let step = flow.step_delay_ms.clamp(60, 500);
    deliver_key(
        hwnd,
        VK_ESCAPE,
        false,
        strategy,
        flow.key_hold_ms,
        20,
        cancel,
    )?;
    // Let the double-Esc receiver expire before navigating the pause menu.
    wait(cancel, Duration::from_millis(step.max(550)))?;
    let direction = if create { VK_LEFT } else { VK_RIGHT };
    for _ in 0..GATEWAY_DIRECTION_REPETITIONS {
        deliver_key(
            hwnd,
            direction,
            false,
            strategy,
            flow.key_hold_ms,
            20,
            cancel,
        )?;
        wait(cancel, Duration::from_millis(step))?;
    }
    deliver_key(
        hwnd,
        VK_RETURN,
        false,
        strategy,
        flow.key_hold_ms,
        20,
        cancel,
    )?;
    wait(cancel, Duration::from_millis(step))?;
    wait(
        cancel,
        Duration::from_millis(ROOM_FORM_SETTLE_MS + EXTRA_PANEL_SETTLE_MS),
    )
}

fn deliver_key(
    hwnd: isize,
    key: u16,
    shift: bool,
    strategy: BackgroundTextStrategy,
    key_hold_ms: u64,
    release_gap_ms: u64,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    cancel.check()?;
    if shift {
        deliver_key_message(hwnd, VK_SHIFT, true, strategy)?;
        if let Err(error) = cancel.check() {
            let _ = deliver_key_message(hwnd, VK_SHIFT, false, strategy);
            return Err(error);
        }
    }
    if let Err(error) = deliver_key_message(hwnd, key, true, strategy) {
        if shift {
            let _ = deliver_key_message(hwnd, VK_SHIFT, false, strategy);
        }
        return Err(error);
    }

    // Cancellation may arrive during the key-down hold. Always emit matching
    // key-up messages before returning so the target window cannot retain a
    // logically pressed key (especially Shift) after the capability stops.
    if let Err(error) = wait(cancel, Duration::from_millis(key_hold_ms.clamp(10, 250))) {
        let key_release = deliver_key_message(hwnd, key, false, strategy);
        let shift_release = shift
            .then(|| deliver_key_message(hwnd, VK_SHIFT, false, strategy))
            .transpose();
        if let Err(release_error) = key_release.and(shift_release) {
            return Err(format!("{error}；同时释放按键失败：{release_error}"));
        }
        return Err(error);
    }

    let key_release = deliver_key_message(hwnd, key, false, strategy);
    let shift_release = shift
        .then(|| deliver_key_message(hwnd, VK_SHIFT, false, strategy))
        .transpose();
    key_release.and(shift_release)?;
    wait(cancel, Duration::from_millis(release_gap_ms))
}

fn deliver_key_message(
    hwnd: isize,
    key: u16,
    pressed: bool,
    strategy: BackgroundTextStrategy,
) -> Result<(), String> {
    let message = if pressed { WM_KEYDOWN } else { WM_KEYUP };
    let lparam = key_lparam(key, pressed);
    if strategy.is_synchronous() {
        let mut result = 0;
        let flags = SMTO_BLOCK | SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT;
        let sent = unsafe {
            SendMessageTimeoutW(
                hwnd,
                message,
                usize::from(key),
                lparam,
                flags,
                250,
                &mut result,
            )
        };
        if sent == 0 {
            return Err(format!(
                "SendMessageTimeout 发送失败或超时：消息 0x{message:X}"
            ));
        }
    } else if unsafe { PostMessageW(hwnd, message, usize::from(key), lparam) } == 0 {
        return Err(format!("PostMessage 发送失败：消息 0x{message:X}"));
    }
    Ok(())
}

fn key_lparam(key: u16, pressed: bool) -> isize {
    let scan_code = unsafe { MapVirtualKeyW(u32::from(key), MAPVK_VK_TO_VSC) } & 0xFF;
    let mut value = 1u32 | (scan_code << 16);
    if key == VK_END {
        value |= 1 << 24;
    }
    if !pressed {
        value |= (1 << 30) | (1 << 31);
    }
    value as isize
}

fn validate_target(hwnd: isize) -> Result<(), String> {
    if hwnd == 0 {
        return Err("目标 D2R 窗口不存在".to_string());
    }
    if unsafe { IsIconic(hwnd) } != 0 {
        return Err("目标 D2R 窗口已最小化，请先恢复窗口".to_string());
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), String> {
    if value.len() > 15 {
        return Err("输入内容超过 15 个字符".to_string());
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("后台房间表单输入只支持英文字母、数字、短横线和下划线".to_string());
    }
    Ok(())
}

fn wait(cancel: &dyn CancellationCheck, duration: Duration) -> Result<(), String> {
    if duration.is_zero() {
        return cancel.check();
    }
    cancel.check()?;
    if cancel.wait_cancelled(duration) {
        Err("自动跟房流程已取消".to_string())
    } else {
        Ok(())
    }
}
