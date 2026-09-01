use crate::commands::account::{update_account_mods_inner, AccountManager, AccountMeta};
use crate::commands::launch::parse_windows_command_line;
use crate::domain::config::GlobalConfig;
use crate::launch_context::{ContextPurpose, LaunchContext};
use crate::rune_audio::catalog::AREA_CATALOG_FILE_NAME;
use crate::rune_audio::item_catalog::ITEM_CATALOG_FILE_NAME;
use crate::rune_audio::protocol::PROTOCOL_VERSION;
use crate::state::SharedState;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::Emitter;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

const MANIFEST_FILE_NAME: &str = "audio-telemetry-manifest.json";
const MANIFEST_FORMAT: &str = "d2r-audio-telemetry-mod";
const PRODUCER_NAME: &str = "d2r-audio-mod";
const REQUIRED_AUDIO_MOD_RECIPE_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize)]
pub struct InstalledMod {
    pub name: String,
    pub audio_ready: bool,
    pub update_required: bool,
    pub source_eligible: bool,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModRuntimeWarning {
    pub account_id: String,
    pub account_name: String,
    pub target_pid: u32,
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct GeneratorReport {
    protocol_version: u8,
    recipe_version: u32,
    mod_name: String,
    mod_directory: String,
}

#[derive(Debug)]
struct Compatibility {
    mod_name: Option<String>,
    has_txt: bool,
    ready: bool,
    update_required: bool,
    recipe_version: Option<u32>,
    build_mode: Option<String>,
    source_mod_name: Option<String>,
    reason_code: String,
    message: String,
}

#[derive(Debug)]
struct ValidatedAudioMod {
    directory: PathBuf,
    recipe_version: Option<u32>,
    build_mode: Option<String>,
    source_mod_name: Option<String>,
}

#[derive(Debug)]
struct OfficialUpdateMetadata {
    recipe_version: Option<u32>,
    build_mode: Option<String>,
    source_mod_name: Option<String>,
}

struct BuildLease(SharedState);

impl BuildLease {
    fn acquire(state: &SharedState) -> Result<Self, String> {
        state
            .audio_mod_build_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "已有一个识别 Mod 正在准备，请稍候".to_string())?;
        Ok(Self(state.clone()))
    }
}

impl Drop for BuildLease {
    fn drop(&mut self) {
        self.0.audio_mod_build_busy.store(false, Ordering::SeqCst);
    }
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

fn plain_mod_name(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Mod 名称不能为空".to_string());
    }
    let path = Path::new(value);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err("Mod 名称不能包含目录路径".to_string());
    }
    Ok(value)
}

fn generated_audio_mod_name(value: &str) -> Result<&str, String> {
    let value = plain_mod_name(value)?;
    if value.len() > 128 {
        return Err("Mod 名称不能超过 128 个字符".to_string());
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Mod 名称仅可使用英文字母、数字、短横线和下划线".to_string());
    }
    let uppercase = value.to_ascii_uppercase();
    if matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || uppercase
            .strip_prefix("COM")
            .or_else(|| uppercase.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
    {
        return Err("该名称是 Windows 保留名称，请换一个".to_string());
    }
    Ok(value)
}

fn find_existing_mod_name(
    mods_directory: &Path,
    candidate: &str,
) -> Result<Option<String>, String> {
    let entries = std::fs::read_dir(mods_directory)
        .map_err(|error| format!("读取 mods 目录失败: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("读取 mods 目录项失败: {error}"))?;
        let existing_name = entry.file_name().to_string_lossy().into_owned();
        if existing_name.eq_ignore_ascii_case(candidate) {
            return Ok(Some(existing_name));
        }
    }
    Ok(None)
}

fn active_mod_name(mod_args: &str) -> Result<Option<String>, String> {
    let args = parse_windows_command_line(mod_args)
        .map_err(|error| format!("无法解析账号启动参数: {error}"))?;
    let mut index = 0usize;
    while index < args.len() {
        let argument = &args[index];
        if argument.eq_ignore_ascii_case("-mod") {
            let value = args
                .get(index + 1)
                .ok_or_else(|| "-mod 后缺少 Mod 名称".to_string())?;
            return Ok(Some(plain_mod_name(value)?.to_string()));
        }
        if argument
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("-mod="))
        {
            return Ok(Some(plain_mod_name(&argument[5..])?.to_string()));
        }
        index += 1;
    }
    Ok(None)
}

fn has_txt_argument(mod_args: &str) -> Result<bool, String> {
    Ok(parse_windows_command_line(mod_args)
        .map_err(|error| format!("无法解析账号启动参数: {error}"))?
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("-txt")))
}

fn read_protocol_version(path: &Path) -> Result<u8, String> {
    let document: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|error| format!("读取 {} 失败: {error}", path.display()))?,
    )
    .map_err(|error| format!("解析 {} 失败: {error}", path.display()))?;
    document
        .get("protocol_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| format!("{} 缺少协议版本", path.display()))
}

