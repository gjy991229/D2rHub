use parking_lot::RwLock;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
    window::Window,
};

const WGC_MIN_UPDATE_INTERVAL_MS: u64 = 150;
const FRAME_CAPACITY_RECLAIM_MIN_BYTES: usize = 4 * 1024 * 1024;

fn should_shrink_frame_capacity(capacity: usize, required_len: usize) -> bool {
    capacity.saturating_sub(required_len) >= FRAME_CAPACITY_RECLAIM_MIN_BYTES
        && required_len <= capacity / 2
}

#[derive(Clone)]
pub struct FrameBufferData {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub enum CaptureBackend {
    Wgc {
        buffer: Arc<RwLock<FrameBufferData>>,
        stop_flag: Arc<AtomicBool>,
    },
    Xcap {
        xcap_window: Option<xcap::Window>,
    },
}

/// 游戏窗口截图器（双路径支持：WGC 异步高性能驱动 + xcap 同步降级）
pub struct Capturer {
    pub width: u32,
    pub height: u32,
    pid: u32,
    cached_title: Option<String>,
    backend: CaptureBackend,
}

struct CaptureHandler {
    buffer: Arc<RwLock<FrameBufferData>>,
    stop_flag: Arc<AtomicBool>,
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = (Arc<RwLock<FrameBufferData>>, Arc<AtomicBool>);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            buffer: ctx.flags.0,
            stop_flag: ctx.flags.1,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.stop_flag.load(Ordering::Acquire) {
            capture_control.stop();
            return Ok(());
        }

        let w = frame.width();
        let h = frame.height();

