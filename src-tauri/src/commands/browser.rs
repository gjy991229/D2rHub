use crate::commands::account::AccountManager;
use crate::commands::global_config::GlobalConfig;
use crate::commands::utils::{sanitize_folder_name, shared_system, silent_cmd};
use crate::error::AppError;
use crate::launch_context::{paths_have_same_identity, AuthMode};
use crate::state::{AccountLifecycleLease, SharedState};
use sysinfo::ProcessesToUpdate;

fn paths_match_config(config_path: &str, requested_path: &str) -> bool {
    if config_path.trim().is_empty() || requested_path.trim().is_empty() {
        return false;
    }
    paths_have_same_identity(
        std::path::Path::new(config_path),
        std::path::Path::new(requested_path),
    )
}

fn ensure_browser_path_allowed(config: &GlobalConfig, browser_path: &str) -> Result<(), AppError> {
    if !paths_match_config(&config.browser_path, browser_path) {
        return Err(AppError::FileError(
            "浏览器路径必须使用已保存的全局配置".to_string(),
        ));
    }
    Ok(())
}

fn ensure_allowed_bnet_login_url(url: &str) -> Result<(), AppError> {
    let lower = url.to_lowercase();
    let allowed_prefixes = [
        "https://kr.battle.net/login/",
        "https://us.battle.net/login/",
        "https://eu.battle.net/login/",
        "https://account.battlenet.com.cn/login/",
    ];
    let allowed_host = allowed_prefixes
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    let expected_query = lower.contains("externalchallenge=login") && lower.contains("app=osi");
    if allowed_host && expected_query {
        Ok(())
    } else {
        Err(AppError::FileError(
            "仅允许打开 Battle.net Token 登录页面".to_string(),
        ))
    }
}
fn browser_profile_name(account_id: &str) -> String {
    format!("D2RHub-{}", sanitize_folder_name(account_id))
}

fn browser_profile_paths(local_dir: &std::path::Path, account_id: &str) -> [std::path::PathBuf; 2] {
    let profile_name = browser_profile_name(account_id);
    [
        local_dir
            .join("Microsoft")
            .join("Edge")
            .join("User Data")
            .join(&profile_name),
        local_dir
            .join("Google")
            .join("Chrome")
            .join("User Data")
            .join(profile_name),
    ]
}

fn browser_is_edge(config: &GlobalConfig, browser_path: &str) -> bool {
    if config.browser_type.trim().is_empty() {
        browser_path.to_ascii_lowercase().contains("msedge")
    } else {
        config.browser_type.eq_ignore_ascii_case("edge")
    }
}

fn private_browser_arguments(
    config: &GlobalConfig,
    browser_path: &str,
    url: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        if browser_is_edge(config, browser_path) {
            "--inprivate".to_string()
        } else {
            "--incognito".to_string()
        },
        "--new-window".to_string(),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
    ];
    if let Some(url) = url {
        args.push(url.to_string());
    }
    args
}

fn launch_private_browser_impl(
    config: &GlobalConfig,
    browser_path: &str,
    url: Option<&str>,
) -> Result<(), AppError> {
    ensure_browser_path_allowed(config, browser_path)?;
    let args = private_browser_arguments(config, browser_path, url);
    let _ = silent_cmd(browser_path)
        .args(&args)
        .spawn()
        .map_err(|e| AppError::FileError(format!("启动无痕浏览器失败: {}", e)))?;
    Ok(())
}

fn prepare_browser_profile(
    config: &GlobalConfig,
    browser_path: &str,
    account_id: &str,
) -> Result<(std::path::PathBuf, String), AppError> {
    AccountManager::validate_account_id(account_id)?;
    let display_name = AccountManager::load_meta(&config.accounts_dir, account_id)?.display_name;
    let stable_profile_name = browser_profile_name(account_id);
    let (user_data_dir, profile_name) = if let Some(local_dir) = dirs::data_local_dir() {
        if browser_is_edge(config, browser_path) {
            (
                local_dir.join("Microsoft").join("Edge").join("User Data"),
                stable_profile_name,
            )
        } else {
            (
                local_dir.join("Google").join("Chrome").join("User Data"),
                stable_profile_name,
            )
        }
    } else {
        let account_dir = AccountManager::account_dir_checked(&config.accounts_dir, account_id)?;
        (account_dir.join("BrowserProfile"), "Default".to_string())
    };

    std::fs::create_dir_all(user_data_dir.join(&profile_name))?;
    if profile_name != "Default" {
        set_profile_name(&user_data_dir, &profile_name, &display_name)?;
    }
    Ok((user_data_dir, profile_name))
}

