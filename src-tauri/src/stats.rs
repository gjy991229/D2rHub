use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;

use crate::state::SharedState;
use crate::stats_page::{render_stats_template, stats_template_candidates};

/// 单条符文掉落记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneDropEntry {
    /// 符文编号 1-33
    pub rune_number: u32,
    /// 符文中文名
    pub rune_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rune_name_en: Option<String>,
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
    pub timer_seconds: f64,
    /// 新版：符文掉落数组（每个元素 = 一次独立掉落）
    pub drops: Vec<RuneDropEntry>,
}

/// 全部统计数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsData {
    pub records: Vec<SceneRecord>,
}

/// 懒初始化数据库连接
static DB: std::sync::OnceLock<Mutex<Connection>> = std::sync::OnceLock::new();

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
            timer_seconds REAL NOT NULL,
            drops_json TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_scene_time ON scene_records(absolute_time);
        CREATE INDEX IF NOT EXISTS idx_scene_name ON scene_records(scene_name);",
    )
    .map_err(|e| format!("初始化数据表失败: {}", e))?;
    let _ = DB.set(Mutex::new(conn));
    DB.get().ok_or_else(|| "数据库初始化状态不可用".to_string())
}

/// 将旧版 drops HashMap 格式迁移为新版 Vec<RuneDropEntry> 格式
fn migrate_legacy_drops(drops_json: &str) -> Vec<RuneDropEntry> {
    // 尝试解析为新版数组格式
    if let Ok(drops) = serde_json::from_str::<Vec<RuneDropEntry>>(drops_json) {
        if !drops.is_empty() || drops_json.trim().starts_with('[') {
            return drops;
        }
    }
    // 尝试解析为旧版 HashMap 格式 { "符文名": count }
    if let Ok(legacy) = serde_json::from_str::<HashMap<String, u32>>(drops_json) {
        let mut result = Vec::new();
        for (name, count) in legacy {
            let rune_number = crate::rune_data::get_rune_number(&name).unwrap_or(0);
            for _ in 0..count {
                result.push(RuneDropEntry {
                    rune_number,
                    rune_name: name.clone(),
                    rune_name_en: None,
                    screenshot_path: None,
                });
            }
        }
        return result;
    }
    Vec::new()
}

/// 保存一条场景记录
#[tauri::command]
pub fn save_scene_record(
    state: tauri::State<'_, SharedState>,
    record: SceneRecord,
) -> Result<(), String> {
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;

    let drops_json =
        serde_json::to_string(&record.drops).map_err(|e| format!("序列化掉落失败: {}", e))?;

    conn.execute(
        "INSERT INTO scene_records (absolute_time, character_name, scene_name, timer_seconds, drops_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            record.absolute_time,
            record.character_name,
            record.scene_name,
            record.timer_seconds,
            drops_json,
        ],
    )
    .map_err(|e| format!("保存记录失败: {}", e))?;

    Ok(())
}

