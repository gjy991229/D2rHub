use crate::commands::account::AccountManager;
use crate::commands::utils::{sanitize_folder_name, shared_system, silent_cmd};
use crate::domain::config::GlobalConfig;
use crate::error::AppError;
use crate::launch_context::paths_have_same_identity;
use crate::state::{AccountLifecycleLease, SharedState};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

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
    let query = lower
        .split_once('?')
        .map(|(_, query)| query.split('#').next().unwrap_or(query));
    let has_query_parameter = |expected_name: &str, expected_value: &str| {
        query.is_some_and(|query| {
            query.split('&').any(|parameter| {
                parameter
                    .split_once('=')
                    .is_some_and(|(name, value)| name == expected_name && value == expected_value)
            })
        })
    };
    let expected_query =
        has_query_parameter("externalchallenge", "login") && has_query_parameter("app", "osi");
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

fn browser_profile_location(
    config: &GlobalConfig,
    browser_path: &str,
    account_id: &str,
) -> Result<(std::path::PathBuf, String), AppError> {
    AccountManager::validate_account_id(account_id)?;
    let stable_profile_name = browser_profile_name(account_id);
    Ok(if let Some(local_dir) = dirs::data_local_dir() {
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
    })
}

/// 判断该账号的浏览器 Profile 是否正被运行中的浏览器占用。
///
/// 读取命令行需要逐个打开进程，代价明显高于读取进程名，因此先用一次廉价的名字扫描
/// 短路掉“根本没有目标浏览器”的常见情况，避免长时间占用共享 `System` 互斥锁。
fn browser_profile_is_running(
    config: &GlobalConfig,
    browser_path: &str,
    user_data_dir: &std::path::Path,
    profile_name: &str,
) -> bool {
    let expected_browser = if browser_is_edge(config, browser_path) {
        "msedge"
    } else {
        "chrome"
    };
    let expected_user_data_prefix = "--user-data-dir=";
    let expected_profile_prefix = "--profile-directory=";
    let mut system = shared_system().lock().unwrap_or_else(|e| e.into_inner());
    system.refresh_processes(ProcessesToUpdate::All);
    let browser_is_running = system.processes().values().any(|process| {
        process
            .name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains(expected_browser)
    });
    if !browser_is_running {
        return false;
    }

    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
    );
    system.processes().values().any(|process| {
        let process_name = process.name().to_string_lossy().to_ascii_lowercase();
        if !process_name.contains(expected_browser) {
            return false;
        }

        let user_data_matches = process.cmd().iter().any(|argument| {
            let argument = argument.to_string_lossy();
            argument
                .strip_prefix(expected_user_data_prefix)
                .map(|value| value.trim_matches('"'))
                .is_some_and(|value| {
                    paths_have_same_identity(std::path::Path::new(value), user_data_dir)
                })
        });
        let profile_matches = process.cmd().iter().any(|argument| {
            argument
                .to_string_lossy()
                .strip_prefix(expected_profile_prefix)
                .is_some_and(|value| value.eq_ignore_ascii_case(profile_name))
        });
        user_data_matches && profile_matches
    })
}

/// 把账号昵称同步到浏览器 Profile 的可见名称。
///
/// 显示名只是界面观感：Profile 目录始终由稳定 account_id 决定，写失败不应阻断浏览器启动。
fn sync_profile_display_name(
    config: &GlobalConfig,
    browser_path: &str,
    user_data_dir: &std::path::Path,
    profile_name: &str,
    account_id: &str,
) -> Result<(), AppError> {
    if profile_name == "Default" {
        return Ok(());
    }
    if browser_profile_is_running(config, browser_path, user_data_dir, profile_name) {
        log::info!(
            "账号 {} 的浏览器 Profile 正在运行，跳过显示名同步以免覆盖浏览器写入",
            account_id
        );
        return Ok(());
    }
    let display_name = AccountManager::load_meta(&config.accounts_dir, account_id)?.display_name;
    set_profile_name(user_data_dir, profile_name, &display_name)
}

