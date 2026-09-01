use crate::commands::global_config::WindowGeometry;
use crate::error::AppError;
use crate::state::SharedState;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, LogicalPosition, Manager, Monitor, PhysicalPosition, WebviewWindow};

const PLACEMENT_VERSION: u32 = 2;
const AUXILIARY_LABELS: [&str; 3] = ["overlay", "stats-overlay", "bongo-cat"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct MonitorSnapshot {
    name: Option<String>,
    position: PhysicalPoint,
    size: PhysicalSize,
    work_area: PhysicalRect,
    scale_factor: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PhysicalSize {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DockEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedPlacement {
    version: u32,
    preferred_rect: PhysicalRect,
    preferred_monitor: MonitorSnapshot,
    anchor_x: f64,
    anchor_y: f64,
    #[serde(default)]
    dock_edge: Option<DockEdge>,
    #[serde(default)]
    fallback_rect: Option<PhysicalRect>,
    saved_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementOutcome {
    pub label: String,
    pub moved: bool,
    pub recovered: bool,
    pub used_fallback: bool,
    pub monitor_name: Option<String>,
}

#[derive(Debug, Clone)]
struct ResolvedPlacement {
    rect: PhysicalRect,
    monitor_index: usize,
    recovered: bool,
    used_fallback: bool,
}

fn validate_label(label: &str) -> Result<(), AppError> {
    if AUXILIARY_LABELS.contains(&label) {
        Ok(())
    } else {
        Err(AppError::Unknown(format!("不支持的悬浮窗标签: {label}")))
    }
}

fn placement_path(app_data_dir: &str, label: &str) -> PathBuf {
    Path::new(app_data_dir).join(format!(
        "window_placement_{}_v2.json",
        label.replace('-', "_")
    ))
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn load_placement(path: &Path) -> Option<SavedPlacement> {
    for candidate in [path.to_path_buf(), backup_path(path)] {
        if let Ok(content) = fs::read_to_string(candidate) {
            if let Ok(placement) = serde_json::from_str::<SavedPlacement>(&content) {
                if placement.version == PLACEMENT_VERSION && valid_rect(placement.preferred_rect) {
                    return Some(placement);
                }
            }
        }
    }
    None
}

/// 使用“当前文件 → 备份 → 新文件”的可恢复替换流程。即使进程在中途退出，下一次
/// 启动也能从旧文件或备份中恢复，而不会留下半个 JSON。
fn save_placement(path: &Path, placement: &SavedPlacement) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| AppError::ConfigWriteError(error.to_string()))?;
    }
    let temporary = temporary_path(path);
    let backup = backup_path(path);
    let content = serde_json::to_vec_pretty(placement)?;
    fs::write(&temporary, content)
        .map_err(|error| AppError::ConfigWriteError(error.to_string()))?;

    if backup.exists() {
        let _ = fs::remove_file(&backup);
    }
    if path.exists() {
        fs::rename(path, &backup).map_err(|error| AppError::ConfigWriteError(error.to_string()))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(AppError::ConfigWriteError(error.to_string()));
    }
    // Keep the previous complete file as a last-known-good backup. Besides
    // surviving an interrupted replacement, this also recovers from a later
    // truncated or manually damaged primary JSON.
    Ok(())
}

fn valid_rect(rect: PhysicalRect) -> bool {
    rect.x > -1_000_000
        && rect.x < 1_000_000
        && rect.y > -1_000_000
        && rect.y < 1_000_000
        && rect.width > 0
        && rect.width < 100_000
        && rect.height > 0
        && rect.height < 100_000
}

fn monitor_snapshot(monitor: &Monitor) -> MonitorSnapshot {
    let position = monitor.position();
    let size = monitor.size();
    let work_area = monitor.work_area();
    MonitorSnapshot {
        name: monitor.name().cloned(),
        position: PhysicalPoint {
            x: position.x,
            y: position.y,
        },
        size: PhysicalSize {
            width: size.width,
            height: size.height,
        },
        work_area: PhysicalRect {
            x: work_area.position.x,
            y: work_area.position.y,
            width: work_area.size.width,
            height: work_area.size.height,
        },
        scale_factor: monitor.scale_factor(),
    }
}

fn window_rect(window: &WebviewWindow) -> Result<PhysicalRect, AppError> {
    let position = window
        .outer_position()
        .map_err(|error| AppError::Unknown(format!("读取窗口位置失败: {error}")))?;
    let size = window
        .outer_size()
        .map_err(|error| AppError::Unknown(format!("读取窗口尺寸失败: {error}")))?;
    Ok(PhysicalRect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

fn intersection_size(a: PhysicalRect, b: PhysicalRect) -> (u32, u32) {
    let left = i64::from(a.x).max(i64::from(b.x));
    let top = i64::from(a.y).max(i64::from(b.y));
    let right = (i64::from(a.x) + i64::from(a.width)).min(i64::from(b.x) + i64::from(b.width));
    let bottom = (i64::from(a.y) + i64::from(a.height)).min(i64::from(b.y) + i64::from(b.height));
    (
        right.saturating_sub(left).max(0) as u32,
        bottom.saturating_sub(top).max(0) as u32,
    )
}

fn visible_threshold(rect: PhysicalRect) -> (u32, u32) {
    (rect.width.min(64), rect.height.min(32))
}

fn is_recoverable(rect: PhysicalRect, work_area: PhysicalRect) -> bool {
    if !valid_rect(rect) {
        return false;
    }
    let (width, height) = intersection_size(rect, work_area);
    let (minimum_width, minimum_height) = visible_threshold(rect);
    width >= minimum_width && height >= minimum_height
}

fn intersection_area(a: PhysicalRect, b: PhysicalRect) -> u64 {
    let (width, height) = intersection_size(a, b);
    u64::from(width) * u64::from(height)
}

fn best_monitor_for_rect(rect: PhysicalRect, monitors: &[MonitorSnapshot]) -> Option<usize> {
    monitors
        .iter()
        .enumerate()
        .max_by_key(|(_, monitor)| intersection_area(rect, monitor.work_area))
        .and_then(|(index, monitor)| {
            (intersection_area(rect, monitor.work_area) > 0).then_some(index)
        })
}

fn monitor_match_score(saved: &MonitorSnapshot, current: &MonitorSnapshot) -> i32 {
    let mut score = 0;
    if saved.name.is_some() && saved.name == current.name {
        score += 100;
    }
    if saved.position == current.position {
        score += 35;
    }
    if saved.size == current.size {
        score += 20;
    }
    if saved.work_area.width == current.work_area.width
        && saved.work_area.height == current.work_area.height
    {
        score += 20;
    }
    if (saved.scale_factor - current.scale_factor).abs() < 0.01 {
        score += 10;
    }
    score
}

fn matching_monitor(saved: &MonitorSnapshot, monitors: &[MonitorSnapshot]) -> Option<usize> {
    monitors
        .iter()
        .enumerate()
        .map(|(index, monitor)| (index, monitor_match_score(saved, monitor)))
        .max_by_key(|(_, score)| *score)
        .and_then(|(index, score)| (score >= 70).then_some(index))
}

fn clamp_axis(position: i32, work_start: i32, work_length: u32, window_length: u32) -> i32 {
    let maximum = i64::from(work_start) + i64::from(work_length.saturating_sub(window_length));
    i64::from(position).clamp(i64::from(work_start), maximum.max(i64::from(work_start))) as i32
}

fn clamp_fully_visible(rect: PhysicalRect, work_area: PhysicalRect) -> PhysicalRect {
    PhysicalRect {
        x: clamp_axis(rect.x, work_area.x, work_area.width, rect.width),
        y: clamp_axis(rect.y, work_area.y, work_area.height, rect.height),
        ..rect
    }
}

fn calculate_anchor(rect: PhysicalRect, work_area: PhysicalRect) -> (f64, f64) {
    let range_x = work_area.width.saturating_sub(rect.width);
    let range_y = work_area.height.saturating_sub(rect.height);
    let anchor_x = if range_x == 0 {
        0.0
    } else {
        (f64::from(rect.x - work_area.x) / f64::from(range_x)).clamp(0.0, 1.0)
    };
    let anchor_y = if range_y == 0 {
        0.0
    } else {
        (f64::from(rect.y - work_area.y) / f64::from(range_y)).clamp(0.0, 1.0)
    };
    (anchor_x, anchor_y)
}

fn position_from_anchor(
    size: PhysicalSize,
    work_area: PhysicalRect,
    anchor_x: f64,
    anchor_y: f64,
    dock_edge: Option<DockEdge>,
) -> PhysicalRect {
    let range_x = work_area.width.saturating_sub(size.width);
    let range_y = work_area.height.saturating_sub(size.height);
    let mut rect = PhysicalRect {
        x: work_area.x + (f64::from(range_x) * anchor_x.clamp(0.0, 1.0)).round() as i32,
        y: work_area.y + (f64::from(range_y) * anchor_y.clamp(0.0, 1.0)).round() as i32,
        width: size.width,
        height: size.height,
    };
    match dock_edge {
        Some(DockEdge::Left) => rect.x = work_area.x,
        Some(DockEdge::Right) => {
            rect.x = work_area.x + work_area.width.saturating_sub(rect.width) as i32
        }
        Some(DockEdge::Top) => rect.y = work_area.y,
        Some(DockEdge::Bottom) => {
            rect.y = work_area.y + work_area.height.saturating_sub(rect.height) as i32
        }
        None => {}
    }
    clamp_fully_visible(rect, work_area)
}

fn default_anchor(label: &str) -> (f64, f64) {
    match label {
        "overlay" => (0.04, 0.05),
        "stats-overlay" => (0.24, 0.05),
        "bongo-cat" => (0.96, 0.95),
        _ => (0.5, 0.5),
    }
}

fn default_rect(label: &str, size: PhysicalSize, monitor: &MonitorSnapshot) -> PhysicalRect {
    let (anchor_x, anchor_y) = default_anchor(label);
    position_from_anchor(size, monitor.work_area, anchor_x, anchor_y, None)
}

fn resolve_saved_placement(
    label: &str,
    saved: &SavedPlacement,
    monitors: &[MonitorSnapshot],
    current_size: PhysicalSize,
    fallback_monitor_index: usize,
) -> ResolvedPlacement {
    if let Some(index) = matching_monitor(&saved.preferred_monitor, monitors) {
        let monitor = &monitors[index];
        let exact = PhysicalRect {
            width: current_size.width,
            height: current_size.height,
            ..saved.preferred_rect
        };
        let work_area_unchanged = saved.preferred_monitor.work_area == monitor.work_area;
        let size_unchanged = saved.preferred_rect.width == current_size.width
            && saved.preferred_rect.height == current_size.height;
        let rect = if work_area_unchanged
            && size_unchanged
            && saved.dock_edge.is_none()
            && is_recoverable(exact, monitor.work_area)
        {
            exact
        } else {
            position_from_anchor(
                current_size,
                monitor.work_area,
                saved.anchor_x,
                saved.anchor_y,
                saved.dock_edge,
            )
        };
        return ResolvedPlacement {
            rect,
            monitor_index: index,
            recovered: !work_area_unchanged
                || !size_unchanged
                || rect.x != exact.x
                || rect.y != exact.y,
            used_fallback: false,
        };
    }

    let exact = PhysicalRect {
        width: current_size.width,
        height: current_size.height,
        ..saved.preferred_rect
    };
    if let Some(index) = monitors
        .iter()
        .position(|monitor| is_recoverable(exact, monitor.work_area))
    {
        return ResolvedPlacement {
            rect: exact,
            monitor_index: index,
            recovered: false,
            used_fallback: false,
        };
    }

    if let Some(fallback) = saved.fallback_rect {
        let fallback = PhysicalRect {
            width: current_size.width,
            height: current_size.height,
            ..fallback
        };
        if let Some(index) = monitors
            .iter()
            .position(|monitor| is_recoverable(fallback, monitor.work_area))
        {
            return ResolvedPlacement {
                rect: clamp_fully_visible(fallback, monitors[index].work_area),
                monitor_index: index,
                recovered: true,
                used_fallback: true,
            };
        }
    }

    let monitor_index = fallback_monitor_index.min(monitors.len().saturating_sub(1));
    ResolvedPlacement {
        rect: default_rect(label, current_size, &monitors[monitor_index]),
        monitor_index,
        recovered: true,
        used_fallback: true,
    }
}

fn capture_preferred(
    rect: PhysicalRect,
    monitor: &MonitorSnapshot,
    dock_edge: Option<DockEdge>,
) -> SavedPlacement {
    let (anchor_x, anchor_y) = calculate_anchor(rect, monitor.work_area);
    SavedPlacement {
        version: PLACEMENT_VERSION,
        preferred_rect: rect,
        preferred_monitor: monitor.clone(),
        anchor_x,
        anchor_y,
        dock_edge,
        fallback_rect: None,
        saved_at: chrono::Utc::now().timestamp(),
    }
}

fn state_from_app(app: &AppHandle) -> Result<tauri::State<'_, SharedState>, AppError> {
    app.try_state::<SharedState>()
        .ok_or_else(|| AppError::Unknown("应用状态尚未就绪".to_string()))
}

fn monitors_for(window: &WebviewWindow) -> Result<Vec<MonitorSnapshot>, AppError> {
    let monitors = window
        .available_monitors()
        .map_err(|error| AppError::Unknown(format!("枚举显示器失败: {error}")))?
        .iter()
        .map(monitor_snapshot)
        .collect::<Vec<_>>();
    if monitors.is_empty() {
        Err(AppError::Unknown("系统未返回可用显示器".to_string()))
    } else {
        Ok(monitors)
    }
}

fn primary_monitor_index(window: &WebviewWindow, monitors: &[MonitorSnapshot]) -> usize {
    window
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor_snapshot(&monitor))
        .and_then(|primary| matching_monitor(&primary, monitors))
        .unwrap_or(0)
}

fn target_monitor_index(
    app: &AppHandle,
    window: &WebviewWindow,
    monitors: &[MonitorSnapshot],
    target: &str,
) -> usize {
    if target == "main" {
        if let Some(main) = app.get_webview_window("main") {
            if let Ok(Some(monitor)) = main.current_monitor() {
                if let Some(index) = matching_monitor(&monitor_snapshot(&monitor), monitors) {
                    return index;
                }
            }
        }
    }
    if target == "cursor" {
        if let Some(point) = cursor_position() {
            if let Ok(Some(monitor)) =
                window.monitor_from_point(f64::from(point.x), f64::from(point.y))
            {
                if let Some(index) = matching_monitor(&monitor_snapshot(&monitor), monitors) {
                    return index;
                }
            }
        }
    }
    primary_monitor_index(window, monitors)
}

#[cfg(target_os = "windows")]
fn cursor_position() -> Option<PhysicalPoint> {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    extern "system" {
        fn GetCursorPos(point: *mut Point) -> i32;
    }
    let mut point = Point { x: 0, y: 0 };
    let succeeded = unsafe { GetCursorPos(&mut point) } != 0;
    succeeded.then_some(PhysicalPoint {
        x: point.x,
        y: point.y,
    })
}

#[cfg(not(target_os = "windows"))]
fn cursor_position() -> Option<PhysicalPoint> {
    None
}

fn apply_position(window: &WebviewWindow, rect: PhysicalRect) -> Result<bool, AppError> {
    let current = window_rect(window)?;
    if current.x == rect.x && current.y == rect.y {
        return Ok(false);
    }
    window
        .set_position(PhysicalPosition::new(rect.x, rect.y))
        .map_err(|error| AppError::Unknown(format!("移动窗口失败: {error}")))?;
    Ok(true)
}

fn restore_impl(
    app: &AppHandle,
    label: &str,
    legacy_geometry: Option<WindowGeometry>,
    persist_default: bool,
) -> Result<PlacementOutcome, AppError> {
    validate_label(label)?;
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| AppError::Unknown(format!("悬浮窗尚未创建: {label}")))?;
    let monitors = monitors_for(&window)?;
    let fallback_index = primary_monitor_index(&window, &monitors);
    let state = state_from_app(app)?;
    let _io_guard = state.window_placement_io.lock();
    let path = placement_path(&state.app_data_dir, label);
    let saved = load_placement(&path);

    if let Some(saved) = saved {
        let current = window_rect(&window)?;
        let resolved = resolve_saved_placement(
            label,
            &saved,
            &monitors,
            PhysicalSize {
                width: current.width,
                height: current.height,
            },
            fallback_index,
        );
        let moved = apply_position(&window, resolved.rect)?;
        if resolved.used_fallback {
            let mut updated = saved;
            updated.fallback_rect = Some(resolved.rect);
            save_placement(&path, &updated)?;
        }
        return Ok(PlacementOutcome {
            label: label.to_string(),
            moved,
            recovered: resolved.recovered,
            used_fallback: resolved.used_fallback,
            monitor_name: monitors[resolved.monitor_index].name.clone(),
        });
    }

    if let Some(legacy) = legacy_geometry.as_ref().filter(|geometry| {
        geometry.x > -32000 && geometry.y > -32000 && geometry.width > 0 && geometry.height > 0
    }) {
        window
            .set_position(LogicalPosition::new(
                f64::from(legacy.x),
                f64::from(legacy.y),
            ))
            .map_err(|error| AppError::Unknown(format!("迁移旧窗口位置失败: {error}")))?;
    }

    let current = window_rect(&window)?;
    let selected = monitors
        .iter()
        .position(|monitor| is_recoverable(current, monitor.work_area));
    let monitor_index = selected.unwrap_or(fallback_index);
    let rect = if selected.is_some() {
        current
    } else {
        default_rect(
            label,
            PhysicalSize {
                width: current.width,
                height: current.height,
            },
            &monitors[monitor_index],
        )
    };
    let moved = apply_position(&window, rect)?;
    if legacy_geometry.is_some() || persist_default {
        let placement = capture_preferred(rect, &monitors[monitor_index], None);
        save_placement(&path, &placement)?;
    }
    Ok(PlacementOutcome {
        label: label.to_string(),
        moved,
        recovered: selected.is_none(),
        used_fallback: false,
        monitor_name: monitors[monitor_index].name.clone(),
    })
}

fn save_current_impl(
    app: &AppHandle,
    label: &str,
    position_override: Option<PhysicalPoint>,
    dock_edge: Option<DockEdge>,
    user_initiated: bool,
) -> Result<bool, AppError> {
    validate_label(label)?;
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| AppError::Unknown(format!("悬浮窗尚未创建: {label}")))?;
    if window.is_minimized().unwrap_or(false) {
        return Ok(false);
    }
    let monitors = monitors_for(&window)?;
    let mut rect = window_rect(&window)?;
    if let Some(position) = position_override {
        rect.x = position.x;
        rect.y = position.y;
    }
    if !valid_rect(rect) {
        return Ok(false);
    }
    let Some(monitor_index) = best_monitor_for_rect(rect, &monitors) else {
        return Ok(false);
    };
    if !is_recoverable(rect, monitors[monitor_index].work_area) {
        return Ok(false);
    }

    let state = state_from_app(app)?;
    let _io_guard = state.window_placement_io.lock();
    let path = placement_path(&state.app_data_dir, label);
    let existing = load_placement(&path);
    let same_preferred_monitor = existing
        .as_ref()
        .and_then(|placement| matching_monitor(&placement.preferred_monitor, &monitors))
        == Some(monitor_index);

    let updated = match existing {
        Some(mut placement) if !user_initiated && !same_preferred_monitor => {
            placement.fallback_rect = Some(rect);
            placement.saved_at = chrono::Utc::now().timestamp();
            placement
        }
        _ => capture_preferred(rect, &monitors[monitor_index], dock_edge),
    };
    save_placement(&path, &updated)?;
    Ok(true)
}