/// 获取所有统计数据（结构体形式，供前端使用）
#[tauri::command]
pub fn get_stats_data(state: tauri::State<'_, SharedState>) -> Result<StatsData, String> {
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;

    let mut stmt = conn
        .prepare("SELECT id, absolute_time, character_name, scene_name, timer_seconds, drops_json FROM scene_records ORDER BY id ASC")
        .map_err(|e| format!("查询准备失败: {}", e))?;

    let records: Vec<SceneRecord> = stmt
        .query_map([], |row| {
            let drops_json: String = row.get(5)?;
            let drops = migrate_legacy_drops(&drops_json);
            Ok(SceneRecord {
                id: Some(row.get(0)?),
                absolute_time: row.get(1)?,
                character_name: row.get(2)?,
                scene_name: row.get(3)?,
                timer_seconds: row.get(4)?,
                drops,
            })
        })
        .map_err(|e| format!("查询执行失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(StatsData { records })
}

/// 获取统计数据 JSON 字符串（用于嵌入 HTML 页面）
#[tauri::command]
pub fn get_stats_json(state: tauri::State<'_, SharedState>) -> Result<String, String> {
    let data = get_stats_data(state)?;
    serde_json::to_string(&data).map_err(|e| format!("JSON 序列化失败: {}", e))
}

/// 查询指定场景的历史平均耗时
#[tauri::command]
pub fn get_scene_avg_time(
    state: tauri::State<'_, SharedState>,
    scene_name: String,
) -> Result<Option<f64>, String> {
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;

    let mut stmt = conn
        .prepare("SELECT AVG(timer_seconds) FROM scene_records WHERE scene_name = ?1")
        .map_err(|e| format!("查询准备失败: {}", e))?;

    let result: Option<f64> = stmt
        .query_row(rusqlite::params![scene_name], |row| row.get(0))
        .optional()
        .map_err(|e| format!("查询平均耗时失败: {}", e))?
        .flatten();

    Ok(result)
}

/// 场景统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneStats {
    pub avg_time: f64,
    pub total_runs: i64,
}

/// 查询指定场景的统计信息（平均耗时，总场次）
#[tauri::command]
pub fn get_scene_stats(
    state: tauri::State<'_, SharedState>,
    scene_name: String,
) -> Result<Option<SceneStats>, String> {
    let db = get_db(&state.app_data_dir)?;
    let conn = db.lock().map_err(|e| format!("数据库锁失败: {}", e))?;

    let mut stmt = conn
        .prepare("SELECT AVG(timer_seconds), COUNT(*) FROM scene_records WHERE scene_name = ?1")
        .map_err(|e| format!("查询准备失败: {}", e))?;

    let result: Option<SceneStats> = stmt
        .query_row(rusqlite::params![scene_name], |row| {
            let avg: Option<f64> = row.get(0)?;
            let count: i64 = row.get(1)?;
            if let Some(a) = avg {
                Ok(Some(SceneStats {
                    avg_time: a,
                    total_runs: count,
                }))
            } else {
                Ok(None)
            }
        })
        .optional()
        .map_err(|e| format!("查询场景统计失败: {}", e))?
        .flatten();

    Ok(result)
}

/// 删除一条场景记录（不可恢复）
#[tauri::command]
pub fn delete_scene_record(state: tauri::State<'_, SharedState>, id: i64) -> Result<(), String> {
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

/// 统计 API 服务端口
static STATS_API_PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
static STATS_API_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 启动统计页微 HTTP API 服务（供浏览器中的 stats.html 调用）
fn start_stats_api(app_data_dir: String) -> Result<u16, String> {
    if STATS_API_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return STATS_API_PORT
            .get()
            .copied()
            .ok_or_else(|| "统计 API 正在启动，请稍后重试".to_string());
    }

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

    let api_thread = std::thread::Builder::new().name("stats-api".into()).spawn(move || {
        for stream in listener.incoming() {
            if !STATS_API_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            if let Ok(mut stream) = stream {
                use std::io::{Read, Write};
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(3)));

                // 读取请求头
                let mut buf = [0u8; 4096];
                let n = match stream.read(&mut buf) {
                    Ok(n) if n > 0 => n,
                    _ => continue,
                };
                let raw = String::from_utf8_lossy(&buf[..n]);
                let first_line = raw.lines().next().unwrap_or("");
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() < 2 { continue; }
                let method = parts[0];
                let full_path = parts[1];

                // CORS 相关
                let cors_headers = "Access-Control-Allow-Origin: *\r\n\
                                    Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\
                                    Access-Control-Allow-Headers: Content-Type\r\n";

                let resp_body: String;

                if method == "OPTIONS" {
                    resp_body = format!("HTTP/1.1 204 No Content\r\n{}\r\n", cors_headers);
                } else {
                    let mut path_parts_split = full_path.split('?');
                    let path = path_parts_split.next().unwrap_or("");
                    let query_str = path_parts_split.next().unwrap_or("");
                    let query_params = parse_query(query_str);

                    if method == "POST" && path == "/api/scenes/rename" {
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
    });
    if let Err(error) = api_thread {
        STATS_API_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        return Err(format!("启动统计 API 线程失败: {error}"));
    }

    let _ = STATS_API_PORT.set(port);
    Ok(port)
}

/// 转义 JSON 字符串以安全地嵌入 HTML `<script>` 标签中
fn escape_json_for_html_script(json: &str) -> String {
    json.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// 打开统计可视化页面
#[tauri::command]
pub fn open_stats_page(
    state: tauri::State<'_, SharedState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
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
    let api_port = start_stats_api(state.app_data_dir.clone())?;
    let stats_json = escape_json_for_html_script(&stats_json);
    let stats_theme = state
        .config
        .read()
        .as_ref()
        .map(|config| config.theme.as_str())
        .unwrap_or("light")
        .to_string();
    let html = render_stats_template(&template, &stats_json, api_port, &stats_theme);

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

    let mut stmt = conn
        .prepare("SELECT id, absolute_time, character_name, scene_name, timer_seconds, drops_json FROM scene_records ORDER BY id ASC")
        .map_err(|e| format!("查询准备失败: {}", e))?;

    let records: Vec<SceneRecord> = stmt
        .query_map([], |row| {
            let drops_json: String = row.get(5)?;
            let drops = migrate_legacy_drops(&drops_json);
            Ok(SceneRecord {
                id: Some(row.get(0)?),
                absolute_time: row.get(1)?,
                character_name: row.get(2)?,
                scene_name: row.get(3)?,
                timer_seconds: row.get(4)?,
                drops,
            })
        })
        .map_err(|e| format!("查询执行失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(StatsData { records })
}
