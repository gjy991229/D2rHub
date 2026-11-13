//! Settings owned by verified Hub products and their processed descendants.
use crate::{resource_install::no_links, state::SharedState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const ORIGIN: &str = "d2rhub-base-mod.json";
const VERSION: &str = "data/global/dataversionbuild.txt";
const JOURNAL: &str = ".d2rhub-version-transaction.json";
const RECORDS: [&str; 4] = [
    "generation-manifest.json",
    "enhancement-manifest.json",
    "mod-version.json",
    crate::resource_install::RECEIPT,
];

/// Runs at startup and after installation, never on a timer.
pub(crate) fn synchronize_installed(state: &SharedState) {
    let Ok(_lease) = state.mod_mutations().try_acquire() else {
        return;
    };
    let Some(config) = state.configuration().snapshot() else {
        return;
    };
    if ensure_closed().is_err() {
        return;
    }
    for game in [&config.cn_game_path, &config.global_game_path] {
        let game = Path::new(game);
        let Ok(version) = crate::mod_resources::game_version(game) else {
            continue;
        };
        let mods = game.join("mods");
        let Ok(entries) = fs::read_dir(&mods) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let result = (|| -> Result<(), String> {
                let root = entry.path();
                if fs::read_to_string(version_path(&root, &name))
                    .ok()
                    .is_some_and(|text| text.trim() == version)
                {
                    return Ok(());
                }
                let Some(current) = settings(&root, &name)? else {
                    return Ok(());
                };
                if current.game_data_version != version {
                    save(&root, &name, &version, &current.etag)?;
                    crate::logger::log_msg(
                        "INFO",
                        "HubMod",
                        &format!("已同步 {name} 数据版本为 {version}"),
                    );
                }
                Ok(())
            })();
            if let Err(error) = result {
                crate::logger::log_msg(
                    "WARN",
                    "HubMod",
                    &format!("{name} 数据版本自动同步未完成: {error}"),
                );
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Identity {
    schema: u8,
    mod_name: String,
    pub base_mod: String,
    pub profile: String,
    pub waypoints: bool,
}

fn profile(name: &str) -> Option<&'static str> {
    match name {
        "LiteHub" => Some("main"),
        "BoHub" => Some("filler"),
        "NullHub" => Some("min"),
        _ => None,
    }
}
pub(crate) fn read(path: &Path) -> Result<Vec<u8>, String> {
    no_links(path)?;
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err("Mod 配置文件类型或大小无效".into());
    }
    fs::read(path).map_err(|e| e.to_string())
}
fn json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&read(path)?).map_err(|e| e.to_string())
}
fn safe_root(mods: &Path, name: &str) -> Result<PathBuf, String> {
    crate::domain::mod_arguments::generated_audio_mod_name(name)?;
    let root = mods.join(name);
    for p in [
        mods.to_path_buf(),
        root.clone(),
        root.join(format!("{name}.mpq")),
        root.join(format!("{name}.mpq/data")),
        root.join(format!("{name}.mpq/data/global")),
    ] {
        no_links(&p)?;
        if !p.is_dir() {
            return Err("请先解压并安装 Mod".into());
        }
    }
    Ok(root)
}

