use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// ── 共享 System 实例 ──
static SHARED_SYSTEM: std::sync::OnceLock<std::sync::Mutex<sysinfo::System>> =
    std::sync::OnceLock::new();

pub fn shared_system() -> &'static std::sync::Mutex<sysinfo::System> {
    SHARED_SYSTEM.get_or_init(|| std::sync::Mutex::new(sysinfo::System::new()))
}

/// 创建静默命令（Windows 下隐藏 CMD 窗口）
pub fn silent_cmd(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
}

/// 清理/过滤用户昵称中的 Windows 不合法文件夹字符
pub fn sanitize_folder_name(name: &str) -> String {
    let mut sanitized: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect();
    sanitized = sanitized.trim().to_string();
    if sanitized.is_empty() {
        "D2RHub_Default".to_string()
    } else {
        sanitized
    }
}

/// 强制关闭指定名称的进程，并确认它们已退出。
/// 调用方可据此避免在旧 Battle.net/Agent 仍写入共享状态时切换账号。
pub fn kill_processes_by_name(names: &[&str]) -> Result<(), crate::error::AppError> {
    use sysinfo::ProcessesToUpdate;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    loop {
        let remaining = {
            let mut sys = shared_system()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            sys.refresh_processes(ProcessesToUpdate::All);

            let mut remaining = Vec::new();
            for process in sys.processes().values() {
                let process_name = process.name().to_string_lossy();
                if names
                    .iter()
                    .any(|target| process_name.eq_ignore_ascii_case(target))
                {
                    remaining.push(format!("{}({})", process_name, process.pid()));
                    if !process.kill() {
                        let _ = silent_cmd("taskkill")
                            .args(["/F", "/PID", &process.pid().to_string()])
                            .output();
                    }
                }
            }
            remaining
        };

        if remaining.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(crate::error::AppError::Unknown(format!(
                "无法在超时前终止共享进程: {}",
                remaining.join(", ")
            )));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// 递归复制目录
pub fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
