//! Reads processor manifest identity and recovers compatible historical metadata.
use crate::domain::mod_arguments::plain_mod_name;
use std::path::{Path, PathBuf};

pub(in crate::audio_mod) const MANIFEST_FILE_NAME: &str = "d2rhub-mod-manifest.json";
pub(in crate::audio_mod) const LEGACY_MANIFEST_FILE_NAME: &str = "audio-telemetry-manifest.json";
pub(in crate::audio_mod) const MANIFEST_FORMAT: &str = "d2r-audio-telemetry-mod";
pub(in crate::audio_mod) const PRODUCER_NAME: &str = "d2r-audio-mod";
pub(in crate::audio_mod) const REQUIRED_AUDIO_MOD_RECIPE_VERSION: u32 = 26;
pub(in crate::audio_mod) const LATEST_AUDIO_MOD_RECIPE_VERSION: u32 = 27;
pub(in crate::audio_mod) const FEATURE_GROUP_PROTOCOL_RECIPE_VERSION: u32 = 22;
#[derive(Debug)]
pub(super) struct OfficialUpdateMetadata {
    pub(super) recipe_version: Option<u32>,
    pub(super) build_mode: Option<String>,
    pub(super) source_mod_name: Option<String>,
    pub(super) feature_groups: Vec<String>,
}

pub(super) fn read_protocol_version(path: &Path) -> Result<u8, String> {
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

pub(super) fn source_mod_name_from_manifest(
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

pub(in crate::audio_mod) fn processing_manifest_path(mod_directory: &Path) -> Option<PathBuf> {
    [MANIFEST_FILE_NAME, LEGACY_MANIFEST_FILE_NAME]
        .iter()
        .map(|name| mod_directory.join(name))
        .find(|path| path.is_file())
}

pub(super) fn official_update_metadata(
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
    let groups = crate::domain::mod_processing::parse_feature_groups(&manifest).ok()?;
    let feature_groups = groups.iter().map(|group| group.id.clone()).collect();
    Some(OfficialUpdateMetadata {
        recipe_version,
        build_mode,
        source_mod_name,
        feature_groups,
    })
}
