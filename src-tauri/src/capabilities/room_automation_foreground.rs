//! Second room-form adapter: foreground mouse and scan-code clipboard paste.
use super::room_automation::ForegroundTiming;
use super::room_automation_layout::RoomLayout;
use super::room_automation_windows::{process_creation_time, CancellationCheck, RoomFormRequest};
use crate::infrastructure::physical_input::{modifiers_released, DesktopInput};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

struct FormMemory {
    created: u64,
    hwnd: isize,
    password: String,
    hell: bool,
    timing: ForegroundTiming,
}
static FORMS: OnceLock<Mutex<HashMap<(u32, bool), FormMemory>>> = OnceLock::new();
static DESKTOP: Mutex<()> = Mutex::new(());

pub(super) fn invalidate(pid: u32) {
    FORMS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .retain(|(id, _), _| *id != pid);
}

pub(super) fn focus_game(pid: u32, cancel: &dyn CancellationCheck) -> Result<(), String> {
    let _desktop = loop {
        cancel.check()?;
        if let Some(guard) = DESKTOP.try_lock() {
            break guard;
        }
        wait(cancel, 25)?;
    };
    let hwnd = crate::infrastructure::system::find_game_hwnd(pid)
        .ok_or("无法找到主号 D2R 窗口")?;
    let created = process_creation_time(pid).ok_or("无法确认主号进程身份")?;
    cancel.check()?;
    crate::infrastructure::system::bring_window_to_foreground_raw(hwnd);
    if process_creation_time(pid) != Some(created)
        || crate::infrastructure::system::find_game_hwnd(pid) != Some(hwnd)
    {
        return Err("主号游戏进程或窗口已变化".to_string());
    }
    if super::room_automation_windows::foreground_pid() != Some(pid) {
        return Err("未能将主号窗口切到前台".to_string());
    }
    Ok(())
}

pub(super) fn fill_room_form(
    request: RoomFormRequest<'_>,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let _desktop = loop {
        cancel.check()?;
        if let Some(guard) = DESKTOP.try_lock() {
            break guard;
        }
        wait(cancel, 25)?;
    };
    let layout = RoomLayout::load(request.pid, request.create)?;
    let hwnd = crate::infrastructure::system::find_game_hwnd(request.pid)
        .ok_or("无法找到目标 D2R 窗口")?;
    let created = process_creation_time(request.pid).ok_or("无法确认目标进程身份")?;
    let forms = FORMS.get_or_init(|| Mutex::new(HashMap::new()));
    let timing = request.foreground_timing;
    let key = (request.pid, request.create);
    let (password_needed, hell_needed) = {
        let mut forms = forms.lock();
        forms.retain(|(pid, _), memory| process_creation_time(*pid) == Some(memory.created));
        let previous = forms.get(&key).filter(|memory| {
            memory.created == created && memory.hwnd == hwnd && memory.timing == *timing
        });
        (
            !previous.is_some_and(|memory| memory.password == request.password),
            request.create && !previous.is_some_and(|memory| memory.hell),
        )
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while !modifiers_released() {
        if Instant::now() >= deadline {
            return Err("请松开快捷键及鼠标按键后重试".to_string());
        }
        wait(cancel, 25)?;
    }
    cancel.check()?;
    let mut input = DesktopInput::acquire(hwnd, request.pid)?;
    finish_step(cancel, timing.window_focus_ms, timing)?;
    let size = input.client_size()?;
    let points = layout.points(size.0, size.1)?;
    // Invalidate before the first click. Any partial edit must be replayed.
    forms.lock().remove(&key);
    fill_fields(&mut input, &request, points, size, password_needed, cancel)?;
    if hell_needed {
        click(
            &mut input,
            points[3],
            size,
            timing.focus_response_ms,
            timing,
            cancel,
        )?;
    }
    if input.client_size()? != size {
        return Err("窗口尺寸已变化，已停止提交".to_string());
    }
    // Submit directly from the current focus, including the difficulty button.
    // Release Enter after its hold and return without a response wait or gap.
    chord(&mut input, &[0x0D], timing.submit_hold_ms, cancel)?;
    input.check_target()?;
    if process_creation_time(request.pid) != Some(created) {
        return Err("目标游戏进程已变化".to_string());
    }
    forms.lock().insert(
        key,
        FormMemory {
            created,
            hwnd,
            password: request.password.to_string(),
            hell: request.create,
            timing: timing.clone(),
        },
    );
    Ok(())
}

