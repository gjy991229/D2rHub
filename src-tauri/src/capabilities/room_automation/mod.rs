//! Pure domain model for the optional room-automation capability.
//!
//! Platform drivers, persistence, commands, and capability registration live
//! outside this module. Keeping this layer deterministic makes legacy imports
//! and workflow recovery testable without a Tauri runtime.

mod model;
mod status;

#[cfg(test)]
pub use model::CURRENT_STRATEGY_VERSION;
pub use model::{
    FlowStrategy, NormalizationReport, RoomAutomationConfig, RoomAutomationConfigError,
};
pub use status::{
    WaitingMode, WorkflowPhase, WorkflowRecoveryAction, WorkflowStateError, WorkflowStatus,
    WorkflowTaskId, WorkflowTaskState,
};
