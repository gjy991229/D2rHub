use super::{region, CaptureMetrics, Captured, Roi};
#[path = "dxgi.rs"]
mod dxgi;
use image::RgbImage;
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc, OnceLock,
};
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::{CloseHandle, FILETIME, HWND, POINT, RECT},
    Graphics::{
        Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS},
        Gdi::ClientToScreen,
    },
    System::{
        Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
        Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    },
    UI::WindowsAndMessaging::{GetClientRect, IsIconic},
};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl},
    settings::*,
    window::Window,
};

type Error = Box<dyn std::error::Error + Send + Sync>;
static RELEASE_EPOCH: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static READBACKS: AtomicU64 = AtomicU64::new(0);
const FRAME_TIMEOUT: Duration = Duration::from_millis(1200);
const IDLE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_SESSIONS: usize = 2;
const MAX_QUEUE_AGE: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct View {
    frame_w: u32,
    frame_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    pid: u32,
    created: u64,
    hwnd: usize,
    view: View,
}

fn target(pid: u32) -> Result<Target, String> {
    let hwnd =
        crate::infrastructure::system::find_game_hwnd(pid).ok_or("目标游戏窗口不存在")? as usize;
    let created = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|e| e.to_string())?;
        let (mut creation, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let result = GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user);
        let _ = CloseHandle(process);
        result.map_err(|e| e.to_string())?;
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime)
    };
    Ok(Target {
        pid,
        created,
        hwnd,
        view: view(hwnd)?,
    })
}

fn view(raw: usize) -> Result<View, String> {
    let _dpi = crate::infrastructure::game_layout_windows::PhysicalDpiScope::enter();
    let hwnd = HWND(raw as *mut _);
    let (mut bounds, mut client, mut origin) = (RECT::default(), RECT::default(), POINT::default());
    unsafe {
        if IsIconic(hwnd).as_bool() {
            return Err("目标游戏已最小化，无法核验".into());
        }
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut bounds as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .map_err(|e| e.to_string())?;
        GetClientRect(hwnd, &mut client).map_err(|e| e.to_string())?;
        if !ClientToScreen(hwnd, &mut origin).as_bool() {
            return Err("无法定位游戏客户区".into());
        }
    }
    let (fw, fh, x, y, w, h) = (
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        origin.x - bounds.left,
        origin.y - bounds.top,
        client.right - client.left,
        client.bottom - client.top,
    );
    if fw <= 0 || fh <= 0 || x < 0 || y < 0 || w < 100 || h < 100 || x + w > fw || y + h > fh {
        return Err("游戏窗口边界不稳定，稍后重试".into());
    }
    region(w as u32, h as u32, None)?;
    Ok(View {
        frame_w: fw as u32,
        frame_h: fh as u32,
        x: x as u32,
        y: y as u32,
        w: w as u32,
        h: h as u32,
    })
}

fn qpc_100ns() -> Result<i64, String> {
    let (mut count, mut frequency) = (0, 0);
    unsafe {
        QueryPerformanceCounter(&mut count).map_err(|e| e.to_string())?;
        QueryPerformanceFrequency(&mut frequency).map_err(|e| e.to_string())?;
    }
    if frequency <= 0 {
        return Err("无法读取截图时钟".into());
    }
    Ok((i128::from(count) * 10_000_000 / i128::from(frequency)) as i64)
}

