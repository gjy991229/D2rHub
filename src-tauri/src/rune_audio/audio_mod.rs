use super::catalog::{
    AreaCatalogEntry, AreaCatalogFile, LocationKind, TelemetryMarker, AREA_CATALOG_FILE_NAME,
    MAX_AREA_ID, RUNE_COUNT,
};
use super::flac::{decode_flac, encode_flac, resample_interleaved_i32};
use super::protocol::{
    detect_markers, embed_marker, embed_marker_with_delay, interleaved_i32_to_mono,
    rune_marker_delay_seconds, MarkerConfig, MIN_SAMPLE_RATE, PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::Manager;

const MOD_NAME: &str = "D2RHubAudioCountessV43";
const SOUND_ENVIRON_FALLBACK_URL: &str = "https://raw.githubusercontent.com/pinkufairy/D2R-Excel/1f16064e09b97e3e65abd6943662207cff00b07f/soundenviron.txt";
const COUNTESS_AREA_IDS: [u32; 8] = [1, 6, 20, 21, 22, 23, 24, 25];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildAudioModRequest {
    pub source_directory: String,
    pub output_directory: Option<String>,
    pub sound_environment_file: Option<String>,
    pub gain_db: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioModAsset {
    pub marker: TelemetryMarker,
    pub label: String,
    pub sound: String,
    pub relative_path: String,
    pub source_audio: Option<String>,
    pub preserved_source_audio: bool,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildAudioModReport {
    pub protocol_version: u8,
    pub mod_directory: String,
    pub mpq_directory: String,
    pub source_excel_directory: String,
    pub source_mod_copied: bool,
    pub sound_environment_source: String,
    pub launch_arguments: String,
    pub rune_assets: Vec<AudioModAsset>,
    pub area_assets: Vec<AudioModAsset>,
    pub area_catalog: Vec<AreaCatalogEntry>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct TsvTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl TsvTable {
    fn parse(name: &str, text: &str) -> Result<Self, String> {
        let normalized = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut lines = normalized.lines();
        let header = lines
            .next()
            .ok_or_else(|| format!("{name} 是空文件"))?
            .trim_end_matches('\r');
        let headers = header.split('\t').map(str::to_string).collect::<Vec<_>>();
        if headers.is_empty() {
            return Err(format!("{name} 缺少表头"));
        }
        let rows = lines
            .filter_map(|line| {
                let line = line.trim_end_matches('\r');
                (!line.is_empty()).then(|| {
                    let mut row = line.split('\t').map(str::to_string).collect::<Vec<_>>();
                    row.resize(headers.len(), String::new());
                    row.truncate(headers.len());
                    row
                })
            })
            .collect();
        Ok(Self { headers, rows })
    }

    fn column(&self, name: &str) -> Result<usize, String> {
        self.headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("数据表缺少列“{name}”"))
    }

    fn set(&self, row: &mut [String], name: &str, value: impl Into<String>) -> Result<(), String> {
        row[self.column(name)?] = value.into();
        Ok(())
    }

    #[cfg(test)]
    fn get<'a>(&self, row: &'a [String], name: &str) -> Option<&'a str> {
        self.column(name)
            .ok()
            .and_then(|index| row.get(index))
            .map(String::as_str)
    }

    fn row_by(&self, column: &str, value: &str) -> Result<Vec<String>, String> {
        let index = self.column(column)?;
        self.rows
            .iter()
            .find(|row| row[index].eq_ignore_ascii_case(value))
            .cloned()
            .ok_or_else(|| format!("数据表中找不到 {column}={value} 的模板行"))
    }

    fn max_number(&self, column: &str) -> Result<u32, String> {
        let index = self.column(column)?;
        self.rows
            .iter()
            .filter_map(|row| row[index].trim().parse::<u32>().ok())
            .max()
            .ok_or_else(|| format!("列“{column}”中没有有效数字"))
    }

    fn to_text(&self) -> String {
        let mut output = String::new();
        output.push_str(&self.headers.join("\t"));
        output.push_str("\r\n");
        for row in &self.rows {
            output.push_str(&row.join("\t"));
            output.push_str("\r\n");
        }
        output
    }
}

#[derive(Debug)]
struct SourceLayout {
    excel: PathBuf,
    mpq: Option<PathBuf>,
}

type SoundDefinition = (TelemetryMarker, String, String);

struct StagingDirectory {
    path: PathBuf,
    parent: PathBuf,
    committed: bool,
}

impl StagingDirectory {
    fn create(output_parent: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(output_parent).map_err(|error| {
            format!("创建 Mod 输出目录失败 {}: {error}", output_parent.display())
        })?;
        let parent = std::fs::canonicalize(output_parent).map_err(|error| {
            format!("解析 Mod 输出目录失败 {}: {error}", output_parent.display())
        })?;
        let path = parent.join(format!(".{MOD_NAME}.building-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path)
            .map_err(|error| format!("创建临时 Mod 目录失败 {}: {error}", path.display()))?;
        Ok(Self {
            path,
            parent,
            committed: false,
        })
    }

    fn commit(mut self, target: &Path) -> Result<(), String> {
        if target.exists() {
            return Err(format!("输出 Mod 已存在，拒绝覆盖: {}", target.display()));
        }
        std::fs::rename(&self.path, target).map_err(|error| {
            format!(
                "提交生成的 Mod 失败 {} -> {}: {error}",
                self.path.display(),
                target.display()
            )
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if self.committed || self.path.parent() != Some(self.parent.as_path()) {
            return;
        }
        if !self
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with(&format!(".{MOD_NAME}.building-")))
        {
            return;
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn has_excel_files(path: &Path) -> bool {
    path.join("misc.txt").is_file()
        && path.join("sounds.txt").is_file()
        && path.join("levels.txt").is_file()
}

fn find_source_layout(source: &Path) -> Result<SourceLayout, String> {
    if has_excel_files(source) {
        let mpq = source.ancestors().find(|ancestor| {
            ancestor
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("mpq"))
        });
        return Ok(SourceLayout {
            excel: source.to_path_buf(),
            mpq: mpq.map(Path::to_path_buf),
        });
    }
    let direct = source.join("data/global/excel");
    if has_excel_files(&direct) {
        return Ok(SourceLayout {
            excel: direct,
            mpq: Some(source.to_path_buf()),
        });
    }
    if source.is_dir() {
        for entry in std::fs::read_dir(source)
            .map_err(|error| format!("读取源目录失败 {}: {error}", source.display()))?
        {
            let path = entry
                .map_err(|error| format!("读取源目录项失败: {error}"))?
                .path();
            let candidate = path.join("data/global/excel");
            if path.is_dir() && has_excel_files(&candidate) {
                return Ok(SourceLayout {
                    excel: candidate,
                    mpq: Some(path),
                });
            }
        }
    }
    Err(format!(
        "在 {} 中找不到 data/global/excel/misc.txt、sounds.txt、levels.txt",
        source.display()
    ))
}

fn read_utf8(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("读取失败 {}: {error}", path.display()))
}

fn set_if_present(table: &TsvTable, row: &mut [String], name: &str, value: &str) {
    if let Ok(index) = table.column(name) {
        row[index] = value.to_string();
    }
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target)
        .map_err(|error| format!("创建目录失败 {}: {error}", target.display()))?;
    for entry in std::fs::read_dir(source)
        .map_err(|error| format!("读取源 Mod 失败 {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("读取源 Mod 项失败: {error}"))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("读取源 Mod 项类型失败 {}: {error}", source_path.display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "源 Mod 含符号链接/目录联接，拒绝递归复制: {}",
                source_path.display()
            ));
        }
        if file_type.is_dir() {
            copy_directory(&source_path, &target_path)?;
        } else {
            std::fs::copy(&source_path, &target_path).map_err(|error| {
                format!(
                    "复制源 Mod 资源失败 {} -> {}: {error}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn write_file(path: &Path, content: impl AsRef<[u8]>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建目录失败 {}: {error}", parent.display()))?;
    }
    std::fs::write(path, content).map_err(|error| format!("写入失败 {}: {error}", path.display()))
}

fn validate_misc(table: &TsvTable) -> Result<(), String> {
    let code_index = table.column("code")?;
    for rune_number in 1..=RUNE_COUNT {
        let code = format!("r{rune_number:02}");
        table
            .rows
            .iter()
            .find(|row| row[code_index].eq_ignore_ascii_case(&code))
            .ok_or_else(|| format!("misc.txt 缺少符文代码 {code}"))?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum SoundRole {
    RuneFlippy,
    AreaAmbience,
}

fn configure_sound_row(table: &TsvTable, row: &mut [String], role: SoundRole) {
    let is_ambience = matches!(role, SoundRole::AreaAmbience);
    for (column, value) in [
        ("Redirect", ""),
        ("Volume Min", "255"),
        ("Volume Max", "255"),
        ("Pitch Min", "100"),
        ("Pitch Max", "100"),
        ("Group Size", "0"),
        ("Group Weight", "0"),
        ("Loop", if is_ambience { "1" } else { "0" }),
        ("Duration", "0"),
        ("Delay", "0"),
        ("Defer Inst", "0"),
        ("Stop Inst", "0"),
        ("Compound", "0"),
        ("Stream", "0"),
        ("Tracking", "0"),
        ("Is2D", "1"),
        ("IsAmbientScene", if is_ambience { "1" } else { "0" }),
        ("IsAmbientEvent", "0"),
    ] {
        set_if_present(table, row, column, value);
    }
}

fn rune_unit_definition_candidates(mpq_directory: &Path, rune_number: u32) -> Vec<PathBuf> {
    let rune_name =
        crate::rune_data::RUNE_NAMES_EN[(rune_number - 1) as usize].to_ascii_lowercase();
    ["rune", "runes"]
        .into_iter()
        .map(|folder| {
            mpq_directory.join(format!("data/hd/items/misc/{folder}/{rune_name}_rune.json"))
        })
        .collect()
}

fn flippy_state_machine_document(
    original_state_machine: &str,
    rune_number: u32,
) -> Result<serde_json::Value, String> {
    let normalized_state_machine = original_state_machine.replace('\\', "/");
    let file_name = normalized_state_machine
        .rsplit('/')
        .next()
        .and_then(|value| value.strip_suffix(".json"))
        .filter(|value| {
            !value.is_empty()
                && value
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character == '_')
        })
        .ok_or_else(|| format!("无法解析物品落地状态机路径: {original_state_machine}"))?;
    let animation = format!("data/hd/items/dropped_items/animation/{file_name}.animation");
    let machine_name = format!("d2rhub_r{rune_number:02}_flippy");
    Ok(serde_json::json!({
        "dependencies": {
            "particles": [],
            "models": [],
            "skeletons": [],
            "animations": [{ "path": animation }],
            "textures": [],
            "physics": [],
            "json": [],
            "variantdata": [],
            "objecteffects": [],
            "other": []
        },
        "type": "AnimationStateMachine",
        "name": machine_name,
        "unitType": "UNIT_OBJECT",
        "animations": [{
            "type": "AnimationItem",
            "name": format!("{machine_name}001"),
            "filename": animation
        }],
        "states": [{
            "type": "AnimationState",
            "name": "AnimationState",
            "_name": "Flippy",
            "audioId": format!("d2rhub_audio_r{rune_number:02}"),
            "loopCount": 1,
            "stateId": 1,
            "enableVfxAttributes": false,
            "modeId": 5,
            "skillIndex": -1,
            "stepIndex": 0,
            "animationBindings": { "hth": [format!("{machine_name}001")] },
            "enterEvents": [],
            "exitEvents": [],
            "exitBlendType": 0
        }, {
            "type": "AnimationState",
            "name": "AnimationState001",
            "_name": "Ground",
            "audioId": "",
            "loopCount": 1,
            "stateId": 2,
            "enableVfxAttributes": false,
            "modeId": 3,
            "skillIndex": -1,
            "stepIndex": 0,
            "animationBindings": { "hth": [format!("{machine_name}001")] },
            "enterEvents": [],
            "exitEvents": [],
            "exitBlendType": 0
        }],
        "transitions": [{
            "type": "AnimationTransitionGroup",
            "name": "AnimationState_transitiongroup",
            "from": 1,
            "settings": [{
                "type": "AnimationTransitionItem",
                "name": "AnimationState_transitiongroup_transition",
                "crossfadeSeconds": 0.2,
                "to": 2
            }]
        }]
    }))
}

fn patch_rune_unit_definitions(mpq_directory: &Path) -> Result<usize, String> {
    let mut patched = 0usize;
    for rune_number in 1..=RUNE_COUNT {
        let path = rune_unit_definition_candidates(mpq_directory, rune_number)
            .into_iter()
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                format!(
                    "源 Mod 缺少 #{rune_number:02} {} 的 HD 地面实体 JSON；无法可靠监听地面掉落",
                    crate::rune_data::RUNE_NAMES_EN[(rune_number - 1) as usize]
                )
            })?;
        let text = read_utf8(&path)?;
        let mut document: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("解析 HD 符文实体失败 {}: {error}", path.display()))?;
        let entities = document
            .get_mut("entities")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| format!("HD 符文实体缺少 entities 数组: {}", path.display()))?;
        let root = entities
            .iter_mut()
            .find(|entity| {
                entity.get("name").and_then(serde_json::Value::as_str) == Some("entity_root")
            })
            .ok_or_else(|| format!("HD 符文实体缺少 entity_root: {}", path.display()))?;
        let components = root
            .get_mut("components")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| format!("HD 符文 entity_root 缺少 components: {}", path.display()))?;
        // An entity-level emitter also fires when the inventory renderer
        // instantiates the unit.  Only the Flippy animation state is specific
        // to a world item being dropped.
        components.retain(|component| {
            !(component.get("type").and_then(serde_json::Value::as_str)
                == Some("AudioEmitterComponent")
                && component
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| name.starts_with("D2RHub_Rune_")))
        });
        let unit_root = components
            .iter_mut()
            .find(|component| {
                component.get("type").and_then(serde_json::Value::as_str)
                    == Some("UnitRootComponent")
            })
            .ok_or_else(|| {
                format!(
                    "HD 符文 entity_root 缺少 UnitRootComponent: {}",
                    path.display()
                )
            })?;
        let original_state_machine = unit_root
            .get("state_machine_filename")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("HD 符文缺少落地状态机路径: {}", path.display()))?
            .to_string();
        let telemetry_state_machine =
            format!("data/hd/items/d2rhub_audio/runes/r{rune_number:02}_flippy.json");
        unit_root["state_machine_filename"] =
            serde_json::Value::String(telemetry_state_machine.clone());

        let dependencies = document
            .get_mut("dependencies")
            .and_then(|value| value.get_mut("json"))
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| format!("HD 符文缺少 dependencies.json: {}", path.display()))?;
        if let Some(reference) = dependencies.iter_mut().find(|reference| {
            reference.get("path").and_then(serde_json::Value::as_str)
                == Some(original_state_machine.as_str())
        }) {
            reference["path"] = serde_json::Value::String(telemetry_state_machine.clone());
        } else {
            dependencies.push(serde_json::json!({ "path": telemetry_state_machine }));
        }

        let state_machine = flippy_state_machine_document(&original_state_machine, rune_number)?;
        write_file(
            &mpq_directory.join(telemetry_state_machine.replace('/', "\\")),
            serde_json::to_vec_pretty(&state_machine)
                .map_err(|error| format!("序列化符文落地状态机失败: {error}"))?,
        )?;
        write_file(
            &path,
            serde_json::to_vec_pretty(&document)
                .map_err(|error| format!("序列化 HD 符文实体失败: {error}"))?,
        )?;
        patched += 1;
    }
    Ok(patched)
}

