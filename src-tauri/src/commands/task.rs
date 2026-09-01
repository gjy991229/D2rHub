use std::sync::Arc;

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::application::task_runtime::{
    TaskObserver, TaskRuntimeError, TaskSnapshot, TaskTimelineEntry,
};
use crate::error::AppError;
use crate::state::SharedState;

pub const TASK_UPDATED_EVENT: &str = "task-status-updated";

struct TauriTaskObserver {
    app: tauri::AppHandle,
}

impl TaskObserver for TauriTaskObserver {
    fn task_updated(&self, snapshot: &TaskSnapshot) {
        if let Err(error) = self.app.emit(TASK_UPDATED_EVENT, snapshot) {
            log::warn!(
                "发布任务 {} 状态 revision={} 失败: {}",
                snapshot.task_id,
                snapshot.revision,
                error
            );
        }
    }
}

pub fn install_observer(app: &tauri::AppHandle, state: &SharedState) {
    state
        .tasks()
        .set_observer(Some(Arc::new(TauriTaskObserver { app: app.clone() })));
}

fn map_task_error(error: TaskRuntimeError) -> AppError {
    AppError::Unknown(error.to_string())
}

#[tauri::command]
pub fn get_tasks(state: tauri::State<'_, SharedState>) -> Vec<TaskSnapshot> {
    state.tasks().snapshots()
}

#[tauri::command]
pub fn get_task(
    state: tauri::State<'_, SharedState>,
    task_id: u64,
) -> Result<TaskSnapshot, AppError> {
    state
        .tasks()
        .snapshot(task_id)
        .ok_or_else(|| map_task_error(TaskRuntimeError::NotFound(task_id)))
}

#[tauri::command]
pub fn get_task_timeline(
    state: tauri::State<'_, SharedState>,
    task_id: u64,
) -> Result<Vec<TaskTimelineEntry>, AppError> {
    state.tasks().timeline(task_id).map_err(map_task_error)
}

#[tauri::command]
pub fn cancel_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    task_id: u64,
) -> Result<TaskSnapshot, AppError> {
    let snapshot = state
        .tasks()
        .request_cancel(task_id)
        .map_err(map_task_error)?;
    // Existing launch and initialization adapters share the core cancellation
    // generation. The task flag is the user-visible source of truth while this
    // compatibility bridge wakes their current checkpoints.
    if matches!(
        snapshot.kind.as_str(),
        "account-initialize" | "account-reinitialize" | "account-launch" | "battle-net-launch"
    ) {
        state.multi_instance().facade().cancel_current_operation();
    }
    if snapshot.kind == "room-automation" {
        if let Some(command_state) = app.try_state::<
            crate::capabilities::room_automation_runtime::RoomAutomationCommandState,
        >() {
            command_state
                .manager()
                .map_err(AppError::Unknown)?
                .cancel()
                .map_err(AppError::Unknown)?;
        }
    }
    Ok(snapshot)
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskRetryDescriptor {
    pub kind: String,
    pub subject: Option<String>,
    pub retry_of: u64,
}

#[tauri::command]
pub fn get_task_retry_descriptor(
    state: tauri::State<'_, SharedState>,
    task_id: u64,
) -> Result<TaskRetryDescriptor, AppError> {
    let request = state
        .tasks()
        .retry_request(task_id)
        .map_err(map_task_error)?;
    Ok(TaskRetryDescriptor {
        kind: request.kind,
        subject: request.subject,
        retry_of: request.retry_of.unwrap_or(task_id),
    })
}
