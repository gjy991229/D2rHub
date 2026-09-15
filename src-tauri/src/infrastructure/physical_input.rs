//! Scoped desktop input ownership. This adapter knows nothing about room forms.
mod clipboard;

use clipboard::ClipboardSession;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Prevent synthetic form input from being consumed as a Hub global shortcut.
pub(crate) const INPUT_TAG: usize = 0x44325248;

pub(crate) struct DesktopInput {
    hwnd: HWND,
    pid: u32,
    clipboard: ClipboardSession,
    releases: Vec<INPUT>,
    hotkeys: Option<crate::input_listener::hotkeys::Pause>,
}

pub(crate) fn modifiers_released() -> bool {
    // A shortcut may still be physically held when its worker starts.
    [0x10, 0x11, 0x12, 0x5B, 0x5C, 0x01, 0x02]
        .iter()
        .all(|key| unsafe { GetAsyncKeyState(*key) } >= 0)
}

impl DesktopInput {
    pub(crate) fn acquire(hwnd: isize, pid: u32) -> Result<Self, String> {
        let mut session = Self {
            hwnd: HWND(hwnd as *mut _),
            pid,
            clipboard: ClipboardSession::capture()?,
            releases: Vec::new(),
            hotkeys: None,
        };
        session.check_clipboard()?;
        session.hotkeys = Some(crate::input_listener::hotkeys::suspend()?);
        // Never freeze the desktop or steal focus back from the user.
        session.check_target()?;
        Ok(session)
    }

    pub(crate) fn check_target(&self) -> Result<(), String> {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(self.hwnd, Some(&mut pid));
        }
        if pid != self.pid || unsafe { IsIconic(self.hwnd) }.as_bool() {
            return Err("目标窗口已退出或最小化，已停止自动输入".to_string());
        }
        if unsafe { GetForegroundWindow() } != self.hwnd {
            return Err("目标窗口已失去前台，已停止自动输入".to_string());
        }
        Ok(())
    }

    pub(crate) fn key_down(&mut self, key: u16) -> Result<(), String> {
        self.check_target()?;
        let scan = unsafe { MapVirtualKeyW(u32::from(key), MAPVK_VK_TO_VSC) } as u16;
        if scan == 0 {
            return Err("无法解析物理按键扫描码".to_string());
        }
        let event = |up| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wScan: scan,
                    dwExtraInfo: INPUT_TAG,
                    dwFlags: KEYEVENTF_SCANCODE
                        | if up {
                            KEYEVENTF_KEYUP
                        } else {
                            KEYBD_EVENT_FLAGS(0)
                        },
                    ..Default::default()
                },
            },
        };
        self.press(event(false), event(true))
    }

    fn press(&mut self, down: INPUT, up: INPUT) -> Result<(), String> {
        // Register cleanup first: partial delivery must never leave a key held.
        self.releases.push(up);
        send(down)
    }

    /// Release the latest press while retaining earlier modifiers. Failed
    /// releases stay registered so scope cleanup can retry them.
    pub(crate) fn release_last(&mut self) -> Result<(), String> {
        if let Some(input) = self.releases.last().copied() {
            send(input)?;
            self.releases.pop();
        }
        Ok(())
    }

    pub(crate) fn release_all(&mut self) -> Result<(), String> {
        let mut failure = None;
        let mut retry = Vec::new();
        while let Some(input) = self.releases.pop() {
            if let Err(error) = send(input) {
                failure = Some(error);
                retry.push(input);
            }
        }
        self.releases = retry;
        failure.map_or(Ok(()), Err)
    }

    pub(crate) fn check_clipboard(&self) -> Result<(), String> {
        self.clipboard.check()
    }

    pub(crate) fn clipboard_text(&mut self, value: &str) -> Result<(), String> {
        self.check_target()?;
        self.clipboard.set_text(value)
    }
}

fn send(input: INPUT) -> Result<(), String> {
    if unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) } != 1 {
        Err("SendInput 物理输入发送失败".to_string())
    } else {
        Ok(())
    }
}

impl Drop for DesktopInput {
    fn drop(&mut self) {
        let _ = self.release_all();
    }
}