fn patch_sounds(
    mut table: TsvTable,
    areas: &[AreaCatalogEntry],
) -> Result<(TsvTable, Vec<SoundDefinition>), String> {
    let mut next_index = table.max_number("*Index")? + 1;
    let rune_template = table
        .row_by("Sound", "item_rune_hd")
        .or_else(|_| table.row_by("Sound", "item_rune"))?;
    let area_template = table
        .row_by("Sound", "wilderness_day_2_hd")
        .or_else(|_| table.row_by("Sound", "scene_wilderness_day"))
        .or_else(|_| {
            let ambient_index = table.column("IsAmbientScene")?;
            table
                .rows
                .iter()
                .find(|row| row[ambient_index] == "1")
                .cloned()
                .ok_or_else(|| "sounds.txt 中找不到持续环境音模板".to_string())
        })?;
    let mut definitions = Vec::with_capacity(RUNE_COUNT as usize + areas.len());

    for rune_number in 1..=RUNE_COUNT {
        let marker = TelemetryMarker::Rune { rune_number };
        let sound = format!("d2rhub_audio_r{rune_number:02}");
        let relative_path = format!("d2rhub_audio\\runes\\r{rune_number:02}.flac");
        let mut row = rune_template.clone();
        table.set(&mut row, "Sound", &sound)?;
        table.set(&mut row, "*Index", next_index.to_string())?;
        table.set(&mut row, "FileName", &relative_path)?;
        configure_sound_row(&table, &mut row, SoundRole::RuneFlippy);
        table.rows.push(row);
        definitions.push((marker, sound, relative_path));
        next_index += 1;
    }

    for area in areas {
        let marker = TelemetryMarker::Area {
            area_id: area.area_id,
        };
        let sound = format!("d2rhub_audio_a{}", area.area_id);
        let relative_path = format!("d2rhub_audio\\areas\\a{}.flac", area.area_id);
        let mut row = area_template.clone();
        table.set(&mut row, "Sound", &sound)?;
        table.set(&mut row, "*Index", next_index.to_string())?;
        table.set(&mut row, "FileName", &relative_path)?;
        configure_sound_row(&table, &mut row, SoundRole::AreaAmbience);
        table.rows.push(row);
        definitions.push((marker, sound, relative_path));
        next_index += 1;
    }
    Ok((table, definitions))
}

