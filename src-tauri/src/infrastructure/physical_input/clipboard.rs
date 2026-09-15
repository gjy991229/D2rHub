//! Materialized clipboard data, never a live IDataObject pointing back at the
//! clipboard that we are about to replace. All handles stay on the owner thread.
use std::marker::PhantomData;
use std::rc::Rc;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HANDLE, HGLOBAL, HWND,
};
use windows::Win32::Graphics::Gdi::{
    CopyEnhMetaFileW, DeleteEnhMetaFile, DeleteMetaFile, DeleteObject, HENHMETAFILE, HGDIOBJ,
};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::{OleDuplicateData, CLIPBOARD_FORMAT};
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) struct ClipboardSession {
    owner: HWND,
    saved: Vec<ClipboardData>,
    sequence: u32,
    dirty: bool,
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

struct ClipboardData {
    format: u32,
    handle: HANDLE,
}

impl ClipboardData {
    // Caller holds OpenGuard. Never retain a handle owned by the old clipboard.
    unsafe fn copy(format: u32) -> Result<Self, String> {
        let source =
            GetClipboardData(format).map_err(|e| format!("读取剪贴板格式 {format} 失败：{e}"))?;
        let handle = match format {
            14 | 142 => HANDLE(CopyEnhMetaFileW(HENHMETAFILE(source.0), PCWSTR::null()).0),
            _ => {
                let kind = match format {
                    130 => 2, // CF_DSPBITMAP uses the same handle type as CF_BITMAP.
                    131 => 3, // CF_DSPMETAFILEPICT.
                    _ => format,
                };
                // This helper only duplicates native handles; it does not fetch,
                // publish or retain an OLE clipboard object or initialize COM.
                OleDuplicateData(source, CLIPBOARD_FORMAT(kind as u16), GMEM_MOVEABLE)
            }
        };
        if handle.is_invalid() {
            return Err(format!(
                "无法备份剪贴板格式 {format}，已保留原剪贴板并停止自动输入"
            ));
        }
        Ok(Self { format, handle })
    }

    unsafe fn publish(&mut self) -> Result<(), String> {
        SetClipboardData(self.format, self.handle).map_err(|e| e.to_string())?;
        // The system owns the handle only after successful publication.
        self.handle = HANDLE::default();
        Ok(())
    }
}

impl Drop for ClipboardData {
    fn drop(&mut self) {
        if self.handle.is_invalid() {
            return;
        }
        unsafe {
            match self.format {
                2 | 9 | 130 => {
                    let _ = DeleteObject(HGDIOBJ(self.handle.0));
                }
                14 | 142 => {
                    let _ = DeleteEnhMetaFile(HENHMETAFILE(self.handle.0));
                }
                3 | 131 => {
                    let memory = HGLOBAL(self.handle.0);
                    let picture = GlobalLock(memory) as *const METAFILEPICT;
                    if !picture.is_null() {
                        let _ = DeleteMetaFile((*picture).hMF);
                        let _ = GlobalUnlock(memory);
                    }
                    let _ = GlobalFree(memory);
                }
                _ => {
                    let _ = GlobalFree(HGLOBAL(self.handle.0));
                }
            }
        }
    }
}

impl ClipboardSession {
    pub(super) fn capture() -> Result<Self, String> {
        // A built-in, message-only window gives the clipboard a local owner.
        // Using the game's HWND made another process own our temporary data.
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
        let mut session = Self {
            owner,
            saved: Vec::new(),
            sequence: 0,
            dirty: false,
            _thread_bound: PhantomData,
        };
        let _open = OpenGuard::acquire(owner)?;
        let mut format = 0;
        loop {
            unsafe {
                SetLastError(ERROR_SUCCESS);
            }
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                let error = unsafe { GetLastError() };
                if error != ERROR_SUCCESS {
                    return Err(format!("枚举剪贴板格式失败：{error:?}"));
                }
                break;
            }
            if format >= 0xC000 {
                let mut name = [0u16; 256];
                let length = unsafe { GetClipboardFormatNameW(format, &mut name) };
                let name = String::from_utf16_lossy(&name[..length.max(0) as usize]);
                // These contain live OLE bookkeeping, not portable content.
                if matches!(name.as_str(), "DataObject" | "Ole Private Data") {
                    continue;
                }
            } else if !matches!(format, 1..=17 | 129..=131 | 142) {
                return Err(format!(
                    "剪贴板包含无法安全备份的格式 {format}，已保留原内容并停止自动输入"
                ));
            }
            session.saved.push(unsafe { ClipboardData::copy(format) }?);
        }
        // Fetching delayed formats may advance the sequence while we own it.
        session.sequence = unsafe { GetClipboardSequenceNumber() };
        Ok(session)
    }

    pub(super) fn check(&self) -> Result<(), String> {
        if self.sequence != unsafe { GetClipboardSequenceNumber() } {
            Err("剪贴板已被其他程序更改，已停止粘贴".into())
        } else {
            Ok(())
        }
    }

    pub(super) fn set_text(&mut self, value: &str) -> Result<(), String> {
        let text = value.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2).map_err(|e| e.to_string())?;
            let mut data = ClipboardData {
                format: 13,
                handle: HANDLE(memory.0),
            };
            let pointer = GlobalLock(memory) as *mut u16;
            if pointer.is_null() {
                return Err("无法分配剪贴板文本".into());
            }
            std::ptr::copy_nonoverlapping(text.as_ptr(), pointer, text.len());
            let _ = GlobalUnlock(memory);
            let _open = OpenGuard::acquire(self.owner)?;
            self.check()?;
            EmptyClipboard().map_err(|e| e.to_string())?;
            self.dirty = true;
            let result = data.publish();
            // Record our write before closing: a later external write must not
            // be mistaken for ours and overwritten during restoration.
            self.sequence = GetClipboardSequenceNumber();
            result
        }
    }

    fn restore(&mut self) -> Result<(), String> {
        if !self.dirty || self.check().is_err() {
            return Ok(());
        }
        let _open = OpenGuard::acquire(self.owner)?;
        // Recheck inside the clipboard lock, including after any retry waits.
        if self.check().is_err() {
            return Ok(());
        }
        unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
        let mut failure = None;
        for data in &mut self.saved {
            if let Err(error) = unsafe { data.publish() } {
                failure = Some(error);
            }
        }
        self.dirty = false;
        failure.map_or(Ok(()), Err)
    }
}

impl Drop for ClipboardSession {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            crate::logger::log_msg("WARN", "PhysicalInput", &format!("恢复剪贴板失败：{error}"));
        }
        // All restored formats already contain actual data; none needs an OLE
        // callback or a surviving worker thread to render it later.
        let _ = unsafe { DestroyWindow(self.owner) };
    }
}