fn prepare_browser_profile(
    config: &GlobalConfig,
    browser_path: &str,
    account_id: &str,
) -> Result<(std::path::PathBuf, String), AppError> {
    let (user_data_dir, profile_name) = browser_profile_location(config, browser_path, account_id)?;

    std::fs::create_dir_all(user_data_dir.join(&profile_name))?;
    if let Err(error) = sync_profile_display_name(
        config,
        browser_path,
        &user_data_dir,
        &profile_name,
        account_id,
    ) {
        log::warn!(
            "账号 {} 的浏览器 Profile 显示名同步失败，继续启动浏览器: {}",
            account_id,
            error
        );
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
    let temp_path = pref_path.with_file_name(format!(
        "Preferences.d2rhub.{}.tmp",
        std::process::id()
    ));
    std::fs::write(&temp_path, serialized)?;
    if let Err(error) = std::fs::rename(&temp_path, &pref_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(AppError::IoError(format!(
            "替换浏览器 Profile Preferences 失败: {error}"
        )));
    }
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

/// 启动浏览器并打开账号对应的独立 Profile。
#[tauri::command]
pub fn launch_browser_for_account(
    state: tauri::State<'_, SharedState>,
    browser_path: String,
    account_id: String,
) -> Result<(), AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("未配置".into()))?;

    // 在启动浏览器之前，收集现有的 Chrome/Edge 窗口句柄列表
    #[cfg(target_os = "windows")]
    let before_hwnds = crate::infrastructure::system::collect_chrome_windows();

    AccountManager::load_meta(&config.accounts_dir, &account_id)?;
    launch_browser_for_account_impl(&config, &browser_path, &account_id)?;

    // 启动后台监测线程，自动将新打开的浏览器空白窗口置顶并激活
    #[cfg(target_os = "windows")]
    crate::infrastructure::system::bring_browser_login_to_foreground(before_hwnds);

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

/// 打开登录 URL，并复用账号对应的隔离 Profile。
#[tauri::command]
pub fn open_url_in_browser(
    state: tauri::State<'_, SharedState>,
    browser_path: String,
    account_id: String,
    url: String,
) -> Result<(), AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("未配置".into()))?;

    #[cfg(target_os = "windows")]
    let before_hwnds = crate::infrastructure::system::collect_chrome_windows();

    ensure_browser_path_allowed(&config, &browser_path)?;
    ensure_allowed_bnet_login_url(&url)?;

    AccountManager::load_meta(&config.accounts_dir, &account_id)?;
    open_url_for_account_impl(&config, &config.browser_path, &account_id, &url)?;

    #[cfg(target_os = "windows")]
    crate::infrastructure::system::bring_browser_login_to_foreground(before_hwnds);

    Ok(())
}

/// 仅同步浏览器 Profile 的可见名称，不启动浏览器、也不创建任何浏览器数据。
///
/// 账号改名不应该在用户的 Chrome/Edge `User Data` 下凭空生成 Profile 目录，
/// 因此这里只在 Profile 已经存在（说明该账号确实用过浏览器）时才写入显示名。
pub fn sync_browser_profile_name(config: &GlobalConfig, account_id: &str) -> Result<(), AppError> {
    // 缺少浏览器配置时无从判断 Profile 归属，直接跳过。
    if config.browser_path.trim().is_empty() || config.browser_type.trim().is_empty() {
        return Ok(());
    }
    let (user_data_dir, profile_name) =
        browser_profile_location(config, &config.browser_path, account_id)?;
    if profile_name == "Default" || !user_data_dir.join(&profile_name).exists() {
        return Ok(());
    }
    sync_profile_display_name(
        config,
        &config.browser_path,
        &user_data_dir,
        &profile_name,
        account_id,
    )
}

/// 使用账号对应的独立浏览器 Profile 打开 Token 登录页。
///
/// Token 向导会先创建一个待完成的账号，因此这里和 Battle.net 登录流程一样，
/// 可以依赖稳定的账号 ID 选择 Profile。URL 仍受 Battle.net 登录页白名单约束。
#[tauri::command]
pub fn open_token_login_url(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    url: String,
) -> Result<(), AppError> {
    let _account_lease = AccountLifecycleLease::try_acquire(state.inner(), &account_id)?;
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("未配置".into()))?;

    if config.browser_path.trim().is_empty() {
        return Err(AppError::ConfigReadError(
            "尚未配置用于获取 Token 的浏览器".to_string(),
        ));
    }
    ensure_allowed_bnet_login_url(&url)?;

    #[cfg(target_os = "windows")]
    let before_hwnds = crate::infrastructure::system::collect_chrome_windows();

    AccountManager::load_meta(&config.accounts_dir, &account_id)?;
    open_url_for_account_impl(&config, &config.browser_path, &account_id, &url)?;

    #[cfg(target_os = "windows")]
    crate::infrastructure::system::bring_browser_login_to_foreground(before_hwnds);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        browser_profile_location, browser_profile_name, browser_profile_paths,
        ensure_allowed_bnet_login_url,
    };
    use crate::domain::config::GlobalConfig;
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
    fn browser_profile_location_rejects_path_like_account_ids() {
        let config = GlobalConfig {
            browser_type: "chrome".to_string(),
            ..GlobalConfig::default()
        };
        assert!(browser_profile_location(&config, r"C:\Chrome\chrome.exe", "../escape").is_err());
        assert!(browser_profile_location(&config, r"C:\Chrome\chrome.exe", "acount1").is_ok());
    }

    #[test]
    fn token_login_urls_require_exact_external_challenge_and_osi_app_parameters() {
        for url in [
            "https://account.battlenet.com.cn/login/zh/?externalChallenge=login&app=OSI",
            "https://kr.battle.net/login/en/?app=OSI&externalChallenge=login",
            "https://us.battle.net/login/en/?externalChallenge=login&app=OSI",
            "https://eu.battle.net/login/en/?externalChallenge=login&app=OSI",
        ] {
            ensure_allowed_bnet_login_url(url).unwrap();
        }

        for url in [
            "https://account.battlenet.com.cn/login/zh/?externalChallenge=login&app=osic",
            "https://us.battle.net/login/en/?externalChallenge=login&xapp=osi",
            "https://eu.battle.net/login/en/?externalChallenge=login2&app=osi",
        ] {
            assert!(ensure_allowed_bnet_login_url(url).is_err());
        }
    }
}
