use chrono::Local;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();
static LOGS_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Bridge crates that use the standard `log` facade into D2RHub's local log.
///
/// Keeping one sink matters for recovery paths: configuration recovery runs
/// before the UI exists and historically used `log::warn!`/`log::error!`, while
/// the application only initialized this custom file writer. Those messages
/// now follow the same retention and support-export path as direct `log_msg`
/// calls.
struct D2rHubLogger;

static LOGGER: D2rHubLogger = D2rHubLogger;

fn system_logs_dir() -> PathBuf {
    dirs::config_dir()
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .join("D2RHub")
        .join("logs")
}

fn legacy_portable_logs_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("logs")))
        .unwrap_or_else(|| PathBuf::from("./logs"))
}

fn available_migration_target(target: &Path, original_name: &std::ffi::OsStr) -> PathBuf {
    let direct = target.join(original_name);
    if !direct.exists() {
        return direct;
    }
    let original = Path::new(original_name);
    let stem = original
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("legacy-log");
    let extension = original
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("log");
    for suffix in 1..=10_000_u32 {
        let candidate = target.join(format!("{stem}-legacy-{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    target.join(format!("legacy-{}.log", Local::now().timestamp_millis()))
}

fn migrate_legacy_log_files(source: &Path, target: &Path) -> Vec<String> {
    if source == target || !source.is_dir() {
        return Vec::new();
    }
    let mut warnings = Vec::new();
    let Ok(entries) = fs::read_dir(source) else {
        return vec![format!("无法读取旧日志目录 {}", source.display())];
    };
    for entry in entries.flatten() {
        let source_path = entry.path();
        if !source_path.is_file() || source_path.extension().is_none_or(|ext| ext != "log") {
            continue;
        }
        let target_path = available_migration_target(target, &entry.file_name());
        match fs::copy(&source_path, &target_path) {
            Ok(_) => {
                if let Err(error) = fs::remove_file(&source_path) {
                    warnings.push(format!(
                        "旧日志已复制但无法移除 {}：{error}",
                        source_path.display()
                    ));
                }
            }
            Err(error) => warnings.push(format!(
                "无法迁移旧日志 {} 到 {}：{error}",
                source_path.display(),
                target_path.display()
            )),
        }
    }
    let _ = fs::remove_dir(source);
    warnings
}

impl log::Log for D2rHubLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            log_msg(
                record.level().as_str(),
                record.target(),
                &record.args().to_string(),
            );
        }
    }

    fn flush(&self) {
        if let Some(mutex) = LOG_FILE.get() {
            if let Ok(mut file) = mutex.lock() {
                let _ = file.flush();
            }
        }
    }
}

/// 初始化日志模块。每次打开新建一个 YYYY-MM-DD_HH-MM-SS.log 日志文件，最多保留16个文件。
pub fn init_logger() -> Result<(), String> {
    let logs_dir = system_logs_dir();

    // 创建日志文件夹
    fs::create_dir_all(&logs_dir).map_err(|e| format!("创建日志文件夹失败: {}", e))?;
    let migration_warnings = migrate_legacy_log_files(&legacy_portable_logs_dir(), &logs_dir);

    // 获取日志文件夹内所有 .log 文件
    let mut log_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "log") {
                log_files.push(path);
            }
        }
    }

    // 按修改时间升序排序（最早的在最前面）
    log_files.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| std::time::SystemTime::now())
    });

    // 如果日志数量大于等于 16 个，删除最早的，直到只剩 15 个，以便为新文件腾出空间
    if log_files.len() >= 16 {
        let delete_count = log_files.len() - 15;
        for path in log_files.iter().take(delete_count) {
            let _ = fs::remove_file(path);
        }
    }

    // 创建新的日志文件，以当前本地日期时间命名
    let now = Local::now();
    let filename = now.format("%Y-%m-%d_%H-%M-%S.log").to_string();
    let log_file_path = logs_dir.join(&filename);

    let file = File::create(&log_file_path).map_err(|e| format!("创建日志文件失败: {}", e))?;

    let _ = LOG_FILE.set(Mutex::new(file));
    let _ = LOGS_DIR.set(logs_dir.clone());

    // `set_logger` may legitimately fail when an embedding test/runtime has
    // already installed a process-wide logger. Direct D2RHub logging remains
    // available in that case, so initialization should not fail or alter the
    // logger already owned by the host.
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    } else {
        log_msg(
            "WARN",
            "Logger",
            "标准日志桥接未安装：进程已存在其他日志实现",
        );
    }

    if migration_warnings.is_empty() {
        log_msg(
            "INFO",
            "Logger",
            &format!(
                "日志目录：{}",
                LOGS_DIR.get().map_or_else(
                    || logs_dir.display().to_string(),
                    |path| path.display().to_string()
                )
            ),
        );
    } else {
        for warning in migration_warnings {
            log_msg("WARN", "Logger", &warning);
        }
    }

    Ok(())
}

/// 打印并记录一条日志
pub fn log_msg(level: &str, module: &str, message: &str) {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let log_line = format!("[{}] [{}] [{}] {}\n", now, level, module, message);

    // 输出到标准输出
    print!("{}", log_line);

    // 写入文件
    if let Some(mutex) = LOG_FILE.get() {
        if let Ok(mut file) = mutex.lock() {
            let _ = file.write_all(log_line.as_bytes());
            let _ = file.flush();
        }
    }
}

/// 获取日志文件夹路径
pub fn get_logs_dir() -> Option<PathBuf> {
    LOGS_DIR.get().cloned()
}

/// 提供给前端调用的日志记录接口
#[tauri::command]
pub fn write_log(level: String, message: String) {
    log_msg(&level.to_uppercase(), "Frontend", &message);
}

#[cfg(test)]
mod tests {
    use super::migrate_legacy_log_files;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "d2rhub_logger_{label}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn legacy_logs_move_to_system_directory_without_overwriting_collisions() {
        let root = temp_dir("migration");
        let source = root.join("portable-logs");
        let target = root.join("system-logs");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(source.join("same.log"), "portable").unwrap();
        std::fs::write(source.join("keep.txt"), "not a log").unwrap();
        std::fs::write(target.join("same.log"), "system").unwrap();

        assert!(migrate_legacy_log_files(&source, &target).is_empty());
        assert_eq!(
            std::fs::read_to_string(target.join("same.log")).unwrap(),
            "system"
        );
        let migrated = std::fs::read_dir(&target)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.file_name().is_some_and(|name| name != "same.log"))
            .unwrap();
        assert_eq!(std::fs::read_to_string(migrated).unwrap(), "portable");
        assert!(source.join("keep.txt").is_file());
        assert!(!source.join("same.log").exists());

        std::fs::remove_dir_all(root).unwrap();
    }
}
