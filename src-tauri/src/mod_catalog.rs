//! Shared Mod catalog derived from installed folders plus user-owned argument overrides.
//!
//! Scanned folder names are immutable identities. Accounts and launch schemes
//! keep their historical argument strings as compatibility mirrors, while all
//! editing is centralized in this versioned sidecar-backed catalog.

use crate::audio_mod::{active_mod_name, arguments_with_audio_mod, installed_mods, InstalledMod};
use crate::commands::account::{update_account_mods_inner, AccountManager, AccountMeta};
use crate::commands::global_config::mutate_loaded_global_config;
use crate::commands::launch::parse_windows_command_line;
use crate::domain::account::GameRegion;
use crate::domain::config::GlobalConfig;
use crate::infrastructure::module_config::ModuleConfigStore;
use crate::state::SharedState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
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
    pub feature_groups: Vec<String>,
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

fn capsule_feature_metadata(
    scanned: &[ScannedMod],
    edition: &str,
    arguments: &str,
) -> (Vec<String>, bool, bool, bool) {
    let active_name = active_mod_name(arguments).ok().flatten();
    let related = active_name.as_deref().and_then(|name| {
        scanned.iter().find(|entry| {
            entry.edition == edition && entry.installed.name.eq_ignore_ascii_case(name)
        })
    });
    related.map_or_else(
        || (Vec::new(), false, false, false),
        |entry| {
            let processed =
                !entry.installed.feature_groups.is_empty() || entry.installed.update_required;
            (
                entry.installed.feature_groups.clone(),
                processed,
                entry.installed.update_required,
                entry.installed.source_eligible,
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
                feature_groups: entry.installed.feature_groups.clone(),
                processed,
                source_eligible: entry.installed.source_eligible,
                update_required: entry.installed.update_required,
                ready: true,
                deletable: false,
                assigned_account_ids: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    capsules.extend(payload.custom_entries.iter().map(|entry| {
        let (feature_groups, processed, update_required, source_eligible) =
            capsule_feature_metadata(scanned, &entry.edition, &entry.launch_arguments);
        ModCapsule {
            id: entry.id.clone(),
            edition: entry.edition.clone(),
            name: custom_display_name(&entry.launch_arguments),
            origin: "custom".to_string(),
            launch_arguments: entry.launch_arguments.clone(),
            default_launch_arguments: None,
            feature_groups,
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

fn scan_pool_locked(state: &SharedState) -> Result<ModCapsulePool, String> {
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let scanned = scan_installations(&config);
    let (generation, payload) = load_payload(state, &config, &scanned)?;
    Ok(build_pool(&config, generation, &payload, &scanned))
}

pub(crate) fn refresh_on_startup(state: SharedState) {
    std::thread::spawn(move || {
        let _catalog = CATALOG_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Err(error) = scan_pool_locked(&state) {
            crate::logger::log_msg("WARN", "ModCatalog", &format!("启动扫描失败：{error}"));
        }
    });
}

#[tauri::command]
pub async fn get_mod_capsule_pool(
    state: tauri::State<'_, SharedState>,
) -> Result<ModCapsulePool, String> {
    let shared = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let _catalog = CATALOG_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        scan_pool_locked(&shared)
    })
    .await
    .map_err(|error| format!("读取 Mod 共享目录的后台任务异常退出：{error}"))?
}

#[tauri::command]
pub async fn scan_mod_capsule_pool(
    state: tauri::State<'_, SharedState>,
) -> Result<ModCapsulePool, String> {
    get_mod_capsule_pool(state).await
}

#[tauri::command]
pub fn add_mod_capsule(
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
    let (generation, mut payload) = load_payload(state.inner(), &config, &scanned)?;
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

fn replace_catalog_arguments_in_accounts(
    state: &SharedState,
    config: &GlobalConfig,
    old_arguments: &str,
    new_arguments: &str,
) -> Result<(), String> {
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
            update_account_mods_inner(state, account_id, active, list)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn replace_catalog_arguments_in_schemes(
    state: &SharedState,
    app: &tauri::AppHandle,
    old_arguments: &str,
    new_arguments: &str,
) -> Result<(), String> {
    mutate_loaded_global_config(state, app, |config| {
        let mut changed = false;
        for group in &mut config.launch_groups {
            for member in &mut group.members {
                if member
                    .mod_args
                    .as_deref()
                    .is_some_and(|arguments| arguments.trim() == old_arguments)
                {
                    member.mod_args = Some(new_arguments.to_string());
                    changed = true;
                }
            }
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
    let (generation, mut payload) = load_payload(state.inner(), &config, &scanned)?;
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
            payload.argument_overrides.remove(&current.id);
        } else {
            payload
                .argument_overrides
                .insert(current.id.clone(), launch_arguments.clone());
        }
    } else {
        let entry = payload
            .custom_entries
            .iter_mut()
            .find(|entry| entry.id == current.id)
            .ok_or_else(|| "自定义 Mod 参数已不存在".to_string())?;
        entry.launch_arguments = launch_arguments.clone();
    }
    let (generation, payload) = save_payload(state.inner(), generation, payload)?;
    replace_catalog_arguments_in_accounts(
        state.inner(),
        &config,
        current.launch_arguments.trim(),
        &launch_arguments,
    )?;
    replace_catalog_arguments_in_schemes(
        state.inner(),
        &app,
        current.launch_arguments.trim(),
        &launch_arguments,
    )?;
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
    let (generation, mut payload) = load_payload(state.inner(), &config, &scanned)?;
    let index = payload
        .custom_entries
        .iter()
        .position(|entry| entry.id == capsule_id)
        .ok_or_else(|| {
            "扫描自游戏目录的官方预设不能删除；删除文件夹后重新扫描即可移除".to_string()
        })?;
    let usage = capsule_usage(&config, &payload.custom_entries[index].launch_arguments);
    if !usage.is_empty() {
        return Err(format!(
            "该自定义参数仍被以下项目使用：{}",
            usage.join("、")
        ));
    }
    payload.custom_entries.remove(index);
    let (generation, payload) = save_payload(state.inner(), generation, payload)?;
    Ok(build_pool(&config, generation, &payload, &scanned))
}

#[tauri::command]
pub fn assign_mod_capsule_to_account(
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
    let account = AccountManager::load_meta(&config.accounts_dir, &account_id)
        .map_err(|error| error.to_string())?;
    let arguments = if let Some(capsule_id) = capsule_id {
        let scanned = scan_installations(&config);
        let (generation, payload) = load_payload(state.inner(), &config, &scanned)?;
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
                audio_ready: false,
                update_required: false,
                source_eligible: true,
                feature_groups: Vec::new(),
                audio_reusable: false,
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
