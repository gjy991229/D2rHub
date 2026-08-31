use std::path::{Path, PathBuf};

fn detect_saved_games_path_for_edition(cn: bool) -> Option<String> {
    let saved_games = dirs::home_dir()?.join("Saved Games");
    std::fs::read_dir(&saved_games)
        .ok()?
        .flatten()
        .find_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_d2r = name.starts_with("Diablo II Resurrected");
            let is_cn = name.to_ascii_lowercase().contains("(cn)");
            (is_d2r && is_cn == cn).then(|| saved_games.join(name).to_string_lossy().to_string())
        })
}

/// 自动探测国服游戏存档路径。
#[tauri::command]
pub fn detect_saved_games_path() -> Option<String> {
    detect_saved_games_path_for_edition(true)
}

/// 自动探测国际服游戏存档路径。
#[tauri::command]
pub fn detect_global_saved_games_path() -> Option<String> {
    detect_saved_games_path_for_edition(false)
}

/// 检测 ProgramData 下的 Agent 路径。
#[tauri::command]
pub fn detect_program_data_agent_path() -> Option<String> {
    let path = r"C:\ProgramData\Battle.net\Agent";
    Path::new(path).exists().then(|| path.to_string())
}

/// 检测 AppData\Roaming\Battle.net 路径。
#[tauri::command]
pub fn detect_app_data_roaming_bnet_path() -> Option<String> {
    let battle_net = dirs::config_dir()?.join("Battle.net");
    battle_net
        .exists()
        .then(|| battle_net.to_string_lossy().to_string())
}

fn browser_candidates(browser_type: &str) -> Vec<PathBuf> {
    let mut candidates = match browser_type {
        "edge" => vec![
            PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
            PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
        ],
        "chrome" => vec![
            PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
        ],
        _ => return Vec::new(),
    };

    if let Some(local) = dirs::data_local_dir() {
        let local_path = match browser_type {
            "edge" => local.join("Microsoft/Edge/Application/msedge.exe"),
            "chrome" => local.join("Google/Chrome/Application/chrome.exe"),
            _ => unreachable!(),
        };
        candidates.push(local_path);
    }
    candidates
}

fn detect_browser(browser_type: &str) -> Option<String> {
    browser_candidates(browser_type)
        .into_iter()
        .find(|path| path.exists())
        .map(|path| path.to_string_lossy().to_string())
}

/// 自动探测浏览器路径（Edge 优先，其次 Chrome）。
#[tauri::command]
pub fn detect_browser_path() -> Option<(String, String)> {
    ["edge", "chrome"].into_iter().find_map(|browser_type| {
        detect_browser(browser_type).map(|path| (path, browser_type.to_string()))
    })
}

/// 根据选择的浏览器类型自动探测路径。
#[tauri::command]
pub fn detect_browser_path_by_type(browser_type: String) -> Option<String> {
    detect_browser(&browser_type)
}
