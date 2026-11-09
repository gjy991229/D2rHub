//! Filesystem-backed trust boundary for installed and generated Mods.
//!
//! Discovery uses credential validation; install, replacement and recovery use
//! complete directory and layout validation. Pure feature rules live in domain.
pub(super) mod layouts;
mod manifest;

use super::filesystem::{
    canonical_safe_mods_root, ensure_safe_existing_node, validate_safe_directory_tree,
};
use super::InstalledMod;
use crate::domain::mod_arguments::{active_mod_name, has_txt_argument, plain_mod_name};
use crate::domain::mod_processing::{
    parse_feature_groups, validate_feature_group_entries, validate_preserved_feature_groups,
    validate_runtime_feature_group_entries, validate_upgrade_source_feature_group_entries,
    GeneratorFeatureGroup, GeneratorReport, RequestedFeatureGroups, AUDIO_TELEMETRY_FEATURE_ID,
    AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION, AUTO_EXIT_ON_DEATH_FEATURE_ID,
    ESC_NEXT_GAME_FEATURE_ID, IN_GAME_ROOM_TOOLS_FEATURE_ID,
};
use crate::rune_audio::{
    catalog::AREA_CATALOG_FILE_NAME, item_catalog::ITEM_CATALOG_FILE_NAME,
    protocol::PROTOCOL_VERSION,
};
use layouts::{
    auto_exit_on_death_layout_enabled, validate_auto_exit_on_death_layouts,
    validate_esc_next_game_layouts, validate_in_game_room_tool_layouts_for_version,
    validate_lobby_return_hint,
};
#[cfg(test)]
pub(super) use manifest::LEGACY_MANIFEST_FILE_NAME;
pub(super) use manifest::REQUIRED_AUDIO_MOD_RECIPE_VERSION;
use manifest::{
    official_update_metadata, processing_manifest_path, read_protocol_version,
    source_mod_name_from_manifest, FEATURE_GROUP_PROTOCOL_RECIPE_VERSION, MANIFEST_FORMAT,
    PRODUCER_NAME,
};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(super) struct Compatibility {
    pub(super) mod_name: Option<String>,
    pub(super) has_txt: bool,
    pub(super) ready: bool,
    pub(super) update_required: bool,
    pub(super) recipe_version: Option<u32>,
    pub(super) build_mode: Option<String>,
    pub(super) source_mod_name: Option<String>,
    pub(super) reason_code: String,
    pub(super) message: String,
}

#[derive(Debug)]
pub(super) struct ValidatedAudioMod {
    pub(super) directory: PathBuf,
    pub(super) recipe_version: Option<u32>,
    pub(super) build_mode: Option<String>,
    pub(super) source_mod_name: Option<String>,
    pub(super) feature_groups: Vec<GeneratorFeatureGroup>,
    pub(super) has_audio_telemetry: bool,
    pub(super) auto_exit_on_death_enabled: bool,
    pub(super) current_feature_protocol: bool,
}

impl ValidatedAudioMod {
    fn update_available(&self) -> bool {
        !self.current_feature_protocol
            || self.feature_groups.iter().any(|group| {
                group.id == AUDIO_TELEMETRY_FEATURE_ID
                    && group.recipe_version < AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION
            })
    }
}

#[derive(Debug)]
pub(super) struct ValidatedGeneratorOutput {
    pub(super) directory: PathBuf,
    pub(super) feature_groups: Vec<GeneratorFeatureGroup>,
}