        if let Ok(mut buf) = frame.buffer() {
            if let Ok(slice) = buf.as_nopadding_buffer() {
                let Some(mut shared) = self.buffer.try_write() else {
                    return Ok(());
                };
                shared.width = w;
                shared.height = h;
                if shared.data.len() != slice.len() {
                    shared.data.resize(slice.len(), 0);
                    if should_shrink_frame_capacity(shared.data.capacity(), slice.len()) {
                        shared.data.shrink_to_fit();
                    }
                }
                shared.data.copy_from_slice(slice);
            }
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Capturer {
    pub fn new(pid: u32, fallback_title: &str) -> Result<Self, String> {
        let live_title = find_title_by_pid(pid)
            .or_else(|| {
                if fallback_title.is_empty() {
                    None
                } else {
                    Some(fallback_title.to_string())
                }
            })
            .ok_or_else(|| "无法定位游戏窗口：PID 和 fallback 标题均不可用".to_string())?;

        let buffer = Arc::new(RwLock::new(FrameBufferData {
            data: Vec::new(),
            width: 0,
            height: 0,
        }));

        let stop_flag = Arc::new(AtomicBool::new(false));

        let mut capturer = Self {
            width: 0,
            height: 0,
            pid,
            cached_title: Some(live_title.clone()),
            backend: CaptureBackend::Wgc {
                buffer: buffer.clone(),
                stop_flag: stop_flag.clone(),
            },
        };

        // 尝试启动 WGC
        if let Err(e) = capturer.start_wgc_thread(&buffer, &stop_flag) {
            stop_flag.store(true, Ordering::Release);
            crate::logger::log_msg(
                "WARN",
                "OCR",
                &format!("WGC 启动失败: {}，自动降级为 xcap 同步截图模式", e),
            );
            // 降级到 xcap
            let xcap_window = Self::find_xcap_window(&live_title).ok();

            if let Some(ref win) = xcap_window {
                capturer.width = win.width().unwrap_or(0);
                capturer.height = win.height().unwrap_or(0);
            }

            capturer.backend = CaptureBackend::Xcap { xcap_window };
        }

        Ok(capturer)
    }

    fn start_wgc_thread(
        &self,
        buffer: &Arc<RwLock<FrameBufferData>>,
        stop_flag: &Arc<AtomicBool>,
    ) -> Result<(), String> {
        // PID 查找优先；失败时（如 D2RHub 重启后 PID 丢失）回退到标题精确匹配
        let hwnd = crate::commands::system::find_game_hwnd(self.pid)
            .or_else(|| {
                self.cached_title
                    .as_ref()
                    .and_then(|t| crate::commands::system::find_game_hwnd_by_title(t))
            })
            .ok_or_else(|| {
                format!(
                    "未找到游戏窗口句柄 (PID={}, title={:?})",
                    self.pid, self.cached_title
                )
            })?;

        let window = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);

        let flags = (buffer.clone(), stop_flag.clone());

        let settings = Settings::new(
            window,
            CursorCaptureSettings::WithoutCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Custom(std::time::Duration::from_millis(
                WGC_MIN_UPDATE_INTERVAL_MS,
            )),
            DirtyRegionSettings::Default,
            ColorFormat::Rgba8,
            flags,
        );

        let buffer_for_verify = buffer.clone();
        let stop_flag_for_thread = stop_flag.clone();
        std::thread::spawn(move || {
            if let Err(e) = CaptureHandler::start(settings) {
                crate::logger::log_msg(
                    "ERROR",
                    "OCR",
                    &format!("CaptureHandler::start 发生错误: {}", e),
                );
            }
            stop_flag_for_thread.store(true, Ordering::Release);
        });

        // 启动验证：等待首帧到达，超时则判定 WGC 不可用
        let verify_timeout = std::time::Duration::from_secs(2);
        let verify_start = std::time::Instant::now();
        loop {
            if stop_flag.load(Ordering::Acquire) {
                return Err("WGC 捕获线程在首帧到达前已退出".to_string());
            }
            {
                let shared = buffer_for_verify.read();
                if shared.width > 0 && shared.height > 0 && !shared.data.is_empty() {
                    break; // 首帧已到达
                }
            }
            if verify_start.elapsed() >= verify_timeout {
                stop_flag.store(true, Ordering::Release);
                return Err("WGC 启动后未能在 2s 内收到首帧，窗口可能不支持 WGC 捕获".to_string());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        Ok(())
    }

    fn find_xcap_window(title: &str) -> Result<xcap::Window, String> {
        let wins = xcap::Window::all().map_err(|e| format!("枚举窗口失败: {}", e))?;
        wins.into_iter()
            .find(|w| w.title().ok().map(|t| t == title).unwrap_or(false))
            .ok_or_else(|| format!("xcap 未找到目标窗口: {}", title))
    }

    pub fn capture_into(&mut self, buffer: &mut [u8]) -> Result<(), String> {
        match &mut self.backend {
            CaptureBackend::Wgc {
                buffer: shared_buf, ..
            } => {
                let shared = shared_buf.read();

                if shared.width == 0 || shared.height == 0 || shared.data.is_empty() {
                    return Err("WGC 尚未捕获到画面或窗口未就绪".to_string());
                }

                let w = shared.width as usize;
                let h = shared.height as usize;

                if w != self.width as usize || h != self.height as usize {
                    self.width = shared.width;
                    self.height = shared.height;
                }

                let expected = w * h * 4;

                if shared.data.len() < expected {
                    return Err(format!(
                        "截图数据不完整, 需要 {} 字节, 实际 {}",
                        expected,
                        shared.data.len()
                    ));
                }
                if buffer.len() < expected {
                    return Err(format!(
                        "缓冲区太小, 需要 {} 字节, 实际 {}",
                        expected,
                        buffer.len()
                    ));
                }

                buffer[..expected].copy_from_slice(&shared.data[..expected]);
                Ok(())
            }
            CaptureBackend::Xcap { xcap_window } => {
                let img = match xcap_window.as_ref().and_then(|w| w.capture_image().ok()) {
                    Some(img) => img,
                    None => {
                        let title = self.cached_title.as_ref().ok_or("无窗口标题用于 xcap")?;
                        let win = Self::find_xcap_window(title)?;
                        let img = win
                            .capture_image()
                            .map_err(|e| format!("xcap 窗口截图失败: {}", e))?;
                        *xcap_window = Some(win);
                        img
                    }
                };

                let w = img.width() as usize;
                let h = img.height() as usize;

                if w != self.width as usize || h != self.height as usize {
                    self.width = w as u32;
                    self.height = h as u32;
                }

                let raw = img.as_raw();
                let expected = w * h * 4;

                if raw.len() < expected {
                    return Err(format!(
                        "截图数据不完整, 需要 {} 字节, 实际 {}",
                        expected,
                        raw.len()
                    ));
                }
                if buffer.len() < expected {
                    return Err(format!(
                        "缓冲区太小, 需要 {} 字节, 实际 {}",
                        expected,
                        buffer.len()
                    ));
                }

                buffer[..expected].copy_from_slice(&raw[..expected]);
                Ok(())
            }
        }
    }

    pub fn buffer_size(&self) -> usize {
        (self.width as usize * self.height as usize * 4).max(1)
    }

    pub fn reset_cache(&mut self) {
        let live_title = find_title_by_pid(self.pid)
            .unwrap_or_else(|| self.cached_title.clone().unwrap_or_default());
        self.cached_title = Some(live_title.clone());

        let mut new_wgc_params = None;

        match &mut self.backend {
            CaptureBackend::Wgc { stop_flag, .. } => {
                stop_flag.store(true, Ordering::Release);
                let new_buffer = Arc::new(RwLock::new(FrameBufferData {
                    data: Vec::new(),
                    width: 0,
                    height: 0,
                }));
                let new_stop_flag = Arc::new(AtomicBool::new(false));
                new_wgc_params = Some((new_buffer, new_stop_flag));
            }
            CaptureBackend::Xcap { xcap_window } => {
                *xcap_window = None;
            }
        }

        if let Some((shared_buf, new_stop_flag)) = new_wgc_params {
            if let Err(e) = self.start_wgc_thread(&shared_buf, &new_stop_flag) {
                new_stop_flag.store(true, Ordering::Release);
                crate::logger::log_msg(
                    "WARN",
                    "OCR",
                    &format!("reset_cache 重启 WGC 失败: {}，降级为 xcap", e),
                );
                let xcap_window = Self::find_xcap_window(&live_title).ok();
                self.backend = CaptureBackend::Xcap { xcap_window };
            } else {
                self.backend = CaptureBackend::Wgc {
                    buffer: shared_buf,
                    stop_flag: new_stop_flag,
                };
            }
        }
    }
}

/// 通过 PID 查找游戏窗口标题
fn find_title_by_pid(pid: u32) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let hwnd = crate::commands::system::find_game_hwnd(pid)?;
        extern "system" {
            fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
        }
        let mut buf = [0u16; 260];
        unsafe {
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 260);
            if len > 0 {
                return Some(String::from_utf16_lossy(&buf[..len as usize]));
            }
        }
        None
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

impl Drop for Capturer {
    fn drop(&mut self) {
        if let CaptureBackend::Wgc { stop_flag, .. } = &self.backend {
            stop_flag.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::should_shrink_frame_capacity;

    #[test]
    fn shrinks_after_a_large_resolution_drop() {
        let rgba_4k = 3840 * 2160 * 4;
        let rgba_1080p = 1920 * 1080 * 4;

        assert!(should_shrink_frame_capacity(rgba_4k, rgba_1080p));
    }

    #[test]
    fn keeps_capacity_for_small_or_minor_changes() {
        assert!(!should_shrink_frame_capacity(3 * 1024 * 1024, 1024));
        assert!(!should_shrink_frame_capacity(
            10 * 1024 * 1024,
            8 * 1024 * 1024,
        ));
    }

    #[test]
    fn keeps_capacity_when_the_frame_did_not_shrink() {
        assert!(!should_shrink_frame_capacity(
            8 * 1024 * 1024,
            8 * 1024 * 1024
        ));
        assert!(!should_shrink_frame_capacity(
            8 * 1024 * 1024,
            12 * 1024 * 1024
        ));
    }
}
