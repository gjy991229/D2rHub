use crate::application::task_runtime::{TaskHandle, TaskRequest};
use crate::commands::account::{update_account_mods_inner, AccountManager, AccountMeta};
use crate::commands::launch::parse_windows_command_line;
use crate::domain::config::GlobalConfig;
use crate::infrastructure::durable_fs;
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
const REQUIRED_AUDIO_MOD_RECIPE_VERSION: u32 = 25;
const FEATURE_GROUP_PROTOCOL_RECIPE_VERSION: u32 = 22;
const AUDIO_TELEMETRY_FEATURE_ID: &str = "audio_telemetry";
const AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION: u32 = 3;
const IN_GAME_ROOM_TOOLS_FEATURE_ID: &str = "in_game_room_tools";
const IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION: u32 = 23;
const PREVIOUS_IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSIONS: [u32; 2] = [21, 22];
const AUTO_EXIT_ON_DEATH_FEATURE_ID: &str = "auto_exit_on_death";
const AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION: u32 = 1;
const AUTO_EXIT_ON_DEATH_FINGERPRINT: &str = "auto-exit-on-death-v1;trigger_ms=10;commit_ms=100";
const AUTO_EXIT_ON_DEATH_LEGACY_ENABLED_FINGERPRINT: &str =
    "auto-exit-on-death-v1;trigger_ms=10;commit_ms=100;enabled=1";
const AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT: &str =
    "auto-exit-on-death-v1;trigger_ms=10;commit_ms=100;enabled=0";
const ROOM_TOOL_LAYOUT_DIRECTORY: &str = "data/global/ui/layouts";
const ROOM_TOOLBAR_OPEN_MESSAGE: &str = "PanelManager:OpenPanel:D2RHubRoomToolbar";
const ROOM_TOOLBAR_CLOSE_MESSAGE: &str = "PanelManager:ClosePanel:D2RHubRoomToolbar";
const ROOM_TOOL_GATEWAY_HUB: &str = "D2RHubKeyboardGatewayHub";
const ROOM_TOOL_CREATE_GATEWAY: &str = "D2RHubKeyboardCreateGateway";
const ROOM_TOOL_JOIN_GATEWAY: &str = "D2RHubKeyboardJoinGateway";
const AUTO_EXIT_ON_DEATH_PANEL: &str = "D2RHubAutoExitOnDeath";
const NEXT_GAME_TOOLTIP_OFFSET_Y: i64 = 267;
const ROOM_TOOL_BUTTON_SCALE: f64 = 0.30;
const ROOM_TOOL_BUTTON_Y: i64 = 12;
const ROOM_TOOL_NEXT_X: i64 = -1_040;
const ROOM_TOOL_CREATE_X: i64 = -760;
const ROOM_TOOL_JOIN_X: i64 = -480;
const QUICK_RECREATE_DOUBLE_CLICK_WINDOW_SECONDS: f64 = 0.5;
const ROOM_TRANSITION_OPEN_PAUSE_DELAY_SECONDS: f64 = 0.01;
const ROOM_TRANSITION_COMMIT_DELAY_SECONDS: f64 = 0.05;
const REPLACE_JOURNAL_FORMAT_VERSION: u8 = 1;
const REPLACE_JOURNAL_PREFIX: &str = ".d2rhub-audio-replace-";
const REPLACE_JOURNAL_SUFFIX: &str = ".json";

#[derive(Debug, Clone, Serialize)]
pub struct InstalledMod {
    pub name: String,
    pub source_mod_name: Option<String>,
    pub audio_ready: bool,
    pub update_required: bool,
    pub source_eligible: bool,
    pub feature_groups: Vec<String>,
    pub audio_reusable: bool,
    pub auto_exit_on_death_enabled: bool,
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
    auto_exit_on_death_enabled: bool,
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
    auto_exit_on_death: bool,
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
        auto_exit_on_death: Option<bool>,
    ) -> Result<Self, String> {
        // Missing fields preserve the pre-r22 command contract used by older D2RHub frontends.
        let requested = Self {
            audio_telemetry: audio_telemetry.unwrap_or(true),
            room_tools: room_tools.unwrap_or(false),
            auto_exit_on_death: auto_exit_on_death.unwrap_or(false),
        };
        if !requested.audio_telemetry && !requested.room_tools && !requested.auto_exit_on_death {
            return Err("请至少选择一个要加工的功能".to_string());
        }
        Ok(requested)
    }

    fn generator_value(self) -> String {
        let mut features = Vec::new();
        if self.audio_telemetry {
            features.push("audio");
        }
        if self.room_tools {
            features.push("rooms");
        }
        if self.auto_exit_on_death {
            features.push("death-exit");
        }
        features.join(",")
    }

    fn validate_present(self, groups: &[GeneratorFeatureGroup]) -> Result<(), String> {
        for (requested, id, label) in [
            (self.audio_telemetry, AUDIO_TELEMETRY_FEATURE_ID, "声纹识别"),
            (
                self.room_tools,
                IN_GAME_ROOM_TOOLS_FEATURE_ID,
                "局内房间工具",
            ),
            (
                self.auto_exit_on_death,
                AUTO_EXIT_ON_DEATH_FEATURE_ID,
                "死亡后自动退出",
            ),
        ] {
            if requested {
                let group = groups
                    .iter()
                    .find(|group| group.id == id)
                    .ok_or_else(|| format!("生成结果缺少已选择的{label}功能组"))?;
                validate_supported_feature_group(group)
                    .map_err(|error| format!("生成结果中的{label}功能组无效：{error}"))?;
            }
        }
        Ok(())
    }

    fn include_existing_known(mut self, groups: &[GeneratorFeatureGroup]) -> Self {
        self.audio_telemetry |= groups
            .iter()
            .any(|group| group.id == AUDIO_TELEMETRY_FEATURE_ID);
        self.room_tools |= groups
            .iter()
            .any(|group| group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID);
        self.auto_exit_on_death |= groups
            .iter()
            .any(|group| group.id == AUTO_EXIT_ON_DEATH_FEATURE_ID);
        self
    }

    fn all_present(self, groups: &[GeneratorFeatureGroup]) -> bool {
        (!self.audio_telemetry
            || groups
                .iter()
                .any(|group| group.id == AUDIO_TELEMETRY_FEATURE_ID))
            && (!self.room_tools
                || groups
                    .iter()
                    .any(|group| group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID))
            && (!self.auto_exit_on_death
                || groups
                    .iter()
                    .any(|group| group.id == AUTO_EXIT_ON_DEATH_FEATURE_ID))
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct AudioModReplaceJournal {
    format_version: u8,
    mod_name: String,
    staged_relative: PathBuf,
    backup_relative: PathBuf,
    #[serde(default)]
    required_feature_groups: Vec<GeneratorFeatureGroup>,
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

pub(crate) fn active_mod_name(mod_args: &str) -> Result<Option<String>, String> {
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
    validate_feature_group_metadata(&groups)?;
    Ok(groups)
}

fn validate_feature_group_metadata(groups: &[GeneratorFeatureGroup]) -> Result<(), String> {
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

fn validate_feature_group_entries(groups: &[GeneratorFeatureGroup]) -> Result<(), String> {
    validate_feature_group_metadata(groups)?;
    for group in groups {
        validate_supported_feature_group(group)?;
    }
    Ok(())
}

fn validate_upgrade_source_feature_group_entries(
    groups: &[GeneratorFeatureGroup],
) -> Result<(), String> {
    validate_feature_group_metadata(groups)?;
    for group in groups {
        if group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID
            && PREVIOUS_IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSIONS.contains(&group.recipe_version)
        {
            if group.fingerprint != format!("room-tools-v{}", group.recipe_version) {
                return Err("上一版局内房间工具指纹无效，不能作为原位升级来源".to_string());
            }
        } else {
            validate_supported_feature_group(group)?;
        }
    }
    Ok(())
}

fn validate_preserved_feature_groups(
    existing: &[GeneratorFeatureGroup],
    candidate: &[GeneratorFeatureGroup],
) -> Result<(), String> {
    for required in existing {
        let preserved = candidate.iter().any(|actual| {
            actual.id == required.id
                && actual.recipe_version == required.recipe_version
                && (actual.fingerprint == required.fingerprint
                    // Normalize the short-lived stateful r1 fingerprints once. This is a metadata
                    // migration during additive processing, not an activation toggle.
                    || (required.id == AUTO_EXIT_ON_DEATH_FEATURE_ID
                        && actual.fingerprint == AUTO_EXIT_ON_DEATH_FINGERPRINT
                        && matches!(
                            required.fingerprint.as_str(),
                            AUTO_EXIT_ON_DEATH_LEGACY_ENABLED_FINGERPRINT
                                | AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT
                        )))
        });
        if !preserved {
            return Err(format!(
                "生成结果未无损保留现有功能组“{}”（r{}）；为避免删除未来版本数据，已停止原位更新",
                required.id, required.recipe_version
            ));
        }
    }
    Ok(())
}

fn validate_supported_feature_group(group: &GeneratorFeatureGroup) -> Result<(), String> {
    match group.id.as_str() {
        AUDIO_TELEMETRY_FEATURE_ID => {
            if group.recipe_version != AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION {
                return Err(format!(
                    "D2RHub Mod 的声纹识别功能组配方 r{} 不受支持（需要 r{AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION}）",
                    group.recipe_version
                ));
            }
            validate_audio_feature_fingerprint(&group.fingerprint)
        }
        IN_GAME_ROOM_TOOLS_FEATURE_ID => {
            if group.recipe_version != IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION {
                return Err(format!(
                    "D2RHub Mod 的局内房间工具配方 r{} 不受支持（需要 r{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}）",
                    group.recipe_version
                ));
            }
            let expected = format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}");
            if group.fingerprint != expected {
                return Err("D2RHub Mod 的局内房间工具指纹无效，请重新加工".to_string());
            }
            Ok(())
        }
        AUTO_EXIT_ON_DEATH_FEATURE_ID => validate_auto_exit_on_death_feature_group(group),
        // Unknown groups are intentionally preserved and accepted. Their owner is responsible for
        // interpreting the recipe and fingerprint once D2RHub learns that feature.
        _ => Ok(()),
    }
}

fn validate_auto_exit_on_death_feature_group(group: &GeneratorFeatureGroup) -> Result<(), String> {
    if group.recipe_version != AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION {
        return Err(format!(
            "D2RHub Mod 的死亡后自动退出配方 r{} 不受支持（需要 r{AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION}）",
            group.recipe_version
        ));
    }
    match group.fingerprint.as_str() {
        AUTO_EXIT_ON_DEATH_FINGERPRINT
        | AUTO_EXIT_ON_DEATH_LEGACY_ENABLED_FINGERPRINT
        | AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT => Ok(()),
        _ => Err("D2RHub Mod 的死亡后自动退出指纹无效，请重新加工".to_string()),
    }
}

fn validate_audio_feature_fingerprint(fingerprint: &str) -> Result<(), String> {
    let parts = fingerprint.split(';').collect::<Vec<_>>();
    let expected_protocol = format!("protocol={PROTOCOL_VERSION}");
    if parts.len() != 5
        || parts[0] != format!("audio-v{AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION}")
        || parts[1] != expected_protocol
        || !matches!(parts[2], "areas=countess_route" | "areas=all_areas")
        || !parts[3].starts_with("track=")
        || !parts[4].starts_with("gain_mdb=")
    {
        return Err("D2RHub Mod 的声纹识别功能组指纹无效，请重新加工".to_string());
    }

    let categories = parts[3].trim_start_matches("track=");
    let supported = [
        "charms", "essences", "gems", "jewels", "keys", "organs", "runes",
    ];
    let requested = if categories.is_empty() {
        Vec::new()
    } else {
        categories.split(',').collect::<Vec<_>>()
    };
    if requested.windows(2).any(|pair| pair[0] >= pair[1])
        || requested
            .iter()
            .any(|category| !supported.contains(category))
        || parts[4]
            .trim_start_matches("gain_mdb=")
            .parse::<i32>()
            .is_err()
    {
        return Err("D2RHub Mod 的声纹识别功能组指纹无效，请重新加工".to_string());
    }
    Ok(())
}

/// Fast, read-only trust check used by settings discovery.
///
/// The signed-by-construction manifest identity, recipe versions and feature
/// fingerprints are enough to render setup state. Expensive recursive tree and
/// generated-asset verification remains mandatory for generation, replacement,
/// recovery, account application, and explicit compatibility checks. Room
/// shortcuts deliberately do not invoke either validation path.
fn validate_audio_mod_credential(
    mods_directory: &Path,
    mod_name: &str,
) -> Result<ValidatedAudioMod, String> {
    const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
    let mod_name = plain_mod_name(mod_name)?;
    let mod_directory = mods_directory.join(mod_name);
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    ensure_safe_existing_node(&canonical_mods, &mod_directory, true, "Mod 目录")?;
    ensure_safe_existing_node(
        &canonical_mods,
        &mod_directory.join(format!("{mod_name}.mpq")),
        true,
        "Mod MPQ 目录",
    )?;
    let manifest_path = processing_manifest_path(&mod_directory)
        .ok_or_else(|| "这个 Mod 未经过 D2RHub 加工".to_string())?;
    ensure_safe_existing_node(&canonical_mods, &manifest_path, false, "D2RHub 加工凭证")?;
    let manifest_metadata = std::fs::metadata(&manifest_path)
        .map_err(|error| format!("无法检查 D2RHub 加工凭证：{error}"))?;
    if manifest_metadata.len() > MAX_MANIFEST_BYTES {
        return Err("D2RHub 加工凭证超过大小限制".to_string());
    }
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|error| format!("无法读取 D2RHub 加工凭证：{error}"))?,
    )
    .map_err(|_| "D2RHub 加工凭证已损坏，请重新加工".to_string())?;
    let protocol = manifest
        .get("protocol_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "D2RHub 加工凭证缺少协议版本".to_string())?;
    if protocol != u64::from(PROTOCOL_VERSION) {
        return Err(format!(
            "识别 Mod 协议版本不匹配（需要 v{PROTOCOL_VERSION}）"
        ));
    }
    match manifest.get("manifest_format") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(format)) if format == MANIFEST_FORMAT => {}
        _ => return Err("D2RHub 加工凭证类型无效".to_string()),
    }
    match manifest.get("producer") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(producer)) if producer == PRODUCER_NAME => {}
        _ => return Err("D2RHub 加工凭证生成器无效".to_string()),
    }
    match manifest.get("mod_name") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(recorded)) if recorded == mod_name => {}
        _ => return Err("D2RHub 加工凭证名称与 Mod 不匹配".to_string()),
    }
    let recipe_version = match manifest.get("recipe_version") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|version| u32::try_from(version).ok())
                .ok_or_else(|| "D2RHub 加工凭证配方版本无效".to_string())?,
        ),
    };
    let parsed_feature_groups = parse_feature_groups(&manifest)?;
    let has_feature_group_protocol = recipe_version
        .is_some_and(|version| version >= FEATURE_GROUP_PROTOCOL_RECIPE_VERSION)
        && !parsed_feature_groups.is_empty();
    let current_feature_protocol = recipe_version
        .is_some_and(|version| version >= REQUIRED_AUDIO_MOD_RECIPE_VERSION)
        && has_feature_group_protocol;
    let has_current_identity = manifest
        .get("manifest_format")
        .and_then(serde_json::Value::as_str)
        == Some(MANIFEST_FORMAT)
        && manifest.get("producer").and_then(serde_json::Value::as_str) == Some(PRODUCER_NAME)
        && manifest.get("mod_name").and_then(serde_json::Value::as_str) == Some(mod_name);
    if has_feature_group_protocol && !has_current_identity {
        return Err("功能组凭证缺少完整的生成器身份".to_string());
    }
    if current_feature_protocol {
        validate_feature_group_entries(&parsed_feature_groups)?;
    }
    let feature_groups = if has_feature_group_protocol {
        parsed_feature_groups
    } else {
        Vec::new()
    };
    let has_audio_telemetry = if has_feature_group_protocol {
        feature_groups
            .iter()
            .any(|group| group.id == AUDIO_TELEMETRY_FEATURE_ID)
    } else {
        true
    };
    let auto_exit_on_death_active = if current_feature_protocol
        && feature_groups
            .iter()
            .any(|group| group.id == AUTO_EXIT_ON_DEATH_FEATURE_ID)
    {
        auto_exit_on_death_layout_enabled(&mod_directory, mod_name)?
    } else {
        false
    };
    let build_mode = manifest
        .get("build_mode")
        .and_then(serde_json::Value::as_str)
        .filter(|value| matches!(*value, "minimal" | "augment"))
        .map(str::to_string);
    let source_mod_name = source_mod_name_from_manifest(&manifest, mods_directory);
    Ok(ValidatedAudioMod {
        directory: mod_directory,
        recipe_version,
        build_mode,
        source_mod_name,
        feature_groups,
        has_audio_telemetry,
        auto_exit_on_death_enabled: auto_exit_on_death_active,
        current_feature_protocol,
    })
}

fn read_room_tool_layout(layout_directory: &Path, name: &str) -> Result<serde_json::Value, String> {
    let path = layout_directory.join(name);
    let bytes = std::fs::read(&path).map_err(|_| format!("局内房间工具缺少布局文件：{name}"))?;
    serde_json::from_slice(&bytes).map_err(|_| format!("局内房间工具布局已损坏：{name}"))
}