fn source_mod_name_from_manifest(
    manifest: &serde_json::Value,
    mods_directory: &Path,
) -> Option<String> {
    if let Some(source) = manifest
        .get("source_mod_name")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| plain_mod_name(value).ok())
    {
        return Some(source.to_string());
    }

    // 0.1.1–0.1.3 only recorded the copied Excel directory. Match that path against
    // direct installed Mod roots so old official releases can recover their source safely.
    let legacy_source = manifest
        .get("source_excel_directory")
        .and_then(serde_json::Value::as_str)?
        .replace('/', "\\");
    std::fs::read_dir(mods_directory)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|entry| {
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            plain_mod_name(&name).ok()?;
            let source_root = entry
                .path()
                .join(format!("{name}.mpq"))
                .to_string_lossy()
                .replace('/', "\\");
            let prefix = format!("{source_root}\\");
            legacy_source
                .get(..prefix.len())
                .is_some_and(|actual| actual.eq_ignore_ascii_case(&prefix))
                .then_some(name)
        })
}

fn validate_audio_mod(mods_directory: &Path, mod_name: &str) -> Result<ValidatedAudioMod, String> {
    let mod_name = plain_mod_name(mod_name)?;
    let mod_directory = mods_directory.join(mod_name);
    if !mod_directory.is_dir() {
        return Err(format!("未找到 Mod：{mod_name}"));
    }
    if !mod_directory.join(format!("{mod_name}.mpq")).is_dir() {
        return Err("Mod 目录结构不完整".to_string());
    }

    let manifest_path = mod_directory.join(MANIFEST_FILE_NAME);
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|_| "这个 Mod 未经过 D2RHub 声纹加工".to_string())?,
    )
    .map_err(|_| "识别 Mod 清单已损坏，请重新准备".to_string())?;
    let protocol = manifest
        .get("protocol_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "识别 Mod 清单缺少协议版本".to_string())?;
    if protocol != u64::from(PROTOCOL_VERSION) {
        return Err(format!(
            "识别 Mod 协议版本不匹配（需要 v{PROTOCOL_VERSION}）"
        ));
    }
    match manifest.get("manifest_format") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(format)) if format == MANIFEST_FORMAT => {}
        Some(serde_json::Value::String(_)) => return Err("识别 Mod 清单类型不受支持".to_string()),
        Some(_) => return Err("识别 Mod 清单类型无效，请重新准备".to_string()),
    }
    match manifest.get("producer") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(producer)) if producer == PRODUCER_NAME => {}
        Some(serde_json::Value::String(_)) => return Err("识别 Mod 生成器不受支持".to_string()),
        Some(_) => return Err("识别 Mod 清单的生成器信息无效，请重新准备".to_string()),
    }
    match manifest.get("mod_name") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(recorded_name)) if recorded_name == mod_name => {}
        Some(serde_json::Value::String(_)) => {
            return Err("识别 Mod 已被改名，请重新准备".to_string())
        }
        Some(_) => return Err("识别 Mod 清单的名称无效，请重新准备".to_string()),
    }

    let recipe_version = match manifest.get("recipe_version") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|version| u32::try_from(version).ok())
                .ok_or_else(|| "识别 Mod 清单的配方版本无效，请重新准备".to_string())?,
        ),
    };
    let build_mode = manifest
        .get("build_mode")
        .and_then(serde_json::Value::as_str)
        .filter(|value| matches!(*value, "minimal" | "augment"))
        .map(str::to_string);
    let source_mod_name = source_mod_name_from_manifest(&manifest, mods_directory);

    for catalog in [AREA_CATALOG_FILE_NAME, ITEM_CATALOG_FILE_NAME] {
        let version = read_protocol_version(&mod_directory.join(catalog))?;
        if version != PROTOCOL_VERSION {
            return Err(format!("{catalog} 协议版本不匹配"));
        }
    }
    Ok(ValidatedAudioMod {
        directory: mod_directory,
        recipe_version,
        build_mode,
        source_mod_name,
    })
}

fn official_update_metadata(
    mods_directory: &Path,
    mod_name: &str,
) -> Option<OfficialUpdateMetadata> {
    let mod_name = plain_mod_name(mod_name).ok()?;
    let mod_directory = mods_directory.join(mod_name);
    if !mod_directory.is_dir() || !mod_directory.join(format!("{mod_name}.mpq")).is_dir() {
        return None;
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(mod_directory.join(MANIFEST_FILE_NAME)).ok()?)
            .ok()?;
    if manifest
        .get("protocol_version")
        .is_none_or(|value| value.as_u64().is_none())
    {
        return None;
    }
    match manifest.get("manifest_format") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(format)) if format == MANIFEST_FORMAT => {}
        _ => return None,
    }
    match manifest.get("producer") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(producer)) if producer == PRODUCER_NAME => {}
        _ => return None,
    }
    match manifest.get("mod_name") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(recorded)) if recorded == mod_name => {}
        _ => return None,
    }
    let recipe_version = match manifest.get("recipe_version") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(u32::try_from(value.as_u64()?).ok()?),
    };
    let build_mode = match manifest.get("build_mode") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value))
            if matches!(value.as_str(), "minimal" | "augment") =>
        {
            Some(value.clone())
        }
        _ => return None,
    };
    let source_mod_name = match manifest.get("source_mod_name") {
        None | Some(serde_json::Value::Null) => {
            source_mod_name_from_manifest(&manifest, mods_directory)
        }
        Some(serde_json::Value::String(value)) => Some(plain_mod_name(value).ok()?.to_string()),
        _ => return None,
    };
    Some(OfficialUpdateMetadata {
        recipe_version,
        build_mode,
        source_mod_name,
    })
}

