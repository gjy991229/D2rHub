use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use tauri::Emitter;

use crate::application::configuration::ConfigurationMutation;
use crate::application::multi_instance::{
    AccountCatalog, AccountCreationRepository, AccountCreationService, AccountDeletionCleanupPort,
    AccountDeletionService, AccountDeletionTransaction, AccountModRepository, AccountModService,
    AccountNameRepository, AccountNamingService, AccountOrderingService, AccountPositionService,
    AccountProfilePatch, AccountProfilePolicy, AccountProfileService, AccountQueryService,
    AccountRepository, AccountRuntimePort, AccountSettingsPreferenceRepository,
    AccountSettingsPreferenceService, CancellationTicket, CreateAccountRequest,
    ResolvedAccountProfile, TimestampProvider, TokenProtector, WindowPosition,
};
use crate::battle_net_config::{try_read_mod_args, update_mod_args};
use crate::commands::utils::{kill_processes_by_name, shared_system};
use crate::domain::account::{normalize_account_display_name, AuthMode, ClientEdition, GameRegion};
pub use crate::domain::account::{AccountMeta, WindowPositionPreset};
use crate::domain::config::GlobalConfig;
use crate::error::AppError;
use crate::launch_context::{ContextPurpose, EditionConventions, HostRuntimeLease, LaunchContext};
use crate::state::{AccountLifecycleLease, SharedState};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryValueBackup {
    pub name: String,
    pub value_type: u32,
    pub value_bytes: Vec<u8>,
}

fn backup_registry_to_json(json_path: &Path) -> Result<(), AppError> {
    let backups = read_registry_snapshot_values()?;
    validate_registry_snapshot(&backups)?;

    let serialized = serde_json::to_string_pretty(&backups)
        .map_err(|e| AppError::FileError(format!("序列化注册表备份失败: {}", e)))?;
    std::fs::write(json_path, serialized)?;

    Ok(())
}

fn read_registry_snapshot_values() -> Result<Vec<RegistryValueBackup>, AppError> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey_with_flags(
        r"Software\Blizzard Entertainment\Battle.net\UnifiedAuth",
        KEY_READ,
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(AppError::RegistryError(format!(
                "打开 UnifiedAuth 注册表键失败: {error}"
            )))
        }
    };

    let mut backups = Vec::new();
    for value in key.enum_values() {
        let (name, raw_val) = value.map_err(|e| {
            AppError::RegistryError(format!("枚举 UnifiedAuth 注册表值失败: {}", e))
        })?;
        backups.push(RegistryValueBackup {
            name,
            value_type: raw_val.vtype as u32,
            value_bytes: raw_val.bytes,
        });
    }

    validate_registry_values(&backups, false)?;
    Ok(backups)
}

fn validate_registry_snapshot(backups: &[RegistryValueBackup]) -> Result<(), AppError> {
    validate_registry_values(backups, true)
}

fn validate_registry_values(
    backups: &[RegistryValueBackup],
    require_nonempty: bool,
) -> Result<(), AppError> {
    if require_nonempty && backups.is_empty() {
        return Err(AppError::RegistryError(
            "UnifiedAuth 注册表快照为空，拒绝精确替换".to_string(),
        ));
    }

    let mut names = std::collections::HashSet::new();
    for item in backups {
        if !matches!(item.value_type, 1 | 2 | 3 | 4 | 5 | 7) {
            return Err(AppError::RegistryError(format!(
                "UnifiedAuth 快照包含不支持的注册表类型 {}（键 {}）",
                item.value_type, item.name
            )));
        }
        if !names.insert(item.name.to_ascii_lowercase()) {
            return Err(AppError::RegistryError(format!(
                "UnifiedAuth 快照包含重复键名: {}",
                item.name
            )));
        }
    }
    Ok(())
}

fn replace_registry_snapshot_with<R, C, W>(
    backups: &[RegistryValueBackup],
    read_current: R,
    clear: C,
    write: W,
) -> Result<(), AppError>
where
    R: FnOnce() -> Result<Vec<RegistryValueBackup>, AppError>,
    C: Fn() -> Result<(), AppError>,
    W: Fn(&[RegistryValueBackup]) -> Result<(), AppError>,
{
    validate_registry_snapshot(backups)?;
    let original = read_current()?;
    validate_registry_values(&original, false)?;
    let rollback = || {
        clear().and_then(|_| {
            if original.is_empty() {
                Ok(())
            } else {
                write(&original)
            }
        })
    };
    if let Err(clear_error) = clear() {
        // No target values have been written yet. Rewriting the captured values is enough to
        // restore entries removed before the clear operation failed; retry clear only for an
        // originally empty key.
        let restore_result = if original.is_empty() {
            clear()
        } else {
            write(&original)
        };
        return match restore_result {
            Ok(()) => Err(AppError::RegistryError(format!(
                "清空 UnifiedAuth 失败，已恢复原注册表状态: {clear_error}"
            ))),
            Err(rollback_error) => Err(AppError::RegistryError(format!(
                "清空 UnifiedAuth 失败: {clear_error}；恢复原注册表也失败: {rollback_error}"
            ))),
        };
    }
    match write(backups) {
        Ok(()) => Ok(()),
        Err(write_error) => match rollback() {
            Ok(()) => Err(AppError::RegistryError(format!(
                "写入 UnifiedAuth 失败，已恢复原注册表状态: {write_error}"
            ))),
            Err(rollback_error) => Err(AppError::RegistryError(format!(
                "写入 UnifiedAuth 失败: {write_error}；恢复原注册表也失败: {rollback_error}"
            ))),
        },
    }
}

pub(crate) fn restore_registry_from_json(json_path: &Path) -> Result<(), AppError> {
    if !json_path.is_file() {
        return Err(AppError::FileError(format!(
            "UnifiedAuth 注册表快照不存在或不是文件: {}",
            json_path.display()
        )));
    }

    let content = std::fs::read_to_string(json_path)?;
    let backups: Vec<RegistryValueBackup> = serde_json::from_str(&content)
        .map_err(|e| AppError::FileError(format!("反序列化注册表备份失败: {}", e)))?;

    replace_registry_snapshot_with(
        &backups,
        read_registry_snapshot_values,
        clear_auth_registry_unlocked,
        write_registry_snapshot_values,
    )
}

fn write_registry_snapshot_values(backups: &[RegistryValueBackup]) -> Result<(), AppError> {
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(r"Software\Blizzard Entertainment\Battle.net\UnifiedAuth")
        .map_err(|e| AppError::RegistryError(format!("创建/打开注册表键失败: {}", e)))?;

    for item in backups.iter() {
        let val = RegValue {
            bytes: item.value_bytes.clone(),
            vtype: match item.value_type {
                1 => RegType::REG_SZ,
                2 => RegType::REG_EXPAND_SZ,
                3 => RegType::REG_BINARY,
                4 => RegType::REG_DWORD,
                5 => RegType::REG_DWORD_BIG_ENDIAN,
                7 => RegType::REG_MULTI_SZ,
                unsupported => {
                    return Err(AppError::RegistryError(format!(
                        "不支持的注册表值类型: {unsupported}"
                    )))
                }
            },
        };
        key.set_raw_value(&item.name, &val).map_err(|e| {
            AppError::RegistryError(format!("写入注册表值 {} 失败: {}", item.name, e))
        })?;
    }

    Ok(())
}

pub struct AccountManager;

pub(crate) struct AccountManagerCatalog<'a> {
    config: &'a GlobalConfig,
}

impl<'a> AccountManagerCatalog<'a> {
    pub(crate) fn new(config: &'a GlobalConfig) -> Self {
        Self { config }
    }

    fn synchronize_listing_mod_arguments(&self, account: &mut AccountMeta) -> Result<(), AppError> {
        if !account.initialized {
            return Ok(());
        }

        let battle_net_config =
            AccountManager::account_dir_checked(&self.config.accounts_dir, &account.id)?
                .join("Battle.net")
                .join("Battle.net.config");
        if !battle_net_config.exists() {
            return Ok(());
        }

        let Ok(context) =
            LaunchContext::for_account(self.config, account, ContextPurpose::Settings)
        else {
            return Ok(());
        };
        let game_key = context.edition.battle_net_config_game_key;
        if let Some(arguments) = try_read_mod_args(&battle_net_config, game_key) {
            account.mod_args = arguments;
        } else if !account.mod_args.is_empty() {
            account.mod_args.clear();
        }
        Ok(())
    }
}

impl AccountCatalog for AccountManagerCatalog<'_> {
    fn list_account_ids(&self) -> Result<Vec<String>, AppError> {
        Ok(AccountManager::list_ids(&self.config.accounts_dir))
    }

    fn list(&self) -> Result<Vec<AccountMeta>, AppError> {
        let mut accounts = Vec::new();
        for account_id in AccountManager::list_ids(&self.config.accounts_dir) {
            // Historical behavior intentionally keeps one damaged account from hiding every
            // healthy account in the dashboard.
            let Ok(mut account) = AccountManager::load_meta(&self.config.accounts_dir, &account_id)
            else {
                continue;
            };
            self.synchronize_listing_mod_arguments(&mut account)?;
            accounts.push(account);
        }
        Ok(accounts)
    }

    fn get(&self, account_id: &str) -> Result<AccountMeta, AppError> {
        AccountManager::load_meta(&self.config.accounts_dir, account_id)
    }
}

impl AccountRepository for AccountManagerCatalog<'_> {
    fn load(&self, account_id: &str) -> Result<AccountMeta, AppError> {
        AccountManager::load_meta(&self.config.accounts_dir, account_id)
    }

    fn save(&self, account: &AccountMeta) -> Result<(), AppError> {
        AccountManager::save_meta(&self.config.accounts_dir, account)
    }
}

impl AccountSettingsPreferenceRepository for AccountManagerCatalog<'_> {
    fn ensure_complete_snapshot(&self, account_id: &str) -> Result<(), AppError> {
        let account_dir =
            AccountManager::account_dir_checked(&self.config.accounts_dir, account_id)?;
        let path = account_dir.join("Settings.json");
        if !path.is_file() {
            return Err(AppError::FileError(format!(
                "账号 Settings.json 不存在: {}",
                path.display()
            )));
        }
        let settings: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        if settings.as_object().is_none_or(|object| object.is_empty()) {
            return Err(AppError::ConfigReadError(format!(
                "账号 Settings.json 为空或根节点不是对象: {}",
                path.display()
            )));
        }
        Ok(())
    }
}

impl AccountModRepository for AccountManagerCatalog<'_> {
    fn save_mod_configuration(&self, account: AccountMeta) -> Result<AccountMeta, AppError> {
        persist_account_mod_configuration(self.config, account)
    }
}

impl AccountNameRepository for AccountManagerCatalog<'_> {
    fn ensure_display_name_available(
        &self,
        requested_name: &str,
        excluded_account_id: Option<&str>,
    ) -> Result<(), AppError> {
        ensure_account_display_name_available(
            &self.config.accounts_dir,
            requested_name,
            excluded_account_id,
        )
    }
}

struct LaunchContextAccountProfilePolicy<'a> {
    config: &'a GlobalConfig,
}

impl AccountProfilePolicy for LaunchContextAccountProfilePolicy<'_> {
    fn resolve(&self, account: &AccountMeta) -> Result<ResolvedAccountProfile, AppError> {
        let context = LaunchContext::for_account(self.config, account, ContextPurpose::LaunchGame)?;
        Ok(ResolvedAccountProfile {
            auth_mode: context.auth_mode,
            game_region: context.game_region,
            client_edition: context.installation.edition,
            default_locale: context.region.default_locale,
        })
    }
}

struct CurrentUserTokenProtector;

impl TokenProtector for CurrentUserTokenProtector {
    fn protect(&self, plaintext: &str) -> Result<String, AppError> {
        let encrypted = crate::commands::crypto::protect_token(plaintext)
            .map_err(|error| AppError::Unknown(format!("Token 加密失败: {error}")))?;
        Ok(crate::commands::crypto::hex_encode(&encrypted))
    }
}

struct SystemTimestampProvider;

impl TimestampProvider for SystemTimestampProvider {
    fn now_rfc3339(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }
}

struct AccountCreationAdapter<'a> {
    config: &'a GlobalConfig,
    state: &'a SharedState,
}

impl AccountRepository for AccountCreationAdapter<'_> {
    fn load(&self, account_id: &str) -> Result<AccountMeta, AppError> {
        AccountManager::load_meta(&self.config.accounts_dir, account_id)
    }

    fn save(&self, account: &AccountMeta) -> Result<(), AppError> {
        AccountManager::save_meta(&self.config.accounts_dir, account)
    }
}

impl AccountNameRepository for AccountCreationAdapter<'_> {
    fn ensure_display_name_available(
        &self,
        requested_name: &str,
        excluded_account_id: Option<&str>,
    ) -> Result<(), AppError> {
        ensure_account_display_name_available(
            &self.config.accounts_dir,
            requested_name,
            excluded_account_id,
        )
    }
}

impl AccountCreationRepository for AccountCreationAdapter<'_> {
    fn next_account_id(&self) -> String {
        AccountManager::next_id(&self.config.accounts_dir)
    }

    fn create(&self, account: &AccountMeta) -> Result<(), AppError> {
        let context = LaunchContext::for_account(self.config, account, ContextPurpose::LaunchGame)?;
        let dir = AccountManager::account_dir_checked(&self.config.accounts_dir, &account.id)?;
        if path_exists(&dir)? {
            return Err(AppError::FileError(format!(
                "账号目录已存在: {}",
                dir.display()
            )));
        }
        let _host_runtime_lease = if context.auth_mode == AuthMode::Token {
            None
        } else {
            Some(HostRuntimeLease::try_acquire(self.state)?)
        };
        let staged = sibling_with_suffix(&dir, ".tmp")?;
        let backup = sibling_with_suffix(&dir, ".bak")?;
        remove_path_if_exists(&staged)?;
        std::fs::create_dir_all(&staged)?;
        if let Some(saved_games_directory) = context.installation.saved_games_directory.as_deref() {
            if let Err(error) =
                copy_system_settings_to_account_if_available(saved_games_directory, &staged)
            {
                crate::logger::log_msg(
                    "WARN",
                    "Account",
                    &format!("创建账号时跳过可选 Settings.json 快照: {error}"),
                );
            }
        } else {
            crate::logger::log_msg(
                "WARN",
                "Account",
                &format!(
                    "创建账号 {} 时未配置可用的存档目录；核心账号已创建，画质快照暂不可用",
                    account.id
                ),
            );
        }
        if let Err(error) = write_account_meta_to_directory(&staged, account) {
            let _ = remove_path_if_exists(&staged);
            return Err(error);
        }
        replace_path_with_backup(&staged, &dir, &backup)
    }
}

struct AccountManagerRuntime<'a> {
    state: &'a SharedState,
}

impl<'a> AccountManagerRuntime<'a> {
    fn new(state: &'a SharedState) -> Self {
        Self { state }
    }
}

