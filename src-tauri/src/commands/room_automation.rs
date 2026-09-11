use crate::capabilities::room_automation::{RoomAutomationConfig, WorkflowStatus};
use crate::capabilities::room_automation_config::RoomAutomationConfigSnapshot;
use crate::capabilities::room_automation_runtime::{
    RoomAutomationCommandState, RoomAutomationManager, RoomAutomationSaveOutcome,
};
use crate::capabilities::room_chat_binding::ChatF13BindingStatus;
use crate::state::SharedState;
use std::sync::Arc;
use tauri::Manager;

fn manager(app: &tauri::AppHandle) -> Result<Arc<RoomAutomationManager>, String> {
    app.try_state::<RoomAutomationCommandState>()
        .ok_or_else(|| "当前模式未加载自动跟房模块".to_string())?
        .manager()
        .cloned()
}

fn require_module_installed(state: &tauri::State<'_, SharedState>) -> Result<(), String> {
    let installed = state.optional_runtime_ready()
        && state.configuration().snapshot().is_some_and(|config| {
            config.optional_module_runtime_allowed(
                crate::domain::config::OPTIONAL_MODULE_ROOM_AUTOMATION,
            )
        });
    installed
        .then_some(())
        .ok_or_else(|| "自动跟房模块尚未安装".to_string())
}

#[tauri::command]
pub(crate) fn room_automation_get_config(
    app: tauri::AppHandle,
) -> Result<RoomAutomationConfigSnapshot, String> {
    Ok(manager(&app)?.get_config())
}

#[tauri::command(async, rename_all = "camelCase")]
pub(crate) fn room_automation_save_config(
    app: tauri::AppHandle,
    global: tauri::State<'_, SharedState>,
    expected_generation: u64,
    config: RoomAutomationConfig,
) -> Result<RoomAutomationSaveOutcome, String> {
    let _profile = global
        .runtime_activation_lock
        .try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    if config.enabled {
        require_module_installed(&global)?;
    }
    manager(&app)?.save_config(expected_generation, config)
}

#[tauri::command]
pub(crate) fn room_automation_get_status(app: tauri::AppHandle) -> Result<WorkflowStatus, String> {
    Ok(manager(&app)?.get_status())
}

#[tauri::command(async)]
pub(crate) fn room_automation_start_primary(
    app: tauri::AppHandle,
    global: tauri::State<'_, SharedState>,
) -> Result<WorkflowStatus, String> {
    let _profile = global
        .runtime_activation_lock
        .try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    require_module_installed(&global)?;
    manager(&app)?.start_primary()
}

#[tauri::command(async)]
pub(crate) fn room_automation_start_followers(
    app: tauri::AppHandle,
    global: tauri::State<'_, SharedState>,
) -> Result<WorkflowStatus, String> {
    let _profile = global
        .runtime_activation_lock
        .try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    require_module_installed(&global)?;
    manager(&app)?.start_followers()
}

#[tauri::command(async)]
pub(crate) fn room_automation_retry(
    app: tauri::AppHandle,
    global: tauri::State<'_, SharedState>,
) -> Result<WorkflowStatus, String> {
    let _profile = global
        .runtime_activation_lock
        .try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    require_module_installed(&global)?;
    manager(&app)?.retry()
}

#[tauri::command(async)]
pub(crate) fn room_automation_cancel(app: tauri::AppHandle) -> Result<WorkflowStatus, String> {
    manager(&app)?.cancel()
}

#[tauri::command(async)]
pub(crate) fn room_automation_get_chat_binding(
    app: tauri::AppHandle,
    global: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    // Resolving this status can initialize the lazy directory service. Wait for
    // activation or a profile transition instead of surfacing transient contention.
    let _profile = global.runtime_activation_lock.lock();
    require_module_installed(&global)?;
    manager(&app)?.get_chat_binding()
}

#[tauri::command(async)]
pub(crate) fn room_automation_install_chat_binding(
    app: tauri::AppHandle,
    global: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    let _profile = global.runtime_activation_lock.lock();
    require_module_installed(&global)?;
    manager(&app)?.install_chat_binding()
}

#[tauri::command(async)]
pub(crate) fn room_automation_restore_chat_binding(
    app: tauri::AppHandle,
    global: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    let _profile = global
        .runtime_activation_lock
        .try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    manager(&app)?.restore_chat_binding()
}
