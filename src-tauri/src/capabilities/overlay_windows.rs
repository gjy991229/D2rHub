use crate::application::capability::{CapabilityDriver, CapabilityFailure, CapabilityHealth};
use crate::{commands, domain::config::GlobalConfig, window_placement};
use std::sync::Arc;
use tauri::Manager;

const MAIN_WINDOW_LABEL: &str = "main";
const TERROR_ZONE_OVERLAY_LABEL: &str = "overlay";
const STATS_OVERLAY_LABEL: &str = "stats-overlay";
const PRESERVE_PLACEMENT_MODE: &str = "preserve";

pub(crate) struct OverlayWindowCapability {
    app: tauri::AppHandle,
    label: &'static str,
}

impl OverlayWindowCapability {
    pub(crate) fn install(app: &tauri::App, label: &'static str) -> Arc<Self> {
        Arc::new(Self {
            app: app.handle().clone(),
            label,
        })
    }
}

impl CapabilityDriver for OverlayWindowCapability {
    fn start(&self) -> Result<(), CapabilityFailure> {
        crate::logger::log_msg(
            "INFO",
            "AuxiliaryWindow",
            &format!("开始启动可选窗口 {}", self.label),
        );
        crate::auxiliary_windows::ensure_window(&self.app, self.label)
            .map_err(|error| CapabilityFailure::new("window-create-failed", error.to_string()))?;
        window_placement::set_auxiliary_window_visible_for_app(
            &self.app,
            self.label,
            true,
            Some(PRESERVE_PLACEMENT_MODE),
        )
        .map(|_| {
            crate::logger::log_msg(
                "INFO",
                "AuxiliaryWindow",
                &format!("可选窗口 {} 已显示", self.label),
            );
        })
        .map_err(|error| CapabilityFailure::new("window-show-failed", error.to_string()))
    }

    fn stop(&self) -> Result<(), CapabilityFailure> {
        crate::auxiliary_windows::destroy_window(&self.app, self.label)
            .map(|destroyed| {
                crate::logger::log_msg(
                    "INFO",
                    "AuxiliaryWindow",
                    &format!(
                        "可选窗口 {} 已{}",
                        self.label,
                        if destroyed {
                            "销毁"
                        } else {
                            "处于关闭状态"
                        }
                    ),
                );
            })
            .map_err(|error| CapabilityFailure::new("window-destroy-failed", error.to_string()))
    }

    fn health(&self) -> CapabilityHealth {
        let Some(window) = self.app.get_webview_window(self.label) else {
            return CapabilityHealth::Failed(CapabilityFailure::new(
                "window-unavailable",
                "overlay window is unavailable",
            ));
        };
        match window.is_visible() {
            Ok(true) => CapabilityHealth::Healthy,
            Ok(false) => CapabilityHealth::Degraded(CapabilityFailure::new(
                "window-hidden",
                "overlay window is hidden",
            )),
            Err(error) => CapabilityHealth::Failed(CapabilityFailure::new(
                "window-status-unavailable",
                error.to_string(),
            )),
        }
    }
}

fn show_overlay_without_blocking(app: tauri::AppHandle, label: &'static str) {
    let thread_name = format!("show-{label}");
    if let Err(error) = std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            if let Err(error) = window_placement::set_auxiliary_window_visible_for_app(
                &app,
                label,
                true,
                Some(PRESERVE_PLACEMENT_MODE),
            ) {
                crate::logger::log_msg(
                    "WARN",
                    "AuxiliaryWindow",
                    &format!("后台显示可选窗口 {label} 失败: {error}"),
                );
                crate::capabilities::schedule_reconcile(&app);
            }
        })
    {
        crate::logger::log_msg(
            "WARN",
            "AuxiliaryWindow",
            &format!("无法启动可选窗口 {label} 显示任务: {error}"),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OverlayVisibility {
    terror_zone: bool,
    stats: bool,
}

impl OverlayVisibility {
    fn from_config(config: Option<&GlobalConfig>) -> Self {
        Self {
            terror_zone: config
                .map(|config| {
                    config
                        .optional_module_runtime_allowed(crate::domain::config::OPTIONAL_MODULE_OVERLAYS)
                        && config.enable_tz_overlay
                })
                .unwrap_or(false),
            stats: config
                .map(|config| {
                    config
                        .optional_module_runtime_allowed(crate::domain::config::OPTIONAL_MODULE_OVERLAYS)
                        && config.optional_module_runtime_allowed(
                            crate::domain::config::OPTIONAL_MODULE_AUTOMATION,
                        )
                        && config.enable_stats_overlay
                })
                .unwrap_or(false),
        }
    }
}

pub(crate) fn install(app: &tauri::AppHandle) {
    let Some(main_window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    let main_window_for_events = main_window.clone();
    main_window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            crate::input_listener::cancel_shortcut_capture();
            let _ = main_window_for_events.hide();

            let config =
                commands::global_config::get_global_config_ext(main_window_for_events.app_handle());
            let visibility = OverlayVisibility::from_config(config.as_ref());

            if visibility.terror_zone {
                show_overlay_without_blocking(
                    main_window_for_events.app_handle().clone(),
                    TERROR_ZONE_OVERLAY_LABEL,
                );
            }
            if visibility.stats {
                show_overlay_without_blocking(
                    main_window_for_events.app_handle().clone(),
                    STATS_OVERLAY_LABEL,
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_keeps_both_overlays_hidden() {
        assert_eq!(
            OverlayVisibility::from_config(None),
            OverlayVisibility {
                terror_zone: false,
                stats: false,
            }
        );
    }

    #[test]
    fn overlay_switches_are_evaluated_independently() {
        let mut config = GlobalConfig {
            enable_tz_overlay: false,
            enable_stats_overlay: true,
            installed_optional_modules: vec![
                crate::domain::config::OPTIONAL_MODULE_OVERLAYS.to_string(),
                crate::domain::config::OPTIONAL_MODULE_AUTOMATION.to_string(),
            ],
            ..GlobalConfig::default()
        };

        assert_eq!(
            OverlayVisibility::from_config(Some(&config)),
            OverlayVisibility {
                terror_zone: false,
                stats: true,
            }
        );

        config.enable_tz_overlay = true;
        config.enable_stats_overlay = false;
        assert_eq!(
            OverlayVisibility::from_config(Some(&config)),
            OverlayVisibility {
                terror_zone: true,
                stats: false,
            }
        );
    }
}
