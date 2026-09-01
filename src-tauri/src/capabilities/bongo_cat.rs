use crate::application::capability::{CapabilityDriver, CapabilityFailure, CapabilityHealth};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

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

pub(crate) struct BongoCatCapability {
    app: AppHandle,
    window: Mutex<Option<BongoCatWindow>>,
    worker: Mutex<Option<BongoCatWorker>>,
}

#[derive(Clone)]
struct BongoCatWindow {
    window: WebviewWindow,
    geometry: GeometryCache,
}

struct BongoCatWorker {
    stop: std::sync::mpsc::SyncSender<()>,
    handle: std::thread::JoinHandle<()>,
}

impl BongoCatCapability {
    pub(crate) fn install(app: &tauri::App) -> Result<Arc<Self>, CapabilityFailure> {
        Ok(Arc::new(Self {
            app: app.handle().clone(),
            window: Mutex::new(None),
            worker: Mutex::new(None),
        }))
    }

    fn ensure_window(&self) -> Result<BongoCatWindow, CapabilityFailure> {
        let mut state = self.window.lock().map_err(|_| {
            CapabilityFailure::new(
                "window-state-poisoned",
                "desktop pet window state is poisoned",
            )
        })?;
        if self.app.get_webview_window(WINDOW_LABEL).is_some() {
            if let Some(window) = state.as_ref() {
                return Ok(window.clone());
            }
        } else {
            // A native close outside the normal hide-only lifecycle invalidates
            // the cached handle. Rebuild and attach fresh geometry listeners.
            *state = None;
        }

        let window = crate::auxiliary_windows::ensure_window(&self.app, WINDOW_LABEL)
            .map_err(|error| CapabilityFailure::new("window-create-failed", error.to_string()))?;
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

        let window = BongoCatWindow { window, geometry };
        *state = Some(window.clone());
        Ok(window)
    }

    fn run_cursor_worker(
        window: WebviewWindow,
        geometry: GeometryCache,
        stop: std::sync::mpsc::Receiver<()>,
    ) {
        let mut is_ignoring_cursor_events = false;

        loop {
            match stop.recv_timeout(POLL_INTERVAL) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }

            if !window.is_visible().unwrap_or(false) {
                continue;
            }

            let geometry = geometry
                .lock()
                .map(|geometry| *geometry)
                .unwrap_or_else(|_| WindowGeometry::unavailable());
            let Some(cursor) = cursor_position() else {
                continue;
            };
            let Some((normalized_x, normalized_y)) = geometry.normalized_cursor_position(cursor)
            else {
                continue;
            };

            let cursor_is_over_cat = is_cat_hit(normalized_x, normalized_y);
            if cursor_is_over_cat && is_ignoring_cursor_events {
                let _ = window.set_ignore_cursor_events(false);
                is_ignoring_cursor_events = false;
            } else if !cursor_is_over_cat && !is_ignoring_cursor_events {
                let _ = window.set_ignore_cursor_events(true);
                is_ignoring_cursor_events = true;
            }
        }

        // A stopped hidden window must never retain click-through state if the
        // user enables it again during the same process.
        let _ = window.set_ignore_cursor_events(false);
    }
}

impl CapabilityDriver for BongoCatCapability {
    fn start(&self) -> Result<(), CapabilityFailure> {
        let mut worker = self.worker.lock().map_err(|_| {
            CapabilityFailure::new(
                "worker-state-poisoned",
                "desktop pet worker lock is poisoned",
            )
        })?;
        if worker
            .as_ref()
            .is_some_and(|worker| !worker.handle.is_finished())
        {
            return Ok(());
        }
        if let Some(finished) = worker.take() {
            let _ = finished.handle.join();
        }

        let window = self.ensure_window()?;
        crate::window_placement::set_auxiliary_window_visible_for_app(
            &self.app,
            WINDOW_LABEL,
            true,
            None,
        )
        .map_err(|error| CapabilityFailure::new("window-show-failed", error.to_string()))?;
        crate::input_listener::set_bongo_cat_input_enabled(true);

        let (stop, stop_rx) = std::sync::mpsc::sync_channel(1);
        let app = self.app.clone();
        let cursor_window = window.window.clone();
        let geometry = Arc::clone(&window.geometry);
        let handle = std::thread::Builder::new()
            .name("desktop-pet-cursor-policy".to_string())
            .spawn(move || Self::run_cursor_worker(cursor_window, geometry, stop_rx))
            .map_err(|error| {
                crate::input_listener::set_bongo_cat_input_enabled(false);
                let _ = crate::window_placement::set_auxiliary_window_visible_for_app(
                    &app,
                    WINDOW_LABEL,
                    false,
                    None,
                );
                CapabilityFailure::new("worker-start-failed", error.to_string())
            })?;
        *worker = Some(BongoCatWorker { stop, handle });
        Ok(())
    }

    fn stop(&self) -> Result<(), CapabilityFailure> {
        crate::input_listener::set_bongo_cat_input_enabled(false);
        let worker = self
            .worker
            .lock()
            .map_err(|_| {
                CapabilityFailure::new(
                    "worker-state-poisoned",
                    "desktop pet worker lock is poisoned",
                )
            })?
            .take();
        let worker_result = if let Some(worker) = worker {
            let _ = worker.stop.try_send(());
            worker.handle.join().map_err(|_| {
                CapabilityFailure::new("worker-panicked", "desktop pet worker panicked")
            })
        } else {
            Ok(())
        };
        if let Ok(mut window) = self.window.lock() {
            *window = None;
        }
        let window_result = crate::auxiliary_windows::destroy_window(&self.app, WINDOW_LABEL)
            .map(|_| ())
            .map_err(|error| CapabilityFailure::new("window-destroy-failed", error.to_string()));

        // Always attempt both cleanup paths. A panicked cursor worker must not
        // leave a hidden renderer alive, and a window failure must not skip join.
        worker_result?;
        window_result?;
        Ok(())
    }

    fn health(&self) -> CapabilityHealth {
        match self.worker.lock() {
            Ok(worker)
                if worker
                    .as_ref()
                    .is_some_and(|worker| !worker.handle.is_finished()) =>
            {
                let Some(window) = self.app.get_webview_window(WINDOW_LABEL) else {
                    return CapabilityHealth::Degraded(CapabilityFailure::new(
                        "window-unavailable",
                        "desktop pet window is unavailable",
                    ));
                };
                match window.is_visible() {
                    Ok(true) => CapabilityHealth::Healthy,
                    Ok(false) => CapabilityHealth::Degraded(CapabilityFailure::new(
                        "window-hidden",
                        "desktop pet window is hidden",
                    )),
                    Err(error) => CapabilityHealth::Degraded(CapabilityFailure::new(
                        "window-status-unavailable",
                        error.to_string(),
                    )),
                }
            }
            Ok(_) => CapabilityHealth::Failed(CapabilityFailure::new(
                "worker-stopped",
                "desktop pet worker is not running",
            )),
            Err(_) => CapabilityHealth::Failed(CapabilityFailure::new(
                "worker-state-poisoned",
                "desktop pet worker lock is poisoned",
            )),
        }
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