fn layout_has_child_message(document: &serde_json::Value, field: &str, expected: &str) -> bool {
    document
        .get("children")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|children| {
            children.iter().any(|child| {
                child
                    .get("fields")
                    .and_then(|fields| fields.get(field))
                    .and_then(serde_json::Value::as_str)
                    == Some(expected)
            })
        })
}

fn room_toolbar_visibility(hud: &serde_json::Value) -> Result<bool, String> {
    match (
        layout_has_child_message(hud, "message", ROOM_TOOLBAR_OPEN_MESSAGE),
        layout_has_child_message(hud, "message", ROOM_TOOLBAR_CLOSE_MESSAGE),
    ) {
        (true, false) => Ok(true),
        (false, true) => Ok(false),
        _ => Err("局内按钮的 HUD 显示配置缺失或冲突，请重新加工".to_string()),
    }
}

pub(crate) fn read_room_toolbar_visible(
    mods_directory: &Path,
    mod_name: &str,
) -> Result<bool, String> {
    let mod_name = plain_mod_name(mod_name)?;
    let layout_path = mods_directory
        .join(mod_name)
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY)
        .join("HudWarningshd.json");
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    ensure_safe_existing_node(&canonical_mods, &layout_path, false, "游戏 HUD 布局")?;
    let document = read_room_tool_layout(
        layout_path
            .parent()
            .ok_or_else(|| "游戏 HUD 布局路径无效".to_string())?,
        "HudWarningshd.json",
    )?;
    room_toolbar_visibility(&document)
}

fn layout_has_timed_child_message(
    document: &serde_json::Value,
    expected: &str,
    expected_time: f64,
) -> bool {
    document
        .get("children")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|children| {
            children.iter().any(|child| {
                let fields = child.get("fields");
                fields
                    .and_then(|value| value.get("message"))
                    .and_then(serde_json::Value::as_str)
                    == Some(expected)
                    && fields
                        .and_then(|value| value.get("time"))
                        .and_then(serde_json::Value::as_f64)
                        .is_some_and(|time| (time - expected_time).abs() < f64::EPSILON)
            })
        })
}

fn layout_field_value_count(document: &serde_json::Value, expected: &str) -> usize {
    let own = document
        .get("fields")
        .and_then(serde_json::Value::as_object)
        .map_or(0, |fields| {
            fields
                .values()
                .filter(|value| value.as_str() == Some(expected))
                .count()
        });
    own + document
        .get("children")
        .and_then(serde_json::Value::as_array)
        .map_or(0, |children| {
            children
                .iter()
                .map(|child| layout_field_value_count(child, expected))
                .sum()
        })
}

fn find_layout_node<'a>(
    document: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    if document.get("name").and_then(serde_json::Value::as_str) == Some(name) {
        return Some(document);
    }
    document
        .get("children")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find_map(|child| find_layout_node(child, name))
}

fn validate_routed_pause_buttons(
    node: &serde_json::Value,
    pause_name: &str,
) -> Result<usize, String> {
    let mut routed = 0;
    if node.get("type").and_then(serde_json::Value::as_str) == Some("ButtonWidget") {
        let name = node.get("name").and_then(serde_json::Value::as_str);
        let is_gateway = name.is_some_and(|name| {
            [
                ROOM_TOOL_GATEWAY_HUB,
                ROOM_TOOL_CREATE_GATEWAY,
                ROOM_TOOL_JOIN_GATEWAY,
            ]
            .contains(&name)
        });
        if !is_gateway {
            if node
                .pointer("/fields/navigation/left/name")
                .and_then(serde_json::Value::as_str)
                != Some(ROOM_TOOL_CREATE_GATEWAY)
                || node
                    .pointer("/fields/navigation/right/name")
                    .and_then(serde_json::Value::as_str)
                    != Some(ROOM_TOOL_JOIN_GATEWAY)
            {
                return Err(format!(
                    "暂停布局按钮未连接安全键盘入口：{pause_name}/{}",
                    name.unwrap_or("<unnamed>")
                ));
            }
            routed += 1;
        }
    }

    if let Some(children) = node.get("children") {
        let children = children
            .as_array()
            .ok_or_else(|| format!("暂停布局 children 不是数组：{pause_name}"))?;
        for child in children {
            routed += validate_routed_pause_buttons(child, pause_name)?;
        }
    }
    Ok(routed)
}

fn read_auto_exit_on_death_layout(
    layout_directory: &Path,
    name: &str,
) -> Result<serde_json::Value, String> {
    let path = layout_directory.join(name);
    let bytes = std::fs::read(&path).map_err(|_| format!("死亡后自动退出缺少布局文件：{name}"))?;
    serde_json::from_slice(&bytes).map_err(|_| format!("死亡后自动退出布局已损坏：{name}"))
}

fn auto_exit_on_death_layout_enabled(mod_directory: &Path, mod_name: &str) -> Result<bool, String> {
    let layout_directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let death_modal = read_auto_exit_on_death_layout(&layout_directory, "youdiedmodalhd.json")?;
    let launcher = find_layout_node(&death_modal, "D2RHubAutoExitOnDeathLauncher");
    let launcher_is_valid = launcher.is_some_and(|launcher| {
        launcher.get("type").and_then(serde_json::Value::as_str) == Some("TimerWidget")
            && launcher
                .pointer("/fields/message")
                .and_then(serde_json::Value::as_str)
                == Some("PanelManager:OpenPanel:D2RHubAutoExitOnDeath")
            && launcher
                .pointer("/fields/time")
                .and_then(serde_json::Value::as_f64)
                .is_some_and(|time| (time - 0.01).abs() < f64::EPSILON)
    });
    if launcher.is_some() && !launcher_is_valid {
        return Err("死亡界面的自动退出入口无效".to_string());
    }
    let has_legacy_exit_timer = death_modal
        .get("children")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|children| {
            children.iter().any(|child| {
                child.get("type").and_then(serde_json::Value::as_str) == Some("TimerWidget")
                    && child
                        .pointer("/fields/message")
                        .and_then(serde_json::Value::as_str)
                        == Some("PanelManager:OpenPanel:exitgame")
            })
        });
    if has_legacy_exit_timer {
        return Err("死亡界面仍包含未规范化的原生 exitgame 定时入口".to_string());
    }
    Ok(launcher_is_valid)
}

fn validate_auto_exit_on_death_layouts(
    mod_directory: &Path,
    mod_name: &str,
    enabled: bool,
) -> Result<(), String> {
    let layout_directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    if auto_exit_on_death_layout_enabled(mod_directory, mod_name)? != enabled {
        return Err("死亡界面的自动退出启用状态与配置不一致".to_string());
    }

    let panel_name = format!("{AUTO_EXIT_ON_DEATH_PANEL}hd.json");
    let exit_panel = read_auto_exit_on_death_layout(&layout_directory, &panel_name)?;
    if exit_panel.get("type").and_then(serde_json::Value::as_str) != Some("PausePanel")
        || exit_panel.get("name").and_then(serde_json::Value::as_str)
            != Some(AUTO_EXIT_ON_DEATH_PANEL)
        || !layout_has_timed_child_message(&exit_panel, "PausePanelMessage:ExitGame", 0.1)
    {
        return Err("死亡后自动退出面板的退出消息链无效".to_string());
    }

    let stub_name = format!("{AUTO_EXIT_ON_DEATH_PANEL}.json");
    let stub = read_auto_exit_on_death_layout(&layout_directory, &stub_name)?;
    if stub.get("type").and_then(serde_json::Value::as_str) != Some("Panel")
        || stub.get("name").and_then(serde_json::Value::as_str) != Some(AUTO_EXIT_ON_DEATH_PANEL)
    {
        return Err("死亡后自动退出面板入口无效".to_string());
    }
    Ok(())
}

fn validate_lobby_return_hint(mod_directory: &Path, mod_name: &str) -> Result<(), String> {
    let layout_directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let lobby = read_room_tool_layout(&layout_directory, "lobbybackgroundpanelhd.json")?;
    let hint = find_layout_node(&lobby, "D2RHubLobbyReturnHint")
        .ok_or_else(|| "局内房间工具缺少大厅 Esc 返回提示，请重新加工".to_string())?;
    let expected = serde_json::json!({
        "type": "TextBoxWidget",
        "name": "D2RHubLobbyReturnHint",
        "fields": {
            "rect": { "x": -50, "y": 0 },
            "text": "按 Esc 键返回",
            "style": {
                "alignment": { "h": "center", "v": "center" },
                "fontColor": "$FontColorDarkGold",
                "pointSize": 120
            }
        }
    });
    if hint != &expected {
        return Err("大厅 Esc 返回提示的文案或格式无效，请重新加工".to_string());
    }
    Ok(())
}

fn validate_in_game_room_tool_layouts_for_recipe(
    mod_directory: &Path,
    mod_name: &str,
    require_room_submission_transition: bool,
) -> Result<(), String> {
    let layout_directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let hud = read_room_tool_layout(&layout_directory, "HudWarningshd.json")?;
    // Toolbar visibility is independent of the PausePanel keyboard gateways.
    room_toolbar_visibility(&hud)?;

    let toolbar = read_room_tool_layout(&layout_directory, "D2RHubRoomToolbarhd.json")?;
    for action in [
        "PanelManager:OpenPanel:D2RHubQuickRecreateArm",
        "PanelManager:OpenPanel:D2RHubOpenCreateGame",
        "PanelManager:OpenPanel:D2RHubOpenJoinGame",
    ] {
        if !layout_has_child_message(&toolbar, "onClickMessage", action) {
            return Err("局内房间工具栏按钮不完整".to_string());
        }
    }
    for (name, expected_x) in [
        ("D2RHubNextGame", ROOM_TOOL_NEXT_X),
        ("D2RHubCreateGame", ROOM_TOOL_CREATE_X),
        ("D2RHubJoinGame", ROOM_TOOL_JOIN_X),
    ] {
        let button = find_layout_node(&toolbar, name)
            .ok_or_else(|| format!("局内房间工具栏缺少按钮 {name}"))?;
        let scale = button
            .pointer("/fields/rect/scale")
            .and_then(serde_json::Value::as_f64);
        if button
            .pointer("/fields/rect/x")
            .and_then(serde_json::Value::as_i64)
            != Some(expected_x)
            || button
                .pointer("/fields/rect/y")
                .and_then(serde_json::Value::as_i64)
                != Some(ROOM_TOOL_BUTTON_Y)
            || scale.is_none_or(|value| (value - ROOM_TOOL_BUTTON_SCALE).abs() > f64::EPSILON)
        {
            return Err(format!("局内房间工具栏按钮尺寸或位置无效：{name}"));
        }
    }
    let next_game = find_layout_node(&toolbar, "D2RHubNextGame")
        .ok_or_else(|| "局内“下一局”按钮不完整".to_string())?;
    if next_game
        .pointer("/fields/tooltipString")
        .and_then(serde_json::Value::as_str)
        .is_none_or(|tooltip| tooltip.is_empty() || tooltip.starts_with('@'))
        || next_game
            .pointer("/fields/tooltipOffset/y")
            .and_then(serde_json::Value::as_i64)
            != Some(NEXT_GAME_TOOLTIP_OFFSET_Y)
    {
        return Err("局内“下一局”第一层提示位置或文字无效".to_string());
    }

    let arm = read_room_tool_layout(&layout_directory, "D2RHubQuickRecreateArmhd.json")?;
    let armed_next = find_layout_node(&arm, "D2RHubArmedNextGame")
        .ok_or_else(|| "局内“下一局”双击接收按钮不完整".to_string())?;
    if arm.get("type").and_then(serde_json::Value::as_str) != Some("TooltipsPanel")
        || !layout_has_child_message(
            &arm,
            "onClickMessage",
            "PanelManager:OpenPanel:D2RHubQuickRecreate",
        )
        || !layout_has_timed_child_message(
            &arm,
            "PanelManager:ClosePanel:D2RHubQuickRecreateArm",
            QUICK_RECREATE_DOUBLE_CLICK_WINDOW_SECONDS,
        )
        || armed_next
            .pointer("/fields/rect/x")
            .and_then(serde_json::Value::as_i64)
            != Some(ROOM_TOOL_NEXT_X)
        || armed_next
            .pointer("/fields/rect/y")
            .and_then(serde_json::Value::as_i64)
            != Some(ROOM_TOOL_BUTTON_Y)
        || armed_next
            .pointer("/fields/rect/scale")
            .and_then(serde_json::Value::as_f64)
            .is_none_or(|value| (value - ROOM_TOOL_BUTTON_SCALE).abs() > f64::EPSILON)
    {
        return Err("局内“下一局”双击窗口不完整".to_string());
    }

    let quick_recreate = read_room_tool_layout(&layout_directory, "D2RHubQuickRecreatehd.json")?;
    if !layout_has_timed_child_message(
        &quick_recreate,
        "PanelManager:OpenPanel:PauseLayoutGarden",
        ROOM_TRANSITION_OPEN_PAUSE_DELAY_SECONDS,
    ) || !layout_has_timed_child_message(
        &quick_recreate,
        "PausePanelMessage:ExitGame",
        ROOM_TRANSITION_COMMIT_DELAY_SECONDS,
    ) || !layout_has_timed_child_message(
        &quick_recreate,
        "CharacterSelect:LoadCharacter:2",
        ROOM_TRANSITION_COMMIT_DELAY_SECONDS,
    ) {
        return Err("局内“下一局”动作无效".to_string());
    }
    let quick_messages = quick_recreate
        .get("children")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "局内“下一局”消息链无效".to_string())?;
    let exit_index = quick_messages
        .iter()
        .position(|child| {
            child
                .pointer("/fields/message")
                .and_then(serde_json::Value::as_str)
                == Some("PausePanelMessage:ExitGame")
        })
        .ok_or_else(|| "局内“下一局”缺少正常退出消息".to_string())?;
    let load_index = quick_messages
        .iter()
        .position(|child| {
            child
                .pointer("/fields/message")
                .and_then(serde_json::Value::as_str)
                == Some("CharacterSelect:LoadCharacter:2")
        })
        .ok_or_else(|| "局内“下一局”缺少地狱加载消息".to_string())?;
    if exit_index >= load_index {
        return Err("局内“下一局”必须先正常退出再加载角色".to_string());
    }
    if [
        "D2RHubQuickRecreateConfirmhd.json",
        "D2RHubQuickRecreateConfirm.json",
    ]
    .iter()
    .any(|name| layout_directory.join(name).exists())
    {
        return Err("局内“下一局”仍包含旧版二级确认布局".to_string());
    }
    if require_room_submission_transition {
        for (name, native_message) in [
            ("D2RHubCommitCreateGamehd.json", "CreateGame:CreateGame"),
            ("D2RHubCommitJoinGamehd.json", "JoinGame:JoinGame"),
        ] {
            let commit = read_room_tool_layout(&layout_directory, name)?;
            if !layout_has_timed_child_message(
                &commit,
                "PanelManager:OpenPanel:PauseLayoutGarden",
                ROOM_TRANSITION_OPEN_PAUSE_DELAY_SECONDS,
            ) || !layout_has_timed_child_message(
                &commit,
                "PausePanelMessage:ExitGame",
                ROOM_TRANSITION_COMMIT_DELAY_SECONDS,
            ) || !layout_has_timed_child_message(
                &commit,
                native_message,
                ROOM_TRANSITION_COMMIT_DELAY_SECONDS,
            ) {
                return Err(format!("局内房间提交控制器无效：{name}"));
            }
            let messages = commit
                .get("children")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("局内房间提交控制器消息链无效：{name}"))?;
            let exit_index = messages
                .iter()
                .position(|child| {
                    child
                        .pointer("/fields/message")
                        .and_then(serde_json::Value::as_str)
                        == Some("PausePanelMessage:ExitGame")
                })
                .ok_or_else(|| format!("局内房间提交控制器缺少正常退出消息：{name}"))?;
            let submit_index = messages
                .iter()
                .position(|child| {
                    child
                        .pointer("/fields/message")
                        .and_then(serde_json::Value::as_str)
                        == Some(native_message)
                })
                .ok_or_else(|| format!("局内房间提交控制器缺少原生提交消息：{name}"))?;
            if exit_index >= submit_index {
                return Err(format!("局内房间提交控制器必须先退出再提交：{name}"));
            }
        }
    }
    for (name, opener_name, native_target, opposite_target) in [
        (
            "D2RHubOpenCreateGamehd.json",
            "D2RHubOpenCreateGame",
            "CreateGamePanel",
            "JoinGamePanel",
        ),
        (
            "D2RHubOpenJoinGamehd.json",
            "D2RHubOpenJoinGame",
            "JoinGamePanel",
            "CreateGamePanel",
        ),
    ] {
        let opener = read_room_tool_layout(&layout_directory, name)?;
        if opener.get("fields").is_some()
            || !layout_has_timed_child_message(
                &opener,
                &format!("PanelManager:TogglePanel:{native_target}"),
                0.1,
            )
            || !layout_has_timed_child_message(
                &opener,
                &format!("PanelManager:ClosePanel:{opposite_target}"),
                0.1,
            )
            || !layout_has_timed_child_message(
                &opener,
                &format!("PanelManager:ClosePanel:{opener_name}"),
                0.1,
            )
        {
            return Err(format!("局内房间工具未按 MDK 时序打开 {native_target}"));
        }
    }

    for pause_name in ["pauselayouthd.json", "pauselayoutgardenhd.json"] {
        let pause = read_room_tool_layout(&layout_directory, pause_name)?;
        let safe_hub = find_layout_node(&pause, ROOM_TOOL_GATEWAY_HUB)
            .ok_or_else(|| format!("暂停布局缺少安全键盘焦点：{pause_name}"))?;
        if pause
            .pointer("/fields/defaultWidget")
            .and_then(serde_json::Value::as_str)
            != Some(ROOM_TOOL_GATEWAY_HUB)
            || safe_hub
                .pointer("/fields/acceptsReturnKey")
                .and_then(serde_json::Value::as_bool)
                != Some(false)
            || safe_hub
                .pointer("/fields/navigation/left/name")
                .and_then(serde_json::Value::as_str)
                != Some(ROOM_TOOL_CREATE_GATEWAY)
            || safe_hub
                .pointer("/fields/navigation/right/name")
                .and_then(serde_json::Value::as_str)
                != Some(ROOM_TOOL_JOIN_GATEWAY)
        {
            return Err(format!("暂停布局安全键盘焦点无效：{pause_name}"));
        }
        if find_layout_node(&pause, "ReturnToGame").is_none() {
            return Err(format!("暂停布局缺少 ReturnToGame：{pause_name}"));
        }
        if validate_routed_pause_buttons(&pause, pause_name)? == 0 {
            return Err(format!("暂停布局没有可验证的真实按钮：{pause_name}"));
        }
        for (gateway, action, select_direction, back_direction) in [
            (
                ROOM_TOOL_CREATE_GATEWAY,
                "PanelManager:OpenPanel:D2RHubKeyboardOpenCreate",
                "left",
                "right",
            ),
            (
                ROOM_TOOL_JOIN_GATEWAY,
                "PanelManager:OpenPanel:D2RHubKeyboardOpenJoin",
                "right",
                "left",
            ),
        ] {
            let node = find_layout_node(&pause, gateway)
                .ok_or_else(|| format!("暂停布局缺少键盘入口 {gateway}：{pause_name}"))?;
            if node
                .pointer("/fields/onClickMessage")
                .and_then(serde_json::Value::as_str)
                != Some(action)
                || node
                    .pointer("/fields/acceptsReturnKey")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                || node
                    .pointer(&format!("/fields/navigation/{select_direction}/name"))
                    .and_then(serde_json::Value::as_str)
                    != Some(gateway)
                || node
                    .pointer(&format!("/fields/navigation/{back_direction}/name"))
                    .and_then(serde_json::Value::as_str)
                    != Some(ROOM_TOOL_GATEWAY_HUB)
            {
                return Err(format!("暂停布局键盘入口无效 {gateway}：{pause_name}"));
            }
        }
    }
    for (helper_name, helper_panel, native_target, opposite_target) in [
        (
            "D2RHubKeyboardOpenCreatehd.json",
            "D2RHubKeyboardOpenCreate",
            "CreateGamePanel",
            "JoinGamePanel",
        ),
        (
            "D2RHubKeyboardOpenJoinhd.json",
            "D2RHubKeyboardOpenJoin",
            "JoinGamePanel",
            "CreateGamePanel",
        ),
    ] {
        let helper = read_room_tool_layout(&layout_directory, helper_name)?;
        if !layout_has_timed_child_message(&helper, "PausePanelMessage:Close", 0.005)
            || !layout_has_timed_child_message(
                &helper,
                &format!("PanelManager:TogglePanel:{native_target}"),
                0.1,
            )
            || !layout_has_timed_child_message(
                &helper,
                &format!("PanelManager:ClosePanel:{opposite_target}"),
                0.1,
            )
            || !layout_has_timed_child_message(
                &helper,
                &format!("PanelManager:ClosePanel:{helper_panel}"),
                0.1,
            )
        {
            return Err(format!("暂停菜单房间入口未复用 MDK 时序：{helper_name}"));
        }
    }

    let form_specs: [(&str, &str, &[&str], &str, &str); 2] = [
        (
            "creategamepanelhd.json",
            "GameNameInput",
            &["GameNameInput", "PasswordInput", "DescriptionInput"],
            "CreateGame:CreateGame",
            "PanelManager:OpenPanel:D2RHubCommitCreateGame",
        ),
        (
            "joingamepanelhd.json",
            "NameInput",
            &["NameInput", "PasswordInput"],
            "JoinGame:JoinGame",
            "PanelManager:OpenPanel:D2RHubCommitJoinGame",
        ),
    ];
    for (name, primary_input, input_names, native_submit, routed_submit) in form_specs {
        let form = read_room_tool_layout(&layout_directory, name)?;
        if form
            .pointer("/fields/defaultWidget")
            .and_then(serde_json::Value::as_str)
            != Some(primary_input)
            || form
                .pointer("/fields/isDismissable")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
            || form
                .pointer("/fields/acceptsEscKeyEverywhere")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
        {
            return Err(format!("局内房间表单未正确加工：{name}"));
        }
        if input_names.iter().any(|input_name| {
            let Some(node) = find_layout_node(&form, input_name) else {
                return true;
            };
            node.pointer("/fields/imeEnabled")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
                || node.pointer("/fields/alwaysAcceptsKeyInput").is_some()
        }) {
            return Err(format!("局内房间表单无法完整捕获键盘输入：{name}"));
        }
        if require_room_submission_transition
            && (layout_field_value_count(&form, native_submit) != 0
                || layout_field_value_count(&form, routed_submit) == 0)
        {
            return Err(format!("局内房间表单没有完整接入主动退出提交链：{name}"));
        }
        let close_action = if primary_input == "NameInput" {
            "PanelManager:ClosePanel:JoinGamePanel"
        } else {
            "PanelManager:ClosePanel:CreateGamePanel"
        };
        if find_layout_node(&form, "D2RHubCloseRoomForm")
            .and_then(|node| node.pointer("/fields/onClickMessage"))
            .and_then(serde_json::Value::as_str)
            != Some(close_action)
        {
            return Err(format!("局内房间表单缺少关闭按钮：{name}"));
        }
    }
    Ok(())
}