/// Create and join share the same field navigation and input timing. Only the
/// layout's control positions and the optional create-only Hell step differ.
fn fill_fields(
    input: &mut DesktopInput,
    request: &RoomFormRequest<'_>,
    points: [(i32, i32); 4],
    size: (i32, i32),
    password_needed: bool,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let timing = request.foreground_timing;
    crate::logger::log_msg("INFO", "RoomAutomation", &format!(
        "前台房间表单（PID {}）：开表单响应 {}ms，焦点响应 {}ms，全选响应 {}ms，粘贴响应 {}ms，组合键按住 {}ms；所有账号统一通过 Tab 切密码",
        request.pid, timing.form_response_ms, timing.focus_response_ms,
        timing.select_response_ms, timing.paste_response_ms, timing.chord_hold_ms,
    ));
    click(
        input,
        points[0],
        size,
        timing.form_response_ms,
        timing,
        cancel,
    )?;
    click(
        input,
        points[1],
        size,
        timing.focus_response_ms,
        timing,
        cancel,
    )?;
    paste(input, request.name, timing, cancel)?;
    if password_needed {
        chord(input, &[0x09], timing.chord_hold_ms, cancel)?;
        finish_step(cancel, timing.focus_response_ms, timing)?;
        paste(input, request.password, timing, cancel)?;
    }
    crate::logger::log_msg(
        "INFO",
        "RoomAutomation",
        &format!(
            "前台房间表单（PID {}）：房名及所需密码输入已发送；不代表游戏已确认输入结果",
            request.pid,
        ),
    );
    Ok(())
}

fn click(
    input: &mut DesktopInput,
    point: (i32, i32),
    size: (i32, i32),
    response_ms: u64,
    timing: &ForegroundTiming,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    cancel.check()?;
    if input.client_size()? != size {
        return Err("窗口尺寸已变化，已停止点击".to_string());
    }
    input.move_to(point.0, point.1)?;
    wait(cancel, 10)?;
    input.mouse_down()?;
    let held = wait(cancel, timing.mouse_hold_ms);
    let released = input.release_all();
    held.and(released)?;
    // The Mod starts its timer from the click. Do not subtract elapsed movement
    // or holding time from the response window after mouse-up.
    finish_step(cancel, response_ms, timing)
}

fn paste(
    input: &mut DesktopInput,
    value: &str,
    timing: &ForegroundTiming,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    chord(input, &[0x11, 0x41], timing.chord_hold_ms, cancel)?;
    finish_step(cancel, timing.select_response_ms, timing)?;
    input.clipboard_text(value)?;
    input.check_clipboard()?;
    if value.is_empty() {
        chord(input, &[0x08], timing.chord_hold_ms, cancel)?;
    } else {
        chord(input, &[0x11, 0x56], timing.chord_hold_ms, cancel)?;
    }
    // Keep this clipboard content intact until the target has had its full
    // response window, before switching fields or restoring the clipboard.
    finish_step(cancel, timing.paste_response_ms, timing)
}

fn finish_step(
    cancel: &dyn CancellationCheck,
    response_ms: u64,
    timing: &ForegroundTiming,
) -> Result<(), String> {
    wait(cancel, response_ms)?;
    wait(cancel, timing.step_interval_ms)
}

fn chord(
    input: &mut DesktopInput,
    keys: &[u16],
    hold_ms: u64,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let pressed = (|| {
        for &key in keys {
            cancel.check()?;
            input.key_down(key)?;
        }
        wait(cancel, hold_ms)
    })();
    // In particular, a cancelled Ctrl+V must still release both V and Ctrl.
    let released = input.release_all();
    pressed.and(released)?;
    cancel.check()
}

fn wait(cancel: &dyn CancellationCheck, milliseconds: u64) -> Result<(), String> {
    wait_duration(cancel, Duration::from_millis(milliseconds))
}

fn wait_duration(cancel: &dyn CancellationCheck, duration: Duration) -> Result<(), String> {
    cancel.check()?;
    if duration.is_zero() {
        return Ok(());
    }
    if cancel.wait_cancelled(duration) {
        Err("自动跟房流程已取消".to_string())
    } else {
        cancel.check()
    }
}