fn sanitize_scene_key(value: &str, area_id: u32) -> String {
    let mut output = String::new();
    let mut underscore = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            underscore = false;
        } else if !underscore && !output.is_empty() {
            output.push('_');
            underscore = true;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        format!("area_{area_id}")
    } else {
        format!("area_{area_id}_{output}")
    }
}

fn collect_areas(levels: &TsvTable) -> Result<Vec<AreaCatalogEntry>, String> {
    let id = levels.column("Id")?;
    let name = levels.column("Name")?;
    let level_name = levels.column("LevelName").unwrap_or(name);
    let mut areas = levels
        .rows
        .iter()
        .filter_map(|row| {
            let area_id = row[id].trim().parse::<u32>().ok()?;
            if !(1..=MAX_AREA_ID).contains(&area_id) {
                return None;
            }
            let internal_name = row[name].trim();
            let display_name = row[level_name].trim();
            if internal_name.is_empty() || internal_name.eq_ignore_ascii_case("null") {
                return None;
            }
            let scene_name = if display_name.is_empty() {
                internal_name
            } else {
                display_name
            };
            let kind = if internal_name.to_ascii_lowercase().contains(" - town")
                || matches!(area_id, 1 | 40 | 75 | 103 | 109)
            {
                LocationKind::Town
            } else {
                LocationKind::Wilderness
            };
            Some(AreaCatalogEntry {
                area_id,
                scene_key: sanitize_scene_key(scene_name, area_id),
                scene_name: scene_name.to_string(),
                scene_name_en: scene_name.to_string(),
                kind,
            })
        })
        .collect::<Vec<_>>();
    areas.sort_by_key(|area| area.area_id);
    areas.dedup_by_key(|area| area.area_id);
    if areas.is_empty() {
        return Err("levels.txt 中没有可编码的 Area Id".to_string());
    }
    Ok(areas)
}

fn patch_sound_environ_and_levels(
    mut environments: TsvTable,
    mut levels: TsvTable,
    areas: &[AreaCatalogEntry],
) -> Result<(TsvTable, TsvTable), String> {
    let level_id = levels.column("Id")?;
    let level_environment = levels.column("SoundEnv")?;
    let environment_index = environments.column("Index")?;
    let mut next_index = environments.max_number("Index")? + 1;
    let area_lookup = areas
        .iter()
        .map(|area| (area.area_id, area))
        .collect::<HashMap<_, _>>();

    for level_row in &mut levels.rows {
        let Some(area_id) = level_row[level_id].trim().parse::<u32>().ok() else {
            continue;
        };
        if !area_lookup.contains_key(&area_id) {
            continue;
        }
        let original_id = level_row[level_environment].trim();
        let mut row = environments
            .rows
            .iter()
            .find(|row| row[environment_index].trim() == original_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "soundenviron.txt 缺少 levels.txt Area {area_id} 使用的 Index={original_id}"
                )
            })?;
        environments.set(
            &mut row,
            "Handle",
            format!("ESOUNDENVIRON_D2RHUB_AUDIO_A{area_id}"),
        )?;
        environments.set(&mut row, "Index", next_index.to_string())?;
        let sound = format!("d2rhub_audio_a{area_id}");
        for column in [
            "Day Ambience",
            "HD Day Ambience",
            "Night Ambience",
            "HD Night Ambience",
        ] {
            environments.set(&mut row, column, &sound)?;
        }
        environments.rows.push(row);
        level_row[level_environment] = next_index.to_string();
        next_index += 1;
    }
    Ok((environments, levels))
}