impl AccountRuntimePort for AccountManagerRuntime<'_> {
    fn registered_pid(&self, account_id: &str) -> Option<u32> {
        self.state.multi_instance().instances().pid_for(account_id)
    }

    fn is_expected_game_process(
        &self,
        config: &GlobalConfig,
        account: &AccountMeta,
        pid: u32,
    ) -> bool {
        let Some(expected_game_path) =
            LaunchContext::for_account(config, account, ContextPurpose::Settings)
                .ok()
                .map(|context| context.installation.game_executable)
        else {
            return false;
        };

        let mut system = shared_system()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let system_pid = sysinfo::Pid::from(pid as usize);
        system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[system_pid]));
        let Some(process) = system.process(system_pid) else {
            return false;
        };
        process
            .name()
            .to_string_lossy()
            .eq_ignore_ascii_case("D2R.exe")
            && process.exe().is_some_and(|actual| {
                crate::commands::system::executable_paths_match(actual, &expected_game_path)
            })
    }

    fn remove_if_pid(&self, account_id: &str, pid: u32) -> bool {
        self.state
            .multi_instance()
            .instances()
            .remove_if_pid(account_id, pid)
    }
}

impl AccountManager {
    pub fn is_valid_account_id(id: &str) -> bool {
        crate::domain::account::is_valid_account_id(id)
    }

    pub fn validate_account_id(id: &str) -> Result<(), AppError> {
        if Self::is_valid_account_id(id) {
            Ok(())
        } else {
            Err(AppError::FileError(format!("账号 ID 非法: {}", id)))
        }
    }

    /// 获取账号目录路径
    pub fn account_dir_checked(accounts_dir: &str, id: &str) -> Result<PathBuf, AppError> {
        Self::validate_account_id(id)?;
        if accounts_dir.trim().is_empty() {
            return Err(AppError::FileError("账号根目录为空".to_string()));
        }

        let root = Path::new(accounts_dir);
        let root_abs = if root.exists() {
            root.canonicalize()?
        } else if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()?.join(root)
        };
        let dir = root_abs.join(id);
        if !dir.starts_with(&root_abs) {
            return Err(AppError::FileError(format!("账号目录越界: {}", id)));
        }
        Ok(dir)
    }

    /// 加载单个账号的元信息
    pub fn load_meta(accounts_dir: &str, id: &str) -> Result<AccountMeta, AppError> {
        let account_dir = Self::account_dir_checked(accounts_dir, id)?;
        recover_interrupted_replacement(&account_dir)?;
        let path = account_dir.join("account.json");
        recover_interrupted_replacement(&path)?;
        if !path.exists() {
            return Err(AppError::AccountNotFound(id.to_string()));
        }
        let content = std::fs::read_to_string(&path)?;
        let mut meta: AccountMeta = serde_json::from_str(&content)?;

        // --- 兼容性适配 ---
        // 仅当 mod_list 为空（旧版本单 mod 格式）时，将 mod_args 迁移到 mod_list
        if meta.mod_list.is_empty() && !meta.mod_args.trim().is_empty() {
            meta.mod_list.push(meta.mod_args.clone());
            // 确保 active mod 与列表一致
            if !meta.mod_list.contains(&meta.mod_args) {
                meta.mod_args = meta.mod_list[0].clone();
            }
        }

        meta.normalize_legacy_window_position();

        if let Some(account_dir) = path.parent() {
            hydrate_meta_from_runtime_snapshot(account_dir, &mut meta);
        }

        Ok(meta)
    }

    /// 保存账号元信息
    pub fn save_meta(accounts_dir: &str, meta: &AccountMeta) -> Result<(), AppError> {
        let dir = Self::account_dir_checked(accounts_dir, &meta.id)?;
        if !dir.is_dir() {
            return Err(AppError::AccountNotFound(meta.id.clone()));
        }
        let path = dir.join("account.json");
        let staged = sibling_with_suffix(&path, ".tmp")?;
        let backup = sibling_with_suffix(&path, ".bak")?;
        remove_path_if_exists(&staged)?;
        let content = serde_json::to_string_pretty(meta)?;
        std::fs::write(&staged, content)?;
        replace_path_with_backup(&staged, &path, &backup).inspect_err(|_error| {
            let _ = remove_path_if_exists(&staged);
        })
    }

    /// 列出所有已存在的账号 ID（通过扫描 accounts 目录）
    pub fn list_ids(accounts_dir: &str) -> Vec<String> {
        let dir = Path::new(accounts_dir);
        if !dir.exists() {
            return vec![];
        }
        recover_interrupted_account_directories(dir);
        let mut ids: Vec<String> = vec![];
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if Self::is_valid_account_id(&name) {
                        ids.push(name);
                    }
                }
            }
        }
        ids.sort();
        ids
    }

    /// 新账号使用不可复用的稳定 ID。旧的 `acountN` 目录继续只读兼容，
    /// 但删除账号后不能让新的浏览器 Profile 继承旧账号 Cookie。
    pub fn next_id(_accounts_dir: &str) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

pub(crate) fn normalized_account_display_name(name: &str) -> String {
    normalize_account_display_name(name)
}

fn ensure_account_display_name_available(
    accounts_dir: &str,
    requested_name: &str,
    excluded_account_id: Option<&str>,
) -> Result<(), AppError> {
    let requested_identity = normalized_account_display_name(requested_name);
    for id in AccountManager::list_ids(accounts_dir) {
        if excluded_account_id.is_some_and(|excluded| id.eq_ignore_ascii_case(excluded)) {
            continue;
        }
        let Ok(meta) = AccountManager::load_meta(accounts_dir, &id) else {
            continue;
        };
        let existing_name = if meta.display_name.trim().is_empty() {
            meta.id.as_str()
        } else {
            meta.display_name.as_str()
        };
        if normalized_account_display_name(existing_name) == requested_identity {
            return Err(AppError::AccountAlreadyExists(
                requested_name.trim().to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn copy_system_settings_to_account_if_available(
    saved_games_path: &Path,
    account_dir: &Path,
) -> Result<bool, AppError> {
    let src = saved_games_path.join("Settings.json");
    if !src.is_file() {
        return Ok(false);
    }

    if !account_dir.exists() {
        std::fs::create_dir_all(account_dir)?;
    }

    std::fs::copy(&src, account_dir.join("Settings.json"))
        .map_err(|e| AppError::FileError(format!("复制 Settings.json 失败: {}", e)))?;
    Ok(true)
}

pub(crate) fn copy_account_settings_to_system(
    account_dir: &Path,
    saved_games_path: &Path,
) -> Result<(), AppError> {
    let src = account_dir.join("Settings.json");
    if !src.is_file() {
        return Err(AppError::FileError(format!(
            "账号 Settings.json 不存在: {}。请先在画质配置中从系统配置创建账号独立配置",
            src.display()
        )));
    }
    if !saved_games_path.is_dir() {
        return Err(AppError::FileError(format!(
            "存档目录无效: {}。请在设置中修正存档目录",
            saved_games_path.display()
        )));
    }

    std::fs::copy(&src, saved_games_path.join("Settings.json"))
        .map_err(|e| AppError::FileError(format!("复制 Settings.json 失败: {}", e)))?;
    Ok(())
}

struct AccountDeletionTransactionAdapter<'a> {
    app: &'a tauri::AppHandle,
    state: &'a SharedState,
}

impl AccountDeletionTransaction for AccountDeletionTransactionAdapter<'_> {
    fn delete(&self, requested_account_id: &str) -> Result<String, AppError> {
        let staged_deletion = RefCell::new(None);
        let post_commit_outcome = RefCell::new(None);
        let deleted_account_id = RefCell::new(None);
        let mutation_result =
            crate::commands::global_config::mutate_loaded_global_config_with_post_commit(
                self.state,
                self.app,
                |cfg| {
                    let stored_account_id = AccountManager::list_ids(&cfg.accounts_dir)
                        .into_iter()
                        .find(|stored_id| stored_id.eq_ignore_ascii_case(requested_account_id))
                        .ok_or_else(|| {
                            AppError::AccountNotFound(requested_account_id.to_string())
                        })?;
                    let dir =
                        AccountManager::account_dir_checked(&cfg.accounts_dir, &stored_account_id)?;
                    let had_configuration_references =
                        config_references_account(cfg, &stored_account_id);
                    *staged_deletion.borrow_mut() = Some(stage_account_directory_for_deletion(
                        &dir,
                        &stored_account_id,
                        had_configuration_references,
                    )?);
                    *deleted_account_id.borrow_mut() = Some(stored_account_id.clone());
                    let cleared_audio_target = cfg
                        .rune_audio_target_account
                        .trim()
                        .eq_ignore_ascii_case(&stored_account_id);
                    if cleared_audio_target {
                        cfg.rune_audio_enabled = false;
                        cfg.rune_audio_target_account.clear();
                    }
                    let removed_from_launch_group =
                        cfg.remove_account_from_launch_groups(&stored_account_id);
                    Ok(cleared_audio_target || removed_from_launch_group)
                },
                |_| {
                    let mut staged_deletion = staged_deletion.borrow_mut();
                    let outcome = match staged_deletion.as_mut() {
                        None => Err(AppError::FileError(
                            "账号删除事务未能创建目录暂存记录".to_string(),
                        )),
                        Some(staged_deletion) => {
                            let completion = complete_staged_account_deletion_after_config_commit(
                                staged_deletion,
                            );
                            if completion.should_retire_account_id {
                                if let Some(account_id) = deleted_account_id.borrow().as_deref() {
                                    self.state.retire_account_id(account_id);
                                }
                            }
                            completion.result
                        }
                    };
                    *post_commit_outcome.borrow_mut() = Some(outcome);
                },
            );
        let staged_deletion = staged_deletion.into_inner();
        let mutation = match mutation_result {
            Ok(mutation) => mutation,
            Err(error) => {
                return Err(match staged_deletion.as_ref() {
                    Some(staged_deletion) => {
                        rollback_staged_account_deletion(staged_deletion, error)
                    }
                    None => error,
                });
            }
        };
        match mutation {
            ConfigurationMutation::Missing => {
                return Err(AppError::ConfigReadError("尚未完成首次配置".to_string()));
            }
            ConfigurationMutation::Unchanged | ConfigurationMutation::Updated => {}
        }
        let deleted_account_id = deleted_account_id
            .into_inner()
            .ok_or_else(|| AppError::FileError("账号删除事务未记录实际账号 ID".to_string()))?;

        post_commit_outcome
            .into_inner()
            .ok_or_else(|| AppError::FileError("账号删除事务未执行提交后目录处理".to_string()))??;
        Ok(deleted_account_id)
    }
}

struct AccountDeletionCleanupAdapter<'a> {
    state: &'a SharedState,
}

impl AccountDeletionCleanupPort for AccountDeletionCleanupAdapter<'_> {
    fn remove_browser_profiles(&self, account_id: &str) -> Result<(), String> {
        crate::commands::browser::remove_browser_profiles_for_account(account_id)
            .map_err(|error| error.to_string())
    }

    fn remove_runtime_instance(&self, account_id: &str) {
        self.state.multi_instance().instances().remove(account_id);
    }

    fn notify_account_removed(&self, account_id: &str) -> Vec<(String, String)> {
        self.state
            .capabilities()
            .notify_account_removed(account_id)
            .into_iter()
            .map(|(capability_id, failure)| (capability_id.to_string(), failure.message))
            .collect()
    }
}

// ── Tauri Commands ──

/// 获取所有账号列表
#[tauri::command]
pub fn list_accounts(state: tauri::State<'_, SharedState>) -> Result<Vec<AccountMeta>, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    let account_catalog = AccountManagerCatalog::new(&cfg);
    let account_runtime = AccountManagerRuntime::new(state.inner());
    AccountQueryService::new(&account_catalog, &account_runtime).list(&cfg)
}

/// 重新排序账号（更新 order 字段）。校验、冲突控制与回滚由核心应用用例负责。
#[tauri::command]
pub fn reorder_accounts(
    state: tauri::State<'_, SharedState>,
    ordered_ids: Vec<String>,
) -> Result<(), AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    AccountOrderingService::new(&repository, state.multi_instance().account_leases())
        .reorder(&ordered_ids)
}

/// 打开账号配置目录（直接用 Explorer 打开）
#[tauri::command]
pub fn open_account_dir(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<(), AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let dir = AccountManager::account_dir_checked(&cfg.accounts_dir, &account_id)?;

    if !dir.exists() {
        return Err(AppError::AccountNotFound(account_id));
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| AppError::FileError(format!("打开目录失败: {}", e)))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| AppError::FileError(format!("打开目录失败: {}", e)))?;
    }
    Ok(())
}

/// 获取账号配置目录路径
#[tauri::command]
pub fn get_account_dir_path(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<String, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let dir = AccountManager::account_dir_checked(&cfg.accounts_dir, &account_id)?;
    Ok(dir.to_string_lossy().to_string())
}

/// 获取单个账号信息
#[tauri::command]
pub fn get_account(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<AccountMeta, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let account_catalog = AccountManagerCatalog::new(&cfg);
    let account_runtime = AccountManagerRuntime::new(state.inner());
    AccountQueryService::new(&account_catalog, &account_runtime).get(&account_id)
}

/// 创建新账号目录并返回 ID
#[tauri::command]
pub fn create_account(
    state: tauri::State<'_, SharedState>,
    nickname: String,
    auth_mode: Option<String>,
    region: Option<String>,
    token: Option<String>,
    language: Option<String>,
    voicelanguage: Option<String>,
) -> Result<String, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    let repository = AccountCreationAdapter {
        config: &cfg,
        state: state.inner(),
    };
    let policy = LaunchContextAccountProfilePolicy { config: &cfg };
    AccountCreationService::new(
        &repository,
        state.multi_instance().catalog_leases(),
        state.multi_instance().account_leases(),
        &policy,
        &CurrentUserTokenProtector,
        &SystemTimestampProvider,
    )
    .create(CreateAccountRequest {
        display_name: nickname,
        auth_mode,
        token,
        region,
        language,
        voice_language: voicelanguage,
    })
}

/// 更新已创建的账号的 Token / 语言 / 区服等字段
#[tauri::command]
pub fn update_account_meta(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    auth_mode: Option<String>,
    token: Option<String>,
    region: Option<String>,
    language: Option<String>,
    voicelanguage: Option<String>,
) -> Result<(), AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    let policy = LaunchContextAccountProfilePolicy { config: &cfg };
    AccountProfileService::new(
        &repository,
        state.multi_instance().account_leases(),
        &policy,
        &CurrentUserTokenProtector,
    )
    .update(
        &account_id,
        AccountProfilePatch {
            auth_mode,
            token,
            region,
            language,
            voice_language: voicelanguage,
        },
    )
}

/// 在共用同一套客户端与 Token 的国际服服务器之间切换。
#[tauri::command]
pub fn update_account_region(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    region: String,
) -> Result<(), AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    let policy = LaunchContextAccountProfilePolicy { config: &cfg };
    AccountProfileService::new(
        &repository,
        state.multi_instance().account_leases(),
        &policy,
        &CurrentUserTokenProtector,
    )
    .switch_international_region(&account_id, &region)
}