#[cfg(test)]
fn validate_in_game_room_tool_layouts(mod_directory: &Path, mod_name: &str) -> Result<(), String> {
    validate_in_game_room_tool_layouts_for_recipe(mod_directory, mod_name, true)
}

fn validate_compatible_audio_mod_directory_with_policy(
    mods_directory: &Path,
    mod_name: &str,
    mod_directory: PathBuf,
    allow_previous_room_tools: bool,
) -> Result<ValidatedAudioMod, String> {
    let mod_name = plain_mod_name(mod_name)?;
    if !mod_directory.is_dir() {
        return Err(format!("未找到 Mod：{mod_name}"));
    }
    validate_safe_directory_tree(mods_directory, &mod_directory)
        .map_err(|error| format!("Mod 目录安全校验失败：{error}"))?;
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
    let has_feature_group_protocol = recipe_version
        .is_some_and(|version| version >= FEATURE_GROUP_PROTOCOL_RECIPE_VERSION)
        && !parsed_feature_groups.is_empty();
    let current_feature_protocol = recipe_version
        .is_some_and(|version| version >= REQUIRED_AUDIO_MOD_RECIPE_VERSION)
        && has_feature_group_protocol;
    let has_current_identity = manifest
        .get("manifest_format")
        .and_then(serde_json::Value::as_str)
        == Some(MANIFEST_FORMAT)
        && manifest.get("producer").and_then(serde_json::Value::as_str) == Some(PRODUCER_NAME)
        && manifest.get("mod_name").and_then(serde_json::Value::as_str) == Some(mod_name);
    if has_feature_group_protocol && !has_current_identity {
        return Err("功能组协议清单缺少完整的生成器身份信息，请重新加工".to_string());
    }
    if current_feature_protocol {
        if allow_previous_room_tools {
            validate_upgrade_source_feature_group_entries(&parsed_feature_groups)?;
        } else {
            validate_feature_group_entries(&parsed_feature_groups)?;
        }
    }
    // r21 and earlier manifests did not have independently verifiable feature groups. Keep their
    // published audio runtime working, but never expose their claims to additive generation.
    let feature_groups = if has_feature_group_protocol {
        parsed_feature_groups
    } else {
        Vec::new()
    };
    let has_audio_telemetry = if has_feature_group_protocol {
        feature_groups
            .iter()
            .any(|group| group.id == AUDIO_TELEMETRY_FEATURE_ID)
    } else {
        true
    };
    let auto_exit_on_death_active = if current_feature_protocol
        && feature_groups
            .iter()
            .any(|group| group.id == AUTO_EXIT_ON_DEATH_FEATURE_ID)
    {
        auto_exit_on_death_layout_enabled(&mod_directory, mod_name)?
    } else {
        false
    };
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
    if current_feature_protocol
        && feature_groups
            .iter()
            .any(|group| group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID)
    {
        let uses_legacy_room_transition = allow_previous_room_tools
            && feature_groups.iter().any(|group| {
                group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID
                    && group.recipe_version == 21
            });
        validate_in_game_room_tool_layouts_for_recipe(
            &mod_directory,
            mod_name,
            !uses_legacy_room_transition,
        )?;
        if feature_groups.iter().any(|group| {
            group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID
                && group.recipe_version == IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION
        }) {
            validate_lobby_return_hint(&mod_directory, mod_name)?;
        }
    }
    if current_feature_protocol
        && feature_groups
            .iter()
            .any(|group| group.id == AUTO_EXIT_ON_DEATH_FEATURE_ID)
    {
        validate_auto_exit_on_death_layouts(&mod_directory, mod_name, auto_exit_on_death_active)?;
    }
    Ok(ValidatedAudioMod {
        directory: mod_directory,
        recipe_version,
        build_mode,
        source_mod_name,
        feature_groups,
        has_audio_telemetry,
        auto_exit_on_death_enabled: auto_exit_on_death_active,
        current_feature_protocol,
    })
}

fn validate_compatible_audio_mod_directory(
    mods_directory: &Path,
    mod_name: &str,
    mod_directory: PathBuf,
) -> Result<ValidatedAudioMod, String> {
    validate_compatible_audio_mod_directory_with_policy(
        mods_directory,
        mod_name,
        mod_directory,
        false,
    )
}

fn validate_upgradeable_audio_mod(
    mods_directory: &Path,
    mod_name: &str,
) -> Result<ValidatedAudioMod, String> {
    let mod_name = plain_mod_name(mod_name)?;
    validate_compatible_audio_mod_directory_with_policy(
        mods_directory,
        mod_name,
        mods_directory.join(mod_name),
        true,
    )
}

fn validate_audio_mod_directory(
    mods_directory: &Path,
    mod_name: &str,
    mod_directory: PathBuf,
) -> Result<ValidatedAudioMod, String> {
    let validated =
        validate_compatible_audio_mod_directory(mods_directory, mod_name, mod_directory)?;
    if validated
        .recipe_version
        .is_none_or(|version| version < REQUIRED_AUDIO_MOD_RECIPE_VERSION)
        || !validated.current_feature_protocol
    {
        return Err(format!(
            "Mod 不是可验证的当前功能组产物（需要 r{REQUIRED_AUDIO_MOD_RECIPE_VERSION}+）"
        ));
    }
    Ok(validated)
}

fn validate_required_feature_groups_directory(
    mods_directory: &Path,
    mod_name: &str,
    mod_directory: PathBuf,
    required_feature_groups: &[GeneratorFeatureGroup],
) -> Result<ValidatedAudioMod, String> {
    let validated = validate_audio_mod_directory(mods_directory, mod_name, mod_directory)?;
    validate_preserved_feature_groups(required_feature_groups, &validated.feature_groups)?;
    Ok(validated)
}

fn validate_recoverable_backup_directory(
    mods_directory: &Path,
    mod_name: &str,
    backup_directory: &Path,
    required_feature_groups: &[GeneratorFeatureGroup],
) -> Result<(), String> {
    if required_feature_groups.is_empty() {
        return validate_recoverable_audio_mod_directory(
            mods_directory,
            mod_name,
            backup_directory,
        );
    }
    validate_required_feature_groups_directory(
        mods_directory,
        mod_name,
        backup_directory.to_path_buf(),
        required_feature_groups,
    )
    .map(|_| ())
}

fn validate_audio_mod(mods_directory: &Path, mod_name: &str) -> Result<ValidatedAudioMod, String> {
    let mod_name = plain_mod_name(mod_name)?;
    validate_compatible_audio_mod_directory(mods_directory, mod_name, mods_directory.join(mod_name))
}

