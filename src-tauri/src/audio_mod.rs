//! Account-aware Mod preparation, upgrade and runtime use cases.
//!
//! Public command signatures and serialized payloads stay here. Filesystem trust,
//! generator IPC and replacement recovery have independent adapter boundaries.
mod filesystem;
mod generator;
mod modification;
mod rebuild;
mod replacement;
mod room_tool_options;
mod source_replace;
mod validation;

use crate::application::task_runtime::{TaskHandle, TaskRequest};
use crate::commands::account::{update_account_mods_inner, AccountManager, AccountMeta};
use crate::domain::config::GlobalConfig;
use crate::domain::mod_arguments::{self as arguments, generated_audio_mod_name, has_txt_argument};
pub use crate::domain::mod_processing::GeneratorFeatureGroup;
use crate::domain::mod_processing::{
    validate_preserved_feature_groups, RequestedFeatureGroups, IN_GAME_ROOM_TOOLS_FEATURE_ID,
};
use crate::launch_context::{ContextPurpose, LaunchContext};
use crate::rune_audio::protocol::PROTOCOL_VERSION;
use crate::state::SharedState;
use filesystem::{find_existing_mod_name, TemporaryDirectory};
use generator::{emit_prepare_progress, run_audio_mod_generator, GeneratorInvocation};
use replacement::replace_audio_mod_directory;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Emitter;
use validation::{
    compatibility, credential_compatibility, resolve_source_directory, validate_audio_mod,
    validate_audio_mod_credential, validate_generator_output, ValidatedAudioMod,
    REQUIRED_AUDIO_MOD_RECIPE_VERSION,
};

