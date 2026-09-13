//! Scoped desktop input ownership. This adapter knows nothing about room forms.
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND, POINT};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::{
    OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard, OleUninitialize,
};
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Prevent synthetic form input from being consumed as a Hub global shortcut.
pub(crate) const INPUT_TAG: usize = 0x44325248;

pub(crate) struct DesktopInput {
    hwnd: HWND,
    pid: u32,
    cursor: POINT,
    dpi: DPI_AWARENESS_CONTEXT,
    clipboard: Option<IDataObject>,
    clipboard_sequence: Option<u32>,
    clipboard_dirty: bool,
    releases: Vec<INPUT>,
    blocked: bool,
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
        unsafe { OleInitialize(None) }.map_err(|e| e.to_string())?;
        let mut session = Self {
            hwnd: HWND(hwnd as *mut _),
            pid,
            cursor: POINT::default(),
            dpi: unsafe {
                SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            },
            clipboard: None,
            clipboard_sequence: None,
            clipboard_dirty: false,
            releases: Vec::new(),
            blocked: false,
            hotkeys: None,
        };
        unsafe { GetCursorPos(&mut session.cursor) }.map_err(|e| e.to_string())?;
        // Capture all clipboard formats, including images and files, before writing.
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if unsafe { CountClipboardFormats() } != 0 {
            session.clipboard = Some(unsafe { OleGetClipboard() }.map_err(|e| e.to_string())?);
        }
        session.clipboard_sequence = Some(sequence);
        session.check_clipboard()?;
        session.hotkeys = Some(crate::input_listener::hotkeys::suspend()?);
        unsafe { BlockInput(true) }.map_err(|e| format!("无法接管键鼠：{e}"))?;
        session.blocked = true;
        super::system::bring_window_to_foreground_raw(hwnd);
        session.check_target()?;
        Ok(session)
    }

    pub(crate) fn check_target(&self) -> Result<(), String> {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(self.hwnd, Some(&mut pid));
        }
        if pid != self.pid || unsafe { IsIconic(self.hwnd) }.as_bool() {
            return Err("目标窗口已退出或最小化，已停止键鼠接管".to_string());
        }
        if unsafe { GetForegroundWindow() } != self.hwnd {
            return Err("目标窗口已失去前台，已停止键鼠接管".to_string());
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
        if self
            .clipboard_sequence
            .is_some_and(|sequence| sequence != unsafe { GetClipboardSequenceNumber() })
        {
            Err("剪贴板已被其他程序更改，已停止粘贴".to_string())
        } else {
            Ok(())
        }
    }

    pub(crate) fn clipboard_text(&mut self, value: &str) -> Result<(), String> {
        self.check_target()?;
        self.check_clipboard()?;
        let text = value.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2).map_err(|e| e.to_string())?;
            let pointer = GlobalLock(memory) as *mut u16;
            if pointer.is_null() {
                let _ = GlobalFree(memory);
                return Err("无法分配剪贴板文本".to_string());
            }
            std::ptr::copy_nonoverlapping(text.as_ptr(), pointer, text.len());
            let _ = GlobalUnlock(memory);
            if let Err(error) = OpenClipboard(self.hwnd) {
                let _ = GlobalFree(memory);
                return Err(format!("剪贴板正忙：{error}"));
            }
            if let Err(error) = self.check_clipboard() {
                let _ = CloseClipboard();
                let _ = GlobalFree(memory);
                return Err(error);
            }
            let result = EmptyClipboard().and_then(|()| {
                self.clipboard_dirty = true;
                SetClipboardData(13, HANDLE(memory.0)).map(|_| ())
            });
            let _ = CloseClipboard();
            self.clipboard_sequence = Some(GetClipboardSequenceNumber());
            if result.is_err() {
                let _ = GlobalFree(memory);
            }
            result.map_err(|e| e.to_string())
        }
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
        unsafe {
            if self.blocked {
                let _ = SetCursorPos(self.cursor.x, self.cursor.y);
                let _ = BlockInput(false);
            }
            if self.clipboard_dirty
                && self
                    .clipboard_sequence
                    .is_some_and(|sequence| sequence == GetClipboardSequenceNumber())
                && OleSetClipboard(self.clipboard.as_ref())
                    .and_then(|()| OleFlushClipboard())
                    .is_err()
            {
                crate::logger::log_msg("WARN", "PhysicalInput", "恢复剪贴板失败");
            }
            self.clipboard.take();
            SetThreadDpiAwarenessContext(self.dpi);
            OleUninitialize();
        }
    }
}