/// 删除账号
#[tauri::command]
pub fn delete_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<(), AppError> {
    let transaction = AccountDeletionTransactionAdapter {
        app: &app,
        state: state.inner(),
    };
    let cleanup = AccountDeletionCleanupAdapter {
        state: state.inner(),
    };
    let outcome = AccountDeletionService::new(
        &transaction,
        &cleanup,
        state.multi_instance().catalog_leases(),
        state.multi_instance().account_leases(),
    )
    .delete(&account_id)?;
    for warning in outcome.warnings {
        log::warn!(
            "账号 {} 已删除，但 {} 的提交后清理失败: {}",
            outcome.account_id,
            warning.component,
            warning.message
        );
    }
    Ok(())
}

/// 重命名账号仅修改展示名；浏览器 Profile 始终由稳定 account_id 标识。
#[tauri::command]
pub fn rename_account(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    new_name: String,
) -> Result<AccountMeta, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    let repository = AccountManagerCatalog::new(&cfg);
    AccountNamingService::new(
        &repository,
        state.multi_instance().catalog_leases(),
        state.multi_instance().account_leases(),
    )
    .rename(&account_id, &new_name)
}

fn persist_account_mod_configuration(
    cfg: &GlobalConfig,
    meta: AccountMeta,
) -> Result<AccountMeta, AppError> {
    let context = LaunchContext::for_account(cfg, &meta, ContextPurpose::LaunchGame)?;

    let account_dir = AccountManager::account_dir_checked(&cfg.accounts_dir, &meta.id)?;
    let pending = stage_account_directory(&account_dir)?;
    let update_result = (|| -> Result<(), AppError> {
        if context.auth_mode == AuthMode::BattleNet && meta.initialized {
            let snapshot = resolve_account_runtime_snapshot(
                &pending.staged_root,
                &meta,
                context.installation.edition,
            )?;
            update_mod_args(
                &snapshot.bnet_directory.join("Battle.net.config"),
                context.edition.battle_net_config_game_key,
                &meta.mod_args,
            )?;
        }
        write_account_meta_to_directory(&pending.staged_root, &meta)
    })();
    if let Err(error) = update_result {
        pending.discard();
        return Err(error);
    }
    pending.commit()?;

    Ok(meta)
}

/// 新增一条 Mod 胶囊配置。完全相同的配置会被安全跳过，而不是作为错误返回。
#[tauri::command]
pub fn add_account_mod(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    mod_configuration: String,
) -> Result<bool, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    AccountModService::new(&repository, state.multi_instance().account_leases())
        .add(&account_id, &mod_configuration)
}

/// 设置账号的当前 Mod 及完整胶囊列表；传入的重复项会按首次出现顺序静默合并。
#[tauri::command]
pub fn update_account_mods(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    active_mod: String,
    mod_list: Vec<String>,
) -> Result<AccountMeta, AppError> {
    update_account_mods_inner(state.inner(), account_id, active_mod, mod_list)
}

pub(crate) fn update_account_mods_inner(
    state: &SharedState,
    account_id: String,
    active_mod: String,
    mod_list: Vec<String>,
) -> Result<AccountMeta, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    AccountModService::new(&repository, state.multi_instance().account_leases()).replace(
        &account_id,
        active_mod,
        mod_list,
    )
}

/// 标记账号已自定义过设置（用于前端引导提示）
#[tauri::command]
pub fn mark_settings_customized(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<(), AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    AccountSettingsPreferenceService::new(&repository, state.multi_instance().account_leases())
        .set_customized(&account_id, true)
}

/// 设置账号是否使用独立 Settings.json 覆盖系统游戏配置
#[tauri::command]
pub fn set_settings_customized(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    customized: bool,
) -> Result<(), AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    AccountSettingsPreferenceService::new(&repository, state.multi_instance().account_leases())
        .set_customized(&account_id, customized)
}

/// 设置账号的窗口位置（持久化到 account.json）
#[tauri::command]
pub fn set_account_window_position(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    window_x: Option<i32>,
    window_y: Option<i32>,
) -> Result<AccountMeta, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    AccountPositionService::new(&repository, state.multi_instance().account_leases())
        .set_window_position(&account_id, window_x, window_y)
}

/// 保存账号的位置胶囊库与主界面默认选择，并同步旧版 window_x/window_y 字段。
#[tauri::command]
pub fn update_account_positions(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    active_position_id: Option<String>,
    position_presets: Vec<WindowPositionPreset>,
) -> Result<AccountMeta, AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let repository = AccountManagerCatalog::new(&cfg);
    AccountPositionService::new(&repository, state.multi_instance().account_leases())
        .replace_positions(&account_id, active_position_id, position_presets)
}

/// 初始化/重新初始化完成后：杀战网、清空 UnifiedAuth 注册表
fn cleanup_after_snapshot() -> Result<(), AppError> {
    // 调用方必须持有 HostRuntimeLease；即使终止进程失败，也要尝试清空认证注册表。
    let process_result = kill_processes_by_name(&["Battle.net.exe", "Agent.exe"]);
    let registry_result = clear_auth_registry_unlocked();
    match (process_result, registry_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(process), Ok(())) => Err(process),
        (Ok(()), Err(registry)) => Err(registry),
        (Err(process), Err(registry)) => Err(AppError::Unknown(format!(
            "终止共享进程失败: {process}；清空 UnifiedAuth 也失败: {registry}"
        ))),
    }
}

