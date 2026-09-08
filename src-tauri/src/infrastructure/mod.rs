//! Required platform adapters shared by the core and optional capabilities.

pub(crate) mod diagnostics;
pub(crate) mod durable_fs;
pub mod module_config;
pub(crate) mod process;
pub(crate) mod system;
#[cfg(target_os = "windows")]
pub(crate) mod physical_input;