fn compatibility(mods_directory: &Path, launch_arguments: &str) -> Compatibility {
    let has_txt = has_txt_argument(launch_arguments).unwrap_or(false);
    let mod_name = match active_mod_name(launch_arguments) {
        Ok(value) => value,
        Err(error) => {
            return Compatibility {
                mod_name: None,
                has_txt,
                ready: false,
                update_required: false,
                recipe_version: None,
                build_mode: None,
                source_mod_name: None,
                reason_code: "invalid_arguments".to_string(),
                message: error,
            }
        }
    };
    let Some(name) = mod_name.clone() else {
        return Compatibility {
            mod_name,
            has_txt,
            ready: false,
            update_required: false,
            recipe_version: None,
            build_mode: None,
            source_mod_name: None,
            reason_code: "missing_mod".to_string(),
            message: "当前账号还没有使用识别 Mod".to_string(),
        };
    };
    if !has_txt {
        return Compatibility {
            mod_name,
            has_txt,
            ready: false,
            update_required: false,
            recipe_version: None,
            build_mode: None,
            source_mod_name: None,
            reason_code: "missing_txt".to_string(),
            message: "启动参数缺少 -txt，声纹资源不会生效".to_string(),
        };
    }
    match validate_audio_mod(mods_directory, &name) {
        Ok(validated) => {
            let update_required = validated
                .recipe_version
                .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION);
            Compatibility {
                mod_name,
                has_txt,
                ready: true,
                update_required,
                recipe_version: validated.recipe_version,
                build_mode: validated.build_mode,
                source_mod_name: validated.source_mod_name,
                reason_code: if update_required {
                    "update_available".to_string()
                } else {
                    "ready".to_string()
                },
                message: if update_required {
                    "旧版识别 Mod 仍可使用；更新后可获得即时恐怖区域识别".to_string()
                } else {
                    "识别 Mod 已准备好".to_string()
                },
            }
        }
        Err(error) => {
            let update_metadata = official_update_metadata(mods_directory, &name);
            Compatibility {
                mod_name,
                has_txt,
                ready: false,
                update_required: update_metadata.is_some(),
                recipe_version: update_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.recipe_version),
                build_mode: update_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.build_mode.clone()),
                source_mod_name: update_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.source_mod_name.clone()),
                reason_code: if update_metadata.is_some() {
                    "update_required".to_string()
                } else {
                    "unsupported_mod".to_string()
                },
                message: if update_metadata.is_some() {
                    format!("旧版识别 Mod 与当前版本不兼容（{error}）；可保留原名称直接更新")
                } else {
                    error
                },
            }
        }
    }
}

fn installed_mods(mods_directory: &Path) -> Vec<InstalledMod> {
    let mut mods = std::fs::read_dir(mods_directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !entry.path().join(format!("{name}.mpq")).is_dir() {
                return None;
            }
            let source_eligible = !entry.path().join(MANIFEST_FILE_NAME).exists();
            let validation = validate_audio_mod(mods_directory, &name);
            let audio_ready = validation.is_ok();
            let update_required = match validation.as_ref() {
                Ok(validated) => validated
                    .recipe_version
                    .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION),
                Err(_) => official_update_metadata(mods_directory, &name).is_some(),
            };
            Some(InstalledMod {
                name,
                audio_ready,
                update_required,
                source_eligible,
            })
        })
        .collect::<Vec<_>>();
    mods.sort_by_key(|entry| entry.name.to_lowercase());
    mods
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

