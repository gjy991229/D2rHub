use crate::application::capability::{CapabilityDescriptorSnapshot, CapabilityStatusSnapshot};
use crate::state::SharedState;

#[tauri::command]
pub fn get_capability_statuses(state: tauri::State<'_, SharedState>) -> CapabilityStatusSnapshot {
    state.capabilities().snapshot()
}

#[tauri::command]
pub fn get_capability_descriptors(
    state: tauri::State<'_, SharedState>,
) -> Vec<CapabilityDescriptorSnapshot> {
    state.capabilities().descriptors()
}
