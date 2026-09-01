use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

const WINDOW_LABEL: &str = "bongo-cat";
const ORIGINAL_WINDOW_WIDTH: f64 = 240.0;
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy)]
struct WindowGeometry {
    position: Option<PhysicalPosition<i32>>,
    size: Option<PhysicalSize<u32>>,
    scale_factor: f64,
}

impl WindowGeometry {
    fn capture(window: &WebviewWindow) -> Self {
        Self {
            position: window.outer_position().ok(),
            size: window.outer_size().ok(),
            scale_factor: window.scale_factor().unwrap_or(1.0),
        }
    }

    fn unavailable() -> Self {
        Self {
            position: None,
            size: None,
            scale_factor: 1.0,
        }
    }

    fn normalized_cursor_position(&self, cursor: PhysicalPosition<i32>) -> Option<(f64, f64)> {
        let window_position = self.position?;
        let window_size = self.size?;

        // Preserve the existing 240x400 logical coordinate system so the hit
        // regions remain stable across window scale and monitor DPI changes.
        let logical_width = window_size.width as f64 / self.scale_factor;
        let window_scale = logical_width / ORIGINAL_WINDOW_WIDTH;
        let logical_x = (cursor.x as f64 - window_position.x as f64) / self.scale_factor;
        let logical_y = (cursor.y as f64 - window_position.y as f64) / self.scale_factor;

        Some((logical_x / window_scale, logical_y / window_scale))
    }
}

type GeometryCache = Arc<Mutex<WindowGeometry>>;

pub(crate) struct InstalledBongoCat {
    window: WebviewWindow,
    geometry: GeometryCache,
}

impl InstalledBongoCat {
    pub(crate) fn install(app: &tauri::App) -> Option<Self> {
        let window = app.get_webview_window(WINDOW_LABEL)?;
        let geometry = Arc::new(Mutex::new(WindowGeometry::capture(&window)));

        let geometry_for_events = Arc::clone(&geometry);
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::Moved(position) => {
                if let Ok(mut geometry) = geometry_for_events.lock() {
                    geometry.position = Some(*position);
                }
            }
            tauri::WindowEvent::Resized(size) => {
                if let Ok(mut geometry) = geometry_for_events.lock() {
                    geometry.size = Some(*size);
                }
            }
            tauri::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Ok(mut geometry) = geometry_for_events.lock() {
                    geometry.scale_factor = *scale_factor;
                }
            }
            _ => {}
        });

        Some(Self { window, geometry })
    }

    pub(crate) fn start(self) {
        std::thread::spawn(move || {
            let mut is_ignoring_cursor_events = false;

            loop {
                std::thread::sleep(POLL_INTERVAL);

                if !self.window.is_visible().unwrap_or(false) {
                    continue;
                }

                let geometry = self
                    .geometry
                    .lock()
                    .map(|geometry| *geometry)
                    .unwrap_or_else(|_| WindowGeometry::unavailable());
                let Some(cursor) = cursor_position() else {
                    continue;
                };
                let Some((normalized_x, normalized_y)) =
                    geometry.normalized_cursor_position(cursor)
                else {
                    continue;
                };

                let cursor_is_over_cat = is_cat_hit(normalized_x, normalized_y);
                if cursor_is_over_cat && is_ignoring_cursor_events {
                    let _ = self.window.set_ignore_cursor_events(false);
                    is_ignoring_cursor_events = false;
                } else if !cursor_is_over_cat && !is_ignoring_cursor_events {
                    let _ = self.window.set_ignore_cursor_events(true);
                    is_ignoring_cursor_events = true;
                }
            }
        });
    }
}

#[derive(Debug, Clone, Copy)]
struct HitBox {
    y_min: f64,
    y_max: f64,
    x_min: f64,
    x_max: f64,
}

const CAT_HIT_BOXES: [HitBox; 2] = [
    HitBox {
        y_min: 280.0,
        y_max: 330.0,
        x_min: 60.0,
        x_max: 195.0,
    },
    HitBox {
        y_min: 330.0,
        y_max: 400.0,
        x_min: 35.0,
        x_max: 205.0,
    },
];

fn is_cat_hit(x: f64, y: f64) -> bool {
    CAT_HIT_BOXES.iter().any(|hit_box| {
        y >= hit_box.y_min && y < hit_box.y_max && x >= hit_box.x_min && x <= hit_box.x_max
    })
}

fn cursor_position() -> Option<PhysicalPosition<i32>> {
    #[repr(C)]
    struct PointWin32 {
        x: i32,
        y: i32,
    }

    extern "system" {
        fn GetCursorPos(point: *mut PointWin32) -> i32;
    }

    let mut point = PointWin32 { x: 0, y: 0 };
    let succeeded = unsafe { GetCursorPos(&mut point) } != 0;
    succeeded.then_some(PhysicalPosition::new(point.x, point.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_regions_preserve_the_original_boundaries() {
        assert!(is_cat_hit(60.0, 280.0));
        assert!(is_cat_hit(195.0, 329.999));
        assert!(!is_cat_hit(59.999, 300.0));
        assert!(!is_cat_hit(196.0, 300.0));

        assert!(is_cat_hit(35.0, 330.0));
        assert!(is_cat_hit(205.0, 399.999));
        assert!(!is_cat_hit(34.999, 350.0));
        assert!(!is_cat_hit(206.0, 350.0));
        assert!(!is_cat_hit(120.0, 400.0));
    }

    #[test]
    fn cursor_coordinates_are_normalized_for_dpi_and_window_scale() {
        let geometry = WindowGeometry {
            position: Some(PhysicalPosition::new(100, 200)),
            size: Some(PhysicalSize::new(480, 800)),
            scale_factor: 2.0,
        };

        let normalized = geometry
            .normalized_cursor_position(PhysicalPosition::new(340, 800))
            .expect("geometry is available");

        assert_eq!(normalized, (120.0, 300.0));
        assert!(is_cat_hit(normalized.0, normalized.1));
    }

    #[test]
    fn unavailable_geometry_skips_hit_testing() {
        assert!(WindowGeometry::unavailable()
            .normalized_cursor_position(PhysicalPosition::new(0, 0))
            .is_none());
    }
}