fn resolve_sound_filename(sounds: &TsvTable, sound_name: &str) -> Option<String> {
    let sound_column = sounds.column("Sound").ok()?;
    let filename_column = sounds.column("FileName").ok()?;
    let redirect_column = sounds.column("Redirect").ok();
    let mut current = sound_name.to_string();
    for _ in 0..8 {
        let row = sounds
            .rows
            .iter()
            .find(|row| row[sound_column].eq_ignore_ascii_case(&current))?;
        if let Some(redirect) = redirect_column
            .and_then(|index| row.get(index))
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            current = redirect.to_string();
            continue;
        }
        return Some(row[filename_column].clone());
    }
    None
}

fn collect_area_ambience_filenames(
    levels: &TsvTable,
    environments: &TsvTable,
    sounds: &TsvTable,
    areas: &[AreaCatalogEntry],
) -> Result<HashMap<u32, String>, String> {
    let level_id = levels.column("Id")?;
    let level_environment = levels.column("SoundEnv")?;
    let environment_index = environments.column("Index")?;
    let hd_day = environments.column("HD Day Ambience")?;
    let day = environments.column("Day Ambience")?;
    let hd_night = environments.column("HD Night Ambience")?;
    let night = environments.column("Night Ambience")?;
    let mut output = HashMap::new();
    for area in areas {
        let level = levels
            .rows
            .iter()
            .find(|row| row[level_id].trim() == area.area_id.to_string())
            .ok_or_else(|| format!("levels.txt 缺少 Area {}", area.area_id))?;
        let environment = environments
            .rows
            .iter()
            .find(|row| row[environment_index].trim() == level[level_environment].trim())
            .ok_or_else(|| format!("找不到 Area {} 的 SoundEnv", area.area_id))?;
        let sound_name = [hd_day, day, hd_night, night]
            .into_iter()
            .map(|column| environment[column].trim())
            .find(|value| !value.is_empty())
            .ok_or_else(|| format!("Area {} 没有持续环境音定义", area.area_id))?;
        let filename = resolve_sound_filename(sounds, sound_name).ok_or_else(|| {
            format!(
                "无法从 sounds.txt 解析 Area {} 的持续环境音 {sound_name}",
                area.area_id
            )
        })?;
        output.insert(area.area_id, filename);
    }
    Ok(output)
}

fn find_game_storage_root(source: &Path, output_parent: &Path) -> Option<PathBuf> {
    [source, output_parent]
        .into_iter()
        .flat_map(Path::ancestors)
        .find(|candidate| {
            candidate.join(".build.info").is_file() && candidate.join("Data").is_dir()
        })
        .map(Path::to_path_buf)
}

fn extract_area_ambiences_from_casc(
    game_root: &Path,
    filenames: &HashMap<u32, String>,
    cache_directory: &Path,
) -> Result<HashMap<u32, PathBuf>, String> {
    let storage = casc_core::Storage::open(game_root)
        .map_err(|error| format!("打开 D2R CASC 失败 {}: {error}", game_root.display()))?;
    let mut output = HashMap::new();
    for (&area_id, filename) in filenames {
        let casc_path = format!(
            "data:data\\hd\\global\\sfx\\{}",
            filename.replace('/', "\\")
        );
        let bytes = storage.read(&casc_path).map_err(|error| {
            format!("从 D2R CASC 读取 Area {area_id} 环境音失败 {casc_path}: {error}")
        })?;
        let path = cache_directory.join(format!("a{area_id}.flac"));
        write_file(&path, bytes)?;
        output.insert(area_id, path);
    }
    Ok(output)
}