pub fn remove_browser_profiles_for_account(account_id: &str) -> Result<(), AppError> {
    AccountManager::validate_account_id(account_id)?;
    let Some(local_dir) = dirs::data_local_dir() else {
        return Ok(());
    };
    for profile_path in browser_profile_paths(&local_dir, account_id) {
        if profile_path.exists() {
            std::fs::remove_dir_all(profile_path)?;
        }
    }
    Ok(())
}

/// 强行修改浏览器 Preferences 中的个人资料名称，解决 Chrome 自动命名为 “您的 Chrome” 或 “用户X” 的问题
fn set_profile_name(
    user_data_dir: &std::path::Path,
    profile_name: &str,
    display_name: &str,
) -> Result<(), AppError> {
    let pref_path = user_data_dir.join(profile_name).join("Preferences");
    if let Some(parent) = pref_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut prefs = if pref_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pref_path) {
            serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
        } else {
            serde_json::Value::Object(serde_json::Map::new())
        }
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    if let Some(obj) = prefs.as_object_mut() {
        let profile_obj = obj
            .entry("profile")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let Some(p_obj) = profile_obj.as_object_mut() {
            p_obj.insert(
                "name".to_string(),
                serde_json::Value::String(display_name.to_string()),
            );
            p_obj.insert(
                "is_using_default_name".to_string(),
                serde_json::Value::Bool(false),
            );
        }
    }

    let serialized = serde_json::to_string_pretty(&prefs)?;
    std::fs::write(&pref_path, serialized)?;
    Ok(())
}

pub fn launch_browser_for_account_impl(
    config: &GlobalConfig,
    browser_path: &str,
    account_id: &str,
) -> Result<(), AppError> {
    ensure_browser_path_allowed(config, browser_path)?;
    let (user_data_dir, profile_name) = prepare_browser_profile(config, browser_path, account_id)?;

    let user_data_arg = format!("--user-data-dir={}", user_data_dir.to_string_lossy());
    let profile_dir_arg = format!("--profile-directory={}", profile_name);

    let args = vec![
        user_data_arg,
        profile_dir_arg,
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
    ];

    let _ = silent_cmd(browser_path)
        .args(&args)
        .spawn()
        .map_err(|e| AppError::FileError(format!("启动浏览器失败: {}", e)))?;

    Ok(())
}

/// 启动浏览器。Token 账号使用无痕窗口；Battle.net 账号使用独立 Profile。
#[tauri::command]
pub fn launch_browser_for_account(
    state: tauri::State<'_, SharedState>,
    browser_path: String,
    account_id: String,
) -> Result<(), AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let config = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::ConfigReadError("未配置".into()))?;

    // 在启动浏览器之前，收集现有的 Chrome/Edge 窗口句柄列表
    #[cfg(target_os = "windows")]
    let before_hwnds = crate::commands::system::collect_chrome_windows();

    let meta = AccountManager::load_meta(&config.accounts_dir, &account_id)?;
    if AuthMode::parse(meta.auth_mode.as_deref())? == AuthMode::Token {
        launch_private_browser_impl(&config, &browser_path, None)?;
    } else {
        launch_browser_for_account_impl(&config, &browser_path, &account_id)?;
    }

    // 启动后台监测线程，自动将新打开的浏览器空白窗口置顶并激活
    #[cfg(target_os = "windows")]
    crate::commands::system::bring_browser_login_to_foreground(before_hwnds);

    Ok(())
}

/// 检测指定类型的浏览器（chrome/edge）是否正在运行
#[tauri::command]
pub fn check_browser_running(browser_type: String) -> bool {
    let target = if browser_type == "edge" {
        "msedge"
    } else {
        "chrome"
    };
    let mut sys = shared_system().lock().unwrap_or_else(|e| e.into_inner());
    sys.refresh_processes(ProcessesToUpdate::All);
    for proc in sys.processes().values() {
        let name = proc.name().to_string_lossy().to_lowercase();
        if name.contains(target) {
            return true;
        }
    }
    false
}