fn validate_recoverable_audio_mod_directory(
    mods_directory: &Path,
    mod_name: &str,
    mod_directory: &Path,
) -> Result<(), String> {
    let mod_name = plain_mod_name(mod_name)?;
    // Compatibility fallback is deliberately permissive about old manifest fields, never about
    // filesystem topology. A failed strict validator must not let a nested link/reparse point slip
    // into the legacy recovery path.
    validate_safe_directory_tree(mods_directory, mod_directory)
        .map_err(|error| format!("Mod 恢复目录安全校验失败：{error}"))?;
    if validate_compatible_audio_mod_directory(
        mods_directory,
        mod_name,
        mod_directory.to_path_buf(),
    )
    .is_ok()
    {
        return Ok(());
    }
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

fn compatibility_with(
    mods_directory: &Path,
    launch_arguments: &str,
    validate: impl Fn(&Path, &str) -> Result<ValidatedAudioMod, String>,
) -> Compatibility {
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
    match validate(mods_directory, &name) {
        Ok(validated) => {
            if !validated.has_audio_telemetry {
                let update_required = !validated.current_feature_protocol;
                return Compatibility {
                    mod_name,
                    has_txt,
                    ready: false,
                    update_required,
                    recipe_version: validated.recipe_version,
                    build_mode: validated.build_mode,
                    source_mod_name: validated.source_mod_name,
                    reason_code: "missing_audio_feature".to_string(),
                    message: if update_required {
                        "当前 Mod 没有声纹识别功能组，已有功能可原位更新".to_string()
                    } else {
                        "当前 Mod 已经过 D2RHub 加工，但没有声纹识别功能组".to_string()
                    },
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

fn compatibility(mods_directory: &Path, launch_arguments: &str) -> Compatibility {
    compatibility_with(mods_directory, launch_arguments, validate_audio_mod)
}

fn credential_compatibility(mods_directory: &Path, launch_arguments: &str) -> Compatibility {
    compatibility_with(
        mods_directory,
        launch_arguments,
        validate_audio_mod_credential,
    )
}

pub(crate) fn installed_mods(mods_directory: &Path) -> Vec<InstalledMod> {
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
            let has_processing_manifest = processing_manifest_path(&entry.path()).is_some();
            let validation = if has_processing_manifest {
                validate_audio_mod_credential(mods_directory, &name)
            } else {
                Err("普通 Mod 没有 D2RHub 加工凭证".to_string())
            };
            let update_metadata = validation
                .as_ref()
                .err()
                .and_then(|_| official_update_metadata(mods_directory, &name));
            let audio_ready = validation
                .as_ref()
                .is_ok_and(|validated| validated.has_audio_telemetry);
            let update_required = match validation.as_ref() {
                Ok(validated) => !validated.current_feature_protocol,
                Err(_) => update_metadata.is_some(),
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
            let source_mod_name = validation
                .as_ref()
                .ok()
                .and_then(|validated| validated.source_mod_name.clone())
                .or_else(|| {
                    update_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.source_mod_name.clone())
                });
            let audio_reusable = validation.as_ref().is_ok_and(|validated| {
                validated.current_feature_protocol
                    && validated.feature_groups.iter().any(|group| {
                        group.id == AUDIO_TELEMETRY_FEATURE_ID
                            && group.recipe_version == AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION
                    })
            });
            let source_eligible = match validation.as_ref() {
                Ok(validated) => validated.current_feature_protocol,
                Err(_) => !has_processing_manifest,
            };
            let auto_exit_on_death_enabled = validation
                .as_ref()
                .is_ok_and(|validated| validated.auto_exit_on_death_enabled);
            Some(InstalledMod {
                name,
                source_mod_name,
                audio_ready,
                update_required,
                source_eligible,
                feature_groups,
                audio_reusable,
                auto_exit_on_death_enabled,
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
    let (_config, account, context) = configured_account(state, account_id)?;
    let account_name = if account.display_name.trim().is_empty() {
        account.id.as_str()
    } else {
        account.display_name.as_str()
    };
    // Only a snapshot whose PID matches the active registry entry is authoritative. Discovered
    // processes and AccountMeta.running_pid intentionally fail closed: persisted mod_args may have
    // changed after that game process started.
    let (launch_arguments, _running_pid) =
        require_verified_running_session(account_name, session_arguments(state, &account))?;
    validate_in_game_room_tools_for_arguments(account_name, &context, &launch_arguments)
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

fn validate_in_game_room_tools_for_arguments(
    account_name: &str,
    context: &LaunchContext,
    launch_arguments: &str,
) -> Result<(), String> {
    let mod_name = active_mod_name(launch_arguments)?
        .ok_or_else(|| format!("账号“{account_name}”没有启用经过 D2RHub 加工的 Mod"))?;
    if !has_txt_argument(launch_arguments)? {
        return Err(format!("账号“{account_name}”的 Mod 启动参数缺少 -txt"));
    }
    let mods_directory = context.installation.game_directory.join("mods");
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

pub(crate) fn ensure_audio_mod_not_in_use(
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

fn replace_mod_layout_file(path: &Path, contents: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Mod 布局文件名无效".to_string())?;
    let temporary = path.with_file_name(format!(
        ".{file_name}.d2rhub-toggle-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("无法创建 Mod 配置临时文件：{error}"))?;
        file.write_all(contents)
            .map_err(|error| format!("无法写入 Mod 配置：{error}"))?;
        file.sync_all()
            .map_err(|error| format!("无法持久化 Mod 配置：{error}"))?;
        drop(file);
        durable_fs::durable_sibling_replace(&temporary, path)
            .map_err(|error| format!("无法原子切换 Mod 配置：{error}"))?;
        if let Some(parent) = path.parent() {
            durable_fs::sync_directory(parent)
                .map_err(|error| format!("无法持久化 Mod 配置目录：{error}"))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn set_room_toolbar_visible(
    state: &SharedState,
    config: &GlobalConfig,
    mods_directory: &Path,
    mod_name: &str,
    visible: bool,
) -> Result<(), String> {
    let _lease = BuildLease::acquire(state)?;
    ensure_audio_mod_not_in_use(state, config, mod_name)?;
    let validated = validate_audio_mod_credential(mods_directory, mod_name)?;
    if !validated.current_feature_protocol
        || !validated.feature_groups.iter().any(|group| {
            group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID
                && group.recipe_version == IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION
        })
    {
        return Err(format!("Mod“{mod_name}”不含当前版本局内房间工具，请先加工更新"));
    }
    // A display toggle only needs the feature credential and room UI layouts;
    // avoid traversing or decoding the unrelated audio assets.
    validate_in_game_room_tool_layouts_for_recipe(&validated.directory, mod_name, true)?;
    validate_lobby_return_hint(&validated.directory, mod_name)?;
    if read_room_toolbar_visible(mods_directory, mod_name)? == visible {
        return Ok(());
    }
    let layout_path = validated
        .directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY)
        .join("HudWarningshd.json");
    let original = std::fs::read(&layout_path)
        .map_err(|error| format!("无法读取游戏 HUD 布局：{error}"))?;
    let mut document: serde_json::Value = serde_json::from_slice(&original)
        .map_err(|_| "游戏 HUD 布局已损坏".to_string())?;
    let children = document
        .get_mut("children")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| "游戏 HUD 布局缺少 children".to_string())?;
    for child in children {
        if let Some(message) = child.pointer_mut("/fields/message") {
            if matches!(
                message.as_str(),
                Some(ROOM_TOOLBAR_OPEN_MESSAGE | ROOM_TOOLBAR_CLOSE_MESSAGE)
            ) {
                // Closing the toolbar removes both visuals and mouse hit regions;
                // all form controllers and keyboard gateways stay installed.
                *message = serde_json::json!(if visible {
                    ROOM_TOOLBAR_OPEN_MESSAGE
                } else {
                    ROOM_TOOLBAR_CLOSE_MESSAGE
                });
            }
        }
    }
    let updated = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("无法序列化局内按钮配置：{error}"))?;
    let result = replace_mod_layout_file(&layout_path, &updated).and_then(|()| {
        if read_room_toolbar_visible(mods_directory, mod_name)? != visible {
            return Err("写入后的按钮显示状态与请求不一致".to_string());
        }
        Ok(())
    });
    if let Err(error) = result {
        return match replace_mod_layout_file(&layout_path, &original) {
            Ok(()) => Err(format!("局内按钮配置保存失败，已恢复原配置：{error}")),
            Err(restore) => Err(format!(
                "局内按钮配置保存失败：{error}；恢复原配置失败：{restore}"
            )),
        };
    }
    Ok(())
}

pub(crate) fn set_auto_exit_on_death_enabled(
    mods_directory: &Path,
    mod_name: &str,
    enabled: bool,
) -> Result<bool, String> {
    let validated = validate_audio_mod_credential(mods_directory, mod_name)?;
    if !validated
        .feature_groups
        .iter()
        .any(|group| group.id == AUTO_EXIT_ON_DEATH_FEATURE_ID)
    {
        return Err(format!("Mod“{mod_name}”不支持死亡后自动退房"));
    }
    if validated.auto_exit_on_death_enabled == enabled {
        return Ok(enabled);
    }
    validate_auto_exit_on_death_layouts(
        &validated.directory,
        mod_name,
        validated.auto_exit_on_death_enabled,
    )?;

    let layout_path = validated
        .directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY)
        .join("youdiedmodalhd.json");
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    ensure_safe_existing_node(&canonical_mods, &layout_path, false, "死亡界面布局")?;
    let original =
        std::fs::read(&layout_path).map_err(|error| format!("无法读取死亡界面布局：{error}"))?;
    let mut document: serde_json::Value = serde_json::from_slice(&original)
        .map_err(|_| "死亡界面布局已损坏，无法切换死亡退房".to_string())?;
    let children = document
        .get_mut("children")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| "死亡界面布局缺少 children，无法切换死亡退房".to_string())?;
    children.retain(|child| {
        child.get("name").and_then(serde_json::Value::as_str)
            != Some("D2RHubAutoExitOnDeathLauncher")
    });
    if enabled {
        children.push(serde_json::json!({
            "type": "TimerWidget",
            "name": "D2RHubAutoExitOnDeathLauncher",
            "fields": {
                "time": 0.01,
                "message": "PanelManager:OpenPanel:D2RHubAutoExitOnDeath"
            }
        }));
    }
    let updated = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("无法序列化死亡退房配置：{error}"))?;
    replace_mod_layout_file(&layout_path, &updated)?;

    match validate_audio_mod_credential(mods_directory, mod_name).and_then(|after| {
        validate_auto_exit_on_death_layouts(&after.directory, mod_name, enabled)?;
        Ok(after)
    }) {
        Ok(after) if after.auto_exit_on_death_enabled == enabled => Ok(enabled),
        validation => {
            let restore = replace_mod_layout_file(&layout_path, &original);
            let detail = match validation {
                Ok(_) => "写入后的启用状态与请求不一致".to_string(),
                Err(error) => error,
            };
            match restore {
                Ok(()) => Err(format!("死亡退房配置校验失败，已恢复原配置：{detail}")),
                Err(error) => Err(format!(
                    "死亡退房配置校验失败且自动恢复失败：{detail}；恢复错误：{error}"
                )),
            }
        }
    }
}

fn setup_state(state: &SharedState, account_id: &str) -> Result<AudioModSetupState, String> {
    let (_config, account, context) = configured_account(state, account_id)?;
    let mods_directory = context.installation.game_directory.join("mods");
    let configured = credential_compatibility(&mods_directory, &account.mod_args);
    let (session_arguments, running_pid, session_verified) = session_arguments(state, &account);
    let active_session = running_pid.and_then(|_| {
        session_verified.then(|| credential_compatibility(&mods_directory, &session_arguments))
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
) -> Result<AudioModSetupState, String> {
    let shared = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let _lease = BuildLease::acquire(&shared)?;
        setup_state(&shared, &account_id)
    })
    .await
    .map_err(|error| format!("读取 Mod 加工凭证的后台任务异常退出: {error}"))?
}

fn emit_prepare_progress(
    app: &tauri::AppHandle,
    task: Option<&TaskHandle>,
    account_id: &str,
    phase: &str,
    percent: u8,
    message: impl Into<String>,
) {
    let message = message.into();
    if let Some(task) = task {
        let _ = task.update(percent.min(99), phase, &message);
    }
    let _ = app.emit(
        "audio-mod-prepare-progress",
        AudioModPrepareProgress {
            account_id: account_id.to_string(),
            phase: phase.to_string(),
            percent: percent.min(100),
            message,
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
                    "旧版 D2RHub Mod 可以继续运行，但不能安全增量加工；请改选原始 Mod 或当前功能组协议产物"
                        .to_string(),
                );
            }
        }
    }
    Ok((source_mod_name, source_directory))
}

async fn run_audio_mod_generator(
    app: &tauri::AppHandle,
    task: &TaskHandle,
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
    ];
    if let Some(source) = source_directory {
        arguments.push("--source".to_string());
        arguments.push(source.to_string_lossy().into_owned());
    }
    arguments.push("--events".to_string());

    let (mut receiver, child) = app
        .shell()
        .sidecar("d2r-audio-mod")
        .map_err(|error| format!("识别 Mod 生成器不可用: {error}"))?
        .args(arguments)
        .spawn()
        .map_err(|error| format!("无法启动识别 Mod 生成器: {error}"))?;

    let mut report: Option<GeneratorReport> = None;
    let mut stderr = String::new();
    let mut exit_code = None;
    loop {
        let event = match tokio::time::timeout(
            std::time::Duration::from_millis(100),
            receiver.recv(),
        )
        .await
        {
            Ok(event) => event,
            Err(_) => {
                if task.cancellation_requested() {
                    child
                        .kill()
                        .map_err(|error| format!("取消识别 Mod 生成器失败: {error}"))?;
                    return Err("识别 Mod 生成已取消".to_string());
                }
                continue;
            }
        };
        let Some(event) = event else {
            break;
        };
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
                            Some(task),
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
    required_existing_groups: &[GeneratorFeatureGroup],
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
    validate_preserved_feature_groups(required_existing_groups, &report.feature_groups)?;
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
    validate_preserved_feature_groups(required_existing_groups, &validated.feature_groups)?;
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

fn valid_transaction_id(value: &str) -> bool {
    value.len() == 32
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && uuid::Uuid::parse_str(value).is_ok()
}

fn replacement_transaction_id(staged_relative: &Path, backup_relative: &Path) -> Option<String> {
    let stage_parent =
        staged_relative
            .components()
            .next()
            .and_then(|component| match component {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })?;
    let backup = backup_relative.file_name()?.to_str()?;
    let stage_id = stage_parent.strip_prefix(".d2rhub-upgrade-stage-")?;
    let backup_id = backup.strip_prefix(".d2rhub-upgrade-backup-")?;
    (stage_id == backup_id && valid_transaction_id(stage_id)).then(|| stage_id.to_string())
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
    replacement_transaction_id(staged_relative, backup_relative).is_some()
}

fn journal_transaction_id(journal_path: &Path) -> Option<String> {
    let name = journal_path.file_name()?.to_str()?;
    let id = name
        .strip_prefix(REPLACE_JOURNAL_PREFIX)?
        .strip_suffix(REPLACE_JOURNAL_SUFFIX)?;
    valid_transaction_id(id).then(|| id.to_string())
}

fn path_exists_no_follow(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("无法检查事务路径 {}：{error}", path.display())),
    }
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn canonical_safe_mods_root(mods_directory: &Path) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(mods_directory)
        .map_err(|error| format!("无法检查 mods 目录 {}：{error}", mods_directory.display()))?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata_is_reparse_point(&metadata)
    {
        return Err("mods 目录不能是符号链接、联接点或重解析点".to_string());
    }
    std::fs::canonicalize(mods_directory)
        .map_err(|error| format!("无法规范化 mods 目录 {}：{error}", mods_directory.display()))
}

fn ensure_safe_existing_node(
    canonical_mods: &Path,
    path: &Path,
    expect_directory: bool,
    label: &str,
) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("无法检查{label} {}：{error}", path.display()))?;
    if metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata) {
        return Err(format!("{label}不能是符号链接、联接点或重解析点"));
    }
    if expect_directory && !metadata.is_dir() {
        return Err(format!("{label}不是目录：{}", path.display()));
    }
    if !expect_directory && !metadata.is_file() {
        return Err(format!("{label}不是普通文件：{}", path.display()));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| format!("无法规范化{label} {}：{error}", path.display()))?;
    if canonical == canonical_mods || !canonical.starts_with(canonical_mods) {
        return Err(format!("{label}越过了 mods 目录边界：{}", path.display()));
    }
    Ok(())
}

fn ensure_safe_directory_if_present(
    canonical_mods: &Path,
    path: &Path,
    label: &str,
) -> Result<bool, String> {
    if !path_exists_no_follow(path)? {
        return Ok(false);
    }
    ensure_safe_existing_node(canonical_mods, path, true, label)?;
    Ok(true)
}

fn ensure_transaction_paths_safe(
    mods_directory: &Path,
    target_directory: &Path,
    staged_directory: &Path,
    backup_directory: &Path,
) -> Result<PathBuf, String> {
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    ensure_safe_directory_if_present(&canonical_mods, target_directory, "更新目标")?;
    ensure_safe_directory_if_present(&canonical_mods, backup_directory, "更新备份")?;
    let stage_parent = staged_directory
        .parent()
        .ok_or_else(|| "更新暂存目录缺少父目录".to_string())?;
    ensure_safe_directory_if_present(&canonical_mods, stage_parent, "更新暂存父目录")?;
    ensure_safe_directory_if_present(&canonical_mods, staged_directory, "更新暂存目录")?;
    Ok(canonical_mods)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SafeTreeNodeKind {
    File,
    Directory,
}

fn traverse_safe_directory_tree<F>(
    boundary_directory: &Path,
    tree_root: &Path,
    mut visitor: F,
) -> Result<(), String>
where
    F: FnMut(&Path, SafeTreeNodeKind) -> Result<(), String>,
{
    let canonical_boundary = canonical_safe_mods_root(boundary_directory)?;
    let mut pending = vec![(tree_root.to_path_buf(), false)];
    while let Some((path, children_visited)) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("无法检查 Mod 目录树 {}：{error}", path.display()))?;
        if metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata) {
            return Err(format!(
                "Mod 目录树不能包含符号链接、联接点或重解析点：{}",
                path.display()
            ));
        }
        let kind = if metadata.is_dir() {
            SafeTreeNodeKind::Directory
        } else if metadata.is_file() {
            SafeTreeNodeKind::File
        } else {
            return Err(format!(
                "Mod 目录树包含不受支持的文件类型：{}",
                path.display()
            ));
        };
        ensure_safe_existing_node(
            &canonical_boundary,
            &path,
            kind == SafeTreeNodeKind::Directory,
            "Mod 目录树节点",
        )?;

        if kind == SafeTreeNodeKind::Directory && !children_visited {
            pending.push((path.clone(), true));
            let mut children = std::fs::read_dir(&path)
                .map_err(|error| format!("无法遍历 Mod 目录树 {}：{error}", path.display()))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.path())
                        .map_err(|error| format!("无法读取 Mod 目录项：{error}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            children.sort();
            pending.extend(children.into_iter().rev().map(|child| (child, false)));
            continue;
        }

        // Directories are visited after all descendants, making directory metadata flushes
        // bottom-up. Re-checking each expanded directory also narrows the validation/mutation race.
        visitor(&path, kind)?;
    }
    Ok(())
}

fn validate_safe_directory_tree(boundary_directory: &Path, tree_root: &Path) -> Result<(), String> {
    traverse_safe_directory_tree(boundary_directory, tree_root, |_path, _kind| Ok(()))
}

fn sync_safe_directory_tree(mods_directory: &Path, tree_root: &Path) -> Result<(), String> {
    traverse_safe_directory_tree(mods_directory, tree_root, |path, kind| match kind {
        SafeTreeNodeKind::File => sync_regular_file(path),
        SafeTreeNodeKind::Directory => sync_directory(path),
    })?;
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    let stage_parent = tree_root
        .parent()
        .ok_or_else(|| "Mod 目录树缺少父目录".to_string())?;
    if stage_parent != mods_directory {
        ensure_safe_existing_node(&canonical_mods, stage_parent, true, "Mod 目录树父目录")?;
        sync_directory(stage_parent)?;
    }
    sync_directory(mods_directory)
}

#[cfg(not(windows))]
fn sync_regular_file(path: &Path) -> Result<(), String> {
    std::fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("无法持久化 Mod 文件 {}：{error}", path.display()))
}

#[cfg(windows)]
fn sync_regular_file(path: &Path) -> Result<(), String> {
    // FlushFileBuffers requires a handle opened for writing on Windows. File::open creates a
    // read-only handle, which makes sync_all fail with ERROR_ACCESS_DENIED even for writable
    // staged files.
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("无法持久化 Mod 文件 {}：{error}", path.display()))
}

fn sync_directory(path: &Path) -> Result<(), String> {
    durable_fs::sync_directory(path)
        .map_err(|error| format!("无法同步目录元数据 {}：{error}", path.display()))
}

fn rename_directory_and_sync(
    mods_directory: &Path,
    from: &Path,
    to: &Path,
    operation: &str,
) -> Result<(), String> {
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    ensure_safe_existing_node(&canonical_mods, from, true, operation)?;
    for (parent, label) in [
        (from.parent(), "重命名源目录的父目录"),
        (to.parent(), "重命名目标目录的父目录"),
    ] {
        let parent = parent.ok_or_else(|| format!("{operation}路径缺少父目录"))?;
        if parent != mods_directory {
            ensure_safe_existing_node(&canonical_mods, parent, true, label)?;
        }
    }
    if path_exists_no_follow(to)? {
        return Err(format!("{operation}的目标路径已存在：{}", to.display()));
    }
    durable_fs::durable_rename(from, to).map_err(|error| format!("{operation}失败：{error}"))?;
    // A cross-directory rename changes both directory entry sets. Flush the nested stage parent as
    // well as the common mods parent; target/backup/quarantine renames only need the latter.
    for parent in [from.parent(), to.parent()].into_iter().flatten() {
        if parent != mods_directory && path_exists_no_follow(parent)? {
            sync_directory(parent)?;
        }
    }
    sync_directory(mods_directory)
}

