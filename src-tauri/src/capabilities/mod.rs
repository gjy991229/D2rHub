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
    CapabilityDescriptor, CapabilityDriver, CapabilityFailure, CapabilityHealth, CapabilityId,
    CapabilityRegistration, CapabilityRegistryError,
};
use crate::domain::config::GlobalConfig;
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
    overlay_windows::install(app);
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

    let (room_driver, room_requested, room_command_state): (
        Arc<dyn CapabilityDriver>,
        bool,
        room_automation_runtime::RoomAutomationCommandState,
    ) = match room_automation_runtime::RoomAutomationManager::install(app) {
        Ok(manager) => (
            manager.clone(),
            manager.requested_enabled(),
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
                true,
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
            descriptor: CapabilityDescriptor::optional(DESKTOP_PET_ID),
            driver: desktop_pet_driver,
        },
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::optional(RUNE_AUDIO_ID),
            driver: rune_audio_driver,
        },
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::optional(TERROR_ZONE_OVERLAY_ID),
            driver: terror_zone_overlay_driver,
        },
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::optional(STATS_OVERLAY_ID),
            driver: stats_overlay_driver,
        },
        CapabilityRegistration {
            descriptor: CapabilityDescriptor::optional(room_automation_runtime::ROOM_AUTOMATION_ID),
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
pub(crate) fn start(app: &tauri::App) {
    let state = app.state::<SharedState>();
    let registry = Arc::clone(state.capabilities());
    let supervisor = match CapabilitySupervisor::start(app.handle().clone(), registry) {
        Ok(supervisor) => supervisor,
        Err(error) => {
            crate::logger::log_msg("ERROR", "Capabilities", &error);
            return;
        }
    };
    if !app.manage(supervisor) {
        crate::logger::log_msg("ERROR", "Capabilities", "capability supervisor 重复安装");
        return;
    }

    if let Some(config) = state.configuration().snapshot() {
        apply_configuration(state.inner(), Some(app.handle()), &config);
    }
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
    if let Some(supervisor) = app.and_then(|app| app.try_state::<CapabilitySupervisor>()) {
        supervisor.schedule_reconcile();
    }
}

fn configured_capabilities(config: &GlobalConfig) -> [(CapabilityId, bool, &'static str); 4] {
    [
        (DESKTOP_PET_ID, config.enable_bongo_cat, "桌宠"),
        (RUNE_AUDIO_ID, config.rune_audio_enabled, "声纹识别"),
        (
            TERROR_ZONE_OVERLAY_ID,
            config.enable_tz_overlay,
            "恐怖区域悬浮窗",
        ),
        (STATS_OVERLAY_ID, config.enable_stats_overlay, "统计悬浮窗"),
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
