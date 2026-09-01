use crate::application::capability::{CapabilityDriver, CapabilityFailure, CapabilityHealth};

pub(crate) struct RuneAudioCapability {
    app: tauri::AppHandle,
}

impl RuneAudioCapability {
    pub(crate) fn install(app: &tauri::App) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            app: app.handle().clone(),
        })
    }
}

impl CapabilityDriver for RuneAudioCapability {
    fn start(&self) -> Result<(), CapabilityFailure> {
        crate::rune_audio::monitor::start_blocking(self.app.clone())
            .map_err(|error| CapabilityFailure::new("monitor-start-failed", error))
    }

    fn stop(&self) -> Result<(), CapabilityFailure> {
        crate::rune_audio::monitor::stop_blocking()
            .map_err(|error| CapabilityFailure::new("monitor-stop-timeout", error))
    }

    fn health(&self) -> CapabilityHealth {
        match crate::rune_audio::monitor::lifecycle_health() {
            Ok(()) => CapabilityHealth::Healthy,
            Err(error) => {
                CapabilityHealth::Failed(CapabilityFailure::new("monitor-unavailable", error))
            }
        }
    }
}