fn running_accounts_using_mod(
    state: &SharedState,
    config: &GlobalConfig,
    mod_name: &str,
) -> Vec<(String, u32)> {
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

fn ensure_audio_mod_not_in_use(
    state: &SharedState,
    config: &GlobalConfig,
    mod_name: &str,
) -> Result<(), String> {
    let running = running_accounts_using_mod(state, config, mod_name);
    if running.is_empty() {
        return Ok(());
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
    let configured = compatibility(&mods_directory, &account.mod_args);
    let (session_arguments, running_pid, session_verified) = session_arguments(state, &account);
    let active_session = running_pid
        .and_then(|_| session_verified.then(|| compatibility(&mods_directory, &session_arguments)));
    let active_session_ready = active_session.as_ref().map(|result| result.ready);
    let active_session_update_required =
        active_session.as_ref().map(|result| result.update_required);
    let restart_required = running_pid.is_some()
        && !configured.update_required
        && (active_session_ready != Some(true) || active_session_update_required != Some(false));
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
pub fn get_audio_mod_setup_state(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<AudioModSetupState, String> {
    setup_state(state.inner(), &account_id)
}

fn emit_prepare_progress(
    app: &tauri::AppHandle,
    account_id: &str,
    phase: &str,
    percent: u8,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "audio-mod-prepare-progress",
        AudioModPrepareProgress {
            account_id: account_id.to_string(),
            phase: phase.to_string(),
            percent: percent.min(100),
            message: message.into(),
        },
    );
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create(path: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("创建临时目录失败 {}: {error}", path.display()))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn resolve_source_directory(
    mods_directory: &Path,
    output_mod_name: &str,
    source_mod_name: Option<String>,
) -> Result<(Option<String>, Option<PathBuf>), String> {
    let source_mod_name = source_mod_name
        .map(|name| plain_mod_name(&name).map(str::to_string))
        .transpose()?;
    if source_mod_name
        .as_deref()
        .is_some_and(|source| source.eq_ignore_ascii_case(output_mod_name))
    {
        return Err("生成目标不能同时作为源 Mod；请选择未经 D2RHub 加工的原始 Mod".to_string());
    }
    let source_directory = source_mod_name
        .as_deref()
        .map(|name| mods_directory.join(name));
    if let Some(source) = source_directory.as_ref() {
        if !source.is_dir() {
            return Err(format!("未找到源 Mod：{}", source.display()));
        }
        if source.join(MANIFEST_FILE_NAME).exists() {
            return Err("请选择原始 Mod，不要再次加工旧版、损坏或已经生成的识别 Mod".to_string());
        }
    }
    Ok((source_mod_name, source_directory))
}

async fn run_audio_mod_generator(
    app: &tauri::AppHandle,
    account_id: &str,
    game_directory: &Path,
    output_directory: &Path,
    mod_name: &str,
    source_directory: Option<&Path>,
    progress_ceiling: u8,
) -> Result<GeneratorReport, String> {
    let command_name = if source_directory.is_some() {
        "augment"
    } else {
        "minimal"
    };
    let mut arguments = vec![
        command_name.to_string(),
        "--game".to_string(),
        game_directory.to_string_lossy().into_owned(),
        "--output".to_string(),
        output_directory.to_string_lossy().into_owned(),
        "--name".to_string(),
        mod_name.to_string(),
        "--areas".to_string(),
        "all".to_string(),
        "--track".to_string(),
        "all".to_string(),
        "--events".to_string(),
    ];
    if let Some(source) = source_directory {
        arguments.push("--source".to_string());
        arguments.push(source.to_string_lossy().into_owned());
    }

    let (mut receiver, _child) = app
        .shell()
        .sidecar("d2r-audio-mod")
        .map_err(|error| format!("识别 Mod 生成器不可用: {error}"))?
        .args(arguments)
        .spawn()
        .map_err(|error| format!("无法启动识别 Mod 生成器: {error}"))?;

    let mut report: Option<GeneratorReport> = None;
    let mut stderr = String::new();
    let mut exit_code = None;
    while let Some(event) = receiver.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
                    continue;
                };
                match value.get("type").and_then(serde_json::Value::as_str) {
                    Some("progress") => {
                        let reported_percent = value
                            .get("percent")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|percent| u8::try_from(percent).ok())
                            .unwrap_or(0);
                        emit_prepare_progress(
                            app,
                            account_id,
                            value
                                .get("phase")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("working"),
                            ((u16::from(reported_percent) * u16::from(progress_ceiling)) / 100)
                                as u8,
                            value
                                .get("message")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("正在准备…"),
                        );
                    }
                    Some("completed") => {
                        report = Some(
                            serde_json::from_value(
                                value.get("report").cloned().unwrap_or_default(),
                            )
                            .map_err(|error| format!("生成器返回了无效结果: {error}"))?,
                        );
                    }
                    Some("error") => {
                        if let Some(message) =
                            value.get("message").and_then(serde_json::Value::as_str)
                        {
                            stderr = message.to_string();
                        }
                    }
                    _ => {}
                }
            }
            CommandEvent::Stderr(bytes) => {
                if stderr.len() < 8_000 {
                    stderr.push_str(&String::from_utf8_lossy(&bytes));
                    stderr.push('\n');
                }
            }
            CommandEvent::Error(error) => stderr.push_str(&error),
            CommandEvent::Terminated(payload) => exit_code = payload.code,
            _ => {}
        }
    }

    if exit_code != Some(0) {
        return Err(if stderr.trim().is_empty() {
            format!("识别 Mod 生成失败（退出码 {:?}）", exit_code)
        } else {
            stderr.trim().to_string()
        });
    }
    report.ok_or_else(|| "生成器没有返回完成结果".to_string())
}

fn validate_generator_output(
    output_directory: &Path,
    requested_mod_name: &str,
    report: &GeneratorReport,
) -> Result<PathBuf, String> {
    if report.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "生成器协议版本不匹配：收到 v{}，需要 v{PROTOCOL_VERSION}",
            report.protocol_version
        ));
    }
    if report.recipe_version < REQUIRED_AUDIO_MOD_RECIPE_VERSION {
        return Err(format!(
            "生成器配方版本过旧：收到 r{}，需要 r{REQUIRED_AUDIO_MOD_RECIPE_VERSION}",
            report.recipe_version
        ));
    }
    if report.mod_name != requested_mod_name {
        return Err("生成器返回的 Mod 名称与用户指定名称不一致".to_string());
    }
    let validated = validate_audio_mod(output_directory, &report.mod_name)?;
    if validated
        .recipe_version
        .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION)
    {
        return Err("生成结果缺少当前配方版本，请重新安装 D2RHub 后重试".to_string());
    }
    let reported_directory = std::fs::canonicalize(&report.mod_directory)
        .map_err(|error| format!("无法校验生成目录: {error}"))?;
    let validated_directory = std::fs::canonicalize(validated.directory)
        .map_err(|error| format!("无法校验识别 Mod: {error}"))?;
    if reported_directory != validated_directory {
        return Err("生成器返回的目录与实际输出不一致".to_string());
    }
    Ok(validated_directory)
}

