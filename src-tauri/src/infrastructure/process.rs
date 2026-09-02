use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

static SHARED_SYSTEM: std::sync::OnceLock<std::sync::Mutex<sysinfo::System>> =
    std::sync::OnceLock::new();

pub(crate) fn shared_system() -> &'static std::sync::Mutex<sysinfo::System> {
    SHARED_SYSTEM.get_or_init(|| std::sync::Mutex::new(sysinfo::System::new()))
}

pub(crate) fn silent_cmd(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    cmd
}

pub(crate) fn ordered_process_names<'a>(names: &'a [&'a str]) -> Vec<&'a str> {
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

    let mut system = shared_system()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    system.refresh_processes(ProcessesToUpdate::All);
    system
        .processes()
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

pub(crate) fn kill_processes_by_name(
    names: &[&str],
) -> Result<(), crate::error::AppError> {
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

        #[cfg(not(target_os = "windows"))]
        {
            let system = shared_system()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            for (pid, _) in &snapshot {
                if let Some(process) = system.process(*pid) {
                    let _ = process.kill();
                }
            }
        }

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