fn force_to_target_impl(
    app: &AppHandle,
    label: &str,
    target: &str,
) -> Result<PlacementOutcome, AppError> {
    validate_label(label)?;
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| AppError::Unknown(format!("悬浮窗尚未创建: {label}")))?;
    let monitors = monitors_for(&window)?;
    let monitor_index = target_monitor_index(app, &window, &monitors, target);
    let current = window_rect(&window)?;
    let rect = default_rect(
        label,
        PhysicalSize {
            width: current.width,
            height: current.height,
        },
        &monitors[monitor_index],
    );
    let moved = apply_position(&window, rect)?;
    let state = state_from_app(app)?;
    let _io_guard = state.window_placement_io.lock();
    let path = placement_path(&state.app_data_dir, label);
    save_placement(
        &path,
        &capture_preferred(rect, &monitors[monitor_index], None),
    )?;
    Ok(PlacementOutcome {
        label: label.to_string(),
        moved,
        recovered: true,
        used_fallback: false,
        monitor_name: monitors[monitor_index].name.clone(),
    })
}

pub fn set_auxiliary_window_visible_for_app(
    app: &AppHandle,
    label: &str,
    visible: bool,
    target: Option<&str>,
) -> Result<PlacementOutcome, AppError> {
    validate_label(label)?;
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| AppError::Unknown(format!("悬浮窗尚未创建: {label}")))?;
    if !visible {
        window
            .hide()
            .map_err(|error| AppError::Unknown(format!("隐藏悬浮窗失败: {error}")))?;
        if label == "bongo-cat" {
            crate::input_listener::set_bongo_cat_input_visible(false);
        }
        return Ok(PlacementOutcome {
            label: label.to_string(),
            moved: false,
            recovered: false,
            used_fallback: false,
            monitor_name: None,
        });
    }

    let outcome = match target.filter(|target| *target != "preserve") {
        Some(target) => force_to_target_impl(app, label, target)?,
        None => restore_impl(app, label, None, false)?,
    };
    let _ = window.unminimize();
    window
        .show()
        .map_err(|error| AppError::Unknown(format!("显示悬浮窗失败: {error}")))?;
    if label == "bongo-cat" {
        crate::input_listener::set_bongo_cat_input_visible(true);
    }
    Ok(outcome)
}