#[cfg(test)]
mod settings_json_tests {
    use super::{
        commit_account_settings_transaction, complete_staged_account_deletion_after_config_commit,
        config_references_account, copy_account_settings_to_system,
        copy_system_settings_to_account_if_available, ensure_account_display_name_available,
        hydrate_meta_from_runtime_snapshot, mark_account_deletion_committed,
        normalized_account_display_name, prepare_battle_net_runtime_directory,
        recover_account_transactions, remove_account_directory_without_resurrection,
        replace_battle_net_snapshot, replace_path_with_backup, replace_registry_snapshot_with,
        resolve_account_runtime_snapshot, restore_staged_account_deletion, sibling_with_suffix,
        stage_account_directory, stage_account_directory_for_deletion,
        validate_runtime_snapshot_root, AccountDeletionPhase, AccountManager, AccountMeta,
        BnetInitializationKind, RegistryValueBackup, ACCOUNT_RUNTIME_SNAPSHOT_SCHEMA,
    };
    use crate::domain::account::ClientEdition;
    use crate::domain::config::GlobalConfig;
    use crate::error::AppError;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = format!(
            "d2rhub_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn legacy_window_coordinates_become_a_position_capsule_without_losing_old_fields() {
        let accounts = temp_dir("legacy_window_position");
        let account_dir = accounts.join("acount1");
        std::fs::create_dir_all(&account_dir).unwrap();
        std::fs::write(
            account_dir.join("account.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "acount1",
                "display_name": "旧账号",
                "window_x": 128,
                "window_y": 256
            }))
            .unwrap(),
        )
        .unwrap();

        let meta = AccountManager::load_meta(accounts.to_str().unwrap(), "acount1").unwrap();

        assert_eq!((meta.window_x, meta.window_y), (Some(128), Some(256)));
        assert_eq!(meta.position_presets.len(), 1);
        assert_eq!(meta.position_presets[0].name, "原位置");
        assert_eq!(
            (meta.position_presets[0].x, meta.position_presets[0].y),
            (128, 256)
        );
        assert_eq!(
            meta.active_position_id.as_deref(),
            Some(meta.position_presets[0].id.as_str())
        );

        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn legacy_account_without_complete_coordinates_keeps_position_disabled() {
        let accounts = temp_dir("legacy_window_position_disabled");
        let account_dir = accounts.join("acount1");
        std::fs::create_dir_all(&account_dir).unwrap();
        std::fs::write(
            account_dir.join("account.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "acount1",
                "window_x": 128
            }))
            .unwrap(),
        )
        .unwrap();

        let meta = AccountManager::load_meta(accounts.to_str().unwrap(), "acount1").unwrap();

        assert!(meta.position_presets.is_empty());
        assert!(meta.active_position_id.is_none());
        assert_eq!((meta.window_x, meta.window_y), (Some(128), None));

        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn account_display_names_are_unique_after_trimming_and_case_folding() {
        let root = temp_dir("unique_display_name");
        let accounts = root.join("accounts");
        std::fs::create_dir_all(accounts.join("acount1")).unwrap();
        let mut existing = AccountMeta::new("acount1");
        existing.display_name = "Primary Account".to_string();
        AccountManager::save_meta(accounts.to_str().unwrap(), &existing).unwrap();

        assert!(ensure_account_display_name_available(
            accounts.to_str().unwrap(),
            "  primary account  ",
            None,
        )
        .is_err());
        assert!(ensure_account_display_name_available(
            accounts.to_str().unwrap(),
            "PRIMARY ACCOUNT",
            Some("acount1"),
        )
        .is_ok());
        assert!(ensure_account_display_name_available(
            accounts.to_str().unwrap(),
            "Secondary Account",
            None,
        )
        .is_ok());
        assert_eq!(
            normalized_account_display_name("  Primary Account  "),
            "primary account"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    fn write_valid_runtime_snapshot(root: &std::path::Path, edition: &str) {
        let bnet = root.join("Battle.net");
        std::fs::create_dir_all(&bnet).unwrap();
        let product_key = if edition == "CN" { "osic" } else { "osi" };
        let games = serde_json::Map::from_iter([(product_key.to_string(), serde_json::json!({}))]);
        std::fs::write(
            bnet.join("Battle.net.config"),
            serde_json::to_vec(&serde_json::json!({
                "Client": {},
                "Games": games
            }))
            .unwrap(),
        )
        .unwrap();
        let registry = vec![RegistryValueBackup {
            name: "account".to_string(),
            value_type: 1,
            value_bytes: vec![1],
        }];
        std::fs::write(
            root.join("unified_auth.json"),
            serde_json::to_string(&registry).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("snapshot.json"),
            serde_json::to_string(&serde_json::json!({
                "schema_version": ACCOUNT_RUNTIME_SNAPSHOT_SCHEMA,
                "edition": edition,
                "created_at": "2026-08-24T00:00:00Z"
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn write_legacy_snapshot(root: &std::path::Path, game_keys: &[&str]) {
        let bnet = root.join("Battle.net");
        std::fs::create_dir_all(&bnet).unwrap();
        let games = game_keys
            .iter()
            .map(|key| ((*key).to_string(), serde_json::json!({})))
            .collect::<serde_json::Map<_, _>>();
        std::fs::write(
            bnet.join("Battle.net.config"),
            serde_json::to_vec(&serde_json::json!({ "Games": games })).unwrap(),
        )
        .unwrap();
        let registry = vec![RegistryValueBackup {
            name: "account".to_string(),
            value_type: 1,
            value_bytes: vec![1],
        }];
        std::fs::write(
            root.join("unified_auth.json"),
            serde_json::to_vec(&registry).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn missing_system_settings_is_optional_for_account_creation() {
        let saved_games = temp_dir("missing_system_settings");
        let account_dir = temp_dir("account_without_settings");

        let copied =
            copy_system_settings_to_account_if_available(&saved_games, &account_dir).unwrap();

        assert!(!copied);
        assert!(!account_dir.join("Settings.json").exists());
        let _ = std::fs::remove_dir_all(saved_games);
        let _ = std::fs::remove_dir_all(account_dir);
    }

    #[test]
    fn existing_system_settings_is_copied_to_account() {
        let saved_games = temp_dir("existing_system_settings");
        let account_dir = temp_dir("account_with_settings");
        std::fs::write(saved_games.join("Settings.json"), r#"{"VSync":1}"#).unwrap();

        let copied =
            copy_system_settings_to_account_if_available(&saved_games, &account_dir).unwrap();

        assert!(copied);
        assert_eq!(
            std::fs::read_to_string(account_dir.join("Settings.json")).unwrap(),
            r#"{"VSync":1}"#
        );
        let _ = std::fs::remove_dir_all(saved_games);
        let _ = std::fs::remove_dir_all(account_dir);
    }

    #[test]
    fn customized_settings_requires_an_account_settings_file() {
        let saved_games = temp_dir("customized_settings_target");
        let account_dir = temp_dir("customized_settings_source");

        let error = copy_account_settings_to_system(&account_dir, &saved_games).unwrap_err();

        assert!(error.to_string().contains("账号 Settings.json 不存在"));
        let _ = std::fs::remove_dir_all(saved_games);
        let _ = std::fs::remove_dir_all(account_dir);
    }

    #[test]
    fn mod_arguments_are_written_to_the_matching_edition() {
        let root = temp_dir("region_specific_mod_args");
        let config_path = root.join("Battle.net.config");
        std::fs::write(&config_path, r#"{"Games":{"osic":{},"osi":{}}}"#).unwrap();

        crate::battle_net_config::update_mod_args(&config_path, "osi", "-mod global").unwrap();

        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(
            config["Games"]["osi"]["AdditionalLaunchArguments"],
            "-mod global"
        );
        assert!(config["Games"]["osic"]["AdditionalLaunchArguments"].is_null());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn atomic_replace_replaces_an_existing_file() {
        let root = temp_dir("atomic_file_replace");
        let staged = root.join("unified_auth.json.tmp");
        let target = root.join("unified_auth.json");
        let backup = root.join("unified_auth.json.bak");
        std::fs::write(&staged, "new").unwrap();
        std::fs::write(&target, "old").unwrap();

        replace_path_with_backup(&staged, &target, &backup).unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert!(!staged.exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn atomic_replace_replaces_a_directory_and_cleans_stale_backup() {
        let root = temp_dir("atomic_dir_replace");
        let staged = root.join("Battle.net.tmp");
        let target = root.join("Battle.net");
        let backup = root.join("Battle.net.bak");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(staged.join("new.txt"), "new").unwrap();
        std::fs::write(target.join("old.txt"), "old").unwrap();
        std::fs::write(backup.join("stale.txt"), "stale").unwrap();

        replace_path_with_backup(&staged, &target, &backup).unwrap();

        assert_eq!(
            std::fs::read_to_string(target.join("new.txt")).unwrap(),
            "new"
        );
        assert!(!target.join("old.txt").exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn atomic_replace_recovers_backup_when_target_is_missing() {
        let root = temp_dir("atomic_replace_interrupted");
        let staged = root.join("runtime.tmp");
        let target = root.join("runtime");
        let backup = root.join("runtime.bak");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(staged.join("new.txt"), "new").unwrap();
        std::fs::write(backup.join("old.txt"), "old").unwrap();

        replace_path_with_backup(&staged, &target, &backup).unwrap();

        assert_eq!(
            std::fs::read_to_string(target.join("new.txt")).unwrap(),
            "new"
        );
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn account_directory_staging_commits_metadata_and_runtime_together() {
        let root = temp_dir("account_directory_transaction");
        let account = root.join("acount1");
        std::fs::create_dir_all(account.join("runtime")).unwrap();
        std::fs::write(account.join("account.json"), "old-meta").unwrap();
        std::fs::write(account.join("runtime").join("old.txt"), "old").unwrap();

        let pending = stage_account_directory(&account).unwrap();
        std::fs::write(pending.staged_root.join("account.json"), "new-meta").unwrap();
        std::fs::write(pending.staged_root.join("runtime").join("new.txt"), "new").unwrap();
        pending.commit().unwrap();

        assert_eq!(
            std::fs::read_to_string(account.join("account.json")).unwrap(),
            "new-meta"
        );
        assert!(account.join("runtime").join("new.txt").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn settings_and_customization_flag_commit_in_one_account_transaction() {
        let accounts = temp_dir("settings_account_transaction");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        let mut meta = AccountMeta::new("acount1");
        AccountManager::save_meta(accounts.to_str().unwrap(), &meta).unwrap();
        std::fs::write(account.join("Settings.json"), r#"{"old":true}"#).unwrap();
        meta.has_customized_settings = true;

        commit_account_settings_transaction(accounts.to_str().unwrap(), &meta, r#"{"new":true}"#)
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(account.join("Settings.json")).unwrap(),
            r#"{"new":true}"#
        );
        assert!(
            AccountManager::load_meta(accounts.to_str().unwrap(), "acount1")
                .unwrap()
                .has_customized_settings
        );
        assert!(!accounts.join("acount1.tmp").exists());
        assert!(!accounts.join("acount1.bak").exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn new_account_ids_are_unique_even_before_a_directory_is_created() {
        let accounts = temp_dir("unique_account_ids");

        let first = AccountManager::next_id(accounts.to_str().unwrap());
        let second = AccountManager::next_id(accounts.to_str().unwrap());

        assert!(AccountManager::is_valid_account_id(&first));
        assert!(AccountManager::is_valid_account_id(&second));
        assert!(!first.starts_with("acount"));
        assert_ne!(first, second);
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn runtime_snapshot_rejects_a_different_client_edition() {
        let root = temp_dir("runtime_edition_mismatch");
        write_valid_runtime_snapshot(&root, "CN");

        let error = validate_runtime_snapshot_root(&root, ClientEdition::Global).unwrap_err();

        assert!(error.to_string().contains("拒绝跨版本恢复"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_snapshot_rejects_corrupt_battle_net_config() {
        let root = temp_dir("runtime_corrupt_bnet_config");
        write_valid_runtime_snapshot(&root, "Global");
        std::fs::write(root.join("Battle.net").join("Battle.net.config"), "{").unwrap();

        let error = validate_runtime_snapshot_root(&root, ClientEdition::Global).unwrap_err();

        assert!(error.to_string().contains("JSON 无效"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_snapshot_requires_the_expected_product_key() {
        let root = temp_dir("runtime_wrong_product_key");
        write_valid_runtime_snapshot(&root, "Global");
        std::fs::write(
            root.join("Battle.net").join("Battle.net.config"),
            r#"{"Games":{"osic":{}}}"#,
        )
        .unwrap();

        let error = validate_runtime_snapshot_root(&root, ClientEdition::Global).unwrap_err();

        assert!(error.to_string().contains("产品键 osi"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_snapshot_without_provenance_requires_a_unique_matching_product_key() {
        let matching = temp_dir("legacy_matching_product");
        write_legacy_snapshot(&matching, &["osi"]);
        let mut meta = AccountMeta::new("acount1");
        meta.initialized = true;
        assert!(resolve_account_runtime_snapshot(&matching, &meta, ClientEdition::Global).is_ok());

        let mismatch = temp_dir("legacy_mismatched_product");
        write_legacy_snapshot(&mismatch, &["osic"]);
        assert!(resolve_account_runtime_snapshot(&mismatch, &meta, ClientEdition::Global).is_err());

        let ambiguous = temp_dir("legacy_ambiguous_product");
        write_legacy_snapshot(&ambiguous, &["osi", "osic"]);
        assert!(
            resolve_account_runtime_snapshot(&ambiguous, &meta, ClientEdition::Global).is_err()
        );

        let _ = std::fs::remove_dir_all(matching);
        let _ = std::fs::remove_dir_all(mismatch);
        let _ = std::fs::remove_dir_all(ambiguous);
    }

    #[test]
    fn valid_runtime_snapshot_is_the_authoritative_initialized_state() {
        let account_dir = temp_dir("runtime_hydrates_meta");
        write_valid_runtime_snapshot(&account_dir.join("runtime"), "Global");
        let mut meta = AccountMeta::new("acount1");
        meta.region = Some("NA".to_string());
        meta.auth_mode = Some("bnet".to_string());

        hydrate_meta_from_runtime_snapshot(&account_dir, &mut meta);

        assert!(meta.initialized);
        assert_eq!(meta.snapshot_edition.as_deref(), Some("Global"));
        assert_eq!(meta.last_reset_at.as_deref(), Some("2026-08-24T00:00:00Z"));
        let _ = std::fs::remove_dir_all(account_dir);
    }

    #[test]
    fn account_listing_recovers_an_interrupted_directory_swap() {
        let accounts = temp_dir("account_list_recovers_backup");
        let backup = accounts.join("acount1.bak");
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(backup.join("account.json"), "{}").unwrap();

        let ids = AccountManager::list_ids(accounts.to_str().unwrap());

        assert_eq!(ids, vec!["acount1".to_string()]);
        assert!(accounts.join("acount1").is_dir());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn startup_recovery_rolls_back_when_backup_and_staging_both_exist() {
        let accounts = temp_dir("startup_recovers_account_transaction");
        let backup = accounts.join("acount1.bak");
        let staged = accounts.join("acount1.tmp");
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(backup.join("account.json"), "old").unwrap();
        std::fs::write(staged.join("account.json"), "new").unwrap();

        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert_eq!(
            std::fs::read_to_string(accounts.join("acount1").join("account.json")).unwrap(),
            "old"
        );
        assert!(!backup.exists());
        assert!(!staged.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn save_meta_never_recreates_a_deleted_account_directory() {
        let accounts = temp_dir("save_meta_no_resurrection");
        let meta = AccountMeta::new("acount1");

        let error = AccountManager::save_meta(accounts.to_str().unwrap(), &meta).unwrap_err();

        assert!(matches!(error, AppError::AccountNotFound(_)));
        assert!(!accounts.join("acount1").exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn permanent_delete_cannot_be_undone_by_startup_recovery() {
        let accounts = temp_dir("delete_no_backup_resurrection");
        let account = accounts.join("acount1");
        let staged = accounts.join("acount1.tmp");
        let backup = accounts.join("acount1.bak");
        for directory in [&account, &staged, &backup] {
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(directory.join("account.json"), "{}").unwrap();
        }

        remove_account_directory_without_resurrection(&account, "acount1").unwrap();
        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert!(!account.exists());
        assert!(!staged.exists());
        assert!(!backup.exists());
        assert!(AccountManager::list_ids(accounts.to_str().unwrap()).is_empty());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn staged_account_deletion_can_be_rolled_back_before_commit() {
        let accounts = temp_dir("delete_transaction_rollback");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "recoverable").unwrap();

        let staged = stage_account_directory_for_deletion(&account, "acount1", false).unwrap();
        assert!(!account.exists());
        assert!(staged.staged_dir.is_dir());

        restore_staged_account_deletion(&staged).unwrap();
        assert_eq!(
            std::fs::read_to_string(account.join("account.json")).unwrap(),
            "recoverable"
        );
        assert!(!staged.staged_dir.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn deletion_rollback_preserves_conflicting_directories_and_journal() {
        let accounts = temp_dir("delete_transaction_conflict");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "original").unwrap();
        let staged = stage_account_directory_for_deletion(&account, "acount1", false).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "conflict").unwrap();

        let error = restore_staged_account_deletion(&staged).unwrap_err();

        assert!(error.to_string().contains("恢复冲突"));
        assert!(account.is_dir());
        assert!(staged.staged_dir.is_dir());
        assert!(staged.journal_path.is_file());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn deletion_reference_matching_accepts_uuid_case_aliases() {
        let stored_id = "550e8400-e29b-41d4-a716-446655440000";
        let requested_id = stored_id.to_ascii_uppercase();
        let audio_config = GlobalConfig {
            rune_audio_target_account: stored_id.to_string(),
            ..GlobalConfig::default()
        };
        assert!(config_references_account(&audio_config, &requested_id));

        let group_config = GlobalConfig {
            launch_groups: vec![crate::commands::global_config::LaunchGroup {
                id: "group".to_string(),
                name: "Group".to_string(),
                account_ids: Vec::new(),
                members: vec![crate::commands::global_config::LaunchGroupMember {
                    account_id: stored_id.to_string(),
                    ..crate::commands::global_config::LaunchGroupMember::default()
                }],
            }],
            ..GlobalConfig::default()
        };
        assert!(config_references_account(&group_config, &requested_id));
    }

    #[test]
    fn startup_recovers_an_interrupted_account_deletion() {
        let accounts = temp_dir("delete_transaction_startup_recovery");
        let staged = accounts.join("acount1.deleting");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("account.json"), "recoverable").unwrap();

        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert_eq!(
            std::fs::read_to_string(accounts.join("acount1").join("account.json")).unwrap(),
            "recoverable"
        );
        assert!(!staged.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn startup_cleans_orphaned_deletion_journal_artifacts() {
        let accounts = temp_dir("delete_transaction_orphan_journal");
        let temporary = accounts.join("acount1.delete.json.tmp");
        let backup = accounts.join("acount1.delete.json.bak");
        std::fs::write(&temporary, "prepared").unwrap();
        std::fs::write(&backup, "older").unwrap();

        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert!(!temporary.exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn committed_deletion_journal_finishes_cleanup_after_restart() {
        let accounts = temp_dir("delete_transaction_committed_recovery");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "delete-me").unwrap();
        let mut staged = stage_account_directory_for_deletion(&account, "acount1", false).unwrap();
        mark_account_deletion_committed(&mut staged).unwrap();

        recover_account_transactions(accounts.to_str().unwrap(), None);
        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert!(!account.exists());
        assert!(!staged.staged_dir.exists());
        assert!(!staged.journal_path.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn uninstalled_committed_temporary_journal_does_not_cross_commit_point() {
        let accounts = temp_dir("delete_transaction_uninstalled_commit");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "recoverable").unwrap();
        let staged = stage_account_directory_for_deletion(&account, "acount1", false).unwrap();
        let temporary = sibling_with_suffix(&staged.journal_path, ".tmp").unwrap();
        let mut committed = staged.journal.clone();
        committed.phase = AccountDeletionPhase::Committed;
        std::fs::write(&temporary, serde_json::to_vec_pretty(&committed).unwrap()).unwrap();

        // The durable linearization point is installing `.tmp` as the primary
        // journal. While the old Prepared primary still exists, recovery must
        // roll back even if a fully written Committed temporary is present.
        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert_eq!(
            std::fs::read_to_string(account.join("account.json")).unwrap(),
            "recoverable"
        );
        assert!(!staged.staged_dir.exists());
        assert!(!staged.journal_path.exists());
        assert!(!temporary.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn committed_temporary_journal_wins_after_primary_rotation() {
        let accounts = temp_dir("delete_transaction_rotated_primary");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "delete-me").unwrap();
        let staged = stage_account_directory_for_deletion(&account, "acount1", false).unwrap();
        let temporary = sibling_with_suffix(&staged.journal_path, ".tmp").unwrap();
        let backup = sibling_with_suffix(&staged.journal_path, ".bak").unwrap();
        let mut committed = staged.journal.clone();
        committed.phase = AccountDeletionPhase::Committed;
        std::fs::write(&temporary, serde_json::to_vec_pretty(&committed).unwrap()).unwrap();
        std::fs::rename(&staged.journal_path, &backup).unwrap();

        // Once the Prepared primary has been rotated away, the committed
        // temporary is the leading valid candidate and deletion must finish.
        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert!(!account.exists());
        assert!(!staged.staged_dir.exists());
        assert!(!staged.journal_path.exists());
        assert!(!temporary.exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn committed_config_with_marker_failure_preserves_staged_account_evidence() {
        let accounts = temp_dir("delete_transaction_marker_failure");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "preserved").unwrap();
        let mut staged = stage_account_directory_for_deletion(&account, "acount1", true).unwrap();
        let original_journal = staged.journal_path.clone();
        staged.journal_path = accounts.join("missing-parent").join("acount1.delete.json");

        let completion = complete_staged_account_deletion_after_config_commit(&mut staged);
        let error = completion.result.unwrap_err();

        assert!(completion.should_retire_account_id);
        assert!(error.to_string().contains("已保留完整账号目录"));
        assert!(!account.exists());
        assert!(staged.staged_dir.is_dir());
        assert!(original_journal.is_file());
        assert_eq!(
            std::fs::read_to_string(staged.staged_dir.join("account.json")).unwrap(),
            "preserved"
        );
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn startup_recovery_preserves_conflicting_account_directories_and_journal() {
        let accounts = temp_dir("delete_transaction_startup_conflict");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "original").unwrap();
        let staged = stage_account_directory_for_deletion(&account, "acount1", false).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "conflict").unwrap();

        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert_eq!(
            std::fs::read_to_string(account.join("account.json")).unwrap(),
            "conflict"
        );
        assert_eq!(
            std::fs::read_to_string(staged.staged_dir.join("account.json")).unwrap(),
            "original"
        );
        assert!(staged.journal_path.is_file());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn valid_committed_backup_survives_a_corrupt_primary_journal() {
        let accounts = temp_dir("delete_transaction_corrupt_primary");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "delete-me").unwrap();
        let mut staged = stage_account_directory_for_deletion(&account, "acount1", false).unwrap();
        mark_account_deletion_committed(&mut staged).unwrap();
        let backup = sibling_with_suffix(&staged.journal_path, ".bak").unwrap();
        std::fs::copy(&staged.journal_path, &backup).unwrap();
        std::fs::write(&staged.journal_path, "corrupt").unwrap();

        recover_account_transactions(accounts.to_str().unwrap(), None);
        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert!(!account.exists());
        assert!(!staged.staged_dir.exists());
        assert!(!staged.journal_path.exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn valid_committed_backup_survives_a_corrupt_temporary_journal() {
        let accounts = temp_dir("delete_transaction_corrupt_temporary");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "delete-me").unwrap();
        let mut staged = stage_account_directory_for_deletion(&account, "acount1", false).unwrap();
        mark_account_deletion_committed(&mut staged).unwrap();
        let temporary = sibling_with_suffix(&staged.journal_path, ".tmp").unwrap();
        let backup = sibling_with_suffix(&staged.journal_path, ".bak").unwrap();
        std::fs::copy(&staged.journal_path, &backup).unwrap();
        std::fs::remove_file(&staged.journal_path).unwrap();
        std::fs::write(&temporary, "corrupt").unwrap();

        recover_account_transactions(accounts.to_str().unwrap(), None);
        recover_account_transactions(accounts.to_str().unwrap(), None);

        assert!(!account.exists());
        assert!(!staged.staged_dir.exists());
        assert!(!temporary.exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn prepared_deletion_restores_when_config_still_references_account() {
        let accounts = temp_dir("delete_transaction_precommit_recovery");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "recoverable").unwrap();
        let staged = stage_account_directory_for_deletion(&account, "acount1", true).unwrap();
        let config = GlobalConfig {
            rune_audio_target_account: "acount1".to_string(),
            ..GlobalConfig::default()
        };

        recover_account_transactions(accounts.to_str().unwrap(), Some(&config));

        assert!(account.is_dir());
        assert!(!staged.staged_dir.exists());
        assert!(!staged.journal_path.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn prepared_deletion_finishes_when_config_commit_removed_references() {
        let accounts = temp_dir("delete_transaction_postcommit_recovery");
        let account = accounts.join("acount1");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("account.json"), "delete-me").unwrap();
        let staged = stage_account_directory_for_deletion(&account, "acount1", true).unwrap();
        let committed_config = GlobalConfig::default();

        recover_account_transactions(accounts.to_str().unwrap(), Some(&committed_config));

        assert!(!account.exists());
        assert!(!staged.staged_dir.exists());
        assert!(!staged.journal_path.exists());
        let _ = std::fs::remove_dir_all(accounts);
    }

    #[test]
    fn atomic_replace_rolls_back_when_installing_staged_path_fails() {
        let root = temp_dir("atomic_replace_rollback");
        let target = root.join("Battle.net");
        let staged = target.join("Battle.net.tmp");
        let backup = root.join("Battle.net.bak");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(target.join("old.txt"), "old").unwrap();
        std::fs::write(staged.join("new.txt"), "new").unwrap();

        let error = replace_path_with_backup(&staged, &target, &backup).unwrap_err();

        assert!(error.to_string().contains("替换"));
        assert_eq!(
            std::fs::read_to_string(target.join("old.txt")).unwrap(),
            "old"
        );
        assert!(target.join("Battle.net.tmp").join("new.txt").is_file());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn battle_net_snapshot_requires_config_and_preserves_existing_target() {
        let root = temp_dir("battle_net_snapshot_missing_config");
        let source = root.join("system-battle-net");
        let target = root.join("account").join("Battle.net");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("old.txt"), "old").unwrap();

        let error = replace_battle_net_snapshot(&source, &target).unwrap_err();

        assert!(error.to_string().contains("Battle.net.config"));
        assert_eq!(
            std::fs::read_to_string(target.join("old.txt")).unwrap(),
            "old"
        );
        assert!(!root.join("account").join("Battle.net.tmp").exists());
        assert!(!root.join("account").join("Battle.net.bak").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn new_bnet_initialization_clears_shared_roaming_without_a_snapshot() {
        let root = temp_dir("new_bnet_clean_runtime");
        let snapshot = root.join("account").join("Battle.net");
        let system = root.join("system").join("Battle.net");
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(system.join("previous-account.txt"), "old").unwrap();

        prepare_battle_net_runtime_directory(&snapshot, &system, BnetInitializationKind::New)
            .unwrap();

        assert!(
            !system.exists(),
            "新账号没有快照时必须清空上一账号的共享 Battle.net 目录"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn new_bnet_initialization_ignores_an_existing_account_snapshot() {
        let root = temp_dir("new_bnet_ignores_snapshot");
        let snapshot = root.join("account").join("Battle.net");
        let system = root.join("system").join("Battle.net");
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(snapshot.join("Battle.net.config"), r#"{"Client":{}}"#).unwrap();
        std::fs::write(snapshot.join("wrong-edition.txt"), "old").unwrap();
        std::fs::write(system.join("previous-account.txt"), "old").unwrap();

        prepare_battle_net_runtime_directory(&snapshot, &system, BnetInitializationKind::New)
            .unwrap();

        assert!(!system.exists());
        assert!(snapshot.join("wrong-edition.txt").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn existing_bnet_snapshot_replaces_shared_roaming_without_merging() {
        let root = temp_dir("exact_bnet_runtime_replace");
        let snapshot = root.join("account").join("Battle.net");
        let system = root.join("system").join("Battle.net");
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(snapshot.join("Battle.net.config"), r#"{"Client":{}}"#).unwrap();
        std::fs::write(snapshot.join("current.txt"), "new").unwrap();
        std::fs::write(system.join("previous-account.txt"), "old").unwrap();

        prepare_battle_net_runtime_directory(
            &snapshot,
            &system,
            BnetInitializationKind::Reinitialize,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(system.join("current.txt")).unwrap(),
            "new"
        );
        assert!(!system.join("previous-account.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn exact_registry_restore_clears_before_writing() {
        use std::cell::RefCell;
        let order = RefCell::new(Vec::new());
        let backups = vec![RegistryValueBackup {
            name: "account".to_string(),
            value_type: 1,
            value_bytes: vec![1],
        }];

        replace_registry_snapshot_with(
            &backups,
            || {
                order.borrow_mut().push("read");
                Ok(Vec::new())
            },
            || {
                order.borrow_mut().push("clear");
                Ok(())
            },
            |_| {
                order.borrow_mut().push("write");
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(*order.borrow(), vec!["read", "clear", "write"]);
    }

    #[test]
    fn invalid_registry_type_is_rejected_before_clearing() {
        use std::cell::Cell;
        let backups = vec![RegistryValueBackup {
            name: "account".to_string(),
            value_type: 999,
            value_bytes: vec![1],
        }];
        let cleared = Cell::new(false);

        let error = replace_registry_snapshot_with(
            &backups,
            || Ok(Vec::new()),
            || {
                cleared.set(true);
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("不支持"));
        assert!(!cleared.get());
    }

    #[test]
    fn exact_registry_restore_rejects_an_empty_snapshot_before_clearing() {
        use std::cell::Cell;
        let cleared = Cell::new(false);
        let error = replace_registry_snapshot_with(
            &[],
            || Ok(Vec::new()),
            || {
                cleared.set(true);
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("为空"));
        assert!(!cleared.get());
    }

    #[test]
    fn registry_write_failure_clears_partial_values_again() {
        use std::cell::Cell;
        let clear_count = Cell::new(0);
        let backups = vec![RegistryValueBackup {
            name: "account".to_string(),
            value_type: 1,
            value_bytes: vec![1],
        }];

        let error = replace_registry_snapshot_with(
            &backups,
            || Ok(Vec::new()),
            || {
                clear_count.set(clear_count.get() + 1);
                Ok(())
            },
            |_| {
                Err(AppError::RegistryError(
                    "injected write failure".to_string(),
                ))
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("injected write failure"));
        assert_eq!(clear_count.get(), 2);
    }

    #[test]
    fn registry_write_failure_restores_the_original_values() {
        use std::cell::{Cell, RefCell};
        let target = vec![RegistryValueBackup {
            name: "new-account".to_string(),
            value_type: 1,
            value_bytes: vec![2],
        }];
        let original = vec![RegistryValueBackup {
            name: "old-account".to_string(),
            value_type: 1,
            value_bytes: vec![1],
        }];
        let write_count = Cell::new(0);
        let restored = RefCell::new(Vec::new());

        let error = replace_registry_snapshot_with(
            &target,
            || Ok(original.clone()),
            || Ok(()),
            |values| {
                let attempt = write_count.get();
                write_count.set(attempt + 1);
                if attempt == 0 {
                    Err(AppError::RegistryError(
                        "injected write failure".to_string(),
                    ))
                } else {
                    restored.replace(values.to_vec());
                    Ok(())
                }
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("已恢复原注册表状态"));
        assert_eq!(restored.borrow()[0].name, "old-account");
    }

    #[test]
    fn partial_clear_failure_also_restores_the_original_values() {
        use std::cell::{Cell, RefCell};
        let target = vec![RegistryValueBackup {
            name: "new-account".to_string(),
            value_type: 1,
            value_bytes: vec![2],
        }];
        let original = vec![RegistryValueBackup {
            name: "old-account".to_string(),
            value_type: 1,
            value_bytes: vec![1],
        }];
        let clear_count = Cell::new(0);
        let restored = RefCell::new(Vec::new());

        let error = replace_registry_snapshot_with(
            &target,
            || Ok(original.clone()),
            || {
                let attempt = clear_count.get();
                clear_count.set(attempt + 1);
                if attempt == 0 {
                    Err(AppError::RegistryError(
                        "injected clear failure".to_string(),
                    ))
                } else {
                    Ok(())
                }
            },
            |values| {
                restored.replace(values.to_vec());
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("已恢复原注册表状态"));
        assert_eq!(restored.borrow()[0].name, "old-account");
    }
}

fn clear_auth_registry_unlocked() -> Result<(), AppError> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey_with_flags(
        r"Software\Blizzard Entertainment\Battle.net\UnifiedAuth",
        KEY_WRITE | KEY_READ,
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(AppError::RegistryError(format!(
                "打开 UnifiedAuth 注册表键失败: {}",
                error
            )))
        }
    };

    let names = key
        .enum_values()
        .map(|value| {
            value.map(|(name, _)| name).map_err(|error| {
                AppError::RegistryError(format!("枚举 UnifiedAuth 注册表值失败: {}", error))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for name in names {
        key.delete_value(&name).map_err(|error| {
            AppError::RegistryError(format!(
                "删除 UnifiedAuth 注册表值 {} 失败: {}",
                name, error
            ))
        })?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BnetInitializationKind {
    New,
    Reinitialize,
}

fn prepare_battle_net_runtime_directory(
    snapshot: &Path,
    system: &Path,
    kind: BnetInitializationKind,
) -> Result<(), AppError> {
    if kind == BnetInitializationKind::New {
        let staged = sibling_with_suffix(system, ".tmp")?;
        let backup = sibling_with_suffix(system, ".bak")?;
        remove_path_if_exists(&staged)?;
        remove_path_if_exists(&backup)?;
        return remove_path_if_exists(system);
    }

    if path_exists(snapshot)? {
        replace_battle_net_snapshot(snapshot, system)?;
        return Ok(());
    }

    Err(AppError::FileError(format!(
        "账号 Battle.net 快照不存在，无法重新初始化: {}",
        snapshot.display()
    )))
}

/// 准备账号初始化所需的共享 Battle.net Roaming 目录。
/// 新账号必须从干净状态开始；重新初始化必须从该账号自己的快照精确替换。
fn restore_account_to_system(
    account_dir: &Path,
    cfg: &GlobalConfig,
    meta: &AccountMeta,
    context: &LaunchContext,
    kind: BnetInitializationKind,
) -> Result<(), AppError> {
    let snapshot = if kind == BnetInitializationKind::Reinitialize {
        resolve_account_runtime_snapshot(account_dir, meta, context.installation.edition)?
            .bnet_directory
    } else {
        // New 分支不会读取快照；该路径只用于保持 helper 接口简单。
        account_dir.join("runtime").join("Battle.net")
    };
    let system = Path::new(&cfg.app_data_roaming_bnet_path);
    prepare_battle_net_runtime_directory(&snapshot, system, kind)
}

/// 首次初始化 Battle.net 账号。
#[tauri::command]
pub async fn initialize_bnet_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<(), AppError> {
    let cancellation_ticket = state.multi_instance().facade().cancellation_ticket();
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        run_bnet_initialization_transaction(
            &app,
            &state_arc,
            &account_id,
            BnetInitializationKind::New,
            cancellation_ticket,
        )
    })
    .await
    .map_err(|error| AppError::Unknown(format!("账号初始化任务异常退出: {error}")))?
}

/// 重新初始化账号；Battle.net 账号复用与首次初始化完全相同的宿主事务。
#[tauri::command]
pub async fn reinitialize_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<(), AppError> {
    let cancellation_ticket = state.multi_instance().facade().cancellation_ticket();
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        run_bnet_initialization_transaction(
            &app,
            &state_arc,
            &account_id,
            BnetInitializationKind::Reinitialize,
            cancellation_ticket,
        )
    })
    .await
    .map_err(|error| AppError::Unknown(format!("账号重新初始化任务异常退出: {error}")))?
}

fn run_bnet_initialization_transaction(
    app: &tauri::AppHandle,
    state: &SharedState,
    account_id: &str,
    kind: BnetInitializationKind,
    cancellation_ticket: CancellationTicket,
) -> Result<(), AppError> {
    // 锁序固定为 Account -> Config snapshot -> Host，禁止反向取得。
    let _account_lease = AccountLifecycleLease::try_acquire(state, account_id)?;

    // 只取得不可变配置快照，不能让最长 120 秒的登录等待阻塞设置写入。
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    // Initialization owns only its generation snapshot. It must never clear the
    // shared launch flag before acquiring the host lease, otherwise a rejected
    // concurrent initialization can resurrect a launch the user just cancelled.
    let is_cancelled = || {
        state
            .multi_instance()
            .facade()
            .is_cancelled(cancellation_ticket)
    };
    if is_cancelled() {
        return Err(AppError::Unknown("账号初始化已取消".to_string()));
    }

    let account_dir = AccountManager::account_dir_checked(&cfg.accounts_dir, account_id)?;
    let mut meta = AccountManager::load_meta(&cfg.accounts_dir, account_id)?;
    let auth_mode = AuthMode::parse(meta.auth_mode.as_deref())?;

    let emit = |step: &str, status: &str, message: &str| {
        crate::logger::log_msg(
            "INFO",
            "AccountInit",
            &format!("[Account {account_id}] [{step}] [{status}]: {message}"),
        );
        let _ = app.emit(
            "launch-progress",
            crate::commands::system::LaunchProgress::new(account_id, step, status, message),
        );
    };

    if auth_mode == AuthMode::Token {
        if kind == BnetInitializationKind::New {
            return Err(AppError::ConfigReadError(
                "Token 账号不能调用 Battle.net 初始化事务".to_string(),
            ));
        }
        LaunchContext::for_account(&cfg, &meta, ContextPurpose::LaunchGame)?;
        meta.initialized = true;
        meta.last_reset_at = Some(chrono::Utc::now().to_rfc3339());
        AccountManager::save_meta(&cfg.accounts_dir, &meta)?;
        emit("done", "ok", "Token 账号初始化状态已刷新");
        return Ok(());
    }

    // 在取得主机租约前完成路径与客户端版本预检，避免错误配置长期占用共享环境。
    LaunchContext::for_account(&cfg, &meta, ContextPurpose::BattleNetOnly)?;

    if kind == BnetInitializationKind::New && meta.initialized {
        return Err(AppError::ConfigReadError(format!(
            "账号 {account_id} 已初始化，拒绝重复执行首次初始化"
        )));
    }

    let _host_runtime_lease = HostRuntimeLease::try_acquire(state.as_ref())?;

    // 主机租约取得后重新读取并解析，只使用第二份上下文，消除等待期间的 TOCTOU。
    meta = AccountManager::load_meta(&cfg.accounts_dir, account_id)?;
    if AuthMode::parse(meta.auth_mode.as_deref())? != AuthMode::BattleNet {
        return Err(AppError::ConfigReadError(
            "账号认证方式在初始化期间发生变化".to_string(),
        ));
    }
    let context = LaunchContext::for_account(&cfg, &meta, ContextPurpose::BattleNetOnly)?;
    let effective_kind = if kind == BnetInitializationKind::Reinitialize && !meta.initialized {
        BnetInitializationKind::New
    } else {
        kind
    };
    let battle_net_path = context.battle_net_executable()?.to_path_buf();
    let battle_net_identity = battle_net_path.to_string_lossy().into_owned();

    let primary_result = (|| -> Result<PendingAccountSnapshot, AppError> {
        emit("clean", "running", "正在清理 Battle.net 与 Agent 进程");
        kill_processes_by_name(&["Battle.net.exe", "Agent.exe"])?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        emit("clean", "ok", "共享进程环境已清理");

        if !cfg.browser_path.is_empty() && !cfg.browser_type.is_empty() {
            emit("browser", "running", "正在启动该账号的独立浏览器配置");
            #[cfg(target_os = "windows")]
            let before_hwnds = crate::commands::system::collect_chrome_windows();

            match crate::commands::browser::launch_browser_for_account_impl(
                &cfg,
                &cfg.browser_path,
                account_id,
            ) {
                Ok(()) => {
                    #[cfg(target_os = "windows")]
                    crate::commands::system::bring_browser_login_to_foreground(before_hwnds);
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    emit("browser", "ok", "浏览器已启动");
                }
                Err(error) => {
                    emit(
                        "browser",
                        "error",
                        &format!("浏览器启动失败，不影响 Battle.net 登录: {error}"),
                    );
                }
            }
        } else {
            emit("browser", "ok", "未配置登录辅助浏览器，已跳过");
        }

        if is_cancelled() {
            return Err(AppError::Unknown("账号初始化已取消".to_string()));
        }

        emit("restore", "running", "正在准备账号专属 Battle.net 配置");
        restore_account_to_system(&account_dir, &cfg, &meta, &context, effective_kind)?;
        emit("restore", "ok", "账号配置已精确恢复");

        emit("registry", "running", "正在清空旧 UnifiedAuth 认证状态");
        clear_auth_registry_unlocked()?;
        emit("registry", "ok", "旧认证状态已清空");

        if is_cancelled() {
            return Err(AppError::Unknown("账号初始化已取消".to_string()));
        }

        emit(
            "launch",
            "running",
            "正在启动账号所属版本的 Battle.net 客户端",
        );
        std::process::Command::new(&battle_net_path)
            .spawn()
            .map_err(|error| {
                AppError::FileError(format!(
                    "启动 Battle.net 失败（{}）: {error}",
                    battle_net_path.display()
                ))
            })?;
        crate::commands::system::bring_bnet_to_foreground();
        emit("launch", "ok", "Battle.net 已启动，请完成登录");

        emit("login", "running", "正在等待 Battle.net 登录完成");
        let mut logged_in = false;
        for elapsed in 1..=120 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if is_cancelled() {
                emit("login", "error", "账号初始化已取消");
                return Err(AppError::Unknown("账号初始化已取消".to_string()));
            }

            if crate::commands::system::count_bnet_processes_for_path(&battle_net_identity) >= 7 {
                logged_in = true;
                break;
            }
            if elapsed % 5 == 0 {
                emit(
                    "login",
                    "running",
                    &format!("等待 Battle.net 登录中（{elapsed}s）"),
                );
            }
        }
        if !logged_in {
            emit("login", "error", "等待登录超时（120 秒）");
            return Err(AppError::LoginTimeout(120));
        }
        emit("login", "ok", "已检测到登录完成");

        emit("snapshot", "running", "正在采集账号认证快照");
        let pending = stage_account_snapshot(&cfg, account_id, &context)?;
        if is_cancelled() {
            pending.discard();
            return Err(AppError::Unknown("账号初始化已取消".to_string()));
        }
        emit("snapshot", "ok", "完整账号状态已暂存并校验");
        Ok(pending)
    })();

    // 成功、失败、取消都在租约释放前收尾，避免认证状态泄漏给下一个账号。
    let cleanup_result = cleanup_after_snapshot();
    if cfg.auto_close_browser && !cfg.browser_type.is_empty() {
        kill_browser_processes_blocking(&cfg.browser_type);
    }

    let result = match (primary_result, cleanup_result) {
        (Ok(pending), Ok(())) => {
            if is_cancelled() {
                pending.discard();
                Err(AppError::Unknown("账号初始化已取消".to_string()))
            } else {
                // 从这里开始视为不可取消提交；认证快照与 account.json 一次生效。
                pending.commit()
            }
        }
        (Err(primary), Ok(())) => Err(primary),
        (Ok(pending), Err(cleanup)) => {
            pending.discard();
            Err(cleanup)
        }
        (Err(primary), Err(cleanup)) => Err(AppError::Unknown(format!(
            "账号初始化失败: {primary}；清理共享认证状态也失败: {cleanup}"
        ))),
    };

    match &result {
        Ok(()) => emit("done", "ok", "账号初始化事务已完成"),
        Err(error) => emit("done", "error", &format!("账号初始化失败: {error}")),
    }
    result
}

/// 在账号目录副本中组装并校验完整状态，真实账号目录保持不变。
fn stage_account_snapshot(
    cfg: &GlobalConfig,
    account_id: &str,
    context: &LaunchContext,
) -> Result<PendingAccountSnapshot, AppError> {
    let account_dir = AccountManager::account_dir_checked(&cfg.accounts_dir, account_id)?;
    let account_pending = stage_account_directory(&account_dir)?;
    let build_result = (|| -> Result<(), AppError> {
        let runtime = stage_runtime_snapshot(
            &account_pending.staged_root,
            cfg,
            context.installation.edition,
        )?;
        let mut meta = AccountManager::load_meta(&cfg.accounts_dir, account_id)?;
        meta.initialized = true;
        meta.last_reset_at = Some(chrono::Utc::now().to_rfc3339());
        meta.snapshot_edition = Some(context.installation.edition.canonical().to_string());

        let game_key = context.edition.battle_net_config_game_key;
        if meta.mod_args.is_empty() {
            if let Some(args) = try_read_mod_args(&runtime.staged_bnet_config, game_key) {
                meta.mod_args = args;
            }
        } else {
            update_mod_args(&runtime.staged_bnet_config, game_key, &meta.mod_args)?;
        }
        runtime.commit()?;
        validate_runtime_snapshot_root(
            &account_pending.staged_root.join("runtime"),
            context.installation.edition,
        )?;
        write_account_meta_to_directory(&account_pending.staged_root, &meta)
    })();

    if let Err(error) = build_result {
        account_pending.discard();
        return Err(error);
    }
    Ok(account_pending)
}

fn path_exists(path: &Path) -> Result<bool, AppError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::FileError(format!(
            "检查路径 {} 失败: {}",
            path.display(),
            error
        ))),
    }
}

pub(crate) fn remove_path_if_exists(path: &Path) -> Result<(), AppError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(AppError::FileError(format!(
                "检查待清理路径 {} 失败: {}",
                path.display(),
                error
            )));
        }
    };

    let result = if metadata.file_type().is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|error| {
        AppError::FileError(format!("清理路径 {} 失败: {}", path.display(), error))
    })
}

pub(crate) fn sibling_with_suffix(path: &Path, suffix: &str) -> Result<PathBuf, AppError> {
    let mut file_name = path
        .file_name()
        .ok_or_else(|| AppError::FileError(format!("路径缺少文件名: {}", path.display())))?
        .to_os_string();
    file_name.push(suffix);
    Ok(path.with_file_name(file_name))
}

const ACCOUNT_DELETION_JOURNAL_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AccountDeletionPhase {
    Prepared,
    Committed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountDeletionJournal {
    schema_version: u32,
    account_id: String,
    phase: AccountDeletionPhase,
    had_configuration_references: bool,
}

struct StagedAccountDeletion {
    account_dir: PathBuf,
    staged_dir: PathBuf,
    journal_path: PathBuf,
    journal: AccountDeletionJournal,
}

fn config_references_account(config: &GlobalConfig, account_id: &str) -> bool {
    config
        .rune_audio_target_account
        .trim()
        .eq_ignore_ascii_case(account_id)
        || config.launch_groups.iter().any(|group| {
            group
                .account_ids
                .iter()
                .any(|member_id| member_id.eq_ignore_ascii_case(account_id))
                || group
                    .members
                    .iter()
                    .any(|member| member.account_id.eq_ignore_ascii_case(account_id))
        })
}

fn account_deletion_journal_path(account_dir: &Path) -> Result<PathBuf, AppError> {
    sibling_with_suffix(account_dir, ".delete.json")
}

fn write_account_deletion_journal(
    journal_path: &Path,
    journal: &AccountDeletionJournal,
) -> Result<(), AppError> {
    let temporary = sibling_with_suffix(journal_path, ".tmp")?;
    let backup = sibling_with_suffix(journal_path, ".bak")?;
    remove_path_if_exists(&temporary)?;
    let bytes = serde_json::to_vec_pretty(journal)?;
    std::fs::write(&temporary, bytes).map_err(|error| {
        AppError::FileError(format!(
            "写入账号删除事务 {} 失败: {}",
            temporary.display(),
            error
        ))
    })?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            AppError::FileError(format!(
                "同步账号删除事务 {} 失败: {}",
                temporary.display(),
                error
            ))
        })?;
    replace_path_with_backup(&temporary, journal_path, &backup)
}

fn cleanup_account_deletion_journal(journal_path: &Path) -> Result<(), AppError> {
    remove_path_if_exists(journal_path)?;
    remove_path_if_exists(&sibling_with_suffix(journal_path, ".tmp")?)?;
    remove_path_if_exists(&sibling_with_suffix(journal_path, ".bak")?)
}

/// Prepares a recoverable account deletion while the configuration transaction
/// is held. The journal lets startup distinguish rollback from committed cleanup.
fn stage_account_directory_for_deletion(
    account_dir: &Path,
    account_id: &str,
    had_configuration_references: bool,
) -> Result<StagedAccountDeletion, AppError> {
    let temporary = sibling_with_suffix(account_dir, ".tmp")?;
    let backup = sibling_with_suffix(account_dir, ".bak")?;
    let staged_deletion = sibling_with_suffix(account_dir, ".deleting")?;
    let journal_path = account_deletion_journal_path(account_dir)?;
    remove_path_if_exists(&temporary)?;
    remove_path_if_exists(&backup)?;
    if path_exists(&staged_deletion)? {
        return Err(AppError::FileError(format!(
            "账号存在未完成的删除事务，请重启 D2RHub 自动恢复后重试: {}",
            staged_deletion.display()
        )));
    }
    if path_exists(&journal_path)? {
        cleanup_account_deletion_journal(&journal_path)?;
    }
    let journal = AccountDeletionJournal {
        schema_version: ACCOUNT_DELETION_JOURNAL_SCHEMA,
        account_id: account_id.to_string(),
        phase: AccountDeletionPhase::Prepared,
        had_configuration_references,
    };
    write_account_deletion_journal(&journal_path, &journal)?;
    if let Err(error) = std::fs::rename(account_dir, &staged_deletion) {
        let _ = cleanup_account_deletion_journal(&journal_path);
        return Err(AppError::FileError(format!(
            "暂存待删除账号 {} 失败: {}",
            account_dir.display(),
            error
        )));
    }
    Ok(StagedAccountDeletion {
        account_dir: account_dir.to_path_buf(),
        staged_dir: staged_deletion,
        journal_path,
        journal,
    })
}

fn mark_account_deletion_committed(
    staged_deletion: &mut StagedAccountDeletion,
) -> Result<(), AppError> {
    staged_deletion.journal.phase = AccountDeletionPhase::Committed;
    write_account_deletion_journal(&staged_deletion.journal_path, &staged_deletion.journal)
}

struct AccountDeletionCompletion {
    should_retire_account_id: bool,
    result: Result<(), AppError>,
}

fn complete_staged_account_deletion_after_config_commit(
    staged_deletion: &mut StagedAccountDeletion,
) -> AccountDeletionCompletion {
    match mark_account_deletion_committed(staged_deletion) {
        Ok(()) => AccountDeletionCompletion {
            should_retire_account_id: true,
            result: finalize_staged_account_deletion(staged_deletion).map_err(|error| {
                AppError::FileError(format!(
                    "账号配置已提交，但目录清理尚未完成；数据保留在删除事务中，重启 D2RHub 将自动继续: {error}"
                ))
            }),
        },
        Err(error) if !staged_deletion.journal.had_configuration_references => {
            AccountDeletionCompletion {
                should_retire_account_id: false,
                result: Err(rollback_staged_account_deletion(staged_deletion, error)),
            }
        }
        Err(error) => AccountDeletionCompletion {
            should_retire_account_id: true,
            result: Err(AppError::FileError(format!(
                "账号配置已提交，但删除事务标记写入失败；已保留完整账号目录，重启 D2RHub 将根据配置自动恢复: {error}"
            ))),
        },
    }
}

fn finalize_staged_account_deletion(
    staged_deletion: &StagedAccountDeletion,
) -> Result<(), AppError> {
    remove_path_if_exists(&staged_deletion.staged_dir)?;
    if let Err(error) = cleanup_account_deletion_journal(&staged_deletion.journal_path) {
        crate::logger::log_msg(
            "WARN",
            "AccountDelete",
            &format!("账号目录已删除，但事务日志清理失败，将在下次启动重试: {error}"),
        );
    }
    Ok(())
}

fn restore_staged_account_deletion(
    staged_deletion: &StagedAccountDeletion,
) -> Result<(), AppError> {
    if path_exists(&staged_deletion.account_dir)? {
        return Err(AppError::FileError(format!(
            "账号删除恢复冲突：正式目录 {} 与暂存目录 {} 同时存在，已保留两者和事务日志等待人工检查",
            staged_deletion.account_dir.display(),
            staged_deletion.staged_dir.display()
        )));
    }
    if !path_exists(&staged_deletion.staged_dir)? {
        return Err(AppError::FileError(format!(
            "待恢复的账号删除暂存目录不存在: {}",
            staged_deletion.staged_dir.display()
        )));
    }
    std::fs::rename(&staged_deletion.staged_dir, &staged_deletion.account_dir).map_err(
        |error| {
            AppError::FileError(format!(
                "恢复待删除账号 {} 失败: {}",
                staged_deletion.account_dir.display(),
                error
            ))
        },
    )?;
    cleanup_account_deletion_journal(&staged_deletion.journal_path)
}

fn rollback_staged_account_deletion(
    staged_deletion: &StagedAccountDeletion,
    original_error: AppError,
) -> AppError {
    match restore_staged_account_deletion(staged_deletion) {
        Ok(()) => original_error,
        Err(rollback_error) => AppError::FileError(format!(
            "账号删除事务失败: {original_error}；恢复账号目录也失败: {rollback_error}"
        )),
    }
}

#[cfg(test)]
fn remove_account_directory_without_resurrection(
    account_dir: &Path,
    account_id: &str,
) -> Result<(), AppError> {
    let mut staged_deletion = stage_account_directory_for_deletion(account_dir, account_id, false)?;
    mark_account_deletion_committed(&mut staged_deletion)?;
    finalize_staged_account_deletion(&staged_deletion)
}

pub(crate) fn recover_interrupted_replacement(target: &Path) -> Result<(), AppError> {
    if path_exists(target)? {
        return Ok(());
    }
    let backup = sibling_with_suffix(target, ".bak")?;
    let staged = sibling_with_suffix(target, ".tmp")?;
    // `.tmp` 同时存在时可能是另一个线程正在两次 rename 之间；只在无活动暂存时恢复。
    if path_exists(&backup)? && !path_exists(&staged)? {
        std::fs::rename(&backup, target).map_err(|error| {
            AppError::FileError(format!(
                "恢复中断事务失败，无法将 {} 还原到 {}: {}",
                backup.display(),
                target.display(),
                error
            ))
        })?;
    }
    Ok(())
}

fn recover_interrupted_account_directories(accounts_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(accounts_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(account_id) = name.strip_suffix(".bak") else {
            continue;
        };
        if !AccountManager::is_valid_account_id(account_id) {
            continue;
        }
        let target = accounts_dir.join(account_id);
        let staged = accounts_dir.join(format!("{account_id}.tmp"));
        if target.exists() || staged.exists() || !entry.path().is_dir() {
            continue;
        }
        if let Err(error) = std::fs::rename(entry.path(), &target) {
            crate::logger::log_msg(
                "WARN",
                "AccountSnapshot",
                &format!("恢复账号目录 {} 失败: {error}", target.display()),
            );
        }
    }
}

fn load_account_deletion_journal(
    journal_path: &Path,
    expected_account_id: &str,
) -> Result<AccountDeletionJournal, AppError> {
    let candidates = [
        journal_path.to_path_buf(),
        sibling_with_suffix(journal_path, ".tmp")?,
        sibling_with_suffix(journal_path, ".bak")?,
    ];
    let mut failures = Vec::new();
    for candidate in candidates {
        if !path_exists(&candidate)? {
            continue;
        }
        let parsed = std::fs::read(&candidate)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<AccountDeletionJournal>(&bytes)
                    .map_err(|error| error.to_string())
            });
        match parsed {
            Ok(journal)
                if journal.schema_version == ACCOUNT_DELETION_JOURNAL_SCHEMA
                    && journal.account_id == expected_account_id =>
            {
                return Ok(journal);
            }
            Ok(_) => failures.push(format!("{}: schema 或账号 ID 不匹配", candidate.display())),
            Err(error) => failures.push(format!("{}: {error}", candidate.display())),
        }
    }
    Err(AppError::ConfigReadError(format!(
        "账号删除事务没有可用日志候选: {}",
        if failures.is_empty() {
            journal_path.display().to_string()
        } else {
            failures.join("；")
        }
    )))
}

fn recover_interrupted_account_deletions(root: &Path, config: Option<&GlobalConfig>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(account_id) = name.strip_suffix(".deleting") else {
            continue;
        };
        if !AccountManager::is_valid_account_id(account_id) || !entry.path().is_dir() {
            continue;
        }
        let account_dir = root.join(account_id);
        if account_dir.exists() {
            crate::logger::log_msg(
                "WARN",
                "AccountDelete",
                &format!(
                    "账号 {} 同时存在正式目录和删除暂存目录，已保留两者等待人工检查",
                    account_id
                ),
            );
            continue;
        }
        let Ok(journal_path) = account_deletion_journal_path(&account_dir) else {
            continue;
        };
        let loaded_journal = match load_account_deletion_journal(&journal_path, account_id) {
            Ok(journal) => Some(journal),
            Err(error) => {
                crate::logger::log_msg(
                    "WARN",
                    "AccountDelete",
                    &format!("账号删除事务 {account_id} 的日志不可用，将优先恢复账号目录: {error}"),
                );
                None
            }
        };
        let should_finalize = loaded_journal.as_ref().is_some_and(|journal| {
            journal.phase == AccountDeletionPhase::Committed
                || (journal.had_configuration_references
                    && config.is_some_and(|config| !config_references_account(config, account_id)))
        });
        let journal = loaded_journal.unwrap_or(AccountDeletionJournal {
            schema_version: ACCOUNT_DELETION_JOURNAL_SCHEMA,
            account_id: account_id.to_string(),
            phase: AccountDeletionPhase::Prepared,
            had_configuration_references: false,
        });
        let staged_deletion = StagedAccountDeletion {
            account_dir,
            staged_dir: entry.path(),
            journal_path,
            journal,
        };
        let result = if should_finalize {
            finalize_staged_account_deletion(&staged_deletion)
        } else {
            restore_staged_account_deletion(&staged_deletion)
        };
        if let Err(error) = result {
            crate::logger::log_msg(
                "WARN",
                "AccountDelete",
                &format!("恢复账号删除事务 {account_id} 失败: {error}"),
            );
        }
    }

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let account_id = name
            .strip_suffix(".delete.json")
            .or_else(|| name.strip_suffix(".delete.json.tmp"))
            .or_else(|| name.strip_suffix(".delete.json.bak"));
        let Some(account_id) = account_id else {
            continue;
        };
        if !AccountManager::is_valid_account_id(account_id)
            || root.join(format!("{account_id}.deleting")).exists()
        {
            continue;
        }
        let _ = cleanup_account_deletion_journal(&root.join(format!("{account_id}.delete.json")));
    }
}

/// 仅在应用启动、尚无账号事务运行时调用；先恢复或完成删除日志，再回滚
/// 账号快照的 `.bak` 并丢弃未提交的 `.tmp`。
pub(crate) fn recover_account_transactions(accounts_dir: &str, config: Option<&GlobalConfig>) {
    let root = Path::new(accounts_dir);
    recover_interrupted_account_deletions(root, config);

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(account_id) = name.strip_suffix(".bak") else {
            continue;
        };
        if !AccountManager::is_valid_account_id(account_id) || !entry.path().is_dir() {
            continue;
        }
        let target = root.join(account_id);
        if target.exists() {
            continue;
        }
        let staged = root.join(format!("{account_id}.tmp"));
        let _ = remove_path_if_exists(&staged);
        if let Err(error) = std::fs::rename(entry.path(), &target) {
            crate::logger::log_msg(
                "WARN",
                "AccountSnapshot",
                &format!("启动时回滚账号目录 {} 失败: {error}", target.display()),
            );
        }
    }

    for account_id in AccountManager::list_ids(accounts_dir) {
        let Ok(account_dir) = AccountManager::account_dir_checked(accounts_dir, &account_id) else {
            continue;
        };
        let meta = account_dir.join("account.json");
        if meta.exists() {
            continue;
        }
        let Ok(backup) = sibling_with_suffix(&meta, ".bak") else {
            continue;
        };
        let Ok(staged) = sibling_with_suffix(&meta, ".tmp") else {
            continue;
        };
        let _ = remove_path_if_exists(&staged);
        if backup.exists() {
            let _ = std::fs::rename(&backup, &meta);
        }
    }
}

const ACCOUNT_RUNTIME_SNAPSHOT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountRuntimeSnapshotManifest {
    schema_version: u32,
    edition: String,
    created_at: String,
}

#[derive(Debug, Clone)]
pub(crate) enum RegistrySnapshotPath {
    Json(PathBuf),
    LegacyReg(PathBuf),
}

#[derive(Debug, Clone)]
pub(crate) struct AccountRuntimeSnapshotPaths {
    pub bnet_directory: PathBuf,
    pub registry: RegistrySnapshotPath,
}

fn validate_runtime_snapshot_root(
    root: &Path,
    expected_edition: ClientEdition,
) -> Result<AccountRuntimeSnapshotPaths, AppError> {
    let manifest_path = root.join("snapshot.json");
    if !manifest_path.is_file() {
        return Err(AppError::FileError(format!(
            "账号运行时快照缺少来源清单: {}",
            manifest_path.display()
        )));
    }
    let manifest: AccountRuntimeSnapshotManifest =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)
            .map_err(|error| AppError::FileError(format!("快照来源清单无效: {error}")))?;
    if manifest.schema_version != ACCOUNT_RUNTIME_SNAPSHOT_SCHEMA {
        return Err(AppError::FileError(format!(
            "不支持的账号快照版本: {}",
            manifest.schema_version
        )));
    }
    if manifest.edition != expected_edition.canonical() {
        return Err(AppError::ConfigReadError(format!(
            "账号快照属于{}，当前账号属于{}，拒绝跨版本恢复",
            manifest.edition,
            expected_edition.canonical()
        )));
    }

    let bnet_directory = root.join("Battle.net");
    let bnet_config = bnet_directory.join("Battle.net.config");
    if !bnet_config.is_file() {
        return Err(AppError::FileError(format!(
            "账号快照缺少 Battle.net.config: {}",
            bnet_directory.display()
        )));
    }
    validate_battle_net_config_for_edition(&bnet_config, expected_edition)?;
    let registry_path = root.join("unified_auth.json");
    if !registry_path.is_file() {
        return Err(AppError::FileError(format!(
            "账号快照缺少 UnifiedAuth JSON: {}",
            registry_path.display()
        )));
    }
    let backups: Vec<RegistryValueBackup> =
        serde_json::from_str(&std::fs::read_to_string(&registry_path)?)
            .map_err(|error| AppError::FileError(format!("账号 UnifiedAuth 快照无效: {error}")))?;
    validate_registry_snapshot(&backups)?;

    Ok(AccountRuntimeSnapshotPaths {
        bnet_directory,
        registry: RegistrySnapshotPath::Json(registry_path),
    })
}

fn validate_battle_net_config_for_edition(
    config_path: &Path,
    expected_edition: ClientEdition,
) -> Result<(), AppError> {
    let content = std::fs::read_to_string(config_path)?;
    let config: serde_json::Value = serde_json::from_str(&content).map_err(|error| {
        AppError::ConfigReadError(format!("账号 Battle.net.config JSON 无效: {error}"))
    })?;
    let root = config.as_object().ok_or_else(|| {
        AppError::ConfigReadError("账号 Battle.net.config 根节点不是 JSON 对象".to_string())
    })?;
    let games = root
        .get("Games")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            AppError::ConfigReadError("账号 Battle.net.config 缺少有效的 Games 对象".to_string())
        })?;
    let expected_key = EditionConventions::for_edition(expected_edition).battle_net_config_game_key;
    let game = games
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(expected_key))
        .map(|(_, value)| value)
        .ok_or_else(|| {
            AppError::ConfigReadError(format!(
                "账号 Battle.net.config 缺少{}产品键 {expected_key}",
                expected_edition.display_name()
            ))
        })?;
    if !game.is_object() {
        return Err(AppError::ConfigReadError(format!(
            "账号 Battle.net.config 的 Games.{expected_key} 不是 JSON 对象"
        )));
    }
    Ok(())
}

fn hydrate_meta_from_runtime_snapshot(account_dir: &Path, meta: &mut AccountMeta) {
    let runtime_root = account_dir.join("runtime");
    if !runtime_root.exists() {
        return;
    }

    let validation = (|| -> Result<AccountRuntimeSnapshotManifest, AppError> {
        let region = meta.region.as_deref().ok_or_else(|| {
            AppError::ConfigReadError("账号缺少区服，无法校验 runtime 快照".to_string())
        })?;
        let expected_edition = GameRegion::parse(region)?.edition();
        validate_runtime_snapshot_root(&runtime_root, expected_edition)?;
        let manifest: AccountRuntimeSnapshotManifest = serde_json::from_str(
            &std::fs::read_to_string(runtime_root.join("snapshot.json"))?,
        )
        .map_err(|error| AppError::FileError(format!("快照来源清单无效: {error}")))?;
        Ok(manifest)
    })();

    match validation {
        Ok(manifest) => {
            meta.initialized = true;
            meta.last_reset_at = Some(manifest.created_at);
            meta.snapshot_edition = Some(manifest.edition);
        }
        Err(error) => {
            meta.initialized = false;
            meta.snapshot_edition = None;
            crate::logger::log_msg(
                "WARN",
                "AccountSnapshot",
                &format!("忽略无效 runtime 快照: {error}"),
            );
        }
    }
}

pub(crate) fn resolve_account_runtime_snapshot(
    account_dir: &Path,
    meta: &AccountMeta,
    expected_edition: ClientEdition,
) -> Result<AccountRuntimeSnapshotPaths, AppError> {
    let runtime_root = account_dir.join("runtime");
    if runtime_root.exists() {
        return validate_runtime_snapshot_root(&runtime_root, expected_edition);
    }

    let bnet_directory = account_dir.join("Battle.net");
    if !bnet_directory.join("Battle.net.config").is_file() {
        return Err(AppError::FileError(format!(
            "账号 Battle.net 快照不存在或不完整: {}",
            bnet_directory.display()
        )));
    }

    // 兼容旧布局；有明确来源标记时必须匹配。无标记旧账号必须能由唯一产品键证明来源。
    if let Some(snapshot_edition) = meta.snapshot_edition.as_deref() {
        if snapshot_edition != expected_edition.canonical() {
            return Err(AppError::ConfigReadError(format!(
                "旧账号快照属于{snapshot_edition}，当前账号属于{}，拒绝跨版本恢复",
                expected_edition.canonical()
            )));
        }
    } else if !meta.initialized {
        return Err(AppError::ConfigReadError(
            "未初始化账号的旧快照没有客户端版本来源，拒绝恢复".to_string(),
        ));
    } else {
        let inferred = infer_legacy_snapshot_edition(&bnet_directory)?;
        if inferred != expected_edition {
            return Err(AppError::ConfigReadError(format!(
                "旧账号快照由产品键识别为{}，当前账号属于{}，拒绝跨版本恢复",
                inferred.canonical(),
                expected_edition.canonical()
            )));
        }
    }

    let json = account_dir.join("unified_auth.json");
    let legacy = account_dir.join("unified_auth.reg");
    let registry = if json.is_file() {
        let backups: Vec<RegistryValueBackup> =
            serde_json::from_str(&std::fs::read_to_string(&json)?).map_err(|error| {
                AppError::FileError(format!("旧 UnifiedAuth JSON 无效: {error}"))
            })?;
        validate_registry_snapshot(&backups)?;
        RegistrySnapshotPath::Json(json)
    } else if legacy.is_file() {
        RegistrySnapshotPath::LegacyReg(legacy)
    } else {
        return Err(AppError::FileError(
            "账号缺少 UnifiedAuth 认证快照".to_string(),
        ));
    };

    Ok(AccountRuntimeSnapshotPaths {
        bnet_directory,
        registry,
    })
}

fn infer_legacy_snapshot_edition(bnet_directory: &Path) -> Result<ClientEdition, AppError> {
    let config_path = bnet_directory.join("Battle.net.config");
    let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config_path)?)
        .map_err(|error| {
        AppError::ConfigReadError(format!(
            "旧 Battle.net.config 无效，无法证明客户端版本: {error}"
        ))
    })?;
    let games = config
        .get("Games")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            AppError::ConfigReadError(
                "旧 Battle.net.config 缺少 Games，无法证明客户端版本；请重新初始化账号".to_string(),
            )
        })?;
    let has_cn = games.keys().any(|key| key.eq_ignore_ascii_case("osic"));
    let has_global = games.keys().any(|key| key.eq_ignore_ascii_case("osi"));
    match (has_cn, has_global) {
        (true, false) => Ok(ClientEdition::Cn),
        (false, true) => Ok(ClientEdition::Global),
        _ => Err(AppError::ConfigReadError(
            "旧 Battle.net.config 的产品键来源不唯一，无法安全迁移；请重新初始化账号".to_string(),
        )),
    }
}

#[derive(Debug)]
struct PendingRuntimeSnapshot {
    staged_root: PathBuf,
    target_root: PathBuf,
    backup_root: PathBuf,
    staged_bnet_config: PathBuf,
}

impl PendingRuntimeSnapshot {
    fn commit(self) -> Result<(), AppError> {
        replace_path_with_backup(&self.staged_root, &self.target_root, &self.backup_root)
    }
}

#[derive(Debug)]
struct PendingAccountSnapshot {
    staged_root: PathBuf,
    target_root: PathBuf,
    backup_root: PathBuf,
}

impl PendingAccountSnapshot {
    fn commit(self) -> Result<(), AppError> {
        replace_path_with_backup(&self.staged_root, &self.target_root, &self.backup_root)
    }

    fn discard(&self) {
        if let Err(error) = remove_path_if_exists(&self.staged_root) {
            crate::logger::log_msg(
                "WARN",
                "AccountSnapshot",
                &format!(
                    "清理未提交账号事务目录 {} 失败: {error}",
                    self.staged_root.display()
                ),
            );
        }
    }
}

fn stage_account_directory(account_dir: &Path) -> Result<PendingAccountSnapshot, AppError> {
    if !account_dir.join("account.json").is_file() {
        return Err(AppError::FileError(format!(
            "账号目录不完整: {}",
            account_dir.display()
        )));
    }
    let staged_root = sibling_with_suffix(account_dir, ".tmp")?;
    let backup_root = sibling_with_suffix(account_dir, ".bak")?;
    remove_path_if_exists(&staged_root)?;
    crate::commands::utils::copy_dir_recursive(account_dir, &staged_root).map_err(|error| {
        let _ = remove_path_if_exists(&staged_root);
        AppError::FileError(format!("暂存账号目录失败: {error}"))
    })?;
    Ok(PendingAccountSnapshot {
        staged_root,
        target_root: account_dir.to_path_buf(),
        backup_root,
    })
}

/// 将 Settings.json 与 `has_customized_settings` 等账号元数据作为一次账号目录事务提交。
pub(crate) fn commit_account_settings_transaction(
    accounts_dir: &str,
    meta: &AccountMeta,
    settings_json: &str,
) -> Result<(), AppError> {
    let account_dir = AccountManager::account_dir_checked(accounts_dir, &meta.id)?;
    let pending = stage_account_directory(&account_dir)?;
    let build_result = (|| -> Result<(), AppError> {
        std::fs::write(pending.staged_root.join("Settings.json"), settings_json)?;
        write_account_meta_to_directory(&pending.staged_root, meta)
    })();
    if let Err(error) = build_result {
        pending.discard();
        return Err(error);
    }
    pending.commit()
}

fn write_account_meta_to_directory(account_dir: &Path, meta: &AccountMeta) -> Result<(), AppError> {
    let path = account_dir.join("account.json");
    let content = serde_json::to_string_pretty(meta)
        .map_err(|error| AppError::FileError(format!("序列化 account.json 失败: {error}")))?;
    std::fs::write(&path, content)?;
    let written: AccountMeta = serde_json::from_str(&std::fs::read_to_string(&path)?)
        .map_err(|error| AppError::FileError(format!("校验 account.json 失败: {error}")))?;
    if written.id != meta.id {
        return Err(AppError::FileError(format!(
            "account.json 校验失败：账号 ID {} != {}",
            written.id, meta.id
        )));
    }
    Ok(())
}

fn stage_runtime_snapshot(
    account_dir: &Path,
    cfg: &GlobalConfig,
    edition: ClientEdition,
) -> Result<PendingRuntimeSnapshot, AppError> {
    let source_bnet = Path::new(&cfg.app_data_roaming_bnet_path);
    let source_config = source_bnet.join("Battle.net.config");
    if !source_bnet.is_dir() || !source_config.is_file() {
        return Err(AppError::FileError(format!(
            "系统 Battle.net 运行目录不完整: {}",
            source_bnet.display()
        )));
    }

    let target_root = account_dir.join("runtime");
    let staged_root = sibling_with_suffix(&target_root, ".tmp")?;
    let backup_root = sibling_with_suffix(&target_root, ".bak")?;
    remove_path_if_exists(&staged_root)?;

    let staged_bnet = staged_root.join("Battle.net");
    let staged_bnet_config = staged_bnet.join("Battle.net.config");
    let stage_result = (|| -> Result<(), AppError> {
        crate::commands::utils::copy_dir_recursive(source_bnet, &staged_bnet).map_err(|error| {
            AppError::FileError(format!("复制系统 Battle.net 目录到暂存快照失败: {error}"))
        })?;
        if !staged_bnet_config.is_file() {
            return Err(AppError::FileError(format!(
                "暂存快照缺少 Battle.net.config: {}",
                staged_bnet_config.display()
            )));
        }
        enforce_single_instance(&staged_bnet_config)?;

        let registry_path = staged_root.join("unified_auth.json");
        backup_registry_to_json(&registry_path).map_err(|error| {
            AppError::RegistryError(format!("导出 UnifiedAuth 到暂存快照失败: {error}"))
        })?;

        let manifest = AccountRuntimeSnapshotManifest {
            schema_version: ACCOUNT_RUNTIME_SNAPSHOT_SCHEMA,
            edition: edition.canonical().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|error| AppError::FileError(format!("序列化快照清单失败: {error}")))?;
        std::fs::write(staged_root.join("snapshot.json"), manifest_json)?;
        validate_runtime_snapshot_root(&staged_root, edition)?;
        Ok(())
    })();

    if let Err(error) = stage_result {
        let _ = remove_path_if_exists(&staged_root);
        return Err(error);
    }

    Ok(PendingRuntimeSnapshot {
        staged_root,
        target_root,
        backup_root,
        staged_bnet_config,
    })
}

pub(crate) fn replace_path_with_backup(
    staged: &Path,
    target: &Path,
    backup: &Path,
) -> Result<(), AppError> {
    if !path_exists(staged)? {
        return Err(AppError::FileError(format!(
            "替换失败，临时路径不存在: {}",
            staged.display()
        )));
    }

    let mut target_exists = path_exists(target)?;
    let backup_exists = path_exists(backup)?;
    if !target_exists && backup_exists {
        std::fs::rename(backup, target).map_err(|error| {
            AppError::FileError(format!(
                "恢复上次中断的替换失败，无法将 {} 还原到 {}: {}",
                backup.display(),
                target.display(),
                error
            ))
        })?;
        target_exists = true;
    } else if backup_exists {
        remove_path_if_exists(backup)?;
    }

    let had_target = target_exists;
    if had_target {
        std::fs::rename(target, backup).map_err(|error| {
            AppError::FileError(format!(
                "替换失败，无法将 {} 移到备份 {}: {}",
                target.display(),
                backup.display(),
                error
            ))
        })?;
    }

    match std::fs::rename(staged, target) {
        Ok(()) => {
            if had_target {
                // 新路径已安装成功后才清理备份；清理失败留待下次容错处理。
                let _ = remove_path_if_exists(backup);
            }
            Ok(())
        }
        Err(install_error) if !had_target => Err(AppError::FileError(format!(
            "替换失败，无法安装 {} 到 {}: {}",
            staged.display(),
            target.display(),
            install_error
        ))),
        Err(install_error) => match std::fs::rename(backup, target) {
            Ok(()) => Err(AppError::FileError(format!(
                "替换失败，已回滚原路径 {}: {}",
                target.display(),
                install_error
            ))),
            Err(rollback_error) => Err(AppError::FileError(format!(
                "替换失败且回滚失败；原数据保留在 {}。安装错误: {}；回滚错误: {}",
                backup.display(),
                install_error,
                rollback_error
            ))),
        },
    }
}

fn replace_battle_net_snapshot(source: &Path, target: &Path) -> Result<PathBuf, AppError> {
    if !source.is_dir() {
        return Err(AppError::FileError(format!(
            "系统 Battle.net 目录不存在或不可用: {}",
            source.display()
        )));
    }

    let source_config = source.join("Battle.net.config");
    if !source_config.is_file() {
        return Err(AppError::FileError(format!(
            "系统 Battle.net.config 不存在或不可用: {}",
            source_config.display()
        )));
    }

    let staged = sibling_with_suffix(target, ".tmp")?;
    let backup = sibling_with_suffix(target, ".bak")?;

    // 上次异常退出留下的临时数据可尽力清理，但绝不能混入本次快照。
    let _ = remove_path_if_exists(&staged);
    if path_exists(&staged)? {
        return Err(AppError::FileError(format!(
            "无法清理 Battle.net 临时目录: {}",
            staged.display()
        )));
    }

    crate::commands::utils::copy_dir_recursive(source, &staged).map_err(|error| {
        AppError::FileError(format!(
            "复制系统 Battle.net 目录到 {} 失败: {}",
            staged.display(),
            error
        ))
    })?;

    let staged_config = staged.join("Battle.net.config");
    if !staged_config.is_file() {
        return Err(AppError::FileError(format!(
            "Battle.net 快照缺少 Battle.net.config: {}",
            staged_config.display()
        )));
    }
    enforce_single_instance(&staged_config)?;

    replace_path_with_backup(&staged, target, &backup)?;
    let installed_config = target.join("Battle.net.config");
    if !installed_config.is_file() {
        return Err(AppError::FileError(format!(
            "Battle.net 快照替换后缺少配置文件: {}",
            installed_config.display()
        )));
    }
    Ok(installed_config)
}

/// 将系统当前状态回写到账号备份（战网优雅退出后调用）。
/// runtime、来源清单和 account.json 在账号目录副本中构建，最后一次交换生效。
pub fn sync_back_to_account(
    account_dir: &std::path::Path,
    cfg: &GlobalConfig,
) -> Result<(), AppError> {
    sync_back_to_account_inner(account_dir, cfg, None)
}

/// 方案启动回写认证快照时，显式保留账号默认 Mod，避免临时启动参数进入账号库。
pub fn sync_back_to_account_preserving_mod(
    account_dir: &std::path::Path,
    cfg: &GlobalConfig,
    default_mod_args: &str,
) -> Result<(), AppError> {
    sync_back_to_account_inner(account_dir, cfg, Some(default_mod_args))
}

fn sync_back_to_account_inner(
    account_dir: &std::path::Path,
    cfg: &GlobalConfig,
    preserved_mod_args: Option<&str>,
) -> Result<(), AppError> {
    let meta_path = account_dir.join("account.json");
    let mut meta: AccountMeta = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)
        .map_err(|error| AppError::FileError(format!("读取 account.json 失败: {error}")))?;
    let context = LaunchContext::for_account(cfg, &meta, ContextPurpose::BattleNetOnly)?;
    let pending = stage_account_directory(account_dir)?;
    let build_result = (|| -> Result<(), AppError> {
        let runtime =
            stage_runtime_snapshot(&pending.staged_root, cfg, context.installation.edition)?;
        if let Some(default_mod_args) = preserved_mod_args {
            meta.mod_args = default_mod_args.to_string();
            update_mod_args(
                &runtime.staged_bnet_config,
                context.edition.battle_net_config_game_key,
                default_mod_args,
            )?;
        } else if meta.mod_args.is_empty() {
            if let Some(args) = try_read_mod_args(
                &runtime.staged_bnet_config,
                context.edition.battle_net_config_game_key,
            ) {
                meta.mod_args = args;
            }
        } else {
            update_mod_args(
                &runtime.staged_bnet_config,
                context.edition.battle_net_config_game_key,
                &meta.mod_args,
            )?;
        }
        meta.initialized = true;
        meta.last_reset_at = Some(chrono::Utc::now().to_rfc3339());
        meta.snapshot_edition = Some(context.installation.edition.canonical().to_string());
        runtime.commit()?;
        validate_runtime_snapshot_root(
            &pending.staged_root.join("runtime"),
            context.installation.edition,
        )?;
        write_account_meta_to_directory(&pending.staged_root, &meta)
    })();
    if let Err(error) = build_result {
        pending.discard();
        return Err(error);
    }
    pending.commit()
}

/// 强制确保 Battle.net.config 中 SingleInstance 为 "true"
pub fn enforce_single_instance(config_path: &Path) -> Result<(), AppError> {
    if !config_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(config_path)?;
    let mut config: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(client) = config.get_mut("Client") {
        if let Some(obj) = client.as_object_mut() {
            let current = obj
                .get("SingleInstance")
                .and_then(|v| v.as_str())
                .unwrap_or("true");
            if current != "true" {
                obj.insert(
                    "SingleInstance".to_string(),
                    serde_json::Value::String("true".to_string()),
                );
                let new_content = serde_json::to_string_pretty(&config)?;
                std::fs::write(config_path, new_content)?;
            }
        }
    }
    Ok(())
}

fn kill_browser_processes_blocking(_browser_type: &str) {
    close_browser_login_windows();
}

#[cfg(target_os = "windows")]
fn is_browser_process(pid: u32) -> bool {
    use sysinfo::System;
    let mut sys = System::new();
    let sys_pid = sysinfo::Pid::from(pid as usize);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sys_pid]));
    if let Some(proc) = sys.process(sys_pid) {
        let name = proc.name().to_string_lossy().to_lowercase();
        return name.contains("chrome") || name.contains("msedge");
    }
    false
}

/// 强制杀掉指定浏览器 Profile 的进程，以自动关闭在初始化/重置中打开的配置浏览器
#[cfg(target_os = "windows")]
pub(crate) fn close_browser_login_windows() {
    // 方案 B：使用原生 Win32 EnumWindows + WM_CLOSE 替代不稳定的 PowerShell 发送按键
    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
        fn GetClassNameW(hWnd: isize, lpClassName: *mut u16, nMaxCount: i32) -> i32;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
        fn PostMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> i32;
    }

    const WM_CLOSE: u32 = 0x0010;

    unsafe extern "system" fn close_bnet_window_callback(hwnd: isize, _lparam: isize) -> i32 {
        let mut title = [0u16; 512];
        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), 512);
        if len > 0 {
            let title_str = String::from_utf16_lossy(&title[..len as usize]).to_lowercase();
            if title_str.contains("battle.net")
                || title_str.contains("blizzard")
                || title_str.contains("战网")
                || title_str.contains("暴雪")
            {
                let mut class_name = [0u16; 256];
                let class_len = GetClassNameW(hwnd, class_name.as_mut_ptr(), 256);
                if class_len > 0 {
                    let class_str = String::from_utf16_lossy(&class_name[..class_len as usize]);
                    if class_str == "Chrome_WidgetWin_1" {
                        let mut pid = 0u32;
                        GetWindowThreadProcessId(hwnd, &mut pid);
                        if pid != 0 && is_browser_process(pid) {
                            // 发送 WM_CLOSE 消息给浏览器窗口
                            let _ = PostMessageW(hwnd, WM_CLOSE, 0, 0);
                        }
                    }
                }
            }
        }
        1 // 继续枚举
    }

    unsafe {
        EnumWindows(close_bnet_window_callback, 0);
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn close_browser_login_windows() {}

#[tauri::command]
pub fn move_game_window(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<(), AppError> {
    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    let meta = AccountManager::load_meta(&cfg.accounts_dir, &account_id)?;
    if meta.window_x.is_none() && meta.window_y.is_none() {
        return Ok(());
    }

    let x = meta.window_x.unwrap_or(0);
    let y = meta.window_y.unwrap_or(0);

    let display = if meta.display_name.is_empty() {
        &meta.id
    } else {
        &meta.display_name
    };
    let windows = crate::commands::system::SystemGameWindowPort;
    let facade = state.multi_instance().facade();
    facade.move_account_window(&windows, &account_id, display, WindowPosition { x, y });
    Ok(())
}