/// Fast, read-only trust check used by settings discovery.
///
/// The signed-by-construction manifest identity, recipe versions and feature
/// fingerprints are enough to render setup state. Expensive recursive tree and
/// generated-asset verification remains mandatory for generation, replacement,
/// recovery, account application, and explicit compatibility checks. Room
/// shortcuts deliberately do not invoke either validation path.
pub(super) fn validate_audio_mod_credential(
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
        validate_runtime_feature_group_entries(&parsed_feature_groups, PROTOCOL_VERSION)?;
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
            validate_upgrade_source_feature_group_entries(
                &parsed_feature_groups,
                PROTOCOL_VERSION,
            )?;
        } else {
            validate_runtime_feature_group_entries(&parsed_feature_groups, PROTOCOL_VERSION)?;
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
    if let Some(room_group) = feature_groups
        .iter()
        .find(|group| current_feature_protocol && group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID)
    {
        validate_in_game_room_tool_layouts_for_version(
            &mod_directory,
            mod_name,
            room_group.recipe_version,
        )?;
        if room_group.recipe_version >= 23 {
            validate_lobby_return_hint(&mod_directory, mod_name)?;
        }
    }
    if feature_groups
        .iter()
        .any(|group| group.id == "hostile_auto_restart")
    {
        if !feature_groups
            .iter()
            .any(|g| g.id == ESC_NEXT_GAME_FEATURE_ID)
        {
            return Err("敌对自动重开缺少双击 Esc 模块".into());
        }
        let root = mod_directory.join(format!("{mod_name}.mpq"));
        let table = std::fs::read_to_string(root.join("data/global/excel/sounds.txt"))
            .map_err(|e| format!("敌对提示声音表不可读：{e}"))?;
        for name in [
            "cursor_hostile",
            "cursor_hostile_hd_1",
            "cursor_hostile_hd_2",
            "cursor_hostile_hd_3",
        ] {
            let file = format!("d2rhub_hostile\\{name}.flac");
            if !table.lines().any(|line| {
                line.split('\t').next() == Some(name) && line.split('\t').any(|value| value == file)
            }) || !root
                .join("data/hd/global/sfx/d2rhub_hostile")
                .join(format!("{name}.flac"))
                .is_file()
            {
                return Err(format!("敌对提示资源不完整：{name}"));
            }
        }
    }
    if let Some(group) = feature_groups
        .iter()
        .find(|group| current_feature_protocol && group.id == "party_loot")
    {
        layouts::validate_party_loot(&mod_directory, mod_name, group.recipe_version)?;
    }
    if let Some(group) = feature_groups
        .iter()
        .find(|group| current_feature_protocol && group.id == "scene_shortcuts")
    {
        layouts::validate_scene_shortcuts_version(&mod_directory, mod_name, group.recipe_version)?;
    }
    if let Some(group) = feature_groups
        .iter()
        .find(|group| current_feature_protocol && group.id == ESC_NEXT_GAME_FEATURE_ID)
    {
        validate_esc_next_game_layouts(&mod_directory, mod_name, group.recipe_version)?;
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

pub(super) fn validate_audio_mod_directory(
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
    validate_feature_group_entries(&validated.feature_groups, PROTOCOL_VERSION)?;
    Ok(validated)
}

pub(super) fn validate_required_feature_groups_directory(
    mods_directory: &Path,
    mod_name: &str,
    mod_directory: PathBuf,
    required_feature_groups: &[GeneratorFeatureGroup],
) -> Result<ValidatedAudioMod, String> {
    let validated = validate_audio_mod_directory(mods_directory, mod_name, mod_directory)?;
    validate_preserved_feature_groups(required_feature_groups, &validated.feature_groups)?;
    Ok(validated)
}

pub(super) fn validate_recoverable_backup_directory(
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

pub(super) fn validate_audio_mod(
    mods_directory: &Path,
    mod_name: &str,
) -> Result<ValidatedAudioMod, String> {
    let mod_name = plain_mod_name(mod_name)?;
    validate_compatible_audio_mod_directory(mods_directory, mod_name, mods_directory.join(mod_name))
}

pub(super) fn validate_recoverable_audio_mod_directory(
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
                let update_required = validated.update_available();
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
            let update_required = validated.update_available();
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
                    "当前 Mod 有新版功能模块可用；请在 Mod 管理中更新，声纹 r5 新增地区声纹"
                        .to_string()
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

pub(super) fn compatibility(mods_directory: &Path, launch_arguments: &str) -> Compatibility {
    compatibility_with(mods_directory, launch_arguments, validate_audio_mod)
}

pub(super) fn credential_compatibility(
    mods_directory: &Path,
    launch_arguments: &str,
) -> Compatibility {
    compatibility_with(
        mods_directory,
        launch_arguments,
        validate_audio_mod_credential,
    )
}

pub(super) fn installed_mods(mods_directory: &Path) -> Vec<InstalledMod> {
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
            let mpq = entry.path().join(format!("{name}.mpq"));
            if !mpq.exists() && processing_manifest_path(&entry.path()).is_none() {
                if let Some((original, suffix)) = name.rsplit_once("_backup_") {
                    if suffix.len() == 32
                        && suffix.bytes().all(|b| b.is_ascii_hexdigit())
                        && plain_mod_name(original).is_ok()
                        && entry.path().join(format!("{original}.mpq")).is_dir()
                    {
                        return Some(InstalledMod {
                            name,
                            source_mod_name: None,
                            audio_ready: false,
                            update_required: false,
                            source_eligible: true,
                            requires_unpack: false,
                            unpack_recovery_required: false,
                            feature_groups: Vec::new(),
                            audio_reusable: false,
                            auto_exit_on_death_enabled: false,
                            room_tools_default_hell: true,
                            scene_audio_enabled: true,
                        });
                    }
                }
            }
            let pending = crate::mpq_mod::has_pending_conversion(&entry.path());
            if mpq.is_file() || pending {
                return Some(InstalledMod {
                    name,
                    source_mod_name: None,
                    audio_ready: false,
                    update_required: false,
                    source_eligible: false,
                    requires_unpack: true,
                    unpack_recovery_required: pending,
                    feature_groups: Vec::new(),
                    audio_reusable: false,
                    auto_exit_on_death_enabled: false,
                    room_tools_default_hell: true,
                    scene_audio_enabled: true,
                });
            }
            if !mpq.is_dir() {
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
                Ok(validated) => validated.update_available(),
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
                .unwrap_or_else(|_| {
                    update_metadata
                        .as_ref()
                        .map(|meta| meta.feature_groups.clone())
                        .unwrap_or_default()
                });
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
                requires_unpack: false,
                unpack_recovery_required: false,
                name,
                source_mod_name,
                audio_ready,
                update_required,
                source_eligible,
                feature_groups,
                audio_reusable,
                auto_exit_on_death_enabled,
                scene_audio_enabled: processing_manifest_path(&entry.path())
                    .and_then(|path| std::fs::read(path).ok())
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .and_then(|document| super::rebuild::audio_options(&document).ok())
                    .is_none_or(|options| options.areas != "none"),
                room_tools_default_hell: super::room_tool_options::default_hell(&entry.path())
                    .unwrap_or(true),
            })
        })
        .collect::<Vec<_>>();
    mods.sort_by_key(|entry| entry.name.to_lowercase());
    mods
}

pub(super) fn resolve_source_directory(
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
        if crate::mpq_mod::has_pending_conversion(source)
            || source
                .join(format!(
                    "{}.mpq",
                    source_mod_name.as_deref().unwrap_or_default()
                ))
                .is_file()
        {
            return Err("请先在 Mod 库点击“解压”，完成后再加工这个 Mod".to_string());
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

pub(super) fn validate_generator_output(
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
    validate_feature_group_entries(&report.feature_groups, PROTOCOL_VERSION)
        .map_err(|error| format!("生成器报告无效：{error}"))?;
    requested_features.validate_present(&report.feature_groups, PROTOCOL_VERSION)?;
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
    requested_features.validate_present(&validated.feature_groups, PROTOCOL_VERSION)?;
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

#[cfg(test)]
mod scene_upgrade_regression {
    use super::*;
    #[test]
    #[ignore = "requires local scene recipe fixtures; read-only, including installed legacy Mod"]
    fn old_scene_sources_validate_by_their_own_recipe() {
        let cases: Vec<(String, String, u32)> =
            serde_json::from_str(&std::env::var("D2RHUB_SCENE_UPGRADE_CASES").unwrap()).unwrap();
        for (directory, name, version) in cases {
            let root = PathBuf::from(directory);
            let result = validate_compatible_audio_mod_directory_with_policy(
                &root,
                &name,
                root.join(&name),
                true,
            );
            assert!(result.is_ok(), "{name} r{version}: {:?}", result.err());
            if version < 7 {
                assert!(
                    layouts::validate_scene_shortcuts(&root.join(&name), &name).is_err(),
                    "legacy source must not pass current output validation"
                );
            } else {
                layouts::validate_scene_shortcuts(&root.join(&name), &name).unwrap();
            }
        }
    }
}
