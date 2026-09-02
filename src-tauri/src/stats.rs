use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{Error as IoError, ErrorKind, Read};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;

use crate::state::SharedState;
use crate::stats_page::{render_stats_template, stats_template_candidates};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropKind {
    #[default]
    Rune,
    Item,
}

fn default_drop_category() -> String {
    "runes".to_string()
}

/// One persisted drop. Aliases keep every historical rune JSON readable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropEntry {
    #[serde(default)]
    pub kind: DropKind,
    #[serde(default)]
    pub telemetry_id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_code: Option<String>,
    #[serde(default = "default_drop_category")]
    pub category: String,
    #[serde(alias = "rune_name")]
    pub display_name: String,
    #[serde(
        default,
        alias = "rune_name_en",
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name_en: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rune_number: Option<u32>,
    /// 截图相对路径（相对于 stateData 目录），低号符文为 null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_path: Option<String>,
}

/// 单条场景记录（新版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneRecord {
    /// 数据库主键（用于删除操作），None 表示未持久化
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub absolute_time: String,
    pub character_name: String,
    pub scene_name: String,
    /// 恐怖区域记录。旧数据库与缺失字段一律按普通区域处理。
    #[serde(default)]
    pub tz: bool,
    pub timer_seconds: f64,
    /// 连续离开主城/主界面后的同一次野外行程。旧记录为 None，不做猜测合并。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journey_id: Option<String>,
    /// 此原始野外分段在行程中的零基序号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_index: Option<u32>,
    /// 新版：通用掉落数组（每个元素 = 一次独立掉落）
    pub drops: Vec<DropEntry>,
}

/// 只影响统计页展示的合并策略；原始场景记录始终保持独立。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeStrategy {
    pub id: i64,
    pub name: String,
    pub scene_names: Vec<String>,
}

/// 原始掉落声纹观测；野外分段结束时通过 scene_record_id 原子归属。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropObservation {
    pub id: i64,
    pub observed_at: String,
    pub account_id: String,
    pub kind: DropKind,
    pub telemetry_id: u32,
    pub item_code: Option<String>,
    pub category: String,
    pub display_name: String,
    pub display_name_en: String,
    pub rune_number: Option<u32>,
    pub confidence: f32,
    pub source: String,
    pub scene_record_id: Option<i64>,
}

/// 全部统计数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsData {
    pub records: Vec<SceneRecord>,
    pub observations: Vec<DropObservation>,
    pub strategies: Vec<MergeStrategy>,
}

#[derive(Debug, Deserialize)]
struct BatchDeleteRecordsRequest {
    ids: Vec<i64>,
}

const MAX_STATS_API_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_STATS_PAGE_PREFERENCES_BYTES: usize = 256 * 1024;
const STATS_PAGE_PREFERENCES_FILE: &str = "stats_page_preferences.json";

fn stats_page_preferences_path(app_data_dir: &str) -> std::path::PathBuf {
    Path::new(app_data_dir)
        .join("stateData")
        .join(STATS_PAGE_PREFERENCES_FILE)
}

fn read_stats_page_preferences(app_data_dir: &str) -> Result<Option<serde_json::Value>, String> {
    let path = stats_page_preferences_path(app_data_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取统计页偏好失败: {error}")),
    };
    if bytes.len() > MAX_STATS_PAGE_PREFERENCES_BYTES {
        return Err("统计页偏好文件过大".to_string());
    }
    let value = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|error| format!("统计页偏好文件损坏: {error}"))?;
    if !value.is_object() {
        return Err("统计页偏好格式无效".to_string());
    }
    Ok(Some(value))
}

fn write_stats_page_preferences(
    app_data_dir: &str,
    preferences: &serde_json::Value,
) -> Result<(), String> {
    if !preferences.is_object() {
        return Err("统计页偏好格式无效".to_string());
    }
    let json = serde_json::to_vec_pretty(preferences)
        .map_err(|error| format!("序列化统计页偏好失败: {error}"))?;
    if json.len() > MAX_STATS_PAGE_PREFERENCES_BYTES {
        return Err("统计页偏好内容过大".to_string());
    }
    let path = stats_page_preferences_path(app_data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建统计页偏好目录失败: {error}"))?;
    }
    std::fs::write(path, json).map_err(|error| format!("保存统计页偏好失败: {error}"))
}

/// 懒初始化数据库连接
static DB: std::sync::OnceLock<Mutex<Connection>> = std::sync::OnceLock::new();

fn ensure_scene_segment_columns(conn: &Connection) -> Result<(), String> {
    let scene_columns = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(scene_records)")
            .map_err(|e| format!("检查场景记录表结构失败: {e}"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("读取场景记录表结构失败: {e}"))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        columns
    };
    if !scene_columns.iter().any(|column| column == "journey_id") {
        conn.execute("ALTER TABLE scene_records ADD COLUMN journey_id TEXT", [])
            .map_err(|e| format!("迁移行程标识字段失败: {e}"))?;
    }
    if !scene_columns.iter().any(|column| column == "segment_index") {
        conn.execute(
            "ALTER TABLE scene_records ADD COLUMN segment_index INTEGER",
            [],
        )
        .map_err(|e| format!("迁移分段序号字段失败: {e}"))?;
    }
    if !scene_columns.iter().any(|column| column == "tz") {
        conn.execute(
            "ALTER TABLE scene_records ADD COLUMN tz INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|e| format!("迁移恐怖区域标记字段失败: {e}"))?;
    }
    Ok(())
}

fn ensure_drop_observation_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS drop_observations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            observed_at TEXT NOT NULL,
            account_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            telemetry_id INTEGER NOT NULL,
            item_code TEXT,
            category TEXT NOT NULL,
            display_name TEXT NOT NULL,
            display_name_en TEXT NOT NULL,
            rune_number INTEGER,
            confidence REAL NOT NULL,
            source TEXT NOT NULL,
            scene_record_id INTEGER,
            legacy_rune_observation_id INTEGER UNIQUE
        );
        CREATE INDEX IF NOT EXISTS idx_drop_observed_at
            ON drop_observations(observed_at);
        CREATE INDEX IF NOT EXISTS idx_drop_account
            ON drop_observations(account_id);
        INSERT OR IGNORE INTO drop_observations
             (observed_at, account_id, kind, telemetry_id, item_code, category,
              display_name, display_name_en, rune_number, confidence, source,
              scene_record_id, legacy_rune_observation_id)
         SELECT observed_at, account_id, 'rune', rune_number,
                printf('r%02d', rune_number), 'runes', rune_name, rune_name_en,
                rune_number, confidence, source, scene_record_id, id
           FROM rune_drop_observations;
        CREATE INDEX IF NOT EXISTS idx_drop_scene_record
            ON drop_observations(scene_record_id);
        CREATE TRIGGER IF NOT EXISTS clear_drop_scene_record_after_delete
            AFTER DELETE ON scene_records
            BEGIN
                UPDATE drop_observations
                   SET scene_record_id = NULL
                 WHERE scene_record_id = OLD.id;
            END;
        CREATE TRIGGER IF NOT EXISTS delete_legacy_rune_after_drop_delete
            AFTER DELETE ON drop_observations
            WHEN OLD.legacy_rune_observation_id IS NOT NULL
            BEGIN
                DELETE FROM rune_drop_observations
                 WHERE id = OLD.legacy_rune_observation_id;
            END;",
    )
    .map_err(|error| format!("初始化通用掉落观测表失败: {error}"))
}

