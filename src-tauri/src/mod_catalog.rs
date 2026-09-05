//! Shared Mod catalog derived from installed folders plus user-owned argument overrides.
//!
//! Scanned folder names are immutable identities. Accounts and launch schemes
//! keep their historical argument strings as compatibility mirrors, while all
//! editing is centralized in this versioned sidecar-backed catalog.

use crate::audio_mod::{
    active_mod_name, arguments_with_audio_mod, ensure_audio_mod_not_in_use, installed_mods,
    read_room_toolbar_visible, set_auto_exit_on_death_enabled, set_room_toolbar_visible, InstalledMod,
};
use crate::commands::account::{
    update_account_mods_inner, update_account_mods_with_lease_held, AccountManager, AccountMeta,
};
use crate::commands::global_config::mutate_loaded_global_config;
use crate::commands::launch::parse_windows_command_line;
use crate::domain::account::GameRegion;
use crate::domain::config::GlobalConfig;
use crate::infrastructure::module_config::ModuleConfigStore;
use crate::state::SharedState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MODULE_ID: &str = "mod-catalog";
const SCHEMA_VERSION: u32 = 1;
const MAX_ARGUMENT_LENGTH: usize = 2_048;
static CATALOG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct ModCatalogPayload {
    argument_overrides: BTreeMap<String, String>,
    custom_entries: Vec<CustomModEntry>,
    legacy_import_completed: bool,
    pending_argument_update: Option<PendingCatalogArgumentUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingCatalogArgumentUpdate {
    capsule_id: String,
    accounts: Vec<AccountModJournalEntry>,
    scheme_members: Vec<SchemeModJournalEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountModJournalEntry {
    account_id: String,
    old_active: String,
    old_list: Vec<String>,
    new_active: String,
    new_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchemeModJournalEntry {
    group_id: String,
    account_id: String,
    old_arguments: Option<String>,
    new_arguments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CustomModEntry {
    id: String,
    edition: String,
    launch_arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModCapsule {
    pub id: String,
    pub edition: String,
    pub name: String,
    pub origin: String,
    pub launch_arguments: String,
    pub default_launch_arguments: Option<String>,
    pub source_mod_name: Option<String>,
    pub feature_groups: Vec<String>,
    pub auto_exit_on_death_enabled: bool,
    pub room_toolbar_visible: Option<bool>,
    pub processed: bool,
    pub source_eligible: bool,
    pub update_required: bool,
    pub ready: bool,
    pub deletable: bool,
    pub assigned_account_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModCapsuleAccountSelection {
    pub account_id: String,
    pub account_name: String,
    pub edition: Option<String>,
    pub selected_capsule_id: Option<String>,
    pub legacy_mod_arguments: String,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModCapsulePool {
    pub generation: u64,
    pub scanned_at: String,
    pub capsules: Vec<ModCapsule>,
    pub accounts: Vec<ModCapsuleAccountSelection>,
}

#[derive(Debug, Clone)]
struct ScannedMod {
    id: String,
    edition: String,
    installed: InstalledMod,
    default_arguments: String,
}

fn catalog_store(state: &SharedState) -> Result<ModuleConfigStore, String> {
    ModuleConfigStore::new(&state.app_data_dir, MODULE_ID, SCHEMA_VERSION)
        .map_err(|error| error.to_string())
}

fn normalize_edition(value: &str) -> Result<String, String> {
    match value.trim().to_ascii_uppercase().as_str() {
        "CN" => Ok("CN".to_string()),
        "GLOBAL" => Ok("Global".to_string()),
        _ => Err("Mod 胶囊版本只能是 CN 或 Global".to_string()),
    }
}

fn account_edition(config: &GlobalConfig, account: &AccountMeta) -> Option<String> {
    if let Some(region) = account.region.as_deref() {
        return GameRegion::parse(region)
            .ok()
            .map(|region| region.edition().canonical().to_string());
    }
    match (
        !config.cn_game_path.trim().is_empty(),
        !config.global_game_path.trim().is_empty(),
    ) {
        (true, false) => Some("CN".to_string()),
        (false, true) => Some("Global".to_string()),
        _ => None,
    }
}

fn scanned_capsule_id(edition: &str, name: &str) -> String {
    format!(
        "scan:{}:{}",
        edition.to_ascii_lowercase(),
        name.trim().to_ascii_lowercase()
    )
}

fn scan_installations(config: &GlobalConfig) -> Vec<ScannedMod> {
    let installations = [
        ("CN", config.cn_game_path.trim()),
        ("Global", config.global_game_path.trim()),
    ];
    let mut scanned = Vec::new();
    for (edition, game_directory) in installations {
        if game_directory.is_empty() || !Path::new(game_directory).is_dir() {
            continue;
        }
        for installed in installed_mods(&Path::new(game_directory).join("mods")) {
            let Ok(default_arguments) = arguments_with_audio_mod("", &installed.name) else {
                continue;
            };
            scanned.push(ScannedMod {
                id: scanned_capsule_id(edition, &installed.name),
                edition: edition.to_string(),
                installed,
                default_arguments,
            });
        }
    }
    scanned.sort_by(|left, right| {
        left.edition.cmp(&right.edition).then_with(|| {
            left.installed
                .name
                .to_lowercase()
                .cmp(&right.installed.name.to_lowercase())
        })
    });
    scanned
}

fn metadata_is_link_or_reparse_point(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn validate_safe_mod_deletion_tree(
    mods_directory: &Path,
    mod_directory: &Path,
) -> Result<(), String> {
    let mods_metadata = std::fs::symlink_metadata(mods_directory)
        .map_err(|error| format!("无法检查 mods 目录 {}：{error}", mods_directory.display()))?;
    if !mods_metadata.is_dir() || metadata_is_link_or_reparse_point(&mods_metadata) {
        return Err(format!(
            "拒绝从非普通目录删除 Mod：{}",
            mods_directory.display()
        ));
    }

    let mod_metadata = std::fs::symlink_metadata(mod_directory)
        .map_err(|error| format!("无法检查 Mod 文件夹 {}：{error}", mod_directory.display()))?;
    if !mod_metadata.is_dir() || metadata_is_link_or_reparse_point(&mod_metadata) {
        return Err(format!(
            "拒绝删除链接或重解析点形式的 Mod 文件夹：{}",
            mod_directory.display()
        ));
    }

    let canonical_mods = std::fs::canonicalize(mods_directory)
        .map_err(|error| format!("无法规范化 mods 目录 {}：{error}", mods_directory.display()))?;
    let canonical_mod = std::fs::canonicalize(mod_directory)
        .map_err(|error| format!("无法规范化 Mod 文件夹 {}：{error}", mod_directory.display()))?;
    if canonical_mod.parent() != Some(canonical_mods.as_path()) {
        return Err(format!(
            "拒绝删除 mods 目录之外的文件夹：{}",
            mod_directory.display()
        ));
    }

    let mut pending = vec![canonical_mod];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| format!("无法检查 Mod 文件夹 {}：{error}", directory.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("无法读取 Mod 文件夹 {}：{error}", directory.display()))?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("无法检查 Mod 文件 {}：{error}", path.display()))?;
            if metadata_is_link_or_reparse_point(&metadata) {
                return Err(format!(
                    "Mod 文件夹包含链接或重解析点，已拒绝删除：{}",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(())
}

fn delete_scanned_mod_directory(
    config: &GlobalConfig,
    edition: &str,
    mod_name: &str,
) -> Result<(), String> {
    let game_directory = match edition {
        "CN" => config.cn_game_path.trim(),
        "Global" => config.global_game_path.trim(),
        _ => return Err(format!("无法识别 Mod 所属游戏版本：{edition}")),
    };
    if game_directory.is_empty() {
        return Err(format!("尚未配置{edition}游戏目录"));
    }
    let mods_directory = PathBuf::from(game_directory).join("mods");
    let mod_directory = mods_directory.join(mod_name);
    validate_safe_mod_deletion_tree(&mods_directory, &mod_directory)?;
    std::fs::remove_dir_all(&mod_directory)
        .map_err(|error| format!("无法删除 Mod 文件夹 {}：{error}", mod_directory.display()))
}

fn validate_arguments(arguments: &str) -> Result<String, String> {
    let arguments = arguments.trim();
    if arguments.is_empty() {
        return Err("自定义 Mod 参数不能为空；原版游戏请直接选择“不使用 Mod”".to_string());
    }
    if arguments.len() > MAX_ARGUMENT_LENGTH {
        return Err(format!("Mod 参数不能超过 {MAX_ARGUMENT_LENGTH} 个字符"));
    }
    parse_windows_command_line(arguments).map_err(|error| format!("Mod 参数无法解析：{error}"))?;
    Ok(arguments.to_string())
}

fn effective_scanned_arguments(payload: &ModCatalogPayload, scanned: &ScannedMod) -> String {
    payload
        .argument_overrides
        .get(&scanned.id)
        .cloned()
        .unwrap_or_else(|| scanned.default_arguments.clone())
}

fn custom_display_name(arguments: &str) -> String {
    active_mod_name(arguments)
        .ok()
        .flatten()
        .unwrap_or_else(|| "自定义参数".to_string())
}

fn legacy_arguments(config: &GlobalConfig) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut editions = HashMap::new();
    for account_id in AccountManager::list_ids(&config.accounts_dir) {
        let Ok(account) = AccountManager::load_meta(&config.accounts_dir, &account_id) else {
            continue;
        };
        let Some(edition) = account_edition(config, &account) else {
            continue;
        };
        editions.insert(account.id.clone(), edition.clone());
        for arguments in account
            .mod_list
            .iter()
            .chain(std::iter::once(&account.mod_args))
        {
            if !arguments.trim().is_empty() {
                result.push((edition.clone(), arguments.trim().to_string()));
            }
        }
    }
    for group in &config.launch_groups {
        for member in &group.members {
            let Some(arguments) = member.mod_args.as_deref() else {
                continue;
            };
            let Some(edition) = editions.get(&member.account_id) else {
                continue;
            };
            if !arguments.trim().is_empty() {
                result.push((edition.clone(), arguments.trim().to_string()));
            }
        }
    }
    result
}

fn merge_legacy_entries(
    config: &GlobalConfig,
    scanned: &[ScannedMod],
    payload: &mut ModCatalogPayload,
) -> bool {
    let mut known = scanned
        .iter()
        .map(|entry| {
            (
                entry.edition.clone(),
                effective_scanned_arguments(payload, entry),
            )
        })
        .chain(
            payload
                .custom_entries
                .iter()
                .map(|entry| (entry.edition.clone(), entry.launch_arguments.clone())),
        )
        .collect::<HashSet<_>>();
    let mut changed = false;
    for (edition, arguments) in legacy_arguments(config) {
        if known.insert((edition.clone(), arguments.clone())) {
            payload.custom_entries.push(CustomModEntry {
                id: format!("custom:{}", uuid::Uuid::new_v4().simple()),
                edition,
                launch_arguments: arguments,
            });
            changed = true;
        }
    }
    changed
}

fn load_payload(
    state: &SharedState,
    config: &GlobalConfig,
    scanned: &[ScannedMod],
) -> Result<(u64, ModCatalogPayload), String> {
    let store = catalog_store(state)?;
    let loaded = store
        .load::<ModCatalogPayload>()
        .map_err(|error| error.to_string())?;
    let (generation, mut payload, missing) = match loaded {
        Some(envelope) => (envelope.generation, envelope.payload, false),
        None => (0, ModCatalogPayload::default(), true),
    };
    let migrated = if payload.legacy_import_completed {
        false
    } else {
        merge_legacy_entries(config, scanned, &mut payload);
        payload.legacy_import_completed = true;
        true
    };
    if missing || migrated {
        let saved = store
            .save_if_generation(generation, payload)
            .map_err(|error| error.to_string())?;
        Ok((saved.generation, saved.payload))
    } else {
        Ok((generation, payload))
    }
}

fn save_payload(
    state: &SharedState,
    generation: u64,
    payload: ModCatalogPayload,
) -> Result<(u64, ModCatalogPayload), String> {
    let saved = catalog_store(state)?
        .save_if_generation(generation, payload)
        .map_err(|error| error.to_string())?;
    Ok((saved.generation, saved.payload))
}

fn load_payload_with_recovery(
    state: &SharedState,
    app: &tauri::AppHandle,
    config: &GlobalConfig,
    scanned: &[ScannedMod],
) -> Result<(u64, ModCatalogPayload), String> {
    let (generation, mut payload) = load_payload(state, config, scanned)?;
    let Some(pending) = payload.pending_argument_update.clone() else {
        return Ok((generation, payload));
    };

    let _account_catalog_lease = state.multi_instance().catalog_leases().acquire();
    let _account_leases = state
        .multi_instance()
        .account_leases()
        .try_acquire_many(
            pending
                .accounts
                .iter()
                .map(|change| change.account_id.as_str()),
        )
        .map_err(|error| format!("恢复未完成的 Mod 目录事务失败: {error}"))?;
    apply_account_mod_journal(config, &pending.accounts, false)
        .map_err(|error| format!("恢复未完成的 Mod 账号引用失败: {error}"))?;
    apply_scheme_mod_replacements(state, app, &pending.scheme_members, false)
        .map_err(|error| format!("恢复未完成的 Mod 启动方案引用失败: {error}"))?;

    payload.pending_argument_update = None;
    let saved = save_payload(state, generation, payload)?;
    crate::logger::log_msg(
        "WARN",
        "ModCatalog",
        &format!("已回滚上次中断的 Mod 目录编辑事务: {}", pending.capsule_id),
    );
    Ok(saved)
}

fn capsule_feature_metadata(
    scanned: &[ScannedMod],
    edition: &str,
    arguments: &str,
) -> (Vec<String>, Option<String>, bool, bool, bool, bool) {
    let active_name = active_mod_name(arguments).ok().flatten();
    let related = active_name.as_deref().and_then(|name| {
        scanned.iter().find(|entry| {
            entry.edition == edition && entry.installed.name.eq_ignore_ascii_case(name)
        })
    });
    related.map_or_else(
        || (Vec::new(), None, false, false, false, false),
        |entry| {
            let processed =
                !entry.installed.feature_groups.is_empty() || entry.installed.update_required;
            (
                entry.installed.feature_groups.clone(),
                entry.installed.source_mod_name.clone(),
                processed,
                entry.installed.update_required,
                entry.installed.source_eligible,
                entry.installed.auto_exit_on_death_enabled,
            )
        },
    )
}

fn build_pool(
    config: &GlobalConfig,
    generation: u64,
    payload: &ModCatalogPayload,
    scanned: &[ScannedMod],
) -> ModCapsulePool {
    let mut capsules = scanned
        .iter()
        .map(|entry| {
            let processed =
                !entry.installed.feature_groups.is_empty() || entry.installed.update_required;
            ModCapsule {
                id: entry.id.clone(),
                edition: entry.edition.clone(),
                name: entry.installed.name.clone(),
                origin: "scanned".to_string(),
                launch_arguments: effective_scanned_arguments(payload, entry),
                default_launch_arguments: Some(entry.default_arguments.clone()),
                source_mod_name: entry.installed.source_mod_name.clone(),
                feature_groups: entry.installed.feature_groups.clone(),
                auto_exit_on_death_enabled: entry.installed.auto_exit_on_death_enabled,
                room_toolbar_visible: if entry
                    .installed
                    .feature_groups
                    .iter()
                    .any(|group| group == "in_game_room_tools")
                {
                    let game_directory = if entry.edition == "CN" {
                        config.cn_game_path.trim()
                    } else {
                        config.global_game_path.trim()
                    };
                    read_room_toolbar_visible(
                        &Path::new(game_directory).join("mods"),
                        &entry.installed.name,
                    )
                    .ok()
                } else {
                    None
                },
                processed,
                source_eligible: entry.installed.source_eligible,
                update_required: entry.installed.update_required,
                ready: true,
                deletable: true,
                assigned_account_ids: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    capsules.extend(payload.custom_entries.iter().map(|entry| {
        let (
            feature_groups,
            source_mod_name,
            processed,
            update_required,
            source_eligible,
            auto_exit_on_death_enabled,
        ) = capsule_feature_metadata(scanned, &entry.edition, &entry.launch_arguments);
        ModCapsule {
            id: entry.id.clone(),
            edition: entry.edition.clone(),
            name: custom_display_name(&entry.launch_arguments),
            origin: "custom".to_string(),
            launch_arguments: entry.launch_arguments.clone(),
            default_launch_arguments: None,
            source_mod_name,
            feature_groups,
            auto_exit_on_death_enabled,
            room_toolbar_visible: None,
            processed,
            source_eligible,
            update_required,
            ready: true,
            deletable: true,
            assigned_account_ids: Vec::new(),
        }
    }));
    capsules.sort_by(|left, right| {
        left.edition
            .cmp(&right.edition)
            .then_with(|| left.origin.cmp(&right.origin))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    let mut selections = Vec::new();
    for account_id in AccountManager::list_ids(&config.accounts_dir) {
        let account = match AccountManager::load_meta(&config.accounts_dir, &account_id) {
            Ok(account) if account.initialized => account,
            Ok(_) => continue,
            Err(error) => {
                selections.push(ModCapsuleAccountSelection {
                    account_id: account_id.clone(),
                    account_name: account_id,
                    edition: None,
                    selected_capsule_id: None,
                    legacy_mod_arguments: String::new(),
                    issue: Some(error.to_string()),
                });
                continue;
            }
        };
        let edition = account_edition(config, &account);
        let selected_capsule_id = edition.as_deref().and_then(|edition| {
            capsules
                .iter()
                .find(|capsule| {
                    capsule.edition == edition
                        && capsule.launch_arguments.trim() == account.mod_args.trim()
                })
                .map(|capsule| capsule.id.clone())
        });
        let issue = (!account.mod_args.trim().is_empty() && selected_capsule_id.is_none())
            .then(|| "旧 Mod 参数尚未合并到共享目录，请重新扫描".to_string());
        selections.push(ModCapsuleAccountSelection {
            account_id: account.id.clone(),
            account_name: if account.display_name.trim().is_empty() {
                account.id
            } else {
                account.display_name
            },
            edition,
            selected_capsule_id,
            legacy_mod_arguments: account.mod_args,
            issue,
        });
    }

    let mut assigned = HashMap::<String, Vec<String>>::new();
    for selection in &selections {
        if let Some(capsule_id) = &selection.selected_capsule_id {
            assigned
                .entry(capsule_id.clone())
                .or_default()
                .push(selection.account_id.clone());
        }
    }
    for capsule in &mut capsules {
        capsule.assigned_account_ids = assigned.remove(&capsule.id).unwrap_or_default();
    }
    selections.sort_by(|left, right| left.account_name.cmp(&right.account_name));

    ModCapsulePool {
        generation,
        scanned_at: chrono::Local::now().to_rfc3339(),
        capsules,
        accounts: selections,
    }
}

fn scan_pool_locked(state: &SharedState, app: &tauri::AppHandle) -> Result<ModCapsulePool, String> {
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let scanned = scan_installations(&config);
    let (generation, payload) = load_payload_with_recovery(state, app, &config, &scanned)?;
    let config = state.configuration().snapshot().unwrap_or(config);
    Ok(build_pool(&config, generation, &payload, &scanned))
}

pub(crate) fn refresh_on_startup(state: SharedState, app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let _catalog = CATALOG_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Err(error) = scan_pool_locked(&state, &app) {
            crate::logger::log_msg("WARN", "ModCatalog", &format!("启动扫描失败：{error}"));
        }
    });
}

#[tauri::command]
pub async fn get_mod_capsule_pool(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<ModCapsulePool, String> {
    let shared = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let _catalog = CATALOG_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        scan_pool_locked(&shared, &app)
    })
    .await
    .map_err(|error| format!("读取 Mod 共享目录的后台任务异常退出：{error}"))?
}

#[tauri::command]
pub async fn scan_mod_capsule_pool(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<ModCapsulePool, String> {
    get_mod_capsule_pool(app, state).await
}

#[tauri::command]
pub fn open_mods_directory(
    state: tauri::State<'_, SharedState>,
    edition: String,
) -> Result<(), String> {
    let edition = normalize_edition(&edition)?;
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let game_directory = if edition == "CN" {
        config.cn_game_path.trim()
    } else {
        config.global_game_path.trim()
    };
    if game_directory.is_empty() {
        return Err(format!("尚未配置{edition}游戏目录"));
    }
    let game_directory = Path::new(game_directory);
    if !game_directory.is_dir() {
        return Err(format!("游戏目录不存在：{}", game_directory.display()));
    }
    let mods_directory = game_directory.join("mods");
    std::fs::create_dir_all(&mods_directory)
        .map_err(|error| format!("无法创建 mods 文件夹 {}：{error}", mods_directory.display()))?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&mods_directory)
        .spawn()
        .map_err(|error| format!("打开 mods 文件夹失败：{error}"))?;
    #[cfg(not(target_os = "windows"))]
    std::process::Command::new("open")
        .arg(&mods_directory)
        .spawn()
        .map_err(|error| format!("打开 mods 文件夹失败：{error}"))?;
    Ok(())
}

#[tauri::command]
pub fn set_mod_room_toolbar_visible(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    capsule_id: String,
    visible: bool,
) -> Result<ModCapsulePool, String> {
    let _catalog = CATALOG_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let scanned = scan_installations(&config);
    let _ = load_payload_with_recovery(state.inner(), &app, &config, &scanned)?;
    let config = state.configuration().snapshot().unwrap_or(config);
    let target = scanned
        .iter()
        .find(|entry| entry.id == capsule_id)
        .ok_or_else(|| "只能切换游戏目录中实际存在的 Mod".to_string())?;
    let game_directory = if target.edition == "CN" {
        config.cn_game_path.trim()
    } else {
        config.global_game_path.trim()
    };
    set_room_toolbar_visible(
        state.inner(),
        &config,
        &Path::new(game_directory).join("mods"),
        &target.installed.name,
        visible,
    )?;
    let rescanned = scan_installations(&config);
    let (generation, payload) =
        load_payload_with_recovery(state.inner(), &app, &config, &rescanned)?;
    Ok(build_pool(&config, generation, &payload, &rescanned))
}

#[tauri::command]
pub fn set_mod_auto_exit_on_death_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    capsule_id: String,
    enabled: bool,
) -> Result<ModCapsulePool, String> {
    let _catalog = CATALOG_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let scanned = scan_installations(&config);
    let _ = load_payload_with_recovery(state.inner(), &app, &config, &scanned)?;
    let config = state.configuration().snapshot().unwrap_or(config);
    let target = scanned
        .iter()
        .find(|entry| entry.id == capsule_id)
        .cloned()
        .ok_or_else(|| "只能切换游戏目录中实际存在的 Mod".to_string())?;
    if !target
        .installed
        .feature_groups
        .iter()
        .any(|group| group == "auto_exit_on_death")
    {
        return Err(format!(
            "Mod“{}”不支持死亡后自动退房",
            target.installed.name
        ));
    }
    ensure_audio_mod_not_in_use(state.inner(), &config, &target.installed.name)?;
    let game_directory = if target.edition == "CN" {
        config.cn_game_path.trim()
    } else {
        config.global_game_path.trim()
    };
    let mods_directory = Path::new(game_directory).join("mods");
    set_auto_exit_on_death_enabled(&mods_directory, &target.installed.name, enabled)?;

    let rescanned = scan_installations(&config);
    let (generation, payload) =
        load_payload_with_recovery(state.inner(), &app, &config, &rescanned)?;
    Ok(build_pool(&config, generation, &payload, &rescanned))
}

#[tauri::command]
pub fn add_mod_capsule(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    edition: String,
    launch_arguments: String,
) -> Result<ModCapsulePool, String> {
    let _catalog = CATALOG_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let edition = normalize_edition(&edition)?;
    let launch_arguments = validate_arguments(&launch_arguments)?;
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let scanned = scan_installations(&config);
    let (generation, mut payload) =
        load_payload_with_recovery(state.inner(), &app, &config, &scanned)?;
    let config = state.configuration().snapshot().unwrap_or(config);
    let pool = build_pool(&config, generation, &payload, &scanned);
    if pool
        .capsules
        .iter()
        .any(|capsule| capsule.edition == edition && capsule.launch_arguments == launch_arguments)
    {
        return Err("共享目录中已有完全相同的 Mod 参数".to_string());
    }
    payload.custom_entries.push(CustomModEntry {
        id: format!("custom:{}", uuid::Uuid::new_v4().simple()),
        edition,
        launch_arguments,
    });
    let (generation, payload) = save_payload(state.inner(), generation, payload)?;
    Ok(build_pool(&config, generation, &payload, &scanned))
}

#[derive(Clone)]
struct AccountModReplacement {
    original: AccountMeta,
    active: String,
    list: Vec<String>,
}

impl AccountModReplacement {
    fn journal_entry(&self) -> AccountModJournalEntry {
        AccountModJournalEntry {
            account_id: self.original.id.clone(),
            old_active: self.original.mod_args.clone(),
            old_list: self.original.mod_list.clone(),
            new_active: self.active.clone(),
            new_list: self.list.clone(),
        }
    }
}

fn plan_catalog_argument_replacements(
    config: &GlobalConfig,
    old_arguments: &str,
    new_arguments: &str,
) -> Result<Vec<AccountModReplacement>, String> {
    let mut changes = Vec::new();
    for account_id in AccountManager::list_ids(&config.accounts_dir) {
        let account = AccountManager::load_meta(&config.accounts_dir, &account_id)
            .map_err(|error| error.to_string())?;
        let active = if account.mod_args.trim() == old_arguments {
            new_arguments.to_string()
        } else {
            account.mod_args.clone()
        };
        let mut changed = active != account.mod_args;
        let list = account
            .mod_list
            .iter()
            .map(|arguments| {
                if arguments.trim() == old_arguments {
                    changed = true;
                    new_arguments.to_string()
                } else {
                    arguments.clone()
                }
            })
            .collect::<Vec<_>>();
        if changed {
            let mut normalized = account.clone();
            normalized.replace_mod_configurations(active, list);
            changes.push(AccountModReplacement {
                original: account,
                active: normalized.mod_args,
                list: normalized.mod_list,
            });
        }
    }
    Ok(changes)
}

fn restore_account_mod_replacements(
    config: &GlobalConfig,
    changes: &[AccountModReplacement],
) -> Vec<String> {
    let mut errors = Vec::new();
    for change in changes.iter().rev() {
        if let Err(error) = update_account_mods_with_lease_held(
            config,
            change.original.clone(),
            change.original.mod_args.clone(),
            change.original.mod_list.clone(),
        ) {
            errors.push(format!("账号 {}: {error}", change.original.id));
        }
    }
    errors
}

fn apply_account_mod_replacements(
    config: &GlobalConfig,
    changes: &[AccountModReplacement],
) -> Result<(), String> {
    for (index, change) in changes.iter().enumerate() {
        if let Err(error) = update_account_mods_with_lease_held(
            config,
            change.original.clone(),
            change.active.clone(),
            change.list.clone(),
        ) {
            let rollback_errors = restore_account_mod_replacements(config, &changes[..index]);
            return Err(if rollback_errors.is_empty() {
                error.to_string()
            } else {
                format!(
                    "{error}；回滚已更新账号时发生错误：{}",
                    rollback_errors.join("；")
                )
            });
        }
    }
    Ok(())
}

fn apply_account_mod_journal(
    config: &GlobalConfig,
    changes: &[AccountModJournalEntry],
    forward: bool,
) -> Result<(), String> {
    for change in changes {
        let account = AccountManager::load_meta(&config.accounts_dir, &change.account_id)
            .map_err(|error| error.to_string())?;
        let (expected_active, expected_list, target_active, target_list) = if forward {
            (
                &change.old_active,
                &change.old_list,
                &change.new_active,
                &change.new_list,
            )
        } else {
            (
                &change.new_active,
                &change.new_list,
                &change.old_active,
                &change.old_list,
            )
        };
        if &account.mod_args == target_active && &account.mod_list == target_list {
            continue;
        }
        if &account.mod_args != expected_active || &account.mod_list != expected_list {
            return Err(format!(
                "账号 {} 的 Mod 配置已被其他操作修改，停止恢复目录事务",
                change.account_id
            ));
        }
        update_account_mods_with_lease_held(
            config,
            account,
            target_active.clone(),
            target_list.clone(),
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn plan_catalog_argument_replacements_in_schemes(
    config: &GlobalConfig,
    old_arguments: &str,
    new_arguments: &str,
) -> Vec<SchemeModJournalEntry> {
    config
        .launch_groups
        .iter()
        .flat_map(|group| {
            group
                .members
                .iter()
                .filter(|member| {
                    member
                        .mod_args
                        .as_deref()
                        .is_some_and(|arguments| arguments.trim() == old_arguments)
                })
                .map(|member| SchemeModJournalEntry {
                    group_id: group.id.clone(),
                    account_id: member.account_id.clone(),
                    old_arguments: member.mod_args.clone(),
                    new_arguments: Some(new_arguments.to_string()),
                })
        })
        .collect()
}

fn apply_scheme_mod_replacements(
    state: &SharedState,
    app: &tauri::AppHandle,
    changes: &[SchemeModJournalEntry],
    forward: bool,
) -> Result<(), String> {
    mutate_loaded_global_config(state, app, |config| {
        let mut changed = false;
        for change in changes {
            let member = config
                .launch_groups
                .iter_mut()
                .find(|group| group.id == change.group_id)
                .and_then(|group| {
                    group
                        .members
                        .iter_mut()
                        .find(|member| member.account_id == change.account_id)
                })
                .ok_or_else(|| {
                    crate::error::AppError::ConfigWriteError(format!(
                        "启动方案 {} 中已找不到账号 {}，停止 Mod 引用事务",
                        change.group_id, change.account_id
                    ))
                })?;
            let (expected, target) = if forward {
                (&change.old_arguments, &change.new_arguments)
            } else {
                (&change.new_arguments, &change.old_arguments)
            };
            if &member.mod_args == target {
                continue;
            }
            if &member.mod_args != expected {
                return Err(crate::error::AppError::ConfigWriteError(format!(
                    "启动方案 {} 的账号 {} 已被其他操作修改，停止 Mod 引用事务",
                    change.group_id, change.account_id
                )));
            }
            member.mod_args = target.clone();
            changed = true;
        }
        Ok(changed)
    })
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_mod_capsule(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    capsule_id: String,
    launch_arguments: String,
) -> Result<ModCapsulePool, String> {
    let _catalog = CATALOG_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let launch_arguments = validate_arguments(&launch_arguments)?;
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let scanned = scan_installations(&config);
    let (generation, payload) = load_payload_with_recovery(state.inner(), &app, &config, &scanned)?;
    let config = state.configuration().snapshot().unwrap_or(config);
    let pool = build_pool(&config, generation, &payload, &scanned);
    let current = pool
        .capsules
        .iter()
        .find(|capsule| capsule.id == capsule_id)
        .cloned()
        .ok_or_else(|| "要编辑的 Mod 胶囊已不存在".to_string())?;
    if pool.capsules.iter().any(|capsule| {
        capsule.id != current.id
            && capsule.edition == current.edition
            && capsule.launch_arguments == launch_arguments
    }) {
        return Err("同一游戏版本中已有完全相同的共享 Mod 参数".to_string());
    }
    if current.launch_arguments == launch_arguments {
        return Ok(pool);
    }
    let mut next_payload = payload.clone();
    if current.origin == "scanned" {
        let selected_name = active_mod_name(&launch_arguments)?
            .ok_or_else(|| "扫描 Mod 的参数必须保留 -mod 名称".to_string())?;
        if !selected_name.eq_ignore_ascii_case(&current.name) {
            return Err(format!(
                "扫描 Mod 名称固定为“{}”，不能改为其他 Mod",
                current.name
            ));
        }
        if current.default_launch_arguments.as_deref() == Some(launch_arguments.as_str()) {
            next_payload.argument_overrides.remove(&current.id);
        } else {
            next_payload
                .argument_overrides
                .insert(current.id.clone(), launch_arguments.clone());
        }
    } else {
        let entry = next_payload
            .custom_entries
            .iter_mut()
            .find(|entry| entry.id == current.id)
            .ok_or_else(|| "自定义 Mod 参数已不存在".to_string())?;
        entry.launch_arguments = launch_arguments.clone();
    }
    let _account_catalog_lease = state.multi_instance().catalog_leases().acquire();
    let account_changes = plan_catalog_argument_replacements(
        &config,
        current.launch_arguments.trim(),
        &launch_arguments,
    )?;
    let scheme_changes = plan_catalog_argument_replacements_in_schemes(
        &config,
        current.launch_arguments.trim(),
        &launch_arguments,
    );
    let _account_leases = state
        .multi_instance()
        .account_leases()
        .try_acquire_many(
            account_changes
                .iter()
                .map(|change| change.original.id.as_str()),
        )
        .map_err(|error| error.to_string())?;

    let pending = PendingCatalogArgumentUpdate {
        capsule_id: current.id.clone(),
        accounts: account_changes
            .iter()
            .map(AccountModReplacement::journal_entry)
            .collect(),
        scheme_members: scheme_changes.clone(),
    };
    let mut prepared_payload = payload.clone();
    prepared_payload.pending_argument_update = Some(pending);
    let (prepared_generation, _) = save_payload(state.inner(), generation, prepared_payload)?;

    if let Err(error) = apply_account_mod_replacements(&config, &account_changes) {
        let rollback_errors = restore_account_mod_replacements(&config, &account_changes);
        if rollback_errors.is_empty() {
            let _ = save_payload(state.inner(), prepared_generation, payload.clone());
        }
        return Err(if rollback_errors.is_empty() {
            error
        } else {
            format!(
                "{error}；回滚账号引用时发生错误：{}",
                rollback_errors.join("；")
            )
        });
    }
    if let Err(error) = apply_scheme_mod_replacements(state.inner(), &app, &scheme_changes, true) {
        let scheme_rollback =
            apply_scheme_mod_replacements(state.inner(), &app, &scheme_changes, false).err();
        let mut rollback_errors = restore_account_mod_replacements(&config, &account_changes);
        if let Some(scheme_error) = scheme_rollback {
            rollback_errors.push(format!("启动方案: {scheme_error}"));
        }
        if rollback_errors.is_empty() {
            let _ = save_payload(state.inner(), prepared_generation, payload.clone());
        }
        return Err(if rollback_errors.is_empty() {
            error
        } else {
            format!(
                "{error}；回滚引用时发生错误：{}",
                rollback_errors.join("；")
            )
        });
    }

    next_payload.pending_argument_update = None;
    let (generation, payload) = match save_payload(state.inner(), prepared_generation, next_payload)
    {
        Ok(saved) => saved,
        Err(error) => {
            // A directory sync can report failure after the atomic rename.
            // Reload first: a journal-free next generation means the catalog
            // commit is authoritative and the already-updated references are
            // the correct state.
            if let Ok((actual_generation, actual_payload)) =
                load_payload(state.inner(), &config, &scanned)
            {
                if actual_generation > prepared_generation
                    && actual_payload.pending_argument_update.is_none()
                {
                    let latest_config = state.configuration().snapshot().unwrap_or(config);
                    return Ok(build_pool(
                        &latest_config,
                        actual_generation,
                        &actual_payload,
                        &scanned,
                    ));
                }
            }
            let scheme_rollback =
                apply_scheme_mod_replacements(state.inner(), &app, &scheme_changes, false).err();
            let account_rollback = restore_account_mod_replacements(&config, &account_changes);
            let mut rollback_errors = account_rollback;
            if let Some(scheme_error) = scheme_rollback {
                rollback_errors.push(format!("启动方案: {scheme_error}"));
            }
            if rollback_errors.is_empty() {
                if let Err(clear_error) =
                    save_payload(state.inner(), prepared_generation, payload.clone())
                {
                    rollback_errors.push(format!("清理事务日志: {clear_error}"));
                }
            }
            return Err(if rollback_errors.is_empty() {
                error
            } else {
                format!(
                    "{error}；回滚引用时发生错误：{}",
                    rollback_errors.join("；")
                )
            });
        }
    };
    let latest_config = state.configuration().snapshot().unwrap_or(config);
    Ok(build_pool(&latest_config, generation, &payload, &scanned))
}

fn capsule_usage(config: &GlobalConfig, arguments: &str) -> Vec<String> {
    let mut usage = Vec::new();
    for account_id in AccountManager::list_ids(&config.accounts_dir) {
        if let Ok(account) = AccountManager::load_meta(&config.accounts_dir, &account_id) {
            if account.mod_args.trim() == arguments {
                usage.push(if account.display_name.trim().is_empty() {
                    account.id
                } else {
                    account.display_name
                });
            }
        }
    }
    for group in &config.launch_groups {
        if group.members.iter().any(|member| {
            member
                .mod_args
                .as_deref()
                .is_some_and(|value| value.trim() == arguments)
        }) {
            usage.push(format!("启动方案：{}", group.name));
        }
    }
    usage
}

#[tauri::command]
pub fn delete_mod_capsule(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    capsule_id: String,
) -> Result<ModCapsulePool, String> {
    let _catalog = CATALOG_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let scanned = scan_installations(&config);
    let (generation, mut payload) =
        load_payload_with_recovery(state.inner(), &app, &config, &scanned)?;
    let config = state.configuration().snapshot().unwrap_or(config);
    let pool = build_pool(&config, generation, &payload, &scanned);
    let current = pool
        .capsules
        .iter()
        .find(|capsule| capsule.id == capsule_id)
        .cloned()
        .ok_or_else(|| "要删除的 Mod 参数已不存在".to_string())?;
    let usage = capsule_usage(&config, &current.launch_arguments);
    if !usage.is_empty() {
        return Err(format!("该 Mod 参数仍被以下项目使用：{}", usage.join("、")));
    }

    if current.origin == "custom" {
        let index = payload
            .custom_entries
            .iter()
            .position(|entry| entry.id == current.id)
            .ok_or_else(|| "要删除的自定义 Mod 参数已不存在".to_string())?;
        payload.custom_entries.remove(index);
        let (generation, payload) = save_payload(state.inner(), generation, payload)?;
        return Ok(build_pool(&config, generation, &payload, &scanned));
    }
    if current.origin != "scanned" {
        return Err(format!("不支持删除来源为“{}”的 Mod 参数", current.origin));
    }

    let scanned_mod = scanned
        .iter()
        .find(|entry| entry.id == current.id)
        .ok_or_else(|| "要删除的扫描 Mod 已不存在，请重新扫描后再试".to_string())?;
    ensure_audio_mod_not_in_use(state.inner(), &config, &scanned_mod.installed.name)?;
    delete_scanned_mod_directory(&config, &scanned_mod.edition, &scanned_mod.installed.name)?;
    payload.argument_overrides.remove(&current.id);
    let (generation, payload) = save_payload(state.inner(), generation, payload)?;
    let rescanned = scan_installations(&config);
    Ok(build_pool(&config, generation, &payload, &rescanned))
}

#[tauri::command]
pub fn assign_mod_capsule_to_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    capsule_id: Option<String>,
) -> Result<(), String> {
    let _catalog = CATALOG_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let scanned = scan_installations(&config);
    let (generation, payload) = load_payload_with_recovery(state.inner(), &app, &config, &scanned)?;
    let config = state.configuration().snapshot().unwrap_or(config);
    let account = AccountManager::load_meta(&config.accounts_dir, &account_id)
        .map_err(|error| error.to_string())?;
    let arguments = if let Some(capsule_id) = capsule_id {
        let pool = build_pool(&config, generation, &payload, &scanned);
        let capsule = pool
            .capsules
            .iter()
            .find(|capsule| capsule.id == capsule_id)
            .ok_or_else(|| "选择的 Mod 胶囊已不存在，请重新扫描".to_string())?;
        let edition = account_edition(&config, &account)
            .ok_or_else(|| "账号缺少明确的游戏版本，无法选择 Mod".to_string())?;
        if capsule.edition != edition {
            return Err(format!(
                "账号属于 {edition}，不能选择 {} 胶囊",
                capsule.edition
            ));
        }
        capsule.launch_arguments.clone()
    } else {
        String::new()
    };
    let mut mod_list = account.mod_list;
    if !arguments.is_empty() && !mod_list.iter().any(|entry| entry.trim() == arguments) {
        mod_list.push(arguments.clone());
    }
    update_account_mods_inner(state.inner(), account_id, arguments, mod_list)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanned_ids_are_case_insensitive_but_edition_scoped() {
        assert_eq!(
            scanned_capsule_id("Global", "MyMod"),
            scanned_capsule_id("Global", "mymod")
        );
        assert_ne!(
            scanned_capsule_id("CN", "MyMod"),
            scanned_capsule_id("Global", "MyMod")
        );
    }

    #[test]
    fn legacy_arguments_become_custom_only_when_they_do_not_match_a_scan_preset() {
        let scanned = ScannedMod {
            id: scanned_capsule_id("CN", "Sample"),
            edition: "CN".to_string(),
            installed: InstalledMod {
                name: "Sample".to_string(),
                source_mod_name: None,
                audio_ready: false,
                update_required: false,
                source_eligible: true,
                feature_groups: Vec::new(),
                audio_reusable: false,
                auto_exit_on_death_enabled: false,
            },
            default_arguments: "-mod Sample -txt -assettestmode 1".to_string(),
        };
        let mut payload = ModCatalogPayload::default();
        let known = effective_scanned_arguments(&payload, &scanned);
        payload.custom_entries.push(CustomModEntry {
            id: "custom:legacy".to_string(),
            edition: "CN".to_string(),
            launch_arguments: "-mod Sample -txt -legacy-flag".to_string(),
        });

        assert_eq!(known, "-mod Sample -txt -assettestmode 1");
        assert_ne!(payload.custom_entries[0].launch_arguments, known);
    }

    #[test]
    fn scanned_argument_edits_cannot_change_the_folder_identity() {
        let accepted = validate_arguments("-mod Sample -txt -assettestmode 1 -foo").unwrap();
        assert_eq!(
            active_mod_name(&accepted).unwrap().as_deref(),
            Some("Sample")
        );
        assert_eq!(
            active_mod_name("-mod Other -txt").unwrap().as_deref(),
            Some("Other")
        );
    }
}