pub fn recover_auxiliary_windows_for_app(
    app: &AppHandle,
    target: &str,
) -> Result<Vec<String>, AppError> {
    let state = state_from_app(app)?;
    let config = state.configuration().snapshot();
    let enabled = |label: &str| match label {
        "overlay" => config
            .as_ref()
            .map(|config| config.enable_tz_overlay)
            .unwrap_or(true),
        "stats-overlay" => config
            .as_ref()
            .map(|config| config.enable_stats_overlay)
            .unwrap_or(true),
        "bongo-cat" => config
            .as_ref()
            .map(|config| config.enable_bongo_cat)
            .unwrap_or(false),
        _ => false,
    };
    let mut recovered = Vec::new();
    for label in AUXILIARY_LABELS {
        if !enabled(label) {
            continue;
        }
        match set_auxiliary_window_visible_for_app(app, label, true, Some(target)) {
            Ok(_) => recovered.push(label.to_string()),
            Err(error) => crate::logger::log_msg(
                "WARN",
                "WindowPlacement",
                &format!("找回悬浮窗 {label} 失败: {error}"),
            ),
        }
    }
    Ok(recovered)
}

pub fn ensure_main_window_visible(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Ok(monitors) = monitors_for(&window) {
        if let Ok(current) = window_rect(&window) {
            if !monitors
                .iter()
                .any(|monitor| is_recoverable(current, monitor.work_area))
            {
                let index = primary_monitor_index(&window, &monitors);
                let rect = default_rect(
                    "main",
                    PhysicalSize {
                        width: current.width,
                        height: current.height,
                    },
                    &monitors[index],
                );
                let _ = apply_position(&window, rect);
            }
        }
    }
}