fn get_db_path(app_data_dir: &str) -> String {
    Path::new(app_data_dir)
        .join("stateData")
        .join("data.db")
        .to_string_lossy()
        .to_string()
}

fn get_db(app_data_dir: &str) -> Result<&Mutex<Connection>, String> {
    if let Some(db) = DB.get() {
        return Ok(db);
    }
    let db_path = get_db_path(app_data_dir);
    if let Some(parent) = Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建数据库目录失败: {}", e))?;
    }

    // 迁移旧数据库：config/data.db → config/stateData/data.db
    let old_db_path = Path::new(app_data_dir).join("data.db");
    let new_db_path = Path::new(&db_path);
    if old_db_path.exists() && !new_db_path.exists() {
        eprintln!(
            "[DB] 检测到旧数据库，正在迁移: {} → {}",
            old_db_path.display(),
            new_db_path.display()
        );
        if let Err(e) = std::fs::copy(&old_db_path, new_db_path) {
            eprintln!("[DB] 迁移失败: {}，将创建新数据库", e);
        } else {
            eprintln!("[DB] 迁移成功");
        }
    }

    let conn = Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scene_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            absolute_time TEXT NOT NULL,
            character_name TEXT NOT NULL,
            scene_name TEXT NOT NULL,
            tz INTEGER NOT NULL DEFAULT 0,
            timer_seconds REAL NOT NULL,
            journey_id TEXT,
            segment_index INTEGER,
            drops_json TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_scene_time ON scene_records(absolute_time);
        CREATE INDEX IF NOT EXISTS idx_scene_name ON scene_records(scene_name);
        CREATE TABLE IF NOT EXISTS rune_drop_observations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            observed_at TEXT NOT NULL,
            account_id TEXT NOT NULL,
            rune_number INTEGER NOT NULL,
            rune_name TEXT NOT NULL,
            rune_name_en TEXT NOT NULL,
            confidence REAL NOT NULL,
            source TEXT NOT NULL,
            scene_record_id INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_rune_drop_observed_at
            ON rune_drop_observations(observed_at);
        CREATE INDEX IF NOT EXISTS idx_rune_drop_account
            ON rune_drop_observations(account_id);
        CREATE TABLE IF NOT EXISTS stats_merge_strategies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            scene_names_json TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("初始化数据表失败: {}", e))?;
    let has_scene_record_id = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(rune_drop_observations)")
            .map_err(|e| format!("检查观测表结构失败: {e}"))?;
        let found = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("读取观测表结构失败: {e}"))?
            .filter_map(Result::ok)
            .any(|column| column == "scene_record_id");
        found
    };
    if !has_scene_record_id {
        conn.execute(
            "ALTER TABLE rune_drop_observations ADD COLUMN scene_record_id INTEGER",
            [],
        )
        .map_err(|e| format!("迁移观测归属字段失败: {e}"))?;
    }
    ensure_scene_segment_columns(&conn)?;
    ensure_drop_observation_schema(&conn)?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_rune_drop_scene_record
             ON rune_drop_observations(scene_record_id);
         CREATE INDEX IF NOT EXISTS idx_scene_journey_segment
             ON scene_records(journey_id, segment_index);
         CREATE TRIGGER IF NOT EXISTS clear_rune_drop_scene_record_after_delete
             AFTER DELETE ON scene_records
             BEGIN
                 UPDATE rune_drop_observations
                    SET scene_record_id = NULL
                  WHERE scene_record_id = OLD.id;
             END;",
    )
    .map_err(|e| format!("初始化观测归属索引失败: {e}"))?;
    let _ = DB.set(Mutex::new(conn));
    DB.get().ok_or_else(|| "数据库初始化状态不可用".to_string())
}

pub(crate) struct NewDropObservation<'a> {
    pub observed_at: &'a str,
    pub account_id: &'a str,
    pub kind: &'a str,
    pub telemetry_id: u32,
    pub item_code: Option<&'a str>,
    pub category: &'a str,
    pub display_name: &'a str,
    pub display_name_en: &'a str,
    pub rune_number: Option<u32>,
    pub confidence: f32,
    pub source: &'a str,
}

pub(crate) fn insert_drop_observation(
    state: &SharedState,
    observation: NewDropObservation<'_>,
) -> Result<i64, String> {
    let db = get_db(&state.app_data_dir)?;
    let conn = db
        .lock()
        .map_err(|error| format!("数据库锁失败: {error}"))?;
    conn.execute(
        "INSERT INTO drop_observations
         (observed_at, account_id, kind, telemetry_id, item_code, category,
          display_name, display_name_en, rune_number, confidence, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            observation.observed_at,
            observation.account_id,
            observation.kind,
            observation.telemetry_id,
            observation.item_code,
            observation.category,
            observation.display_name,
            observation.display_name_en,
            observation.rune_number,
            observation.confidence,
            observation.source,
        ],
    )
    .map_err(|error| format!("保存掉落观测失败: {error}"))?;
    Ok(conn.last_insert_rowid())
}

fn normalize_drop(mut drop: DropEntry) -> DropEntry {
    if drop.kind == DropKind::Rune {
        let rune_number = drop.rune_number.unwrap_or(drop.telemetry_id);
        drop.rune_number = (1..=33).contains(&rune_number).then_some(rune_number);
        if drop.telemetry_id == 0 {
            drop.telemetry_id = rune_number;
        }
        if drop.item_code.is_none() && rune_number > 0 {
            drop.item_code = Some(format!("r{rune_number:02}"));
        }
        if drop.category.trim().is_empty() {
            drop.category = "runes".to_string();
        }
    }
    drop
}

/// 将旧版 drops HashMap/符文数组迁移为通用掉落数组。
fn migrate_legacy_drops(drops_json: &str) -> Vec<DropEntry> {
    // 尝试解析为新版数组格式
    if let Ok(drops) = serde_json::from_str::<Vec<DropEntry>>(drops_json) {
        if !drops.is_empty() || drops_json.trim().starts_with('[') {
            return drops.into_iter().map(normalize_drop).collect();
        }
    }
    // 尝试解析为旧版 HashMap 格式 { "符文名": count }
    if let Ok(legacy) = serde_json::from_str::<HashMap<String, u32>>(drops_json) {
        let mut result = Vec::new();
        for (name, count) in legacy {
            let rune_number = crate::rune_data::get_rune_number(&name).unwrap_or(0);
            for _ in 0..count {
                result.push(DropEntry {
                    kind: DropKind::Rune,
                    telemetry_id: rune_number,
                    item_code: (rune_number > 0).then(|| format!("r{rune_number:02}")),
                    category: "runes".to_string(),
                    display_name: name.clone(),
                    display_name_en: None,
                    rune_number: (rune_number > 0).then_some(rune_number),
                    screenshot_path: None,
                });
            }
        }
        return result;
    }
    Vec::new()
}

fn ensure_stats_module_installed(state: &SharedState) -> Result<(), String> {
    let installed = state
        .configuration()
        .snapshot()
        .is_some_and(|config| {
            config.optional_module_installed(
                crate::domain::config::OPTIONAL_MODULE_AUTOMATION,
            )
        });
    installed
        .then_some(())
        .ok_or_else(|| "识别与统计模块尚未安装".to_string())
}

