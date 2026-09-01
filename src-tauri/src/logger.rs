use chrono::Local;
use std::fs::{self, File};
use std::io::Write;
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
    let logs_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|parent| parent.join("logs")))
        .unwrap_or_else(|| std::path::PathBuf::from("./logs"));

    // 创建日志文件夹
    fs::create_dir_all(&logs_dir).map_err(|e| format!("创建日志文件夹失败: {}", e))?;

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
    let _ = LOGS_DIR.set(logs_dir);

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
