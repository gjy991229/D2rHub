//! Shared Mod catalog derived from installed folders plus user-owned argument overrides.
//!
//! Scanned folder names are immutable identities. Accounts and launch schemes
//! keep their historical argument strings as compatibility mirrors, while all
//! editing is centralized in this versioned sidecar-backed catalog.

use crate::audio_mod::{
    ensure_audio_mod_not_in_use, installed_mods, set_auto_exit_on_death_enabled, InstalledMod,
};
use crate::commands::account::{
    update_account_mods_inner, update_account_mods_with_lease_held, AccountManager,
};
use crate::commands::global_config::mutate_loaded_global_config;
use crate::domain::account::AccountMeta;
use crate::domain::config::GlobalConfig;
use crate::domain::mod_arguments::{active_mod_name, arguments_with_audio_mod};
use crate::domain::mod_catalog::{self as rules, *};
pub use crate::domain::mod_catalog::{ModCapsule, ModCapsuleAccountSelection, ModCapsulePool};
use crate::infrastructure::module_config::ModuleConfigStore;
use crate::state::SharedState;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod transactions;
use transactions::*;

fn account_snapshots(config: &GlobalConfig) -> Result<Vec<AccountMeta>, String> {
    AccountManager::list_ids(&config.accounts_dir)
        .into_iter()
        .map(|id| {
            AccountManager::load_meta(&config.accounts_dir, &id)
                .map_err(|error| format!("无法读取账号 {id} 的 Mod 引用：{error}"))
        })
        .collect()
}

fn plan_catalog_argument_replacements(
    config: &GlobalConfig,
    edition: &str,
    old: &str,
    new: &str,
) -> Result<Vec<AccountModReplacement>, String> {
    Ok(rules::plan_catalog_argument_replacements(
        config,
        &account_snapshots(config)?,
        edition,
        old,
        new,
    ))
}

fn plan_catalog_argument_replacements_in_schemes(
    config: &GlobalConfig,
    edition: &str,
    old: &str,
    new: &str,
) -> Result<Vec<SchemeModJournalEntry>, String> {
    Ok(rules::plan_catalog_argument_replacements_in_schemes(
        config,
        &account_snapshots(config)?,
        edition,
        old,
        new,
    ))
}

fn capsule_usage(config: &GlobalConfig, capsule: &ModCapsule) -> Result<Vec<String>, String> {
    let identity = |game: &str| {
        if game.trim().is_empty() {
            return None;
        }
        let canonical = std::fs::canonicalize(Path::new(game.trim()).join("mods")).ok()?;
        crate::launch_context::normalized_path_identity(&canonical)
    };
    let identities = InstallationIdentities {
        cn: identity(&config.cn_game_path),
        global: identity(&config.global_game_path),
    };
    Ok(rules::capsule_usage(
        config,
        capsule,
        &account_snapshots(config)?,
        &identities,
    ))
}

const MODULE_ID: &str = "mod-catalog";
const SCHEMA_VERSION: u32 = 1;
static CATALOG_LOCK: Mutex<()> = Mutex::new(());
static RESTART_RESERVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct CatalogRestartReservation;

impl Drop for CatalogRestartReservation {
    fn drop(&mut self) {
        RESTART_RESERVED.store(false, std::sync::atomic::Ordering::Release);
    }
}

fn lock_catalog() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    let guard = CATALOG_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if RESTART_RESERVED.load(std::sync::atomic::Ordering::Acquire) {
        return Err("模式切换准备中，请稍候".to_string());
    }
    Ok(guard)
}

pub(crate) fn freeze_for_restart() -> Result<impl Sized, String> {
    let _guard = CATALOG_LOCK
        .try_lock()
        .map_err(|_| "Mod 扫描或写入进行中，请完成后再切换模式".to_string())?;
    if RESTART_RESERVED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return Err("模式切换已经开始".to_string());
    }
    Ok(CatalogRestartReservation)
}