/// 保存一条场景记录
#[tauri::command]
pub fn save_scene_record(
    state: tauri::State<'_, SharedState>,
    record: SceneRecord,
) -> Result<(), String> {
    ensure_stats_module_installed(state.inner())?;
    save_scene_record_inner(&state, &record).map(|_| ())
}

pub(crate) fn save_scene_record_inner(
    state: &SharedState,
    record: &SceneRecord,
) -> Result<i64, String> {
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;

    let drops_json =
        serde_json::to_string(&record.drops).map_err(|e| format!("序列化掉落失败: {}", e))?;

    conn.execute(
        "INSERT INTO scene_records
         (absolute_time, character_name, scene_name, tz, timer_seconds, journey_id, segment_index, drops_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            &record.absolute_time,
            &record.character_name,
            &record.scene_name,
            record.tz,
            record.timer_seconds,
            &record.journey_id,
            record.segment_index,
            drops_json,
        ],
    )
    .map_err(|e| format!("保存记录失败: {}", e))?;

    Ok(conn.last_insert_rowid())
}

pub(crate) fn save_completed_segment(
    state: &SharedState,
    segment: &crate::rune_audio::tracking::CompletedSegment,
) -> Result<i64, String> {
    let record = SceneRecord {
        id: None,
        absolute_time: segment.absolute_time.clone(),
        character_name: segment.character_name.clone(),
        scene_name: segment.scene_name.clone(),
        tz: segment.tz,
        timer_seconds: segment.timer_seconds,
        journey_id: Some(segment.journey_id.clone()),
        segment_index: Some(segment.segment_index),
        drops: segment
            .drops
            .iter()
            .map(|drop| DropEntry {
                kind: match drop.kind {
                    crate::rune_audio::tracking::TrackedDropKind::Rune => DropKind::Rune,
                    crate::rune_audio::tracking::TrackedDropKind::Item => DropKind::Item,
                },
                telemetry_id: drop.telemetry_id,
                item_code: drop.code.clone(),
                category: drop.category.clone(),
                display_name: drop.name.clone(),
                display_name_en: Some(drop.name_en.clone()),
                rune_number: drop.rune_number,
                screenshot_path: None,
            })
            .collect(),
    };
    let drops_json = serde_json::to_string(&record.drops)
        .map_err(|error| format!("序列化自动刷图掉落失败: {error}"))?;
    let db = get_db(&state.app_data_dir)?;
    let mut conn = db
        .lock()
        .map_err(|error| format!("数据库锁失败: {error}"))?;
    let transaction = conn
        .transaction()
        .map_err(|error| format!("开始自动刷图保存事务失败: {error}"))?;
    transaction
        .execute(
            "INSERT INTO scene_records
             (absolute_time, character_name, scene_name, tz, timer_seconds, journey_id, segment_index, drops_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                &record.absolute_time,
                &record.character_name,
                &record.scene_name,
                record.tz,
                record.timer_seconds,
                &record.journey_id,
                record.segment_index,
                drops_json,
            ],
        )
        .map_err(|error| format!("保存自动刷图记录失败: {error}"))?;
    let scene_record_id = transaction.last_insert_rowid();
    for observation_id in segment
        .drops
        .iter()
        .map(|drop| drop.observation_id)
        .filter(|observation_id| *observation_id > 0)
    {
        transaction
            .execute(
                "UPDATE drop_observations SET scene_record_id = ?1 WHERE id = ?2",
                rusqlite::params![scene_record_id, observation_id],
            )
            .map_err(|error| format!("关联掉落观测与场次失败: {error}"))?;
    }
    transaction
        .commit()
        .map_err(|error| format!("提交自动刷图保存事务失败: {error}"))?;
    Ok(scene_record_id)
}

fn query_scene_records(conn: &Connection) -> Result<Vec<SceneRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, absolute_time, character_name, scene_name, timer_seconds,
                    tz, journey_id, segment_index, drops_json
             FROM scene_records ORDER BY id ASC",
        )
        .map_err(|e| format!("查询准备失败: {e}"))?;
    let records = stmt
        .query_map([], |row| {
            let drops_json: String = row.get(8)?;
            Ok(SceneRecord {
                id: Some(row.get(0)?),
                absolute_time: row.get(1)?,
                character_name: row.get(2)?,
                scene_name: row.get(3)?,
                timer_seconds: row.get(4)?,
                tz: row.get(5)?,
                journey_id: row.get(6)?,
                segment_index: row.get(7)?,
                drops: migrate_legacy_drops(&drops_json),
            })
        })
        .map_err(|e| format!("查询执行失败: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取场景记录失败: {e}"))?;
    Ok(records)
}

fn query_merge_strategies(conn: &Connection) -> Result<Vec<MergeStrategy>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, scene_names_json FROM stats_merge_strategies ORDER BY id ASC")
        .map_err(|e| format!("准备查询统计策略失败: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("查询统计策略失败: {e}"))?;
    let mut strategies = Vec::new();
    for row in rows {
        let (id, name, scene_names_json) = row.map_err(|e| format!("读取统计策略失败: {e}"))?;
        let scene_names = serde_json::from_str(&scene_names_json)
            .map_err(|e| format!("统计策略“{name}”的数据损坏: {e}"))?;
        strategies.push(MergeStrategy {
            id,
            name,
            scene_names,
        });
    }
    Ok(strategies)
}

fn normalize_strategy(
    name: &str,
    scene_names: Vec<String>,
) -> Result<(String, Vec<String>), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("策略名称不能为空".to_string());
    }
    if name.chars().count() > 40 {
        return Err("策略名称不能超过 40 个字符".to_string());
    }
    if scene_names.len() > 64 {
        return Err("单个策略最多包含 64 个场景".to_string());
    }
    let mut normalized = Vec::new();
    for scene_name in scene_names {
        let scene_name = scene_name.trim().to_string();
        if scene_name.chars().count() > 100 {
            return Err("场景名称不能超过 100 个字符".to_string());
        }
        if !scene_name.is_empty() && !normalized.contains(&scene_name) {
            normalized.push(scene_name);
        }
    }
    if normalized.is_empty() {
        return Err("至少选择一个野外场景".to_string());
    }
    Ok((name, normalized))
}

fn insert_merge_strategy(
    conn: &Connection,
    name: &str,
    scene_names: Vec<String>,
) -> Result<MergeStrategy, String> {
    let (name, scene_names) = normalize_strategy(name, scene_names)?;
    let scene_names_json =
        serde_json::to_string(&scene_names).map_err(|e| format!("序列化统计策略失败: {e}"))?;
    conn.execute(
        "INSERT INTO stats_merge_strategies (name, scene_names_json) VALUES (?1, ?2)",
        rusqlite::params![&name, scene_names_json],
    )
    .map_err(|e| format!("保存统计策略失败（名称不可重复）: {e}"))?;
    Ok(MergeStrategy {
        id: conn.last_insert_rowid(),
        name,
        scene_names,
    })
}