fn source_audio_path(mpq: Option<&Path>, filename: &str) -> Option<PathBuf> {
    let root = mpq?;
    let relative = filename.replace('\\', "/");
    let candidates = [
        root.join("data/hd/global/sfx").join(&relative),
        root.join("data/global/sfx").join(&relative),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

fn resolve_rune_sources(
    mpq: Option<&Path>,
    misc: &TsvTable,
    sounds: &TsvTable,
) -> Result<HashMap<u32, PathBuf>, String> {
    let code = misc.column("code")?;
    let drop_sound = misc.column("dropsound")?;
    let mut output = HashMap::new();
    for rune_number in 1..=RUNE_COUNT {
        let code_value = format!("r{rune_number:02}");
        let table_source = misc
            .rows
            .iter()
            .find(|row| row[code].eq_ignore_ascii_case(&code_value))
            .and_then(|row| resolve_sound_filename(sounds, &row[drop_sound]))
            .and_then(|filename| source_audio_path(mpq, &filename));
        let direct_source = mpq
            .map(|root| root.join(format!("data/hd/global/sfx/item/r{rune_number:02}.flac")))
            .filter(|path| path.is_file());
        if let Some(path) = direct_source.or(table_source) {
            output.insert(rune_number, path);
        }
    }
    Ok(output)
}

fn write_marker_flac(
    path: &Path,
    marker: TelemetryMarker,
    source_audio: Option<&Path>,
    config: MarkerConfig,
) -> Result<f32, String> {
    let (mut samples, mut sample_rate, channels, bits_per_sample) =
        if let Some(source) = source_audio {
            decode_flac(source)?
        } else {
            (vec![0i32; 48_000 / 3 * 2], 48_000, 2, 16)
        };
    if sample_rate < MIN_SAMPLE_RATE {
        samples = resample_interleaved_i32(&samples, channels as usize, sample_rate, 48_000);
        sample_rate = 48_000;
    }
    let periodic_area = matches!(marker, TelemetryMarker::Area { .. }) && source_audio.is_some();
    let mut expected_detections = 1usize;
    if periodic_area {
        let interval_samples = sample_rate as usize * 5 * channels as usize;
        let mut embedded_count = 0usize;
        for chunk in samples.chunks_mut(interval_samples) {
            if chunk.len() < interval_samples {
                break;
            }
            let mut marker_chunk = chunk.to_vec();
            embed_marker(
                &mut marker_chunk,
                channels as usize,
                bits_per_sample,
                sample_rate,
                marker,
                config,
            )?;
            chunk.copy_from_slice(&marker_chunk);
            embedded_count += 1;
        }
        if embedded_count == 0 {
            embed_marker(
                &mut samples,
                channels as usize,
                bits_per_sample,
                sample_rate,
                marker,
                config,
            )?;
        } else {
            expected_detections = embedded_count;
        }
    } else {
        match marker {
            TelemetryMarker::Rune { rune_number } => embed_marker_with_delay(
                &mut samples,
                channels as usize,
                bits_per_sample,
                sample_rate,
                marker,
                config,
                rune_marker_delay_seconds(rune_number)?,
            )?,
            TelemetryMarker::Area { .. } | TelemetryMarker::Frontend => embed_marker(
                &mut samples,
                channels as usize,
                bits_per_sample,
                sample_rate,
                marker,
                config,
            )?,
        };
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建音频目录失败 {}: {error}", parent.display()))?;
    }
    encode_flac(path, &samples, sample_rate, channels, bits_per_sample)?;
    let (verified, rate, verified_channels, verified_bits) = decode_flac(path)?;
    let mono = interleaved_i32_to_mono(&verified, verified_channels as usize, verified_bits)?;
    let detections = detect_markers(&mono, rate, config.detection_threshold)
        .into_iter()
        .filter(|detection| detection.marker == marker)
        .collect::<Vec<_>>();
    let verified = detections.len() == expected_detections;
    if !verified {
        return Err(format!(
            "生成的 {:?} FLAC 自检失败：应识别 {expected_detections} 次，实际识别 {} 次（periodic={periodic_area}）",
            marker,
            detections.len()
        ));
    }
    Ok(detections
        .iter()
        .map(|detection| detection.confidence)
        .fold(f32::INFINITY, f32::min))
}

fn default_output_parent(source: &Path) -> PathBuf {
    source
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("D2RHub-Audio-Mod-Output")
}

fn asset_label(marker: TelemetryMarker, catalog: &[AreaCatalogEntry]) -> String {
    match marker {
        TelemetryMarker::Rune { rune_number } => {
            let rune = crate::rune_data::get_rune_name(rune_number).unwrap_or("未知符文");
            format!("#{rune_number:02} {rune}")
        }
        TelemetryMarker::Area { area_id } => catalog
            .iter()
            .find(|area| area.area_id == area_id)
            .map(|area| format!("{} (Area {area_id})", area.scene_name))
            .unwrap_or_else(|| format!("Area {area_id}")),
        TelemetryMarker::Frontend => "主界面".to_string(),
    }
}

fn build(
    app_data_dir: String,
    request: BuildAudioModRequest,
    downloaded_sound_environment: Option<String>,
) -> Result<BuildAudioModReport, String> {
    let source = PathBuf::from(request.source_directory.trim());
    let layout = find_source_layout(&source)?;
    let output_parent = request
        .output_directory
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_parent(&source));
    let final_mod_directory = output_parent.join(MOD_NAME);
    if final_mod_directory.exists() {
        return Err(format!(
            "输出 Mod 已存在，未覆盖: {}。请删除旧输出或选择另一个目录",
            final_mod_directory.display()
        ));
    }
    let staging = StagingDirectory::create(&output_parent)?;
    if let Some(source_mpq) = &layout.mpq {
        let canonical_source = std::fs::canonicalize(source_mpq)
            .map_err(|error| format!("解析源 Mod 目录失败 {}: {error}", source_mpq.display()))?;
        if staging.path.starts_with(&canonical_source) {
            return Err("输出目录不能位于源 .mpq 内部，避免递归复制".to_string());
        }
    }
    let staging_mod_directory = staging.path.clone();
    let mpq_directory = staging_mod_directory.join(format!("{MOD_NAME}.mpq"));
    let final_mpq_directory = final_mod_directory.join(format!("{MOD_NAME}.mpq"));
    let source_mod_copied = layout.mpq.is_some();
    if let Some(source_mpq) = &layout.mpq {
        copy_directory(source_mpq, &mpq_directory)?;
    } else {
        std::fs::create_dir_all(&mpq_directory)
            .map_err(|error| format!("创建 Mod 目录失败: {error}"))?;
    }
    let excel_output = mpq_directory.join("data/global/excel");

    let misc = TsvTable::parse("misc.txt", &read_utf8(&layout.excel.join("misc.txt"))?)?;
    let sounds = TsvTable::parse("sounds.txt", &read_utf8(&layout.excel.join("sounds.txt"))?)?;
    let levels = TsvTable::parse("levels.txt", &read_utf8(&layout.excel.join("levels.txt"))?)?;
    let rune_sources = resolve_rune_sources(layout.mpq.as_deref(), &misc, &sounds)?;
    let mut areas = collect_areas(&levels)?;
    areas.retain(|area| COUNTESS_AREA_IDS.contains(&area.area_id));
    if areas.len() != COUNTESS_AREA_IDS.len() {
        return Err("女伯爵实机版需要 levels.txt 包含 Area 1、6、20–25".to_string());
    }

    let explicit_sound_environment = request
        .sound_environment_file
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);
    let local_sound_environment = layout.excel.join("soundenviron.txt");
    let (sound_environment_text, sound_environment_source) = if let Some(path) =
        explicit_sound_environment
    {
        (read_utf8(&path)?, path.to_string_lossy().to_string())
    } else if local_sound_environment.is_file() {
        (
            read_utf8(&local_sound_environment)?,
            local_sound_environment.to_string_lossy().to_string(),
        )
    } else {
        (
            downloaded_sound_environment
                .ok_or_else(|| "源目录缺少 soundenviron.txt，且固定公共基线下载失败".to_string())?,
            SOUND_ENVIRON_FALLBACK_URL.to_string(),
        )
    };
    let environments = TsvTable::parse("soundenviron.txt", &sound_environment_text)?;
    let area_ambience_filenames =
        collect_area_ambience_filenames(&levels, &environments, &sounds, &areas)?;
    let casc_cache = staging_mod_directory.join(".d2rhub-casc-cache");
    let game_root = find_game_storage_root(&source, &output_parent);
    let area_sources = if let Some(game_root) = &game_root {
        extract_area_ambiences_from_casc(game_root, &area_ambience_filenames, &casc_cache)?
    } else {
        HashMap::new()
    };
    validate_misc(&misc)?;
    let (sounds, definitions) = patch_sounds(sounds, &areas)?;
    let (environments, levels) = patch_sound_environ_and_levels(environments, levels, &areas)?;
    let patched_rune_units = patch_rune_unit_definitions(&mpq_directory)?;

    write_file(&excel_output.join("misc.txt"), misc.to_text())?;
    write_file(&excel_output.join("sounds.txt"), sounds.to_text())?;
    write_file(&excel_output.join("levels.txt"), levels.to_text())?;
    write_file(
        &excel_output.join("soundenviron.txt"),
        environments.to_text(),
    )?;

    let config = MarkerConfig {
        gain_db: request.gain_db.unwrap_or(MarkerConfig::default().gain_db),
        ..MarkerConfig::default()
    }
    .validate()?;
    let mut rune_assets = Vec::new();
    let mut area_assets = Vec::new();
    for (marker, sound, relative_path) in definitions {
        let source_audio = match marker {
            TelemetryMarker::Rune { rune_number } => {
                rune_sources.get(&rune_number).map(PathBuf::as_path)
            }
            TelemetryMarker::Area { area_id } => area_sources.get(&area_id).map(PathBuf::as_path),
            TelemetryMarker::Frontend => None,
        };
        let output_path = mpq_directory
            .join("data/hd/global/sfx")
            .join(relative_path.replace('\\', "/"));
        let confidence = write_marker_flac(&output_path, marker, source_audio, config)?;
        let asset = AudioModAsset {
            marker,
            label: asset_label(marker, &areas),
            sound,
            relative_path,
            source_audio: match marker {
                TelemetryMarker::Area { area_id } => source_audio.and_then(|_| {
                    area_ambience_filenames
                        .get(&area_id)
                        .map(|filename| format!("CASC:{filename}"))
                }),
                TelemetryMarker::Rune { .. } | TelemetryMarker::Frontend => {
                    source_audio.map(|path| path.to_string_lossy().to_string())
                }
            },
            preserved_source_audio: source_audio.is_some(),
            confidence,
        };
        match marker {
            TelemetryMarker::Rune { .. } => rune_assets.push(asset),
            TelemetryMarker::Area { .. } => area_assets.push(asset),
            TelemetryMarker::Frontend => {}
        }
    }
    if casc_cache.is_dir() {
        std::fs::remove_dir_all(&casc_cache).map_err(|error| {
            format!("清理 CASC 环境音缓存失败 {}: {error}", casc_cache.display())
        })?;
    }

    write_file(
        &mpq_directory.join("modinfo.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "name": MOD_NAME,
            "author": "D2RHub audio telemetry v4",
            "savepath": "../",
        }))
        .map_err(|error| format!("生成 modinfo.json 失败: {error}"))?,
    )?;
    let catalog_file = AreaCatalogFile {
        protocol_version: PROTOCOL_VERSION,
        source_levels: layout
            .excel
            .join("levels.txt")
            .to_string_lossy()
            .to_string(),
        areas: areas.clone(),
    };
    let catalog_json = serde_json::to_vec_pretty(&catalog_file)
        .map_err(|error| format!("生成地图声纹目录失败: {error}"))?;
    write_file(
        &staging_mod_directory.join(AREA_CATALOG_FILE_NAME),
        &catalog_json,
    )?;

    let report = BuildAudioModReport {
        protocol_version: PROTOCOL_VERSION,
        mod_directory: final_mod_directory.to_string_lossy().to_string(),
        mpq_directory: final_mpq_directory.to_string_lossy().to_string(),
        source_excel_directory: layout.excel.to_string_lossy().to_string(),
        source_mod_copied,
        sound_environment_source,
        launch_arguments: format!("-mod {MOD_NAME} -txt"),
        rune_assets,
        area_assets,
        area_catalog: areas,
        notes: vec![
            format!(
                "已给 {patched_rune_units} 个符文建立单向 Flippy→Ground 状态机；每次世界掉落只播放一次，背包实体创建不触发。"
            ),
            "源 Mod 中能定位到的符文 FLAC 会保留原声并混入 v4 标记；缺失资源使用纯超声标记。"
                .to_string(),
            if let Some(game_root) = game_root {
                format!(
                    "女伯爵路线覆盖 Area {:?}；已从 {} 的 D2R CASC 提取真实持续环境音，混入 AreaId 后替换克隆 SoundEnv 的 Day/Night Ambience。",
                    COUNTESS_AREA_IDS,
                    game_root.display()
                )
            } else {
                format!(
                    "女伯爵路线覆盖 Area {:?}；未定位 D2R CASC，使用静默循环标记。",
                    COUNTESS_AREA_IDS
                )
            },
            "地点触发目标是场景切换时必然启动的持续 Ambience，不再依赖实机未执行的随机 Event。"
                .to_string(),
        ],
    };
    write_file(
        &staging_mod_directory.join("d2rhub-audio-manifest.json"),
        serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("生成音频 Mod 清单失败: {error}"))?,
    )?;
    write_file(
        &staging_mod_directory.join("README-安装与测试.txt"),
        format!(
            "D2RHub 女伯爵音频实机版 v4.3\r\n\r\n启动参数：{}\r\n\r\n1. 把整个 {} 文件夹放入 D2R 的 mods 目录。\r\n2. 账号启动参数启用上面的 -mod/-txt。\r\n3. 在 D2RHub 设置 → 自动化中选择目标账号并启动音频监控。\r\n4. 地点覆盖罗格营地、黑色荒地、遗忘之塔与高塔地牢 1–5 层。\r\n5. 符文只在 Flippy 世界落地动画播放一次；33 个符文使用不同延迟槽，降低同时爆落的碰撞。\r\n6. 游戏的“音效”通道必须非静音；D2RHub 捕获目标进程，不读取游戏内存、不注入 DLL。\r\n",
            report.launch_arguments, MOD_NAME
        ),
    )?;
    staging.commit(&final_mod_directory)?;
    if let Err(error) = write_file(
        &Path::new(&app_data_dir).join(AREA_CATALOG_FILE_NAME),
        &catalog_json,
    ) {
        crate::logger::log_msg(
            "WARN",
            "RuneAudio",
            &format!("Mod 已生成，但保存本机地图名称目录失败: {error}"),
        );
    }
    Ok(report)
}

