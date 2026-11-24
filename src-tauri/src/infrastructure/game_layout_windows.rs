//! On-demand physical monitor/window snapshots. No process polling or global
//! hooks are needed for layout editing, capture, or restore.

use std::path::PathBuf;

use crate::application::multi_instance::FrameCalibrationKey;
use crate::domain::window_layout::{
    FrameInsets, LayoutMonitor, LayoutRect, LayoutTarget, WindowFrameMetrics, WindowFrameProfile,
};
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct GameLayoutWindow {
    pub handle: isize,
    pub pid: u32,
    pub title: String,
    pub executable: PathBuf,
    pub started_at: u64,
    /// Visible frame in physical pixels (minimized snapshots are never captured).
    pub rect: LayoutRect,
    pub minimized: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameFrameSample {
    pub profile: WindowFrameProfile,
    pub visible_rect: LayoutRect,
    pub client_width: u32,
    pub client_height: u32,
    pub handle: isize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCalibrationTarget {
    pub handle: isize,
    pub pid: u32,
    pub key: FrameCalibrationKey,
}

/// Per-launch, bounded calibration work. The ordinary path is two samples;
/// missing windows back off and no launch can request more than eight samples.
pub(crate) struct StartupFrameProbe {
    previous: Option<GameFrameSample>,
    attempts: u8,
    misses: u8,
    next_at: std::time::Duration,
    complete: bool,
}

impl Default for StartupFrameProbe {
    fn default() -> Self {
        Self {
            previous: None,
            attempts: 0,
            misses: 0,
            next_at: std::time::Duration::from_secs(12),
            complete: false,
        }
    }
}

impl StartupFrameProbe {
    pub(crate) fn due(&self, elapsed: std::time::Duration) -> bool {
        !self.complete
            && self.attempts < 8
            && elapsed >= self.next_at
            && elapsed <= std::time::Duration::from_secs(120)
    }

    pub(crate) fn observe(
        &mut self,
        elapsed: std::time::Duration,
        sample: Option<GameFrameSample>,
    ) -> Option<GameFrameSample> {
        self.attempts += 1;
        let stable = sample.is_some() && sample == self.previous;
        self.misses = if sample.is_none() {
            self.misses.saturating_add(1)
        } else {
            0
        };
        let delay = if self.misses == 0 {
            3
        } else {
            3u64 << self.misses.saturating_sub(1).min(3)
        };
        self.next_at = elapsed + std::time::Duration::from_secs(delay);
        self.previous = sample.clone();
        stable.then_some(sample).flatten()
    }

    pub(crate) fn exhausted(&self, elapsed: std::time::Duration) -> bool {
        self.complete || self.attempts >= 8 || elapsed > std::time::Duration::from_secs(120)
    }

    pub(crate) fn reset_sample(&mut self) {
        self.previous = None;
    }

    pub(crate) fn finish(&mut self) {
        self.complete = true;
        self.previous = None;
    }
}

fn native_position(target: LayoutRect, outer: LayoutRect, visible: LayoutRect) -> (i32, i32) {
    (
        target.x.saturating_sub(visible.x.saturating_sub(outer.x)),
        target.y.saturating_sub(visible.y.saturating_sub(outer.y)),
    )
}

/// All three rectangles must be in physical screen pixels, including the client.
fn measured_frame(
    outer: LayoutRect,
    visible: LayoutRect,
    client: LayoutRect,
    dpi: u32,
    style: u32,
    ex_style: u32,
) -> Option<WindowFrameMetrics> {
    fn insets(outer: LayoutRect, inner: LayoutRect) -> Option<FrameInsets> {
        let values = [
            i64::from(inner.x) - i64::from(outer.x),
            i64::from(inner.y) - i64::from(outer.y),
            i64::from(outer.x) + i64::from(outer.width)
                - i64::from(inner.x)
                - i64::from(inner.width),
            i64::from(outer.y) + i64::from(outer.height)
                - i64::from(inner.y)
                - i64::from(inner.height),
        ];
        if values.iter().any(|value| !(0..=256).contains(value)) {
            return None;
        }
        Some(FrameInsets {
            left: values[0] as u32,
            top: values[1] as u32,
            right: values[2] as u32,
            bottom: values[3] as u32,
        })
    }
    let result = WindowFrameMetrics {
        dpi,
        style,
        ex_style,
        visible: insets(visible, client)?,
        invisible: insets(outer, visible)?,
    };
    result.valid().then_some(result)
}

pub fn with_frame_profiles(
    mut monitors: Vec<LayoutMonitor>,
    profiles: &[WindowFrameProfile],
) -> Vec<LayoutMonitor> {
    for monitor in &mut monitors {
        // Several window styles can be measured on one display. Prefer the
        // newest measurement instead of pinning the preview to the first style.
        if let Some(profile) = profiles.iter().rev().find(|profile| {
            profile.monitor_id == monitor.id
                && (profile.scale_factor - monitor.scale_factor).abs() < 0.001
                && profile.metrics.valid()
        }) {
            monitor.frame = Some(profile.metrics);
        }
    }
    monitors
}

#[cfg(target_os = "windows")]
mod native {
    use std::collections::HashMap;
    use std::os::windows::ffi::OsStringExt;

    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, BOOL, FILETIME, HWND, LPARAM, POINT, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::Graphics::Gdi::{
        ClientToScreen, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW,
        MonitorFromWindow, DISPLAY_DEVICEW, HDC, HMONITOR, MONITORINFOEXW,
        MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::HiDpi::{
        AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, SetThreadDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed, SetWindowPos,
        ShowWindowAsync, GWL_EXSTYLE, GWL_STYLE, GW_OWNER, HWND_TOP, MONITORINFOF_PRIMARY,
        SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
        SW_RESTORE, WINDOW_EX_STYLE, WS_CAPTION, WS_OVERLAPPEDWINDOW, WS_THICKFRAME,
    };

    use super::*;

    pub(crate) struct PhysicalDpiScope(DPI_AWARENESS_CONTEXT);

    impl PhysicalDpiScope {
        pub(crate) fn enter() -> Self {
            Self(unsafe {
                SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            })
        }
    }

    impl Drop for PhysicalDpiScope {
        fn drop(&mut self) {
            if !self.0 .0.is_null() {
                unsafe {
                    SetThreadDpiAwarenessContext(self.0);
                }
            }
        }
    }

    // DISPLAY1/2 are source names, and can be reassigned after a reconnect.
    // Persist the monitor interface identity while retaining DISPLAYn as its label.
    fn monitor_identity(device: &[u16]) -> String {
        let end = device
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(device.len());
        let fallback = String::from_utf16_lossy(&device[..end]);
        let mut display = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        if unsafe { EnumDisplayDevicesW(PCWSTR(device.as_ptr()), 0, &mut display, 1) }.as_bool() {
            let end = display
                .DeviceID
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(display.DeviceID.len());
            if end > 0 {
                return String::from_utf16_lossy(&display.DeviceID[..end]);
            }
        }
        fallback
    }

    fn rectangle(rect: RECT) -> LayoutRect {
        LayoutRect {
            x: rect.left,
            y: rect.top,
            width: rect.right.saturating_sub(rect.left).max(0) as u32,
            height: rect.bottom.saturating_sub(rect.top).max(0) as u32,
        }
    }

    fn estimated_frame(dpi: u32) -> WindowFrameMetrics {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 1280,
            bottom: 720,
        };
        let mut result = WindowFrameMetrics {
            dpi,
            style: WS_OVERLAPPEDWINDOW.0,
            ex_style: 0,
            visible: FrameInsets {
                left: 1,
                top: (31 * dpi / 96),
                right: 1,
                bottom: 1,
            },
            invisible: FrameInsets::default(),
        };
        if unsafe {
            AdjustWindowRectExForDpi(
                &mut rect,
                WS_OVERLAPPEDWINDOW,
                false,
                WINDOW_EX_STYLE(0),
                dpi,
            )
        }
        .is_ok()
        {
            let edge = (dpi / 96).max(1);
            result.visible = FrameInsets {
                left: edge,
                top: (-rect.top).max(0) as u32,
                right: edge,
                bottom: edge,
            };
            result.invisible = FrameInsets {
                left: ((-rect.left).max(0) as u32).saturating_sub(edge),
                top: 0,
                right: ((rect.right - 1280).max(0) as u32).saturating_sub(edge),
                bottom: ((rect.bottom - 720).max(0) as u32).saturating_sub(edge),
            };
        }
        result
    }

    fn visible_bounds(hwnd: HWND) -> Option<RECT> {
        let mut rect = RECT::default();
        unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut rect as *mut _ as *mut _,
                std::mem::size_of::<RECT>() as u32,
            )
        }
        .ok()?;
        (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
    }

    /// Shared by account capture/polling and layout snapshots. Never return the
    /// native invisible frame (or minimized sentinel coordinates) as user data.
    pub fn visible_window_rect(handle: isize) -> Option<LayoutRect> {
        let _dpi = PhysicalDpiScope::enter();
        let hwnd = HWND(handle as *mut _);
        if !unsafe { IsWindow(hwnd) }.as_bool() || unsafe { IsIconic(hwnd) }.as_bool() {
            return None;
        }
        visible_bounds(hwnd).map(rectangle)
    }

    pub fn move_visible_window(handle: isize, x: i32, y: i32) -> bool {
        apply_handle(
            HWND(handle as *mut _),
            LayoutRect {
                x,
                y,
                width: 0,
                height: 0,
            },
            false,
        )
        .is_ok()
    }

    // Group lookup deliberately avoids DWM, client rectangles and outer-frame
    // measurements. Each non-representative only needs this metadata once.
    pub fn frame_target(pid: u32, executable: &std::path::Path) -> Option<FrameCalibrationTarget> {
        let _dpi = PhysicalDpiScope::enter();
        let (actual, _) = process_identity(pid)?;
        if !crate::infrastructure::system::executable_paths_match(&actual, executable) {
            return None;
        }
        target_for_handle(
            HWND(crate::infrastructure::system::find_game_hwnd(pid)? as *mut _),
            pid,
        )
    }

    fn target_for_handle(hwnd: HWND, pid: u32) -> Option<FrameCalibrationTarget> {
        unsafe {
            let mut current_pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut current_pid));
            if current_pid != pid || !IsWindow(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
                return None;
            }
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            if IsIconic(hwnd).as_bool()
                || IsZoomed(hwnd).as_bool()
                || style & WS_CAPTION.0 != WS_CAPTION.0
            {
                return None;
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if !GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
                return None;
            }
            let (mut dpi_x, mut dpi_y) = (96, 96);
            GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).ok()?;
            let window_dpi = GetDpiForWindow(hwnd);
            if window_dpi == 0 {
                return None;
            }
            Some(FrameCalibrationTarget {
                handle: hwnd.0 as isize,
                pid,
                key: FrameCalibrationKey {
                    monitor_id: monitor_identity(&info.szDevice),
                    monitor_dpi: dpi_x,
                    window_dpi,
                    style,
                    ex_style: GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
                },
            })
        }
    }

    /// Only a group representative may call this. Validate the group both before
    /// and after measurement so a DPI/monitor/style transition cannot poison it.
    pub fn sample_target(target: &FrameCalibrationTarget) -> Option<GameFrameSample> {
        let _dpi = PhysicalDpiScope::enter();
        let hwnd = HWND(target.handle as *mut _);
        if target_for_handle(hwnd, target.pid).as_ref() != Some(target) {
            return None;
        }
        unsafe {
            let visible = rectangle(visible_bounds(hwnd)?);
            let (mut outer, mut client) = (RECT::default(), RECT::default());
            GetWindowRect(hwnd, &mut outer).ok()?;
            GetClientRect(hwnd, &mut client).ok()?;
            if client.right < 800 || client.bottom < 600 {
                return None;
            }
            let mut origin = POINT::default();
            if !ClientToScreen(hwnd, &mut origin).as_bool() {
                return None;
            }
            let client = LayoutRect {
                x: origin.x,
                y: origin.y,
                width: client.right as u32,
                height: client.bottom as u32,
            };
            let metrics = measured_frame(
                rectangle(outer),
                visible,
                client,
                target.key.window_dpi,
                target.key.style,
                target.key.ex_style,
            )?;
            if target_for_handle(hwnd, target.pid).as_ref() != Some(target) {
                return None;
            }
            Some(GameFrameSample {
                profile: WindowFrameProfile {
                    monitor_id: target.key.monitor_id.clone(),
                    scale_factor: f64::from(target.key.monitor_dpi) / 96.0,
                    metrics,
                },
                visible_rect: visible,
                client_width: client.width,
                client_height: client.height,
                handle: target.handle,
            })
        }
    }

    pub fn monitors() -> Result<Vec<LayoutMonitor>, AppError> {
        let _dpi = PhysicalDpiScope::enter();
        unsafe extern "system" fn collect(
            monitor: HMONITOR,
            _: HDC,
            _: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            let result = &mut *(data.0 as *mut Vec<LayoutMonitor>);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
                let end = info
                    .szDevice
                    .iter()
                    .position(|&unit| unit == 0)
                    .unwrap_or(info.szDevice.len());
                let name = String::from_utf16_lossy(&info.szDevice[..end]);
                let id = monitor_identity(&info.szDevice);
                let (mut dpi_x, mut dpi_y) = (96, 96);
                let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
                result.push(LayoutMonitor {
                    name: name.trim_start_matches(r"\\.\").to_string(),
                    id,
                    bounds: rectangle(info.monitorInfo.rcMonitor),
                    work_area: rectangle(info.monitorInfo.rcWork),
                    primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                    scale_factor: f64::from(dpi_x) / 96.0,
                    frame: Some(estimated_frame(dpi_x)),
                });
            }
            BOOL(1)
        }
        let mut result: Vec<LayoutMonitor> = Vec::new();
        unsafe {
            if !EnumDisplayMonitors(
                None,
                None,
                Some(collect),
                LPARAM(&mut result as *mut _ as isize),
            )
            .as_bool()
            {
                return Err(AppError::FileError("读取显示器失败".into()));
            }
        }
        result.sort_by_key(|monitor| (!monitor.primary, monitor.bounds.x, monitor.bounds.y));
        if result.is_empty() {
            return Err(AppError::FileError("未检测到显示器".into()));
        }
        Ok(result)
    }

    fn process_identity(pid: u32) -> Option<(PathBuf, u64)> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let read = (|| {
                let mut path = vec![0u16; 32768];
                let mut length = path.len() as u32;
                QueryFullProcessImageNameW(
                    process,
                    PROCESS_NAME_WIN32,
                    PWSTR(path.as_mut_ptr()),
                    &mut length,
                )
                .ok()?;
                let executable =
                    PathBuf::from(std::ffi::OsString::from_wide(&path[..length as usize]));
                if !executable
                    .file_name()?
                    .to_string_lossy()
                    .eq_ignore_ascii_case("D2R.exe")
                {
                    return None;
                }
                let (mut creation, mut exit, mut kernel, mut user) = (
                    FILETIME::default(),
                    FILETIME::default(),
                    FILETIME::default(),
                    FILETIME::default(),
                );
                GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user).ok()?;
                Some((
                    executable,
                    (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
                ))
            })();
            let _ = CloseHandle(process);
            read
        }
    }

    pub fn snapshot() -> Result<Vec<GameLayoutWindow>, AppError> {
        let _dpi = PhysicalDpiScope::enter();
        #[derive(Default)]
        struct Scan {
            processes: HashMap<u32, Option<(PathBuf, u64)>>,
            windows: HashMap<u32, GameLayoutWindow>,
        }
        unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null()) {
                return BOOL(1);
            }
            let scan = &mut *(data.0 as *mut Scan);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let Some((executable, started_at)) = scan
                .processes
                .entry(pid)
                .or_insert_with(|| process_identity(pid))
                .clone()
            else {
                return BOOL(1);
            };
            let mut title = [0u16; 512];
            let length = GetWindowTextW(hwnd, &mut title);
            if length <= 0 {
                return BOOL(1);
            }
            let minimized = IsIconic(hwnd).as_bool();
            let rect = if minimized {
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_err() {
                    return BOOL(1);
                }
                rectangle(rect)
            } else {
                let Some(visible) = visible_window_rect(hwnd.0 as isize) else {
                    return BOOL(1);
                };
                visible
            };
            let window = GameLayoutWindow {
                handle: hwnd.0 as isize,
                pid,
                title: String::from_utf16_lossy(&title[..length as usize]),
                executable,
                started_at,
                rect,
                minimized,
            };
            // Game-owned dialogs must not replace the largest/main window.
            if scan.windows.get(&pid).is_none_or(|previous| {
                u64::from(previous.rect.width) * u64::from(previous.rect.height)
                    < u64::from(rect.width) * u64::from(rect.height)
            }) {
                scan.windows.insert(pid, window);
            }
            BOOL(1)
        }
        let mut scan = Scan::default();
        unsafe { EnumWindows(Some(collect), LPARAM(&mut scan as *mut _ as isize)) }
            .map_err(|error| AppError::FileError(format!("读取游戏窗口失败：{error}")))?;
        let mut windows: Vec<_> = scan.windows.into_values().collect();
        windows.sort_by_key(|window| (window.started_at, window.pid));
        Ok(windows)
    }

    fn apply_handle(hwnd: HWND, rect: LayoutRect, raise: bool) -> Result<(), AppError> {
        let _dpi = PhysicalDpiScope::enter();
        unsafe {
            if IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
                let _ = ShowWindowAsync(hwnd, SW_RESTORE);
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
                while IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
                    if std::time::Instant::now() >= deadline {
                        return Err(AppError::FileError("正在还原游戏窗口，请稍后重试".into()));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let mut outer = RECT::default();
            GetWindowRect(hwnd, &mut outer).map_err(|e| AppError::FileError(e.to_string()))?;
            let visible = visible_bounds(hwnd)
                .ok_or_else(|| AppError::FileError("无法读取游戏窗口可见边界".into()))?;
            let (x, y) = native_position(rect, rectangle(outer), rectangle(visible));
            if !raise && visible.left == rect.x && visible.top == rect.y {
                return Ok(());
            }
            let flags = SWP_NOACTIVATE
                | SWP_NOOWNERZORDER
                | SWP_ASYNCWINDOWPOS
                | SWP_NOSIZE
                | if raise {
                    Default::default()
                } else {
                    SWP_NOZORDER
                };
            SetWindowPos(hwnd, HWND_TOP, x, y, 0, 0, flags)
                .map_err(|error| AppError::FileError(format!("调整游戏窗口失败：{error}")))
        }
    }

    pub fn is_borderless(handle: isize) -> bool {
        let hwnd = HWND(handle as *mut _);
        unsafe {
            IsWindow(hwnd).as_bool()
                && GetWindowLongW(hwnd, GWL_STYLE) as u32 & (WS_CAPTION.0 | WS_THICKFRAME.0) == 0
        }
    }

    fn apply_layout(hwnd: HWND, target: LayoutTarget, raise: bool) -> Result<(), AppError> {
        if is_borderless(hwnd.0 as isize) != target.fullscreen {
            return Err(AppError::FileError(
                "布局的窗口模式将在下次启动时生效".into(),
            ));
        }
        let rect = target.rect;
        if !target.fullscreen {
            return apply_handle(hwnd, rect, raise);
        }
        let _dpi = PhysicalDpiScope::enter();
        if unsafe { IsIconic(hwnd) }.as_bool() {
            return Err(AppError::FileError("请先还原最小化的全屏窗口".into()));
        }
        let flags = SWP_NOACTIVATE
            | SWP_NOOWNERZORDER
            | SWP_ASYNCWINDOWPOS
            | if raise {
                Default::default()
            } else {
                SWP_NOZORDER
            };
        if !raise && visible_bounds(hwnd).map(rectangle) == Some(rect) {
            return Ok(());
        }
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOP,
                rect.x,
                rect.y,
                rect.width as i32,
                rect.height as i32,
                flags,
            )
        }
        .map_err(|error| AppError::FileError(format!("调整全屏游戏窗口失败：{error}")))
    }

    pub fn apply(
        window: &GameLayoutWindow,
        target: LayoutTarget,
        raise: bool,
    ) -> Result<(), AppError> {
        let hwnd = HWND(window.handle as *mut _);
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if !IsWindow(hwnd).as_bool() || pid != window.pid {
                return Err(AppError::FileError("游戏窗口已关闭，请重试".into()));
            }
        }
        if process_identity(pid).is_none_or(|(path, started)| {
            started != window.started_at
                || !crate::infrastructure::system::executable_paths_match(&path, &window.executable)
        }) {
            return Err(AppError::FileError("游戏进程已变化，请重试".into()));
        }
        apply_layout(hwnd, target, raise)
    }

    pub fn apply_for_pid(pid: u32, executable: &std::path::Path, target: LayoutTarget) -> bool {
        if process_identity(pid).is_none_or(|(path, _)| {
            !crate::infrastructure::system::executable_paths_match(&path, executable)
        }) {
            return false;
        }
        crate::infrastructure::system::find_game_hwnd(pid)
            .is_some_and(|handle| apply_layout(HWND(handle as *mut _), target, false).is_ok())
    }
}

