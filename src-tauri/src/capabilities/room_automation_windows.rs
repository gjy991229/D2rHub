//! Windows keyboard delivery adapter for room automation.
//!
//! The application runtime owns cancellation and task lifecycles; this module
//! only translates one already-validated room form operation into native
//! window messages. Every wait and key boundary consults the caller's cancel
//! signal so capability shutdown never leaves detached input work behind.

use crate::capabilities::room_automation::{FlowStrategy, RoomAutomationConfig};
use crate::domain::room_automation::CollaborationMode;
use crate::infrastructure::physical_input::{modifiers_released, DesktopInput};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{
    GetLastError, SetLastError, ERROR_TIMEOUT, FILETIME, WIN32_ERROR,
};

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
const GATEWAY_DIRECTION_REPETITIONS: usize = 2;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

#[derive(Clone, Copy)]
struct KeyDelivery<'a> {
    task_id: Option<u64>,
    account_id: &'a str,
    primary: bool,
    pid: u32,
    hwnd: isize,
    created: u64,
    strategy: BackgroundTextStrategy,
    timeout_ms: u32,
}

impl<'a> KeyDelivery<'a> {
    fn new(
        config: &RoomAutomationConfig,
        account_id: &'a str,
        pid: u32,
        hwnd: isize,
        created: u64,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, String> {
        Ok(Self {
            task_id: cancel.task_id(),
            account_id,
            primary: !config.join_only
                && account_id.eq_ignore_ascii_case(&config.primary_account_id),
            pid,
            hwnd,
            created,
            strategy: BackgroundTextStrategy::from_value(&config.background_text_strategy),
            timeout_ms: u32::try_from(config.flow().sync_message_timeout_ms)
                .map_err(|_| "同步按键超时超出 Windows 支持的范围")?,
        })
    }

    fn send(&self, key: u16, pressed: bool, step: &'static str) -> Result<(), String> {
        deliver_key_message(self, key, pressed, step, false)
    }

    fn release(&self, key: u16, step: &'static str) -> Result<(), String> {
        deliver_key_message(self, key, false, step, true)
    }
}

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

static DESKTOP: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

static PREPARED_FORMS: OnceLock<Mutex<HashMap<u32, PreparedForm>>> = OnceLock::new();

struct PreparationAttempt {
    pids: Vec<u32>,
    committed: bool,
}

impl Drop for PreparationAttempt {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        // A failed operation provides no evidence that any form or password
        // is still present. In particular, the user may close it before retry.
        if let Some(forms) = PREPARED_FORMS.get() {
            let mut forms = forms.lock();
            for pid in &self.pids {
                forms.remove(pid);
            }
        }
        if let Some(passwords) = ENTERED_PASSWORDS.get() {
            let mut passwords = passwords.lock();
            for pid in &self.pids {
                passwords.remove(pid);
            }
        }
    }
}