pub(crate) fn identity(mods: &Path, name: &str) -> Result<Option<Identity>, String> {
    if crate::domain::mod_arguments::generated_audio_mod_name(name).is_err() {
        return Ok(None);
    }
    let root = safe_root(mods, name)?;
    if let Some(source) = crate::audio_mod::settings_source(mods, name)? {
        if json(&root.join(format!("{name}.mpq/modinfo.json")))?["name"] != name {
            return Err("加工 Mod 名称与文件不一致".into());
        }
        if root.join(ORIGIN).exists() {
            let origin: Identity =
                serde_json::from_slice(&read(&root.join(ORIGIN))?).map_err(|e| e.to_string())?;
            if origin.schema != 1
                || origin.mod_name != name
                || origin.base_mod != source
                || profile(&origin.base_mod) != Some(origin.profile.as_str())
                || (origin.profile == "min" && origin.waypoints)
            {
                return Err("Hub 来源记录与加工凭证不一致".into());
            }
            return Ok(Some(origin));
        }
        if profile(&source).is_none() {
            return Ok(None);
        }
        if !mods.join(&source).is_dir() {
            return Ok(None);
        }
        // Existing beta18 outputs only copied the MPQ tree. Recover their origin
        // from a validated installed base, never from the folder name alone.
        let source_root = safe_root(mods, &source)?;
        let Some(base) = crate::lightweight_mod::inspect(&source_root, &source)? else {
            return Ok(None);
        };
        return Ok(Some(Identity {
            schema: 1,
            mod_name: name.into(),
            base_mod: source,
            waypoints: base.profile != "min"
                && root
                    .join(format!("{name}.mpq/data/global/excel/actinfo.txt"))
                    .is_file(),
            profile: base.profile,
        }));
    }
    let Some(base) = crate::lightweight_mod::inspect(&root, name)? else {
        return Ok(None);
    };
    if profile(name) != Some(base.profile.as_str()) {
        return Ok(None);
    }
    Ok(Some(Identity {
        schema: 1,
        mod_name: name.into(),
        base_mod: name.into(),
        profile: base.profile,
        waypoints: matches!(name, "LiteHub" | "BoHub")
            && root.join("d2rhub-waypoints.json").is_file(),
    }))
}

pub(crate) fn inherit(source: Option<&Path>, output: &Path, name: &str) -> Result<(), String> {
    let Some(source) = source else {
        return Ok(());
    };
    let Some(parent) = source.parent() else {
        return Ok(());
    };
    let Some(source_name) = source.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    if let Ok(Some(mut origin)) = identity(parent, source_name) {
        origin.mod_name = name.into();
        crate::downloads::save_json(&output.join(ORIGIN), &origin)?;
    }
    Ok(())
}

