use crate::commands::account::{update_account_mods_inner, AccountManager, AccountMeta};
use crate::commands::launch::parse_windows_command_line;
use crate::domain::config::GlobalConfig;
use crate::launch_context::{ContextPurpose, LaunchContext};
use crate::rune_audio::catalog::AREA_CATALOG_FILE_NAME;
use crate::rune_audio::item_catalog::ITEM_CATALOG_FILE_NAME;
use crate::rune_audio::protocol::PROTOCOL_VERSION;
use crate::state::SharedState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::Emitter;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

const MANIFEST_FILE_NAME: &str = "d2rhub-mod-manifest.json";
const LEGACY_MANIFEST_FILE_NAME: &str = "audio-telemetry-manifest.json";
const MANIFEST_FORMAT: &str = "d2r-audio-telemetry-mod";
const PRODUCER_NAME: &str = "d2r-audio-mod";
const REQUIRED_AUDIO_MOD_RECIPE_VERSION: u32 = 22;
const AUDIO_TELEMETRY_FEATURE_ID: &str = "audio_telemetry";
const AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION: u32 = 1;
const IN_GAME_ROOM_TOOLS_FEATURE_ID: &str = "in_game_room_tools";
const REPLACE_JOURNAL_FORMAT_VERSION: u8 = 1;
const REPLACE_JOURNAL_PREFIX: &str = ".d2rhub-audio-replace-";
const REPLACE_JOURNAL_SUFFIX: &str = ".json";

#[derive(Debug, Clone, Serialize)]
pub struct InstalledMod {
    pub name: String,
    pub audio_ready: bool,
    pub update_required: bool,
    pub source_eligible: bool,
    pub feature_groups: Vec<String>,
    pub audio_reusable: bool,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GeneratorFeatureGroup {
    pub id: String,
    pub recipe_version: u32,
    pub fingerprint: String,
    #[serde(default)]
    pub reused_from_source: bool,
}

#[derive(Debug, Deserialize)]
struct GeneratorReport {
    protocol_version: u8,
    recipe_version: u32,
    mod_name: String,
    mod_directory: String,
    #[serde(default)]
    feature_groups: Vec<GeneratorFeatureGroup>,
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
    feature_groups: Vec<GeneratorFeatureGroup>,
    has_audio_telemetry: bool,
    current_feature_protocol: bool,
}

#[derive(Debug)]
struct ValidatedGeneratorOutput {
    directory: PathBuf,
    feature_groups: Vec<GeneratorFeatureGroup>,
}

#[derive(Debug, Clone, Copy)]
struct RequestedFeatureGroups {
    audio_telemetry: bool,
    room_tools: bool,
}

struct GeneratorInvocation<'a> {
    account_id: &'a str,
    game_directory: &'a Path,
    output_directory: &'a Path,
    mod_name: &'a str,
    source_directory: Option<&'a Path>,
    requested_features: RequestedFeatureGroups,
    progress_ceiling: u8,
}

impl RequestedFeatureGroups {
    fn from_options(
        audio_telemetry: Option<bool>,
        room_tools: Option<bool>,
    ) -> Result<Self, String> {
        // Missing fields preserve the pre-r22 command contract used by older D2RHub frontends.
        let requested = Self {
            audio_telemetry: audio_telemetry.unwrap_or(true),
            room_tools: room_tools.unwrap_or(false),
        };
        if !requested.audio_telemetry && !requested.room_tools {
            return Err("请至少选择一个要加工的功能".to_string());
        }
        Ok(requested)
    }