/// 强行杀死同名浏览器的所有进程（已重定向为安全的窗口点杀，避免关闭默认浏览器）
#[tauri::command]
pub fn kill_browser_processes(_browser_type: String) {
    #[cfg(target_os = "windows")]
    {
        crate::commands::account::close_browser_login_windows();
    }
}

/// 使用账号对应的浏览器配置文件打开指定 URL
fn open_url_for_account_impl(
    config: &GlobalConfig,
    browser_path: &str,
    account_id: &str,
    url: &str,
) -> Result<(), AppError> {
    let (user_data_dir, profile_name) = prepare_browser_profile(config, browser_path, account_id)?;

    let user_data_arg = format!("--user-data-dir={}", user_data_dir.to_string_lossy());
    let profile_dir_arg = format!("--profile-directory={}", profile_name);

    let args = vec![
        user_data_arg,
        profile_dir_arg,
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        url.to_string(),
    ];

    let _ = silent_cmd(browser_path)
        .args(&args)
        .spawn()
        .map_err(|e| AppError::FileError(format!("启动浏览器失败: {}", e)))?;

    Ok(())
}

/// 打开登录 URL。Token 账号不创建或指定用户 Profile；Battle.net 账号保持隔离 Profile。
#[tauri::command]
pub fn open_url_in_browser(
    state: tauri::State<'_, SharedState>,
    browser_path: String,
    account_id: String,
    url: String,
) -> Result<(), AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let config = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::ConfigReadError("未配置".into()))?;

    #[cfg(target_os = "windows")]
    let before_hwnds = crate::commands::system::collect_chrome_windows();

    ensure_browser_path_allowed(&config, &browser_path)?;
    ensure_allowed_bnet_login_url(&url)?;

    let meta = AccountManager::load_meta(&config.accounts_dir, &account_id)?;
    if AuthMode::parse(meta.auth_mode.as_deref())? == AuthMode::Token {
        launch_private_browser_impl(&config, &config.browser_path, Some(&url))?;
    } else {
        open_url_for_account_impl(&config, &config.browser_path, &account_id, &url)?;
    }

    #[cfg(target_os = "windows")]
    crate::commands::system::bring_browser_login_to_foreground(before_hwnds);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{browser_profile_name, browser_profile_paths, private_browser_arguments};
    use crate::commands::global_config::GlobalConfig;
    use std::path::Path;

    #[test]
    fn browser_profiles_are_bound_to_stable_account_ids() {
        assert_eq!(browser_profile_name("acount1"), "D2RHub-acount1");
        assert_eq!(browser_profile_name("acount2"), "D2RHub-acount2");
        assert_ne!(
            browser_profile_name("acount1"),
            browser_profile_name("acount2")
        );
    }

    #[test]
    fn browser_profile_cleanup_targets_only_d2rhub_profiles() {
        let paths = browser_profile_paths(Path::new("C:/Users/Test/AppData/Local"), "acount1");
        assert!(paths.iter().all(|path| path.ends_with("D2RHub-acount1")));
        assert!(paths
            .iter()
            .any(|path| path.to_string_lossy().contains("Edge")));
        assert!(paths
            .iter()
            .any(|path| path.to_string_lossy().contains("Chrome")));
    }

    #[test]
    fn token_browser_arguments_use_private_mode_without_a_profile() {
        for (browser_type, browser_path, private_flag) in [
            ("edge", r"C:\Program Files\Edge\msedge.exe", "--inprivate"),
            (
                "chrome",
                r"C:\Program Files\Chrome\chrome.exe",
                "--incognito",
            ),
        ] {
            let config = GlobalConfig {
                browser_type: browser_type.to_string(),
                ..GlobalConfig::default()
            };
            let args = private_browser_arguments(
                &config,
                browser_path,
                Some("https://example.invalid/login"),
            );

            assert!(args.iter().any(|argument| argument == private_flag));
            assert!(args.iter().any(|argument| argument == "--new-window"));
            assert!(args
                .iter()
                .any(|argument| argument == "https://example.invalid/login"));
            assert!(!args
                .iter()
                .any(|argument| argument.starts_with("--user-data-dir=")));
            assert!(!args
                .iter()
                .any(|argument| argument.starts_with("--profile-directory=")));
        }
    }
}
