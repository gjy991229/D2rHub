#[cfg(test)]
use crate::domain::config::GlobalConfig;
use crate::error::AppError;
use parking_lot::Mutex;
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

pub(crate) const TERROR_ZONE_OVERLAY_LABEL: &str = "overlay";
pub(crate) const STATS_OVERLAY_LABEL: &str = "stats-overlay";
pub(crate) const DESKTOP_PET_LABEL: &str = "bongo-cat";
pub(crate) const AUXILIARY_WINDOW_LABELS: [&str; 3] = [
    TERROR_ZONE_OVERLAY_LABEL,
    STATS_OVERLAY_LABEL,
    DESKTOP_PET_LABEL,
];

/// Serializes the check-and-create sequence for auxiliary WebViews. Tauri
/// commands and capability reconciliation can request the same window at the
/// same time, but a label must still be instantiated at most once.
#[derive(Default)]
pub(crate) struct AuxiliaryWindowLifecycle {
    create_lock: Mutex<()>,
}

fn validate_label(label: &str) -> Result<(), AppError> {
    if AUXILIARY_WINDOW_LABELS.contains(&label) {
        Ok(())
    } else {
        Err(AppError::Unknown(format!("不支持的辅助窗口标签: {label}")))
    }
}

#[cfg(test)]
fn startup_window_labels(config: &GlobalConfig) -> Vec<&'static str> {
    let mut labels = Vec::with_capacity(AUXILIARY_WINDOW_LABELS.len());
    if config.optional_module_installed(crate::domain::config::OPTIONAL_MODULE_OVERLAYS)
        && config.enable_tz_overlay
    {
        labels.push(TERROR_ZONE_OVERLAY_LABEL);
    }
    if config.optional_module_installed(crate::domain::config::OPTIONAL_MODULE_OVERLAYS)
        && config.optional_module_installed(crate::domain::config::OPTIONAL_MODULE_AUTOMATION)
        && config.enable_stats_overlay
    {
        labels.push(STATS_OVERLAY_LABEL);
    }
    if config.optional_module_installed(crate::domain::config::OPTIONAL_MODULE_PET)
        && config.enable_bongo_cat
    {
        labels.push(DESKTOP_PET_LABEL);
    }
    labels
}

/// Returns an existing auxiliary WebView or creates it from the corresponding
/// `create: false` entry in `tauri.conf.json`. Capability stop destroys the
/// WebView; a later start recreates it from this same static configuration.
pub(crate) fn ensure_window(app: &AppHandle, label: &str) -> Result<WebviewWindow, AppError> {
    validate_label(label)?;
    if let Some(window) = app.get_webview_window(label) {
        return Ok(window);
    }

    let lifecycle = app
        .try_state::<AuxiliaryWindowLifecycle>()
        .ok_or_else(|| AppError::Unknown("辅助窗口生命周期尚未就绪".to_string()))?;
    // A worker can be waiting for the Tauri main thread while it owns this
    // lock. Never block that same main thread behind the worker: the current
    // owner will finish the creation, and capability reconciliation retries a
    // transient loser.
    let _create_guard = lifecycle
        .create_lock
        .try_lock()
        .ok_or_else(|| AppError::Unknown(format!("辅助窗口正在创建，请稍后重试: {label}")))?;

    // Another caller may have completed creation between the initial lookup
    // and this successful lock acquisition.
    if let Some(window) = app.get_webview_window(label) {
        return Ok(window);
    }

    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == label)
        .cloned()
        .ok_or_else(|| AppError::Unknown(format!("缺少辅助窗口配置: {label}")))?;
    if config.create {
        return Err(AppError::Unknown(format!(
            "辅助窗口必须配置为按需创建: {label}"
        )));
    }

    crate::logger::log_msg(
        "INFO",
        "AuxiliaryWindow",
        &format!("正在创建可选窗口 {label}"),
    );
    let started_at = std::time::Instant::now();
    let window = WebviewWindowBuilder::from_config(app, &config)
        .map_err(|error| AppError::Unknown(format!("读取辅助窗口配置失败 {label}: {error}")))?
        .build()
        .map_err(|error| AppError::Unknown(format!("创建辅助窗口失败 {label}: {error}")))?;
    crate::logger::log_msg(
        "INFO",
        "AuxiliaryWindow",
        &format!(
            "可选窗口 {label} 创建完成，耗时 {} ms",
            started_at.elapsed().as_millis()
        ),
    );
    Ok(window)
}

/// Releases the renderer and native window owned by a disabled capability.
/// Placement survives in the versioned placement file, not in a hidden WebView.
pub(crate) fn destroy_window(app: &AppHandle, label: &str) -> Result<bool, AppError> {
    validate_label(label)?;
    let Some(window) = app.get_webview_window(label) else {
        return Ok(false);
    };
    if label == STATS_OVERLAY_LABEL {
        crate::input_listener::set_stats_overlay_mini_input_region_state(false, 0, 0, 0, 0);
    }
    crate::logger::log_msg(
        "INFO",
        "AuxiliaryWindow",
        &format!("正在销毁可选窗口 {label}"),
    );
    window
        .destroy()
        .map_err(|error| AppError::Unknown(format!("销毁辅助窗口失败 {label}: {error}")))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_selection_follows_each_feature_switch_independently() {
        let config = GlobalConfig {
            enable_tz_overlay: true,
            enable_stats_overlay: false,
            enable_bongo_cat: true,
            installed_optional_modules: vec![
                crate::domain::config::OPTIONAL_MODULE_OVERLAYS.to_string(),
                crate::domain::config::OPTIONAL_MODULE_PET.to_string(),
            ],
            ..GlobalConfig::default()
        };

        assert_eq!(
            startup_window_labels(&config),
            vec![TERROR_ZONE_OVERLAY_LABEL, DESKTOP_PET_LABEL]
        );
    }

    #[test]
    fn disabled_features_create_no_auxiliary_windows_at_startup() {
        let config = GlobalConfig {
            enable_tz_overlay: false,
            enable_stats_overlay: false,
            enable_bongo_cat: false,
            ..GlobalConfig::default()
        };

        assert!(startup_window_labels(&config).is_empty());
    }

    #[test]
    fn only_known_auxiliary_labels_are_accepted() {
        for label in AUXILIARY_WINDOW_LABELS {
            assert!(validate_label(label).is_ok());
        }
        assert!(validate_label("main").is_err());
        assert!(validate_label("arbitrary-window").is_err());
    }
}