fn update_merge_strategy(
    conn: &Connection,
    strategy_id: i64,
    name: &str,
    scene_names: Vec<String>,
) -> Result<MergeStrategy, String> {
    let (name, scene_names) = normalize_strategy(name, scene_names)?;
    let scene_names_json =
        serde_json::to_string(&scene_names).map_err(|e| format!("序列化统计策略失败: {e}"))?;
    let affected = conn
        .execute(
            "UPDATE stats_merge_strategies SET name = ?2, scene_names_json = ?3 WHERE id = ?1",
            rusqlite::params![strategy_id, &name, scene_names_json],
        )
        .map_err(|e| format!("更新统计策略失败（名称不可重复）: {e}"))?;
    if affected == 0 {
        return Err("统计策略不存在或已被删除".to_string());
    }
    Ok(MergeStrategy {
        id: strategy_id,
        name,
        scene_names,
    })
}

/// 获取所有统计数据（结构体形式，供前端使用）
#[tauri::command]
pub fn get_stats_data(state: tauri::State<'_, SharedState>) -> Result<StatsData, String> {
    ensure_stats_module_installed(state.inner())?;
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
    let records = query_scene_records(&conn)?;
    let observations = query_drop_observations(&conn)?;
    let strategies = query_merge_strategies(&conn)?;

    Ok(StatsData {
        records,
        observations,
        strategies,
    })
}

#[tauri::command]
pub fn get_stats_page_preferences(
    state: tauri::State<'_, SharedState>,
) -> Result<Option<serde_json::Value>, String> {
    ensure_stats_module_installed(state.inner())?;
    read_stats_page_preferences(&state.app_data_dir)
}

#[tauri::command]
pub fn save_stats_page_preferences(
    state: tauri::State<'_, SharedState>,
    preferences: serde_json::Value,
) -> Result<(), String> {
    ensure_stats_module_installed(state.inner())?;
    write_stats_page_preferences(&state.app_data_dir, &preferences)
}

fn query_drop_observations(conn: &Connection) -> Result<Vec<DropObservation>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, observed_at, account_id, kind, telemetry_id, item_code, category,
                    display_name, display_name_en, rune_number, confidence, source,
                    scene_record_id
             FROM drop_observations
             ORDER BY id ASC",
        )
        .map_err(|error| format!("准备查询掉落声纹观测失败: {error}"))?;
    let observations = stmt
        .query_map([], |row| {
            let kind = match row.get::<_, String>(3)?.as_str() {
                "item" => DropKind::Item,
                _ => DropKind::Rune,
            };
            Ok(DropObservation {
                id: row.get(0)?,
                observed_at: row.get(1)?,
                account_id: row.get(2)?,
                kind,
                telemetry_id: row.get(4)?,
                item_code: row.get(5)?,
                category: row.get(6)?,
                display_name: row.get(7)?,
                display_name_en: row.get(8)?,
                rune_number: row.get(9)?,
                confidence: row.get(10)?,
                source: row.get(11)?,
                scene_record_id: row.get(12)?,
            })
        })
        .map_err(|error| format!("查询掉落声纹观测失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取掉落声纹观测失败: {error}"))?;
    Ok(observations)
}

/// 获取统计数据 JSON 字符串（用于嵌入 HTML 页面）
#[tauri::command]
pub fn get_stats_json(state: tauri::State<'_, SharedState>) -> Result<String, String> {
    let data = get_stats_data(state)?;
    serde_json::to_string(&data).map_err(|e| format!("JSON 序列化失败: {}", e))
}

fn query_scene_stats(
    conn: &Connection,
    scene_name: &str,
    tz: bool,
) -> Result<Option<SceneStats>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT AVG(timer_seconds), COUNT(*)
             FROM scene_records
             WHERE scene_name = ?1 AND tz = ?2",
        )
        .map_err(|e| format!("查询准备失败: {}", e))?;

    stmt.query_row(rusqlite::params![scene_name, tz], |row| {
        let avg: Option<f64> = row.get(0)?;
        let count: i64 = row.get(1)?;
        Ok(avg.map(|avg_time| SceneStats {
            avg_time,
            total_runs: count,
        }))
    })
    .optional()
    .map_err(|e| format!("查询场景统计失败: {}", e))
    .map(Option::flatten)
}

/// 查询指定场景及 TZ 状态的历史平均耗时。
#[tauri::command]
pub fn get_scene_avg_time(
    state: tauri::State<'_, SharedState>,
    scene_name: String,
    tz: Option<bool>,
) -> Result<Option<f64>, String> {
    ensure_stats_module_installed(state.inner())?;
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
    Ok(query_scene_stats(&conn, &scene_name, tz.unwrap_or(false))?.map(|stats| stats.avg_time))
}

/// 场景统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneStats {
    pub avg_time: f64,
    pub total_runs: i64,
}

/// 查询指定场景及 TZ 状态的统计信息（平均耗时，总场次）。
#[tauri::command]
pub fn get_scene_stats(
    state: tauri::State<'_, SharedState>,
    scene_name: String,
    tz: Option<bool>,
) -> Result<Option<SceneStats>, String> {
    ensure_stats_module_installed(state.inner())?;
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
    query_scene_stats(&conn, &scene_name, tz.unwrap_or(false))
}

/// 删除一条场景记录（不可恢复）
#[tauri::command]
pub fn delete_scene_record(state: tauri::State<'_, SharedState>, id: i64) -> Result<(), String> {
    ensure_stats_module_installed(state.inner())?;
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;

    let affected = conn
        .execute(
            "DELETE FROM scene_records WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| format!("删除记录失败: {}", e))?;

    if affected == 0 {
        return Err(format!("未找到 ID={} 的记录", id));
    }

    Ok(())
}

fn delete_scene_records_by_ids(conn: &mut Connection, ids: &[i64]) -> Result<usize, String> {
    let mut seen = HashSet::new();
    let ids = ids
        .iter()
        .copied()
        .filter(|id| *id > 0 && seen.insert(*id))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err("至少选择一条有效记录".to_string());
    }
    if ids.len() > 10_000 {
        return Err("单次最多删除 10000 条记录".to_string());
    }

    let transaction = conn
        .transaction()
        .map_err(|error| format!("开始批量删除事务失败: {error}"))?;
    let deleted = {
        let mut statement = transaction
            .prepare("DELETE FROM scene_records WHERE id = ?1")
            .map_err(|error| format!("准备批量删除失败: {error}"))?;
        let mut deleted = 0usize;
        for id in ids {
            deleted += statement
                .execute(rusqlite::params![id])
                .map_err(|error| format!("删除记录 ID={id} 失败: {error}"))?;
        }
        deleted
    };
    transaction
        .commit()
        .map_err(|error| format!("提交批量删除失败: {error}"))?;
    Ok(deleted)
}

fn request_content_length(headers: &[u8]) -> usize {
    String::from_utf8_lossy(headers)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn request_header_value<'a>(headers: &'a str, expected_name: &str) -> Option<&'a str> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(expected_name)
            .then(|| value.trim())
    })
}

fn stats_api_token_is_valid(headers: &str, expected_token: &str) -> bool {
    request_header_value(headers, "x-d2rhub-stats-token")
        .is_some_and(|provided| provided.as_bytes() == expected_token.as_bytes())
}

fn read_stats_http_request(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::with_capacity(4096);
    let mut header_end = None;
    let mut expected_length = None;
    loop {
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > MAX_STATS_API_REQUEST_BYTES {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                "统计 API 请求超过 1 MiB",
            ));
        }

        if header_end.is_none() {
            header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| position + 4);
            if let Some(end) = header_end {
                expected_length = Some(end + request_content_length(&request[..end]));
            }
        }
        if expected_length.is_some_and(|length| request.len() >= length) {
            break;
        }
    }
    Ok(request)
}

