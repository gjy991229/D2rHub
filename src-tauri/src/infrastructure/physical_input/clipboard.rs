//! Publishes room text directly without saving or restoring previous clipboard
//! contents. All handles stay on the owner thread.
use std::marker::PhantomData;
use std::rc::Rc;
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) struct ClipboardSession {
    owner: HWND,
    has_text: bool,
    _thread_bound: PhantomData<Rc<()>>,
}

struct OpenGuard;

impl OpenGuard {
    fn acquire(owner: HWND) -> Result<Self, String> {
        // Clipboard viewers can briefly hold it immediately after a paste.
        for attempt in 0..5 {
            match unsafe { OpenClipboard(owner) } {
                Ok(()) => return Ok(Self),
                Err(error) if attempt == 4 => return Err(format!("剪贴板正忙：{error}")),
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        unreachable!()
    }
}

impl Drop for OpenGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// Frees unpublished text on failure; successful publication transfers ownership
/// to Windows, which keeps the text after this session ends.
struct ClipboardText(HANDLE);

impl Drop for ClipboardText {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { GlobalFree(HGLOBAL(self.0 .0)) };
        }
    }
}

impl ClipboardSession {
    pub(super) fn new() -> Result<Self, String> {
        // A local, message-only window owns the text we publish.
        let owner = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("D2RHub Clipboard"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                None,
                None,
            )
        }
        .map_err(|e| format!("创建剪贴板窗口失败：{e}"))?;
        Ok(Self {
            owner,
            has_text: false,
            _thread_bound: PhantomData,
        })
    }

    pub(super) fn check(&self) -> Result<(), String> {
        // Only guard the room text we published, never the previous contents.
        // Windows may synthesize additional text formats without changing the
        // owner, so ownership avoids false positives from those conversions.
        if self.has_text && unsafe { GetClipboardOwner() }.ok() != Some(self.owner) {
            Err("剪贴板已被其他程序更改，已停止粘贴".into())
        } else {
            Ok(())
        }
    }

    pub(super) fn set_text(&mut self, value: &str) -> Result<(), String> {
        let text = value.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2).map_err(|e| e.to_string())?;
            let mut data = ClipboardText(HANDLE(memory.0));
            let pointer = GlobalLock(memory) as *mut u16;
            if pointer.is_null() {
                return Err("无法分配剪贴板文本".into());
            }
            std::ptr::copy_nonoverlapping(text.as_ptr(), pointer, text.len());
            let _ = GlobalUnlock(memory);
            let _open = OpenGuard::acquire(self.owner)?;
            self.check()?;
            EmptyClipboard().map_err(|e| e.to_string())?;
            self.has_text = false;
            SetClipboardData(13, data.0).map_err(|e| e.to_string())?;
            data.0 = HANDLE::default();
            self.has_text = true;
            Ok(())
        }
    }
}

impl Drop for ClipboardSession {
    fn drop(&mut self) {
        // Text is already materialized and owned by Windows. Leave it in the
        // clipboard; destroying this window does not require a render callback.
        let _ = unsafe { DestroyWindow(self.owner) };
    }
}
