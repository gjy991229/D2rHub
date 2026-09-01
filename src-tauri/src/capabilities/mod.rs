//! Built-in optional capabilities.
//!
//! The application bootstrap installs event-driven policies first, then starts
//! long-running capability runtimes after the core services are ready. Keeping
//! those responsibilities explicit leaves a small seam for supervised
//! start/stop behavior without introducing a general plugin framework.

mod bongo_cat;
mod overlay_windows;

/// Install capability-owned window policies and event handlers.
pub(crate) fn install(app: &tauri::App) {
    overlay_windows::install(app);
}

/// Start capability-owned background runtimes.
pub(crate) fn start(app: &tauri::App) {
    if let Some(bongo_cat) = bongo_cat::InstalledBongoCat::install(app) {
        bongo_cat.start();
    }
}