fn replace_audio_mod_directory(
    mods_directory: &Path,
    mod_name: &str,
    staged_directory: &Path,
    backup_directory: &Path,
) -> Result<(), String> {
    let target_directory = mods_directory.join(mod_name);
    if !staged_directory.is_dir() {
        return Err("同名更新的暂存目录不存在".to_string());
    }
    if !target_directory.is_dir() {
        return Err(format!("待更新的 Mod 不存在：{mod_name}"));
    }
    if backup_directory.exists() {
        return Err("同名更新的备份目录发生冲突，请重试".to_string());
    }

    std::fs::rename(&target_directory, backup_directory)
        .map_err(|error| format!("无法备份旧 Mod；请确认游戏已经关闭：{error}"))?;
    match std::fs::rename(staged_directory, &target_directory) {
        Ok(()) => {
            // 新目录已完成同盘原子切换，旧目录仅在切换成功后清理。
            let _ = std::fs::remove_dir_all(backup_directory);
            Ok(())
        }
        Err(install_error) => match std::fs::rename(backup_directory, &target_directory) {
            Ok(()) => Err(format!("安装新版 Mod 失败，已恢复旧版：{install_error}")),
            Err(rollback_error) => Err(format!(
                "安装新版 Mod 失败且自动恢复失败；旧版仍保留在 {}。安装错误：{}；恢复错误：{}",
                backup_directory.display(),
                install_error,
                rollback_error
            )),
        },
    }
}

#[tauri::command]
pub async fn prepare_audio_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    mod_name: String,
    source_mod_name: Option<String>,
) -> Result<AudioModPrepareResult, String> {
    let shared_state = state.inner().clone();
    let _lease = BuildLease::acquire(&shared_state)?;
    let (_config, _account, context) = configured_account(&shared_state, &account_id)?;
    let game_directory = context.installation.game_directory;
    let mods_directory = game_directory.join("mods");
    std::fs::create_dir_all(&mods_directory)
        .map_err(|error| format!("创建 mods 目录失败: {error}"))?;

    let mod_name = generated_audio_mod_name(&mod_name)?.to_string();
    if let Some(existing_name) = find_existing_mod_name(&mods_directory, &mod_name)? {
        return Err(format!("Mod 名称“{existing_name}”已存在，请换一个名称"));
    }

    let (source_mod_name, source_directory) =
        resolve_source_directory(&mods_directory, &mod_name, source_mod_name)?;

    emit_prepare_progress(&app, &account_id, "starting", 1, "正在开始准备…");
    let report = run_audio_mod_generator(
        &app,
        &account_id,
        &game_directory,
        &mods_directory,
        &mod_name,
        source_directory.as_deref(),
        100,
    )
    .await?;
    validate_generator_output(&mods_directory, &mod_name, &report)?;
    emit_prepare_progress(&app, &account_id, "complete", 100, "识别 Mod 已准备完成");
    Ok(AudioModPrepareResult {
        account_id,
        mod_name: report.mod_name,
        mod_directory: report.mod_directory,
        launch_arguments: arguments_with_audio_mod("", &mod_name)?,
        source_mod_name,
    })
}

