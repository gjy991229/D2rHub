use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::commands::account::{commit_account_settings_transaction, AccountManager};
use crate::domain::config::GlobalConfig;
use crate::error::AppError;
use crate::launch_context::{ContextPurpose, HostRuntimeLease, LaunchContext};
use crate::state::{AccountLifecycleLease, SharedState};

fn read_optional_settings_file(path: &Path) -> Result<HashMap<String, Value>, AppError> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = std::fs::read_to_string(path)?;
    let map: HashMap<String, Value> = serde_json::from_str(&content)?;
    Ok(map)
}

fn ensure_nonempty_settings(
    settings: &HashMap<String, Value>,
    source: &str,
) -> Result<(), AppError> {
    if settings.is_empty() {
        return Err(AppError::ConfigReadError(format!(
            "{source} 为空，无法创建完整的账号画质配置"
        )));
    }
    Ok(())
}

fn read_required_settings_file(
    path: &Path,
    source: &str,
) -> Result<HashMap<String, Value>, AppError> {
    if !path.is_file() {
        return Err(AppError::FileError(format!(
            "{source}不存在: {}",
            path.display()
        )));
    }
    let settings = read_optional_settings_file(path)?;
    ensure_nonempty_settings(&settings, source)?;
    Ok(settings)
}

fn save_account_settings_with_config(
    config: &GlobalConfig,
    account_id: &str,
    settings: &HashMap<String, Value>,
) -> Result<(), AppError> {
    // Preflight the persisted account and its complete edition-specific launch context before
    // Settings.json or account.json can be changed.
    let mut meta = AccountManager::load_meta(&config.accounts_dir, account_id)?;
    LaunchContext::for_account(config, &meta, ContextPurpose::Settings)?;
    ensure_nonempty_settings(settings, "待保存的 Settings.json")?;
    let content = serde_json::to_string_pretty(settings)?;
    meta.has_customized_settings = true;
    commit_account_settings_transaction(&config.accounts_dir, &meta, &content)
}

/// 获取指定账号的 Settings.json 内容
#[tauri::command]
pub fn get_account_settings(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<HashMap<String, Value>, AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let cfg = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    let meta = AccountManager::load_meta(&cfg.accounts_dir, &account_id)?;
    if meta.has_customized_settings {
        let settings_path = AccountManager::account_dir_checked(&cfg.accounts_dir, &account_id)?
            .join("Settings.json");
        read_required_settings_file(&settings_path, "账号 Settings.json")
    } else {
        let context = LaunchContext::for_account(&cfg, &meta, ContextPurpose::Settings)?;
        let _host_runtime_lease = HostRuntimeLease::try_acquire(state.inner().as_ref())?;
        let system_settings_path = context
            .required_saved_games_directory()?
            .join("Settings.json");
        read_required_settings_file(&system_settings_path, "系统 Settings.json")
    }
}

/// 保存指定账号的 Settings.json
#[tauri::command]
pub fn save_account_settings(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    settings: HashMap<String, Value>,
) -> Result<(), AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let cfg = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    save_account_settings_with_config(&cfg, &account_id, &settings)
}

/// 将系统 Saved Games 下的 Settings.json 快照到指定账号
#[tauri::command]
pub fn snapshot_system_settings_to_account(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<HashMap<String, Value>, AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let cfg = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    let mut meta = AccountManager::load_meta(&cfg.accounts_dir, &account_id)?;
    let context = LaunchContext::for_account(&cfg, &meta, ContextPurpose::Settings)?;
    let _host_runtime_lease = HostRuntimeLease::try_acquire(state.inner().as_ref())?;
    let src = context
        .required_saved_games_directory()?
        .join("Settings.json");
    let settings = read_required_settings_file(&src, "系统 Settings.json")?;
    meta.has_customized_settings = true;
    let content = serde_json::to_string_pretty(&settings)?;
    commit_account_settings_transaction(&cfg.accounts_dir, &meta, &content)?;

    Ok(settings)
}

/// 获取游戏安装目录下的 Settings.json（如果存在，用于对比）
#[tauri::command]
pub fn get_game_settings(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<HashMap<String, Value>, AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let cfg = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    let meta = AccountManager::load_meta(&cfg.accounts_dir, &account_id)?;
    let context = LaunchContext::for_account(&cfg, &meta, ContextPurpose::Settings)?;
    let _host_runtime_lease = HostRuntimeLease::try_acquire(state.inner().as_ref())?;
    let path = context
        .required_saved_games_directory()?
        .join("Settings.json");
    read_optional_settings_file(&path)
}

#[cfg(test)]
mod tests {
    use super::{ensure_nonempty_settings, save_account_settings_with_config};
    use crate::commands::account::{AccountManager, AccountMeta};
    use crate::domain::config::GlobalConfig;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "d2rhub_settings_{}_{}_{}",
            name,
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_preflight_failure_does_not_write_settings_or_meta() {
        let root = temp_dir("preflight");
        let accounts_dir = root.join("accounts");
        let config = GlobalConfig {
            accounts_dir: accounts_dir.to_string_lossy().to_string(),
            ..GlobalConfig::default()
        };

        let mut meta = AccountMeta::new("acount1");
        meta.region = Some("unknown".to_string());
        meta.auth_mode = Some("token".to_string());
        let account_dir =
            AccountManager::account_dir_checked(&config.accounts_dir, "acount1").unwrap();
        std::fs::create_dir_all(&account_dir).unwrap();
        AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();
        let meta_path = account_dir.join("account.json");
        let original_meta = std::fs::read_to_string(&meta_path).unwrap();
        let settings = HashMap::from([("VSync".to_string(), Value::Bool(true))]);

        let result = save_account_settings_with_config(&config, "acount1", &settings);

        assert!(result.is_err());
        assert!(!account_dir.join("Settings.json").exists());
        assert_eq!(std::fs::read_to_string(meta_path).unwrap(), original_meta);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn empty_settings_cannot_become_a_custom_account_snapshot() {
        let error =
            ensure_nonempty_settings(&HashMap::new(), "待保存的 Settings.json").unwrap_err();
        assert!(error.to_string().contains("无法创建完整"));
    }
}