#[derive(Debug, Clone)]
struct ScannedMod {
    id: String,
    edition: String,
    installed: InstalledMod,
    default_arguments: String,
    lightweight_profile: Option<String>,
    issue: Option<String>,
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
            let light = if !installed.requires_unpack
                && installed.feature_groups.is_empty()
                && !installed.update_required
            {
                crate::lightweight_mod::inspect(
                    &Path::new(game_directory).join("mods").join(&installed.name),
                    &installed.name,
                )
            } else if !installed.requires_unpack {
                Ok(crate::hub_mod_settings::identity(
                    &Path::new(game_directory).join("mods"),
                    &installed.name,
                )
                .ok()
                .flatten()
                .and_then(|origin| {
                    arguments_with_audio_mod("", &installed.name)
                        .ok()
                        .map(|arguments| crate::lightweight_mod::Metadata {
                            profile: origin.profile,
                            arguments,
                        })
                }))
            } else {
                Ok(None)
            };
            let (lightweight_profile, issue, default_arguments) = match light {
                Ok(Some(info)) => (Some(info.profile), None, info.arguments),
                other => {
                    let Ok(args) = arguments_with_audio_mod("", &installed.name) else {
                        continue;
                    };
                    (None, other.err(), args)
                }
            };
            scanned.push(ScannedMod {
                id: scanned_capsule_id(edition, &installed.name),
                edition: edition.to_string(),
                installed,
                default_arguments,
                lightweight_profile,
                issue,
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

fn effective_scanned_arguments(payload: &ModCatalogPayload, scanned: &ScannedMod) -> String {
    payload
        .argument_overrides
        .get(&scanned.id)
        .cloned()
        .unwrap_or_else(|| scanned.default_arguments.clone())
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

/// Required core recovery, deliberately independent of installation scanning
/// and legacy catalog initialization. A missing sidecar stays missing.
pub(crate) fn recover_before_launch(
    state: &SharedState,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    if state
        .core_recovery_complete
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Ok(());
    }
    let _catalog = lock_catalog()?;
    if state
        .core_recovery_complete
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Ok(());
    }
    if let Some(envelope) = catalog_store(state)?
        .load::<ModCatalogPayload>()
        .map_err(|error| error.to_string())?
    {
        if envelope.payload.pending_argument_update.is_some() {
            let config = state.configuration().snapshot().ok_or("全局配置尚未加载")?;
            recover_argument_update(state, app, &config, envelope.generation, envelope.payload)?;
        }
    }
    state
        .core_recovery_complete
        .store(true, std::sync::atomic::Ordering::Release);
    Ok(())
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
                lightweight_profile: entry.lightweight_profile.clone(),
                issue: if entry.installed.unpack_recovery_required {
                    Some("上次 MPQ 转换未完成，请点击解压恢复".to_string())
                } else {
                    entry.issue.clone()
                },
                requires_unpack: entry.installed.requires_unpack,
                unpack_recovery_required: entry.installed.unpack_recovery_required,
                feature_groups: entry.installed.feature_groups.clone(),
                auto_exit_on_death_enabled: entry.installed.auto_exit_on_death_enabled,
                processed,
                source_eligible: entry.installed.source_eligible && entry.issue.is_none(),
                update_required: entry.installed.update_required,
                ready: entry.issue.is_none() && !entry.installed.unpack_recovery_required,
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
        let linked = active_mod_name(&entry.launch_arguments)
            .ok()
            .flatten()
            .and_then(|name| {
                scanned.iter().find(|m| {
                    m.edition == entry.edition && m.installed.name.eq_ignore_ascii_case(&name)
                })
            });
        ModCapsule {
            requires_unpack: linked.is_some_and(|m| m.installed.requires_unpack),
            unpack_recovery_required: linked.is_some_and(|m| m.installed.unpack_recovery_required),
            id: entry.id.clone(),
            edition: entry.edition.clone(),
            name: custom_display_name(&entry.launch_arguments),
            origin: "custom".to_string(),
            launch_arguments: entry.launch_arguments.clone(),
            default_launch_arguments: None,
            source_mod_name,
            lightweight_profile: None,
            issue: None,
            feature_groups,
            auto_exit_on_death_enabled,
            processed,
            source_eligible,
            update_required,
            ready: !linked.is_some_and(|m| m.installed.unpack_recovery_required),
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

#[tauri::command]
pub async fn get_mod_capsule_pool(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<ModCapsulePool, String> {
    let shared = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let _catalog = lock_catalog()?;
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
pub fn set_mod_auto_exit_on_death_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    capsule_id: String,
    enabled: bool,
) -> Result<ModCapsulePool, String> {
    let _mutation = state.mod_mutations().try_acquire()?;
    let _catalog = lock_catalog()?;
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
    let game_directory = if target.edition == "CN" {
        config.cn_game_path.trim()
    } else {
        config.global_game_path.trim()
    };
    ensure_audio_mod_not_in_use(
        state.inner(),
        &config,
        Path::new(game_directory),
        &target.installed.name,
    )?;
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
    let _catalog = lock_catalog()?;
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

#[tauri::command]
pub fn update_mod_capsule(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    capsule_id: String,
    launch_arguments: String,
) -> Result<ModCapsulePool, String> {
    let _catalog = lock_catalog()?;
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
    let _account_catalog_lease = state
        .multi_instance()
        .catalog_leases()
        .acquire()
        .map_err(|error| error.to_string())?;
    let account_changes = plan_catalog_argument_replacements(
        &config,
        &current.edition,
        current.launch_arguments.trim(),
        &launch_arguments,
    )?;
    let scheme_changes = plan_catalog_argument_replacements_in_schemes(
        &config,
        &current.edition,
        current.launch_arguments.trim(),
        &launch_arguments,
    )?;
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
    state
        .core_recovery_complete
        .store(false, std::sync::atomic::Ordering::Release);
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

#[tauri::command]
pub fn delete_mod_capsule(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    capsule_id: String,
) -> Result<ModCapsulePool, String> {
    let _mutation = state.mod_mutations().try_acquire()?;
    let _catalog = lock_catalog()?;
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
    let usage = capsule_usage(&config, &current)?;
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
    if scanned_mod.installed.unpack_recovery_required {
        return Err("此 Mod 有未完成的解压事务，请先点击解压恢复，再删除".to_string());
    }
    let game_directory = if scanned_mod.edition == "CN" {
        config.cn_game_path.trim()
    } else {
        config.global_game_path.trim()
    };
    ensure_audio_mod_not_in_use(
        state.inner(),
        &config,
        Path::new(game_directory),
        &scanned_mod.installed.name,
    )?;
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
    let _catalog = lock_catalog()?;
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

/// Resolve only scanned presets. The mutation lease is owned by the caller.
pub(crate) fn resolve_unpack_target(
    state: &SharedState,
    capsule_id: &str,
) -> Result<(GlobalConfig, PathBuf, String), String> {
    let _catalog = lock_catalog()?;
    let config = state.configuration().snapshot().ok_or("尚未完成首次配置")?;
    let scanned = scan_installations(&config);
    let target = scanned
        .iter()
        .find(|entry| entry.id == capsule_id)
        .ok_or("只能解压游戏目录中扫描到的 Mod，请重新扫描")?;
    let game = if target.edition == "CN" {
        &config.cn_game_path
    } else {
        &config.global_game_path
    };
    let root = Path::new(game.trim())
        .join("mods")
        .join(&target.installed.name);
    let name = target.installed.name.clone();
    Ok((config, root, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::{LaunchGroup, LaunchGroupMember};

    struct CatalogFixture {
        root: PathBuf,
        config: GlobalConfig,
    }

    impl CatalogFixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("hub-catalog-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir_all(root.join("cn-game/mods")).unwrap();
            std::fs::create_dir_all(root.join("global-game/mods")).unwrap();
            Self {
                config: GlobalConfig {
                    accounts_dir: root.to_string_lossy().into_owned(),
                    cn_game_path: root.join("cn-game").to_string_lossy().into_owned(),
                    global_game_path: root.join("global-game").to_string_lossy().into_owned(),
                    ..GlobalConfig::default()
                },
                root,
            }
        }

        fn account(&self, id: &str, region: Option<&str>, arguments: &str) {
            let mut account = AccountMeta::new(id);
            account.region = region.map(str::to_owned);
            account.mod_args = arguments.to_string();
            account.initialized = true;
            std::fs::create_dir(self.root.join(id)).unwrap();
            AccountManager::save_meta(&self.config.accounts_dir, &account).unwrap();
        }

        fn scheme(&mut self, members: &[(&str, &str)]) {
            self.config.launch_groups.push(LaunchGroup {
                revision: 0,
                environment: None,
                id: "scheme".into(),
                name: "Mixed editions".into(),
                account_ids: members.iter().map(|(id, _)| (*id).into()).collect(),
                members: members
                    .iter()
                    .map(|(id, arguments)| LaunchGroupMember {
                        account_id: (*id).into(),
                        mod_args: Some((*arguments).into()),
                        ..LaunchGroupMember::default()
                    })
                    .collect(),
            });
        }
    }

    impl Drop for CatalogFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn sample_capsule(edition: &str, origin: &str) -> ModCapsule {
        ModCapsule {
            requires_unpack: false,
            unpack_recovery_required: false,
            id: scanned_capsule_id(edition, "Sample"),
            edition: edition.into(),
            name: "Sample".into(),
            origin: origin.into(),
            launch_arguments: "-mod Sample -txt -assettestmode 1".into(),
            default_launch_arguments: None,
            source_mod_name: None,
            lightweight_profile: None,
            issue: None,
            feature_groups: Vec::new(),
            auto_exit_on_death_enabled: false,
            processed: false,
            source_eligible: true,
            update_required: false,
            ready: true,
            deletable: true,
            assigned_account_ids: Vec::new(),
        }
    }

    #[test]
    fn argument_edits_only_update_accounts_and_scheme_members_in_the_selected_edition() {
        let mut fixture = CatalogFixture::new();
        let old = "-mod Sample -txt";
        let new = "-mod Sample -txt -assettestmode 1";
        fixture.account("acount1", Some("CN"), old);
        fixture.account("acount2", Some("KR"), old);
        fixture.account("acount3", Some("EU"), old);
        fixture.account("acount4", None, old);
        fixture.scheme(&[
            ("acount1", old),
            ("acount2", old),
            ("acount3", old),
            ("acount4", old),
        ]);

        let accounts = plan_catalog_argument_replacements(&fixture.config, "CN", old, new).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].original.id, "acount1");
        assert_eq!(accounts[0].active, new);
        let schemes =
            plan_catalog_argument_replacements_in_schemes(&fixture.config, "CN", old, new).unwrap();
        assert_eq!(schemes.len(), 1);
        assert_eq!(schemes[0].account_id, "acount1");

        let accounts =
            plan_catalog_argument_replacements(&fixture.config, "Global", old, new).unwrap();
        assert_eq!(
            accounts
                .iter()
                .map(|change| change.original.id.as_str())
                .collect::<Vec<_>>(),
            vec!["acount2", "acount3"]
        );
        let schemes =
            plan_catalog_argument_replacements_in_schemes(&fixture.config, "Global", old, new)
                .unwrap();
        assert_eq!(
            schemes
                .iter()
                .map(|change| change.account_id.as_str())
                .collect::<Vec<_>>(),
            vec!["acount2", "acount3"]
        );
    }

    #[test]
    fn scanned_deletion_protects_same_folder_with_different_flags_and_casing() {
        let mut fixture = CatalogFixture::new();
        fixture.account("acount1", Some("CN"), "-mod sample -txt");
        fixture.account("acount2", Some("CN"), "");
        fixture.account("acount3", Some("KR"), "-mod Sample -txt -assettestmode 1");
        fixture.scheme(&[("acount2", "-mod SAMPLE -custom-flag")]);
        let usage = capsule_usage(&fixture.config, &sample_capsule("CN", "scanned")).unwrap();
        assert_eq!(usage, vec!["acount1", "启动方案：Mixed editions"]);
    }

    #[test]
    fn other_edition_does_not_block_deleting_an_unused_mod() {
        let mut fixture = CatalogFixture::new();
        fixture.account("acount1", Some("KR"), "-mod Sample -txt -assettestmode 1");
        fixture.scheme(&[("acount1", "-mod Sample -txt -assettestmode 1")]);
        assert!(
            capsule_usage(&fixture.config, &sample_capsule("CN", "scanned"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn deletion_protects_shared_installation_across_edition_labels() {
        let mut fixture = CatalogFixture::new();
        fixture.config.global_game_path = fixture.config.cn_game_path.clone();
        fixture.account("acount1", Some("KR"), "-mod Sample -different-flags");
        fixture.account("acount2", Some("KR"), "");
        fixture.scheme(&[("ACOUNT2", "-mod sample -txt")]);
        assert_eq!(
            capsule_usage(&fixture.config, &sample_capsule("CN", "scanned")).unwrap(),
            vec!["acount1", "启动方案：Mixed editions"]
        );
    }

    #[test]
    fn deletion_resolves_installation_aliases_and_keeps_uncertain_roots_safe() {
        let mut fixture = CatalogFixture::new();
        fixture.config.global_game_path = fixture
            .root
            .join("global-game/../cn-game")
            .to_string_lossy()
            .into_owned();
        fixture.account("acount1", Some("KR"), "-mod Sample -txt");
        let capsule = sample_capsule("CN", "scanned");
        assert_eq!(
            capsule_usage(&fixture.config, &capsule).unwrap(),
            vec!["acount1"]
        );
        fixture.config.global_game_path = fixture
            .root
            .join("unavailable")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            capsule_usage(&fixture.config, &capsule).unwrap(),
            vec!["acount1"]
        );
    }

    #[cfg(windows)]
    #[test]
    fn deletion_resolves_windows_case_and_separator_aliases() {
        let mut fixture = CatalogFixture::new();
        fixture.config.global_game_path = fixture
            .config
            .cn_game_path
            .to_ascii_uppercase()
            .replace('\\', "/");
        fixture.account("acount1", Some("KR"), "-mod Sample -txt");
        assert_eq!(
            capsule_usage(&fixture.config, &sample_capsule("CN", "scanned")).unwrap(),
            vec!["acount1"]
        );
    }

    #[test]
    fn argument_edits_match_windows_account_id_aliases_in_schemes() {
        let mut fixture = CatalogFixture::new();
        fixture.account("acount1", Some("CN"), "");
        fixture.scheme(&[("ACOUNT1", "-mod Sample -txt")]);
        let changes = plan_catalog_argument_replacements_in_schemes(
            &fixture.config,
            "CN",
            "-mod Sample -txt",
            "-mod Sample -txt -assettestmode 1",
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].account_id, "ACOUNT1");
    }

    #[test]
    fn unresolved_legacy_account_and_unreadable_metadata_prevent_unsafe_deletion() {
        let fixture = CatalogFixture::new();
        fixture.account("acount1", None, "-mod Sample -txt");
        assert_eq!(
            capsule_usage(&fixture.config, &sample_capsule("CN", "scanned")).unwrap(),
            vec!["acount1"]
        );
        std::fs::write(fixture.root.join("acount1/account.json"), "invalid json").unwrap();
        assert!(capsule_usage(&fixture.config, &sample_capsule("CN", "scanned")).is_err());
    }

    #[test]
    fn deleting_custom_parameters_only_checks_the_exact_preset() {
        let fixture = CatalogFixture::new();
        fixture.account("acount1", Some("CN"), "-mod Sample -different-flags");
        assert!(
            capsule_usage(&fixture.config, &sample_capsule("CN", "custom"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn legacy_arguments_become_custom_only_when_they_do_not_match_a_scan_preset() {
        let scanned = ScannedMod {
            id: scanned_capsule_id("CN", "Sample"),
            edition: "CN".to_string(),
            installed: InstalledMod {
                requires_unpack: false,
                unpack_recovery_required: false,
                name: "Sample".to_string(),
                source_mod_name: None,
                audio_ready: false,
                update_required: false,
                source_eligible: true,
                feature_groups: Vec::new(),
                audio_reusable: false,
                auto_exit_on_death_enabled: false,
                room_tools_default_hell: true,
                scene_audio_enabled: true,
            },
            default_arguments: "-mod Sample -txt -assettestmode 1".to_string(),
            lightweight_profile: None,
            issue: None,
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
}
