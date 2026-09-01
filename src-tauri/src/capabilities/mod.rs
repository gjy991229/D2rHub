//! Built-in optional capabilities.
//!
//! The application registry owns lifecycle truth while this module provides
//! Tauri/Windows adapters and the asynchronous supervisor. Optional modules
//! are never started directly from configuration transactions.

mod bongo_cat;
mod overlay_windows;
mod room_chat_binding;
pub(crate) mod room_automation;
pub(crate) mod room_automation_config;
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

    let driver: Arc<dyn CapabilityDriver> = match bongo_cat::BongoCatCapability::install(app) {
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
    let state = app.state::<SharedState>();
    if let Err(error) = state
        .capabilities()
        .register_all(vec![CapabilityRegistration {
            descriptor: CapabilityDescriptor::optional(DESKTOP_PET_ID),
            driver,
        }])
    {
        crate::logger::log_msg(
            "ERROR",
            "Capabilities",
            &format!("注册桌宠 capability 失败: {error}"),
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
    match state
        .capabilities()
        .set_requested(DESKTOP_PET_ID, config.enable_bongo_cat)
    {
        Ok(_) => {
            if let Some(supervisor) = app.and_then(|app| app.try_state::<CapabilitySupervisor>()) {
                supervisor.schedule_reconcile();
            }
        }
        // Initial global config loading happens before Tauri adapters are
        // registered. Setup replays the cached snapshot after registration.
        Err(CapabilityRegistryError::UnknownCapability(_)) if app.is_none() => {}
        Err(error) => crate::logger::log_msg(
            "ERROR",
            "Capabilities",
            &format!("应用桌宠模块开关失败: {error}"),
        ),
    }
}

pub(crate) fn shutdown(app: &tauri::AppHandle) {
    if let Some(supervisor) = app.try_state::<CapabilitySupervisor>() {
        supervisor.shutdown();
    }
}