/// URL 解码辅助函数
fn url_decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            if let (Some(b1), Some(b2)) = (chars.next(), chars.next()) {
                if let (Ok(h1), Ok(h2)) = (std::str::from_utf8(&[b1]), std::str::from_utf8(&[b2])) {
                    if let Ok(hex) = u8::from_str_radix(&format!("{}{}", h1, h2), 16) {
                        bytes.push(hex);
                        continue;
                    }
                }
            }
        } else if b == b'+' {
            bytes.push(b' ');
            continue;
        }
        bytes.push(b);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// 解析 Query String
fn parse_query(query_str: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in query_str.split('&') {
        let mut parts = pair.splitn(2, '=');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            params.insert(key.to_string(), url_decode(value));
        }
    }
    params
}

const STATS_API_CORS_HEADERS: &str = "Access-Control-Allow-Origin: null\r\n\
                                       Vary: Origin\r\n\
                                       Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS\r\n\
                                       Access-Control-Allow-Headers: Content-Type, X-D2RHub-Stats-Token\r\n";

struct StatsApiRuntime {
    port: u16,
    token: String,
    worker: std::thread::JoinHandle<()>,
}

static STATS_API_RUNTIME: std::sync::OnceLock<std::sync::Mutex<Option<StatsApiRuntime>>> =
    std::sync::OnceLock::new();
static STATS_API_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STATS_API_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn stats_api_runtime() -> &'static std::sync::Mutex<Option<StatsApiRuntime>> {
    STATS_API_RUNTIME.get_or_init(|| std::sync::Mutex::new(None))
}

pub(crate) fn stop_stats_api() {
    STATS_API_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    STATS_API_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    let runtime = stats_api_runtime()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    let Some(runtime) = runtime else {
        return;
    };
    // Wake `listener.incoming()` so the worker can observe the stop flag.
    let _ = std::net::TcpStream::connect(("127.0.0.1", runtime.port));
    if runtime.worker.join().is_err() {
        crate::logger::log_msg("WARN", "Stats", "统计 API 线程停止时发生 panic");
    }
}

