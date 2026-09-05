use crate::capabilities::room_automation::{RoomAutomationConfig, WorkflowStatus};
use crate::capabilities::room_automation_config::RoomAutomationConfigSnapshot;
use crate::capabilities::room_automation_runtime::{
    RoomAutomationCommandState, RoomAutomationManager, RoomAutomationSaveOutcome,
};
use crate::capabilities::room_chat_binding::ChatF13BindingStatus;
use crate::state::SharedState;
use std::sync::Arc;

fn manager<'a>(
    state: &'a tauri::State<'a, RoomAutomationCommandState>,
) -> Result<&'a Arc<RoomAutomationManager>, String> {
    state.manager()
}

fn require_module_installed(state: &tauri::State<'_, SharedState>) -> Result<(), String> {
    let installed = state.optional_runtime_ready() && state.configuration().snapshot().is_some_and(|config| {
        config.optional_module_runtime_allowed(crate::domain::config::OPTIONAL_MODULE_ROOM_AUTOMATION)
    });
    installed
        .then_some(())
        .ok_or_else(|| "自动跟房模块尚未安装".to_string())
}

#[tauri::command]
pub(crate) fn room_automation_get_config(
    state: tauri::State<'_, RoomAutomationCommandState>,
) -> Result<RoomAutomationConfigSnapshot, String> {
    Ok(manager(&state)?.get_config())
}

#[tauri::command(async, rename_all = "camelCase")]
pub(crate) fn room_automation_save_config(
    state: tauri::State<'_, RoomAutomationCommandState>,
    global: tauri::State<'_, SharedState>,
    expected_generation: u64,
    config: RoomAutomationConfig,
) -> Result<RoomAutomationSaveOutcome, String> {
    let _profile = global.runtime_activation_lock.try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    if config.enabled {
        require_module_installed(&global)?;
    }
    manager(&state)?.save_config(expected_generation, config)
}

#[tauri::command]
pub(crate) fn room_automation_get_status(
    state: tauri::State<'_, RoomAutomationCommandState>,
) -> Result<WorkflowStatus, String> {
    Ok(manager(&state)?.get_status())
}

#[tauri::command(async)]
pub(crate) fn room_automation_start_primary(
    state: tauri::State<'_, RoomAutomationCommandState>,
    global: tauri::State<'_, SharedState>,
) -> Result<WorkflowStatus, String> {
    let _profile = global.runtime_activation_lock.try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    require_module_installed(&global)?;
    manager(&state)?.start_primary()
}

#[tauri::command(async)]
pub(crate) fn room_automation_start_followers(
    state: tauri::State<'_, RoomAutomationCommandState>,
    global: tauri::State<'_, SharedState>,
) -> Result<WorkflowStatus, String> {
    let _profile = global.runtime_activation_lock.try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    require_module_installed(&global)?;
    manager(&state)?.start_followers()
}

#[tauri::command(async)]
pub(crate) fn room_automation_retry(
    state: tauri::State<'_, RoomAutomationCommandState>,
    global: tauri::State<'_, SharedState>,
) -> Result<WorkflowStatus, String> {
    let _profile = global.runtime_activation_lock.try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    require_module_installed(&global)?;
    manager(&state)?.retry()
}

#[tauri::command(async)]
pub(crate) fn room_automation_cancel(
    state: tauri::State<'_, RoomAutomationCommandState>,
) -> Result<WorkflowStatus, String> {
    manager(&state)?.cancel()
}

#[tauri::command(async)]
pub(crate) fn room_automation_get_chat_binding(
    state: tauri::State<'_, RoomAutomationCommandState>,
    global: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    // Resolving this status can initialize the lazy directory service.
    let _profile = global.runtime_activation_lock.try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    require_module_installed(&global)?;
    manager(&state)?.get_chat_binding()
}

#[tauri::command(async)]
pub(crate) fn room_automation_install_chat_binding(
    state: tauri::State<'_, RoomAutomationCommandState>,
    global: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    let _profile = global.runtime_activation_lock.try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    require_module_installed(&global)?;
    manager(&state)?.install_chat_binding()
}

#[tauri::command(async)]
pub(crate) fn room_automation_restore_chat_binding(
    state: tauri::State<'_, RoomAutomationCommandState>,
    global: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    let _profile = global.runtime_activation_lock.try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    manager(&state)?.restore_chat_binding()
}
