//! Built-in optional capabilities.
//!
//! The application registry owns lifecycle truth while this module provides
//! Tauri/Windows adapters and the asynchronous supervisor. Optional modules
//! are never started directly from configuration transactions.

mod bongo_cat;
mod overlay_windows;
pub(crate) mod room_automation;
pub(crate) mod room_automation_config;
pub(crate) mod room_automation_runtime;
#[cfg(target_os = "windows")]
mod room_automation_windows;
pub(crate) mod room_chat_binding;
mod rune_audio;
mod supervisor;

use crate::application::capability::{
    CapabilityCategory, CapabilityDescriptor, CapabilityDriver, CapabilityFailure,
    CapabilityHealth, CapabilityId, CapabilityRegistration, CapabilityRegistryError,
};
use crate::domain::config::{
    GlobalConfig, CURRENT_CONFIG_VERSION, OPTIONAL_MODULE_AUTOMATION, OPTIONAL_MODULE_OVERLAYS,
    OPTIONAL_MODULE_PET, OPTIONAL_MODULE_ROOM_AUTOMATION,
};
use crate::state::SharedState;
use std::sync::Arc;
use supervisor::CapabilitySupervisor;
use tauri::Manager;

pub(crate) const DESKTOP_PET_ID: CapabilityId = CapabilityId::new("desktop-pet");
pub(crate) const RUNE_AUDIO_ID: CapabilityId = CapabilityId::new("audio-telemetry");
pub(crate) const TERROR_ZONE_OVERLAY_ID: CapabilityId = CapabilityId::new("terror-zone-overlay");
pub(crate) const STATS_OVERLAY_ID: CapabilityId = CapabilityId::new("statistics-overlay");

struct UnavailableCapability {
    failure: CapabilityFailure,
}

impl CapabilityDriver for UnavailableCapability {
    fn start(&self) -> Result<(), CapabilityFailure> {
        Err(self.failure.clone())
    }

    fn stop(&self) -> Result<(), CapabilityFailure> {
        Ok(())
    }

    fn health(&self) -> CapabilityHealth {
        CapabilityHealth::Failed(self.failure.clone())
    }
}