#[cfg(target_os = "windows")]
pub(crate) use native::PhysicalDpiScope;
#[cfg(target_os = "windows")]
pub use native::{
    apply, apply_for_pid, frame_target, is_borderless, monitors, move_visible_window,
    sample_target, snapshot, visible_window_rect,
};

#[cfg(not(target_os = "windows"))]
pub fn visible_window_rect(_: isize) -> Option<LayoutRect> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn move_visible_window(_: isize, _: i32, _: i32) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn frame_target(_: u32, _: &std::path::Path) -> Option<FrameCalibrationTarget> {
    None
}
#[cfg(not(target_os = "windows"))]
pub fn sample_target(_: &FrameCalibrationTarget) -> Option<GameFrameSample> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn monitors() -> Result<Vec<LayoutMonitor>, AppError> {
    Err(AppError::FileError("窗口布局仅支持 Windows".into()))
}
#[cfg(not(target_os = "windows"))]
pub fn snapshot() -> Result<Vec<GameLayoutWindow>, AppError> {
    Err(AppError::FileError("窗口布局仅支持 Windows".into()))
}
#[cfg(not(target_os = "windows"))]
pub fn is_borderless(_: isize) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn apply(_: &GameLayoutWindow, _: LayoutTarget, _: bool) -> Result<(), AppError> {
    Err(AppError::FileError("窗口布局仅支持 Windows".into()))
}
#[cfg(not(target_os = "windows"))]
pub fn apply_for_pid(_: u32, _: &std::path::Path, _: LayoutTarget) -> bool {
    false
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn latest_frame_profile_wins_without_crossing_display_scale() {
        let first = sample().profile;
        let mut latest = first.clone();
        latest.metrics.style = 2;
        latest.metrics.visible.top = 40;
        let mut other_scale = latest.clone();
        other_scale.scale_factor = 1.5;
        other_scale.metrics.visible.top = 60;
        let rect = sample().visible_rect;
        let monitor = LayoutMonitor {
            id: first.monitor_id.clone(),
            name: "main".into(),
            bounds: rect,
            work_area: rect,
            primary: true,
            scale_factor: 1.0,
            frame: None,
        };
        let expected = latest.metrics;
        let result = with_frame_profiles(vec![monitor], &[first, latest, other_scale]);
        assert_eq!(result[0].frame, Some(expected));
    }

    fn sample() -> GameFrameSample {
        GameFrameSample {
            profile: WindowFrameProfile {
                monitor_id: "main".into(),
                scale_factor: 1.0,
                metrics: WindowFrameMetrics {
                    dpi: 96,
                    style: 0,
                    ex_style: 0,
                    visible: FrameInsets {
                        left: 1,
                        top: 31,
                        right: 1,
                        bottom: 1,
                    },
                    invisible: FrameInsets {
                        left: 7,
                        top: 0,
                        right: 7,
                        bottom: 7,
                    },
                },
            },
            visible_rect: LayoutRect {
                x: 0,
                y: 0,
                width: 1282,
                height: 752,
            },
            client_width: 1280,
            client_height: 720,
            handle: 1,
        }
    }

    #[test]
    fn stable_startup_calibration_takes_two_samples_then_stops() {
        use std::time::Duration;
        let mut probe = StartupFrameProbe::default();
        let first = Duration::from_secs(12);
        let second = Duration::from_secs(15);
        assert!(probe.due(first));
        assert!(probe.observe(first, Some(sample())).is_none());
        assert!(probe.due(second));
        assert_eq!(probe.observe(second, Some(sample())), Some(sample()));
        probe.finish();
        assert!(!probe.due(Duration::from_secs(18)));
    }

    #[test]
    fn missing_and_unstable_windows_have_bounded_sampling_cost() {
        use std::time::Duration;
        for missing in [true, false] {
            let mut probe = StartupFrameProbe::default();
            let mut times = Vec::new();
            for seconds in (0..600).step_by(3) {
                let elapsed = Duration::from_secs(seconds);
                if probe.due(elapsed) {
                    times.push(seconds);
                    let mut value = sample();
                    value.visible_rect.x = seconds as i32;
                    assert!(probe
                        .observe(elapsed, (!missing).then_some(value))
                        .is_none());
                }
            }
            if missing {
                assert!(!times.is_empty() && times.len() <= 8);
                let intervals: Vec<_> = times.windows(2).map(|pair| pair[1] - pair[0]).collect();
                assert!(intervals.windows(2).all(|pair| pair[1] >= pair[0]));
                assert!(intervals.last().unwrap() > intervals.first().unwrap());
            } else {
                assert_eq!(times.len(), 8);
            }
            assert!(!probe.due(Duration::from_secs(121)));
        }
    }

    #[test]
    fn physical_rects_separate_caption_from_invisible_resize_borders() {
        let rect = |x, y, width, height| LayoutRect {
            x,
            y,
            width,
            height,
        };
        let frame = measured_frame(
            rect(-7, 0, 1296, 759),
            rect(0, 0, 1282, 752),
            rect(1, 31, 1280, 720),
            96,
            0,
            0,
        )
        .unwrap();
        assert_eq!(
            frame.invisible,
            FrameInsets {
                left: 7,
                top: 0,
                right: 7,
                bottom: 7
            }
        );
        assert_eq!(
            frame.visible,
            FrameInsets {
                left: 1,
                top: 31,
                right: 1,
                bottom: 1
            }
        );
        // The same borders on a monitor to the left of the primary.
        assert_eq!(
            native_position(
                rect(0, 0, 1280, 720),
                rect(93, 100, 1296, 759),
                rect(100, 100, 1282, 752)
            ),
            (-7, 0)
        );
        assert_eq!(
            native_position(
                rect(-2560, -20, 1280, 720),
                rect(93, 100, 1296, 759),
                rect(100, 100, 1282, 752)
            ),
            (-2567, -20)
        );
        assert_eq!(
            measured_frame(
                rect(-2567, -100, 1296, 759),
                rect(-2560, -100, 1282, 752),
                rect(-2559, -69, 1280, 720),
                96,
                0,
                0
            ),
            Some(frame)
        );
        // Reject mixed coordinate systems/transitional geometry instead of caching it.
        assert!(measured_frame(
            rect(0, 0, 1296, 759),
            rect(0, 0, 1282, 752),
            rect(0, 0, 1920, 1080),
            144,
            0,
            0
        )
        .is_none());
    }
}
