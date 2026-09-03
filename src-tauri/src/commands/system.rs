//! Thin Tauri boundary for required operating-system adapters.
//!
//! These commands only translate IPC parameters and results. Process, window,
//! input, filesystem, and update mechanics live in `infrastructure::system`.

use crate::error::AppError;
use crate::infrastructure::system as adapter;

#[tauri::command]
pub fn activate_application_runtime(app: tauri::AppHandle) -> Result<bool, String> {
    crate::activate_application_runtime(&app)
}

#[tauri::command]
pub fn get_d2r_pids() -> Vec<u32> {
    adapter::get_d2r_pids()
}

#[tauri::command]
pub fn kill_all_d2r_processes() -> Result<(), AppError> {
    adapter::kill_all_d2r_processes()
}

#[tauri::command]
pub fn snapshot_processes(process_name: String) -> Vec<u32> {
    adapter::snapshot_processes(process_name)
}

#[tauri::command]
pub async fn wait_for_new_process(
    process_name: String,
    known_pids: Vec<u32>,
    timeout_secs: u64,
) -> Result<u32, AppError> {
    adapter::wait_for_new_process(process_name, known_pids, timeout_secs).await
}

#[tauri::command]
pub fn check_game_connected(pid: u32) -> bool {
    adapter::check_game_connected(pid)
}

#[tauri::command]
pub fn bring_window_by_title_to_front(window_title: &str) -> bool {
    adapter::bring_window_by_title_to_front(window_title)
}

#[tauri::command]
pub fn get_foreground_window_title() -> String {
    adapter::get_foreground_window_title()
}

#[tauri::command]
pub fn get_d2r_window_titles() -> Vec<String> {
    adapter::get_d2r_window_titles()
}

#[tauri::command]
pub fn refresh_account_running_state(
    state: tauri::State<'_, crate::state::SharedState>,
) -> Result<Vec<String>, String> {
    adapter::refresh_account_running_state(state, |config| {
        crate::commands::account::AccountManager::list_ids(&config.accounts_dir)
            .into_iter()
            .filter_map(|account_id| {
                let meta = crate::commands::account::AccountManager::load_meta(
                    &config.accounts_dir,
                    &account_id,
                )
                .ok()?;
                let context = crate::launch_context::LaunchContext::for_account(
                    config,
                    &meta,
                    crate::launch_context::ContextPurpose::LaunchGame,
                )
                .ok()?;
                let window_title = if meta.display_name.is_empty() {
                    account_id.clone()
                } else {
                    meta.display_name
                };
                Some(adapter::AccountGameIdentity::new(
                    account_id,
                    window_title,
                    context.installation.game_executable,
                ))
            })
            .collect()
    })
}

#[tauri::command]
pub fn bring_bnet_to_foreground() {
    adapter::bring_bnet_to_foreground();
}

#[tauri::command]
pub fn bring_self_to_foreground(app: tauri::AppHandle) {
    adapter::bring_self_to_foreground(app);
}

#[tauri::command]
pub fn send_keys_to_window(pid: u32) -> Result<(), AppError> {
    adapter::send_keys_to_window(pid)
}

#[tauri::command]
pub fn is_admin() -> bool {
    adapter::is_admin()
}

#[tauri::command]
pub fn exit_app(app: tauri::AppHandle) {
    adapter::exit_app(app);
}

#[tauri::command]
pub fn open_logs_dir() -> Result<(), AppError> {
    adapter::open_logs_dir()
}

#[tauri::command]
pub fn open_user_guide(app: tauri::AppHandle) -> Result<(), AppError> {
    adapter::open_user_guide(app)
}

#[tauri::command]
pub fn get_app_version() -> String {
    adapter::get_app_version()
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle, url: String) -> Result<(), String> {
    adapter::install_update(app, url).await
}

#[tauri::command]
pub fn check_cloud_version() -> Result<adapter::CloudVersionInfo, String> {
    adapter::check_cloud_version()
}

#[tauri::command]
pub fn check_path_exists(path: String, is_file: bool) -> bool {
    adapter::check_path_exists(path, is_file)
}