/// Install capability-owned policies and register concrete lifecycle drivers.
pub(crate) fn install(app: &tauri::App) {
    crate::input_listener::set_bongo_cat_input_enabled(false);

    let desktop_pet_driver: Arc<dyn CapabilityDriver> =
        match bongo_cat::BongoCatCapability::install(app) {
            Ok(driver) => driver,
            Err(failure) => {
                crate::logger::log_msg(
                    "ERROR",
                    "DesktopPet",
                    &format!("桌宠 capability 安装失败: {}", failure.message),
                );
                Arc::new(UnavailableCapability { failure })
            }
        };
    let rune_audio_driver: Arc<dyn CapabilityDriver> =
        rune_audio::RuneAudioCapability::install(app);
    let terror_zone_overlay_driver: Arc<dyn CapabilityDriver> =
        overlay_windows::OverlayWindowCapability::install(
            app,
            crate::auxiliary_windows::TERROR_ZONE_OVERLAY_LABEL,
        );
    let stats_overlay_driver: Arc<dyn CapabilityDriver> =
        overlay_windows::OverlayWindowCapability::install(
            app,
            crate::auxiliary_windows::STATS_OVERLAY_LABEL,
        );

    let room_module_installed = app
        .state::<SharedState>()
        .configuration()
        .snapshot()
        .is_some_and(|config| config.optional_module_installed(OPTIONAL_MODULE_ROOM_AUTOMATION));
    let (room_driver, room_requested, room_command_state): (
        Arc<dyn CapabilityDriver>,
        bool,
        room_automation_runtime::RoomAutomationCommandState,
    ) = match room_automation_runtime::RoomAutomationManager::install(app) {
        Ok(manager) => (
            manager.clone(),
            room_module_installed && manager.requested_enabled(),
            room_automation_runtime::RoomAutomationCommandState::available(manager),
        ),
        Err(failure) => {
            crate::logger::log_msg(
                "ERROR",
                "RoomAutomation",
                &format!("自动跟房 capability 安装失败: {}", failure.message),
            );
            (
                Arc::new(UnavailableCapability {
                    failure: failure.clone(),
                }),
                room_module_installed,
                room_automation_runtime::RoomAutomationCommandState::unavailable(failure.message),
            )
        }
    };
    if !app.manage(room_command_state) {
        crate::logger::log_msg("ERROR", "RoomAutomation", "自动跟房 command state 重复安装");
    }

    let state = app.state::<SharedState>();
    if let Err(error) = state.capabilities().register_all(vec![
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::first_party(
                DESKTOP_PET_ID,
                CapabilityCategory::Companion,
                CURRENT_CONFIG_VERSION,
                "pet",
                &["set_bongo_cat_input_visible"],
                &["global-input-event"],
            ),
            driver: desktop_pet_driver,
        },
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::first_party(
                RUNE_AUDIO_ID,
                CapabilityCategory::Telemetry,
                CURRENT_CONFIG_VERSION,
                "automation",
                &[
                    "get_rune_audio_status",
                    "start_rune_audio_monitor",
                    "restart_rune_audio_monitor",
                    "stop_rune_audio_monitor",
                    "start_rune_audio_diagnostic_recording",
                    "stop_rune_audio_diagnostic_recording",
                ],
                &[
                    "audio-tracking-state",
                    "rune-audio-detected",
                    "item-audio-detected",
                ],
            ),
            driver: rune_audio_driver,
        },
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::first_party(
                TERROR_ZONE_OVERLAY_ID,
                CapabilityCategory::Overlay,
                CURRENT_CONFIG_VERSION,
                "overlays",
                &["set_auxiliary_window_visible", "recover_auxiliary_windows"],
                &[],
            ),
            driver: terror_zone_overlay_driver,
        },
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::first_party(
                STATS_OVERLAY_ID,
                CapabilityCategory::Overlay,
                CURRENT_CONFIG_VERSION,
                "overlays",
                &["set_auxiliary_window_visible", "recover_auxiliary_windows"],
                &[],
            ),
            driver: stats_overlay_driver,
        },
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::first_party(
                room_automation_runtime::ROOM_AUTOMATION_ID,
                CapabilityCategory::Automation,
                room_automation::CURRENT_STRATEGY_VERSION.into(),
                "room-automation",
                &[
                    "room_automation_get_config",
                    "room_automation_save_config",
                    "room_automation_get_status",
                    "room_automation_start_primary",
                    "room_automation_start_followers",
                    "room_automation_retry",
                    "room_automation_cancel",
                ],
                &[
                    room_automation_runtime::STATUS_EVENT,
                    room_automation_runtime::CONFIG_EVENT,
                ],
            ),
            driver: room_driver,
        },
    ]) {
        crate::logger::log_msg(
            "ERROR",
            "Capabilities",
            &format!("注册 capability 失败: {error}"),
        );
    }
    if let Err(error) = state
        .capabilities()
        .set_requested(room_automation_runtime::ROOM_AUTOMATION_ID, room_requested)
    {
        crate::logger::log_msg(
            "ERROR",
            "RoomAutomation",
            &format!("应用自动跟房模块开关失败: {error}"),
        );
    }
}

/// Starts the serialized supervisor after all platform services are ready and
/// reconciles the latest committed configuration snapshot.
pub(crate) fn start(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<SharedState>();
    let registry = Arc::clone(state.capabilities());
    let supervisor = CapabilitySupervisor::start(app.clone(), registry)?;
    if !app.manage(supervisor) {
        return Err("capability supervisor 重复安装".to_string());
    }
    overlay_windows::install(app);

    if let Some(config) = state.configuration().snapshot() {
        apply_configuration(state.inner(), Some(app), &config);
    }
    Ok(())
}