/// Open every form concurrently, then share physical Ctrl with posted A/V.
/// Only the primary receives Enter here; followers keep their prepared forms.
pub(crate) fn prepare_background_room(
    config: &RoomAutomationConfig,
    primary_pid: u32,
    follower_pids: &[u32],
    room_name: &str,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let follower_pids: Vec<u32> = follower_pids
        .iter()
        .copied()
        .filter(|pid| !config.join_only || *pid != primary_pid)
        .collect();
    validate_text(room_name)?;
    validate_text(&config.password)?;
    let _desktop = loop {
        cancel.check()?;
        if let Some(guard) = DESKTOP.try_lock() {
            break guard;
        }
        wait(cancel, Duration::from_millis(25))?;
    };
    let mut attempt = PreparationAttempt {
        pids: std::iter::once(primary_pid)
            .chain(follower_pids.iter().copied())
            .collect(),
        committed: false,
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while !modifiers_released() {
        if Instant::now() >= deadline {
            return Err("请松开快捷键及鼠标按键后重试".to_string());
        }
        wait(cancel, Duration::from_millis(25))?;
    }
    let mut targets = Vec::new();
    for (index, pid) in std::iter::once(primary_pid)
        .chain(follower_pids.iter().copied())
        .enumerate()
    {
        let hwnd = crate::infrastructure::system::find_game_hwnd(pid)
            .ok_or_else(|| format!("无法找到 D2R 窗口 (PID: {pid})"))?;
        validate_target(hwnd)?;
        let created = process_creation_time(pid).ok_or("无法确认游戏进程身份")?;
        let account_id = if config.join_only {
            config
                .follower_account_ids
                .get(index)
                .ok_or("跟随账号与进程列表不一致")?
                .as_str()
        } else if index == 0 {
            config.primary_account_id.as_str()
        } else {
            config
                .follower_account_ids
                .get(index - 1)
                .ok_or("跟随账号与进程列表不一致")?
                .as_str()
        };
        targets.push(KeyDelivery::new(
            config, account_id, pid, hwnd, created, cancel,
        )?);
    }
    let use_window_messages =
        config.join_only || config.collaboration_mode == CollaborationMode::UberDiablo;
    if !use_window_messages && foreground_pid() != Some(primary_pid) {
        if !foreground_is_game() {
            return Err("前台已离开 D2R，已停止创建房间".into());
        }
        use crate::application::multi_instance::GameWindowPort;
        if !crate::infrastructure::system::SystemGameWindowPort.focus_by_pid(primary_pid) {
            return Err("无法切换到建房主号窗口".into());
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while foreground_pid() != Some(primary_pid) {
            if Instant::now() >= deadline {
                return Err("未能激活建房主号窗口，请重试".into());
            }
            wait(cancel, Duration::from_millis(25))?;
        }
    }
    let mut input = if use_window_messages {
        DesktopInput::acquire_for_messages(targets[0].hwnd, primary_pid)?
    } else {
        DesktopInput::acquire(targets[0].hwnd, primary_pid)?
    };
    let flow = config.flow();
    if !config.join_only && config.collaboration_mode == CollaborationMode::Host {
        let signal = crate::vision::processing::chat_signal(
            room_name
                .strip_prefix(&config.name_prefix)
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or("聊天信号房间序号无效")?,
        );
        for _ in 0..3 {
            cancel.check()?;
            input.key_down(VK_RETURN)?;
            wait(cancel, Duration::from_millis(flow.key_hold_ms))?;
            input.release_last()?;
            wait(cancel, Duration::from_millis(50))?;
            paste_group(&mut input, &[], &signal, flow, cancel, "chat.signal")?;
            input.key_down(VK_RETURN)?;
            wait(cancel, Duration::from_millis(flow.key_hold_ms))?;
            input.release_last()?;
            wait(cancel, Duration::from_millis(50))?;
        }
    }
    let forms = PREPARED_FORMS.get_or_init(|| Mutex::new(HashMap::new()));
    let passwords = ENTERED_PASSWORDS.get_or_init(|| Mutex::new(HashMap::new()));
    let password_needed = {
        let cached = passwords.lock();
        targets.iter().any(|target| {
            !cached.get(&target.pid).is_some_and(|entry| {
                entry.hwnd == target.hwnd
                    && entry.process_created_at == target.created
                    && entry.create == target.primary
                    && entry.password == config.password
            })
        })
    };
    // A partial paste must not become a password-cache hit on the next attempt.
    if password_needed {
        let mut cached = passwords.lock();
        for target in &targets {
            cached.remove(&target.pid);
        }
    }
    let results = std::thread::scope(|scope| {
        let handles = targets
            .iter()
            .map(|target| {
                scope.spawn(move || -> Result<(), String> {
                    let previous = forms.lock().remove(&target.pid);
                    if !target.primary
                        && previous.is_some_and(|entry| {
                            entry.created == target.created && entry.hwnd == target.hwnd
                        })
                    {
                        // A second create during manual waiting replaces the open
                        // join form, instead of navigating inside that old form.
                        deliver_key(
                            target,
                            VK_ESCAPE,
                            false,
                            flow.key_hold_ms,
                            flow.step_delay_ms,
                            cancel,
                            "form.replace",
                        )?;
                    }
                    open_room_form(target, flow, cancel)?;
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
        .skip(usize::from(input.uses_foreground_keys()))
        .copied()
        .collect::<Vec<_>>();
    paste_group(
        &mut input,
        &background,
        room_name,
        flow,
        cancel,
        "room_name.paste",
    )?;
    if password_needed {
        // Every participant uses the same password; refresh the group when any
        // participant lacks a valid cache, otherwise leave all passwords alone.
        if input.uses_foreground_keys() {
            input.key_down(VK_TAB)?;
            // Release before visiting followers, avoiding physical key repeat.
            wait(
                cancel,
                Duration::from_millis(flow.key_hold_ms.clamp(10, 250)),
            )?;
            input.release_all()?;
        }
        for target in &background {
            deliver_key(
                target,
                VK_TAB,
                false,
                flow.key_hold_ms,
                0,
                cancel,
                "password.tab",
            )?;
        }
        wait(cancel, Duration::from_millis(flow.step_delay_ms))?;
        paste_group(
            &mut input,
            &background,
            &config.password,
            flow,
            cancel,
            "password.paste",
        )?;
    }
    // Let the game observe Ctrl-up before the externally visible submit.
    // This gap is outside the Ctrl+A/V step and prevents Ctrl+Enter coupling.
    wait(cancel, Duration::from_millis(flow.step_delay_ms))?;
    input.check_target()?;
    for target in &targets {
        if process_creation_time(target.pid) != Some(target.created)
            || crate::infrastructure::system::find_game_hwnd(target.pid) != Some(target.hwnd)
        {
            return Err("同步粘贴期间游戏进程或窗口已变化".to_string());
        }
    }
    if !config.join_only {
        // Consume the primary's prepared marker before the externally visible submit.
        forms.lock().remove(&primary_pid);
        deliver_key(
            &targets[0],
            VK_RETURN,
            false,
            flow.key_hold_ms,
            flow.step_delay_ms,
            cancel,
            "submit.primary",
        )?;
    }
    for target in &targets {
        if password_needed {
            passwords.lock().insert(
                target.pid,
                EnteredPassword {
                    process_created_at: target.created,
                    hwnd: target.hwnd,
                    password: config.password.clone(),
                    create: target.primary,
                },
            );
        }
        if !target.primary {
            forms.lock().insert(
                target.pid,
                PreparedForm {
                    created: target.created,
                    hwnd: target.hwnd,
                    room_name: room_name.to_string(),
                },
            );
        }
    }
    attempt.committed = true;
    Ok(())
}

fn paste_group(
    input: &mut DesktopInput,
    background: &[KeyDelivery<'_>],
    value: &str,
    flow: &FlowStrategy,
    cancel: &dyn CancellationCheck,
    step: &'static str,
) -> Result<(), String> {
    if !value.is_empty() {
        input.clipboard_text(value)?;
    }
    if value.is_empty() {
        group_chord(input, background, 0x41, true, flow, cancel, step)?;
        group_chord(input, background, VK_BACK, false, flow, cancel, step)
    } else {
        paste_select_and_paste(input, background, flow, cancel, step)?;
        input.check_clipboard()?;
        Ok(())
    }
}

/// Send Ctrl+A and Ctrl+V as one timed step. Ctrl stays down for the whole
/// sequence; the three flow delays are Ctrl->A, A->V, and V->Ctrl-up.
fn paste_select_and_paste(
    input: &mut DesktopInput,
    background: &[KeyDelivery<'_>],
    flow: &FlowStrategy,
    cancel: &dyn CancellationCheck,
    step: &'static str,
) -> Result<(), String> {
    let mut background_ctrl = Vec::new();
    let mut background_a_down = Vec::new();
    let mut background_v_down = Vec::new();
    let result = (|| {
        cancel.check()?;
        input.check_target()?;
        if !modifiers_released() {
            return Err("检测到修饰键或鼠标按键按下，已停止自动输入".to_string());
        }
        input.key_down(0x11)?;
        for target in background {
            cancel.check()?;
            input.check_target()?;
            input.check_clipboard()?;
            background_ctrl.push(target);
            target.send(0x11, true, step)?;
        }

        wait(cancel, Duration::from_millis(flow.physical_ctrl_settle_ms))?;

        for target in background {
            cancel.check()?;
            input.check_target()?;
            background_a_down.push(target);
            target.send(0x41, true, step)?;
            target.send(0x41, false, step)?;
            background_a_down.pop();
        }
        if input.uses_foreground_keys() {
            input.key_down(0x41)?;
            input.release_last()?;
        }

        wait(cancel, Duration::from_millis(flow.chord_hold_ms))?;
        input.check_clipboard()?;

        for target in background {
            cancel.check()?;
            input.check_target()?;
            input.check_clipboard()?;
            background_v_down.push(target);
            target.send(0x56, true, step)?;
            target.send(0x56, false, step)?;
            background_v_down.pop();
        }
        if input.uses_foreground_keys() {
            input.key_down(0x56)?;
            input.release_last()?;
        }

        wait(cancel, Duration::from_millis(flow.character_delay_ms))?;
        Ok(())
    })();
    let mut cleanup = Ok(());
    for target in background_v_down.iter().rev() {
        cleanup = combine_cleanup(cleanup, target.release(0x56, step));
    }
    for target in background_a_down.iter().rev() {
        cleanup = combine_cleanup(cleanup, target.release(0x41, step));
    }
    for target in background_ctrl.iter().rev() {
        cleanup = combine_cleanup(cleanup, target.release(0x11, step));
    }
    cleanup = combine_cleanup(cleanup, input.release_all());
    combine_cleanup(result, cleanup)
}

fn group_chord(
    input: &mut DesktopInput,
    background: &[KeyDelivery<'_>],
    key: u16,
    ctrl: bool,
    flow: &FlowStrategy,
    cancel: &dyn CancellationCheck,
    step: &'static str,
) -> Result<(), String> {
    // Record presses before delivery so partial failures still release them.
    let mut background_ctrl = Vec::new();
    let mut background_keys = Vec::new();
    let result = (|| {
        cancel.check()?;
        input.check_target()?;
        if !modifiers_released() {
            return Err("检测到修饰键或鼠标按键按下，已停止自动输入".to_string());
        }
        if ctrl {
            input.key_down(0x11)?;
            for target in background {
                cancel.check()?;
                input.check_target()?;
                background_ctrl.push(target);
                target.send(0x11, true, step)?;
            }
            // One lead interval after all Ctrl-down events, before any A/V.
            wait(cancel, Duration::from_millis(flow.physical_ctrl_settle_ms))?;
        }
        for target in background {
            cancel.check()?;
            input.check_target()?;
            if key == 0x56 && ctrl {
                input.check_clipboard()?;
            }
            validate_target(target.hwnd)?;
            background_keys.push(target);
            target.send(key, true, step)?;
        }
        if key == 0x56 && ctrl {
            input.check_clipboard()?;
        }
        // Send the physical key last so background message delays cannot extend
        // the primary's key hold and cause repeated pastes.
        if input.uses_foreground_keys() {
            input.key_down(key)?;
        }
        wait(cancel, Duration::from_millis(flow.key_hold_ms))?;
        if input.uses_foreground_keys() {
            input.release_last()?;
        }
        while let Some(target) = background_keys.last() {
            target.send(key, false, step)?;
            background_keys.pop();
        }
        // The tail interval starts only after every A/V-up was delivered.
        // Physical and posted Ctrl remain down throughout this interval.
        if ctrl {
            wait(cancel, Duration::from_millis(flow.chord_hold_ms))?;
        }
        input.check_target()
    })();
    // Cancellation/errors skip the remaining waits, but always release keys.
    let mut cleanup = Ok(());
    for target in &background_keys {
        cleanup = combine_cleanup(cleanup, target.release(key, step));
    }
    for target in &background_ctrl {
        cleanup = combine_cleanup(cleanup, target.release(0x11, step));
    }
    cleanup = combine_cleanup(cleanup, input.release_all());
    combine_cleanup(result, cleanup)?;
    wait(cancel, Duration::from_millis(flow.character_delay_ms))
}

pub(crate) fn validate_prepared_follower(pid: u32, room_name: &str) -> Result<(), String> {
    let forms = PREPARED_FORMS.get_or_init(|| Mutex::new(HashMap::new()));
    let cached = forms.lock();
    let entry = cached
        .get(&pid)
        .ok_or("该小号没有预填表单，请重新触发创建快捷键")?;
    validate_prepared_entry(entry, pid, room_name)
}

fn validate_prepared_entry(entry: &PreparedForm, pid: u32, room_name: &str) -> Result<(), String> {
    if entry.room_name != room_name
        || process_creation_time(pid) != Some(entry.created)
        || crate::infrastructure::system::find_game_hwnd(pid) != Some(entry.hwnd)
    {
        return Err("小号预填表单与当前房间或进程不匹配，请重新触发创建快捷键".to_string());
    }
    // Minimization is a temporary input failure, not loss of preparation.
    Ok(())
}

pub(crate) fn submit_prepared_follower(
    config: &RoomAutomationConfig,
    account_id: &str,
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
    validate_prepared_entry(entry, pid, room_name)?;
    validate_target(entry.hwnd)?;
    let target = KeyDelivery::new(config, account_id, pid, entry.hwnd, entry.created, cancel)?;
    // Consume the prepared form before its single submission.
    cached.remove(&pid);
    drop(cached);
    deliver_key(
        &target,
        VK_RETURN,
        false,
        config.flow().key_hold_ms,
        0,
        cancel,
        "submit.follower",
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
    fn task_id(&self) -> Option<u64> {
        None
    }

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

pub(crate) fn foreground_is_game() -> bool {
    let Some(pid) = foreground_pid() else {
        return false;
    };
    crate::infrastructure::game_layout_windows::snapshot()
        .is_ok_and(|windows| windows.iter().any(|window| window.pid == pid))
}

fn open_room_form(
    target: &KeyDelivery<'_>,
    flow: &FlowStrategy,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let step = flow.step_delay_ms;
    deliver_key(
        target,
        VK_ESCAPE,
        false,
        flow.key_hold_ms,
        step,
        cancel,
        "form.escape",
    )?;
    // This path sends only one Esc, followed by direction keys. Its next
    // operation uses the configured interval, not the double-Esc timeout.
    let direction = if target.primary { VK_LEFT } else { VK_RIGHT };
    for index in 0..GATEWAY_DIRECTION_REPETITIONS {
        deliver_key(
            target,
            direction,
            false,
            flow.key_hold_ms,
            step,
            cancel,
            if index == 0 {
                "form.direction.1"
            } else {
                "form.direction.2"
            },
        )?;
    }
    deliver_key(
        target,
        VK_RETURN,
        false,
        flow.key_hold_ms,
        room_form_settle_ms(target.primary, flow.form_settle_ms),
        cancel,
        "form.open",
    )
}

fn room_form_settle_ms(create: bool, configured_ms: u64) -> u64 {
    // The Mod opens the form at 100 ms and selects Hell 50 ms later.
    // Reserve the existing default 300 ms even for a zero-delay create profile;
    // this is a scheduling margin, not an acknowledgement from the game.
    if create {
        configured_ms.max(300)
    } else {
        configured_ms
    }
}

fn deliver_key(
    target: &KeyDelivery<'_>,
    key: u16,
    shift: bool,
    key_hold_ms: u64,
    release_gap_ms: u64,
    cancel: &dyn CancellationCheck,
    step: &'static str,
) -> Result<(), String> {
    cancel.check()?;
    deliver_key_sequence(
        key,
        shift,
        key_hold_ms,
        release_gap_ms,
        |key, pressed, cleanup| deliver_key_message(target, key, pressed, step, cleanup),
        |duration| wait(cancel, duration),
    )
}

/// A failed down can already have reached the window. Track it before sending
/// and attempt only matching releases on failure; never replay a key-down.
fn deliver_key_sequence(
    key: u16,
    shift: bool,
    key_hold_ms: u64,
    release_gap_ms: u64,
    mut send: impl FnMut(u16, bool, bool) -> Result<(), String>,
    mut pause: impl FnMut(Duration) -> Result<(), String>,
) -> Result<(), String> {
    let mut held = Vec::with_capacity(2);
    let result = (|| {
        if shift {
            held.push(VK_SHIFT);
            send(VK_SHIFT, true, false)?;
            pause(Duration::ZERO)?;
        }
        held.push(key);
        send(key, true, false)?;
        pause(Duration::from_millis(key_hold_ms.clamp(10, 250)))?;

        let key_release = send(key, false, false);
        if key_release.is_ok() {
            held.retain(|held_key| *held_key != key);
        }
        // Still release Shift if releasing the main key failed.
        let shift_release = if shift {
            let release = send(VK_SHIFT, false, false);
            if release.is_ok() {
                held.retain(|held_key| *held_key != VK_SHIFT);
            }
            release
        } else {
            Ok(())
        };
        combine_cleanup(key_release, shift_release)?;
        pause(Duration::from_millis(release_gap_ms))
    })();

    let mut cleanup = Ok(());
    for key in held.into_iter().rev() {
        cleanup = combine_cleanup(cleanup, send(key, false, true));
    }
    combine_cleanup(result, cleanup)
}

fn combine_cleanup(result: Result<(), String>, cleanup: Result<(), String>) -> Result<(), String> {
    match (result, cleanup) {
        (Err(error), Err(cleanup_error)) => Err(format!("{error}；释放按键失败：{cleanup_error}")),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), cleanup) => cleanup,
    }
}

fn delivery_status(sent: bool, win32_error: u32, strategy: BackgroundTextStrategy) -> &'static str {
    if sent {
        if strategy.is_synchronous() {
            "processed"
        } else {
            "queued"
        }
    } else if strategy.is_synchronous() && win32_error == ERROR_TIMEOUT.0 {
        "timeout"
    } else if win32_error == 0 {
        // ERROR_SUCCESS after a failed call is an unspecified generic failure,
        // not evidence that SMTO_ABORTIFHUNG was responsible.
        "generic_failure"
    } else {
        "win32_error"
    }
}

fn deliver_key_message(
    target: &KeyDelivery<'_>,
    key: u16,
    pressed: bool,
    step: &'static str,
    cleanup: bool,
) -> Result<(), String> {
    let message = if pressed { WM_KEYDOWN } else { WM_KEYUP };
    let lparam = key_lparam(key, pressed);
    let started = Instant::now();
    let target_changed = cleanup
        && (process_creation_time(target.pid) != Some(target.created)
            || crate::infrastructure::system::find_game_hwnd(target.pid) != Some(target.hwnd));
    let (sent, win32_error) = if target_changed {
        (false, 0)
    } else if target.strategy.is_synchronous() {
        let mut result = 0;
        let flags = SMTO_BLOCK | SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT;
        unsafe {
            SetLastError(WIN32_ERROR(0));
            let sent = SendMessageTimeoutW(
                target.hwnd,
                message,
                usize::from(key),
                lparam,
                flags,
                target.timeout_ms,
                &mut result,
            );
            let error = if sent == 0 { GetLastError().0 } else { 0 };
            (sent != 0, error)
        }
    } else {
        unsafe {
            SetLastError(WIN32_ERROR(0));
            let sent = PostMessageW(target.hwnd, message, usize::from(key), lparam);
            let error = if sent == 0 { GetLastError().0 } else { 0 };
            (sent != 0, error)
        }
    };
    let status = if target_changed {
        "target_changed"
    } else {
        delivery_status(sent, win32_error, target.strategy)
    };
    let elapsed_ms = started.elapsed().as_millis();
    // Keep room names, passwords, and clipboard contents out of diagnostics.
    let details = format!(
        "task={:?} account={:?} role={} pid={} hwnd=0x{:X} step={step} vk=0x{key:X} state={} message=0x{message:X} strategy={} timeout_ms={} elapsed_ms={elapsed_ms} result={status} win32_error={win32_error} cleanup={cleanup}",
        target.task_id,
        target.account_id,
        if target.primary { "primary" } else { "follower" },
        target.pid,
        target.hwnd,
        if pressed { "down" } else { "up" },
        if target.strategy.is_synchronous() { "send_keys" } else { "post_keys" },
        if target.strategy.is_synchronous() { target.timeout_ms } else { 0 },
    );
    crate::logger::log_msg(
        if sent { "INFO" } else { "WARN" },
        "RoomAutomation",
        &details,
    );
    if sent {
        Ok(())
    } else {
        let key_name = match key {
            VK_RETURN => "Enter".to_string(),
            VK_ESCAPE => "Esc".to_string(),
            VK_LEFT => "左方向键".to_string(),
            VK_RIGHT => "右方向键".to_string(),
            VK_TAB => "Tab".to_string(),
            VK_SHIFT => "Shift".to_string(),
            0x11 => "Ctrl".to_string(),
            0x41 => "A".to_string(),
            0x56 => "V".to_string(),
            VK_BACK => "Backspace".to_string(),
            _ => format!("0x{key:X}"),
        };
        if target_changed {
            return Err(format!(
                "账号“{}”的进程或窗口已变化，未发送 {key_name} 释放消息",
                target.account_id
            ));
        }
        let step_name = match step {
            "form.replace" => "重开表单",
            "form.escape" => "打开暂停菜单",
            "form.direction.1" | "form.direction.2" => "选择房间操作",
            "form.open" => "打开房间表单",
            "room_name.paste" => "填写房名",
            "password.tab" => "切换密码输入框",
            "password.paste" => "填写密码",
            "submit.primary" => "提交建房",
            "submit.follower" => "提交入房",
            _ => step,
        };
        let reason = match status {
            "timeout" => "等待超时",
            "generic_failure" => "投递失败，原因未明确",
            _ => "投递失败",
        };
        Err(format!(
            "账号“{}”在{step_name}时，{key_name} {}消息{reason}（错误码 {win32_error}，耗时 {elapsed_ms} 毫秒）",
            target.account_id,
            if pressed { "按下" } else { "松开" },
        ))
    }
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

pub(crate) fn validate_room_window(pid: u32) -> Result<(), String> {
    let hwnd = crate::infrastructure::system::find_game_hwnd(pid)
        .ok_or_else(|| format!("无法找到 D2R 窗口 (PID: {pid})"))?;
    validate_target(hwnd)
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
    if value.encode_utf16().count() > 15 {
        return Err("输入内容超过 15 个字符".to_string());
    }
    if value
        .chars()
        .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
    {
        return Err("房间表单输入不支持换行或控制字符".to_string());
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

/// Reuse bounded window-message delivery; never activate or steal focus.
pub(crate) fn hostile_escape_burst(pid: u32, active: impl Fn() -> bool) -> Result<(), String> {
    let created = process_creation_time(pid).ok_or("目标进程已退出")?;
    let hwnd = crate::infrastructure::system::find_game_hwnd(pid).ok_or("找不到目标游戏窗口")?;
    let delivery = KeyDelivery {
        task_id: None,
        account_id: "hostile",
        primary: false,
        pid,
        hwnd,
        created,
        strategy: BackgroundTextStrategy::from_value("post_message"),
        timeout_ms: 100,
    };
    for index in 0..4 {
        if !active()
            || process_creation_time(pid) != Some(created)
            || crate::infrastructure::system::find_game_hwnd(pid) != Some(hwnd)
        {
            return Err("敌对重开已取消或目标窗口已变化".into());
        }
        let down = delivery.send(VK_ESCAPE, true, "hostile-escape-down");
        let up = delivery.release(VK_ESCAPE, "hostile-escape-up");
        combine_cleanup(down, up)?;
        if index < 3 {
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    Ok(())
}

#[cfg(test)]
mod timing_tests {
    use super::*;

    #[test]
    fn failed_down_releases_enter_and_navigation_without_replaying_them() {
        for key in [VK_RETURN, VK_ESCAPE, VK_LEFT, VK_RIGHT] {
            let mut events = Vec::new();
            let result = deliver_key_sequence(
                key,
                false,
                50,
                80,
                |key, pressed, cleanup| {
                    events.push((key, pressed, cleanup));
                    if pressed {
                        Err("injected down timeout".to_string())
                    } else {
                        Ok(())
                    }
                },
                |_| panic!("a failed down must skip remaining waits"),
            );
            assert_eq!(result.unwrap_err(), "injected down timeout");
            assert_eq!(events, [(key, true, false), (key, false, true)]);
        }
    }

    #[test]
    fn failed_up_gets_one_cleanup_attempt_and_keeps_original_error() {
        let mut events = Vec::new();
        let result = deliver_key_sequence(
            VK_RETURN,
            false,
            50,
            80,
            |key, pressed, cleanup| {
                events.push((key, pressed, cleanup));
                if !pressed && !cleanup {
                    Err("injected up timeout".to_string())
                } else {
                    Ok(())
                }
            },
            |_| Ok(()),
        );
        assert_eq!(result.unwrap_err(), "injected up timeout");
        assert_eq!(
            events,
            [
                (VK_RETURN, true, false),
                (VK_RETURN, false, false),
                (VK_RETURN, false, true),
            ]
        );
    }

    #[test]
    fn cancellation_during_hold_releases_key_then_shift() {
        let mut events = Vec::new();
        let result = deliver_key_sequence(
            VK_RETURN,
            true,
            50,
            80,
            |key, pressed, cleanup| {
                events.push((key, pressed, cleanup));
                Ok(())
            },
            |duration| {
                if duration.is_zero() {
                    Ok(())
                } else {
                    Err("injected cancellation".to_string())
                }
            },
        );
        assert_eq!(result.unwrap_err(), "injected cancellation");
        assert_eq!(
            events,
            [
                (VK_SHIFT, true, false),
                (VK_RETURN, true, false),
                (VK_RETURN, false, true),
                (VK_SHIFT, false, true),
            ]
        );
    }

    #[test]
    fn failed_shift_down_still_attempts_shift_release() {
        let mut events = Vec::new();
        let result = deliver_key_sequence(
            VK_RETURN,
            true,
            50,
            80,
            |key, pressed, cleanup| {
                events.push((key, pressed, cleanup));
                if pressed {
                    Err("injected shift failure".to_string())
                } else {
                    Ok(())
                }
            },
            |_| panic!("a failed shift down must skip remaining waits"),
        );
        assert_eq!(result.unwrap_err(), "injected shift failure");
        assert_eq!(events, [(VK_SHIFT, true, false), (VK_SHIFT, false, true)]);
    }

    #[test]
    fn cleanup_failure_keeps_both_errors_without_another_down() {
        let mut events = Vec::new();
        let result = deliver_key_sequence(
            VK_RETURN,
            false,
            50,
            80,
            |key, pressed, cleanup| {
                events.push((key, pressed, cleanup));
                Err(if pressed {
                    "original failure"
                } else {
                    "cleanup failure"
                }
                .to_string())
            },
            |_| panic!("a failed down must skip remaining waits"),
        );
        let error = result.unwrap_err();
        assert!(error.contains("original failure"));
        assert!(error.contains("cleanup failure"));
        assert_eq!(events, [(VK_RETURN, true, false), (VK_RETURN, false, true)]);
    }

    #[test]
    fn successful_press_preserves_pacing_and_has_no_cleanup_replay() {
        let mut events = Vec::new();
        let mut waits = Vec::new();
        deliver_key_sequence(
            VK_RETURN,
            false,
            50,
            80,
            |key, pressed, cleanup| {
                events.push((key, pressed, cleanup));
                Ok(())
            },
            |duration| {
                waits.push(duration);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            events,
            [(VK_RETURN, true, false), (VK_RETURN, false, false)]
        );
        assert_eq!(
            waits,
            [Duration::from_millis(50), Duration::from_millis(80)]
        );
    }

    #[test]
    fn zero_last_error_is_generic_failure_and_posting_only_confirms_queuing() {
        assert_eq!(
            delivery_status(false, 0, BackgroundTextStrategy::SendKeys),
            "generic_failure"
        );
        assert_eq!(
            delivery_status(false, ERROR_TIMEOUT.0, BackgroundTextStrategy::SendKeys),
            "timeout"
        );
        assert_eq!(
            delivery_status(false, 1400, BackgroundTextStrategy::SendKeys),
            "win32_error"
        );
        assert_eq!(
            delivery_status(true, 0, BackgroundTextStrategy::PostKeys),
            "queued"
        );
        assert_eq!(
            delivery_status(true, 0, BackgroundTextStrategy::SendKeys),
            "processed"
        );
    }

    #[test]
    fn native_invalid_window_failure_reports_actual_error_and_account_context() {
        struct Active;
        impl CancellationCheck for Active {
            fn check(&self) -> Result<(), String> {
                Ok(())
            }
            fn wait_cancelled(&self, _: Duration) -> bool {
                false
            }
        }
        let invalid_hwnd = -12_345;
        let mut pid = 0;
        // Verify that this handle is invalid before attempting a message.
        assert_eq!(
            unsafe { GetWindowThreadProcessId(invalid_hwnd, &mut pid) },
            0
        );
        let config = RoomAutomationConfig::default();
        let target =
            KeyDelivery::new(&config, "test-follower", 0, invalid_hwnd, 0, &Active).unwrap();
        unsafe {
            SetLastError(WIN32_ERROR(1234));
        }
        let error = target.send(VK_RETURN, true, "submit.follower").unwrap_err();
        assert!(error.contains("test-follower"));
        assert!(error.contains("提交入房"));
        assert!(error.contains("Enter 按下"));
        assert!(error.contains("错误码 1400"));
        assert!(!error.contains("错误码 1234"));
    }

    #[test]
    fn creating_reserves_initialization_time_without_shortening_user_delays() {
        assert_eq!(room_form_settle_ms(true, 0), 300);
        assert_eq!(room_form_settle_ms(true, 150), 300);
        assert_eq!(room_form_settle_ms(true, 300), 300);
        assert_eq!(room_form_settle_ms(true, 2000), 2000);
        assert_eq!(room_form_settle_ms(false, 0), 0);
        assert_eq!(room_form_settle_ms(false, 600), 600);
    }
}
