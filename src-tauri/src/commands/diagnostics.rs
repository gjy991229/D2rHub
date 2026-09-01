use std::path::PathBuf;

use crate::commands::account::AccountManager;
use crate::error::AppError;
use crate::infrastructure::diagnostics::{
    create_diagnostic_bundle, DiagnosticRedaction, DiagnosticReport,
};
use crate::state::SharedState;

#[tauri::command]
pub fn export_diagnostic_bundle(state: tauri::State<'_, SharedState>) -> Result<String, AppError> {
    let config = state.configuration().snapshot();
    let mut redaction = DiagnosticRedaction::default();
    for (variable, replacement) in [
        ("USERPROFILE", "[user-profile]"),
        ("APPDATA", "[appdata]"),
        ("LOCALAPPDATA", "[local-appdata]"),
        ("TEMP", "[temp]"),
    ] {
        if let Ok(value) = std::env::var(variable) {
            redaction.add(value, replacement);
        }
    }

    if let Some(config) = config.as_ref() {
        for (path, replacement) in [
            (&config.accounts_dir, "[accounts-dir]"),
            (&config.cn_game_path, "[cn-game-dir]"),
            (&config.global_game_path, "[global-game-dir]"),
            (&config.cn_saved_games_path, "[cn-save-dir]"),
            (&config.global_saved_games_path, "[global-save-dir]"),
            (&config.cn_battle_net_path, "[battle-net]"),
            (&config.browser_path, "[browser]"),
        ] {
            redaction.add(path, replacement);
        }
        for account_id in AccountManager::list_ids(&config.accounts_dir) {
            redaction.add(&account_id, "[account]");
            if let Ok(account) = AccountManager::load_meta(&config.accounts_dir, &account_id) {
                redaction.add(account.display_name, "[account-name]");
            }
        }
    }

    let tasks = state
        .tasks()
        .snapshots()
        .into_iter()
        .map(|task| {
            let timeline = state.tasks().timeline(task.task_id).unwrap_or_default();
            serde_json::json!({
                "revision": task.revision,
                "task_id": task.task_id,
                "kind": task.kind,
                "state": task.state,
                "progress": task.progress,
                "step": task.step,
                "message": task.message,
                "error_code": task.error_code,
                "cancel_requested": task.cancel_requested,
                "retryable": task.retryable,
                "retry_of": task.retry_of,
                "started_at_ms": task.started_at_ms,
                "finished_at_ms": task.finished_at_ms,
                "timeline": timeline,
            })
        })
        .collect::<Vec<_>>();
    let configuration = config.as_ref().map_or_else(
        || serde_json::json!({ "loaded": false }),
        |config| {
            serde_json::json!({
                "loaded": true,
                "schema_version": config.version,
                "first_run_complete": config.first_run_complete,
                "accounts_directory_configured": !config.accounts_dir.trim().is_empty(),
                "cn_game_configured": !config.cn_game_path.trim().is_empty(),
                "global_game_configured": !config.global_game_path.trim().is_empty(),
                "browser_configured": !config.browser_path.trim().is_empty(),
                "audio_requested": config.rune_audio_enabled,
                "terror_zone_overlay_requested": config.enable_tz_overlay,
                "statistics_overlay_requested": config.enable_stats_overlay,
                "desktop_pet_requested": config.enable_bongo_cat,
            })
        },
    );
    let report = DiagnosticReport {
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        configuration,
        capabilities: serde_json::to_value(state.capabilities().snapshot())
            .unwrap_or_else(|_| serde_json::json!({ "serialization_error": true })),
        tasks: serde_json::Value::Array(tasks),
    };
    let output_directory = PathBuf::from(&state.app_data_dir).join("diagnostics");
    let logs_directory = crate::logger::get_logs_dir();
    let output = create_diagnostic_bundle(
        &output_directory,
        logs_directory.as_deref(),
        report,
        &redaction,
    )
    .map_err(AppError::FileError)?;
    Ok(output.to_string_lossy().into_owned())
}
