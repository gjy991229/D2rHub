//! Windows keyboard delivery adapter for room automation.
//!
//! The application runtime owns cancellation and task lifecycles; this module
//! only translates one already-validated room form operation into native
//! window messages. Every wait and key boundary consults the caller's cancel
//! signal so capability shutdown never leaves detached input work behind.

use crate::capabilities::room_automation::FlowStrategy;
use std::time::Duration;

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
const VK_F13: u16 = 0x7C;
const VK_OEM_MINUS: u16 = 0xBD;
const MAPVK_VK_TO_VSC: u32 = 0;
const KEY_DOWN_HOLD_MS: u64 = 14;
const MIN_CHARACTER_GAP_MS: u64 = 10;
const PUNCTUATION_GAP_MS: u64 = 18;
const FIELD_CLEAR_SETTLE_MS: u64 = 24;
const CHAT_MODE_SETTLE_MS: u64 = 120;
const ROOM_FORM_SETTLE_MS: u64 = 200;
const FIELD_CLEAR_COUNT: usize = 16;
const GATEWAY_DIRECTION_REPETITIONS: usize = 2;

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

pub(crate) struct RoomFormRequest<'a> {
    pub pid: u32,
    pub background_text_strategy: &'a str,
    pub create: bool,
    pub open_form: bool,
    pub name: &'a str,
    pub password: &'a str,
    pub flow: &'a FlowStrategy,
}

pub(crate) fn fill_room_form(
    request: RoomFormRequest<'_>,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let hwnd = crate::infrastructure::system::find_game_hwnd(request.pid)
        .ok_or_else(|| format!("无法找到 D2R 窗口 (PID: {})", request.pid))?;
    validate_target(hwnd)?;
    let strategy = BackgroundTextStrategy::from_value(request.background_text_strategy);

    if request.open_form {
        open_room_form(
            hwnd,
            request.create,
            strategy,
            request.flow.step_delay_ms,
            cancel,
        )?;
    }
    enter_native_chat_mode(hwnd, strategy, request.flow.step_delay_ms, cancel)?;
    replace_text(
        hwnd,
        request.name,
        strategy,
        request.flow.character_delay_ms,
        cancel,
    )?;
    wait(cancel, Duration::from_millis(request.flow.step_delay_ms))?;
    deliver_key(hwnd, VK_TAB, false, strategy, 20, cancel)?;
    wait(cancel, Duration::from_millis(request.flow.step_delay_ms))?;
    replace_text(
        hwnd,
        request.password,
        strategy,
        request.flow.character_delay_ms,
        cancel,
    )?;
    wait(cancel, Duration::from_millis(request.flow.step_delay_ms))?;
    deliver_key(hwnd, VK_RETURN, false, strategy, 20, cancel)
}

pub(crate) fn confirm_retry(
    pid: u32,
    background_text_strategy: &str,
    step_delay_ms: u64,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let hwnd = crate::infrastructure::system::find_game_hwnd(pid)
        .ok_or_else(|| format!("无法找到 D2R 窗口 (PID: {pid})"))?;
    validate_target(hwnd)?;
    let strategy = BackgroundTextStrategy::from_value(background_text_strategy);
    deliver_key(hwnd, VK_RETURN, false, strategy, 20, cancel)?;
    wait(cancel, Duration::from_millis(step_delay_ms))
}

fn open_room_form(
    hwnd: isize,
    create: bool,
    strategy: BackgroundTextStrategy,
    step_delay_ms: u64,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let step = step_delay_ms.clamp(60, 500);
    deliver_key(hwnd, VK_ESCAPE, false, strategy, 20, cancel)?;
    wait(cancel, Duration::from_millis(step))?;
    let direction = if create { VK_LEFT } else { VK_RIGHT };
    for _ in 0..GATEWAY_DIRECTION_REPETITIONS {
        deliver_key(hwnd, direction, false, strategy, 20, cancel)?;
        wait(cancel, Duration::from_millis(step))?;
    }
    deliver_key(hwnd, VK_RETURN, false, strategy, 20, cancel)?;
    wait(cancel, Duration::from_millis(step))?;
    wait(cancel, Duration::from_millis(ROOM_FORM_SETTLE_MS))
}

fn enter_native_chat_mode(
    hwnd: isize,
    strategy: BackgroundTextStrategy,
    step_delay_ms: u64,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let step = step_delay_ms.clamp(60, 500).max(CHAT_MODE_SETTLE_MS);
    deliver_key(hwnd, VK_F13, false, strategy, 20, cancel)?;
    wait(cancel, Duration::from_millis(step))
}

fn replace_text(
    hwnd: isize,
    value: &str,
    strategy: BackgroundTextStrategy,
    character_delay_ms: u64,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    validate_text(value)?;
    let gap = character_delay_ms.clamp(MIN_CHARACTER_GAP_MS, 250);
    deliver_key(hwnd, VK_END, false, strategy, gap, cancel)?;
    for _ in 0..FIELD_CLEAR_COUNT {
        deliver_key(hwnd, VK_BACK, false, strategy, 0, cancel)?;
    }
    wait(cancel, Duration::from_millis(FIELD_CLEAR_SETTLE_MS))?;
    for character in value.chars() {
        let (key, shift) = character_key(character)?;
        let release_gap = if matches!(character, '-' | '_') {
            gap.max(PUNCTUATION_GAP_MS)
        } else {
            gap
        };
        deliver_key(hwnd, key, shift, strategy, release_gap, cancel)?;
    }
    Ok(())
}

fn deliver_key(
    hwnd: isize,
    key: u16,
    shift: bool,
    strategy: BackgroundTextStrategy,
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
    if let Err(error) = wait(cancel, Duration::from_millis(KEY_DOWN_HOLD_MS)) {
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
        return Err("后台原生聊天态输入只支持英文字母、数字、短横线和下划线".to_string());
    }
    Ok(())
}

fn character_key(character: char) -> Result<(u16, bool), String> {
    match character {
        'a'..='z' => Ok((character.to_ascii_uppercase() as u16, false)),
        'A'..='Z' => Ok((character as u16, true)),
        '0'..='9' => Ok((character as u16, false)),
        '-' => Ok((VK_OEM_MINUS, false)),
        '_' => Ok((VK_OEM_MINUS, true)),
        _ => Err(format!("后台原生聊天态输入暂不支持字符：{character}")),
    }
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
