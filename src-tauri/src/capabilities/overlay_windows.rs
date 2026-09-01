use crate::{commands, domain::config::GlobalConfig, window_placement};
use tauri::Manager;

const MAIN_WINDOW_LABEL: &str = "main";
const TERROR_ZONE_OVERLAY_LABEL: &str = "overlay";
const STATS_OVERLAY_LABEL: &str = "stats-overlay";
const PRESERVE_PLACEMENT_MODE: &str = "preserve";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OverlayVisibility {
    terror_zone: bool,
    stats: bool,
}

impl OverlayVisibility {
    fn from_config(config: Option<&GlobalConfig>) -> Self {
        Self {
            terror_zone: config
                .map(|config| config.enable_tz_overlay)
                .unwrap_or(true),
            stats: config
                .map(|config| config.enable_stats_overlay)
                .unwrap_or(true),
        }
    }
}

pub(crate) fn install(app: &tauri::App) {
    let Some(main_window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    let main_window_for_events = main_window.clone();
    main_window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = main_window_for_events.hide();

            let config =
                commands::global_config::get_global_config_ext(main_window_for_events.app_handle());
            let visibility = OverlayVisibility::from_config(config.as_ref());

            if visibility.terror_zone {
                let _ = window_placement::set_auxiliary_window_visible_for_app(
                    main_window_for_events.app_handle(),
                    TERROR_ZONE_OVERLAY_LABEL,
                    true,
                    Some(PRESERVE_PLACEMENT_MODE),
                );
            }
            if visibility.stats {
                let _ = window_placement::set_auxiliary_window_visible_for_app(
                    main_window_for_events.app_handle(),
                    STATS_OVERLAY_LABEL,
                    true,
                    Some(PRESERVE_PLACEMENT_MODE),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_keeps_both_overlays_visible() {
        assert_eq!(
            OverlayVisibility::from_config(None),
            OverlayVisibility {
                terror_zone: true,
                stats: true,
            }
        );
    }

    #[test]
    fn overlay_switches_are_evaluated_independently() {
        let mut config = GlobalConfig {
            enable_tz_overlay: false,
            enable_stats_overlay: true,
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