fn remove_transaction_directory(
    mods_directory: &Path,
    path: &Path,
    label: &str,
) -> Result<(), String> {
    if !path_exists_no_follow(path)? {
        return Ok(());
    }
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    ensure_safe_existing_node(&canonical_mods, path, true, label)?;
    validate_safe_directory_tree(mods_directory, path)
        .map_err(|error| format!("拒绝清理不安全的{label}：{error}"))?;
    std::fs::remove_dir_all(path)
        .map_err(|error| format!("清理{label}失败 {}：{error}", path.display()))?;
    sync_directory(mods_directory)
}

fn remove_replace_journal(mods_directory: &Path, journal_path: &Path) -> Result<(), String> {
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    ensure_safe_existing_node(&canonical_mods, journal_path, false, "Mod 更新事务记录")?;
    std::fs::remove_file(journal_path)
        .map_err(|error| format!("清理 Mod 更新事务记录失败：{error}"))?;
    sync_directory(mods_directory)
}

fn write_replace_journal(
    mods_directory: &Path,
    mod_name: &str,
    staged_directory: &Path,
    backup_directory: &Path,
    required_feature_groups: &[GeneratorFeatureGroup],
) -> Result<PathBuf, String> {
    write_replace_journal_with_stage_sync(
        mods_directory,
        mod_name,
        staged_directory,
        backup_directory,
        required_feature_groups,
        sync_safe_directory_tree,
    )
}

fn write_replace_journal_with_stage_sync<F>(
    mods_directory: &Path,
    mod_name: &str,
    staged_directory: &Path,
    backup_directory: &Path,
    required_feature_groups: &[GeneratorFeatureGroup],
    sync_staged_tree: F,
) -> Result<PathBuf, String>
where
    F: FnOnce(&Path, &Path) -> Result<(), String>,
{
    validate_feature_group_entries(required_feature_groups)
        .map_err(|error| format!("同名更新需要保留的功能组无效：{error}"))?;
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

    let transaction_id = replacement_transaction_id(&staged_relative, &backup_relative)
        .ok_or_else(|| "同名更新的事务标识无效".to_string())?;
    let journal_path = mods_directory.join(format!(
        "{REPLACE_JOURNAL_PREFIX}{transaction_id}{REPLACE_JOURNAL_SUFFIX}"
    ));
    let temporary_path = mods_directory.join(format!(
        "{REPLACE_JOURNAL_PREFIX}{transaction_id}{REPLACE_JOURNAL_SUFFIX}.tmp"
    ));
    let target_directory = mods_directory.join(mod_name);
    let canonical_mods = ensure_transaction_paths_safe(
        mods_directory,
        &target_directory,
        staged_directory,
        backup_directory,
    )?;
    ensure_safe_existing_node(&canonical_mods, staged_directory, true, "更新暂存目录")?;
    ensure_safe_existing_node(&canonical_mods, &target_directory, true, "更新目标")?;
    if path_exists_no_follow(&journal_path)? || path_exists_no_follow(&temporary_path)? {
        return Err("同名更新的事务记录发生冲突，请重试".to_string());
    }
    // The journal authorizes deletion of the last known-good backup later. It must never become
    // visible until every staged file and directory entry is durable and the no-link tree has been
    // revalidated immediately before the journal mutation.
    sync_staged_tree(mods_directory, staged_directory)
        .map_err(|error| format!("无法持久化同名更新的暂存 Mod：{error}"))?;
    validate_safe_directory_tree(mods_directory, staged_directory)
        .map_err(|error| format!("暂存 Mod 在写入事务记录前发生安全变化：{error}"))?;
    let staged_validation =
        validate_audio_mod_directory(mods_directory, mod_name, staged_directory.to_path_buf())
            .map_err(|error| format!("暂存 Mod 在写入事务记录前严格校验失败：{error}"))?;
    validate_preserved_feature_groups(required_feature_groups, &staged_validation.feature_groups)?;
    let journal = AudioModReplaceJournal {
        format_version: REPLACE_JOURNAL_FORMAT_VERSION,
        mod_name: mod_name.to_string(),
        staged_relative,
        backup_relative,
        required_feature_groups: required_feature_groups.to_vec(),
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
        durable_fs::durable_rename(&temporary_path, &journal_path)
            .map_err(|error| format!("无法提交 Mod 更新事务记录：{error}"))?;
        sync_directory(mods_directory)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    write_result.map(|()| journal_path)
}

fn cleanup_staged_directory(mods_directory: &Path, staged_directory: &Path) -> Result<(), String> {
    let stage_parent = staged_directory
        .parent()
        .ok_or_else(|| "更新暂存目录缺少父目录".to_string())?;
    if !path_exists_no_follow(stage_parent)? {
        return Ok(());
    }
    if stage_parent.parent() != Some(mods_directory)
        || !stage_parent
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".d2rhub-upgrade-stage-"))
    {
        return Err("拒绝清理事务范围外的 Mod 更新暂存目录".to_string());
    }
    remove_transaction_directory(mods_directory, stage_parent, "Mod 更新暂存目录")
}

fn ensure_rollback_marker(mods_directory: &Path, staged_directory: &Path) -> Result<(), String> {
    if path_exists_no_follow(staged_directory)? {
        return Ok(());
    }
    let stage_parent = staged_directory
        .parent()
        .ok_or_else(|| "更新暂存目录缺少父目录".to_string())?;
    if !path_exists_no_follow(stage_parent)? {
        std::fs::create_dir(stage_parent)
            .map_err(|error| format!("创建回滚标记目录失败：{error}"))?;
        sync_directory(mods_directory)?;
    }
    let canonical_mods = canonical_safe_mods_root(mods_directory)?;
    ensure_safe_existing_node(&canonical_mods, stage_parent, true, "更新暂存父目录")?;
    std::fs::create_dir(staged_directory).map_err(|error| format!("创建回滚标记失败：{error}"))?;
    sync_directory(stage_parent)?;
    sync_directory(mods_directory)
}

fn quarantine_directory(
    mods_directory: &Path,
    path: &Path,
    mod_name: &str,
    kind: &str,
) -> Result<PathBuf, String> {
    let quarantine = mods_directory.join(format!(
        ".d2rhub-upgrade-failed-{kind}-{}-{mod_name}",
        uuid::Uuid::new_v4().simple()
    ));
    rename_directory_and_sync(mods_directory, path, &quarantine, "隔离损坏的 Mod")?;
    Ok(quarantine)
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
        let canonical_mods = canonical_safe_mods_root(mods_directory)?;
        ensure_safe_existing_node(&canonical_mods, &journal_path, false, "Mod 更新事务记录")?;
        let journal: AudioModReplaceJournal = serde_json::from_slice(
            &std::fs::read(&journal_path)
                .map_err(|error| format!("无法读取 Mod 更新事务记录：{error}"))?,
        )
        .map_err(|error| format!("Mod 更新事务记录已损坏：{error}"))?;
        let mod_name = plain_mod_name(&journal.mod_name)?;
        let transaction_id =
            replacement_transaction_id(&journal.staged_relative, &journal.backup_relative);
        if journal.format_version != REPLACE_JOURNAL_FORMAT_VERSION
            || !replace_journal_paths_are_valid(
                mod_name,
                &journal.staged_relative,
                &journal.backup_relative,
            )
            || transaction_id != journal_transaction_id(&journal_path)
        {
            return Err("Mod 更新事务记录版本或路径无效，已停止自动恢复".to_string());
        }
        validate_feature_group_entries(&journal.required_feature_groups)
            .map_err(|error| format!("Mod 更新事务需要保留的功能组无效：{error}"))?;
        let target_directory = mods_directory.join(mod_name);
        let staged_directory = mods_directory.join(&journal.staged_relative);
        let backup_directory = mods_directory.join(&journal.backup_relative);
        ensure_transaction_paths_safe(
            mods_directory,
            &target_directory,
            &staged_directory,
            &backup_directory,
        )?;
        let target_exists = path_exists_no_follow(&target_directory)?;
        let staged_exists = path_exists_no_follow(&staged_directory)?;
        let backup_exists = path_exists_no_follow(&backup_directory)?;
        if target_exists && staged_exists && backup_exists {
            return Err(
                "Mod 更新事务同时存在目标、暂存和备份，状态不明确，已停止自动恢复".to_string(),
            );
        }

        let mut final_is_strict = true;
        let mut quarantine_bad_backup = false;
        match (target_exists, staged_exists, backup_exists) {
            // Journal persisted, but the first switch never happened. This is the only target state
            // that may be an old published Mod and therefore uses the compatibility validator.
            (true, true, false) => {
                validate_recoverable_audio_mod_directory(
                    mods_directory,
                    mod_name,
                    &target_directory,
                )
                .map_err(|error| format!("未切换的旧版 Mod 无法通过恢复校验：{error}"))?;
                final_is_strict = false;
            }
            // Backup proves the staged directory was already switched into the target. Never accept
            // that new target via the permissive legacy validator.
            (true, false, true) => {
                if let Err(target_error) = validate_required_feature_groups_directory(
                    mods_directory,
                    mod_name,
                    target_directory.clone(),
                    &journal.required_feature_groups,
                ) {
                    validate_recoverable_backup_directory(
                        mods_directory,
                        mod_name,
                        &backup_directory,
                        &journal.required_feature_groups,
                    )
                    .map_err(|backup_error| {
                        format!(
                            "新版与备份 Mod 都无法通过恢复校验；新版：{target_error}；备份：{backup_error}"
                        )
                    })?;
                    // Keep the rejected new target in the staged slot. Besides preserving evidence,
                    // this makes a crash after rollback unambiguously recover as an old target.
                    let stage_parent = staged_directory
                        .parent()
                        .ok_or_else(|| "更新暂存目录缺少父目录".to_string())?;
                    if !path_exists_no_follow(stage_parent)? {
                        std::fs::create_dir(stage_parent)
                            .map_err(|error| format!("创建损坏新版隔离目录失败：{error}"))?;
                        sync_directory(mods_directory)?;
                    }
                    rename_directory_and_sync(
                        mods_directory,
                        &target_directory,
                        &staged_directory,
                        "隔离损坏的新版 Mod",
                    )?;
                    rename_directory_and_sync(
                        mods_directory,
                        &backup_directory,
                        &target_directory,
                        "恢复旧版 Mod",
                    )?;
                    final_is_strict = false;
                }
            }
            // With no remaining transaction artifacts, the target can only be the newly installed
            // candidate. A legacy/recoverable check here could bless a corrupted new target.
            (true, false, false) => {}
            (false, staged, true) => {
                let backup_validation = validate_recoverable_backup_directory(
                    mods_directory,
                    mod_name,
                    &backup_directory,
                    &journal.required_feature_groups,
                );
                match backup_validation {
                    Ok(()) => {
                        if !staged {
                            ensure_rollback_marker(mods_directory, &staged_directory)?;
                        }
                        rename_directory_and_sync(
                            mods_directory,
                            &backup_directory,
                            &target_directory,
                            "恢复旧版 Mod",
                        )?;
                        final_is_strict = false;
                    }
                    Err(backup_error) if staged => {
                        let staged_validation = validate_audio_mod_directory(
                            mods_directory,
                            mod_name,
                            staged_directory.clone(),
                        )
                        .map_err(|staged_error| {
                            format!(
                                "备份与暂存 Mod 都无法恢复；备份：{backup_error}；暂存：{staged_error}"
                            )
                        })?;
                        validate_preserved_feature_groups(
                            &journal.required_feature_groups,
                            &staged_validation.feature_groups,
                        )?;
                        sync_safe_directory_tree(mods_directory, &staged_directory)?;
                        validate_required_feature_groups_directory(
                            mods_directory,
                            mod_name,
                            staged_directory.clone(),
                            &journal.required_feature_groups,
                        )
                        .map_err(|error| format!("暂存 Mod 在安装前发生安全变化：{error}"))?;
                        rename_directory_and_sync(
                            mods_directory,
                            &staged_directory,
                            &target_directory,
                            "完成暂存 Mod 安装",
                        )?;
                        quarantine_bad_backup = true;
                    }
                    Err(backup_error) => {
                        return Err(format!(
                            "更新备份无法通过恢复校验，且没有严格有效的暂存 Mod：{backup_error}"
                        ));
                    }
                }
            }
            (false, true, false) => {
                let staged_validation = validate_audio_mod_directory(
                    mods_directory,
                    mod_name,
                    staged_directory.clone(),
                )
                .map_err(|error| format!("更新暂存 Mod 无法恢复：{error}"))?;
                validate_preserved_feature_groups(
                    &journal.required_feature_groups,
                    &staged_validation.feature_groups,
                )?;
                sync_safe_directory_tree(mods_directory, &staged_directory)?;
                validate_required_feature_groups_directory(
                    mods_directory,
                    mod_name,
                    staged_directory.clone(),
                    &journal.required_feature_groups,
                )
                .map_err(|error| format!("暂存 Mod 在安装前发生安全变化：{error}"))?;
                rename_directory_and_sync(
                    mods_directory,
                    &staged_directory,
                    &target_directory,
                    "完成暂存 Mod 安装",
                )?;
            }
            (false, false, false) => {
                return Err("Mod 更新事务缺少目标、备份与暂存目录，无法自动恢复".to_string())
            }
            (true, true, true) => unreachable!("ambiguous state was rejected above"),
        }

        if final_is_strict {
            validate_required_feature_groups_directory(
                mods_directory,
                mod_name,
                target_directory.clone(),
                &journal.required_feature_groups,
            )
            .map_err(|error| format!("新版 Mod 恢复后严格校验失败：{error}"))?;
            sync_safe_directory_tree(mods_directory, &target_directory)
                .map_err(|error| format!("新版 Mod 恢复后持久化失败：{error}"))?;
            validate_required_feature_groups_directory(
                mods_directory,
                mod_name,
                target_directory.clone(),
                &journal.required_feature_groups,
            )
            .map_err(|error| format!("新版 Mod 在清理备份前发生安全变化：{error}"))?;
            if path_exists_no_follow(&backup_directory)? {
                if quarantine_bad_backup {
                    let _ = quarantine_directory(
                        mods_directory,
                        &backup_directory,
                        mod_name,
                        "backup",
                    )?;
                } else {
                    remove_transaction_directory(
                        mods_directory,
                        &backup_directory,
                        "Mod 更新备份",
                    )?;
                }
            }
            cleanup_staged_directory(mods_directory, &staged_directory)?;
            remove_replace_journal(mods_directory, &journal_path)?;
        } else {
            if journal.required_feature_groups.is_empty() {
                validate_recoverable_audio_mod_directory(
                    mods_directory,
                    mod_name,
                    &target_directory,
                )
                .map_err(|error| format!("旧版 Mod 恢复后校验失败：{error}"))?;
            } else {
                let restored = validate_audio_mod_directory(
                    mods_directory,
                    mod_name,
                    target_directory.clone(),
                )
                .map_err(|error| format!("原功能组 Mod 恢复后严格校验失败：{error}"))?;
                validate_preserved_feature_groups(
                    &journal.required_feature_groups,
                    &restored.feature_groups,
                )?;
            }
            // Keep the staged path as a rollback marker until the journal is durably removed. If
            // cleanup or journal deletion is interrupted, target+staging still selects legacy-safe
            // validation on the next launch rather than misclassifying the old target as new.
            ensure_rollback_marker(mods_directory, &staged_directory)?;
            remove_replace_journal(mods_directory, &journal_path)?;
            cleanup_staged_directory(mods_directory, &staged_directory)?;
        }
    }
    Ok(())
}