/// Bounded projection from legacy global fields to lifecycle intent. It is
/// safe inside `ConfigurationObserver::publish`: hooks run on the supervisor.
pub(crate) fn apply_configuration(
    state: &SharedState,
    app: Option<&tauri::AppHandle>,
    config: &GlobalConfig,
) {
    for (id, requested, name) in configured_capabilities(config) {
        match state.capabilities().set_requested(id, requested) {
            Ok(_) => {}
            // Initial global config loading happens before Tauri adapters are
            // registered. Setup replays the cached snapshot after registration.
            Err(CapabilityRegistryError::UnknownCapability(_)) if app.is_none() => {}
            Err(error) => crate::logger::log_msg(
                "ERROR",
                "Capabilities",
                &format!("应用{name}模块开关失败: {error}"),
            ),
        }
    }
    if let Some(app) = app {
        let room_requested = if config.optional_module_installed(OPTIONAL_MODULE_ROOM_AUTOMATION) {
            app.try_state::<room_automation_runtime::RoomAutomationCommandState>()
                .map(|command_state| {
                    command_state
                        .manager()
                        .map(|manager| manager.requested_enabled())
                        .unwrap_or(true)
                })
                .unwrap_or(false)
        } else {
            false
        };
        if let Err(error) = state
            .capabilities()
            .set_requested(room_automation_runtime::ROOM_AUTOMATION_ID, room_requested)
        {
            crate::logger::log_msg(
                "ERROR",
                "Capabilities",
                &format!("应用自动跟房模块开关失败: {error}"),
            );
        }
    }
    if let Some(supervisor) = app.and_then(|app| app.try_state::<CapabilitySupervisor>()) {
        supervisor.schedule_reconcile();
    }
}

fn configured_capabilities(config: &GlobalConfig) -> [(CapabilityId, bool, &'static str); 4] {
    let overlays_installed = config.optional_module_installed(OPTIONAL_MODULE_OVERLAYS);
    let automation_installed = config.optional_module_installed(OPTIONAL_MODULE_AUTOMATION);
    [
        (
            DESKTOP_PET_ID,
            config.optional_module_installed(OPTIONAL_MODULE_PET) && config.enable_bongo_cat,
            "桌宠",
        ),
        (
            RUNE_AUDIO_ID,
            automation_installed && config.rune_audio_enabled,
            "声纹识别",
        ),
        (
            TERROR_ZONE_OVERLAY_ID,
            overlays_installed && config.enable_tz_overlay,
            "恐怖区域悬浮窗",
        ),
        (
            STATS_OVERLAY_ID,
            overlays_installed && automation_installed && config.enable_stats_overlay,
            "统计悬浮窗",
        ),
    ]
}

pub(crate) fn schedule_reconcile(app: &tauri::AppHandle) {
    if let Some(supervisor) = app.try_state::<CapabilitySupervisor>() {
        supervisor.schedule_reconcile();
    }
}

pub(crate) fn shutdown(app: &tauri::AppHandle) {
    if let Some(supervisor) = app.try_state::<CapabilitySupervisor>() {
        supervisor.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_configuration_projects_every_supervised_optional_capability() {
        let config = GlobalConfig {
            enable_bongo_cat: true,
            rune_audio_enabled: false,
            enable_tz_overlay: true,
            enable_stats_overlay: false,
            installed_optional_modules: vec![
                OPTIONAL_MODULE_OVERLAYS.to_string(),
                OPTIONAL_MODULE_PET.to_string(),
            ],
            ..GlobalConfig::default()
        };

        assert_eq!(
            configured_capabilities(&config)
                .into_iter()
                .map(|(id, requested, _)| (id.as_str(), requested))
                .collect::<Vec<_>>(),
            [
                ("desktop-pet", true),
                ("audio-telemetry", false),
                ("terror-zone-overlay", true),
                ("statistics-overlay", false),
            ]
        );
    }
}