pub fn show_main_window_safely(app: &AppHandle) {
    ensure_main_window_visible(app);
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub fn restore_window_placement(
    app: AppHandle,
    label: String,
    legacy_geometry: Option<WindowGeometry>,
) -> Result<PlacementOutcome, AppError> {
    restore_impl(&app, &label, legacy_geometry, true)
}

#[tauri::command]
pub fn save_window_placement(
    app: AppHandle,
    label: String,
    position_override: Option<PhysicalPoint>,
    dock_edge: Option<DockEdge>,
    user_initiated: Option<bool>,
) -> Result<bool, AppError> {
    save_current_impl(
        &app,
        &label,
        position_override,
        dock_edge,
        user_initiated.unwrap_or(false),
    )
}

#[tauri::command]
pub fn set_auxiliary_window_visible(
    app: AppHandle,
    label: String,
    visible: bool,
    target: Option<String>,
) -> Result<PlacementOutcome, AppError> {
    set_auxiliary_window_visible_for_app(&app, &label, visible, target.as_deref())
}

#[tauri::command]
pub fn recover_auxiliary_windows(
    app: AppHandle,
    target: Option<String>,
) -> Result<Vec<String>, AppError> {
    recover_auxiliary_windows_for_app(&app, target.as_deref().unwrap_or("main"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(
        name: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> MonitorSnapshot {
        MonitorSnapshot {
            name: Some(name.to_string()),
            position: PhysicalPoint { x, y },
            size: PhysicalSize { width, height },
            work_area: PhysicalRect {
                x,
                y,
                width,
                height: height.saturating_sub(40),
            },
            scale_factor,
        }
    }

    fn saved(rect: PhysicalRect, monitor: &MonitorSnapshot) -> SavedPlacement {
        capture_preferred(rect, monitor, None)
    }

    #[test]
    fn negative_coordinates_on_a_left_monitor_remain_valid() {
        let left = monitor("DISPLAY2", -1920, 0, 1920, 1080, 1.0);
        let primary = monitor("DISPLAY1", 0, 0, 2560, 1440, 1.5);
        let placement = saved(
            PhysicalRect {
                x: -1800,
                y: 80,
                width: 280,
                height: 250,
            },
            &left,
        );
        let resolved = resolve_saved_placement(
            "overlay",
            &placement,
            &[primary, left],
            PhysicalSize {
                width: 280,
                height: 250,
            },
            0,
        );
        assert_eq!(resolved.rect.x, -1800);
        assert!(!resolved.used_fallback);
    }

    #[test]
    fn missing_secondary_uses_primary_without_overwriting_the_preference() {
        let secondary = monitor("DISPLAY2", 2560, 0, 1920, 1080, 1.0);
        let primary = monitor("DISPLAY1", 0, 0, 2560, 1440, 1.5);
        let placement = saved(
            PhysicalRect {
                x: 2720,
                y: 90,
                width: 280,
                height: 250,
            },
            &secondary,
        );
        let resolved = resolve_saved_placement(
            "overlay",
            &placement,
            std::slice::from_ref(&primary),
            PhysicalSize {
                width: 280,
                height: 250,
            },
            0,
        );
        assert!(resolved.used_fallback);
        assert!(is_recoverable(resolved.rect, primary.work_area));
        assert_eq!(placement.preferred_rect.x, 2720);
    }

    #[test]
    fn returning_secondary_restores_the_preferred_position_after_a_fallback() {
        let secondary = monitor("DISPLAY2", 2560, 0, 1920, 1080, 1.0);
        let primary = monitor("DISPLAY1", 0, 0, 2560, 1440, 1.5);
        let mut placement = saved(
            PhysicalRect {
                x: 2720,
                y: 90,
                width: 280,
                height: 250,
            },
            &secondary,
        );
        placement.fallback_rect = Some(PhysicalRect {
            x: 80,
            y: 80,
            width: 280,
            height: 250,
        });
        let resolved = resolve_saved_placement(
            "overlay",
            &placement,
            &[primary, secondary],
            PhysicalSize {
                width: 280,
                height: 250,
            },
            0,
        );
        assert_eq!(resolved.rect.x, 2720);
        assert!(!resolved.used_fallback);
    }

    #[test]
    fn resolution_change_preserves_relative_anchor_and_keeps_the_window_visible() {
        let old = monitor("DISPLAY2", 1920, 0, 2560, 1440, 1.25);
        let current = monitor("DISPLAY2", 1920, 0, 1920, 1080, 1.25);
        let placement = saved(
            PhysicalRect {
                x: 4000,
                y: 1100,
                width: 320,
                height: 220,
            },
            &old,
        );
        let resolved = resolve_saved_placement(
            "overlay",
            &placement,
            std::slice::from_ref(&current),
            PhysicalSize {
                width: 320,
                height: 220,
            },
            0,
        );
        assert!(resolved.recovered);
        assert!(is_recoverable(resolved.rect, current.work_area));
        assert!(resolved.rect.x + resolved.rect.width as i32 <= 3840);
    }

    #[test]
    fn window_size_change_preserves_the_relative_anchor() {
        let display = monitor("DISPLAY1", 0, 0, 1920, 1080, 1.0);
        let placement = saved(
            PhysicalRect {
                x: 1580,
                y: 700,
                width: 280,
                height: 250,
            },
            &display,
        );
        let resolved = resolve_saved_placement(
            "bongo-cat",
            &placement,
            std::slice::from_ref(&display),
            PhysicalSize {
                width: 420,
                height: 360,
            },
            0,
        );
        assert!(resolved.recovered);
        assert!(is_recoverable(resolved.rect, display.work_area));
        assert!(resolved.rect.x + resolved.rect.width as i32 <= 1920);
        assert!(resolved.rect.y + resolved.rect.height as i32 <= 1040);
    }

    #[test]
    fn docking_is_remapped_to_the_same_edge_after_rotation() {
        let old = monitor("DISPLAY2", 1920, 0, 1920, 1080, 1.0);
        let current = monitor("DISPLAY2", 1920, 0, 1080, 1920, 1.0);
        let mut placement = saved(
            PhysicalRect {
                x: 3560,
                y: 300,
                width: 280,
                height: 250,
            },
            &old,
        );
        placement.dock_edge = Some(DockEdge::Right);
        let resolved = resolve_saved_placement(
            "overlay",
            &placement,
            std::slice::from_ref(&current),
            PhysicalSize {
                width: 280,
                height: 250,
            },
            0,
        );
        assert_eq!(
            resolved.rect.x,
            current.work_area.x + current.work_area.width as i32 - 280
        );
        assert!(is_recoverable(resolved.rect, current.work_area));
    }

    #[test]
    fn a_one_pixel_intersection_is_not_considered_recoverable() {
        let work = PhysicalRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        assert!(!is_recoverable(
            PhysicalRect {
                x: 1919,
                y: 200,
                width: 280,
                height: 250,
            },
            work,
        ));
    }

    #[test]
    fn corrupt_primary_file_can_fall_back_to_the_backup() {
        let root = std::env::temp_dir().join(format!(
            "d2rhub_window_placement_test_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("placement.json");
        let display = monitor("DISPLAY1", 0, 0, 1920, 1080, 1.0);
        let placement = saved(
            PhysicalRect {
                x: 80,
                y: 80,
                width: 280,
                height: 250,
            },
            &display,
        );
        fs::write(&path, "not-json").unwrap();
        fs::write(backup_path(&path), serde_json::to_vec(&placement).unwrap()).unwrap();
        assert!(load_placement(&path).is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn successful_replacement_keeps_the_previous_placement_as_backup() {
        let root = std::env::temp_dir().join(format!(
            "d2rhub_window_placement_backup_test_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let path = root.join("placement.json");
        let display = monitor("DISPLAY1", 0, 0, 1920, 1080, 1.0);
        let original = saved(
            PhysicalRect {
                x: 80,
                y: 80,
                width: 280,
                height: 250,
            },
            &display,
        );
        let replacement = saved(
            PhysicalRect {
                x: 400,
                y: 240,
                width: 280,
                height: 250,
            },
            &display,
        );

        save_placement(&path, &original).unwrap();
        save_placement(&path, &replacement).unwrap();

        let backup: SavedPlacement =
            serde_json::from_slice(&fs::read(backup_path(&path)).unwrap()).unwrap();
        assert_eq!(backup.preferred_rect, original.preferred_rect);
        let _ = fs::remove_dir_all(root);
    }
}