fn replace_audio_mod_directory(
    mods_directory: &Path,
    mod_name: &str,
    staged_directory: &Path,
    backup_directory: &Path,
    required_feature_groups: &[GeneratorFeatureGroup],
) -> Result<(), String> {
    recover_audio_mod_replacements(mods_directory)?;
    let target_directory = mods_directory.join(mod_name);
    let staged_relative = staged_directory
        .strip_prefix(mods_directory)
        .map_err(|_| "同名更新的暂存目录必须位于 mods 目录内".to_string())?;
    let backup_relative = backup_directory
        .strip_prefix(mods_directory)
        .map_err(|_| "同名更新的备份目录必须位于 mods 目录内".to_string())?;
    if !replace_journal_paths_are_valid(mod_name, staged_relative, backup_relative) {
        return Err("同名更新的事务目录无效".to_string());
    }
    ensure_transaction_paths_safe(
        mods_directory,
        &target_directory,
        staged_directory,
        backup_directory,
    )?;
    if !path_exists_no_follow(staged_directory)? {
        return Err("同名更新的暂存目录不存在".to_string());
    }
    if !path_exists_no_follow(&target_directory)? {
        return Err(format!("待更新的 Mod 不存在：{mod_name}"));
    }
    if path_exists_no_follow(backup_directory)? {
        return Err("同名更新的备份目录发生冲突，请重试".to_string());
    }
    let staged_validation =
        validate_audio_mod_directory(mods_directory, mod_name, staged_directory.to_path_buf())
            .map_err(|error| format!("同名更新的暂存 Mod 严格校验失败：{error}"))?;
    validate_preserved_feature_groups(required_feature_groups, &staged_validation.feature_groups)?;
    validate_recoverable_audio_mod_directory(mods_directory, mod_name, &target_directory)
        .map_err(|error| format!("同名更新的旧版 Mod 无法安全备份：{error}"))?;

    let journal_path = match write_replace_journal(
        mods_directory,
        mod_name,
        staged_directory,
        backup_directory,
        required_feature_groups,
    ) {
        Ok(path) => path,
        Err(error) => {
            // A directory-sync error can happen after the named journal is already visible. Recover
            // it while the staged directory is still alive, so the caller's temporary-directory
            // guard cannot turn a prepared legacy target into an ambiguous target-only journal.
            let recovery = recover_audio_mod_replacements(mods_directory);
            return Err(match recovery {
                Ok(()) => error,
                Err(recovery_error) => {
                    format!("{error}；同时清理未提交事务失败：{recovery_error}")
                }
            });
        }
    };
    let staged_validation =
        validate_audio_mod_directory(mods_directory, mod_name, staged_directory.to_path_buf())
            .map_err(|error| format!("暂存 Mod 在目录切换前发生安全变化：{error}"))?;
    validate_preserved_feature_groups(required_feature_groups, &staged_validation.feature_groups)?;
    if let Err(error) = rename_directory_and_sync(
        mods_directory,
        &target_directory,
        backup_directory,
        "备份旧 Mod",
    ) {
        let recovery = recover_audio_mod_replacements(mods_directory);
        return Err(match recovery {
            Ok(()) => format!("无法备份旧 Mod；请确认游戏已经关闭：{error}"),
            Err(recovery_error) => {
                format!("无法备份旧 Mod，且事务恢复需要人工处理：{error}；{recovery_error}")
            }
        });
    }
    if let Err(error) =
        validate_audio_mod_directory(mods_directory, mod_name, staged_directory.to_path_buf())
            .and_then(|validated| {
                validate_preserved_feature_groups(
                    required_feature_groups,
                    &validated.feature_groups,
                )
            })
    {
        let recovery = recover_audio_mod_replacements(mods_directory);
        return Err(match recovery {
            Ok(()) => format!("暂存 Mod 在安装前发生安全变化，已恢复旧版：{error}"),
            Err(recovery_error) => format!(
                "暂存 Mod 在安装前发生安全变化，且事务恢复需要人工处理：{error}；{recovery_error}"
            ),
        });
    }
    match rename_directory_and_sync(
        mods_directory,
        staged_directory,
        &target_directory,
        "安装新版 Mod",
    ) {
        Ok(()) => {
            if let Err(validation_error) =
                validate_audio_mod_directory(mods_directory, mod_name, target_directory.clone())
                    .and_then(|validated| {
                        validate_preserved_feature_groups(
                            required_feature_groups,
                            &validated.feature_groups,
                        )
                    })
            {
                let recovery = recover_audio_mod_replacements(mods_directory);
                return Err(match recovery {
                    Ok(()) => {
                        format!("安装后的新版 Mod 严格校验失败，已恢复旧版：{validation_error}")
                    }
                    Err(recovery_error) => format!(
                        "安装后的新版 Mod 严格校验失败，且事务恢复需要人工处理：{validation_error}；{recovery_error}"
                    ),
                });
            }
            sync_safe_directory_tree(mods_directory, &target_directory)
                .map_err(|error| format!("安装后的新版 Mod 持久化失败：{error}"))?;
            validate_required_feature_groups_directory(
                mods_directory,
                mod_name,
                target_directory.clone(),
                required_feature_groups,
            )
            .map_err(|error| format!("新版 Mod 在清理备份前发生安全变化：{error}"))?;
            remove_transaction_directory(mods_directory, backup_directory, "Mod 更新备份")?;
            cleanup_staged_directory(mods_directory, staged_directory)?;
            remove_replace_journal(mods_directory, &journal_path)?;
            Ok(())
        }
        Err(install_error) => {
            let recovery = recover_audio_mod_replacements(mods_directory);
            Err(match recovery {
                Ok(()) => format!("安装新版 Mod 失败，已恢复旧版：{install_error}"),
                Err(recovery_error) => format!(
                    "安装新版 Mod 失败且自动恢复未完成；旧版仍保留在 {}。安装错误：{}；恢复错误：{}",
                    backup_directory.display(),
                    install_error,
                    recovery_error
                ),
            })
        }
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn prepare_audio_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    mod_name: String,
    source_mod_name: Option<String>,
    include_audio_telemetry: Option<bool>,
    include_room_tools: Option<bool>,
    include_auto_exit_on_death: Option<bool>,
) -> Result<AudioModPrepareResult, String> {
    prepare_audio_mod_task(
        app,
        state,
        AudioModTaskRetryPayload::Prepare {
            account_id,
            mod_name,
            source_mod_name,
            include_audio_telemetry,
            include_room_tools,
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
        mod_name,
        source_mod_name,
        include_audio_telemetry,
        include_room_tools,
        include_auto_exit_on_death,
    } = payload
    else {
        return Err("任务重试数据与 Mod 准备操作不匹配".to_string());
    };
    let retry_payload = serde_json::to_string(&AudioModTaskRetryPayload::Prepare {
        account_id: account_id.clone(),
        mod_name: mod_name.clone(),
        source_mod_name: source_mod_name.clone(),
        include_audio_telemetry,
        include_room_tools,
        include_auto_exit_on_death,
    })
    .map_err(|error| format!("创建任务重试数据失败: {error}"))?;
    let mut request = TaskRequest::new("audio-mod-prepare")
        .for_subject(&account_id)
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
            mod_name,
            source_mod_name,
            include_audio_telemetry,
            include_room_tools,
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
    mod_name: String,
    source_mod_name: Option<String>,
    include_audio_telemetry: Option<bool>,
    include_room_tools: Option<bool>,
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
        mod_name,
        source_mod_name,
        include_audio_telemetry,
        include_room_tools,
        include_auto_exit_on_death,
    } = request;
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
    let requested_features = RequestedFeatureGroups::from_options(
        include_audio_telemetry,
        include_room_tools,
        include_auto_exit_on_death,
    )?;

    emit_prepare_progress(
        &app,
        Some(task),
        &account_id,
        "starting",
        1,
        "正在开始准备…",
    );
    let report = run_audio_mod_generator(
        &app,
        task,
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
        validate_generator_output(&mods_directory, &mod_name, &report, requested_features, &[])?;
    emit_prepare_progress(
        &app,
        None,
        &account_id,
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
    mod_name: Option<String>,
    source_mod_name: Option<String>,
    include_audio_telemetry: Option<bool>,
    include_room_tools: Option<bool>,
    include_auto_exit_on_death: Option<bool>,
) -> Result<AudioModSetupState, String> {
    upgrade_audio_mod_task(
        app,
        state,
        AudioModTaskRetryPayload::Upgrade {
            account_id,
            mod_name,
            source_mod_name,
            include_audio_telemetry,
            include_room_tools,
            include_auto_exit_on_death,
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
        mod_name,
        source_mod_name,
        include_audio_telemetry,
        include_room_tools,
        include_auto_exit_on_death,
    } = payload
    else {
        return Err("任务重试数据与 Mod 更新操作不匹配".to_string());
    };
    let retry_payload = serde_json::to_string(&AudioModTaskRetryPayload::Upgrade {
        account_id: account_id.clone(),
        mod_name: mod_name.clone(),
        source_mod_name: source_mod_name.clone(),
        include_audio_telemetry,
        include_room_tools,
        include_auto_exit_on_death,
    })
    .map_err(|error| format!("创建任务重试数据失败: {error}"))?;
    let mut request = TaskRequest::new("audio-mod-upgrade")
        .for_subject(&account_id)
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
            requested_mod_name: mod_name,
            source_mod_name,
            include_audio_telemetry,
            include_room_tools,
            include_auto_exit_on_death,
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
        mod_name: String,
        source_mod_name: Option<String>,
        include_audio_telemetry: Option<bool>,
        include_room_tools: Option<bool>,
        include_auto_exit_on_death: Option<bool>,
    },
    Upgrade {
        account_id: String,
        mod_name: Option<String>,
        source_mod_name: Option<String>,
        include_audio_telemetry: Option<bool>,
        include_room_tools: Option<bool>,
        include_auto_exit_on_death: Option<bool>,
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
    requested_mod_name: Option<String>,
    source_mod_name: Option<String>,
    include_audio_telemetry: Option<bool>,
    include_room_tools: Option<bool>,
    include_auto_exit_on_death: Option<bool>,
}

async fn upgrade_audio_mod_impl(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    request: UpgradeAudioModRequest,
    task: &TaskHandle,
) -> Result<AudioModSetupState, String> {
    let UpgradeAudioModRequest {
        account_id,
        requested_mod_name,
        source_mod_name,
        include_audio_telemetry,
        include_room_tools,
        include_auto_exit_on_death,
    } = request;
    let shared_state = state.inner().clone();
    let _lease = BuildLease::acquire(&shared_state)?;
    let (config, account, context) = configured_account(&shared_state, &account_id)?;
    let mods_directory = context.installation.game_directory.join("mods");
    recover_audio_mod_replacements(&mods_directory)?;
    let current = if let Some(requested_mod_name) = requested_mod_name.as_deref() {
        let requested_arguments = arguments_with_audio_mod("", requested_mod_name)?;
        compatibility(&mods_directory, &requested_arguments)
    } else {
        compatibility(&mods_directory, &account.mod_args)
    };
    let mod_name = current
        .mod_name
        .as_deref()
        .ok_or_else(|| "当前账号没有配置识别 Mod".to_string())?;
    // The manifest is the source of truth for legacy augment builds. A caller
    // may omit the base Mod (or be unable to expose it after strict validation
    // rejects an old recipe), but a recorded source must still drive the safe
    // rebuild and same-name replacement automatically.
    let source_mod_name = current.source_mod_name.clone().or(source_mod_name);
    let mut explicitly_requested = RequestedFeatureGroups::from_options(
        include_audio_telemetry,
        include_room_tools,
        include_auto_exit_on_death,
    )?;
    let current_validated = match validate_audio_mod(&mods_directory, mod_name) {
        Ok(validated) => Some(validated),
        Err(strict_error)
            if current
                .recipe_version
                .is_some_and(|version| version >= REQUIRED_AUDIO_MOD_RECIPE_VERSION) =>
        {
            Some(
                validate_upgradeable_audio_mod(&mods_directory, mod_name).map_err(
                    |upgrade_error| {
                        format!(
                            "当前功能组协议 Mod 无法作为安全升级来源：{upgrade_error}（当前版本校验：{strict_error}）"
                        )
                    },
                )?,
            )
        }
        Err(_) => None,
    };
    // Only manifests older than the r22 feature-group protocol need the legacy audio fallback.
    // Newer outdated manifests explicitly describe whether audio was installed and must remain
    // room-only when that is what their recorded feature list says.
    if current.update_required
        && current_validated
            .as_ref()
            .is_none_or(|validated| validated.feature_groups.is_empty())
    {
        explicitly_requested.audio_telemetry = true;
    }
    let required_existing_groups: Vec<GeneratorFeatureGroup> = current_validated
        .as_ref()
        .filter(|validated| validated.current_feature_protocol)
        .map(|validated| {
            validated
                .feature_groups
                .iter()
                // The generator replaces known r21/r22 room groups with the current recipe.
                // Preserve every other known or opaque group byte-for-byte across replacement.
                .filter(|group| {
                    !(group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID
                        && PREVIOUS_IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSIONS
                            .contains(&group.recipe_version))
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let requested_features = current_validated
        .as_ref()
        .map_or(explicitly_requested, |validated| {
            explicitly_requested.include_existing_known(&validated.feature_groups)
        });
    let requested_groups_are_present = current_validated
        .as_ref()
        .is_some_and(|validated| requested_features.all_present(&validated.feature_groups));
    let can_add_missing_groups = current_validated
        .as_ref()
        .is_some_and(|validated| validated.current_feature_protocol)
        && !requested_groups_are_present;
    if !current.update_required && !can_add_missing_groups {
        return Err(if requested_groups_are_present {
            "当前识别 Mod 已包含所选功能组，无需更新".to_string()
        } else {
            "当前 Mod 不支持安全原位更新".to_string()
        });
    }
    ensure_audio_mod_not_in_use(&shared_state, &config, mod_name)?;

    let current_protocol_source = current_validated
        .as_ref()
        .filter(|validated| validated.current_feature_protocol)
        .map(|validated| validated.directory.clone());
    let source_directory = if let Some(current_source) = current_protocol_source {
        // Generate into a separate temporary root while using the verified current-protocol Mod
        // as the additive source. The generator carries opaque future groups forward from it.
        Some(current_source)
    } else {
        if current.build_mode.as_deref() == Some("augment") && source_mod_name.is_none() {
            return Err(
                "这个旧版识别 Mod 基于其他 Mod 生成；请选择当时未经加工的原始 Mod".to_string(),
            );
        }
        resolve_source_directory(&mods_directory, mod_name, source_mod_name)?.1
    };
    emit_prepare_progress(
        &app,
        Some(task),
        &account_id,
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
        &required_existing_groups,
    )?;

    if task.cancellation_requested() {
        return Err("识别 Mod 更新已取消".to_string());
    }
    emit_prepare_progress(
        &app,
        Some(task),
        &account_id,
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
    requested_features.validate_present(&staged.feature_groups)?;
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
        &account_id,
        "switching",
        96,
        "正在替换同名旧版 Mod…",
    );
    // 生成过程可能持续数分钟；切换前再次检查，避免另一账号中途启动同名 Mod。
    ensure_audio_mod_not_in_use(&shared_state, &config, mod_name)?;
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
    if !config.optional_module_runtime_allowed(
        crate::domain::config::OPTIONAL_MODULE_AUTOMATION,
    ) || !config.rune_audio_enabled
        || config.rune_audio_target_account != account.id
    {
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
        replace_audio_mod_directory, replace_journal_paths_are_valid,
        require_verified_running_session, resolve_source_directory, set_auto_exit_on_death_enabled,
        traverse_safe_directory_tree, validate_audio_mod, validate_audio_mod_credential,
        validate_auto_exit_on_death_layouts, validate_generator_output,
        validate_in_game_room_tool_layouts, validate_preserved_feature_groups,
        validate_recoverable_audio_mod_directory, write_replace_journal,
        write_replace_journal_with_stage_sync, GeneratorFeatureGroup, GeneratorReport,
        RequestedFeatureGroups, SafeTreeNodeKind, AREA_CATALOG_FILE_NAME,
        AUDIO_TELEMETRY_FEATURE_ID, AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
        AUTO_EXIT_ON_DEATH_FEATURE_ID, AUTO_EXIT_ON_DEATH_FINGERPRINT,
        AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT, IN_GAME_ROOM_TOOLS_FEATURE_ID,
        IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION, ITEM_CATALOG_FILE_NAME,
        LEGACY_MANIFEST_FILE_NAME, NEXT_GAME_TOOLTIP_OFFSET_Y, PROTOCOL_VERSION,
        REQUIRED_AUDIO_MOD_RECIPE_VERSION, ROOM_TOOL_BUTTON_SCALE, ROOM_TOOL_BUTTON_Y,
        ROOM_TOOL_CREATE_X, ROOM_TOOL_JOIN_X, ROOM_TOOL_LAYOUT_DIRECTORY, ROOM_TOOL_NEXT_X,
    };

    const TEST_TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

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

    fn test_audio_fingerprint() -> String {
        format!(
            "audio-v{AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION};protocol={PROTOCOL_VERSION};areas=all_areas;track=charms,essences,gems,jewels,keys,organs,runes;gain_mdb=0"
        )
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
                "recipe_version": AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
                "fingerprint": test_audio_fingerprint()
            }]
        })
    }

    fn write_test_room_tool_layouts(mods_directory: &std::path::Path, mod_name: &str) {
        let layouts = mods_directory
            .join(mod_name)
            .join(format!("{mod_name}.mpq"))
            .join(ROOM_TOOL_LAYOUT_DIRECTORY);
        std::fs::create_dir_all(&layouts).unwrap();
        let pause_layout = || {
            serde_json::json!({
                "fields": {"defaultWidget": "D2RHubKeyboardGatewayHub"},
                "children": [
                    {"type": "ButtonWidget", "name": "D2RHubKeyboardGatewayHub", "fields": {
                        "acceptsReturnKey": false,
                        "navigation": {
                            "left": {"name": "D2RHubKeyboardCreateGateway"},
                            "right": {"name": "D2RHubKeyboardJoinGateway"}
                        }
                    }},
                    {"type": "ButtonWidget", "name": "ReturnToGame", "fields": {"navigation": {
                        "left": {"name": "D2RHubKeyboardCreateGateway"},
                        "right": {"name": "D2RHubKeyboardJoinGateway"}
                    }}},
                    {"type": "ButtonWidget", "name": "D2RHubKeyboardCreateGateway", "fields": {
                        "acceptsReturnKey": true,
                        "navigation": {
                            "left": {"name": "D2RHubKeyboardCreateGateway"},
                            "right": {"name": "D2RHubKeyboardGatewayHub"}
                        },
                        "onClickMessage": "PanelManager:OpenPanel:D2RHubKeyboardOpenCreate"
                    }},
                    {"type": "ButtonWidget", "name": "D2RHubKeyboardJoinGateway", "fields": {
                        "acceptsReturnKey": true,
                        "navigation": {
                            "left": {"name": "D2RHubKeyboardGatewayHub"},
                            "right": {"name": "D2RHubKeyboardJoinGateway"}
                        },
                        "onClickMessage": "PanelManager:OpenPanel:D2RHubKeyboardOpenJoin"
                    }}
                ]
            })
        };
        for (name, document) in [
            (
                "HudWarningshd.json",
                serde_json::json!({"children": [{"fields": {"message": "PanelManager:OpenPanel:D2RHubRoomToolbar"}}]}),
            ),
            ("pauselayouthd.json", pause_layout()),
            ("pauselayoutgardenhd.json", pause_layout()),
            (
                "D2RHubRoomToolbarhd.json",
                serde_json::json!({"children": [
                    {"name": "D2RHubNextGame", "fields": {
                        "rect": {"x": ROOM_TOOL_NEXT_X, "y": ROOM_TOOL_BUTTON_Y, "scale": ROOM_TOOL_BUTTON_SCALE},
                        "tooltipString": "左键双击进入下一局",
                        "tooltipOffset": {"y": NEXT_GAME_TOOLTIP_OFFSET_Y},
                        "onClickMessage": "PanelManager:OpenPanel:D2RHubQuickRecreateArm"
                    }},
                    {"name": "D2RHubCreateGame", "fields": {
                        "rect": {"x": ROOM_TOOL_CREATE_X, "y": ROOM_TOOL_BUTTON_Y, "scale": ROOM_TOOL_BUTTON_SCALE},
                        "onClickMessage": "PanelManager:OpenPanel:D2RHubOpenCreateGame"
                    }},
                    {"name": "D2RHubJoinGame", "fields": {
                        "rect": {"x": ROOM_TOOL_JOIN_X, "y": ROOM_TOOL_BUTTON_Y, "scale": ROOM_TOOL_BUTTON_SCALE},
                        "onClickMessage": "PanelManager:OpenPanel:D2RHubOpenJoinGame"
                    }}
                ]}),
            ),
            (
                "D2RHubQuickRecreateArmhd.json",
                serde_json::json!({
                    "type": "TooltipsPanel",
                    "children": [
                        {"name": "D2RHubArmedNextGame", "fields": {
                            "rect": {"x": ROOM_TOOL_NEXT_X, "y": ROOM_TOOL_BUTTON_Y, "scale": ROOM_TOOL_BUTTON_SCALE},
                            "onClickMessage": "PanelManager:OpenPanel:D2RHubQuickRecreate"
                        }},
                        {"fields": {"time": 0.5, "message": "PanelManager:ClosePanel:D2RHubQuickRecreateArm"}}
                    ]
                }),
            ),
            (
                "D2RHubQuickRecreatehd.json",
                serde_json::json!({"children": [
                    {"fields": {"time": 0.01, "message": "PanelManager:OpenPanel:PauseLayoutGarden"}},
                    {"fields": {"time": 0.05, "message": "PausePanelMessage:ExitGame"}},
                    {"fields": {"time": 0.05, "message": "CharacterSelect:LoadCharacter:2"}}
                ]}),
            ),
            (
                "D2RHubCommitCreateGamehd.json",
                serde_json::json!({"children": [
                    {"fields": {"time": 0.01, "message": "PanelManager:OpenPanel:PauseLayoutGarden"}},
                    {"fields": {"time": 0.05, "message": "PausePanelMessage:ExitGame"}},
                    {"fields": {"time": 0.05, "message": "CreateGame:CreateGame"}}
                ]}),
            ),
            (
                "D2RHubCommitJoinGamehd.json",
                serde_json::json!({"children": [
                    {"fields": {"time": 0.01, "message": "PanelManager:OpenPanel:PauseLayoutGarden"}},
                    {"fields": {"time": 0.05, "message": "PausePanelMessage:ExitGame"}},
                    {"fields": {"time": 0.05, "message": "JoinGame:JoinGame"}}
                ]}),
            ),
            (
                "D2RHubOpenCreateGamehd.json",
                serde_json::json!({"children": [
                    {"fields": {"time": 0.1, "message": "PanelManager:TogglePanel:CreateGamePanel"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:JoinGamePanel"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubOpenCreateGame"}}
                ]}),
            ),
            (
                "D2RHubOpenJoinGamehd.json",
                serde_json::json!({"children": [
                    {"fields": {"time": 0.1, "message": "PanelManager:TogglePanel:JoinGamePanel"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:CreateGamePanel"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubOpenJoinGame"}}
                ]}),
            ),
            (
                "D2RHubKeyboardOpenCreatehd.json",
                serde_json::json!({"children": [
                    {"fields": {"time": 0.005, "message": "PausePanelMessage:Close"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:TogglePanel:CreateGamePanel"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:JoinGamePanel"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubKeyboardOpenCreate"}}
                ]}),
            ),
            (
                "D2RHubKeyboardOpenJoinhd.json",
                serde_json::json!({"children": [
                    {"fields": {"time": 0.005, "message": "PausePanelMessage:Close"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:TogglePanel:JoinGamePanel"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:CreateGamePanel"}},
                    {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubKeyboardOpenJoin"}}
                ]}),
            ),
            (
                "creategamepanelhd.json",
                serde_json::json!({
                    "fields": {"defaultWidget": "GameNameInput", "isDismissable": true, "acceptsEscKeyEverywhere": true},
                    "children": [
                        {"name": "GameNameInput", "fields": {
                            "imeEnabled": true,
                            "onReturnInputMessage": "PanelManager:OpenPanel:D2RHubCommitCreateGame"
                        }},
                        {"name": "PasswordInput", "fields": {"imeEnabled": true}},
                        {"name": "DescriptionInput", "fields": {"imeEnabled": true}},
                        {"name": "D2RHubCloseRoomForm", "fields": {"onClickMessage": "PanelManager:ClosePanel:CreateGamePanel"}}
                    ]
                }),
            ),
            (
                "joingamepanelhd.json",
                serde_json::json!({
                    "fields": {"defaultWidget": "NameInput", "isDismissable": true, "acceptsEscKeyEverywhere": true},
                    "children": [
                        {"name": "NameInput", "fields": {
                            "imeEnabled": true,
                            "onReturnInputMessage": "PanelManager:OpenPanel:D2RHubCommitJoinGame"
                        }},
                        {"name": "PasswordInput", "fields": {"imeEnabled": true}},
                        {"name": "D2RHubCloseRoomForm", "fields": {"onClickMessage": "PanelManager:ClosePanel:JoinGamePanel"}}
                    ]
                }),
            ),
        ] {
            std::fs::write(layouts.join(name), serde_json::to_vec(&document).unwrap()).unwrap();
        }
    }

    fn write_test_auto_exit_on_death_layouts(
        mods_directory: &std::path::Path,
        mod_name: &str,
        enabled: bool,
    ) {
        let layouts = mods_directory
            .join(mod_name)
            .join(format!("{mod_name}.mpq"))
            .join(ROOM_TOOL_LAYOUT_DIRECTORY);
        std::fs::create_dir_all(&layouts).unwrap();
        let death_children = if enabled {
            serde_json::json!([{
                "type": "TimerWidget",
                "name": "D2RHubAutoExitOnDeathLauncher",
                "fields": {
                    "time": 0.01,
                    "message": "PanelManager:OpenPanel:D2RHubAutoExitOnDeath"
                }
            }])
        } else {
            serde_json::json!([])
        };
        for (name, document) in [
            (
                "youdiedmodalhd.json",
                serde_json::json!({
                    "type": "YouDiedModal",
                    "name": "YouDiedModal",
                    "children": death_children
                }),
            ),
            (
                "D2RHubAutoExitOnDeathhd.json",
                serde_json::json!({
                    "type": "PausePanel",
                    "name": "D2RHubAutoExitOnDeath",
                    "children": [{
                        "type": "TimerWidget",
                        "name": "D2RHubAutoExitOnDeathCommit",
                        "fields": {
                            "time": 0.1,
                            "message": "PausePanelMessage:ExitGame"
                        }
                    }]
                }),
            ),
            (
                "D2RHubAutoExitOnDeath.json",
                serde_json::json!({
                    "type": "Panel",
                    "name": "D2RHubAutoExitOnDeath"
                }),
            ),
        ] {
            std::fs::write(layouts.join(name), serde_json::to_vec(&document).unwrap()).unwrap();
        }
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
        let requested = RequestedFeatureGroups::from_options(None, None, None).unwrap();
        assert!(requested.audio_telemetry);
        assert!(!requested.room_tools);
        assert!(!requested.auto_exit_on_death);
        assert_eq!(requested.generator_value(), "audio");
    }

    #[test]
    fn room_tools_require_a_running_pid_with_a_matching_launch_snapshot() {
        assert!(
            require_verified_running_session("offline", ("-mod new".into(), None, false))
                .unwrap_err()
                .contains("没有由 D2RHub 确认的运行实例")
        );
        let discovered = require_verified_running_session(
            "discovered",
            ("-mod persisted-new".into(), Some(42), false),
        )
        .unwrap_err();
        assert!(discovered.contains("PID 42"));
        assert!(discovered.contains("可信启动快照"));
        let trusted = require_verified_running_session(
            "trusted",
            ("-mod actually-running -txt".into(), Some(43), true),
        )
        .unwrap();
        assert_eq!(trusted, ("-mod actually-running -txt".to_string(), 43));
    }

    #[test]
    fn audio_only_mod_can_be_upgraded_in_place_to_audio_and_room_tools() {
        let audio = GeneratorFeatureGroup {
            id: AUDIO_TELEMETRY_FEATURE_ID.to_string(),
            recipe_version: AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
            fingerprint: test_audio_fingerprint(),
            reused_from_source: false,
        };
        let requested = RequestedFeatureGroups::from_options(Some(true), Some(true), Some(false))
            .unwrap()
            .include_existing_known(std::slice::from_ref(&audio));
        assert!(!requested.all_present(std::slice::from_ref(&audio)));
        assert_eq!(requested.generator_value(), "audio,rooms");

        let room = GeneratorFeatureGroup {
            id: IN_GAME_ROOM_TOOLS_FEATURE_ID.to_string(),
            recipe_version: IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
            fingerprint: format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}"),
            reused_from_source: false,
        };
        assert!(requested.all_present(&[audio, room]));
    }

    #[test]
    fn death_auto_exit_is_an_independent_verified_feature_group() {
        let requested =
            RequestedFeatureGroups::from_options(Some(false), Some(false), Some(true)).unwrap();
        assert_eq!(requested.generator_value(), "death-exit");

        let root = test_mods_directory("death_auto_exit");
        let mod_name = "DeathExit";
        write_test_audio_mod(
            &root,
            mod_name,
            serde_json::json!({
                "manifest_format": "d2r-audio-telemetry-mod",
                "producer": "d2r-audio-mod",
                "protocol_version": PROTOCOL_VERSION,
                "recipe_version": REQUIRED_AUDIO_MOD_RECIPE_VERSION,
                "build_mode": "minimal",
                "mod_name": mod_name,
                "feature_groups": [{
                    "id": AUTO_EXIT_ON_DEATH_FEATURE_ID,
                    "recipe_version": 1,
                    "fingerprint": AUTO_EXIT_ON_DEATH_FINGERPRINT
                }]
            }),
        );
        write_test_auto_exit_on_death_layouts(&root, mod_name, true);
        let validated = validate_audio_mod(&root, mod_name).unwrap();
        assert!(!validated.has_audio_telemetry);
        assert!(validated.auto_exit_on_death_enabled);
        assert!(requested.all_present(&validated.feature_groups));
        validate_auto_exit_on_death_layouts(&root.join(mod_name), mod_name, true).unwrap();

        let enabled_group = validated.feature_groups[0].clone();
        let manifest_path = root.join(mod_name).join(LEGACY_MANIFEST_FILE_NAME);
        let manifest_before_toggle = std::fs::read(&manifest_path).unwrap();
        assert!(!set_auto_exit_on_death_enabled(&root, mod_name, false).unwrap());
        assert_eq!(
            std::fs::read(&manifest_path).unwrap(),
            manifest_before_toggle
        );
        let disabled = validate_audio_mod(&root, mod_name).unwrap();
        assert!(!disabled.auto_exit_on_death_enabled);
        let disabled_request =
            RequestedFeatureGroups::from_options(Some(false), Some(false), Some(true)).unwrap();
        assert!(disabled_request.all_present(&disabled.feature_groups));
        validate_preserved_feature_groups(&[enabled_group], &disabled.feature_groups).unwrap();
        let mut legacy_group = disabled.feature_groups[0].clone();
        legacy_group.fingerprint = AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT.to_string();
        validate_preserved_feature_groups(&[legacy_group], &disabled.feature_groups).unwrap();
        validate_auto_exit_on_death_layouts(&root.join(mod_name), mod_name, false).unwrap();
        assert!(set_auto_exit_on_death_enabled(&root, mod_name, true).unwrap());
        validate_auto_exit_on_death_layouts(&root.join(mod_name), mod_name, true).unwrap();

        let death_layout = root
            .join(mod_name)
            .join(format!("{mod_name}.mpq"))
            .join(ROOM_TOOL_LAYOUT_DIRECTORY)
            .join("youdiedmodalhd.json");
        std::fs::write(
            death_layout,
            serde_json::to_vec(&serde_json::json!({
                "children": [{
                    "type": "TimerWidget",
                    "name": "legacy",
                    "fields": {"time": 0.01, "message": "PanelManager:OpenPanel:exitgame"}
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(validate_audio_mod(&root, mod_name)
            .unwrap_err()
            .contains("exitgame"));
        std::fs::remove_dir_all(root).unwrap();
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
            std::path::Path::new(".d2rhub-upgrade-stage-0123456789abcdef0123456789abcdef/safe-mod",),
            std::path::Path::new(".d2rhub-upgrade-backup-0123456789abcdef0123456789abcdef",),
        ));
        assert!(!replace_journal_paths_are_valid(
            "safe-mod",
            std::path::Path::new("safe-mod"),
            std::path::Path::new("safe-mod"),
        ));
        assert!(!replace_journal_paths_are_valid(
            "safe-mod",
            std::path::Path::new("../outside/safe-mod"),
            std::path::Path::new(".d2rhub-upgrade-backup-0123456789abcdef0123456789abcdef",),
        ));
        assert!(!replace_journal_paths_are_valid(
            "safe-mod",
            std::path::Path::new(".d2rhub-upgrade-stage-0123456789abcdef0123456789abcdef/safe-mod",),
            std::path::Path::new(".d2rhub-upgrade-backup-fedcba9876543210fedcba9876543210",),
        ));
    }

    #[test]
    fn durability_traversal_visits_files_before_directories_bottom_up() {
        let root = test_mods_directory("durability_order");
        let mods = root.join("mods");
        let staged = mods.join("stage");
        let nested = staged.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("payload.bin"), b"payload").unwrap();
        let mut events = Vec::new();

        traverse_safe_directory_tree(&mods, &staged, |path, kind| {
            events.push((path.strip_prefix(&mods).unwrap().to_path_buf(), kind));
            Ok(())
        })
        .unwrap();

        let file_index = events
            .iter()
            .position(|(path, kind)| {
                path == std::path::Path::new("stage/nested/payload.bin")
                    && *kind == SafeTreeNodeKind::File
            })
            .unwrap();
        let nested_index = events
            .iter()
            .position(|(path, kind)| {
                path == std::path::Path::new("stage/nested") && *kind == SafeTreeNodeKind::Directory
            })
            .unwrap();
        let root_index = events
            .iter()
            .position(|(path, kind)| {
                path == std::path::Path::new("stage") && *kind == SafeTreeNodeKind::Directory
            })
            .unwrap();
        assert!(file_index < nested_index && nested_index < root_index);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staged_tree_is_synced_before_the_replace_journal_becomes_visible() {
        let root = test_mods_directory("durability_before_journal");
        let mods = root.join("mods");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("durable");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
        let journal = mods.join(format!(
            "{}{}{}",
            super::REPLACE_JOURNAL_PREFIX,
            TEST_TRANSACTION_ID,
            super::REPLACE_JOURNAL_SUFFIX
        ));
        let temporary_journal = journal.with_extension("json.tmp");
        std::fs::create_dir_all(&mods).unwrap();
        write_test_audio_mod(&mods, "durable", current_audio_manifest("durable"));
        write_test_audio_mod(&stage_parent, "durable", current_audio_manifest("durable"));
        let sync_observed = std::cell::Cell::new(false);

        let actual_journal = write_replace_journal_with_stage_sync(
            &mods,
            "durable",
            &staged,
            &backup,
            &[],
            |mods_directory, staged_directory| {
                assert!(!journal.exists());
                assert!(!temporary_journal.exists());
                sync_observed.set(true);
                super::sync_safe_directory_tree(mods_directory, staged_directory)
            },
        )
        .unwrap();

        assert!(sync_observed.get());
        assert_eq!(actual_journal, journal);
        assert!(journal.is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_name_update_switches_only_after_staged_mod_is_ready() {
        let root = test_mods_directory("same_name_upgrade");
        let mods = root.join("mods");
        let staging = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
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
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));

        replace_audio_mod_directory(&mods, "jcy-tz", &staging.join("jcy-tz"), &backup, &[])
            .unwrap();

        assert!(mods.join("jcy-tz").join("new-marker.txt").is_file());
        assert!(!mods.join("jcy-tz").join("old-marker.txt").exists());
        assert!(!backup.exists());
        let result = compatibility(&mods, "-mod jcy-tz -txt");
        assert!(result.ready);
        assert!(!result.update_required);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_name_upgrade_cannot_drop_an_unknown_existing_feature_group() {
        let root = test_mods_directory("preserve_unknown_feature");
        let mods = root.join("mods");
        let staging = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
        std::fs::create_dir_all(&mods).unwrap();
        let unknown = GeneratorFeatureGroup {
            id: "future_feature".to_string(),
            recipe_version: 77,
            fingerprint: "future-v77;opaque=true".to_string(),
            reused_from_source: false,
        };
        let mut current_manifest = current_audio_manifest("preserve-me");
        current_manifest["feature_groups"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(&unknown).unwrap());
        write_test_audio_mod(&mods, "preserve-me", current_manifest);
        let existing = validate_audio_mod(&mods, "preserve-me")
            .unwrap()
            .feature_groups;

        let mut candidate_manifest = current_audio_manifest("preserve-me");
        candidate_manifest["feature_groups"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
                "recipe_version": IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
                "fingerprint": format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}")
            }));
        write_test_audio_mod(&staging, "preserve-me", candidate_manifest.clone());
        write_test_room_tool_layouts(&staging, "preserve-me");

        let error = replace_audio_mod_directory(
            &mods,
            "preserve-me",
            &staging.join("preserve-me"),
            &backup,
            &existing,
        )
        .unwrap_err();
        assert!(error.contains("未无损保留现有功能组“future_feature”"));
        assert!(validate_audio_mod(&mods, "preserve-me")
            .unwrap()
            .feature_groups
            .iter()
            .any(|group| group.id == "future_feature"));
        assert!(!backup.exists());

        let mut carried = unknown.clone();
        carried.reused_from_source = true;
        candidate_manifest["feature_groups"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(carried).unwrap());
        write_test_audio_mod(&staging, "preserve-me", candidate_manifest);
        replace_audio_mod_directory(
            &mods,
            "preserve-me",
            &staging.join("preserve-me"),
            &backup,
            &existing,
        )
        .unwrap();

        let installed = validate_audio_mod(&mods, "preserve-me").unwrap();
        validate_preserved_feature_groups(&existing, &installed.feature_groups).unwrap();
        assert!(installed
            .feature_groups
            .iter()
            .any(|group| group.id == "future_feature" && group.reused_from_source));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crash_recovery_restores_backup_when_new_target_drops_an_unknown_group() {
        let root = test_mods_directory("recover_unknown_feature");
        let mods = root.join("mods");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("preserve-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
        std::fs::create_dir_all(&mods).unwrap();
        let unknown = GeneratorFeatureGroup {
            id: "future_feature".to_string(),
            recipe_version: 77,
            fingerprint: "future-v77;opaque=true".to_string(),
            reused_from_source: false,
        };
        let mut old_manifest = current_audio_manifest("preserve-me");
        old_manifest["feature_groups"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(&unknown).unwrap());
        write_test_audio_mod(&mods, "preserve-me", old_manifest);
        std::fs::write(mods.join("preserve-me").join("old-marker.txt"), b"old").unwrap();
        let required = validate_audio_mod(&mods, "preserve-me")
            .unwrap()
            .feature_groups;

        let mut incomplete_new = current_audio_manifest("preserve-me");
        incomplete_new["feature_groups"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
                "recipe_version": IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
                "fingerprint": format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}")
            }));
        write_test_audio_mod(&stage_parent, "preserve-me", incomplete_new);
        write_test_room_tool_layouts(&stage_parent, "preserve-me");
        std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();

        // Simulate a journal produced just before preservation metadata was enforced, then a crash
        // after both directory renames. Recovery must treat the missing opaque group as an invalid
        // new target and restore the strict r22 backup instead of deleting it.
        let journal = write_replace_journal(&mods, "preserve-me", &staged, &backup, &[]).unwrap();
        let mut document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
        document["required_feature_groups"] = serde_json::to_value(&required).unwrap();
        std::fs::write(&journal, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        std::fs::rename(mods.join("preserve-me"), &backup).unwrap();
        std::fs::rename(&staged, mods.join("preserve-me")).unwrap();

        recover_audio_mod_replacements(&mods).unwrap();

        let restored = validate_audio_mod(&mods, "preserve-me").unwrap();
        validate_preserved_feature_groups(&required, &restored.feature_groups).unwrap();
        assert!(mods.join("preserve-me").join("old-marker.txt").is_file());
        assert!(!mods.join("preserve-me").join("new-marker.txt").exists());
        assert!(!backup.exists());
        assert!(!journal.exists());
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
            recipe_version: AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
            fingerprint: test_audio_fingerprint(),
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
                auto_exit_on_death: false,
            },
            &[],
        )
        .unwrap();
        assert_eq!(actual.feature_groups, vec![audio_group]);

        let required_future = GeneratorFeatureGroup {
            id: "future_feature".to_string(),
            recipe_version: 77,
            fingerprint: "future-v77;opaque=true".to_string(),
            reused_from_source: false,
        };
        assert!(validate_generator_output(
            &root,
            mod_name,
            &report,
            RequestedFeatureGroups {
                audio_telemetry: true,
                room_tools: false,
                auto_exit_on_death: false,
            },
            &[required_future],
        )
        .unwrap_err()
        .contains("未无损保留现有功能组“future_feature”"));

        assert!(validate_generator_output(
            &root,
            mod_name,
            &report,
            RequestedFeatureGroups {
                audio_telemetry: true,
                room_tools: true,
                auto_exit_on_death: false,
            },
            &[],
        )
        .unwrap_err()
        .contains("局内房间工具"));

        report.feature_groups.push(GeneratorFeatureGroup {
            id: IN_GAME_ROOM_TOOLS_FEATURE_ID.to_string(),
            recipe_version: IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
            fingerprint: format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}"),
            reused_from_source: false,
        });
        assert!(validate_generator_output(
            &root,
            mod_name,
            &report,
            RequestedFeatureGroups {
                audio_telemetry: true,
                room_tools: true,
                auto_exit_on_death: false,
            },
            &[],
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
            "recipe_version": IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
            "fingerprint": format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}")
        }]);
        write_test_audio_mod(&root, name, manifest);
        write_test_room_tool_layouts(&root, name);
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
    fn claimed_room_tools_require_supported_metadata_and_complete_layouts() {
        let root = test_mods_directory("claimed_room_tools");
        let name = "room-tools";
        let room_manifest = |recipe_version, fingerprint: &str| {
            serde_json::json!({
                "manifest_format": "d2r-audio-telemetry-mod",
                "producer": "d2r-audio-mod",
                "protocol_version": PROTOCOL_VERSION,
                "recipe_version": REQUIRED_AUDIO_MOD_RECIPE_VERSION,
                "build_mode": "minimal",
                "mod_name": name,
                "feature_groups": [{
                    "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
                    "recipe_version": recipe_version,
                    "fingerprint": fingerprint
                }]
            })
        };

        let room_fingerprint = format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}");
        write_test_audio_mod(
            &root,
            name,
            room_manifest(IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION, &room_fingerprint),
        );
        let credential = validate_audio_mod_credential(&root, name).unwrap();
        assert_eq!(
            credential.feature_groups[0].id,
            IN_GAME_ROOM_TOOLS_FEATURE_ID
        );
        let incomplete = validate_audio_mod(&root, name).unwrap_err();
        assert!(incomplete.contains("缺少布局文件"));

        write_test_room_tool_layouts(&root, name);
        let validated = validate_audio_mod(&root, name).unwrap();
        validate_in_game_room_tool_layouts(&validated.directory, name).unwrap();

        let previous_recipe = IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION - 1;
        write_test_audio_mod(
            &root,
            name,
            room_manifest(previous_recipe, &format!("room-tools-v{previous_recipe}")),
        );
        assert!(validate_audio_mod(&root, name)
            .unwrap_err()
            .contains(&format!("配方 r{previous_recipe} 不受支持")));
        write_test_audio_mod(
            &root,
            name,
            room_manifest(
                IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
                "forged-room-tools",
            ),
        );
        assert!(validate_audio_mod(&root, name)
            .unwrap_err()
            .contains("指纹无效"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn known_audio_metadata_is_strict_but_unknown_groups_remain_forward_compatible() {
        let root = test_mods_directory("known_and_unknown_groups");
        let name = "feature-metadata";
        let mut manifest = current_audio_manifest(name);
        let unsupported_recipe = AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION + 1;
        manifest["feature_groups"][0]["recipe_version"] = serde_json::json!(unsupported_recipe);
        write_test_audio_mod(&root, name, manifest.clone());
        assert!(validate_audio_mod(&root, name)
            .unwrap_err()
            .contains(&format!("配方 r{unsupported_recipe} 不受支持")));

        manifest["feature_groups"][0]["recipe_version"] =
            serde_json::json!(AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION);
        manifest["feature_groups"][0]["fingerprint"] = serde_json::json!(format!(
            "audio-v{AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION};fixture=forged"
        ));
        write_test_audio_mod(&root, name, manifest.clone());
        assert!(validate_audio_mod(&root, name)
            .unwrap_err()
            .contains("指纹无效"));

        manifest["feature_groups"] = serde_json::json!([{
            "id": "future_feature",
            "recipe_version": 77,
            "fingerprint": "future-v77;opaque=true"
        }]);
        write_test_audio_mod(&root, name, manifest);
        let validated = validate_audio_mod(&root, name).unwrap();
        assert_eq!(validated.feature_groups[0].id, "future_feature");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_directory_switch_restores_the_old_mod() {
        let root = test_mods_directory("replace_recovery_rollback");
        let mods = root.join("mods");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
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
        let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
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
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
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
        let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
        std::fs::rename(mods.join("recover-me"), &backup).unwrap();
        std::fs::rename(&staged, mods.join("recover-me")).unwrap();

        recover_audio_mod_replacements(&mods).unwrap();

        assert!(mods.join("recover-me").join("new-marker.txt").is_file());
        assert!(!backup.exists());
        assert!(!journal.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_never_accepts_a_legacy_shaped_new_target_over_a_good_backup() {
        let root = test_mods_directory("replace_recovery_strict_new_target");
        let mods = root.join("mods");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
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
        std::fs::write(mods.join("recover-me").join("old-marker.txt"), b"old").unwrap();
        write_test_audio_mod(
            &stage_parent,
            "recover-me",
            current_audio_manifest("recover-me"),
        );
        std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();
        let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
        std::fs::rename(mods.join("recover-me"), &backup).unwrap();
        std::fs::rename(&staged, mods.join("recover-me")).unwrap();
        // It still passes the permissive published-release validator, but it is not a complete r22
        // target and therefore must never make the good backup disposable.
        write_test_audio_mod(
            &mods,
            "recover-me",
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "build_mode": "minimal",
                "mod_name": "recover-me"
            }),
        );

        recover_audio_mod_replacements(&mods).unwrap();

        assert!(mods.join("recover-me").join("old-marker.txt").is_file());
        assert!(!mods.join("recover-me").join("new-marker.txt").exists());
        assert!(!backup.exists());
        assert!(!journal.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn target_without_transaction_artifacts_requires_strict_current_validation() {
        let root = test_mods_directory("replace_recovery_target_only_strict");
        let mods = root.join("mods");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
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
        std::fs::write(mods.join("recover-me").join("only-copy.txt"), b"evidence").unwrap();
        write_test_audio_mod(
            &stage_parent,
            "recover-me",
            current_audio_manifest("recover-me"),
        );
        let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
        std::fs::remove_dir_all(&stage_parent).unwrap();

        let error = recover_audio_mod_replacements(&mods).unwrap_err();

        assert!(error.contains("严格校验"));
        assert!(mods.join("recover-me").join("only-copy.txt").is_file());
        assert!(
            journal.is_file(),
            "unsafe recovery must preserve its journal"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn valid_staged_candidate_wins_only_after_a_missing_target_backup_is_rejected() {
        let root = test_mods_directory("replace_recovery_bad_backup");
        let mods = root.join("mods");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(mods.join("recover-me").join("recover-me.mpq")).unwrap();
        std::fs::write(mods.join("recover-me").join("bad-backup.txt"), b"bad").unwrap();
        write_test_audio_mod(
            &stage_parent,
            "recover-me",
            current_audio_manifest("recover-me"),
        );
        std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();
        let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
        std::fs::rename(mods.join("recover-me"), &backup).unwrap();

        recover_audio_mod_replacements(&mods).unwrap();

        assert!(mods.join("recover-me").join("new-marker.txt").is_file());
        assert!(!backup.exists());
        assert!(!journal.exists());
        assert!(std::fs::read_dir(&mods)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".d2rhub-upgrade-failed-backup-")
            }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn transaction_paths_reject_a_symlinked_stage_parent_outside_mods() {
        use std::os::unix::fs::symlink;

        let root = test_mods_directory("replace_symlink_escape");
        let mods = root.join("mods");
        let outside = root.join("outside");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(outside.join("recover-me")).unwrap();
        std::fs::write(outside.join("recover-me").join("evidence.txt"), b"outside").unwrap();
        write_test_audio_mod(&mods, "recover-me", current_audio_manifest("recover-me"));
        symlink(&outside, &stage_parent).unwrap();

        let error = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap_err();

        assert!(error.contains("符号链接"));
        assert_eq!(
            std::fs::read(outside.join("recover-me").join("evidence.txt")).unwrap(),
            b"outside"
        );
        let journal = mods.join(format!(
            "{}{}{}",
            super::REPLACE_JOURNAL_PREFIX,
            TEST_TRANSACTION_ID,
            super::REPLACE_JOURNAL_SUFFIX
        ));
        std::fs::write(
            &journal,
            serde_json::to_vec(&serde_json::json!({
                "format_version": 1,
                "mod_name": "recover-me",
                "staged_relative": format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}/recover-me"),
                "backup_relative": format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}")
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(recover_audio_mod_replacements(&mods)
            .unwrap_err()
            .contains("符号链接"));
        assert!(journal.is_file());
        assert_eq!(
            std::fs::read(outside.join("recover-me").join("evidence.txt")).unwrap(),
            b"outside"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn validation_and_journaling_reject_nested_staged_symlinks() {
        use std::os::unix::fs::symlink;

        let root = test_mods_directory("nested_stage_symlink");
        let mods = root.join("mods");
        let outside = root.join("outside");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("evidence.txt"), b"outside").unwrap();
        write_test_audio_mod(&mods, "recover-me", current_audio_manifest("recover-me"));
        write_test_audio_mod(
            &stage_parent,
            "recover-me",
            current_audio_manifest("recover-me"),
        );
        let nested_link = staged.join("recover-me.mpq").join("nested-link");
        symlink(&outside, &nested_link).unwrap();

        assert!(validate_audio_mod(&stage_parent, "recover-me")
            .unwrap_err()
            .contains("符号链接"));
        assert!(
            validate_recoverable_audio_mod_directory(&mods, "recover-me", &staged)
                .unwrap_err()
                .contains("符号链接")
        );
        assert!(
            write_replace_journal(&mods, "recover-me", &staged, &backup, &[])
                .unwrap_err()
                .contains("符号链接")
        );
        assert_eq!(
            std::fs::read(outside.join("evidence.txt")).unwrap(),
            b"outside"
        );
        assert!(!mods
            .join(format!(
                "{}{}{}",
                super::REPLACE_JOURNAL_PREFIX,
                TEST_TRANSACTION_ID,
                super::REPLACE_JOURNAL_SUFFIX
            ))
            .exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn transaction_paths_reject_windows_directory_reparse_points() {
        use std::os::windows::fs::symlink_dir;

        let root = test_mods_directory("replace_reparse_escape");
        let mods = root.join("mods");
        let outside = root.join("outside");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(outside.join("recover-me")).unwrap();
        write_test_audio_mod(&mods, "recover-me", current_audio_manifest("recover-me"));
        if symlink_dir(&outside, &stage_parent).is_err() {
            // Windows may deny symlink creation when Developer Mode is disabled. Production also
            // checks FILE_ATTRIBUTE_REPARSE_POINT, which covers directory junctions.
            std::fs::remove_dir_all(root).unwrap();
            return;
        }

        let error = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap_err();
        assert!(error.contains("重解析点") || error.contains("符号链接"));
        std::fs::remove_dir(&stage_parent).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn validation_and_journaling_reject_nested_windows_reparse_points() {
        use std::os::windows::fs::symlink_dir;

        let root = test_mods_directory("nested_stage_reparse");
        let mods = root.join("mods");
        let outside = root.join("outside");
        let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
        let staged = stage_parent.join("recover-me");
        let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        write_test_audio_mod(&mods, "recover-me", current_audio_manifest("recover-me"));
        write_test_audio_mod(
            &stage_parent,
            "recover-me",
            current_audio_manifest("recover-me"),
        );
        let nested_link = staged.join("recover-me.mpq").join("nested-link");
        if symlink_dir(&outside, &nested_link).is_err() {
            std::fs::remove_dir_all(root).unwrap();
            return;
        }

        assert!(validate_audio_mod(&stage_parent, "recover-me")
            .unwrap_err()
            .contains("重解析点"));
        assert!(
            validate_recoverable_audio_mod_directory(&mods, "recover-me", &staged)
                .unwrap_err()
                .contains("重解析点")
        );
        assert!(
            write_replace_journal(&mods, "recover-me", &staged, &backup, &[])
                .unwrap_err()
                .contains("重解析点")
        );
        std::fs::remove_dir(&nested_link).unwrap();
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
                        "recipe_version": AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
                        "fingerprint": test_audio_fingerprint()
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