    fn generator_value(self) -> &'static str {
        match (self.audio_telemetry, self.room_tools) {
            (true, true) => "audio,rooms",
            (true, false) => "audio",
            (false, true) => "rooms",
            (false, false) => unreachable!("feature selection is validated at the command edge"),
        }
    }

    fn validate_present(self, groups: &[GeneratorFeatureGroup]) -> Result<(), String> {
        for (requested, id, label) in [
            (self.audio_telemetry, AUDIO_TELEMETRY_FEATURE_ID, "声纹识别"),
            (
                self.room_tools,
                IN_GAME_ROOM_TOOLS_FEATURE_ID,
                "局内房间工具",
            ),
        ] {
            if requested && !groups.iter().any(|group| group.id == id) {
                return Err(format!("生成结果缺少已选择的{label}功能组"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct AudioModReplaceJournal {
    format_version: u8,
    mod_name: String,
    staged_relative: PathBuf,
    backup_relative: PathBuf,
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

fn processing_manifest_path(mod_directory: &Path) -> Option<PathBuf> {
    [MANIFEST_FILE_NAME, LEGACY_MANIFEST_FILE_NAME]
        .iter()
        .map(|name| mod_directory.join(name))
        .find(|path| path.is_file())
}

fn parse_feature_groups(
    manifest: &serde_json::Value,
) -> Result<Vec<GeneratorFeatureGroup>, String> {
    let groups = match manifest.get("feature_groups") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(value) => serde_json::from_value::<Vec<GeneratorFeatureGroup>>(value.clone())
            .map_err(|_| "D2RHub Mod 清单的功能组信息无效，请重新加工".to_string())?,
    };
    validate_feature_group_entries(&groups)?;
    Ok(groups)
}

fn validate_feature_group_entries(groups: &[GeneratorFeatureGroup]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for group in groups {
        if group.id.is_empty()
            || group.id.len() > 128
            || !group.id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
        {
            return Err("D2RHub Mod 清单包含无效的功能组标识，请重新加工".to_string());
        }
        if !ids.insert(group.id.as_str()) {
            return Err(format!("D2RHub Mod 清单重复声明功能组：{}", group.id));
        }
        if group.recipe_version == 0
            || group.fingerprint.trim().is_empty()
            || group.fingerprint.len() > 4_096
        {
            return Err(format!("D2RHub Mod 功能组元数据无效：{}", group.id));
        }
    }
    Ok(())
}

fn validate_audio_mod_directory(
    mods_directory: &Path,
    mod_name: &str,
    mod_directory: PathBuf,
) -> Result<ValidatedAudioMod, String> {
    let mod_name = plain_mod_name(mod_name)?;
    if !mod_directory.is_dir() {
        return Err(format!("未找到 Mod：{mod_name}"));
    }
    if !mod_directory.join(format!("{mod_name}.mpq")).is_dir() {
        return Err("Mod 目录结构不完整".to_string());
    }

    let manifest_path = processing_manifest_path(&mod_directory)
        .ok_or_else(|| "这个 Mod 未经过 D2RHub 声纹加工".to_string())?;
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path).map_err(|_| "无法读取 D2RHub Mod 加工清单".to_string())?,
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
    let parsed_feature_groups = parse_feature_groups(&manifest)?;
    let current_feature_protocol = recipe_version
        .is_some_and(|version| version >= REQUIRED_AUDIO_MOD_RECIPE_VERSION)
        && !parsed_feature_groups.is_empty();
    let has_current_identity = manifest
        .get("manifest_format")
        .and_then(serde_json::Value::as_str)
        == Some(MANIFEST_FORMAT)
        && manifest.get("producer").and_then(serde_json::Value::as_str) == Some(PRODUCER_NAME)
        && manifest.get("mod_name").and_then(serde_json::Value::as_str) == Some(mod_name);
    if current_feature_protocol && !has_current_identity {
        return Err("当前功能组协议清单缺少完整的生成器身份信息，请重新加工".to_string());
    }
    // r21 and earlier manifests did not have independently verifiable feature groups. Keep their
    // published audio runtime working, but never expose their claims to additive generation.
    let feature_groups = if current_feature_protocol {
        parsed_feature_groups
    } else {
        Vec::new()
    };
    let has_audio_telemetry = feature_groups.is_empty()
        || feature_groups
            .iter()
            .any(|group| group.id == AUDIO_TELEMETRY_FEATURE_ID);
    let build_mode = manifest
        .get("build_mode")
        .and_then(serde_json::Value::as_str)
        .filter(|value| matches!(*value, "minimal" | "augment"))
        .map(str::to_string);
    let source_mod_name = source_mod_name_from_manifest(&manifest, mods_directory);

    if has_audio_telemetry {
        for catalog in [AREA_CATALOG_FILE_NAME, ITEM_CATALOG_FILE_NAME] {
            let version = read_protocol_version(&mod_directory.join(catalog))?;
            if version != PROTOCOL_VERSION {
                return Err(format!("{catalog} 协议版本不匹配"));
            }
        }
    }
    Ok(ValidatedAudioMod {
        directory: mod_directory,
        recipe_version,
        build_mode,
        source_mod_name,
        feature_groups,
        has_audio_telemetry,
        current_feature_protocol,
    })
}

fn validate_audio_mod(mods_directory: &Path, mod_name: &str) -> Result<ValidatedAudioMod, String> {
    let mod_name = plain_mod_name(mod_name)?;
    validate_audio_mod_directory(mods_directory, mod_name, mods_directory.join(mod_name))
}

fn validate_recoverable_audio_mod_directory(
    mods_directory: &Path,
    mod_name: &str,
    mod_directory: &Path,
) -> Result<(), String> {
    if validate_audio_mod_directory(mods_directory, mod_name, mod_directory.to_path_buf()).is_ok() {
        return Ok(());
    }
    let mod_name = plain_mod_name(mod_name)?;
    if !mod_directory.is_dir() || !mod_directory.join(format!("{mod_name}.mpq")).is_dir() {
        return Err("Mod 恢复目录结构不完整".to_string());
    }
    let manifest_path = processing_manifest_path(mod_directory)
        .ok_or_else(|| "Mod 恢复目录缺少 D2RHub 清单".to_string())?;
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(manifest_path).map_err(|error| format!("无法读取 Mod 恢复清单：{error}"))?,
    )
    .map_err(|_| "Mod 恢复清单已损坏".to_string())?;
    if manifest
        .get("protocol_version")
        .and_then(serde_json::Value::as_u64)
        .is_none()
    {
        return Err("Mod 恢复清单缺少协议版本".to_string());
    }
    match manifest.get("manifest_format") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(value)) if value == MANIFEST_FORMAT => {}
        _ => return Err("Mod 恢复清单类型无效".to_string()),
    }
    match manifest.get("producer") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(value)) if value == PRODUCER_NAME => {}
        _ => return Err("Mod 恢复清单生成器无效".to_string()),
    }
    match manifest.get("mod_name") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(value)) if value == mod_name => {}
        _ => return Err("Mod 恢复清单名称无效".to_string()),
    }
    if manifest.get("recipe_version").is_some_and(|value| {
        !value.is_null()
            && value
                .as_u64()
                .and_then(|version| u32::try_from(version).ok())
                .is_none()
    }) {
        return Err("Mod 恢复清单配方版本无效".to_string());
    }
    Ok(())
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
    let manifest_path = processing_manifest_path(&mod_directory)?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manifest_path).ok()?).ok()?;
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
            if !validated.has_audio_telemetry {
                return Compatibility {
                    mod_name,
                    has_txt,
                    ready: false,
                    update_required: false,
                    recipe_version: validated.recipe_version,
                    build_mode: validated.build_mode,
                    source_mod_name: validated.source_mod_name,
                    reason_code: "missing_audio_feature".to_string(),
                    message: "当前 Mod 已经过 D2RHub 加工，但没有声纹识别功能组".to_string(),
                };
            }
            let update_required = !validated.current_feature_protocol;
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
                    "旧版识别 Mod 仍可使用；重新加工后可获得可验证、可复用的独立功能组".to_string()
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
            let validation = validate_audio_mod(mods_directory, &name);
            let audio_ready = validation
                .as_ref()
                .is_ok_and(|validated| validated.has_audio_telemetry);
            let update_required = match validation.as_ref() {
                Ok(validated) if validated.has_audio_telemetry => {
                    !validated.current_feature_protocol
                }
                Ok(_) => false,
                Err(_) => official_update_metadata(mods_directory, &name).is_some(),
            };
            let feature_groups = validation
                .as_ref()
                .map(|validated| {
                    validated
                        .feature_groups
                        .iter()
                        .map(|group| group.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let audio_reusable = validation.as_ref().is_ok_and(|validated| {
                validated.current_feature_protocol
                    && validated.feature_groups.iter().any(|group| {
                        group.id == AUDIO_TELEMETRY_FEATURE_ID
                            && group.recipe_version == AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION
                    })
            });
            let has_processing_manifest = processing_manifest_path(&entry.path()).is_some();
            let source_eligible = match validation.as_ref() {
                Ok(validated) => validated.current_feature_protocol,
                Err(_) => !has_processing_manifest,
            };
            Some(InstalledMod {
                name,
                audio_ready,
                update_required,
                source_eligible,
                feature_groups,
                audio_reusable,
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
    let feature_groups = configured
        .mod_name
        .as_deref()
        .and_then(|name| validate_audio_mod(&mods_directory, name).ok())
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
    let _lease = BuildLease::acquire(state.inner())?;
    let (_config, _account, context) = configured_account(state.inner(), &account_id)?;
    let mods_directory = context.installation.game_directory.join("mods");
    if mods_directory.is_dir() {
        recover_audio_mod_replacements(&mods_directory)?;
    }
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
        return Err("生成目标不能同时作为源 Mod；请使用新名称生成后再替换".to_string());
    }
    let source_directory = source_mod_name
        .as_deref()
        .map(|name| mods_directory.join(name));
    if let Some(source) = source_directory.as_ref() {
        if !source.is_dir() {
            return Err(format!("未找到源 Mod：{}", source.display()));
        }
        if processing_manifest_path(source).is_some() {
            let source_name = source_mod_name.as_deref().unwrap_or_default();
            let validated = validate_audio_mod(mods_directory, source_name)
                .map_err(|error| format!("这个 D2RHub Mod 不能作为增量来源：{error}"))?;
            if !validated.current_feature_protocol {
                return Err(
                    "旧版 D2RHub Mod 可以继续运行，但不能安全增量加工；请改选原始 Mod 或 r22+ 产物"
                        .to_string(),
                );
            }
        }
    }
    Ok((source_mod_name, source_directory))
}

async fn run_audio_mod_generator(
    app: &tauri::AppHandle,
    invocation: GeneratorInvocation<'_>,
) -> Result<GeneratorReport, String> {
    let GeneratorInvocation {
        account_id,
        game_directory,
        output_directory,
        mod_name,
        source_directory,
        requested_features,
        progress_ceiling,
    } = invocation;
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
        "--features".to_string(),
        requested_features.generator_value().to_string(),
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
    requested_features: RequestedFeatureGroups,
) -> Result<ValidatedGeneratorOutput, String> {
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
    if report.feature_groups.is_empty() {
        return Err("生成器没有返回功能组清单".to_string());
    }
    validate_feature_group_entries(&report.feature_groups)
        .map_err(|error| format!("生成器报告无效：{error}"))?;
    requested_features.validate_present(&report.feature_groups)?;
    if report.mod_name != requested_mod_name {
        return Err("生成器返回的 Mod 名称与用户指定名称不一致".to_string());
    }
    let validated = validate_audio_mod(output_directory, &report.mod_name)?;
    if validated
        .recipe_version
        .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION)
        || !validated.current_feature_protocol
    {
        return Err("生成结果缺少当前配方版本，请重新安装 D2RHub 后重试".to_string());
    }
    if validated.recipe_version != Some(report.recipe_version) {
        return Err("生成器报告的配方版本与落盘清单不一致".to_string());
    }
    if validated.feature_groups != report.feature_groups {
        return Err("生成器报告的功能组与落盘清单不一致".to_string());
    }
    requested_features.validate_present(&validated.feature_groups)?;
    let reported_directory = std::fs::canonicalize(&report.mod_directory)
        .map_err(|error| format!("无法校验生成目录: {error}"))?;
    let validated_directory = std::fs::canonicalize(validated.directory)
        .map_err(|error| format!("无法校验识别 Mod: {error}"))?;
    if reported_directory != validated_directory {
        return Err("生成器返回的目录与实际输出不一致".to_string());
    }
    Ok(ValidatedGeneratorOutput {
        directory: validated_directory,
        feature_groups: validated.feature_groups,
    })
}

fn is_safe_relative_path(path: &Path) -> bool {
    path.components().next().is_some()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn replace_journal_paths_are_valid(
    mod_name: &str,
    staged_relative: &Path,
    backup_relative: &Path,
) -> bool {
    if !is_safe_relative_path(staged_relative)
        || !is_safe_relative_path(backup_relative)
        || staged_relative.components().count() != 2
        || backup_relative.components().count() != 1
        || staged_relative.file_name().and_then(|name| name.to_str()) != Some(mod_name)
    {
        return false;
    }
    let stage_parent_is_valid = staged_relative
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .is_some_and(|name| name.starts_with(".d2rhub-upgrade-stage-"));
    let backup_is_valid = backup_relative
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(".d2rhub-upgrade-backup-"));
    stage_parent_is_valid && backup_is_valid
}

fn write_replace_journal(
    mods_directory: &Path,
    mod_name: &str,
    staged_directory: &Path,
    backup_directory: &Path,
) -> Result<PathBuf, String> {
    let staged_relative = staged_directory
        .strip_prefix(mods_directory)
        .map_err(|_| "同名更新的暂存目录必须位于 mods 目录内".to_string())?
        .to_path_buf();
    let backup_relative = backup_directory
        .strip_prefix(mods_directory)
        .map_err(|_| "同名更新的备份目录必须位于 mods 目录内".to_string())?
        .to_path_buf();
    if !replace_journal_paths_are_valid(mod_name, &staged_relative, &backup_relative) {
        return Err("同名更新的事务目录无效".to_string());
    }

    let transaction_id = uuid::Uuid::new_v4().simple().to_string();
    let journal_path = mods_directory.join(format!(
        "{REPLACE_JOURNAL_PREFIX}{transaction_id}{REPLACE_JOURNAL_SUFFIX}"
    ));
    let temporary_path = mods_directory.join(format!(
        "{REPLACE_JOURNAL_PREFIX}{transaction_id}{REPLACE_JOURNAL_SUFFIX}.tmp"
    ));
    let journal = AudioModReplaceJournal {
        format_version: REPLACE_JOURNAL_FORMAT_VERSION,
        mod_name: mod_name.to_string(),
        staged_relative,
        backup_relative,
    };
    let bytes = serde_json::to_vec_pretty(&journal)
        .map_err(|error| format!("无法创建 Mod 更新事务记录：{error}"))?;
    let write_result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|error| format!("无法创建 Mod 更新事务记录：{error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("无法写入 Mod 更新事务记录：{error}"))?;
        file.sync_all()
            .map_err(|error| format!("无法持久化 Mod 更新事务记录：{error}"))?;
        std::fs::rename(&temporary_path, &journal_path)
            .map_err(|error| format!("无法提交 Mod 更新事务记录：{error}"))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    write_result.map(|()| journal_path)
}

fn cleanup_staged_directory(mods_directory: &Path, staged_directory: &Path) -> Result<(), String> {
    if !staged_directory.exists() {
        return Ok(());
    }
    let removable = staged_directory
        .parent()
        .filter(|parent| parent.parent() == Some(mods_directory))
        .filter(|parent| {
            parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".d2rhub-upgrade-stage-"))
        })
        .unwrap_or(staged_directory);
    std::fs::remove_dir_all(removable)
        .map_err(|error| format!("清理 Mod 更新暂存目录失败：{error}"))
}

pub(crate) fn recover_audio_mod_replacements(mods_directory: &Path) -> Result<(), String> {
    let mut journal_paths = std::fs::read_dir(mods_directory)
        .map_err(|error| format!("无法检查 Mod 更新事务：{error}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            (name.starts_with(REPLACE_JOURNAL_PREFIX) && name.ends_with(REPLACE_JOURNAL_SUFFIX))
                .then(|| entry.path())
        })
        .collect::<Vec<_>>();
    journal_paths.sort();

    for journal_path in journal_paths {
        let journal: AudioModReplaceJournal = serde_json::from_slice(
            &std::fs::read(&journal_path)
                .map_err(|error| format!("无法读取 Mod 更新事务记录：{error}"))?,
        )
        .map_err(|error| format!("Mod 更新事务记录已损坏：{error}"))?;
        let mod_name = plain_mod_name(&journal.mod_name)?;
        if journal.format_version != REPLACE_JOURNAL_FORMAT_VERSION
            || !replace_journal_paths_are_valid(
                mod_name,
                &journal.staged_relative,
                &journal.backup_relative,
            )
        {
            return Err("Mod 更新事务记录版本或路径无效，已停止自动恢复".to_string());
        }
        let target_directory = mods_directory.join(mod_name);
        let staged_directory = mods_directory.join(&journal.staged_relative);
        let backup_directory = mods_directory.join(&journal.backup_relative);

        if target_directory.exists() {
            let target_is_recoverable = validate_recoverable_audio_mod_directory(
                mods_directory,
                mod_name,
                &target_directory,
            )
            .is_ok();
            if !target_is_recoverable && backup_directory.exists() {
                validate_recoverable_audio_mod_directory(
                    mods_directory,
                    mod_name,
                    &backup_directory,
                )
                .map_err(|error| format!("新版与备份 Mod 都无法通过恢复校验：{error}"))?;
                let quarantine = mods_directory.join(format!(
                    ".d2rhub-upgrade-failed-{}-{}",
                    uuid::Uuid::new_v4().simple(),
                    mod_name
                ));
                std::fs::rename(&target_directory, &quarantine)
                    .map_err(|error| format!("隔离损坏的新版 Mod 失败：{error}"))?;
                if let Err(error) = std::fs::rename(&backup_directory, &target_directory) {
                    let _ = std::fs::rename(&quarantine, &target_directory);
                    return Err(format!("恢复旧版 Mod 失败：{error}"));
                }
            } else if !target_is_recoverable {
                return Err("更新事务中的 Mod 已损坏，且没有可恢复的备份".to_string());
            }
        } else if backup_directory.exists() {
            std::fs::rename(&backup_directory, &target_directory)
                .map_err(|error| format!("自动恢复旧版 Mod 失败：{error}"))?;
        } else if staged_directory.exists() {
            validate_audio_mod_directory(mods_directory, mod_name, staged_directory.clone())
                .map_err(|error| format!("更新暂存 Mod 无法恢复：{error}"))?;
            std::fs::rename(&staged_directory, &target_directory)
                .map_err(|error| format!("自动完成暂存 Mod 安装失败：{error}"))?;
        } else {
            return Err("Mod 更新事务缺少目标、备份与暂存目录，无法自动恢复".to_string());
        }

        validate_recoverable_audio_mod_directory(mods_directory, mod_name, &target_directory)
            .map_err(|error| format!("Mod 更新事务恢复后校验失败：{error}"))?;
        if backup_directory.exists() {
            std::fs::remove_dir_all(&backup_directory)
                .map_err(|error| format!("清理已恢复的 Mod 备份失败：{error}"))?;
        }
        cleanup_staged_directory(mods_directory, &staged_directory)?;
        std::fs::remove_file(&journal_path)
            .map_err(|error| format!("清理 Mod 更新事务记录失败：{error}"))?;
    }
    Ok(())
}

fn replace_audio_mod_directory(
    mods_directory: &Path,
    mod_name: &str,
    staged_directory: &Path,
    backup_directory: &Path,
) -> Result<(), String> {
    recover_audio_mod_replacements(mods_directory)?;
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

    let journal_path =
        write_replace_journal(mods_directory, mod_name, staged_directory, backup_directory)?;
    if let Err(error) = std::fs::rename(&target_directory, backup_directory) {
        let _ = std::fs::remove_file(&journal_path);
        return Err(format!("无法备份旧 Mod；请确认游戏已经关闭：{error}"));
    }
    match std::fs::rename(staged_directory, &target_directory) {
        Ok(()) => {
            // 新目录已完成同盘原子切换，旧目录仅在切换成功后清理。
            if std::fs::remove_dir_all(backup_directory).is_ok() {
                let _ = std::fs::remove_file(journal_path);
            }
            Ok(())
        }
        Err(install_error) => match std::fs::rename(backup_directory, &target_directory) {
            Ok(()) => {
                let _ = std::fs::remove_file(journal_path);
                Err(format!("安装新版 Mod 失败，已恢复旧版：{install_error}"))
            }
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
    include_audio_telemetry: Option<bool>,
    include_room_tools: Option<bool>,
) -> Result<AudioModPrepareResult, String> {
    let shared_state = state.inner().clone();
    let _lease = BuildLease::acquire(&shared_state)?;
    let (_config, _account, context) = configured_account(&shared_state, &account_id)?;
    let game_directory = context.installation.game_directory;
    let mods_directory = game_directory.join("mods");
    std::fs::create_dir_all(&mods_directory)
        .map_err(|error| format!("创建 mods 目录失败: {error}"))?;
    recover_audio_mod_replacements(&mods_directory)?;

    let mod_name = generated_audio_mod_name(&mod_name)?.to_string();
    if let Some(existing_name) = find_existing_mod_name(&mods_directory, &mod_name)? {
        return Err(format!("Mod 名称“{existing_name}”已存在，请换一个名称"));
    }

    let (source_mod_name, source_directory) =
        resolve_source_directory(&mods_directory, &mod_name, source_mod_name)?;
    let requested_features =
        RequestedFeatureGroups::from_options(include_audio_telemetry, include_room_tools)?;

    emit_prepare_progress(&app, &account_id, "starting", 1, "正在开始准备…");
    let report = run_audio_mod_generator(
        &app,
        GeneratorInvocation {
            account_id: &account_id,
            game_directory: &game_directory,
            output_directory: &mods_directory,
            mod_name: &mod_name,
            source_directory: source_directory.as_deref(),
            requested_features,
            progress_ceiling: 100,
        },
    )
    .await?;
    let generated =
        validate_generator_output(&mods_directory, &mod_name, &report, requested_features)?;
    emit_prepare_progress(&app, &account_id, "complete", 100, "识别 Mod 已准备完成");
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
pub async fn upgrade_audio_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    source_mod_name: Option<String>,
    include_audio_telemetry: Option<bool>,
    include_room_tools: Option<bool>,
) -> Result<AudioModSetupState, String> {
    let shared_state = state.inner().clone();
    let _lease = BuildLease::acquire(&shared_state)?;
    let (config, account, context) = configured_account(&shared_state, &account_id)?;
    let mods_directory = context.installation.game_directory.join("mods");
    recover_audio_mod_replacements(&mods_directory)?;
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
    let requested_features =
        RequestedFeatureGroups::from_options(include_audio_telemetry, include_room_tools)?;

    emit_prepare_progress(&app, &account_id, "starting", 1, "正在生成同名新版 Mod…");
    let temporary_output = TemporaryDirectory::create(std::env::temp_dir().join(format!(
        "d2rhub-audio-upgrade-output-{}",
        uuid::Uuid::new_v4()
    )))?;
    let report = run_audio_mod_generator(
        &app,
        GeneratorInvocation {
            account_id: &account_id,
            game_directory: &context.installation.game_directory,
            output_directory: temporary_output.path(),
            mod_name,
            source_directory: source_directory.as_deref(),
            requested_features,
            progress_ceiling: 85,
        },
    )
    .await?;
    let generated = validate_generator_output(
        temporary_output.path(),
        mod_name,
        &report,
        requested_features,
    )?;

    emit_prepare_progress(&app, &account_id, "staging", 90, "正在校验并暂存新版 Mod…");
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
    requested_features.validate_present(&staged.feature_groups)?;
    if staged.feature_groups != generated.feature_groups {
        return Err("暂存 Mod 的功能组与已验证生成结果不一致，旧版未被修改".to_string());
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
        || !installed.current_feature_protocol
        || installed.feature_groups != generated.feature_groups
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
    let _lease = BuildLease::acquire(state.inner())?;
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
        generated_audio_mod_name, has_txt_argument, installed_mods, recover_audio_mod_replacements,
        replace_audio_mod_directory, replace_journal_paths_are_valid, resolve_source_directory,
        validate_generator_output, write_replace_journal, GeneratorFeatureGroup, GeneratorReport,
        RequestedFeatureGroups, AREA_CATALOG_FILE_NAME, AUDIO_TELEMETRY_FEATURE_ID,
        IN_GAME_ROOM_TOOLS_FEATURE_ID, ITEM_CATALOG_FILE_NAME, PROTOCOL_VERSION,
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

    fn current_audio_manifest(mod_name: &str) -> serde_json::Value {
        serde_json::json!({
            "manifest_format": "d2r-audio-telemetry-mod",
            "producer": "d2r-audio-mod",
            "protocol_version": PROTOCOL_VERSION,
            "recipe_version": REQUIRED_AUDIO_MOD_RECIPE_VERSION,
            "build_mode": "minimal",
            "mod_name": mod_name,
            "feature_groups": [{
                "id": AUDIO_TELEMETRY_FEATURE_ID,
                "recipe_version": 1,
                "fingerprint": "audio-v1;fixture=true"
            }]
        })
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
    fn omitted_feature_flags_keep_the_legacy_audio_command_contract() {
        let requested = RequestedFeatureGroups::from_options(None, None).unwrap();
        assert!(requested.audio_telemetry);
        assert!(!requested.room_tools);
        assert_eq!(requested.generator_value(), "audio");
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
    fn replacement_journal_paths_cannot_alias_the_target_or_escape_the_transaction_layout() {
        assert!(replace_journal_paths_are_valid(
            "safe-mod",
            std::path::Path::new(".d2rhub-upgrade-stage-test/safe-mod"),
            std::path::Path::new(".d2rhub-upgrade-backup-test"),
        ));
        assert!(!replace_journal_paths_are_valid(
            "safe-mod",
            std::path::Path::new("safe-mod"),
            std::path::Path::new("safe-mod"),
        ));
        assert!(!replace_journal_paths_are_valid(
            "safe-mod",
            std::path::Path::new("../outside/safe-mod"),
            std::path::Path::new(".d2rhub-upgrade-backup-test"),
        ));
    }

    #[test]
    fn same_name_update_switches_only_after_staged_mod_is_ready() {
        let root = test_mods_directory("same_name_upgrade");
        let mods = root.join("mods");
        let staging = mods.join(".d2rhub-upgrade-stage-test");
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
        write_test_audio_mod(&staging, "jcy-tz", current_audio_manifest("jcy-tz"));
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
    fn legacy_generated_mod_runs_but_is_not_an_additive_source() {
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

        let runtime = compatibility(&root, "-mod old-audio -txt");
        assert!(runtime.ready);
        assert!(runtime.update_required);
        assert!(error.contains("不能安全增量加工"));
        assert!(root.join("old-audio").is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_current_feature_mod_is_an_additive_source() {
        let root = test_mods_directory("current_generated_source");
        write_test_audio_mod(&root, "audio-r22", current_audio_manifest("audio-r22"));

        let (source_name, source_directory) =
            resolve_source_directory(&root, "audio-plus-rooms", Some("audio-r22".to_string()))
                .unwrap();

        let expected_source = root.join("audio-r22");
        assert_eq!(source_name.as_deref(), Some("audio-r22"));
        assert_eq!(source_directory.as_deref(), Some(expected_source.as_path()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_or_unfingerprinted_groups_are_not_trusted_as_sources() {
        let root = test_mods_directory("invalid_feature_sources");
        for (name, groups) in [
            (
                "duplicate",
                serde_json::json!([
                    {"id": AUDIO_TELEMETRY_FEATURE_ID, "recipe_version": 1, "fingerprint": "one"},
                    {"id": AUDIO_TELEMETRY_FEATURE_ID, "recipe_version": 1, "fingerprint": "two"}
                ]),
            ),
            (
                "empty-fingerprint",
                serde_json::json!([
                    {"id": AUDIO_TELEMETRY_FEATURE_ID, "recipe_version": 1, "fingerprint": ""}
                ]),
            ),
        ] {
            let mut manifest = current_audio_manifest(name);
            manifest["feature_groups"] = groups;
            write_test_audio_mod(&root, name, manifest);
            assert!(resolve_source_directory(&root, "next", Some(name.to_string())).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generator_report_must_match_requested_and_persisted_groups() {
        let root = test_mods_directory("generator_groups");
        let mod_name = "generated";
        write_test_audio_mod(&root, mod_name, current_audio_manifest(mod_name));
        let audio_group = GeneratorFeatureGroup {
            id: AUDIO_TELEMETRY_FEATURE_ID.to_string(),
            recipe_version: 1,
            fingerprint: "audio-v1;fixture=true".to_string(),
            reused_from_source: false,
        };
        let mut report = GeneratorReport {
            protocol_version: PROTOCOL_VERSION,
            recipe_version: REQUIRED_AUDIO_MOD_RECIPE_VERSION,
            mod_name: mod_name.to_string(),
            mod_directory: root.join(mod_name).to_string_lossy().into_owned(),
            feature_groups: vec![audio_group.clone()],
        };

        let actual = validate_generator_output(
            &root,
            mod_name,
            &report,
            RequestedFeatureGroups {
                audio_telemetry: true,
                room_tools: false,
            },
        )
        .unwrap();
        assert_eq!(actual.feature_groups, vec![audio_group]);

        assert!(validate_generator_output(
            &root,
            mod_name,
            &report,
            RequestedFeatureGroups {
                audio_telemetry: true,
                room_tools: true,
            },
        )
        .unwrap_err()
        .contains("局内房间工具"));

        report.feature_groups.push(GeneratorFeatureGroup {
            id: IN_GAME_ROOM_TOOLS_FEATURE_ID.to_string(),
            recipe_version: 19,
            fingerprint: "room-tools-v19".to_string(),
            reused_from_source: false,
        });
        assert!(validate_generator_output(
            &root,
            mod_name,
            &report,
            RequestedFeatureGroups {
                audio_telemetry: true,
                room_tools: true,
            },
        )
        .unwrap_err()
        .contains("落盘清单不一致"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn final_persisted_groups_control_audio_readiness() {
        let root = test_mods_directory("room_only");
        let name = "room-only";
        let mut manifest = current_audio_manifest(name);
        manifest["feature_groups"] = serde_json::json!([{
            "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
            "recipe_version": 19,
            "fingerprint": "room-tools-v19"
        }]);
        write_test_audio_mod(&root, name, manifest);
        std::fs::remove_file(root.join(name).join(AREA_CATALOG_FILE_NAME)).unwrap();
        std::fs::remove_file(root.join(name).join(ITEM_CATALOG_FILE_NAME)).unwrap();

        let state = compatibility(&root, &format!("-mod {name} -txt"));
        assert!(!state.ready);
        assert_eq!(state.reason_code, "missing_audio_feature");
        let listed = installed_mods(&root);
        assert_eq!(
            listed[0].feature_groups,
            vec![IN_GAME_ROOM_TOOLS_FEATURE_ID]
        );
        assert!(!listed[0].audio_ready);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_directory_switch_restores_the_old_mod() {
        let root = test_mods_directory("replace_recovery_rollback");
        let mods = root.join("mods");
        let stage_parent = mods.join(".d2rhub-upgrade-stage-recovery");
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(".d2rhub-upgrade-backup-recovery");
        std::fs::create_dir_all(&mods).unwrap();
        write_test_audio_mod(
            &mods,
            "recover-me",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION - 1,
                "build_mode": "minimal",
                "mod_name": "recover-me"
            }),
        );
        std::fs::write(mods.join("recover-me").join("old-marker.txt"), b"old").unwrap();
        write_test_audio_mod(
            &stage_parent,
            "recover-me",
            current_audio_manifest("recover-me"),
        );
        std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();
        let journal = write_replace_journal(&mods, "recover-me", &staged, &backup).unwrap();
        std::fs::rename(mods.join("recover-me"), &backup).unwrap();

        recover_audio_mod_replacements(&mods).unwrap();

        assert!(mods.join("recover-me").join("old-marker.txt").is_file());
        assert!(!mods.join("recover-me").join("new-marker.txt").exists());
        assert!(!backup.exists());
        assert!(!journal.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_directory_switch_commits_the_valid_new_mod() {
        let root = test_mods_directory("replace_recovery_commit");
        let mods = root.join("mods");
        let stage_parent = mods.join(".d2rhub-upgrade-stage-recovery");
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(".d2rhub-upgrade-backup-recovery");
        std::fs::create_dir_all(&mods).unwrap();
        write_test_audio_mod(
            &mods,
            "recover-me",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "build_mode": "minimal",
                "mod_name": "recover-me"
            }),
        );
        write_test_audio_mod(
            &stage_parent,
            "recover-me",
            current_audio_manifest("recover-me"),
        );
        std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();
        let journal = write_replace_journal(&mods, "recover-me", &staged, &backup).unwrap();
        std::fs::rename(mods.join("recover-me"), &backup).unwrap();
        std::fs::rename(&staged, mods.join("recover-me")).unwrap();

        recover_audio_mod_replacements(&mods).unwrap();

        assert!(mods.join("recover-me").join("new-marker.txt").is_file());
        assert!(!backup.exists());
        assert!(!journal.exists());
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
                    "mod_name": name,
                    "feature_groups": [{
                        "id": AUDIO_TELEMETRY_FEATURE_ID,
                        "recipe_version": 1,
                        "fingerprint": "audio-v1;fixture=true"
                    }]
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