#[tauri::command]
pub async fn upgrade_audio_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    source_mod_name: Option<String>,
) -> Result<AudioModSetupState, String> {
    let shared_state = state.inner().clone();
    let _lease = BuildLease::acquire(&shared_state)?;
    let (config, account, context) = configured_account(&shared_state, &account_id)?;
    let mods_directory = context.installation.game_directory.join("mods");
    let current = compatibility(&mods_directory, &account.mod_args);
    if !current.update_required {
        return Err("当前账号没有需要原位更新的旧版识别 Mod".to_string());
    }
    let mod_name = current
        .mod_name
        .as_deref()
        .ok_or_else(|| "当前账号没有配置识别 Mod".to_string())?;
    ensure_audio_mod_not_in_use(&shared_state, &config, mod_name)?;

    let requested_source = source_mod_name;
    if current.build_mode.as_deref() == Some("augment") && requested_source.is_none() {
        return Err("这个旧版识别 Mod 基于其他 Mod 生成；请选择当时未经加工的原始 Mod".to_string());
    }
    let (_source_name, source_directory) =
        resolve_source_directory(&mods_directory, mod_name, requested_source)?;

    emit_prepare_progress(&app, &account_id, "starting", 1, "正在生成同名新版 Mod…");
    let temporary_output = TemporaryDirectory::create(std::env::temp_dir().join(format!(
        "d2rhub-audio-upgrade-output-{}",
        uuid::Uuid::new_v4()
    )))?;
    let report = run_audio_mod_generator(
        &app,
        &account_id,
        &context.installation.game_directory,
        temporary_output.path(),
        mod_name,
        source_directory.as_deref(),
        85,
    )
    .await?;
    let generated_directory =
        validate_generator_output(temporary_output.path(), mod_name, &report)?;

    emit_prepare_progress(&app, &account_id, "staging", 90, "正在校验并暂存新版 Mod…");
    let transaction_id = uuid::Uuid::new_v4().simple().to_string();
    let staging_parent = TemporaryDirectory::create(
        mods_directory.join(format!(".d2rhub-upgrade-stage-{transaction_id}")),
    )?;
    let staged_directory = staging_parent.path().join(mod_name);
    crate::commands::utils::copy_dir_recursive(&generated_directory, &staged_directory)
        .map_err(|error| format!("暂存新版 Mod 失败: {error}"))?;
    let staged = validate_audio_mod(staging_parent.path(), mod_name)?;
    if staged
        .recipe_version
        .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION)
    {
        return Err("暂存的 Mod 未通过当前配方校验，旧版未被修改".to_string());
    }

    emit_prepare_progress(&app, &account_id, "switching", 96, "正在替换同名旧版 Mod…");
    // 生成过程可能持续数分钟；切换前再次检查，避免另一账号中途启动同名 Mod。
    ensure_audio_mod_not_in_use(&shared_state, &config, mod_name)?;
    let backup_directory = mods_directory.join(format!(".d2rhub-upgrade-backup-{transaction_id}"));
    replace_audio_mod_directory(
        &mods_directory,
        mod_name,
        &staged_directory,
        &backup_directory,
    )?;
    let installed = validate_audio_mod(&mods_directory, mod_name)?;
    if installed
        .recipe_version
        .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION)
    {
        return Err("同名更新完成后校验异常，请重新准备识别 Mod".to_string());
    }
    emit_prepare_progress(
        &app,
        &account_id,
        "complete",
        100,
        "同名识别 Mod 已更新完成",
    );
    setup_state(&shared_state, &account_id)
}

fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty()
        && !argument
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return argument.to_string();
    }
    let mut output = String::from("\"");
    let mut slashes = 0usize;
    for character in argument.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        if character == '"' {
            output.push_str(&"\\".repeat(slashes * 2 + 1));
            output.push('"');
        } else {
            output.push_str(&"\\".repeat(slashes));
            output.push(character);
        }
        slashes = 0;
    }
    output.push_str(&"\\".repeat(slashes * 2));
    output.push('"');
    output
}

pub(crate) fn arguments_with_audio_mod(
    existing_arguments: &str,
    mod_name: &str,
) -> Result<String, String> {
    let mod_name = plain_mod_name(mod_name)?;
    let arguments = parse_windows_command_line(existing_arguments)
        .map_err(|error| format!("无法解析原启动参数: {error}"))?;
    let mut preserved = Vec::new();
    let mut index = 0usize;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument.eq_ignore_ascii_case("-mod") {
            index += 2;
            continue;
        }
        if argument
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("-mod="))
            || argument.eq_ignore_ascii_case("-txt")
        {
            index += 1;
            continue;
        }
        if argument.eq_ignore_ascii_case("-assettestmode") {
            index += 1;
            if arguments
                .get(index)
                .is_some_and(|value| !value.starts_with('-'))
            {
                index += 1;
            }
            continue;
        }
        if argument
            .get(..15)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("-assettestmode="))
        {
            index += 1;
            continue;
        }
        preserved.push(argument.clone());
        index += 1;
    }
    preserved.push("-mod".to_string());
    preserved.push(mod_name.to_string());
    preserved.push("-txt".to_string());
    preserved.push("-assettestmode".to_string());
    preserved.push("1".to_string());
    Ok(preserved
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" "))
}

#[tauri::command]
pub fn apply_audio_mod_to_account(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    mod_name: String,
) -> Result<AudioModSetupState, String> {
    let (_config, account, context) = configured_account(state.inner(), &account_id)?;
    validate_audio_mod(&context.installation.game_directory.join("mods"), &mod_name)?;
    let next_arguments = arguments_with_audio_mod(&account.mod_args, &mod_name)?;
    let mut mod_list = account.mod_list.clone();
    if !mod_list.iter().any(|entry| entry == &next_arguments) {
        mod_list.push(next_arguments.clone());
    }
    update_account_mods_inner(state.inner(), account_id.clone(), next_arguments, mod_list)
        .map_err(|error| error.to_string())?;
    setup_state(state.inner(), &account_id)
}