struct Pending {
    roi: Option<Roi>,
    requested: Instant,
    not_before: i64,
    reply: mpsc::SyncSender<Result<Captured, String>>,
}
struct Shared {
    target: Target,
    pending: Mutex<Option<Pending>>,
}
struct Shot {
    shared: Arc<Shared>,
}
impl GraphicsCaptureApiHandler for Shot {
    type Flags = Arc<Shared>;
    type Error = Error;
    fn new(ctx: Context<Self::Flags>) -> Result<Self, Error> {
        Ok(Self { shared: ctx.flags })
    }
    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _: InternalCaptureControl,
    ) -> Result<(), Error> {
        // No readback, image allocation or color conversion when no caller needs a frame.
        let mut slot = self.shared.pending.lock();
        let Some(request) = slot.as_ref() else {
            return Ok(());
        };
        let timestamp = match frame.timestamp() {
            Ok(time) => time.Duration,
            Err(error) => {
                if let Some(request) = slot.take() {
                    let _ = request.reply.try_send(Err(error.to_string()));
                }
                return Ok(());
            }
        };
        if timestamp < request.not_before {
            return Ok(());
        }
        let request = slot.take().unwrap();
        drop(slot);
        let result = (|| {
            if request.requested.elapsed() > FRAME_TIMEOUT {
                return Err("截图请求已过期".into());
            }
            let v = self.shared.target.view;
            if frame.width() != v.frame_w
                || frame.height() != v.frame_h
                || view(self.shared.target.hwnd)? != v
            {
                return Err("游戏窗口尺寸已变化，重新建立截图会话".into());
            }
            let (x, y, w, h) = region(v.w, v.h, request.roi)?;
            let mut metrics = CaptureMetrics {
                wait_frame_ms: request.requested.elapsed().as_secs_f64() * 1000.0,
                client_width: v.w,
                client_height: v.h,
                output_width: w,
                output_height: h,
                ..Default::default()
            };
            let start = Instant::now();
            // Copy only this GPU sub-rectangle to CPU memory. Never read the full frame first.
            #[cfg(test)]
            READBACKS.fetch_add(1, Ordering::Relaxed);
            let mut buffer = frame
                .buffer_crop(v.x + x, v.y + y, v.x + x + w, v.y + y + h)
                .map_err(|e| e.to_string())?;
            metrics.readback_ms = start.elapsed().as_secs_f64() * 1000.0;
            let start = Instant::now();
            let pitch = buffer.row_pitch() as usize;
            let raw = buffer.as_raw_buffer();
            if pitch < w as usize * 4 || raw.len() < pitch * h as usize {
                return Err("截图像素行跨度无效".into());
            }
            let image = rgba_to_rgb(raw, pitch, w, h)?;
            metrics.convert_ms = start.elapsed().as_secs_f64() * 1000.0;
            metrics.frame_age_ms =
                (qpc_100ns()?.saturating_sub(timestamp).max(0) as f64) / 10_000.0;
            if request.roi.is_none()
                && image
                    .pixels()
                    .step_by(97)
                    .all(|p| p.0.iter().all(|c| *c < 5))
            {
                return Err("截图为空或黑帧，无法核验".into());
            }
            Ok(Captured { image, metrics })
        })();
        let _ = request.reply.try_send(result);
        Ok(())
    }
    fn on_closed(&mut self) -> Result<(), Error> {
        if let Some(request) = self.shared.pending.lock().take() {
            let _ = request.reply.try_send(Err("游戏截图窗口已关闭".into()));
        }
        Ok(())
    }
}

fn rgba_to_rgb(raw: &[u8], pitch: usize, width: u32, height: u32) -> Result<RgbImage, String> {
    if pitch < width as usize * 4 || raw.len() < pitch * height as usize {
        return Err("截图像素行跨度无效".into());
    }
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for row in raw.chunks(pitch).take(height as usize) {
        for pixel in row[..width as usize * 4].as_chunks::<4>().0 {
            rgb.extend_from_slice(&pixel[..3]);
        }
    }
    RgbImage::from_raw(width, height, rgb).ok_or("截图像素长度无效".into())
}