#[derive(Debug, Clone, Serialize)]
pub struct InstalledMod {
    pub name: String,
    pub source_mod_name: Option<String>,
    pub audio_ready: bool,
    pub update_required: bool,
    pub source_eligible: bool,
    pub requires_unpack: bool,
    pub unpack_recovery_required: bool,
    pub feature_groups: Vec<String>,
    pub audio_reusable: bool,
    pub auto_exit_on_death_enabled: bool,
    pub room_tools_default_hell: bool,
    pub scene_audio_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModSetupState {
    pub account_id: String,
    pub account_name: String,
    pub current_mod_name: Option<String>,
    pub launch_arguments: String,
    pub has_txt: bool,
    pub ready: bool,
    pub update_required: bool,
    pub recipe_version: Option<u32>,
    pub required_recipe_version: u32,
    pub build_mode: Option<String>,
    pub source_mod_name: Option<String>,
    pub feature_groups: Vec<String>,
    pub auto_exit_on_death_enabled: bool,
    pub reason_code: String,
    pub message: String,
    pub installed_mods: Vec<InstalledMod>,
    pub running_pid: Option<u32>,
    pub session_verified: bool,
    pub active_session_ready: Option<bool>,
    pub active_session_update_required: Option<bool>,
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModPrepareProgress {
    pub account_id: String,
    pub phase: String,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModPrepareResult {
    pub account_id: String,
    pub mod_name: String,
    pub mod_directory: String,
    pub launch_arguments: String,
    pub source_mod_name: Option<String>,
    pub feature_groups: Vec<GeneratorFeatureGroup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModRuntimeWarning {
    pub account_id: String,
    pub account_name: String,
    pub target_pid: u32,
    pub reason_code: String,
    pub message: String,
}

// Compatibility entry points used by the catalog and runtime commands.
pub(crate) fn active_mod_name(arguments: &str) -> Result<Option<String>, String> {
    arguments::active_mod_name(arguments)
}

pub(crate) fn arguments_with_audio_mod(arguments: &str, mod_name: &str) -> Result<String, String> {
    arguments::arguments_with_audio_mod(arguments, mod_name)
}

pub(crate) fn installed_mods(mods_directory: &Path) -> Vec<InstalledMod> {
    validation::installed_mods(mods_directory)
}

pub(crate) fn settings_source(mods: &Path, name: &str) -> Result<Option<String>, String> {
    let root = mods.join(name);
    let Some(path) = ["d2rhub-mod-manifest.json", "audio-telemetry-manifest.json"]
        .iter()
        .map(|file| root.join(file))
        .find(|path| path.exists())
    else {
        return Ok(None);
    };
    // Source inheritance does not depend on whether an installed feature recipe
    // needs an update. Keep those feature permissions in the existing validators.
    let manifest: serde_json::Value =
        serde_json::from_slice(&crate::hub_mod_settings::read(&path)?)
            .map_err(|e| e.to_string())?;
    if manifest["manifest_format"] != "d2r-audio-telemetry-mod"
        || manifest["producer"] != "d2r-audio-mod"
        || manifest["mod_name"] != name
        || manifest["protocol_version"] != PROTOCOL_VERSION
    {
        return Err("无法验证加工 Mod 的来源凭证".into());
    }
    if manifest["build_mode"] != "augment" || manifest["source_mod_copied"] != true {
        return Ok(None);
    }
    let source = manifest["source_mod_name"]
        .as_str()
        .ok_or("加工凭证缺少来源 Mod")?;
    Ok(Some(arguments::plain_mod_name(source)?.to_string()))
}

pub(crate) fn set_auto_exit_on_death_enabled(
    mods_directory: &Path,
    mod_name: &str,
    enabled: bool,
) -> Result<bool, String> {
    modification::set_auto_exit_on_death_enabled(mods_directory, mod_name, enabled)
}

pub(crate) fn recover_audio_mod_replacements(mods_directory: &Path) -> Result<(), String> {
    source_replace::recover(mods_directory)?;
    replacement::recover_audio_mod_replacements(mods_directory)
}

fn configured_account(
    state: &SharedState,
    account_id: &str,
) -> Result<(GlobalConfig, AccountMeta, LaunchContext), String> {
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let account = AccountManager::load_meta(&config.accounts_dir, account_id)
        .map_err(|error| error.to_string())?;

    if !account.initialized {
        return Err("请先初始化该账号".to_string());
    }
    let context = LaunchContext::for_account(&config, &account, ContextPurpose::LaunchGame)
        .map_err(|error| error.to_string())?;
    Ok((config, account, context))
}

fn mod_task_subject(account_id: &str, edition: Option<&str>) -> String {
    edition
        .filter(|_| account_id.is_empty())
        .map(|edition| format!("installation:{edition}"))
        .unwrap_or_else(|| account_id.to_string())
}

/// Existing account commands keep their original validation; installation-only
/// operations resolve neither authentication nor Saved Games/account metadata.
fn mod_directory_for_subject(
    state: &SharedState,
    account_id: &str,
    edition: Option<&str>,
) -> Result<(GlobalConfig, PathBuf, String), String> {
    if !account_id.is_empty() {
        if edition.is_some() {
            return Err("请仅指定账号或游戏安装版本中的一种加工目标".into());
        }
        let (config, account, context) = configured_account(state, account_id)?;
        return Ok((
            config,
            context.installation.game_directory,
            account.mod_args,
        ));
    }
    let config = state.configuration().snapshot().ok_or("尚未完成首次配置")?;
    let edition = edition.ok_or("请选择加工目标账号或游戏安装版本")?;
    let directory = crate::rune_audio::external::installation_directory(&config, edition)?;
    let arguments = config
        .rune_audio_external_target
        .as_ref()
        .filter(|target| target.edition == edition)
        .filter(|target| !target.mod_name.is_empty())
        .map(|target| arguments_with_audio_mod("", &target.mod_name))
        .transpose()?
        .unwrap_or_default();
    Ok((config, directory, arguments))
}

fn installation_setup_state(
    state: &SharedState,
    edition: &str,
    mod_name: Option<&str>,
) -> Result<AudioModSetupState, String> {
    let (config, directory, configured_arguments) =
        mod_directory_for_subject(state, "", Some(edition))?;
    let mods = directory.join("mods");
    let arguments = mod_name
        .filter(|name| !name.is_empty())
        .map(|name| arguments_with_audio_mod("", name))
        .transpose()?
        .unwrap_or(configured_arguments);
    let configured = credential_compatibility(&mods, &arguments);
    let validated = configured
        .mod_name
        .as_deref()
        .and_then(|name| validate_audio_mod_credential(&mods, name).ok());
    Ok(AudioModSetupState {
        account_id: String::new(),
        account_name: crate::rune_audio::external::source_name(&config, edition),
        current_mod_name: configured.mod_name,
        launch_arguments: arguments,
        has_txt: configured.has_txt,
        ready: configured.ready,
        update_required: configured.update_required,
        recipe_version: configured.recipe_version,
        required_recipe_version: REQUIRED_AUDIO_MOD_RECIPE_VERSION,
        build_mode: configured.build_mode,
        source_mod_name: configured.source_mod_name,
        feature_groups: validated
            .as_ref()
            .map(|mod_| {
                mod_.feature_groups
                    .iter()
                    .map(|group| group.id.clone())
                    .collect()
            })
            .unwrap_or_default(),
        auto_exit_on_death_enabled: validated.is_some_and(|mod_| mod_.auto_exit_on_death_enabled),
        reason_code: configured.reason_code,
        message: configured.message,
        installed_mods: installed_mods(&mods),
        running_pid: None,
        session_verified: false,
        active_session_ready: None,
        active_session_update_required: None,
        restart_required: false,
    })
}

fn session_arguments(state: &SharedState, account: &AccountMeta) -> (String, Option<u32>, bool) {
    if let Some(instance) = state.multi_instance().instances().get(&account.id) {
        if let Some(snapshot) = instance.launch {
            return (snapshot.mod_args, Some(instance.pid), true);
        }
        return (account.mod_args.clone(), Some(instance.pid), false);
    }
    if let Some(pid) = account.running_pid {
        return (account.mod_args.clone(), Some(pid), false);
    }
    (account.mod_args.clone(), None, false)
}

#[allow(dead_code)] // retained for strict runtime consumers; automation has a title fallback
fn require_verified_running_session(
    account_name: &str,
    session: (String, Option<u32>, bool),
) -> Result<(String, u32), String> {
    let (arguments, running_pid, session_verified) = session;
    let pid = running_pid.ok_or_else(|| {
        format!("账号“{account_name}”当前没有由 D2RHub 确认的运行实例；请先通过 D2RHub 启动该账号")
    })?;
    if !session_verified {
        return Err(format!(
            "账号“{account_name}”检测到运行进程（PID {pid}），但缺少与该进程匹配的可信启动快照；请关闭游戏后通过 D2RHub 重新启动该账号"
        ));
    }
    Ok((arguments, pid))
}

#[allow(dead_code)] // retained for callers that require a trusted launch snapshot
pub(crate) fn validate_in_game_room_tools_for_account(
    state: &SharedState,
    account_id: &str,
) -> Result<(), String> {
    let config = state.configuration().snapshot().ok_or("尚未完成首次配置")?;
    let account = AccountManager::load_meta(&config.accounts_dir, account_id)
        .map_err(|error| error.to_string())?;
    let account_name = if account.display_name.trim().is_empty() {
        account.id.as_str()
    } else {
        account.display_name.as_str()
    };
    // Only a snapshot whose PID matches the active registry entry is authoritative. Discovered
    // processes and AccountMeta.running_pid intentionally fail closed: persisted mod_args may have
    // changed after that game process started.
    let (launch_arguments, running_pid) =
        require_verified_running_session(account_name, session_arguments(state, &account))?;
    let directory = crate::infrastructure::system::running_game_directory(running_pid)?;
    validate_room_tools_in_directory(account_name, &directory, &launch_arguments)
}

fn validate_room_tool_capability(
    account_name: &str,
    validated: &ValidatedAudioMod,
) -> Result<(), String> {
    let room_group = validated
        .feature_groups
        .iter()
        .find(|group| group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID);
    if !validated.current_feature_protocol
        || validated
            .recipe_version
            .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION)
        || room_group.is_none()
    {
        return Err(format!(
            "账号“{account_name}”的识别 Mod 不含受支持的局内房间工具，请重新加工并重启该账号"
        ));
    }
    Ok(())
}

fn validate_room_tools_in_directory(
    account_name: &str,
    directory: &Path,
    launch_arguments: &str,
) -> Result<(), String> {
    let mod_name = active_mod_name(launch_arguments)?
        .ok_or_else(|| format!("账号“{account_name}”没有启用经过 D2RHub 加工的 Mod"))?;
    if !has_txt_argument(launch_arguments)? {
        return Err(format!("账号“{account_name}”的 Mod 启动参数缺少 -txt"));
    }
    let mods_directory = directory.join("mods");
    let installed_name = find_existing_mod_name(&mods_directory, &mod_name)?
        .ok_or_else(|| format!("账号“{account_name}”配置的 Mod 不存在"))?;
    let validated = validate_audio_mod(&mods_directory, &installed_name)
        .map_err(|error| format!("账号“{account_name}”：{error}"))?;
    validate_room_tool_capability(account_name, &validated)?;
    // `validate_audio_mod` already checks the current recipe/fingerprint and every required layout.
    Ok(())
}

fn running_accounts_using_mod(
    state: &SharedState,
    config: &GlobalConfig,
    game_directory: &Path,
    mod_name: &str,
) -> Vec<(String, u32)> {
    let executable = game_directory.join("D2R.exe");
    AccountManager::list_ids(&config.accounts_dir)
        .into_iter()
        .filter_map(|account_id| {
            let account = AccountManager::load_meta(&config.accounts_dir, &account_id).ok()?;
            let (arguments, pid, _) = session_arguments(state, &account);
            let pid = pid?;
            let active_name = active_mod_name(&arguments).ok().flatten()?;
            if !active_name.eq_ignore_ascii_case(mod_name) {
                return None;
            }
            // Unknown legacy installation identity stays conservative; a known
            // different installation cannot hold this directory's Mod in use.
            if crate::launch_context::account_game_executable_identity(config, &account).is_ok_and(
                |account_executable| {
                    !crate::launch_context::paths_have_same_identity(
                        &account_executable,
                        &executable,
                    )
                },
            ) {
                return None;
            }
            Some((
                if account.display_name.trim().is_empty() {
                    account.id
                } else {
                    account.display_name
                },
                pid,
            ))
        })
        .collect()
}

pub(crate) fn ensure_audio_mod_not_in_use(
    state: &SharedState,
    config: &GlobalConfig,
    game_directory: &Path,
    mod_name: &str,
) -> Result<(), String> {
    ensure_audio_mod_not_in_use_with(
        state,
        config,
        game_directory,
        mod_name,
        crate::rune_audio::external::ensure_mod_not_in_use,
    )
}

fn ensure_audio_mod_not_in_use_with(
    state: &SharedState,
    config: &GlobalConfig,
    game_directory: &Path,
    mod_name: &str,
    check_external_use: impl FnOnce(&Path, &str) -> Result<(), String>,
) -> Result<(), String> {
    let running = running_accounts_using_mod(state, config, game_directory, mod_name);
    if running.is_empty() {
        // Filesystem protection follows the mutation, independently of which
        // game or Mod is selected for audio recognition.
        return check_external_use(game_directory, mod_name);
    }
    Err(format!(
        "请先关闭正在使用 Mod“{mod_name}”的游戏：{}",
        running
            .iter()
            .map(|(name, pid)| format!("{name}（PID {pid}）"))
            .collect::<Vec<_>>()
            .join("、")
    ))
}

fn setup_state(state: &SharedState, account_id: &str) -> Result<AudioModSetupState, String> {
    let (_config, account, context) = configured_account(state, account_id)?;
    let mods_directory = context.installation.game_directory.join("mods");
    let configured = credential_compatibility(&mods_directory, &account.mod_args);
    let (session_arguments, running_pid, session_verified) = session_arguments(state, &account);
    let active_session = running_pid.and_then(|pid| {
        if !session_verified {
            return None;
        }
        let directory = crate::infrastructure::system::running_game_directory(pid).ok()?;
        Some(credential_compatibility(
            &directory.join("mods"),
            &session_arguments,
        ))
    });
    let active_session_ready = active_session.as_ref().map(|result| result.ready);
    let active_session_update_required =
        active_session.as_ref().map(|result| result.update_required);
    let restart_required = running_pid.is_some()
        && !configured.update_required
        && (active_session_ready != Some(true) || active_session_update_required != Some(false));
    let configured_mod = configured
        .mod_name
        .as_deref()
        .and_then(|name| validate_audio_mod_credential(&mods_directory, name).ok());
    let auto_exit_on_death_enabled = configured_mod
        .as_ref()
        .is_some_and(|validated| validated.auto_exit_on_death_enabled);
    let feature_groups = configured_mod
        .map(|validated| {
            validated
                .feature_groups
                .into_iter()
                .map(|group| group.id)
                .collect()
        })
        .unwrap_or_default();
    Ok(AudioModSetupState {
        account_id: account.id.clone(),
        account_name: if account.display_name.trim().is_empty() {
            account.id.clone()
        } else {
            account.display_name.clone()
        },
        current_mod_name: configured.mod_name,
        launch_arguments: account.mod_args,
        has_txt: configured.has_txt,
        ready: configured.ready,
        update_required: configured.update_required,
        recipe_version: configured.recipe_version,
        required_recipe_version: REQUIRED_AUDIO_MOD_RECIPE_VERSION,
        build_mode: configured.build_mode,
        source_mod_name: configured.source_mod_name,
        feature_groups,
        auto_exit_on_death_enabled,
        reason_code: configured.reason_code,
        message: configured.message,
        installed_mods: installed_mods(&mods_directory),
        running_pid,
        session_verified,
        active_session_ready,
        active_session_update_required,
        restart_required,
    })
}

#[tauri::command]
pub async fn get_audio_mod_setup_state(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    edition: Option<String>,
    mod_name: Option<String>,
) -> Result<AudioModSetupState, String> {
    let shared = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let _lease = shared.mod_mutations().try_acquire()?;
        if account_id.is_empty() {
            installation_setup_state(
                &shared,
                edition.as_deref().ok_or("请选择游戏安装版本")?,
                mod_name.as_deref(),
            )
        } else {
            if edition.is_some() {
                return Err("账号检查不能同时指定安装版本".into());
            }
            setup_state(&shared, &account_id)
        }
    })
    .await
    .map_err(|error| format!("读取 Mod 加工凭证的后台任务异常退出: {error}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn prepare_audio_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    edition: Option<String>,
    mod_name: String,
    source_mod_name: Option<String>,
    replace_source: Option<bool>,
    include_audio_telemetry: Option<bool>,
    include_scene_audio: Option<bool>,
    include_room_tools: Option<bool>,
    room_tools_default_hell: Option<bool>,
    include_esc_next_game: Option<bool>,
    include_scene_shortcuts: Option<bool>,
    include_party_loot: Option<bool>,
    include_hostile_auto_restart: Option<bool>,
    include_auto_exit_on_death: Option<bool>,
) -> Result<AudioModPrepareResult, String> {
    prepare_audio_mod_task(
        app,
        state,
        AudioModTaskRetryPayload::Prepare {
            account_id,
            edition,
            mod_name,
            source_mod_name,
            replace_source,
            include_audio_telemetry,
            include_scene_audio,
            include_room_tools,
            room_tools_default_hell,
            include_esc_next_game,
            include_scene_shortcuts,
            include_party_loot,
            include_hostile_auto_restart,
            include_auto_exit_on_death,
        },
        None,
    )
    .await
}

async fn prepare_audio_mod_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    payload: AudioModTaskRetryPayload,
    retry_of: Option<u64>,
) -> Result<AudioModPrepareResult, String> {
    let AudioModTaskRetryPayload::Prepare {
        account_id,
        edition,
        mod_name,
        source_mod_name,
        replace_source,
        include_audio_telemetry,
        include_scene_audio,
        include_room_tools,
        room_tools_default_hell,
        include_esc_next_game,
        include_scene_shortcuts,
        include_party_loot,
        include_hostile_auto_restart,
        include_auto_exit_on_death,
    } = payload
    else {
        return Err("任务重试数据与 Mod 准备操作不匹配".to_string());
    };
    let retry_payload = serde_json::to_string(&AudioModTaskRetryPayload::Prepare {
        account_id: account_id.clone(),
        edition: edition.clone(),
        mod_name: mod_name.clone(),
        source_mod_name: source_mod_name.clone(),
        replace_source,
        include_audio_telemetry,
        include_scene_audio,
        include_room_tools,
        room_tools_default_hell,
        include_esc_next_game,
        include_scene_shortcuts,
        include_party_loot,
        include_hostile_auto_restart,
        include_auto_exit_on_death,
    })
    .map_err(|error| format!("创建任务重试数据失败: {error}"))?;
    let mut request = TaskRequest::new("audio-mod-prepare")
        .for_subject(mod_task_subject(&account_id, edition.as_deref()))
        .with_conflict_key("audio-mod-build")
        .with_retry_payload(retry_payload)
        .with_initial_status("preflight", "正在检查 Mod 加工环境");
    if let Some(retry_of) = retry_of {
        request = request.with_retry_of(retry_of);
    }
    let task = state
        .tasks()
        .begin(request)
        .map_err(|error| error.to_string())?;
    let result = prepare_audio_mod_impl(
        app,
        state,
        PrepareAudioModRequest {
            account_id,
            edition,
            mod_name,
            source_mod_name,
            replace_source,
            include_audio_telemetry,
            include_scene_audio,
            include_room_tools,
            room_tools_default_hell,
            include_esc_next_game,
            include_scene_shortcuts,
            include_party_loot,
            include_hostile_auto_restart,
            include_auto_exit_on_death,
        },
        &task,
    )
    .await;
    match &result {
        Ok(_) => {
            let _ = task.succeed("识别 Mod 已准备完成");
        }
        Err(error) if task.cancellation_requested() => {
            let _ = task.cancelled(error);
        }
        Err(error) => {
            let _ = task.fail("audio-mod-prepare-failed", error);
        }
    }
    result
}

struct PrepareAudioModRequest {
    account_id: String,
    edition: Option<String>,
    mod_name: String,
    source_mod_name: Option<String>,
    replace_source: Option<bool>,
    include_audio_telemetry: Option<bool>,
    include_scene_audio: Option<bool>,
    include_room_tools: Option<bool>,
    room_tools_default_hell: Option<bool>,
    include_esc_next_game: Option<bool>,
    include_scene_shortcuts: Option<bool>,
    include_party_loot: Option<bool>,
    include_hostile_auto_restart: Option<bool>,
    include_auto_exit_on_death: Option<bool>,
}

async fn prepare_audio_mod_impl(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    request: PrepareAudioModRequest,
    task: &TaskHandle,
) -> Result<AudioModPrepareResult, String> {
    let PrepareAudioModRequest {
        account_id,
        edition,
        mod_name,
        source_mod_name,
        replace_source,
        include_audio_telemetry,
        include_scene_audio,
        include_room_tools,
        room_tools_default_hell,
        include_esc_next_game,
        include_scene_shortcuts,
        include_party_loot,
        include_hostile_auto_restart,
        include_auto_exit_on_death,
    } = request;
    let shared_state = state.inner().clone();
    let _lease = shared_state.mod_mutations().try_acquire()?;
    let (config, game_directory, _) =
        mod_directory_for_subject(&shared_state, &account_id, edition.as_deref())?;
    let progress_id = mod_task_subject(&account_id, edition.as_deref());
    let mods_directory = game_directory.join("mods");
    let processor = crate::bundled_processor::resolve_processor(&app).await?;
    std::fs::create_dir_all(&mods_directory)
        .map_err(|error| format!("创建 mods 目录失败: {error}"))?;
    recover_audio_mod_replacements(&mods_directory)?;

    let mod_name = generated_audio_mod_name(&mod_name)?.to_string();
    let replace_source = replace_source.unwrap_or(false);
    let transaction = uuid::Uuid::new_v4().simple().to_string();
    let backup_name = source_replace::backup_name(&mod_name, &transaction);
    if replace_source {
        if source_mod_name.as_deref() != Some(mod_name.as_str()) {
            return Err("同名加工必须选择与输出名称相同的源 Mod".into());
        }
        ensure_audio_mod_not_in_use(&shared_state, &config, &game_directory, &mod_name)?;
    } else if let Some(existing_name) = find_existing_mod_name(&mods_directory, &mod_name)? {
        return Err(format!("Mod 名称“{existing_name}”已存在，请换一个名称"));
    }
    let (mut source_mod_name, source_directory) = resolve_source_directory(
        &mods_directory,
        if replace_source {
            &backup_name
        } else {
            &mod_name
        },
        source_mod_name,
    )?;
    if let Some(source) = &source_directory {
        filesystem::validate_safe_directory_tree(&mods_directory, source)?;
    }
    let stage = if replace_source {
        Some(TemporaryDirectory::create(
            mods_directory.join(format!(".d2rhub-source-stage-{transaction}")),
        )?)
    } else {
        None
    };
    let output_directory = stage
        .as_ref()
        .map_or(mods_directory.as_path(), |stage| stage.path());
    let requested_features = RequestedFeatureGroups::from_options(
        include_audio_telemetry,
        include_room_tools,
        include_esc_next_game,
        include_scene_shortcuts,
        include_party_loot,
        include_hostile_auto_restart,
        include_auto_exit_on_death,
    )?;

    emit_prepare_progress(
        &app,
        Some(task),
        &progress_id,
        "starting",
        1,
        "正在开始准备…",
    );
    let report = run_audio_mod_generator(
        &app,
        task,
        GeneratorInvocation {
            processor: &processor,
            account_id: &progress_id,
            game_directory: &game_directory,
            output_directory,
            mod_name: &mod_name,
            source_directory: source_directory.as_deref(),
            requested_features,
            progress_ceiling: 100,
            audio_options: Some(&rebuild::AudioOptions {
                areas: if include_scene_audio == Some(false) {
                    "none"
                } else {
                    "all"
                }
                .into(),
                ..Default::default()
            }),
        },
    )
    .await?;
    let default_hell = room_tools_default_hell.unwrap_or(
        source_directory
            .as_deref()
            .map(room_tool_options::default_hell)
            .transpose()?
            .unwrap_or(true),
    );
    room_tool_options::configure(output_directory, &mod_name, default_hell)?;
    let mut generated = validate_generator_output(
        output_directory,
        &mod_name,
        &report,
        requested_features,
        &[],
    )?;
    crate::hub_mod_settings::inherit(source_directory.as_deref(), &generated.directory, &mod_name)?;
    if replace_source {
        // Retain the original source lineage for processed sources. Raw sources now live in the backup.
        if source_directory.as_ref().is_some_and(|dir| {
            !["d2rhub-mod-manifest.json", "audio-telemetry-manifest.json"]
                .iter()
                .any(|name| dir.join(name).is_file())
        }) {
            for filename in ["d2rhub-mod-manifest.json", "audio-telemetry-manifest.json"] {
                let path = generated.directory.join(filename);
                if path.is_file() {
                    let mut manifest: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    manifest["source_mod_name"] = serde_json::json!(backup_name);
                    std::fs::write(
                        &path,
                        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            source_mod_name = Some(backup_name.clone());
        }
        // Re-check immediately before swapping; cancellation never leaves half a directory move.
        if task.cancellation_requested() {
            return Err("Mod 加工已取消，源目录未修改".into());
        }
        ensure_audio_mod_not_in_use(&shared_state, &config, &game_directory, &mod_name)?;
        source_replace::commit(
            &mods_directory,
            &mod_name,
            &generated.directory,
            &transaction,
        )?;
        generated.directory = mods_directory.join(&mod_name);
    }
    emit_prepare_progress(
        &app,
        None,
        &progress_id,
        "complete",
        100,
        "识别 Mod 已准备完成",
    );
    Ok(AudioModPrepareResult {
        account_id,
        mod_name: report.mod_name,
        mod_directory: generated.directory.to_string_lossy().into_owned(),
        launch_arguments: arguments_with_audio_mod("", &mod_name)?,
        source_mod_name,
        feature_groups: generated.feature_groups,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn upgrade_audio_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    edition: Option<String>,
    mod_name: Option<String>,
    source_mod_name: Option<String>,
    include_audio_telemetry: Option<bool>,
    include_scene_audio: Option<bool>,
    include_room_tools: Option<bool>,
    room_tools_default_hell: Option<bool>,
    include_esc_next_game: Option<bool>,
    include_scene_shortcuts: Option<bool>,
    include_party_loot: Option<bool>,
    include_hostile_auto_restart: Option<bool>,
    include_auto_exit_on_death: Option<bool>,
    force_rebuild: Option<bool>,
) -> Result<AudioModSetupState, String> {
    let force_rebuild = force_rebuild.unwrap_or(false);
    upgrade_audio_mod_task(
        app,
        state,
        AudioModTaskRetryPayload::Upgrade {
            account_id,
            edition,
            mod_name,
            source_mod_name,
            include_audio_telemetry,
            include_scene_audio,
            include_room_tools,
            room_tools_default_hell,
            include_esc_next_game,
            include_scene_shortcuts,
            include_party_loot,
            include_hostile_auto_restart,
            include_auto_exit_on_death,
            force_rebuild,
        },
        None,
    )
    .await
}

async fn upgrade_audio_mod_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    payload: AudioModTaskRetryPayload,
    retry_of: Option<u64>,
) -> Result<AudioModSetupState, String> {
    let AudioModTaskRetryPayload::Upgrade {
        account_id,
        edition,
        mod_name,
        source_mod_name,
        include_audio_telemetry,
        include_scene_audio,
        include_room_tools,
        room_tools_default_hell,
        include_esc_next_game,
        include_scene_shortcuts,
        include_party_loot,
        include_hostile_auto_restart,
        include_auto_exit_on_death,
        force_rebuild,
    } = payload
    else {
        return Err("任务重试数据与 Mod 更新操作不匹配".to_string());
    };
    let retry_payload = serde_json::to_string(&AudioModTaskRetryPayload::Upgrade {
        account_id: account_id.clone(),
        edition: edition.clone(),
        mod_name: mod_name.clone(),
        source_mod_name: source_mod_name.clone(),
        include_audio_telemetry,
        include_scene_audio,
        include_room_tools,
        room_tools_default_hell,
        include_esc_next_game,
        include_scene_shortcuts,
        include_party_loot,
        include_hostile_auto_restart,
        include_auto_exit_on_death,
        force_rebuild,
    })
    .map_err(|error| format!("创建任务重试数据失败: {error}"))?;
    let mut request = TaskRequest::new("audio-mod-upgrade")
        .for_subject(mod_task_subject(&account_id, edition.as_deref()))
        .with_conflict_key("audio-mod-build")
        .with_retry_payload(retry_payload)
        .with_initial_status("preflight", "正在检查 Mod 更新环境");
    if let Some(retry_of) = retry_of {
        request = request.with_retry_of(retry_of);
    }
    let task = state
        .tasks()
        .begin(request)
        .map_err(|error| error.to_string())?;
    let result = upgrade_audio_mod_impl(
        app,
        state,
        UpgradeAudioModRequest {
            account_id,
            edition,
            requested_mod_name: mod_name,
            source_mod_name,
            include_audio_telemetry,
            include_scene_audio,
            include_room_tools,
            room_tools_default_hell,
            include_esc_next_game,
            include_scene_shortcuts,
            include_party_loot,
            include_hostile_auto_restart,
            include_auto_exit_on_death,
            force_rebuild,
        },
        &task,
    )
    .await;
    match &result {
        Ok(_) => {
            let _ = task.succeed("同名识别 Mod 已更新完成");
        }
        Err(error) if task.cancellation_requested() => {
            let _ = task.cancelled(error);
        }
        Err(error) => {
            let _ = task.fail("audio-mod-upgrade-failed", error);
        }
    }
    result
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub(crate) enum AudioModTaskRetryPayload {
    Prepare {
        account_id: String,
        #[serde(default)]
        edition: Option<String>,
        mod_name: String,
        source_mod_name: Option<String>,
        #[serde(default)]
        replace_source: Option<bool>,
        include_audio_telemetry: Option<bool>,
        include_scene_audio: Option<bool>,
        include_room_tools: Option<bool>,
        room_tools_default_hell: Option<bool>,
        include_esc_next_game: Option<bool>,
        include_scene_shortcuts: Option<bool>,
        include_party_loot: Option<bool>,
        include_hostile_auto_restart: Option<bool>,
        include_auto_exit_on_death: Option<bool>,
    },
    Upgrade {
        account_id: String,
        #[serde(default)]
        edition: Option<String>,
        mod_name: Option<String>,
        source_mod_name: Option<String>,
        include_audio_telemetry: Option<bool>,
        include_scene_audio: Option<bool>,
        include_room_tools: Option<bool>,
        room_tools_default_hell: Option<bool>,
        include_esc_next_game: Option<bool>,
        include_scene_shortcuts: Option<bool>,
        include_party_loot: Option<bool>,
        include_hostile_auto_restart: Option<bool>,
        include_auto_exit_on_death: Option<bool>,
        #[serde(default)]
        force_rebuild: bool,
    },
}

pub(crate) async fn retry_audio_mod_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    retry_of: u64,
    payload: AudioModTaskRetryPayload,
) -> Result<(), String> {
    match payload {
        payload @ AudioModTaskRetryPayload::Prepare { .. } => {
            prepare_audio_mod_task(app, state, payload, Some(retry_of))
                .await
                .map(|_| ())
        }
        payload @ AudioModTaskRetryPayload::Upgrade { .. } => {
            upgrade_audio_mod_task(app, state, payload, Some(retry_of))
                .await
                .map(|_| ())
        }
    }
}

struct UpgradeAudioModRequest {
    account_id: String,
    edition: Option<String>,
    requested_mod_name: Option<String>,
    source_mod_name: Option<String>,
    include_audio_telemetry: Option<bool>,
    include_scene_audio: Option<bool>,
    include_room_tools: Option<bool>,
    room_tools_default_hell: Option<bool>,
    include_esc_next_game: Option<bool>,
    include_scene_shortcuts: Option<bool>,
    include_party_loot: Option<bool>,
    include_hostile_auto_restart: Option<bool>,
    include_auto_exit_on_death: Option<bool>,
    force_rebuild: bool,
}

async fn upgrade_audio_mod_impl(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    request: UpgradeAudioModRequest,
    task: &TaskHandle,
) -> Result<AudioModSetupState, String> {
    let UpgradeAudioModRequest {
        account_id,
        edition,
        requested_mod_name,
        source_mod_name,
        include_audio_telemetry,
        include_scene_audio,
        include_room_tools,
        room_tools_default_hell,
        include_esc_next_game,
        include_scene_shortcuts,
        include_party_loot,
        include_hostile_auto_restart,
        include_auto_exit_on_death,
        force_rebuild,
    } = request;
    let shared_state = state.inner().clone();
    let _lease = shared_state.mod_mutations().try_acquire()?;
    let (config, game_directory, configured_arguments) =
        mod_directory_for_subject(&shared_state, &account_id, edition.as_deref())?;
    let progress_id = mod_task_subject(&account_id, edition.as_deref());
    let mods_directory = game_directory.join("mods");
    let processor = crate::bundled_processor::resolve_processor(&app).await?;
    recover_audio_mod_replacements(&mods_directory)?;
    let current = if let Some(requested_mod_name) = requested_mod_name.as_deref() {
        let requested_arguments = arguments_with_audio_mod("", requested_mod_name)?;
        compatibility(&mods_directory, &requested_arguments)
    } else {
        compatibility(&mods_directory, &configured_arguments)
    };
    let mod_name = current
        .mod_name
        .as_deref()
        .ok_or_else(|| "当前账号没有配置识别 Mod".to_string())?;
    // Default to the recorded original, allowing an explicit replacement when it moved.
    let source_mod_name = source_mod_name.or_else(|| current.source_mod_name.clone());
    let mut explicitly_requested = RequestedFeatureGroups::from_options(
        include_audio_telemetry,
        include_room_tools,
        include_esc_next_game,
        include_scene_shortcuts,
        include_party_loot,
        include_hostile_auto_restart,
        include_auto_exit_on_death,
    )?;
    let current_validated = validate_audio_mod(&mods_directory, mod_name).ok();
    let rebuilding = include_scene_audio.is_some()
        || force_rebuild
        || current.update_required
        || current_validated.is_none();
    let old_directory = mods_directory.join(mod_name);
    validation::validate_recoverable_audio_mod_directory(
        &mods_directory,
        mod_name,
        &old_directory,
    )?;
    let manifest_path = ["d2rhub-mod-manifest.json", "audio-telemetry-manifest.json"]
        .into_iter()
        .map(|name| old_directory.join(name))
        .find(|path| path.is_file())
        .ok_or("旧 Mod 缺少加工清单")?;
    let document: serde_json::Value = serde_json::from_slice(
        &std::fs::read(manifest_path).map_err(|e| format!("读取旧 Mod 清单失败：{e}"))?,
    )
    .map_err(|e| e.to_string())?;
    if document["feature_groups"]
        .as_array()
        .is_none_or(|groups| groups.is_empty())
        && current.recipe_version.unwrap_or(0) < 22
    {
        explicitly_requested.audio_telemetry = true;
    }
    let (requested_features, mut audio_options) = if rebuilding {
        rebuild::recipe(&document, explicitly_requested)?
    } else {
        (
            explicitly_requested
                .include_existing_known(&current_validated.as_ref().unwrap().feature_groups),
            rebuild::audio_options(&document)?,
        )
    };
    if let Some(enabled) = include_scene_audio {
        audio_options.areas = if enabled { "all" } else { "none" }.into();
    }
    let required_existing_groups = if rebuilding {
        Vec::new()
    } else {
        current_validated.as_ref().unwrap().feature_groups.clone()
    };
    let default_hell =
        room_tools_default_hell.unwrap_or(room_tool_options::default_hell(&old_directory)?);
    let room_options_changed = requested_features.room_tools
        && default_hell != room_tool_options::default_hell(&old_directory)?;
    if !rebuilding
        && !room_options_changed
        && current_validated
            .as_ref()
            .is_some_and(|v| requested_features.all_present(&v.feature_groups))
    {
        return Err("当前 Mod 已包含所选模块，无需增补".into());
    }
    ensure_audio_mod_not_in_use(&shared_state, &config, &game_directory, mod_name)?;
    let source_directory = if rebuilding {
        if current.build_mode.as_deref() == Some("augment") && source_mod_name.is_none() {
            return Err("重做需要当时未经加工的源 Mod，请重新指定来源".into());
        }
        let source = resolve_source_directory(&mods_directory, mod_name, source_mod_name)?.1;
        if source.as_ref().is_some_and(|dir| {
            ["d2rhub-mod-manifest.json", "audio-telemetry-manifest.json"]
                .iter()
                .any(|name| dir.join(name).exists())
        }) {
            return Err("同名重做必须使用未经加工的原始源 Mod，不能使用旧加工成品".into());
        }
        source
    } else {
        Some(old_directory.clone())
    };
    emit_prepare_progress(
        &app,
        Some(task),
        &progress_id,
        "starting",
        1,
        "正在生成同名新版 Mod…",
    );
    let temporary_output = TemporaryDirectory::create(std::env::temp_dir().join(format!(
        "d2rhub-audio-upgrade-output-{}",
        uuid::Uuid::new_v4()
    )))?;
    let report = run_audio_mod_generator(
        &app,
        task,
        GeneratorInvocation {
            processor: &processor,
            account_id: &progress_id,
            game_directory: &game_directory,
            output_directory: temporary_output.path(),
            mod_name,
            source_directory: source_directory.as_deref(),
            requested_features,
            progress_ceiling: 85,
            audio_options: Some(&audio_options),
        },
    )
    .await?;
    room_tool_options::configure(temporary_output.path(), mod_name, default_hell)?;
    let generated = validate_generator_output(
        temporary_output.path(),
        mod_name,
        &report,
        requested_features,
        &required_existing_groups,
    )?;
    crate::hub_mod_settings::inherit_preferences(&old_directory, &generated.directory, mod_name)?;
    if rebuilding
        && document["feature_groups"]
            .as_array()
            .is_some_and(|groups| groups.iter().any(|g| g["id"] == "auto_exit_on_death"))
    {
        let enabled =
            validation::layouts::auto_exit_on_death_layout_enabled(&old_directory, mod_name)?;
        modification::set_auto_exit_on_death_enabled(temporary_output.path(), mod_name, enabled)?;
    }

    if task.cancellation_requested() {
        return Err("识别 Mod 更新已取消".to_string());
    }
    emit_prepare_progress(
        &app,
        Some(task),
        &progress_id,
        "staging",
        90,
        "正在校验并暂存新版 Mod…",
    );
    let transaction_id = uuid::Uuid::new_v4().simple().to_string();
    let staging_parent = TemporaryDirectory::create(
        mods_directory.join(format!(".d2rhub-upgrade-stage-{transaction_id}")),
    )?;
    let staged_directory = staging_parent.path().join(mod_name);
    crate::commands::utils::copy_dir_recursive(&generated.directory, &staged_directory)
        .map_err(|error| format!("暂存新版 Mod 失败: {error}"))?;
    let staged = validate_audio_mod(staging_parent.path(), mod_name)?;
    if staged
        .recipe_version
        .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION)
        || !staged.current_feature_protocol
    {
        return Err("暂存的 Mod 未通过当前配方校验，旧版未被修改".to_string());
    }
    requested_features.validate_present(&staged.feature_groups, PROTOCOL_VERSION)?;
    validate_preserved_feature_groups(&required_existing_groups, &staged.feature_groups)?;
    if staged.feature_groups != generated.feature_groups {
        return Err("暂存 Mod 的功能组与已验证生成结果不一致，旧版未被修改".to_string());
    }

    if task.cancellation_requested() {
        return Err("识别 Mod 更新已取消".to_string());
    }
    emit_prepare_progress(
        &app,
        Some(task),
        &progress_id,
        "switching",
        96,
        "正在替换同名旧版 Mod…",
    );
    // 生成过程可能持续数分钟；切换前再次检查，避免另一账号中途启动同名 Mod。
    ensure_audio_mod_not_in_use(&shared_state, &config, &game_directory, mod_name)?;
    let backup_directory = mods_directory.join(format!(".d2rhub-upgrade-backup-{transaction_id}"));
    replace_audio_mod_directory(
        &mods_directory,
        mod_name,
        &staged_directory,
        &backup_directory,
        &required_existing_groups,
    )?;
    let installed = validate_audio_mod(&mods_directory, mod_name)?;
    if installed
        .recipe_version
        .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION)
        || !installed.current_feature_protocol
        || installed.feature_groups != generated.feature_groups
    {
        return Err("同名更新完成后校验异常，请重新准备识别 Mod".to_string());
    }
    validate_preserved_feature_groups(&required_existing_groups, &installed.feature_groups)?;
    emit_prepare_progress(
        &app,
        None,
        &progress_id,
        "complete",
        100,
        "同名识别 Mod 已更新完成",
    );
    if let Some(edition) = edition.as_deref().filter(|_| account_id.is_empty()) {
        installation_setup_state(&shared_state, edition, Some(mod_name))
    } else {
        setup_state(&shared_state, &account_id)
    }
}

#[tauri::command]
pub fn apply_audio_mod_to_account(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    mod_name: String,
) -> Result<AudioModSetupState, String> {
    let _lease = state.mod_mutations().try_acquire()?;
    let (_config, account, context) = configured_account(state.inner(), &account_id)?;
    let mods_directory = context.installation.game_directory.join("mods");
    recover_audio_mod_replacements(&mods_directory)?;
    validate_audio_mod(&mods_directory, &mod_name)?;
    let next_arguments = arguments_with_audio_mod(&account.mod_args, &mod_name)?;
    let mut mod_list = account.mod_list.clone();
    if !mod_list.iter().any(|entry| entry == &next_arguments) {
        mod_list.push(next_arguments.clone());
    }
    update_account_mods_inner(state.inner(), account_id.clone(), next_arguments, mod_list)
        .map_err(|error| error.to_string())?;
    setup_state(state.inner(), &account_id)
}

pub(crate) fn validate_runtime_audio_mod_directory(
    game_directory: &Path,
    launch_arguments: &str,
) -> Result<PathBuf, String> {
    let mods_directory = game_directory.join("mods");
    let result = compatibility(&mods_directory, launch_arguments);
    if !result.ready {
        return Err(result.message);
    }
    validate_audio_mod(
        &mods_directory,
        result.mod_name.as_deref().unwrap_or_default(),
    )
    .map(|validated| validated.directory)
}

pub(crate) fn validate_runtime_feature_directory(
    game: &Path,
    arguments: &str,
    feature: &str,
) -> Result<PathBuf, String> {
    use crate::domain::mod_arguments::{active_mod_name, has_txt_argument};
    if !has_txt_argument(arguments)? {
        return Err("目标游戏需要 -txt 启动参数".into());
    }
    let name = active_mod_name(arguments)?.ok_or("目标游戏未加载 Mod")?;
    let validated = validate_audio_mod(&game.join("mods"), &name)?;
    if !validated.feature_groups.iter().any(|g| g.id == feature) {
        let label = match feature {
            "hostile_auto_restart" => "敌对自动重开",
            "esc_next_game" => "双击 Esc 下一局",
            _ => feature,
        };
        return Err(format!("目标 Mod 未安装“{label}”功能"));
    }
    if feature == "hostile_auto_restart"
        && !validated
            .feature_groups
            .iter()
            .any(|g| g.id == "esc_next_game")
    {
        return Err("敌对自动重开需要双击 Esc 下一局".into());
    }
    Ok(validated.directory)
}

pub(crate) fn validate_runtime_feature(
    pid: u32,
    arguments: &str,
    feature: &str,
) -> Result<PathBuf, String> {
    let directory = crate::infrastructure::system::running_game_directory(pid)?;
    validate_runtime_feature_directory(&directory, arguments, feature)
}

pub(crate) fn emit_runtime_compatibility_warning(
    app: &tauri::AppHandle,
    state: &SharedState,
    config: &GlobalConfig,
    account: &AccountMeta,
    pid: u32,
    launch_arguments: &str,
) {
    if !config.optional_module_runtime_allowed(crate::domain::config::OPTIONAL_MODULE_AUTOMATION)
        || !config.rune_audio_enabled
        || config.rune_audio_external_target.is_some()
        || config.rune_audio_target_account != account.id
    {
        return;
    }
    let directory = match crate::infrastructure::system::running_game_directory(pid) {
        Ok(directory) => directory,
        Err(_) => return,
    };
    let result = compatibility(&directory.join("mods"), launch_arguments);
    let account_name = if account.display_name.trim().is_empty() {
        account.id.clone()
    } else {
        account.display_name.clone()
    };
    let warning = if !result.ready {
        Some(AudioModRuntimeWarning {
            account_id: account.id.clone(),
            account_name: account_name.clone(),
            target_pid: pid,
            reason_code: result.reason_code,
            message: format!(
                "“{account_name}”当前使用的 Mod 不支持声纹识别，请检查。游戏可以继续运行，但本次识别与统计不会生效。"
            ),
        })
    } else if result.update_required {
        Some(AudioModRuntimeWarning {
            account_id: account.id.clone(),
            account_name: account_name.clone(),
            target_pid: pid,
            reason_code: result.reason_code,
            message: format!("“{account_name}”：{}。", result.message),
        })
    } else {
        None
    };
    if let Some(warning) = warning {
        let _ = app.emit("audio-mod-compatibility-warning", warning);
    }
    state
        .multi_instance()
        .instances()
        .record_launch_snapshot(&account.id, pid, launch_arguments);
}

#[cfg(test)]
#[path = "audio_mod/lightweight_integration_tests.rs"]
mod lightweight_integration_tests;

#[cfg(test)]
#[path = "audio_mod/tests.rs"]
mod tests;