#[tauri::command]
pub async fn build_rune_audio_mod(
    app: tauri::AppHandle,
    request: BuildAudioModRequest,
) -> Result<BuildAudioModReport, String> {
    let source = PathBuf::from(request.source_directory.trim());
    let layout = find_source_layout(&source)?;
    let needs_fallback = !layout.excel.join("soundenviron.txt").is_file()
        && request
            .sound_environment_file
            .as_deref()
            .is_none_or(|value| value.trim().is_empty());
    let downloaded = if needs_fallback {
        Some(
            reqwest::get(SOUND_ENVIRON_FALLBACK_URL)
                .await
                .map_err(|error| format!("下载固定版本 soundenviron.txt 失败: {error}"))?
                .error_for_status()
                .map_err(|error| format!("下载固定版本 soundenviron.txt 失败: {error}"))?
                .text()
                .await
                .map_err(|error| format!("读取下载的 soundenviron.txt 失败: {error}"))?,
        )
    } else {
        None
    };
    let app_data_dir = app
        .state::<crate::state::SharedState>()
        .app_data_dir
        .clone();
    tauri::async_runtime::spawn_blocking(move || build(app_data_dir, request, downloaded))
        .await
        .map_err(|error| format!("等待音频遥测 Mod 生成失败: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_rune_unit_definitions(mpq: &Path) {
        let directory = mpq.join("data/hd/items/misc/rune");
        std::fs::create_dir_all(&directory).unwrap();
        for (index, name) in crate::rune_data::RUNE_NAMES_EN.iter().enumerate() {
            let document = serde_json::json!({
                "dependencies": {
                    "json": [{
                        "path": "data/hd/items/dropped_items/dropped_items_helms_flip_ne.json"
                    }]
                },
                "type": "UnitDefinition",
                "name": format!("{}_rune", name.to_ascii_lowercase()),
                "entities": [{
                    "type": "Entity",
                    "name": "entity_root",
                    "id": 1000 + index,
                    "components": [{
                        "type": "UnitRootComponent",
                        "name": "component_root",
                        "state_machine_filename": "data/hd/items/dropped_items/dropped_items_helms_flip_ne.json"
                    }]
                }]
            });
            std::fs::write(
                directory.join(format!("{}_rune.json", name.to_ascii_lowercase())),
                serde_json::to_vec_pretty(&document).unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn patches_all_runes_and_countess_area_rows() {
        let mut misc = "name\tcode\tdropsound\tusesound\n".to_string();
        for number in 1..=33 {
            misc.push_str(&format!(
                "Rune {number}\tr{number:02}\titem_rune\titem_rune\n"
            ));
        }
        let patched = TsvTable::parse("misc", &misc).unwrap();
        validate_misc(&patched).unwrap();
        assert_eq!(
            patched.get(&patched.rows[0], "dropsound"),
            Some("item_rune")
        );
        assert_eq!(
            patched.get(&patched.rows[32], "dropsound"),
            Some("item_rune")
        );

        let levels = TsvTable::parse(
            "levels",
            "Name\tId\tSoundEnv\tLevelName\nNull\t0\t0\tNull\nAct 1 - Town\t1\t1\tRogue Encampment\nAct 1 - Wilderness 5\t6\t2\tBlack Marsh\nAct 1 - Crypt 1\t20\t2\tForgotten Tower\nAct 1 - Crypt 2\t21\t2\tTower Cellar Level 1\nAct 1 - Crypt 3\t22\t2\tTower Cellar Level 2\nAct 1 - Crypt 4\t23\t2\tTower Cellar Level 3\nAct 1 - Crypt 5\t24\t2\tTower Cellar Level 4\nAct 1 - Crypt 6\t25\t2\tTower Cellar Level 5\n",
        )
        .unwrap();
        let areas = collect_areas(&levels).unwrap();
        assert_eq!(areas.len(), 8);
        assert_eq!(areas[0].kind, LocationKind::Town);
        assert_eq!(areas[1].scene_name, "Black Marsh");
        let environments = TsvTable::parse(
            "soundenviron",
            "Handle\tIndex\tDay Ambience\tHD Day Ambience\tNight Ambience\tHD Night Ambience\tDay Event\tHD Day Event\tNight Event\tHD Night Event\tEvent Delay\tHD Event Delay\nTown\t1\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\ta\ta\ta\ta\t500\t500\nWild\t2\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\tb\tb\tb\tb\t500\t500\n",
        )
        .unwrap();
        let (environments, levels) =
            patch_sound_environ_and_levels(environments, levels, &areas).unwrap();
        assert_eq!(environments.rows.len(), 10);
        assert_eq!(levels.get(&levels.rows[1], "SoundEnv"), Some("3"));
        assert_eq!(levels.get(&levels.rows[2], "SoundEnv"), Some("4"));

        let sounds = TsvTable::parse(
            "sounds",
            "Sound\t*Index\tRedirect\tFileName\tIsAmbientScene\tIsAmbientEvent\tGroup Weight\tLoop\nitem_rune_hd\t10\t\titem\\rune.flac\t0\t0\t0\t0\nscene_wilderness_day\t11\t\tambient\\scene.flac\t1\t0\t0\t1\n",
        )
        .unwrap();
        let (sounds, _) = patch_sounds(sounds, &areas).unwrap();
        let area_row = sounds.row_by("Sound", "d2rhub_audio_a1").unwrap();
        assert_eq!(sounds.get(&area_row, "IsAmbientScene"), Some("1"));
        assert_eq!(sounds.get(&area_row, "Loop"), Some("1"));
    }

    #[test]
    fn generated_marker_flac_self_verifies() {
        let root = std::env::temp_dir().join(format!("d2rhub-audio-v4-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("a137.flac");
        let confidence = write_marker_flac(
            &path,
            TelemetryMarker::Area { area_id: 137 },
            None,
            MarkerConfig::default(),
        )
        .unwrap();
        assert!(path.is_file());
        assert!(confidence > 0.7);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_build_removes_only_its_transaction_directory() {
        let root = std::env::temp_dir().join(format!("d2rhub-audio-fail-{}", uuid::Uuid::new_v4()));
        let source = root.join("broken.mpq");
        let excel = source.join("data/global/excel");
        let output = root.join("mods");
        std::fs::create_dir_all(&excel).unwrap();
        std::fs::write(
            excel.join("misc.txt"),
            "name\tcode\tdropsound\nEl Rune\tr01\titem_rune_hd\n",
        )
        .unwrap();
        std::fs::write(
            excel.join("sounds.txt"),
            "Sound\t*Index\tRedirect\tFileName\tIsAmbientScene\nitem_rune_hd\t1\t\titem\\rune.flac\t0\nscene_wilderness_day\t2\t\tambient\\scene.flac\t1\n",
        )
        .unwrap();
        std::fs::write(
            excel.join("levels.txt"),
            "Name\tId\tSoundEnv\tLevelName\nAct 1 - Town\t1\t1\tRogue Encampment\nAct 1 - Wilderness 5\t6\t2\tBlack Marsh\nAct 1 - Crypt 1\t20\t2\tForgotten Tower\nAct 1 - Crypt 2\t21\t2\tTower Cellar Level 1\nAct 1 - Crypt 3\t22\t2\tTower Cellar Level 2\nAct 1 - Crypt 4\t23\t2\tTower Cellar Level 3\nAct 1 - Crypt 5\t24\t2\tTower Cellar Level 4\nAct 1 - Crypt 6\t25\t2\tTower Cellar Level 5\n",
        )
        .unwrap();
        std::fs::write(
            excel.join("soundenviron.txt"),
            "Handle\tIndex\tDay Ambience\tHD Day Ambience\tNight Ambience\tHD Night Ambience\nTown\t1\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\nWild\t2\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\n",
        )
        .unwrap();
        let error = build(
            root.join("config").to_string_lossy().to_string(),
            BuildAudioModRequest {
                source_directory: source.to_string_lossy().to_string(),
                output_directory: Some(output.to_string_lossy().to_string()),
                sound_environment_file: None,
                gain_db: None,
            },
            None,
        )
        .unwrap_err();
        assert!(error.contains("r02"));
        assert!(!output.join(MOD_NAME).exists());
        assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn builds_a_complete_mod_and_preserves_available_rune_audio() {
        let root = std::env::temp_dir().join(format!("d2rhub-audio-mod-{}", uuid::Uuid::new_v4()));
        let source = root.join("jcy.mpq");
        let excel = source.join("data/global/excel");
        let output = root.join("mods");
        let app_data = root.join("app-config");
        std::fs::create_dir_all(&excel).unwrap();

        let mut misc = "name\tcode\tdropsound\tusesound\n".to_string();
        for number in 1..=33 {
            misc.push_str(&format!(
                "Rune {number}\tr{number:02}\titem_rune_hd\titem_rune_hd\n"
            ));
        }
        std::fs::write(excel.join("misc.txt"), misc).unwrap();
        std::fs::write(
            excel.join("sounds.txt"),
            "Sound\t*Index\tRedirect\tFileName\tChannel\tIsAmbientScene\tIsAmbientEvent\tVolume Min\tVolume Max\tPitch Min\tPitch Max\tGroup Size\tGroup Weight\tLoop\tDefer Inst\tStop Inst\tCompound\tStream\tTracking\tIs2D\nitem_rune_hd\t10\t\titem\\rune.flac\tsfx/items_hd\t0\t0\t200\t200\t100\t100\t0\t0\t0\t0\t1\t0\t0\t0\t0\nscene_wilderness_day\t11\t\tambient\\scene.flac\tsfx/ambient/scene-2d_hd\t1\t0\t200\t200\t100\t100\t0\t0\t1\t0\t0\t0\t1\t0\t1\n",
        )
        .unwrap();
        std::fs::write(
            excel.join("levels.txt"),
            "Name\tId\tSoundEnv\tLevelName\nNull\t0\t0\tNull\nAct 1 - Town\t1\t1\tRogue Encampment\nAct 1 - Wilderness 5\t6\t2\tBlack Marsh\nAct 1 - Crypt 1\t20\t2\tForgotten Tower\nAct 1 - Crypt 2\t21\t2\tTower Cellar Level 1\nAct 1 - Crypt 3\t22\t2\tTower Cellar Level 2\nAct 1 - Crypt 4\t23\t2\tTower Cellar Level 3\nAct 1 - Crypt 5\t24\t2\tTower Cellar Level 4\nAct 1 - Crypt 6\t25\t2\tTower Cellar Level 5\n",
        )
        .unwrap();
        std::fs::write(
            excel.join("soundenviron.txt"),
            "Handle\tIndex\tDay Ambience\tHD Day Ambience\tNight Ambience\tHD Night Ambience\nTown\t1\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\nWild\t2\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\tscene_wilderness_day\n",
        )
        .unwrap();
        write_rune_unit_definitions(&source);
        let source_audio = source.join("data/hd/global/sfx/item/r01.flac");
        std::fs::create_dir_all(source_audio.parent().unwrap()).unwrap();
        let samples = (0..24_000)
            .map(|index| {
                ((std::f32::consts::TAU * 880.0 * index as f32 / 48_000.0).sin() * 4_000.0) as i32
            })
            .collect::<Vec<_>>();
        encode_flac(&source_audio, &samples, 48_000, 1, 16).unwrap();

        let report = build(
            app_data.to_string_lossy().to_string(),
            BuildAudioModRequest {
                source_directory: source.to_string_lossy().to_string(),
                output_directory: Some(output.to_string_lossy().to_string()),
                sound_environment_file: None,
                gain_db: Some(-26.0),
            },
            None,
        )
        .unwrap();
        assert_eq!(report.rune_assets.len(), 33);
        assert_eq!(report.area_assets.len(), COUNTESS_AREA_IDS.len());
        assert_eq!(
            report
                .rune_assets
                .iter()
                .filter(|asset| asset.preserved_source_audio)
                .count(),
            1
        );
        let output_mpq = output.join(MOD_NAME).join(format!("{MOD_NAME}.mpq"));
        assert!(output_mpq
            .join("data/global/excel/soundenviron.txt")
            .is_file());
        assert!(output_mpq
            .join("data/hd/global/sfx/d2rhub_audio/runes/r01.flac")
            .is_file());
        assert!(output_mpq
            .join("data/hd/global/sfx/d2rhub_audio/areas/a6.flac")
            .is_file());
        let misc_output = TsvTable::parse(
            "misc",
            &read_utf8(&output_mpq.join("data/global/excel/misc.txt")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            misc_output.get(&misc_output.rows[0], "dropsound"),
            Some("item_rune_hd")
        );
        let el_document: serde_json::Value = serde_json::from_str(
            &read_utf8(&output_mpq.join("data/hd/items/misc/rune/el_rune.json")).unwrap(),
        )
        .unwrap();
        let unit_root = el_document["entities"][0]["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|component| component["type"] == "UnitRootComponent")
            .unwrap();
        assert_eq!(
            unit_root["state_machine_filename"],
            "data/hd/items/d2rhub_audio/runes/r01_flippy.json"
        );
        let flippy: serde_json::Value = serde_json::from_str(
            &read_utf8(&output_mpq.join("data/hd/items/d2rhub_audio/runes/r01_flippy.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(flippy["states"][0]["audioId"], "d2rhub_audio_r01");
        assert_eq!(flippy["states"][1]["audioId"], "");
        assert_eq!(flippy["transitions"].as_array().unwrap().len(), 1);
        assert_eq!(flippy["transitions"][0]["from"], 1);
        assert!(app_data.join(AREA_CATALOG_FILE_NAME).is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires D2RHUB_AUDIO_REAL_MOD_SOURCE"]
    fn builds_real_source_mod_from_environment() {
        let source = std::env::var("D2RHUB_AUDIO_REAL_MOD_SOURCE").unwrap();
        let temporary_root =
            std::env::temp_dir().join(format!("d2rhub-audio-real-{}", uuid::Uuid::new_v4()));
        let output = std::env::var("D2RHUB_AUDIO_REAL_OUTPUT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| temporary_root.join("mods"));
        let app_data = std::env::var("D2RHUB_AUDIO_REAL_APP_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| temporary_root.join("config"));
        let report = build(
            app_data.to_string_lossy().to_string(),
            BuildAudioModRequest {
                source_directory: source,
                output_directory: Some(output.to_string_lossy().to_string()),
                sound_environment_file: std::env::var("D2RHUB_AUDIO_REAL_SOUND_ENVIRON").ok(),
                gain_db: Some(-30.0),
            },
            None,
        )
        .unwrap();
        assert_eq!(report.rune_assets.len(), RUNE_COUNT as usize);
        assert_eq!(report.area_assets.len(), COUNTESS_AREA_IDS.len());
        assert!(report
            .rune_assets
            .iter()
            .all(|asset| asset.confidence > 0.7));
        assert!(report
            .area_assets
            .iter()
            .all(|asset| asset.confidence > 0.7));
        if std::env::var_os("D2RHUB_AUDIO_REAL_OUTPUT").is_some() {
            assert!(report
                .area_assets
                .iter()
                .all(|asset| asset.preserved_source_audio));
        }
        if std::env::var_os("D2RHUB_AUDIO_REAL_OUTPUT").is_none() {
            std::fs::remove_dir_all(temporary_root).unwrap();
        }
    }
}
