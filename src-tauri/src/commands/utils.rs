pub(crate) use crate::infrastructure::process::{
    kill_processes_by_name, shared_system, silent_cmd,
};

#[cfg(test)]
use crate::infrastructure::process::ordered_process_names;

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