/// Reapply supported preferences to a freshly generated tree, without copying
/// any old generated layouts or resources into it.
pub(crate) fn inherit_preferences(source: &Path, output: &Path, name: &str) -> Result<(), String> {
    inherit(Some(source), output, name)?;
    if let Some(previous) = settings(source, name)? {
        if let Some(next) = settings(output, name)? {
            if previous.game_data_version != next.game_data_version {
                save(output, name, &previous.game_data_version, &next.etag)?;
            }
        }
        if previous.waypoints_supported && !crate::mod_waypoints::scene_shortcuts_enabled(output)? {
            if let (Some(old), Some(new)) = (
                crate::mod_waypoints::read_config(source, name)?,
                crate::mod_waypoints::read_config(output, name)?,
            ) {
                crate::mod_waypoints::save(output, name, &old.selected, &new.etag)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn mod_root(state: &SharedState, edition: &str, name: &str) -> Result<PathBuf, String> {
    let config = state.configuration().snapshot().ok_or("尚未配置游戏目录")?;
    let game = match edition {
        "CN" => config.cn_game_path,
        "Global" => config.global_game_path,
        _ => return Err("未知客户端版本".into()),
    };
    if game.trim().is_empty() {
        return Err("尚未配置游戏目录".into());
    }
    safe_root(&Path::new(game.trim()).join("mods"), name)
}
pub(crate) fn ensure_closed() -> Result<(), String> {
    if sysinfo::System::new_all()
        .processes()
        .values()
        .any(|p| p.name().to_string_lossy().eq_ignore_ascii_case("D2R.exe"))
    {
        return Err("请先关闭游戏，再修改 Mod 设置".into());
    }
    Ok(())
}
fn version_path(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{name}.mpq")).join(VERSION)
}
fn version(value: &str) -> Result<String, String> {
    let value = value.trim_start_matches('\u{feff}').trim();
    if value.is_empty()
        || value.len() > 10
        || !value.bytes().all(|b| b.is_ascii_digit())
        || value.parse::<u32>().ok().is_none_or(|n| n == 0)
    {
        return Err("游戏数据版本必须为有效的正整数".into());
    }
    Ok(value.to_string())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
type Snapshot = BTreeMap<String, Vec<u8>>;
fn snapshot(root: &Path, name: &str) -> Result<Snapshot, String> {
    let mut files = BTreeMap::from([(VERSION.into(), read(&version_path(root, name))?)]);
    for record in RECORDS {
        if root.join(record).exists() {
            files.insert(record.into(), read(&root.join(record))?);
        }
    }
    Ok(files)
}
fn etag(files: &Snapshot) -> Result<String, String> {
    Ok(hash(&serde_json::to_vec(files).map_err(|e| e.to_string())?))
}
fn file_path(root: &Path, name: &str, key: &str) -> Result<PathBuf, String> {
    if key == VERSION {
        Ok(version_path(root, name))
    } else if RECORDS.contains(&key) {
        Ok(root.join(key))
    } else {
        Err("版本恢复记录包含未知文件".into())
    }
}
fn write_files(root: &Path, name: &str, files: &Snapshot) -> Result<(), String> {
    for (key, bytes) in files {
        crate::mod_waypoints::atomic_write(&file_path(root, name, key)?, bytes)?;
    }
    Ok(())
}
pub(crate) fn recover(root: &Path, name: &str) -> Result<(), String> {
    recover_with_guard(root, name, ensure_closed)
}
fn recover_with_guard(
    root: &Path,
    name: &str,
    ensure_closed: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if !root.join(JOURNAL).exists() {
        return Ok(());
    }
    ensure_closed()?;
    let files: Snapshot =
        serde_json::from_slice(&read(&root.join(JOURNAL))?).map_err(|e| e.to_string())?;
    if !files.contains_key(VERSION) {
        return Err("版本恢复记录缺少原始数据".into());
    }
    for key in files.keys() {
        file_path(root, name, key)?;
    }
    write_files(root, name, &files)?;
    fs::remove_file(root.join(JOURNAL)).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct HubModSettings {
    base_mod: String,
    game_data_version: String,
    waypoints_supported: bool,
    etag: String,
}
fn settings(root: &Path, name: &str) -> Result<Option<HubModSettings>, String> {
    recover(root, name)?;
    crate::mod_waypoints::recover(root, name)?;
    let Some(origin) = identity(root.parent().ok_or("缺少 mods 目录")?, name)? else {
        return Ok(None);
    };
    let files = snapshot(root, name)?;
    Ok(Some(HubModSettings {
        base_mod: origin.base_mod,
        waypoints_supported: origin.waypoints,
        game_data_version: version(
            std::str::from_utf8(&files[VERSION]).map_err(|e| e.to_string())?,
        )?,
        etag: etag(&files)?,
    }))
}
fn save(root: &Path, name: &str, next: &str, expected: &str) -> Result<HubModSettings, String> {
    let next = version(next)?;
    let current = settings(root, name)?.ok_or("仅 Hub Mod 及其加工成品可修改数据版本")?;
    let before = snapshot(root, name)?;
    if current.etag != expected || etag(&before)? != expected {
        return Err("Mod 配置已被修改，请重新读取后保存".into());
    }
    let mut after = before.clone();
    let managed_receipt = crate::resource_install::read_receipt(root)?.filter(|receipt| {
        crate::resource_install::tree_hash(root).ok().as_deref()
            == Some(receipt.tree_sha256.as_str())
    });
    after.insert(VERSION.into(), next.as_bytes().to_vec());
    for record in RECORDS {
        if record == crate::resource_install::RECEIPT {
            continue;
        }
        let Some(bytes) = before.get(record) else {
            continue;
        };
        let mut document: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if document["mod_name"] != name {
            return Err("Mod 版本记录名称不匹配".into());
        }
        document["game_data_version"] = next.clone().into();
        if let Some(entry) = document.get_mut("files").and_then(|v| v.get_mut(VERSION)) {
            let prior_size = entry["bytes"].as_u64().ok_or("数据版本大小记录无效")?;
            *entry = serde_json::json!({"sha256":hash(next.as_bytes()), "bytes":next.len()});
            if let Some(total) = document.get("generated_bytes").and_then(Value::as_u64) {
                document["generated_bytes"] = total
                    .checked_sub(prior_size)
                    .and_then(|v| v.checked_add(next.len() as u64))
                    .ok_or("Mod 大小记录无效")?
                    .into();
            }
        }
        after.insert(
            record.into(),
            serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?,
        );
    }
    let journal = serde_json::to_vec(&before).map_err(|e| e.to_string())?;
    if journal.len() > 32 * 1024 * 1024 {
        return Err("Mod 版本恢复记录过大，未修改文件".into());
    }
    crate::mod_waypoints::atomic_write(&root.join(JOURNAL), &journal)?;
    let result = (|| {
        write_files(root, name, &after)?;
        if let Some(mut receipt) = managed_receipt {
            receipt.tree_sha256 = crate::resource_install::tree_hash(root)?;
            let bytes = serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?;
            crate::mod_waypoints::atomic_write(
                &root.join(crate::resource_install::RECEIPT),
                &bytes,
            )?;
            after.insert(crate::resource_install::RECEIPT.into(), bytes);
        }
        if snapshot(root, name)? != after {
            return Err("数据版本写入校验失败".into());
        }
        let origin = identity(root.parent().ok_or("缺少 mods 目录")?, name)?
            .ok_or("保存后 Hub 身份不可用")?;
        let updated = HubModSettings {
            base_mod: origin.base_mod,
            waypoints_supported: origin.waypoints,
            game_data_version: next.clone(),
            etag: etag(&after)?,
        };
        fs::remove_file(root.join(JOURNAL)).map_err(|e| e.to_string())?;
        Ok(updated)
    })();
    if result.is_err() {
        recover(root, name)?;
    }
    result
}

#[tauri::command]
pub fn get_hub_mod_settings(
    state: tauri::State<'_, SharedState>,
    edition: String,
    mod_name: String,
) -> Result<Option<HubModSettings>, String> {
    let _lease = state.mod_mutations().try_acquire()?;
    let root = mod_root(state.inner(), &edition, &mod_name)?;
    settings(&root, &mod_name)
}
#[tauri::command]
pub fn save_hub_mod_data_version(
    state: tauri::State<'_, SharedState>,
    edition: String,
    mod_name: String,
    game_data_version: String,
    etag: String,
) -> Result<HubModSettings, String> {
    let _lease = state.mod_mutations().try_acquire()?;
    let root = mod_root(state.inner(), &edition, &mod_name)?;
    ensure_closed()?;
    save(&root, &mod_name, &game_data_version, &etag)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("hub-settings-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn put(path: &Path, value: Value) {
        crate::downloads::save_json(path, &value).unwrap();
    }
    fn base(mods: &Path, name: &str) -> PathBuf {
        let root = mods.join(name);
        let mpq = root.join(format!("{name}.mpq"));
        fs::create_dir_all(mpq.join("data/global/excel")).unwrap();
        fs::write(mpq.join(VERSION), "93854").unwrap();
        put(&mpq.join("modinfo.json"), serde_json::json!({"name":name}));
        put(
            &root.join("mod-version.json"),
            serde_json::json!({"mod_name":name,"mod_version":"r1","game_data_version":"93854"}),
        );
        put(
            &root.join("enhancement-manifest.json"),
            serde_json::json!({"producer":"d2rhub-local-mod-builder","mod_name":name,"profile":profile(name).unwrap(),"mod_version":"r1","game_data_version":"93854","launch_arguments":format!("-mod {name} -txt -assettestmode 1"),"verified_output_integrity":true,"files":{VERSION:{"bytes":5,"sha256":hash(b"93854")}},"generated_bytes":5}),
        );
        if name != "NullHub" {
            put(
                &root.join("d2rhub-waypoints.json"),
                serde_json::json!({"revision":1,"feature":"act4_waypoints"}),
            );
        }
        root
    }
    fn derived(mods: &Path, name: &str, source: &str) -> PathBuf {
        use crate::domain::mod_processing::IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION;
        let root = mods.join(name);
        let mpq = root.join(format!("{name}.mpq"));
        fs::create_dir_all(mpq.join("data/global/excel")).unwrap();
        fs::write(mpq.join(VERSION), "93854").unwrap();
        fs::write(mpq.join("data/global/excel/actinfo.txt"), "fixture").unwrap();
        put(&mpq.join("modinfo.json"), serde_json::json!({"name":name}));
        put(
            &root.join("d2rhub-mod-manifest.json"),
            serde_json::json!({"manifest_format":"d2r-audio-telemetry-mod","producer":"d2r-audio-mod","mod_name":name,"protocol_version":7,"recipe_version":25,"build_mode":"augment","source_mod_copied":true,"source_mod_name":source,"feature_groups":[{"id":"in_game_room_tools","recipe_version":IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,"fingerprint":format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}"),"reused_from_source":false}]}),
        );
        root
    }
    #[test]
    fn scene_rebuild_keeps_generated_waypoints_instead_of_old_preferences() {
        let old = Scratch::new();
        let new = Scratch::new();
        base(&old.0, "LiteHub");
        base(&new.0, "LiteHub");
        let source = derived(&old.0, "Custom", "LiteHub");
        let output = derived(&new.0, "Custom", "LiteHub");
        let path = output.join("d2rhub-mod-manifest.json");
        let mut manifest = json(&path).unwrap();
        manifest["feature_groups"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id":"scene_shortcuts", "recipe_version":7,
                "fingerprint":crate::domain::mod_processing::SCENE_SHORTCUTS_FINGERPRINT,
                "reused_from_source":false
            }));
        put(&path, manifest);
        let table = output.join("Custom.mpq/data/global/excel/actinfo.txt");
        let before = fs::read(&table).unwrap();
        // Fixtures deliberately contain no editable waypoint table: inheritance
        // must skip reading the old configuration entirely when the output owns it.
        inherit_preferences(&source, &output, "Custom").unwrap();
        assert_eq!(fs::read(table).unwrap(), before);
    }
    #[test]
    fn versions_update_metadata_and_reject_stale_or_invalid_writes() {
        let s = Scratch::new();
        for name in ["LiteHub", "BoHub", "NullHub"] {
            let root = base(&s.0, name);
            let current = settings(&root, name).unwrap().unwrap();
            assert_eq!(current.waypoints_supported, name != "NullHub");
            for bad in ["", "0", "-1", "93854.1", "../file", "4294967296"] {
                assert!(save(&root, name, bad, &current.etag).is_err());
            }
            let next = save(&root, name, "100001", &current.etag).unwrap();
            assert_eq!(next.game_data_version, "100001");
            assert!(save(&root, name, "93854", &current.etag).is_err());
            let m = json(&root.join("enhancement-manifest.json")).unwrap();
            assert_eq!(m["files"][VERSION]["sha256"], hash(b"100001"));
            assert_eq!(m["generated_bytes"], 6);
            assert_eq!(
                json(&root.join("mod-version.json")).unwrap()["game_data_version"],
                "100001"
            );
            assert!(!root.join(JOURNAL).exists());
        }
    }

    #[test]
    fn automatic_version_changes_keep_managed_resources_updatable() {
        let scratch = Scratch::new();
        let root = base(&scratch.0, "NullHub");
        let receipt = crate::resource_install::Receipt {
            id: "NullHub".into(),
            version: "r1".into(),
            sequence: 1,
            sha256: "a".repeat(64),
            tree_sha256: crate::resource_install::tree_hash(&root).unwrap(),
        };
        crate::downloads::save_json(&root.join(crate::resource_install::RECEIPT), &receipt)
            .unwrap();
        let before = settings(&root, "NullHub").unwrap().unwrap();
        save(&root, "NullHub", "100001", &before.etag).unwrap();
        let receipt = crate::resource_install::read_receipt(&root)
            .unwrap()
            .unwrap();
        assert_eq!(
            receipt.tree_sha256,
            crate::resource_install::tree_hash(&root).unwrap()
        );
        assert!(!root.join(JOURNAL).exists());
    }
    #[test]
    fn verified_descendants_inherit_without_granting_unrelated_mods_access() {
        let s = Scratch::new();
        for base_name in ["LiteHub", "BoHub", "NullHub"] {
            let original = base(&s.0, base_name);
            let name = format!("{base_name}-Custom");
            let output = derived(&s.0, &name, base_name);
            let recovered = identity(&s.0, &name).unwrap().unwrap();
            assert_eq!(recovered.profile, profile(base_name).unwrap());
            assert_eq!(recovered.waypoints, base_name != "NullHub");
            inherit(Some(&original), &output, &name).unwrap();
            fs::rename(&original, s.0.join(format!("{base_name}-removed"))).unwrap();
            let current = settings(&output, &name).unwrap().unwrap();
            assert_eq!(current.base_mod, base_name);
            save(&output, &name, "93855", &current.etag).unwrap();
            let mut fake = json(&output.join(ORIGIN)).unwrap();
            fake["profile"] = "invalid".into();
            put(&output.join(ORIGIN), fake);
            assert!(identity(&s.0, &name).is_err());
        }
        let other = derived(&s.0, "Other", "jcy");
        assert!(settings(&other, "Other").unwrap().is_none());
        assert!(save(&other, "Other", "93855", "anything").is_err());
        assert!(safe_root(&s.0, "../LiteHub").is_err());
    }
    #[test]
    fn interrupted_version_edit_restores_all_fixed_files() {
        let s = Scratch::new();
        let root = base(&s.0, "LiteHub");
        let before = snapshot(&root, "LiteHub").unwrap();
        crate::downloads::save_json(&root.join(JOURNAL), &before).unwrap();
        fs::write(version_path(&root, "LiteHub"), "99999").unwrap();
        let interrupted = snapshot(&root, "LiteHub").unwrap();
        let journal = fs::read(root.join(JOURNAL)).unwrap();
        let blocked = recover_with_guard(&root, "LiteHub", || Err("game running".into()));
        assert_eq!(blocked.unwrap_err(), "game running");
        assert_eq!(snapshot(&root, "LiteHub").unwrap(), interrupted);
        assert_eq!(fs::read(root.join(JOURNAL)).unwrap(), journal);
        recover_with_guard(&root, "LiteHub", || Ok(())).unwrap();
        assert_eq!(snapshot(&root, "LiteHub").unwrap(), before);
        let mut invalid = before.clone();
        invalid.insert("../outside".into(), vec![]);
        crate::downloads::save_json(&root.join(JOURNAL), &invalid).unwrap();
        assert!(recover_with_guard(&root, "LiteHub", || Ok(())).is_err());
        assert_eq!(snapshot(&root, "LiteHub").unwrap(), before);
    }

    #[test]
    #[ignore = "Set D2RHUB_SETTINGS_GAME and D2RHUB_SETTINGS_PROCESSOR; creates only temporary processed outputs"]
    fn real_beta18_descendants_keep_settings_and_processing_credentials() {
        let game = PathBuf::from(std::env::var_os("D2RHUB_SETTINGS_GAME").unwrap());
        let processor = std::env::var_os("D2RHUB_SETTINGS_PROCESSOR").unwrap();
        let s = Scratch::new();
        for base_name in ["LiteHub", "BoHub", "NullHub"] {
            let source = game.join("mods").join(base_name);
            let name = format!("{base_name}SettingsTest");
            let result = std::process::Command::new(&processor)
                .args(["augment", "--game"])
                .arg(&game)
                .arg("--source")
                .arg(&source)
                .arg("--output")
                .arg(&s.0)
                .args(["--name", &name, "--features", "rooms", "--events"])
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let output = s.0.join(&name);
            inherit(Some(&source), &output, &name).unwrap();
            let current = settings(&output, &name).unwrap().unwrap();
            assert_eq!(current.base_mod, base_name);
            save(&output, &name, "93855", &current.etag).unwrap();
            assert_eq!(
                crate::audio_mod::settings_source(&s.0, &name)
                    .unwrap()
                    .as_deref(),
                Some(base_name)
            );
            if base_name != "NullHub" {
                let waypoints = crate::mod_waypoints::read_config(&output, &name)
                    .unwrap()
                    .unwrap();
                let blank = vec![String::new(); 6];
                let saved =
                    crate::mod_waypoints::save(&output, &name, &blank, &waypoints.etag).unwrap();
                assert_eq!(saved.selected, blank);
                assert!(
                    settings(&output, &name)
                        .unwrap()
                        .unwrap()
                        .waypoints_supported
                );
            }
            let found = crate::audio_mod::installed_mods(&s.0)
                .into_iter()
                .find(|m| m.name == name)
                .unwrap();
            assert!(found
                .feature_groups
                .iter()
                .any(|g| g == "in_game_room_tools"));
            assert!(!found.update_required);
        }
    }
}