struct Session {
    interval_ms: Option<u64>,
    shared: Arc<Shared>,
    control: Option<CaptureControl<Shot, Error>>,
    used: Instant,
}
impl Session {
    fn new(target: Target) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            target,
            pending: Mutex::new(None),
        });
        let interval =
            if GraphicsCaptureApi::is_minimum_update_interval_supported().unwrap_or(false) {
                MinimumUpdateIntervalSettings::Custom(Duration::from_millis(100))
            } else {
                MinimumUpdateIntervalSettings::Default
            };
        let border = if GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false) {
            DrawBorderSettings::WithoutBorder
        } else {
            DrawBorderSettings::Default
        };
        let settings = |border| {
            Settings::new(
                Window::from_raw_hwnd(target.hwnd as *mut _),
                CursorCaptureSettings::WithoutCursor,
                border,
                SecondaryWindowSettings::Default,
                interval,
                DirtyRegionSettings::Default,
                ColorFormat::Rgba8,
                shared.clone(),
            )
        };
        // Border suppression is best-effort: older Windows versions or system
        // policy must not prevent the recognition capture from starting.
        let control = match Shot::start_free_threaded(settings(border)) {
            Ok(control) => control,
            Err(error) if border == DrawBorderSettings::WithoutBorder => {
                crate::logger::log_msg(
                    "WARN",
                    "VisionCapture",
                    &format!("无边框截图启动失败，尝试系统默认边框：{error}"),
                );
                Shot::start_free_threaded(settings(DrawBorderSettings::Default))
                    .map_err(|e| e.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            interval_ms: if matches!(interval, MinimumUpdateIntervalSettings::Custom(_)) {
                Some(100)
            } else {
                None
            },
            shared,
            control: Some(control),
            used: Instant::now(),
        })
    }
    fn finished(&self) -> bool {
        self.control.as_ref().is_none_or(|c| c.is_finished())
    }
    fn read(&mut self, roi: Option<Roi>) -> Result<Captured, String> {
        self.used = Instant::now();
        let (tx, rx) = mpsc::sync_channel(1);
        *self.shared.pending.lock() = Some(Pending {
            roi,
            requested: Instant::now(),
            not_before: qpc_100ns()?,
            reply: tx,
        });
        let result = rx
            .recv_timeout(FRAME_TIMEOUT)
            .map_err(|_| "截图超时或窗口未刷新".to_string());
        self.shared.pending.lock().take();
        let mut captured = result??;
        captured.metrics.minimum_update_interval_ms = self.interval_ms;
        if target(self.shared.target.pid)? != self.shared.target {
            return Err("截图目标已变化".into());
        }
        Ok(captured)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.shared.pending.lock().take();
        if let Some(control) = self.control.take() {
            let _ = control.stop();
        }
    }
}
struct Request {
    epoch: u64,
    pid: u32,
    roi: Option<Roi>,
    created: Instant,
    reply: mpsc::SyncSender<Result<Captured, String>>,
}
enum Command {
    Capture(Request),
    Release,
}
static BROKER: OnceLock<mpsc::SyncSender<Command>> = OnceLock::new();
static INITIALIZE: Mutex<()> = Mutex::new(());
fn broker() -> Result<&'static mpsc::SyncSender<Command>, String> {
    let _init = INITIALIZE.lock();
    if let Some(tx) = BROKER.get() {
        return Ok(tx);
    }
    let (tx, rx) = mpsc::sync_channel(4);
    std::thread::Builder::new()
        .name("vision-capture-pool".into())
        .spawn(move || {
            use windows::Win32::System::WinRT::{
                RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED,
            };
            let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
            let mut sessions: Vec<Session> = Vec::new();
            let mut epoch = RELEASE_EPOCH.load(Ordering::Acquire);
            loop {
                let current_epoch = RELEASE_EPOCH.load(Ordering::Acquire);
                if epoch != current_epoch {
                    sessions.clear();
                    epoch = current_epoch;
                }
                // Also runs while idle, so a stopped/minimized/unused game does not retain a session.
                sessions.retain(|s| {
                    s.used.elapsed() < IDLE_TIMEOUT
                        && !s.finished()
                        && target(s.shared.target.pid).ok() == Some(s.shared.target)
                });
                let command = match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(c) => c,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                let Command::Capture(request) = command else {
                    sessions.clear();
                    continue;
                };
                let result = (|| {
                    if request.epoch != RELEASE_EPOCH.load(Ordering::Acquire) {
                        return Err("截图请求已取消".into());
                    }
                    if request.created.elapsed() > MAX_QUEUE_AGE {
                        return Err("截图排队超时，请稍后重试".into());
                    }
                    let queue_ms = request.created.elapsed().as_secs_f64() * 1000.0;
                    let current = target(request.pid)?;
                    sessions.retain(|s| {
                        s.shared.target.pid != request.pid || s.shared.target == current
                    });
                    let found = sessions
                        .iter()
                        .position(|s| s.shared.target == current && !s.finished());
                    let reused = found.is_some();
                    let setup = Instant::now();
                    let index = if let Some(index) = found {
                        index
                    } else {
                        if sessions.len() >= MAX_SESSIONS {
                            let oldest = sessions
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, s)| s.used)
                                .unwrap()
                                .0;
                            sessions.remove(oldest);
                        }
                        sessions.push(Session::new(current)?);
                        sessions.len() - 1
                    };
                    let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
                    let captured = sessions[index].read(request.roi);
                    if captured.is_err() {
                        sessions.remove(index);
                    }
                    let mut captured = captured?;
                    captured.metrics.reused = reused;
                    captured.metrics.setup_ms = setup_ms;
                    captured.metrics.queue_ms = queue_ms;
                    captured.metrics.total_ms = request.created.elapsed().as_secs_f64() * 1000.0;
                    Ok(captured)
                })();
                let _ = request.reply.try_send(result);
            }
            drop(sessions);
            if initialized {
                unsafe {
                    RoUninitialize();
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let _ = BROKER.set(tx);
    Ok(BROKER.get().unwrap())
}
pub(super) fn capture(pid: u32, roi: Option<Roi>) -> Result<Captured, String> {
    // Windows 10 21H2 cannot reliably suppress WGC's colored capture border.
    // Use DXGI Desktop Duplication instead; deliberately do not fall back to WGC,
    // because that would reintroduce the very border this backend avoids.
    dxgi::capture(pid, roi)
}

#[allow(dead_code)]
fn capture_wgc(pid: u32, roi: Option<Roi>) -> Result<Captured, String> {
    let (tx, rx) = mpsc::sync_channel(1);
    broker()?
        .try_send(Command::Capture(Request {
            epoch: RELEASE_EPOCH.load(Ordering::Acquire),
            pid,
            roi,
            created: Instant::now(),
            reply: tx,
        }))
        .map_err(|_| "截图队列忙，请稍后重试".to_string())?;
    rx.recv_timeout(Duration::from_secs(3))
        .map_err(|_| "截图服务响应超时".to_string())?
}
pub(super) fn release_sessions() {
    dxgi::release_sessions();
    RELEASE_EPOCH.fetch_add(1, Ordering::AcqRel);
    if let Some(tx) = BROKER.get() {
        let _ = tx.try_send(Command::Release);
    }
}

#[cfg(test)]
pub(super) fn assert_idle_has_no_readbacks() {
    let before = READBACKS.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        before,
        READBACKS.load(Ordering::Relaxed),
        "idle frames must not be copied to CPU"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pitched_readback_skips_padding_and_alpha() {
        let raw = [
            1, 2, 3, 255, 4, 5, 6, 255, 99, 99, 99, 99, 7, 8, 9, 255, 10, 11, 12, 255, 99, 99, 99,
            99,
        ];
        assert_eq!(
            rgba_to_rgb(&raw, 12, 2, 2).unwrap().into_raw(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
        );
        assert!(rgba_to_rgb(&raw[..12], 12, 2, 2).is_err());
        assert!(rgba_to_rgb(&raw, 4, 2, 2).is_err());
    }
}