/// 启动统计页微 HTTP API 服务（供浏览器中的 stats.html 调用）
fn start_stats_api(app_data_dir: String) -> Result<(u16, String), String> {
    let mut runtime = stats_api_runtime()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if STATS_API_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
        let running = runtime
            .as_ref()
            .ok_or_else(|| "统计 API 正在启动，请稍后重试".to_string())?;
        return Ok((running.port, running.token.clone()));
    }
    if let Some(stale) = runtime.take() {
        let _ = stale.worker.join();
    }
    STATS_API_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
    let generation = STATS_API_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|error| {
        STATS_API_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        format!("绑定统计 API 端口失败: {error}")
    })?;
    let port = listener
        .local_addr()
        .map_err(|error| {
            STATS_API_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
            format!("读取统计 API 端口失败: {error}")
        })?
        .port();
    let api_token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let expected_api_token = api_token.clone();

    let api_thread = std::thread::Builder::new().name("stats-api".into()).spawn(move || {
        for stream in listener.incoming() {
            if !STATS_API_RUNNING.load(std::sync::atomic::Ordering::SeqCst)
                || STATS_API_GENERATION.load(std::sync::atomic::Ordering::SeqCst) != generation
            {
                break;
            }
            if let Ok(mut stream) = stream {
                use std::io::Write;
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(3)));

                // 读取完整请求（批量删除使用 JSON body）。
                let request = match read_stats_http_request(&mut stream) {
                    Ok(request) if !request.is_empty() => request,
                    _ => continue,
                };
                let raw = String::from_utf8_lossy(&request);
                let (request_headers, request_body) =
                    raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
                let first_line = request_headers.lines().next().unwrap_or("");
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() < 2 { continue; }
                let method = parts[0];
                let full_path = parts[1];

                let cors_headers = STATS_API_CORS_HEADERS;

                let resp_body: String;

                if method == "OPTIONS" {
                    resp_body = format!("HTTP/1.1 204 No Content\r\n{}\r\n", cors_headers);
                } else if !stats_api_token_is_valid(request_headers, &expected_api_token) {
                    resp_body = format!(
                        "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\n{}\r\n\r\n{}",
                        cors_headers,
                        r#"{"ok":false,"error":"Unauthorized"}"#
                    );
                } else {
                    let mut path_parts_split = full_path.split('?');
                    let path = path_parts_split.next().unwrap_or("");
                    let query_str = path_parts_split.next().unwrap_or("");
                    let query_params = parse_query(query_str);

                    if method == "DELETE" && path == "/api/records/batch" {
                        match serde_json::from_str::<BatchDeleteRecordsRequest>(request_body) {
                            Ok(request) => match get_db(&app_data_dir) {
                                Ok(db) => match db.lock() {
                                    Ok(mut conn) => {
                                        match delete_scene_records_by_ids(&mut conn, &request.ids) {
                                            Ok(deleted) => {
                                                let body = serde_json::json!({
                                                    "ok": true,
                                                    "requested": request.ids.len(),
                                                    "deleted": deleted
                                                });
                                                resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                            }
                                            Err(error) => {
                                                let body = serde_json::json!({"ok": false, "error": error});
                                                resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        let body = serde_json::json!({"ok": false, "error": error.to_string()});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                },
                                Err(error) => {
                                    let body = serde_json::json!({"ok": false, "error": error});
                                    resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                            },
                            Err(error) => {
                                let body = serde_json::json!({"ok": false, "error": format!("批量删除请求无效: {error}")});
                                resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                        }
                    } else if method == "POST" && path == "/api/strategies" {
                        let name = query_params.get("name").cloned().unwrap_or_default();
                        let scenes_json = query_params.get("scenes").cloned().unwrap_or_default();
                        let scene_names = serde_json::from_str::<Vec<String>>(&scenes_json)
                            .map_err(|error| format!("场景列表格式无效: {error}"));
                        match scene_names {
                            Ok(scene_names) => match get_db(&app_data_dir) {
                                Ok(db) => match db.lock() {
                                    Ok(conn) => match insert_merge_strategy(&conn, &name, scene_names) {
                                        Ok(strategy) => {
                                            let body = serde_json::json!({"ok": true, "strategy": strategy});
                                            resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                        Err(error) => {
                                            let body = serde_json::json!({"ok": false, "error": error});
                                            resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                    },
                                    Err(error) => {
                                        let body = serde_json::json!({"ok": false, "error": error.to_string()});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                },
                                Err(error) => {
                                    let body = serde_json::json!({"ok": false, "error": error});
                                    resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                            },
                            Err(error) => {
                                let body = serde_json::json!({"ok": false, "error": error});
                                resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                        }
                    } else if method == "PUT" && path.starts_with("/api/strategies/") {
                        let id_str = path.trim_start_matches("/api/strategies/");
                        let name = query_params.get("name").cloned().unwrap_or_default();
                        let scenes_json = query_params.get("scenes").cloned().unwrap_or_default();
                        let strategy_id = id_str
                            .parse::<i64>()
                            .map_err(|_| "无效的策略 ID".to_string());
                        let scene_names = serde_json::from_str::<Vec<String>>(&scenes_json)
                            .map_err(|error| format!("场景列表格式无效: {error}"));
                        match (strategy_id, scene_names) {
                            (Ok(strategy_id), Ok(scene_names)) => match get_db(&app_data_dir) {
                                Ok(db) => match db.lock() {
                                    Ok(conn) => match update_merge_strategy(
                                        &conn,
                                        strategy_id,
                                        &name,
                                        scene_names,
                                    ) {
                                        Ok(strategy) => {
                                            let body = serde_json::json!({"ok": true, "strategy": strategy});
                                            resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                        Err(error) => {
                                            let body = serde_json::json!({"ok": false, "error": error});
                                            resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                    },
                                    Err(error) => {
                                        let body = serde_json::json!({"ok": false, "error": error.to_string()});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                },
                                Err(error) => {
                                    let body = serde_json::json!({"ok": false, "error": error});
                                    resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                            },
                            (Err(error), _) | (_, Err(error)) => {
                                let body = serde_json::json!({"ok": false, "error": error});
                                resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                        }
                    } else if method == "DELETE" && path.starts_with("/api/strategies/") {
                        let id_str = path.trim_start_matches("/api/strategies/");
                        if let Ok(strategy_id) = id_str.parse::<i64>() {
                            match get_db(&app_data_dir) {
                                Ok(db) => match db.lock() {
                                    Ok(conn) => match conn.execute(
                                        "DELETE FROM stats_merge_strategies WHERE id = ?1",
                                        rusqlite::params![strategy_id],
                                    ) {
                                        Ok(affected) => {
                                            let body = serde_json::json!({"ok": true, "deleted": affected});
                                            resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                        Err(error) => {
                                            let body = serde_json::json!({"ok": false, "error": error.to_string()});
                                            resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                    },
                                    Err(error) => {
                                        let body = serde_json::json!({"ok": false, "error": error.to_string()});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                },
                                Err(error) => {
                                    let body = serde_json::json!({"ok": false, "error": error});
                                    resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                            }
                        } else {
                            let body = serde_json::json!({"ok": false, "error": "无效的策略 ID"});
                            resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                        }
                    } else if method == "POST" && path == "/api/scenes/rename" {
                        let from_name = query_params.get("from").cloned().unwrap_or_default();
                        let to_name = query_params.get("to").cloned().unwrap_or_default();
                        if from_name.is_empty() || to_name.is_empty() {
                            let body = serde_json::json!({"ok": false, "error": "参数 from 和 to 不能为空"});
                            resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                        } else {
                            match get_db(&app_data_dir) {
                                Ok(db) => match db.lock() {
                                    Ok(conn) => {
                                        match conn.execute(
                                            "UPDATE scene_records SET scene_name = ?2 WHERE scene_name = ?1",
                                            rusqlite::params![from_name, to_name],
                                        ) {
                                            Ok(affected) => {
                                                let body = serde_json::json!({"ok": true, "affected": affected});
                                                resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                            }
                                            Err(e) => {
                                                let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                                resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                }
                                Err(e) => {
                                    let body = serde_json::json!({"ok": false, "error": e});
                                    resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                            }
                        }
                    } else if method == "POST" && path.starts_with("/api/records/") && path.ends_with("/rename") {
                        let id_str = path.trim_start_matches("/api/records/").trim_end_matches("/rename");
                        let to_name = query_params.get("to").cloned().unwrap_or_default();
                        if to_name.is_empty() {
                            let body = serde_json::json!({"ok": false, "error": "参数 to 不能为空"});
                            resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                        } else if let Ok(rid) = id_str.parse::<i64>() {
                            match get_db(&app_data_dir) {
                                Ok(db) => match db.lock() {
                                    Ok(conn) => {
                                        match conn.execute(
                                            "UPDATE scene_records SET scene_name = ?2 WHERE id = ?1",
                                            rusqlite::params![rid, to_name],
                                        ) {
                                            Ok(affected) => {
                                                let body = serde_json::json!({"ok": true, "affected": affected});
                                                resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                            }
                                            Err(e) => {
                                                let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                                resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                }
                                Err(e) => {
                                    let body = serde_json::json!({"ok": false, "error": e});
                                    resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                            }
                        } else {
                            let body = serde_json::json!({"ok": false, "error": "无效的记录 ID"});
                            resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                        }
                    } else if method == "DELETE" && path.starts_with("/api/observations/") {
                        let id_str = path.trim_start_matches("/api/observations/");
                        if let Ok(observation_id) = id_str.parse::<i64>() {
                            match get_db(&app_data_dir) {
                                Ok(db) => match db.lock() {
                                    Ok(conn) => match conn.execute(
                                        "DELETE FROM drop_observations WHERE id = ?1",
                                        rusqlite::params![observation_id],
                                    ) {
                                        Ok(affected) => {
                                            let body = serde_json::json!({"ok": true, "deleted": affected});
                                            resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                        Err(error) => {
                                            let body = serde_json::json!({"ok": false, "error": error.to_string()});
                                            resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                    },
                                    Err(error) => {
                                        let body = serde_json::json!({"ok": false, "error": error.to_string()});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                },
                                Err(error) => {
                                    let body = serde_json::json!({"ok": false, "error": error});
                                    resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                            }
                        } else {
                            let body = serde_json::json!({"ok": false, "error": "无效的声纹观测 ID"});
                            resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                        }
                    } else if method == "DELETE" && path.starts_with("/api/records/") {
                        let subpath = path.trim_start_matches("/api/records/");
                        let path_parts: Vec<&str> = subpath.split('/').collect();
                        if path_parts.len() == 1 {
                            if let Ok(rid) = path_parts[0].parse::<i64>() {
                                match get_db(&app_data_dir) {
                                    Ok(db) => match db.lock() {
                                        Ok(conn) => match conn.execute(
                                            "DELETE FROM scene_records WHERE id = ?1",
                                            rusqlite::params![rid],
                                        ) {
                                            Ok(affected) => {
                                                let body = serde_json::json!({"ok": true, "deleted": affected});
                                                resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                            }
                                            Err(e) => {
                                                let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                                resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                            }
                                        },
                                        Err(e) => {
                                            let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                            resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                    },
                                    Err(e) => {
                                        let body = serde_json::json!({"ok": false, "error": e});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                }
                            } else {
                                let body = serde_json::json!({"ok": false, "error": "无效的记录 ID"});
                                resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                        } else if path_parts.len() == 3 && path_parts[1] == "drops" {
                            if let (Ok(rid), Ok(idx)) = (path_parts[0].parse::<i64>(), path_parts[2].parse::<usize>()) {
                                match get_db(&app_data_dir) {
                                    Ok(db) => match db.lock() {
                                        Ok(conn) => {
                                            match conn.query_row(
                                                "SELECT drops_json FROM scene_records WHERE id = ?1",
                                                rusqlite::params![rid],
                                                |row| row.get::<_, String>(0),
                                            ) {
                                                Ok(drops_json) => {
                                                    let mut drops = migrate_legacy_drops(&drops_json);
                                                    if idx < drops.len() {
                                                        drops.remove(idx);
                                                        let new_drops_json = serde_json::to_string(&drops).unwrap_or_default();
                                                        match conn.execute(
                                                            "UPDATE scene_records SET drops_json = ?2 WHERE id = ?1",
                                                            rusqlite::params![rid, new_drops_json],
                                                        ) {
                                                            Ok(affected) => {
                                                                let body = serde_json::json!({"ok": true, "affected": affected});
                                                                resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                                            }
                                                            Err(e) => {
                                                                let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                                                resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                                            }
                                                        }
                                                    } else {
                                                        let body = serde_json::json!({"ok": false, "error": "符文索引越界"});
                                                        resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                                    }
                                                }
                                                Err(e) => {
                                                    let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                                    resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let body = serde_json::json!({"ok": false, "error": e.to_string()});
                                            resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                        }
                                    }
                                    Err(e) => {
                                        let body = serde_json::json!({"ok": false, "error": e});
                                        resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                    }
                                }
                            } else {
                                let body = serde_json::json!({"ok": false, "error": "无效的记录 ID 或符文索引"});
                                resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                        } else {
                            let body = serde_json::json!({"ok": false, "error": "不支持的 DELETE 路径"});
                            resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                        }
                    } else if method == "GET" && path == "/api/preferences" {
                        match read_stats_page_preferences(&app_data_dir) {
                            Ok(preferences) => {
                                let body = serde_json::json!({"ok": true, "preferences": preferences});
                                resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                            Err(error) => {
                                let body = serde_json::json!({"ok": false, "error": error});
                                resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                        }
                    } else if method == "PUT" && path == "/api/preferences" {
                        match serde_json::from_str::<serde_json::Value>(request_body) {
                            Ok(preferences) => match write_stats_page_preferences(&app_data_dir, &preferences) {
                                Ok(()) => {
                                    let body = serde_json::json!({"ok": true});
                                    resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                                Err(error) => {
                                    let body = serde_json::json!({"ok": false, "error": error});
                                    resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                                }
                            },
                            Err(error) => {
                                let body = serde_json::json!({"ok": false, "error": format!("统计页偏好请求无效: {error}")});
                                resp_body = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                        }
                    } else if method == "GET" && path == "/api/records" {
                        match get_stats_data_inner(&app_data_dir) {
                            Ok(data) => {
                                let body = serde_json::to_string(&data).unwrap_or_default();
                                resp_body = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                            Err(e) => {
                                let body = serde_json::json!({"ok": false, "error": e});
                                resp_body = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, body);
                            }
                        }
                    } else {
                        resp_body = format!("HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n{}\r\n\r\n{}", cors_headers, r#"{"ok":false,"error":"Not found"}"#);
                    }
                }
                let _ = stream.write_all(resp_body.as_bytes());
            }
        }
        if STATS_API_GENERATION.load(std::sync::atomic::Ordering::SeqCst) == generation {
            STATS_API_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    });
    let api_thread = match api_thread {
        Ok(worker) => worker,
        Err(error) => {
            STATS_API_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(format!("启动统计 API 线程失败: {error}"));
        }
    };

    *runtime = Some(StatsApiRuntime {
        port,
        token: api_token.clone(),
        worker: api_thread,
    });
    Ok((port, api_token))
}

/// 转义 JSON 字符串以安全地嵌入 HTML `<script>` 标签中
fn escape_json_for_html_script(json: &str) -> String {
    json.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// 打开统计可视化页面。
#[tauri::command]
pub fn open_stats_page(
    state: tauri::State<'_, SharedState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| "全局配置尚未加载".to_string())?;
    if !config.optional_module_installed(
        crate::domain::config::OPTIONAL_MODULE_AUTOMATION,
    ) {
        return Err("识别与统计模块尚未安装".to_string());
    }
    // 1. 查询统计数据
    let stats_data = get_stats_data_inner(&state.app_data_dir)?;
    let stats_json =
        serde_json::to_string(&stats_data).map_err(|e| format!("JSON 序列化失败: {}", e))?;

    // 2. 读取 stats.html 模板（优先资源目录，开发模式回退项目目录）
    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))?;
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("docs")
        .join("stats.html");
    // 调试运行时工作区模板是事实来源，避免 target/debug/_up_ 中的旧打包副本遮蔽最新页面。
    // 发布版本仍然优先使用安装包资源，项目目录只作为最后回退。
    let template_path = stats_template_candidates(&resource_dir, &dev_path, cfg!(debug_assertions))
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            format!(
                "stats.html 模板不存在 (资源目录: {}, 开发路径: {})",
                resource_dir.display(),
                dev_path.display()
            )
        })?;

    let template = std::fs::read_to_string(&template_path).map_err(|e| {
        format!(
            "读取 stats.html 模板失败 ({}): {}",
            template_path.display(),
            e
        )
    })?;

    // 3. 启动统计 API 服务并注入端口号
    let (api_port, api_token) = start_stats_api(state.app_data_dir.clone())?;
    let stats_json = escape_json_for_html_script(&stats_json);
    let preferences_json = read_stats_page_preferences(&state.app_data_dir)?
        .map(|preferences| serde_json::to_string(&preferences))
        .transpose()
        .map_err(|error| format!("序列化统计页偏好失败: {error}"))?
        .unwrap_or_else(|| "null".to_string());
    let preferences_json = escape_json_for_html_script(&preferences_json);
    let stats_theme = state
        .configuration()
        .snapshot()
        .map(|config| config.theme)
        .unwrap_or_else(|| "light".to_string());
    let html = render_stats_template(
        &template,
        &stats_json,
        &preferences_json,
        api_port,
        &api_token,
        &stats_theme,
    );

    // 4. 写入 stateData/stats.html（使相对路径 img/ 可用）
    let state_data_dir = Path::new(&state.app_data_dir).join("stateData");
    std::fs::create_dir_all(&state_data_dir)
        .map_err(|e| format!("创建 stateData 目录失败: {}", e))?;
    let html_path = state_data_dir.join("stats.html");
    std::fs::write(&html_path, html).map_err(|e| format!("写入 stats.html 失败: {}", e))?;

    // 5. 用默认浏览器打开
    open::that(html_path.to_string_lossy().as_ref())
        .map_err(|e| format!("打开浏览器失败: {}", e))?;

    Ok(())
}
/// 内部函数：直接从 app_data_dir 读取统计数据（避免移动 state）
fn get_stats_data_inner(app_data_dir: &str) -> Result<StatsData, String> {
    let db = get_db(app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
    let records = query_scene_records(&conn)?;
    let observations = query_drop_observations(&conn)?;
    let strategies = query_merge_strategies(&conn)?;

    Ok(StatsData {
        records,
        observations,
        strategies,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        delete_scene_records_by_ids, ensure_drop_observation_schema, ensure_scene_segment_columns,
        migrate_legacy_drops, normalize_strategy, query_scene_stats, request_content_length,
        stats_api_token_is_valid, update_merge_strategy, DropKind, STATS_API_CORS_HEADERS,
    };
    use rusqlite::Connection;

    #[test]
    fn scene_stats_separate_normal_and_terror_zone_records() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE scene_records (
                scene_name TEXT NOT NULL,
                tz INTEGER NOT NULL DEFAULT 0,
                timer_seconds REAL NOT NULL
            );
            INSERT INTO scene_records (scene_name, tz, timer_seconds) VALUES
                ('营房', 0, 20.0),
                ('营房', 0, 40.0),
                ('营房', 1, 120.0);",
        )
        .unwrap();

        let normal = query_scene_stats(&conn, "营房", false).unwrap().unwrap();
        let terror_zone = query_scene_stats(&conn, "营房", true).unwrap().unwrap();
        assert_eq!(normal.total_runs, 2);
        assert_eq!(normal.avg_time, 30.0);
        assert_eq!(terror_zone.total_runs, 1);
        assert_eq!(terror_zone.avg_time, 120.0);
        assert!(query_scene_stats(&conn, "不存在", false).unwrap().is_none());
    }

    #[test]
    fn legacy_scene_table_gains_tracking_columns_without_losing_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE scene_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                absolute_time TEXT NOT NULL,
                character_name TEXT NOT NULL,
                scene_name TEXT NOT NULL,
                timer_seconds REAL NOT NULL,
                drops_json TEXT NOT NULL DEFAULT '[]'
            );
            INSERT INTO scene_records
                (absolute_time, character_name, scene_name, timer_seconds, drops_json)
            VALUES ('2026/08/26/12:00:00', '旧角色', '旧场景', 10.0, '[]');",
        )
        .unwrap();

        ensure_scene_segment_columns(&conn).unwrap();
        ensure_scene_segment_columns(&conn).unwrap();
        let columns = conn
            .prepare("PRAGMA table_info(scene_records)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.contains(&"journey_id".to_string()));
        assert!(columns.contains(&"segment_index".to_string()));
        assert!(columns.contains(&"tz".to_string()));
        assert!(!conn
            .query_row("SELECT tz FROM scene_records LIMIT 1", [], |row| {
                row.get::<_, bool>(0)
            })
            .unwrap());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM scene_records", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
    }

    #[test]
    fn legacy_rune_observations_migrate_once_into_generic_drops() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE scene_records (id INTEGER PRIMARY KEY);
             INSERT INTO scene_records(id) VALUES (9);
             CREATE TABLE rune_drop_observations (
                 id INTEGER PRIMARY KEY,
                 observed_at TEXT NOT NULL,
                 account_id TEXT NOT NULL,
                 rune_number INTEGER NOT NULL,
                 rune_name TEXT NOT NULL,
                 rune_name_en TEXT NOT NULL,
                 confidence REAL NOT NULL,
                 source TEXT NOT NULL,
                 scene_record_id INTEGER
             );
             INSERT INTO rune_drop_observations
                 (id, observed_at, account_id, rune_number, rune_name, rune_name_en,
                  confidence, source, scene_record_id)
             VALUES (7, '2026-08-27T00:00:00+08:00', 'account', 15,
                     '海尔', 'Hel', 0.91, 'rune_audio', 9);",
        )
        .unwrap();

        ensure_drop_observation_schema(&conn).unwrap();
        ensure_drop_observation_schema(&conn).unwrap();
        let migrated = conn
            .query_row(
                "SELECT kind, telemetry_id, item_code, display_name, scene_record_id
                   FROM drop_observations",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            migrated,
            (
                "rune".to_string(),
                15,
                "r15".to_string(),
                "海尔".to_string(),
                Some(9)
            )
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM drop_observations", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        conn.execute("DELETE FROM scene_records WHERE id = 9", [])
            .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT scene_record_id FROM drop_observations WHERE id = 1",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .unwrap(),
            None
        );
        conn.execute("DELETE FROM drop_observations WHERE id = 1", [])
            .unwrap();
        ensure_drop_observation_schema(&conn).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM rune_drop_observations", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM drop_observations", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn historical_rune_json_and_new_item_json_share_one_drop_model() {
        let legacy =
            migrate_legacy_drops(r#"[{"rune_number":15,"rune_name":"海尔","rune_name_en":"Hel"}]"#);
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].kind, DropKind::Rune);
        assert_eq!(legacy[0].telemetry_id, 15);
        assert_eq!(legacy[0].display_name, "海尔");

        let item = migrate_legacy_drops(
            r#"[{"kind":"item","telemetry_id":40,"item_code":"pk1","category":"keys","display_name":"恐惧之钥","display_name_en":"Key of Terror"}]"#,
        );
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].kind, DropKind::Item);
        assert_eq!(item[0].item_code.as_deref(), Some("pk1"));
        assert_eq!(item[0].rune_number, None);
    }

    #[test]
    fn strategy_normalization_trims_and_deduplicates_scene_names() {
        let (name, scenes) = normalize_strategy(
            " 女伯爵 ",
            vec![
                "黑色荒地".to_string(),
                " 遗忘之塔地牢第1层 ".to_string(),
                "黑色荒地".to_string(),
                "".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(name, "女伯爵");
        assert_eq!(scenes, ["黑色荒地", "遗忘之塔地牢第1层"]);
    }

    #[test]
    fn strategy_requires_a_name_and_at_least_one_scene() {
        assert!(normalize_strategy("", vec!["黑色荒地".to_string()]).is_err());
        assert!(normalize_strategy("女伯爵", vec![" ".to_string()]).is_err());
    }

    #[test]
    fn custom_strategy_can_be_updated_without_replacing_its_id() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE stats_merge_strategies (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    scene_names_json TEXT NOT NULL
                );
                INSERT INTO stats_merge_strategies (name, scene_names_json)
                VALUES ('旧路线', '[\"黑色荒地\"]');",
            )
            .unwrap();
        let updated = update_merge_strategy(
            &connection,
            1,
            " 新路线 ",
            vec!["黑色荒地".to_string(), "毁灭王座".to_string()],
        )
        .unwrap();
        assert_eq!(updated.id, 1);
        assert_eq!(updated.name, "新路线");
        assert_eq!(updated.scene_names, ["黑色荒地", "毁灭王座"]);
    }

    #[test]
    fn batch_delete_is_atomic_and_deduplicates_record_ids() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute(
                "CREATE TABLE scene_records (id INTEGER PRIMARY KEY, scene_name TEXT NOT NULL)",
                [],
            )
            .unwrap();
        for id in 1..=4 {
            connection
                .execute(
                    "INSERT INTO scene_records (id, scene_name) VALUES (?1, 'test')",
                    rusqlite::params![id],
                )
                .unwrap();
        }

        let deleted = delete_scene_records_by_ids(&mut connection, &[2, 3, 3, -1]).unwrap();
        assert_eq!(deleted, 2);
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM scene_records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 2);
    }

    #[test]
    fn request_content_length_is_case_insensitive() {
        assert_eq!(
            request_content_length(b"DELETE / HTTP/1.1\r\ncontent-Length: 42\r\n\r\n"),
            42
        );
        assert_eq!(request_content_length(b"GET / HTTP/1.1\r\n\r\n"), 0);
    }

    #[test]
    fn stats_api_requires_the_exact_session_token() {
        let headers = "GET /api/records HTTP/1.1\r\nX-D2RHub-Stats-Token: secret\r\n\r\n";
        assert!(stats_api_token_is_valid(headers, "secret"));
        assert!(!stats_api_token_is_valid(headers, "other"));
        assert!(!stats_api_token_is_valid(
            "GET /api/records HTTP/1.1\r\n\r\n",
            "secret"
        ));
    }

    #[test]
    fn stats_api_cors_is_limited_to_the_local_file_origin() {
        assert!(STATS_API_CORS_HEADERS.contains("Access-Control-Allow-Origin: null"));
        assert!(STATS_API_CORS_HEADERS.contains("X-D2RHub-Stats-Token"));
        assert!(!STATS_API_CORS_HEADERS.contains("Access-Control-Allow-Origin: *"));
    }
}
