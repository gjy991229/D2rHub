//! Pure domain model for the optional room-automation capability.
//!
//! Platform drivers, persistence, commands, and capability registration live
//! outside this module. Keeping this layer deterministic makes legacy imports
//! and workflow recovery testable without a Tauri runtime.

mod model;
mod status;

pub use model::{
    canonicalize_shortcut, FlowStrategy, NormalizationReport, RoomAutomationConfig,
    RoomAutomationConfigError, ShortcutValidationError, CURRENT_STRATEGY_VERSION,
    MAX_ROOM_TEXT_LENGTH,
};
pub use status::{
    PendingRoom, PrimaryTask, WaitingMode, WorkflowPhase, WorkflowStateError, WorkflowStatus,
    WorkflowTaskId, WorkflowTaskState,
};