pub(crate) fn validate_runtime_audio_mod(
    config: &GlobalConfig,
    account: &AccountMeta,
    launch_arguments: &str,
) -> Result<PathBuf, String> {
    let context = LaunchContext::for_account(config, account, ContextPurpose::Settings)
        .map_err(|error| error.to_string())?;
    let mods_directory = context.installation.game_directory.join("mods");
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

pub(crate) fn emit_runtime_compatibility_warning(
    app: &tauri::AppHandle,
    state: &SharedState,
    config: &GlobalConfig,
    account: &AccountMeta,
    pid: u32,
    launch_arguments: &str,
) {
    if !config.rune_audio_enabled || config.rune_audio_target_account != account.id {
        return;
    }
    let context = match LaunchContext::for_account(config, account, ContextPurpose::Settings) {
        Ok(context) => context,
        Err(_) => return,
    };
    let result = compatibility(
        &context.installation.game_directory.join("mods"),
        launch_arguments,
    );
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
mod tests {
    use super::{
        active_mod_name, arguments_with_audio_mod, compatibility, find_existing_mod_name,
        generated_audio_mod_name, has_txt_argument, installed_mods, replace_audio_mod_directory,
        resolve_source_directory, AREA_CATALOG_FILE_NAME, ITEM_CATALOG_FILE_NAME, PROTOCOL_VERSION,
        REQUIRED_AUDIO_MOD_RECIPE_VERSION,
    };

    fn write_test_audio_mod(
        mods_directory: &std::path::Path,
        mod_name: &str,
        manifest: serde_json::Value,
    ) {
        let mod_directory = mods_directory.join(mod_name);
        std::fs::create_dir_all(mod_directory.join(format!("{mod_name}.mpq"))).unwrap();
        std::fs::write(
            mod_directory.join("audio-telemetry-manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let catalog = serde_json::to_vec(&serde_json::json!({
            "protocol_version": PROTOCOL_VERSION
        }))
        .unwrap();
        std::fs::write(mod_directory.join(AREA_CATALOG_FILE_NAME), &catalog).unwrap();
        std::fs::write(mod_directory.join(ITEM_CATALOG_FILE_NAME), &catalog).unwrap();
    }

    fn test_mods_directory(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("d2rhub_audio_mod_{label}_{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn rewrites_only_mod_and_txt_arguments() {
        let result = arguments_with_audio_mod(
            r#"-w -mod "Old Mod" -assettestmode 1 --label "hello world""#,
            "jcy-D2RHubAudio",
        )
        .unwrap();
        assert_eq!(
            active_mod_name(&result).unwrap().as_deref(),
            Some("jcy-D2RHubAudio")
        );
        assert!(has_txt_argument(&result).unwrap());
        assert!(result.contains("-w"));
        assert!(result.contains("-assettestmode 1"));
        assert!(result.contains("--label \"hello world\""));
        assert!(!result.contains("Old Mod"));
    }

    #[test]
    fn adds_audio_arguments_to_an_original_profile() {
        let result = arguments_with_audio_mod("-w", "D2RHubAudio").unwrap();
        assert_eq!(result, "-w -mod D2RHubAudio -txt -assettestmode 1");
    }

    #[test]
    fn normalizes_existing_asset_test_mode_arguments() {
        let result =
            arguments_with_audio_mod("-assettestmode 0 -w -assettestmode=1 -txt", "FreshAudio")
                .unwrap();
        assert_eq!(result, "-w -mod FreshAudio -txt -assettestmode 1");
    }

    #[test]
    fn validates_user_supplied_generated_mod_names() {
        assert_eq!(
            generated_audio_mod_name("  My-Audio_2  ").unwrap(),
            "My-Audio_2"
        );
        assert!(generated_audio_mod_name("").is_err());
        assert!(generated_audio_mod_name("My Audio").is_err());
        assert!(generated_audio_mod_name("我的Mod").is_err());
        assert!(generated_audio_mod_name("CON").is_err());
        assert!(generated_audio_mod_name("lpt9").is_err());
    }

    #[test]
    fn detects_existing_mod_names_case_insensitively() {
        let root = std::env::temp_dir().join(format!(
            "d2rhub_audio_mod_collision_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("jcy")).unwrap();

        assert_eq!(
            find_existing_mod_name(&root, "jcy").unwrap().as_deref(),
            Some("jcy")
        );
        assert_eq!(
            find_existing_mod_name(&root, "JCY").unwrap().as_deref(),
            Some("jcy")
        );
        assert_eq!(find_existing_mod_name(&root, "fresh").unwrap(), None);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_name_update_switches_only_after_staged_mod_is_ready() {
        let root = test_mods_directory("same_name_upgrade");
        let mods = root.join("mods");
        let staging = root.join("staging");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&staging).unwrap();
        write_test_audio_mod(
            &mods,
            "jcy-tz",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "build_mode": "minimal",
                "mod_name": "jcy-tz"
            }),
        );
        std::fs::write(mods.join("jcy-tz").join("old-marker.txt"), b"old").unwrap();
        write_test_audio_mod(
            &staging,
            "jcy-tz",
            serde_json::json!({
                "manifest_format": "d2r-audio-telemetry-mod",
                "producer": "d2r-audio-mod",
                "protocol_version": PROTOCOL_VERSION,
                "recipe_version": REQUIRED_AUDIO_MOD_RECIPE_VERSION,
                "build_mode": "minimal",
                "mod_name": "jcy-tz"
            }),
        );
        std::fs::write(staging.join("jcy-tz").join("new-marker.txt"), b"new").unwrap();
        let backup = mods.join(".d2rhub-upgrade-backup-test");

        replace_audio_mod_directory(&mods, "jcy-tz", &staging.join("jcy-tz"), &backup).unwrap();

        assert!(mods.join("jcy-tz").join("new-marker.txt").is_file());
        assert!(!mods.join("jcy-tz").join("old-marker.txt").exists());
        assert!(!backup.exists());
        let result = compatibility(&mods, "-mod jcy-tz -txt");
        assert!(result.ready);
        assert!(!result.update_required);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generated_mod_is_never_accepted_as_an_upgrade_source() {
        let root = test_mods_directory("generated_source");
        write_test_audio_mod(
            &root,
            "old-audio",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "build_mode": "augment",
                "mod_name": "old-audio"
            }),
        );

        let error =
            resolve_source_directory(&root, "old-audio-updated", Some("old-audio".to_string()))
                .unwrap_err();

        assert!(error.contains("不要再次加工"));
        assert!(root.join("old-audio").is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn published_legacy_manifests_remain_usable_but_request_an_update() {
        let root = test_mods_directory("legacy_recipe");
        let source_excel = root.join("jcy").join("jcy.mpq").join("data/global/excel");
        std::fs::create_dir_all(&source_excel).unwrap();
        write_test_audio_mod(
            &root,
            "early-official",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "build_mode": "minimal"
            }),
        );
        write_test_audio_mod(
            &root,
            "v013-official",
            serde_json::json!({
                "manifest_format": "d2r-audio-telemetry-mod",
                "producer": "d2r-audio-mod",
                "producer_version": "0.1.3",
                "protocol_version": PROTOCOL_VERSION,
                "build_mode": "augment",
                "mod_name": "v013-official",
                "source_excel_directory": source_excel
            }),
        );

        for name in ["early-official", "v013-official"] {
            let result = compatibility(&root, &format!("-mod {name} -txt"));
            assert!(result.ready, "legacy Mod {name} should stay usable");
            assert!(result.update_required);
            assert_eq!(result.reason_code, "update_available");
            assert_eq!(result.recipe_version, None);
        }
        let augmented = compatibility(&root, "-mod v013-official -txt");
        assert_eq!(augmented.source_mod_name.as_deref(), Some("jcy"));
        let listed = installed_mods(&root);
        let generated = listed
            .iter()
            .filter(|entry| entry.name != "jcy")
            .collect::<Vec<_>>();
        assert!(generated.iter().all(|entry| entry.audio_ready));
        assert!(generated.iter().all(|entry| entry.update_required));
        assert!(generated.iter().all(|entry| !entry.source_eligible));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_and_future_recipes_do_not_trigger_false_updates() {
        let root = test_mods_directory("current_recipe");
        for version in [
            REQUIRED_AUDIO_MOD_RECIPE_VERSION,
            REQUIRED_AUDIO_MOD_RECIPE_VERSION + 1,
        ] {
            let name = format!("recipe-{version}");
            write_test_audio_mod(
                &root,
                &name,
                serde_json::json!({
                    "manifest_format": "d2r-audio-telemetry-mod",
                    "producer": "d2r-audio-mod",
                    "protocol_version": PROTOCOL_VERSION,
                    "recipe_version": version,
                    "build_mode": "augment",
                    "source_mod_name": "jcy",
                    "mod_name": name
                }),
            );
            let result = compatibility(&root, &format!("-mod {name} -txt"));
            assert!(result.ready);
            assert!(!result.update_required);
            assert_eq!(result.reason_code, "ready");
            assert_eq!(result.recipe_version, Some(version));
            assert_eq!(result.source_mod_name.as_deref(), Some("jcy"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_recipe_metadata_is_not_treated_as_a_legacy_release() {
        let root = test_mods_directory("invalid_recipe");
        write_test_audio_mod(
            &root,
            "broken",
            serde_json::json!({
                "manifest_format": "d2r-audio-telemetry-mod",
                "producer": "d2r-audio-mod",
                "protocol_version": PROTOCOL_VERSION,
                "recipe_version": "two",
                "mod_name": "broken"
            }),
        );
        let result = compatibility(&root, "-mod broken -txt");
        assert!(!result.ready);
        assert!(!result.update_required);
        assert_eq!(result.reason_code, "unsupported_mod");
        assert!(result.message.contains("配方版本无效"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn published_old_protocol_can_update_in_place_but_foreign_manifests_remain_blocked() {
        let root = test_mods_directory("incompatible");
        write_test_audio_mod(
            &root,
            "old-protocol",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION - 1,
                "mod_name": "old-protocol"
            }),
        );
        write_test_audio_mod(
            &root,
            "foreign",
            serde_json::json!({
                "manifest_format": "d2r-audio-telemetry-mod",
                "producer": "someone-else",
                "protocol_version": PROTOCOL_VERSION,
                "mod_name": "foreign"
            }),
        );
        let old = compatibility(&root, "-mod old-protocol -txt");
        assert!(!old.ready);
        assert!(old.update_required);
        assert_eq!(old.reason_code, "update_required");
        assert!(old.message.contains("保留原名称直接更新"));

        let foreign = compatibility(&root, "-mod foreign -txt");
        assert!(!foreign.ready);
        assert!(!foreign.update_required);
        assert_eq!(foreign.reason_code, "unsupported_mod");
        std::fs::remove_dir_all(root).unwrap();
    }
}
