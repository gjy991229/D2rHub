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

/// Battle.net 与 Agent 会互相拉起进程；必须先停止 Agent，再清理 Battle.net。
/// 其余进程名保持调用方给出的顺序，并按名称忽略大小写去重。
fn ordered_process_names<'a>(names: &'a [&'a str]) -> Vec<&'a str> {
    let mut ordered = Vec::with_capacity(names.len());
    for preferred in ["Agent.exe", "Battle.net.exe"] {
        if let Some(name) = names
            .iter()
            .copied()
            .find(|name| name.eq_ignore_ascii_case(preferred))
        {
            ordered.push(name);
        }
    }
    for name in names.iter().copied() {
        if !ordered
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(name))
        {
            ordered.push(name);
        }
    }
    ordered
}

fn matching_processes(names: &[&str]) -> Vec<(sysinfo::Pid, String)> {
    use sysinfo::ProcessesToUpdate;

    let mut sys = shared_system()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    sys.refresh_processes(ProcessesToUpdate::All);
    sys.processes()
        .values()
        .filter_map(|process| {
            let process_name = process.name().to_string_lossy();
            names
                .iter()
                .any(|target| process_name.eq_ignore_ascii_case(target))
                .then(|| (process.pid(), process_name.into_owned()))
        })
        .collect()
}

/// 强制关闭指定名称的进程，并确认它们已退出。
///
/// Windows 下先使用 `/IM` 一次终止同名进程，避免逐 PID 清理时 Agent 与 Battle.net
/// 互相重启；若整批终止失败，再按本轮快照中的 PID 兜底。这里不能使用 `/T`：该函数
/// 也会在游戏启动后调用，递归终止进程树可能误杀仍在运行的 D2R.exe。
pub fn kill_processes_by_name(names: &[&str]) -> Result<(), crate::error::AppError> {
    let ordered_names = ordered_process_names(names);
    if ordered_names.is_empty() {
        return Ok(());
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let snapshot = matching_processes(&ordered_names);
        if snapshot.is_empty() {
            return Ok(());
        }

        #[cfg(target_os = "windows")]
        {
            for name in &ordered_names {
                let image_killed = silent_cmd("taskkill")
                    .args(["/F", "/IM", name])
                    .output()
                    .is_ok_and(|output| output.status.success());

                if !image_killed {
                    for (pid, process_name) in &snapshot {
                        if process_name.eq_ignore_ascii_case(name) {
                            let _ = silent_cmd("taskkill")
                                .args(["/F", "/PID", &pid.to_string()])
                                .output();
                        }
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let sys = shared_system()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            for (pid, _) in &snapshot {
                if let Some(process) = sys.process(*pid) {
                    let _ = process.kill();
                }
            }
        }

        // 终止命令成功只代表请求已被系统接受；短暂等待后必须重新枚举确认。
        std::thread::sleep(std::time::Duration::from_millis(200));
        let remaining = matching_processes(&ordered_names);
        if remaining.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(crate::error::AppError::Unknown(format!(
                "无法在超时前终止共享进程: {}",
                remaining
                    .iter()
                    .map(|(pid, name)| format!("{name}({pid})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
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

#[cfg(test)]
mod process_kill_tests {
    use super::ordered_process_names;

    #[test]
    fn agent_is_stopped_before_battle_net_and_names_are_deduplicated() {
        assert_eq!(
            ordered_process_names(&["Battle.net.exe", "helper.exe", "agent.EXE", "HELPER.EXE",]),
            vec!["agent.EXE", "Battle.net.exe", "helper.exe"]
        );
    }
}
