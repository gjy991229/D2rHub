use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

pub const MIN_WINDOW_WIDTH: u32 = 800;
pub const MIN_WINDOW_HEIGHT: u32 = 600;
pub const MAX_LAYOUT_WINDOWS: usize = 32;
pub const MIN_VISIBLE_PIXELS: i32 = 64;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameInsets {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowFrameMetrics {
    pub dpi: u32,
    pub style: u32,
    pub ex_style: u32,
    /// Client area to DWM visible frame; includes the caption, never shadows.
    pub visible: FrameInsets,
    /// DWM visible frame to native window rect; used only for Win32 placement.
    pub invisible: FrameInsets,
}

impl WindowFrameMetrics {
    pub fn valid(&self) -> bool {
        (48..=768).contains(&self.dpi)
            && [self.visible, self.invisible].iter().all(|edge| {
                [edge.left, edge.top, edge.right, edge.bottom]
                    .iter()
                    .all(|value| *value <= 256)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowFrameProfile {
    pub monitor_id: String,
    pub scale_factor: f64,
    pub metrics: WindowFrameMetrics,
}

/// Physical desktop coordinates. Launch targets use the visible frame origin;
/// width/height are client/game resolution values, not window-frame dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutMonitor {
    pub id: String,
    pub name: String,
    pub bounds: LayoutRect,
    pub work_area: LayoutRect,
    pub primary: bool,
    pub scale_factor: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<WindowFrameMetrics>,
}

impl LayoutMonitor {
    pub fn visible_size(&self, width: u32, height: u32) -> (u32, u32) {
        let edge = self.frame.map(|frame| frame.visible).unwrap_or_default();
        (
            width + edge.left + edge.right,
            height + edge.top + edge.bottom,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutSlot {
    pub monitor_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fullscreen: bool,
    /// Visible frame origin relative to this monitor's work area, in physical pixels.
    pub x: i32,
    pub y: i32,
    /// Game resolution, with the same meaning and limits as account settings.
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutTarget {
    pub rect: LayoutRect,
    pub fullscreen: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowLayout {
    pub id: String,
    pub name: String,
    pub monitors: Vec<LayoutMonitor>,
    /// Array order controls presentation and stacking. Independent slots match titles; scheme slots bind account IDs.
    pub windows: Vec<LayoutSlot>,
}

impl WindowLayout {
    pub fn slot_index_for_title(&self, title: &str) -> Option<usize> {
        let key = window_title_key(title);
        if key.is_empty() {
            return None;
        }
        self.windows.iter().position(|slot| {
            slot.window_title
                .as_deref()
                .is_some_and(|name| window_title_key(name) == key)
        })
    }

    pub fn validate(&self) -> Result<(), AppError> {
        let invalid = |message: &str| AppError::ConfigWriteError(format!("窗口布局：{message}"));
        if self.id.trim().is_empty()
            || self.id.len() > 128
            || self.name.trim().is_empty()
            || self.name.chars().count() > 40
        {
            return Err(invalid("请输入有效的布局名称（最多 40 字）"));
        }
        if self.monitors.is_empty() || self.monitors.len() > 32 {
            return Err(invalid("显示器配置无效"));
        }
        let mut ids = HashSet::new();
        for monitor in &self.monitors {
            let work = monitor.work_area;
            let bounds = monitor.bounds;
            if monitor.id.is_empty()
                || !ids.insert(&monitor.id)
                || bounds.width == 0
                || bounds.height == 0
                || bounds.width > 32768
                || bounds.height > 32768
                || work.width == 0
                || work.height == 0
                || work.x < bounds.x
                || work.y < bounds.y
                || i64::from(work.x) + i64::from(work.width)
                    > i64::from(bounds.x) + i64::from(bounds.width)
                || i64::from(work.y) + i64::from(work.height)
                    > i64::from(bounds.y) + i64::from(bounds.height)
                || !monitor.scale_factor.is_finite()
                || !(0.5..=8.0).contains(&monitor.scale_factor)
                || monitor.frame.is_some_and(|frame| !frame.valid())
            {
                return Err(invalid("显示器尺寸、工作区或缩放比例无效"));
            }
        }
        if self
            .monitors
            .iter()
            .filter(|monitor| monitor.primary)
            .count()
            != 1
        {
            return Err(invalid("必须有一个主显示器"));
        }
        if self.windows.is_empty() || self.windows.len() > MAX_LAYOUT_WINDOWS {
            return Err(invalid("窗口数量须在 1 到 32 之间"));
        }
        let mut titles = HashSet::new();
        for slot in &self.windows {
            if let Some(title) = &slot.window_title {
                let key = window_title_key(title);
                if key.is_empty() || !titles.insert(key) {
                    return Err(invalid("窗口名称不能为空或重复"));
                }
            }
            let monitor = self
                .monitors
                .iter()
                .find(|monitor| monitor.id == slot.monitor_id)
                .ok_or_else(|| invalid("窗口引用了不存在的显示器"))?;
            if slot.fullscreen {
                continue;
            }
            if !(MIN_WINDOW_WIDTH..=7680).contains(&slot.width)
                || !(MIN_WINDOW_HEIGHT..=4320).contains(&slot.height)
            {
                return Err(invalid("游戏分辨率须在 800 × 600 到 7680 × 4320 之间"));
            }
            let (visible_width, visible_height) = monitor.visible_size(slot.width, slot.height);
            if slot.x < MIN_VISIBLE_PIXELS - visible_width as i32
                || slot.y < MIN_VISIBLE_PIXELS - visible_height as i32
                || i64::from(slot.x)
                    > i64::from(monitor.work_area.width.saturating_sub(visible_width))
                || i64::from(slot.y)
                    > i64::from(monitor.work_area.height.saturating_sub(visible_height))
            {
                return Err(invalid("窗口坐标超出可用范围，须保留至少 64 像素可见区域"));
            }
        }
        Ok(())
    }

    /// Preserve edge/center anchors when resolution changes. A disconnected
    /// monitor falls back to the primary. Game resolution remains unchanged.
    pub fn resolve(&self, monitors: &[LayoutMonitor]) -> Result<Vec<LayoutRect>, AppError> {
        self.validate()?;
        let primary = monitors
            .iter()
            .find(|monitor| monitor.primary)
            .or_else(|| monitors.first())
            .ok_or_else(|| AppError::FileError("未检测到显示器".into()))?;
        self.windows
            .iter()
            .map(|slot| {
                let saved = self
                    .monitors
                    .iter()
                    .find(|monitor| monitor.id == slot.monitor_id)
                    .expect("validated monitor");
                let current = monitors
                    .iter()
                    .find(|monitor| monitor.id == slot.monitor_id)
                    // Legacy layouts only stored DISPLAYn. Prefer their exact saved
                    // desktop location when the display source numbers were swapped.
                    .or_else(|| {
                        if saved.id.starts_with(r"\\.\DISPLAY") {
                            monitors
                                .iter()
                                .find(|monitor| monitor.bounds == saved.bounds)
                                .or_else(|| {
                                    monitors.iter().find(|monitor| monitor.name == saved.name)
                                })
                        } else {
                            None
                        }
                    })
                    .unwrap_or(primary);
                if slot.fullscreen {
                    return Ok(current.bounds);
                }
                let work = current.work_area;
                if work.width < MIN_WINDOW_WIDTH || work.height < MIN_WINDOW_HEIGHT {
                    return Err(AppError::FileError(format!(
                        "{} 的工作区小于 800 × 600，无法应用布局",
                        current.name
                    )));
                }
                // Monitor adaptation changes placement only. Never silently
                // replace the user's game resolution with a work-area size.
                let width = slot.width;
                let height = slot.height;
                let (saved_width, saved_height) = saved.visible_size(width, height);
                let (current_width, current_height) = current.visible_size(width, height);
                Ok(LayoutRect {
                    x: work.x.saturating_add(adapt_axis(
                        slot.x,
                        saved_width,
                        saved.work_area.width,
                        current_width,
                        work.width,
                    )),
                    y: work.y.saturating_add(adapt_axis(
                        slot.y,
                        saved_height,
                        saved.work_area.height,
                        current_height,
                        work.height,
                    )),
                    width,
                    height,
                })
            })
            .collect()
    }

    pub fn resolve_targets(
        &self,
        monitors: &[LayoutMonitor],
    ) -> Result<Vec<LayoutTarget>, AppError> {
        Ok(self
            .resolve(monitors)?
            .into_iter()
            .zip(&self.windows)
            .map(|(rect, slot)| LayoutTarget {
                rect,
                fullscreen: slot.fullscreen,
            })
            .collect())
    }
}

pub(crate) fn window_title_key(title: &str) -> String {
    title.trim().to_lowercase()
}

fn adapt_axis(offset: i32, size: u32, previous: u32, next_size: u32, next: u32) -> i32 {
    if offset < 0 {
        return offset.max(MIN_VISIBLE_PIXELS - next_size as i32);
    }
    if previous == next && size == next_size {
        return offset;
    }
    let old_span = previous.saturating_sub(size);
    let anchor = if old_span == 0 {
        0.5
    } else {
        f64::from(offset) / f64::from(old_span)
    };
    (anchor.clamp(0.0, 1.0) * f64::from(next.saturating_sub(next_size))).round() as i32
}

pub fn validate_layout_configuration(
    layouts: &[WindowLayout],
    selected: Option<&str>,
) -> Result<(), AppError> {
    if layouts.len() > 64 {
        return Err(AppError::ConfigWriteError("最多保存 64 个窗口布局".into()));
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for layout in layouts {
        layout.validate()?;
        if !ids.insert(&layout.id) || !names.insert(layout.name.trim().to_lowercase()) {
            return Err(AppError::ConfigWriteError(
                "窗口布局的名称和 ID 不能重复".into(),
            ));
        }
    }
    if selected.is_some_and(|id| !layouts.iter().any(|layout| layout.id == id)) {
        return Err(AppError::ConfigWriteError(
            "所选窗口布局不存在，请重新选择".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: &str, x: i32, width: u32, height: u32, primary: bool) -> LayoutMonitor {
        LayoutMonitor {
            id: id.into(),
            name: id.into(),
            bounds: LayoutRect {
                x,
                y: 0,
                width,
                height,
            },
            work_area: LayoutRect {
                x,
                y: 0,
                width,
                height,
            },
            primary,
            scale_factor: 1.0,
            frame: None,
        }
    }

    fn layout() -> WindowLayout {
        WindowLayout {
            id: "desk".into(),
            name: "桌面".into(),
            monitors: vec![
                monitor("main", 0, 1920, 1040, true),
                monitor("left", -2560, 2560, 1400, false),
            ],
            windows: vec![LayoutSlot {
                monitor_id: "left".into(),
                account_id: None,
                window_title: None,
                fullscreen: false,
                x: 640,
                y: 340,
                width: 1280,
                height: 720,
            }],
        }
    }

    #[test]
    fn fullscreen_uses_display_bounds_instead_of_work_area_or_window_frame() {
        let mut value = layout();
        value.windows[0].fullscreen = true;
        let mut monitors = value.monitors.clone();
        monitors[1].bounds.height = 1440;
        monitors[1].work_area.y = 40;

        assert_eq!(
            value.resolve_targets(&monitors).unwrap(),
            vec![LayoutTarget {
                rect: monitors[1].bounds,
                fullscreen: true,
            }]
        );
        assert_eq!(
            value.resolve(&monitors[..1]).unwrap(),
            vec![monitors[0].bounds]
        );
    }

    #[test]
    fn legacy_source_names_migrate_by_location_and_stable_ids_survive_renumbering() {
        let mut value = layout();
        value.monitors[1].id = r"\\.\DISPLAY1".into();
        value.monitors[1].name = "DISPLAY1".into();
        value.windows[0].monitor_id = value.monitors[1].id.clone();
        let mut current = value.monitors.clone();
        current[1].id = "monitor-interface-left".into();
        current[1].name = "DISPLAY2".into();
        current[0].id = "monitor-interface-right".into();
        current[0].name = "DISPLAY1".into();
        assert_eq!(value.resolve(&current).unwrap()[0].x, -1920);
        value.monitors = current.clone();
        value.windows[0].monitor_id = "monitor-interface-left".into();
        current.reverse();
        current[0].name = "DISPLAY3".into();
        assert_eq!(value.resolve(&current).unwrap()[0].x, -1920);
    }

    #[test]
    fn negative_desktop_origins_and_overlap_are_valid() {
        let mut value = layout();
        value.windows.push(value.windows[0].clone());
        let rectangles = value.resolve(&value.monitors).unwrap();
        assert_eq!(rectangles[0].x, -1920);
        assert_eq!(rectangles[0], rectangles[1]);
    }

    #[test]
    fn missing_monitor_and_smaller_work_area_preserve_game_resolution() {
        let rectangles = layout()
            .resolve(&[monitor("main", 20, 1024, 768, true)])
            .unwrap();
        assert_eq!(
            rectangles[0],
            LayoutRect {
                x: 20,
                y: 24,
                width: 1280,
                height: 720
            }
        );
    }

    #[test]
    fn negative_border_offsets_survive_resolve_and_monitor_changes() {
        let mut value = layout();
        value.windows[0].x = -8;
        value.windows[0].y = -12;
        let rect = value.resolve(&value.monitors).unwrap()[0];
        assert_eq!((rect.x, rect.y), (-2568, -12));
        let rect = value
            .resolve(&[monitor("main", 0, 1920, 1040, true)])
            .unwrap()[0];
        assert_eq!((rect.x, rect.y), (-8, -12));
        value.windows[0].x = -(value.windows[0].width as i32);
        assert!(value.validate().is_err());
    }

    #[test]
    fn calibration_preserves_visible_edge_anchors_and_content_resolution() {
        let mut value = layout();
        let frame = WindowFrameMetrics {
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
        };
        value.monitors[1].frame = Some(frame);
        value.windows[0].x = 2560 - 1282;
        value.windows[0].y = 1400 - 752;
        let mut current = value.monitors.clone();
        current[1].frame = Some(WindowFrameMetrics {
            visible: FrameInsets {
                left: 2,
                top: 46,
                right: 2,
                bottom: 2,
            },
            ..frame
        });
        let resolved = value.resolve(&current).unwrap()[0];
        assert_eq!(
            (resolved.x, resolved.y, resolved.width, resolved.height),
            (-1284, 632, 1280, 720)
        );
        value.windows[0].x = 0;
        value.windows[0].y = 0;
        let resolved = value.resolve(&current).unwrap()[0];
        assert_eq!((resolved.x, resolved.y), (-2560, 0));
    }

    #[test]
    fn resolution_changes_preserve_center_and_right_edge() {
        let mut value = layout();
        value.windows.push(LayoutSlot {
            monitor_id: "left".into(),
            account_id: None,
            window_title: None,
            fullscreen: false,
            x: 1280,
            y: 680,
            width: 1280,
            height: 720,
        });
        let rectangles = value
            .resolve(&[monitor("left", -3840, 3840, 2120, true)])
            .unwrap();
        assert_eq!(rectangles[0].x, -2560);
        assert_eq!(rectangles[1].x, -1280);
        assert_eq!(rectangles[1].y, 1400);
    }

    #[test]
    fn invalid_sizes_offscreen_rectangles_and_missing_selection_are_rejected() {
        let mut value = layout();
        value.windows[0].width = 799;
        assert!(value.validate().is_err());
        value.windows[0].width = 1280;
        value.windows[0].x = 1281;
        assert!(value.validate().is_err());
        assert!(validate_layout_configuration(&[layout()], Some("missing")).is_err());
    }
}
