use crate::application::capability::CapabilityStatusSnapshot;
use crate::state::SharedState;

#[tauri::command]
pub fn get_capability_statuses(state: tauri::State<'_, SharedState>) -> CapabilityStatusSnapshot {
    state.capabilities().snapshot()
}
