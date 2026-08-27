use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::commands::account::{recover_account_transactions, AccountManager, AccountMeta};
use crate::error::AppError;
use crate::state::SharedState;

const CURRENT_CONFIG_VERSION: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyRegionPathMigration {
    NotNeeded,
    Migrated,
    Ambiguous,
}

/// 全局配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub version: u32,
    #[serde(default)]
    pub cn_battle_net_path: String,
    /// 国服游戏安装目录。
    #[serde(default)]
    pub cn_game_path: String,
    /// 国服存档目录（通常以 Diablo II Resurrected (CN) 结尾）。
    #[serde(default)]
    pub cn_saved_games_path: String,
    /// 国际服游戏安装目录（亚服、美服、欧服共用）。
    #[serde(default)]
    pub global_game_path: String,
    /// 国际服存档目录（亚服、美服、欧服共用）。
    #[serde(default)]
    pub global_saved_games_path: String,
    pub program_data_agent_path: String,
    pub app_data_roaming_bnet_path: String,
    pub accounts_dir: String,
    pub first_run_complete: bool,
    /// 浏览器可执行文件路径（Edge 或 Chrome）
    #[serde(default)]
    pub browser_path: String,
    /// 浏览器类型: "edge" | "chrome" | "" (未配置)
    #[serde(default)]
    pub browser_type: String,
    #[serde(default = "default_enable_bongo_cat")]
    pub enable_bongo_cat: bool,
    #[serde(default = "default_bongo_cat_chatterbox")]
    pub bongo_cat_chatterbox: bool,
    #[serde(default = "default_bongo_cat_scale")]
    pub bongo_cat_scale: f32,
    #[serde(default = "default_bongo_cat_skin")]
    pub bongo_cat_skin: String,
    #[serde(default = "default_bongo_cat_unlocked_skins")]
    pub bongo_cat_unlocked_skins: Vec<String>,
    /// 旧版合并开关，仅作为向后兼容字段保存；运行时读取下方两个独立开关。
    #[serde(default = "default_enable_overlay")]
    pub enable_overlay: bool,
    /// 邪恶区域播报悬浮窗。旧配置从 enable_overlay 迁移。
    #[serde(default = "default_enable_overlay")]
    pub enable_tz_overlay: bool,
    /// OCR 场景计时与掉落统计悬浮窗。旧配置从 enable_overlay 迁移。
    #[serde(default = "default_enable_overlay")]
    pub enable_stats_overlay: bool,
    /// 主题选择: "onyx" | "light"
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 悬浮窗主题选择: "onyx" | "light"
    #[serde(default = "default_theme")]
    pub theme_overlay: String,
    /// 是否在登录后/流程结束后自动关闭浏览器，以及在启动前做清理
    #[serde(default = "default_auto_close_browser")]
    pub auto_close_browser: bool,
    /// 是否在每天启动时自动检查更新
    #[serde(default = "default_enable_auto_update")]
    pub enable_auto_update: bool,
    /// 是否首次启动（自动弹出帮助文档）
    #[serde(default = "default_first_launch")]
    pub first_launch: bool,
    /// OCR：是否启用自动文字识别
    #[serde(default)]
    pub ocr_enabled: bool,
    /// OCR：被监控的账号 ID（对应 account.json 中的 id）
    #[serde(default)]
    pub ocr_target_account: String,
    pub ocr_ch_b_profiles_json: String,
    /// OCR：是否开启调试输出（保存截图到 config/test）
    #[serde(default)]
    pub ocr_debug_output: bool,

    /// OCR：轮询间隔 (ms)，默认 500 (2Hz)，性能不足可设为 1000 (1Hz)
    #[serde(default = "default_ocr_poll_interval")]
    pub ocr_poll_interval_ms: u64,
    /// 快捷键绑定 JSON: {"1": "Ctrl+1", "2": "Ctrl+2", ...} ，key 为账号位置序号（1-based）
    /// 空字符串表示从未配置过（首次启动时自动迁移为默认值）
    #[serde(default)]
    pub shortcut_bindings_json: String,
    /// 悬浮窗透明度 (10-100, 默认 95)
    #[serde(default = "default_opacity")]
    pub overlay_opacity: u8,
    /// 主界面透明度 (10-100, 默认 95)
    #[serde(default = "default_opacity")]
    pub main_opacity: u8,
    /// 界面字体缩放 ("small" / "default" / "large")
    #[serde(default = "default_font_scale")]
    pub font_scale: String,
    /// 是否为每个游戏账号窗口设置独立的任务栏 AppUserModelID。
    #[serde(default)]
    pub separate_game_taskbar_icons: bool,
    /// 应用界面语言 ("zh-CN" / "en-US")
    #[serde(default = "default_app_language")]
    pub app_language: String,
    /// Agent 多开模式: 1=延时杀, 2=进程数阈值杀
    #[serde(default = "default_agent_mode")]
    pub agent_mode: u8,
    /// 模式1: Agent 存活延迟 (秒), 0-30, 默认 1.0
    #[serde(default = "default_agent_delay_secs")]
    pub agent_delay_secs: f64,
    /// 模式2: bnet_count 阈值, 4/5/7, 默认 5
    #[serde(default = "default_agent_threshold")]
    pub agent_threshold: u32,
}

fn default_font_scale() -> String {
    "default".to_string()
}
fn default_app_language() -> String {
    "zh-CN".to_string()
}
fn default_opacity() -> u8 {
    95
}

fn default_ocr_poll_interval() -> u64 {
    500
}

fn default_agent_mode() -> u8 {
    1
}
fn default_agent_delay_secs() -> f64 {
    1.0
}
fn default_agent_threshold() -> u32 {
    5
}

fn default_theme() -> String {
    "light".to_string()
}

fn default_enable_overlay() -> bool {
    true
}

fn default_auto_close_browser() -> bool {
    true
}

fn default_enable_auto_update() -> bool {
    true
}

fn default_first_launch() -> bool {
    true
}
fn default_enable_bongo_cat() -> bool {
    true
}
fn default_bongo_cat_chatterbox() -> bool {
    true
}
fn default_bongo_cat_scale() -> f32 {
    1.0
}
fn default_bongo_cat_skin() -> String {
    "original".to_string()
}
fn default_bongo_cat_unlocked_skins() -> Vec<String> {
    vec!["original".to_string()]
}

fn app_accounts_dir(app_data_dir: &str) -> PathBuf {
    Path::new(app_data_dir).join("accounts")
}

fn saved_games_settings_exists(path: &Path) -> bool {
    path.join("Settings.json").is_file()
}

fn validate_installation_paths(config: &GlobalConfig) -> Result<(), AppError> {
    // 核心启动只依赖 D2R.exe。存档目录与 Settings.json 属于可选的画质覆盖能力，
    // Battle.net、另一客户端版本及重复安装档案都在真正使用对应账号时精确校验，
    // 不能因为一项备用配置失效而阻断已经可用的最小启动路径。
    let game_paths = [&config.cn_game_path, &config.global_game_path];
    if game_paths.iter().any(|game_path| {
        let directory = Path::new(game_path.trim());
        directory.is_dir() && directory.join("D2R.exe").is_file()
    }) {
        return Ok(());
    }

    Err(AppError::ConfigWriteError(
        "请至少配置一组有效的国服或国际服游戏安装目录（目录中必须存在 D2R.exe）".to_string(),
    ))
}

/// 全局保存只验证核心游戏安装路径；浏览器、Battle.net 与存档目录在实际使用
/// 对应功能时校验，避免可选能力阻断账号创建或启动所需的最小路径。
fn should_validate_installation_paths(
    previous: Option<&GlobalConfig>,
    next: &GlobalConfig,
) -> bool {
    if !next.first_run_complete {
        return false;
    }

    let Some(previous) = previous else {
        return true;
    };
    if !previous.first_run_complete {
        return true;
    }

    previous.cn_battle_net_path != next.cn_battle_net_path
        || previous.cn_game_path != next.cn_game_path
        || previous.cn_saved_games_path != next.cn_saved_games_path
        || previous.global_game_path != next.global_game_path
        || previous.global_saved_games_path != next.global_saved_games_path
}

#[cfg(test)]
mod validation_tests {
    use super::{
        saved_games_settings_exists, should_validate_installation_paths,
        validate_installation_paths, GlobalConfig, CURRENT_CONFIG_VERSION,
    };
    use crate::commands::account::{AccountManager, AccountMeta};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let unique = format!(
            "d2rhub_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn invalid_saved_games_path_does_not_block_core_configuration() {
        let root = temp_dir("config_without_saved_games");
        let battle_net = root.join("Battle.net.exe");
        let game_dir = root.join("game");
        std::fs::write(&battle_net, b"").unwrap();
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("D2R.exe"), b"").unwrap();

        let config = GlobalConfig {
            cn_battle_net_path: battle_net.to_string_lossy().to_string(),
            cn_game_path: game_dir.to_string_lossy().to_string(),
            cn_saved_games_path: root.join("missing").to_string_lossy().to_string(),
            ..GlobalConfig::default()
        };

        assert!(validate_installation_paths(&config).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn game_path_without_saved_games_is_a_valid_configuration() {
        let root = temp_dir("token_only_config");
        let game_dir = root.join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("D2R.exe"), b"").unwrap();

        let config = GlobalConfig {
            global_game_path: game_dir.to_string_lossy().to_string(),
            ..GlobalConfig::default()
        };

        assert!(validate_installation_paths(&config).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn at_least_one_existing_d2r_executable_is_required() {
        let root = temp_dir("partial_token_config");
        let game_dir = root.join("game");
        std::fs::create_dir_all(&game_dir).unwrap();

        let config = GlobalConfig {
            cn_game_path: game_dir.to_string_lossy().to_string(),
            ..GlobalConfig::default()
        };

        assert!(validate_installation_paths(&config).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_cn_paths_do_not_block_a_complete_global_edition() {
        let root = temp_dir("complete_global_partial_cn");
        let global_game = root.join("global-game");
        std::fs::create_dir_all(&global_game).unwrap();
        std::fs::write(global_game.join("D2R.exe"), b"").unwrap();

        let config = GlobalConfig {
            cn_battle_net_path: root
                .join("missing-battle-net.exe")
                .to_string_lossy()
                .to_string(),
            cn_game_path: root.join("missing-cn-game").to_string_lossy().to_string(),
            global_game_path: global_game.to_string_lossy().to_string(),
            global_saved_games_path: root.join("global-saves").to_string_lossy().to_string(),
            ..GlobalConfig::default()
        };

        assert!(validate_installation_paths(&config).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_global_paths_do_not_block_a_complete_cn_edition() {
        let root = temp_dir("complete_cn_partial_global");
        let cn_game = root.join("cn-game");
        std::fs::create_dir_all(&cn_game).unwrap();
        std::fs::write(cn_game.join("D2R.exe"), b"").unwrap();

        let config = GlobalConfig {
            cn_game_path: cn_game.to_string_lossy().to_string(),
            cn_saved_games_path: root.join("cn-saves").to_string_lossy().to_string(),
            global_game_path: root
                .join("missing-global-game")
                .to_string_lossy()
                .to_string(),
            ..GlobalConfig::default()
        };

        assert!(validate_installation_paths(&config).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn appearance_only_changes_do_not_revalidate_installation_paths() {
        let previous = GlobalConfig {
            first_run_complete: true,
            cn_game_path: r"Z:\missing\D2R-CN".to_string(),
            cn_saved_games_path: r"Z:\missing\Saved Games\D2R-CN".to_string(),
            ..GlobalConfig::default()
        };
        let next = GlobalConfig {
            theme: "onyx".to_string(),
            overlay_opacity: 82,
            ..previous.clone()
        };

        assert!(!should_validate_installation_paths(Some(&previous), &next));
    }

    #[test]
    fn installation_path_changes_still_require_validation() {
        let previous = GlobalConfig {
            first_run_complete: true,
            global_game_path: r"C:\Games\D2R".to_string(),
            global_saved_games_path: r"C:\Saved Games\D2R".to_string(),
            ..GlobalConfig::default()
        };
        let next = GlobalConfig {
            global_game_path: r"D:\Games\D2R".to_string(),
            ..previous.clone()
        };

        assert!(should_validate_installation_paths(Some(&previous), &next));
        assert!(should_validate_installation_paths(None, &next));
    }

    #[test]
    fn optional_browser_changes_do_not_revalidate_offline_game_installations() {
        let previous = GlobalConfig {
            first_run_complete: true,
            cn_game_path: r"Z:\offline\D2R-CN".to_string(),
            cn_saved_games_path: r"Z:\offline\Saved Games\D2R-CN".to_string(),
            browser_path: r"C:\Browser\old.exe".to_string(),
            ..GlobalConfig::default()
        };
        let next = GlobalConfig {
            browser_path: r"C:\Browser\new.exe".to_string(),
            ..previous.clone()
        };

        assert!(!should_validate_installation_paths(Some(&previous), &next));
    }

    #[test]
    fn settings_availability_requires_the_actual_file() {
        let saved_games = temp_dir("settings_availability");
        assert!(!saved_games_settings_exists(&saved_games));

        std::fs::write(saved_games.join("Settings.json"), b"{}").unwrap();

        assert!(saved_games_settings_exists(&saved_games));
        let _ = std::fs::remove_dir_all(saved_games);
    }

    #[test]
    fn enabled_ocr_requires_a_selected_account() {
        let config = GlobalConfig {
            ocr_enabled: true,
            ..GlobalConfig::default()
        };

        assert!(config.resolve_ocr_target_account().is_err());
    }

    #[test]
    fn disabled_ocr_does_not_require_a_target_account() {
        let config = GlobalConfig::default();

        assert!(config.resolve_ocr_target_account().unwrap().is_none());
    }

    #[test]
    fn enabled_ocr_requires_an_initialized_account() {
        let accounts_dir = temp_dir("ocr_uninitialized_account");
        let account = AccountMeta::new("acount1");
        std::fs::create_dir_all(accounts_dir.join(&account.id)).unwrap();
        AccountManager::save_meta(accounts_dir.to_str().unwrap(), &account).unwrap();

        let config = GlobalConfig {
            accounts_dir: accounts_dir.to_string_lossy().to_string(),
            ocr_enabled: true,
            ocr_target_account: account.id,
            ..GlobalConfig::default()
        };

        assert!(config.resolve_ocr_target_account().is_err());
        let _ = std::fs::remove_dir_all(accounts_dir);
    }

    #[test]
    fn enabled_ocr_accepts_an_initialized_account() {
        let accounts_dir = temp_dir("ocr_initialized_account");
        let mut account = AccountMeta::new("acount1");
        account.initialized = true;
        std::fs::create_dir_all(accounts_dir.join(&account.id)).unwrap();
        AccountManager::save_meta(accounts_dir.to_str().unwrap(), &account).unwrap();

        let config = GlobalConfig {
            accounts_dir: accounts_dir.to_string_lossy().to_string(),
            ocr_enabled: true,
            ocr_target_account: account.id.clone(),
            ..GlobalConfig::default()
        };

        let resolved = config.resolve_ocr_target_account().unwrap().unwrap();
        assert_eq!(resolved.id, account.id);
        let _ = std::fs::remove_dir_all(accounts_dir);
    }

    #[test]
    fn invalid_legacy_ocr_configuration_is_disabled() {
        let mut config = GlobalConfig {
            ocr_enabled: true,
            ..GlobalConfig::default()
        };

        assert!(config.normalize_ocr_configuration());
        assert!(!config.ocr_enabled);
    }

    #[test]
    fn legacy_overlay_switch_is_split_without_changing_user_intent() {
        for enabled in [false, true] {
            let root = temp_dir(if enabled {
                "legacy_overlay_enabled"
            } else {
                "legacy_overlay_disabled"
            });
            let mut legacy = serde_json::to_value(GlobalConfig::default()).unwrap();
            let object = legacy.as_object_mut().unwrap();
            object.insert("version".to_string(), serde_json::json!(5));
            object.insert("enable_overlay".to_string(), serde_json::json!(enabled));
            object.remove("enable_tz_overlay");
            object.remove("enable_stats_overlay");
            std::fs::write(
                root.join("global_config.json"),
                serde_json::to_vec_pretty(&legacy).unwrap(),
            )
            .unwrap();

            let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();

            assert_eq!(config.version, CURRENT_CONFIG_VERSION);
            assert_eq!(config.enable_tz_overlay, enabled);
            assert_eq!(config.enable_stats_overlay, enabled);
            assert_eq!(config.enable_overlay, enabled);

            let saved: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("global_config.json")).unwrap())
                    .unwrap();
            assert_eq!(saved["enable_tz_overlay"], enabled);
            assert_eq!(saved["enable_stats_overlay"], enabled);
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn legacy_paths_are_migrated_to_the_detected_edition() {
        let root = temp_dir("legacy_region_path_migration");
        let mut legacy = serde_json::to_value(GlobalConfig::default()).unwrap();
        let object = legacy.as_object_mut().unwrap();
        object.remove("cn_game_path");
        object.remove("cn_saved_games_path");
        object.remove("cn_battle_net_path");
        object.remove("global_game_path");
        object.remove("global_saved_games_path");
        object.insert("version".to_string(), serde_json::json!(1));
        object.insert("game_path".to_string(), serde_json::json!(r"D:\Games\D2R"));
        object.insert(
            "battle_net_path".to_string(),
            serde_json::json!(r"D:\Battle.net\Battle.net.exe"),
        );
        object.insert(
            "saved_games_path".to_string(),
            serde_json::json!(r"C:\Users\Tester\Saved Games\Diablo II Resurrected"),
        );
        std::fs::write(
            root.join("global_config.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert!(config.cn_game_path.is_empty());
        assert!(config.cn_battle_net_path.is_empty());
        assert_eq!(config.global_game_path, r"D:\Games\D2R");
        assert_eq!(
            config.global_saved_games_path,
            r"C:\Users\Tester\Saved Games\Diablo II Resurrected"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_cn_paths_keep_the_single_battle_net_executable() {
        let root = temp_dir("legacy_cn_battle_net_path");
        let config_path = root.join("global_config.json");
        let mut legacy = serde_json::to_value(GlobalConfig::default()).unwrap();
        let object = legacy.as_object_mut().unwrap();
        object.remove("cn_game_path");
        object.remove("cn_saved_games_path");
        object.remove("cn_battle_net_path");
        object.remove("global_game_path");
        object.remove("global_saved_games_path");
        object.insert("version".to_string(), serde_json::json!(1));
        object.insert(
            "game_path".to_string(),
            serde_json::json!(r"D:\Games\D2R-CN"),
        );
        object.insert(
            "battle_net_path".to_string(),
            serde_json::json!(r"C:\Battle.net\Battle.net.exe"),
        );
        object.insert(
            "saved_games_path".to_string(),
            serde_json::json!(r"C:\Users\Tester\Saved Games\Diablo II Resurrected (CN)"),
        );
        std::fs::write(&config_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();

        assert_eq!(config.cn_battle_net_path, r"C:\Battle.net\Battle.net.exe");
        assert_eq!(config.cn_game_path, r"D:\Games\D2R-CN");
        assert!(config.global_game_path.is_empty());
        assert!(persisted.get("battle_net_path").is_none());
        assert!(persisted.get("global_battle_net_path").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_custom_legacy_paths_are_preserved_for_user_confirmation() {
        let root = temp_dir("ambiguous_legacy_region_path");
        let config_path = root.join("global_config.json");
        let mut legacy = serde_json::to_value(GlobalConfig {
            first_run_complete: true,
            ..GlobalConfig::default()
        })
        .unwrap();
        let object = legacy.as_object_mut().unwrap();
        for key in [
            "cn_game_path",
            "cn_saved_games_path",
            "cn_battle_net_path",
            "global_game_path",
            "global_saved_games_path",
        ] {
            object.remove(key);
        }
        object.insert("version".to_string(), serde_json::json!(1));
        object.insert("game_path".to_string(), serde_json::json!(r"D:\Games\D2R"));
        object.insert(
            "battle_net_path".to_string(),
            serde_json::json!(r"D:\Battle.net\Battle.net.exe"),
        );
        object.insert(
            "saved_games_path".to_string(),
            serde_json::json!(r"D:\Saves\D2R"),
        );
        std::fs::write(&config_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert!(!config.first_run_complete);
        assert!(config.cn_game_path.is_empty());
        assert!(config.global_game_path.is_empty());
        let preserved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(preserved["version"], 1);
        assert_eq!(preserved["saved_games_path"], r"D:\Saves\D2R");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_recovers_account_transaction_before_normalizing_ocr_target() {
        let root = temp_dir("ocr_after_account_recovery");
        let accounts = root.join("accounts");
        let backup = accounts.join("acount1.bak");
        let staged = accounts.join("acount1.tmp");
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        let mut account = AccountMeta::new("acount1");
        account.initialized = true;
        std::fs::write(
            backup.join("account.json"),
            serde_json::to_vec_pretty(&account).unwrap(),
        )
        .unwrap();
        let config = GlobalConfig {
            accounts_dir: accounts.to_string_lossy().to_string(),
            first_run_complete: true,
            ocr_enabled: true,
            ocr_target_account: account.id,
            ..GlobalConfig::default()
        };
        config.save(root.to_str().unwrap()).unwrap();

        let loaded = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert!(loaded.ocr_enabled);
        assert!(accounts.join("acount1").is_dir());
        assert!(!backup.exists());
        assert!(!staged.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_legacy_battle_net_path_is_not_copied_to_two_editions() {
        let mut legacy = serde_json::to_value(GlobalConfig::default()).unwrap();
        let object = legacy.as_object_mut().unwrap();
        object.remove("cn_battle_net_path");
        object.insert(
            "battle_net_path".to_string(),
            serde_json::json!(r"C:\Battle.net\Battle.net.exe"),
        );
        object.insert(
            "cn_game_path".to_string(),
            serde_json::json!(r"C:\Games\D2R-CN"),
        );
        object.insert(
            "cn_saved_games_path".to_string(),
            serde_json::json!(r"C:\Saves\D2R-CN"),
        );
        object.insert(
            "global_game_path".to_string(),
            serde_json::json!(r"D:\Games\D2R-Global"),
        );
        object.insert(
            "global_saved_games_path".to_string(),
            serde_json::json!(r"D:\Saves\D2R-Global"),
        );

        assert!(GlobalConfig::migrate_legacy_battle_net_paths(&mut legacy));
        let config: GlobalConfig = serde_json::from_value(legacy).unwrap();

        assert!(config.cn_battle_net_path.is_empty());
    }

    #[test]
    fn version_three_global_battle_net_path_is_removed_on_load() {
        let root = temp_dir("remove_global_battle_net_path");
        let config_path = root.join("global_config.json");
        let mut legacy = serde_json::to_value(GlobalConfig::default()).unwrap();
        let object = legacy.as_object_mut().unwrap();
        object.insert("version".to_string(), serde_json::json!(3));
        object.insert(
            "cn_battle_net_path".to_string(),
            serde_json::json!(r"C:\Battle.net-CN\Battle.net.exe"),
        );
        object.insert(
            "global_battle_net_path".to_string(),
            serde_json::json!(r"D:\Battle.net\Battle.net.exe"),
        );
        std::fs::write(&config_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();

        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert_eq!(
            config.cn_battle_net_path,
            r"C:\Battle.net-CN\Battle.net.exe"
        );
        assert!(persisted.get("global_battle_net_path").is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}

/// 窗口几何信息（位置+尺寸持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION,
            cn_battle_net_path: String::new(),
            cn_game_path: String::new(),
            cn_saved_games_path: String::new(),
            global_game_path: String::new(),
            global_saved_games_path: String::new(),
            program_data_agent_path: String::new(),
            app_data_roaming_bnet_path: String::new(),
            accounts_dir: String::new(),
            first_run_complete: false,
            browser_path: String::new(),
            browser_type: String::new(),
            enable_bongo_cat: true,
            bongo_cat_chatterbox: true,
            bongo_cat_scale: 1.0,
            bongo_cat_skin: "original".to_string(),
            bongo_cat_unlocked_skins: vec!["original".to_string()],
            enable_overlay: true,
            enable_tz_overlay: true,
            enable_stats_overlay: true,
            theme: "light".to_string(),
            theme_overlay: "light".to_string(),
            auto_close_browser: true,
            enable_auto_update: true,
            first_launch: true,
            ocr_enabled: false,
            ocr_target_account: String::new(),
            ocr_ch_b_profiles_json: String::new(),
            ocr_debug_output: false,

            ocr_poll_interval_ms: 500,
            shortcut_bindings_json: r#"{"1":"Ctrl+1","2":"Ctrl+2","3":"Ctrl+3"}"#.to_string(),
            overlay_opacity: 95,
            main_opacity: 95,
            font_scale: "default".to_string(),
            separate_game_taskbar_icons: false,
            app_language: "zh-CN".to_string(),
            agent_mode: 1,
            agent_delay_secs: 1.0,
            agent_threshold: 5,
        }
    }
}

impl GlobalConfig {
    /// 解析并验证当前 OCR 目标。OCR 关闭时不要求配置目标账号。
    pub(crate) fn resolve_ocr_target_account(&self) -> Result<Option<AccountMeta>, AppError> {
        if !self.ocr_enabled {
            return Ok(None);
        }

        let account_id = self.ocr_target_account.trim();
        if account_id.is_empty() {
            return Err(AppError::ConfigWriteError(
                "启用 OCR 前请先选择目标账号".to_string(),
            ));
        }

        let account = AccountManager::load_meta(&self.accounts_dir, account_id)
            .map_err(|_| AppError::ConfigWriteError(format!("OCR 目标账号不存在: {account_id}")))?;
        if !account.initialized {
            return Err(AppError::ConfigWriteError(format!(
                "OCR 目标账号尚未初始化: {account_id}"
            )));
        }

        Ok(Some(account))
    }

    /// 兼容旧配置：无效目标不能保持 OCR 启用状态。
    fn normalize_ocr_configuration(&mut self) -> bool {
        if !self.ocr_enabled || self.resolve_ocr_target_account().is_ok() {
            return false;
        }

        self.ocr_enabled = false;
        true
    }

    fn config_path(app_data_dir: &str) -> PathBuf {
        Path::new(app_data_dir).join("global_config.json")
    }

    fn geometry_path(app_data_dir: &str) -> PathBuf {
        Path::new(app_data_dir).join("window_geometry.json")
    }

    /// 从磁盘加载配置
    pub fn load(app_data_dir: &str) -> Result<Self, AppError> {
        let path = Self::config_path(app_data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content =
            std::fs::read_to_string(&path).map_err(|e| AppError::ConfigReadError(e.to_string()))?;
        let mut value: serde_json::Value = serde_json::from_str(&content)?;
        let legacy_overlay_enabled = value
            .get("enable_overlay")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_else(default_enable_overlay);
        let mut overlay_split_migrated = false;
        if let Some(object) = value.as_object_mut() {
            if !object.contains_key("enable_tz_overlay") {
                object.insert(
                    "enable_tz_overlay".to_string(),
                    serde_json::Value::Bool(legacy_overlay_enabled),
                );
                overlay_split_migrated = true;
            }
            if !object.contains_key("enable_stats_overlay") {
                object.insert(
                    "enable_stats_overlay".to_string(),
                    serde_json::Value::Bool(legacy_overlay_enabled),
                );
                overlay_split_migrated = true;
            }
        }
        let region_path_migration = Self::migrate_legacy_region_paths(&mut value);
        let preserve_ambiguous_legacy_paths =
            region_path_migration == LegacyRegionPathMigration::Ambiguous;
        let mut migrated =
            overlay_split_migrated || region_path_migration == LegacyRegionPathMigration::Migrated;
        if !preserve_ambiguous_legacy_paths {
            migrated |= Self::migrate_legacy_battle_net_paths(&mut value);
        }
        let mut config: GlobalConfig = serde_json::from_value(value)?;

        if config.version != CURRENT_CONFIG_VERSION {
            config.version = CURRENT_CONFIG_VERSION;
            migrated = true;
        }
        let combined_overlay_enabled = config.enable_tz_overlay || config.enable_stats_overlay;
        if config.enable_overlay != combined_overlay_enabled {
            config.enable_overlay = combined_overlay_enabled;
            migrated = true;
        }

        let accounts_dir = app_accounts_dir(app_data_dir).to_string_lossy().to_string();
        if config.accounts_dir != accounts_dir {
            config.accounts_dir = accounts_dir;
            migrated = true;
        }

        // OCR 目标依赖账号目录。必须先回滚中断的账号目录交换，再判断目标是否有效。
        recover_account_transactions(&config.accounts_dir);

        // 迁移：从未配置过快捷键的旧用户，自动写入默认值
        if config.shortcut_bindings_json.is_empty() {
            config.shortcut_bindings_json =
                r#"{"1":"Ctrl+1","2":"Ctrl+2","3":"Ctrl+3"}"#.to_string();
            migrated = true;
        }
        // 迁移：去除旧版本可能存在的 Win/Meta/Cmd 修饰键（v0.6.6 起不再支持）
        migrated |= Self::strip_win_modifiers(&mut config.shortcut_bindings_json);

        if config.normalize_ocr_configuration() {
            log::warn!("检测到无效的旧版 OCR 目标配置，已自动关闭 OCR");
            migrated = true;
        }

        if preserve_ambiguous_legacy_paths {
            config.first_run_complete = false;
            log::warn!("旧版游戏与存档路径无法无歧义判断国服或国际服，保留原始配置并要求重新确认");
        } else if migrated {
            config.save(app_data_dir)?;
        }
        Ok(config)
    }

    fn migrate_legacy_region_paths(value: &mut serde_json::Value) -> LegacyRegionPathMigration {
        let Some(object) = value.as_object_mut() else {
            return LegacyRegionPathMigration::NotNeeded;
        };
        if object.contains_key("cn_game_path")
            || object.contains_key("global_game_path")
            || (!object.contains_key("game_path") && !object.contains_key("saved_games_path"))
        {
            return LegacyRegionPathMigration::NotNeeded;
        }

        let game_path = object
            .get("game_path")
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default();
        let saved_games_path = object
            .get("saved_games_path")
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default();
        let Some(edition) = Self::infer_legacy_saved_games_edition(&saved_games_path) else {
            return LegacyRegionPathMigration::Ambiguous;
        };
        object.remove("game_path");
        object.remove("saved_games_path");
        let is_cn = edition == crate::launch_context::ClientEdition::Cn;
        let (game_key, saves_key) = if is_cn {
            ("cn_game_path", "cn_saved_games_path")
        } else {
            ("global_game_path", "global_saved_games_path")
        };
        object.insert(game_key.to_string(), serde_json::Value::String(game_path));
        object.insert(
            saves_key.to_string(),
            serde_json::Value::String(saved_games_path),
        );
        object
            .entry(
                if is_cn {
                    "global_game_path"
                } else {
                    "cn_game_path"
                }
                .to_string(),
            )
            .or_insert_with(|| serde_json::Value::String(String::new()));
        object
            .entry(
                if is_cn {
                    "global_saved_games_path"
                } else {
                    "cn_saved_games_path"
                }
                .to_string(),
            )
            .or_insert_with(|| serde_json::Value::String(String::new()));
        LegacyRegionPathMigration::Migrated
    }

    fn infer_legacy_saved_games_edition(
        saved_games_path: &str,
    ) -> Option<crate::launch_context::ClientEdition> {
        let directory_name = saved_games_path
            .trim_end_matches(['\\', '/'])
            .rsplit(['\\', '/'])
            .next()?;
        if directory_name.eq_ignore_ascii_case("Diablo II Resurrected (CN)") {
            Some(crate::launch_context::ClientEdition::Cn)
        } else if directory_name.eq_ignore_ascii_case("Diablo II Resurrected") {
            Some(crate::launch_context::ClientEdition::Global)
        } else {
            None
        }
    }

    fn migrate_legacy_battle_net_paths(value: &mut serde_json::Value) -> bool {
        let Some(object) = value.as_object_mut() else {
            return false;
        };
        let mut changed = object.remove("global_battle_net_path").is_some();
        if object.contains_key("cn_battle_net_path") {
            changed |= object.remove("battle_net_path").is_some();
            return changed;
        }
        if !object.contains_key("battle_net_path") {
            return changed;
        }

        let battle_net_path = object
            .remove("battle_net_path")
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default();
        let cn_configured = ["cn_game_path", "cn_saved_games_path"].iter().any(|key| {
            object
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|path| !path.trim().is_empty())
        });
        let global_configured = ["global_game_path", "global_saved_games_path"]
            .iter()
            .any(|key| {
                object
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|path| !path.trim().is_empty())
            });

        object.insert(
            "cn_battle_net_path".to_string(),
            serde_json::Value::String(if cn_configured && !global_configured {
                battle_net_path
            } else {
                String::new()
            }),
        );
        true
    }

    /// 保存配置到磁盘
    pub fn save(&self, app_data_dir: &str) -> Result<(), AppError> {
        let dir = Path::new(app_data_dir);
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        }
        let path = Self::config_path(app_data_dir);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        Ok(())
    }

    /// 规范化所有快捷键绑定：去除 Win/Meta/Cmd 修饰键，统一首字母大写格式
    /// 返回 true 表示发生了修改，调用方应持久化
    fn strip_win_modifiers(json: &mut String) -> bool {
        let bindings: std::collections::HashMap<String, String> = match serde_json::from_str(json) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let mut changed = false;
        let cleaned: std::collections::HashMap<String, String> = bindings
            .into_iter()
            .filter_map(|(pos, combo)| {
                let lower = combo.to_lowercase();
                // 剥离 Win/Meta/Cmd 修饰键
                let stripped_parts: Vec<&str> = lower
                    .split('+')
                    .filter(|p| !matches!(*p, "win" | "meta" | "cmd" | "command"))
                    .collect();
                if stripped_parts.is_empty() {
                    log::warn!(
                        "快捷键位置 {} 的原绑定 \"{}\" 仅包含 Win 修饰键，已自动清除",
                        pos,
                        combo
                    );
                    changed = true;
                    return None;
                }
                let had_win = stripped_parts.len() < lower.split('+').count();
                // 对所有部分进行规范化（统一首字母大写格式）
                let normalized = stripped_parts
                    .iter()
                    .map(|p| Self::capitalize_key_part(p))
                    .collect::<Vec<_>>()
                    .join("+");
                if normalized != combo {
                    if had_win {
                        log::warn!(
                            "快捷键位置 {} 的原绑定 \"{}\" 包含 Win/Meta/Cmd，已自动迁移为 \"{}\"",
                            pos,
                            combo,
                            normalized
                        );
                    } else {
                        log::warn!(
                            "快捷键位置 {} 的原绑定 \"{}\" 格式不规范，已自动规范化为 \"{}\"",
                            pos,
                            combo,
                            normalized
                        );
                    }
                    changed = true;
                    Some((pos, normalized))
                } else {
                    Some((pos, combo))
                }
            })
            .collect();
        if changed {
            *json = serde_json::to_string(&cleaned).unwrap_or_else(|_| "{}".to_string());
        }
        changed
    }

    /// 将小写键名转为标准格式：Ctrl, Alt, Shift, F1-F24, A-Z, 0-9, Space, Enter 等
    fn capitalize_key_part(p: &str) -> String {
        match p {
            "ctrl" => "Ctrl".to_string(),
            "alt" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "space" => "Space".to_string(),
            "enter" => "Enter".to_string(),
            "tab" => "Tab".to_string(),
            "escape" => "Escape".to_string(),
            "backspace" => "Backspace".to_string(),
            "delete" => "Delete".to_string(),
            "insert" => "Insert".to_string(),
            "home" => "Home".to_string(),
            "end" => "End".to_string(),
            "pageup" => "PageUp".to_string(),
            "pagedown" => "PageDown".to_string(),
            "up" => "Up".to_string(),
            "down" => "Down".to_string(),
            "left" => "Left".to_string(),
            "right" => "Right".to_string(),
            _ if p.len() == 1 => p.to_uppercase(),
            _ => p.to_string(),
        }
    }

    /// 保存窗口几何
    pub fn save_geometry(app_data_dir: &str, geo: &WindowGeometry) -> Result<(), AppError> {
        let dir = Path::new(app_data_dir);
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        }
        let path = Self::geometry_path(app_data_dir);
        let content = serde_json::to_string_pretty(geo)?;
        std::fs::write(&path, content).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        Ok(())
    }

    /// 加载窗口几何
    pub fn load_geometry(app_data_dir: &str) -> Option<WindowGeometry> {
        let path = Self::geometry_path(app_data_dir);
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str::<WindowGeometry>(&content).ok()
    }

    fn overlay_geometry_path(app_data_dir: &str) -> PathBuf {
        Path::new(app_data_dir).join("overlay_geometry.json")
    }

    fn stats_overlay_geometry_path(app_data_dir: &str) -> PathBuf {
        Path::new(app_data_dir).join("stats_overlay_geometry.json")
    }

    /// 保存悬浮窗几何
    pub fn save_overlay_geometry_fn(
        app_data_dir: &str,
        geo: &WindowGeometry,
    ) -> Result<(), AppError> {
        let dir = Path::new(app_data_dir);
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        }
        let path = Self::overlay_geometry_path(app_data_dir);
        let content = serde_json::to_string_pretty(geo)?;
        std::fs::write(&path, content).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        Ok(())
    }

    /// 加载悬浮窗几何
    pub fn load_overlay_geometry_fn(app_data_dir: &str) -> Option<WindowGeometry> {
        let path = Self::overlay_geometry_path(app_data_dir);
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str::<WindowGeometry>(&content).ok()
    }

    pub fn save_stats_overlay_geometry_fn(
        app_data_dir: &str,
        geo: &WindowGeometry,
    ) -> Result<(), AppError> {
        let dir = Path::new(app_data_dir);
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        }
        let content = serde_json::to_string_pretty(geo)?;
        std::fs::write(Self::stats_overlay_geometry_path(app_data_dir), content)
            .map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        Ok(())
    }

    pub fn load_stats_overlay_geometry_fn(app_data_dir: &str) -> Option<WindowGeometry> {
        let path = Self::stats_overlay_geometry_path(app_data_dir);
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str::<WindowGeometry>(&content).ok()
    }

    /// 确保必要的目录存在
    pub fn ensure_dirs(&self) -> Result<(), AppError> {
        for dir in [&self.accounts_dir] {
            if !dir.is_empty() {
                std::fs::create_dir_all(dir).map_err(|e| {
                    AppError::ConfigWriteError(format!("无法创建目录 {}: {}", dir, e))
                })?;
            }
        }
        Ok(())
    }
}

pub fn update_shortcut_map(state: &SharedState, cfg: &GlobalConfig) {
    let mut map = state.shortcut_map.write();
    map.clear();
    let bindings: std::collections::HashMap<String, String> =
        serde_json::from_str(&cfg.shortcut_bindings_json).unwrap_or_default();
    for (pos_str, shortcut) in &bindings {
        if let Ok(pos) = pos_str.parse::<usize>() {
            if pos >= 1 {
                map.insert(shortcut.to_lowercase(), pos);
            }
        }
    }
}

// ── Tauri Commands ──

#[tauri::command]
pub fn get_global_config(state: tauri::State<'_, SharedState>) -> Result<GlobalConfig, AppError> {
    let config = state.config.read();
    match &*config {
        Some(c) => Ok(c.clone()),
        None => {
            // 首次调用，尝试从磁盘加载
            drop(config);
            let loaded = GlobalConfig::load(&state.app_data_dir)?;
            let mut cfg = state.config.write();
            *cfg = Some(loaded.clone());
            update_shortcut_map(&state, &loaded);
            Ok(loaded)
        }
    }
}

#[tauri::command]
pub fn save_global_config(
    state: tauri::State<'_, SharedState>,
    config: GlobalConfig,
) -> Result<(), AppError> {
    let previous = state.config.read().clone();
    let mut cfg = config.clone();
    cfg.version = CURRENT_CONFIG_VERSION;
    // 保留旧字段作为向后兼容总状态，新代码只读取两个独立开关。
    cfg.enable_overlay = cfg.enable_tz_overlay || cfg.enable_stats_overlay;
    cfg.accounts_dir = app_accounts_dir(&state.app_data_dir)
        .to_string_lossy()
        .to_string();

    if should_validate_installation_paths(previous.as_ref(), &cfg) {
        validate_installation_paths(&cfg)?;
    }
    cfg.resolve_ocr_target_account()?;

    cfg.save(&state.app_data_dir)?;
    cfg.ensure_dirs()?;
    let mut stored = state.config.write();
    *stored = Some(cfg.clone());
    update_shortcut_map(&state, &cfg);
    crate::input_listener::set_bongo_cat_input_enabled(cfg.enable_bongo_cat);
    Ok(())
}

#[tauri::command]
pub fn check_saved_games_settings(path: String) -> bool {
    saved_games_settings_exists(Path::new(&path))
}

/// 保存窗口几何信息（位置+尺寸）
#[tauri::command]
pub fn save_window_geometry(
    state: tauri::State<'_, SharedState>,
    geometry: WindowGeometry,
) -> Result<(), AppError> {
    GlobalConfig::save_geometry(&state.app_data_dir, &geometry)
}

/// 加载窗口几何信息（返回 None 表示从未保存过）
#[tauri::command]
pub fn load_window_geometry(
    state: tauri::State<'_, SharedState>,
) -> Result<Option<WindowGeometry>, AppError> {
    Ok(GlobalConfig::load_geometry(&state.app_data_dir))
}

fn detect_saved_games_path_for_edition(cn: bool) -> Option<String> {
    let user = dirs::home_dir()?;
    let saved_games = user.join("Saved Games");
    let entries = std::fs::read_dir(&saved_games).ok()?;
    entries.flatten().find_map(|entry| {
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

/// 检测 ProgramData 下的 Agent 路径
#[tauri::command]
pub fn detect_program_data_agent_path() -> Option<String> {
    let path = r"C:\ProgramData\Battle.net\Agent";
    if Path::new(path).exists() {
        Some(path.to_string())
    } else {
        None
    }
}

/// 供非命令函数（如 tray）获取全局配置
pub fn get_global_config_ext(app: &tauri::AppHandle) -> Option<GlobalConfig> {
    use tauri::Manager;
    if let Some(state) = app.try_state::<SharedState>() {
        let config_lock = state.config.read();
        return config_lock.clone();
    }
    None
}

/// 检测 AppData\Roaming\Battle.net 路径
#[tauri::command]
pub fn detect_app_data_roaming_bnet_path() -> Option<String> {
    if let Some(appdata) = dirs::config_dir() {
        // config_dir on Windows = %APPDATA%
        let bnet = appdata.join("Battle.net");
        if bnet.exists() {
            return Some(bnet.to_string_lossy().to_string());
        }
    }
    None
}
/// 自动探测浏览器路径（仅支持 Edge 和 Chrome）
/// 返回 (path, browser_type) 或 None
#[tauri::command]
pub fn detect_browser_path() -> Option<(String, String)> {
    // 1. 优先检测 Microsoft Edge（系统自带，路径稳定）
    let edge_candidates = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ];
    for p in &edge_candidates {
        if std::path::Path::new(p).exists() {
            return Some((p.to_string(), "edge".to_string()));
        }
    }
    // 也尝试通过 LocalAppData 找
    if let Some(local) = dirs::data_local_dir() {
        let edge = local
            .join("Microsoft")
            .join("Edge")
            .join("Application")
            .join("msedge.exe");
        if edge.exists() {
            return Some((edge.to_string_lossy().to_string(), "edge".to_string()));
        }
    }

    // 2. 检测 Google Chrome
    let chrome_candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ];
    for p in &chrome_candidates {
        if std::path::Path::new(p).exists() {
            return Some((p.to_string(), "chrome".to_string()));
        }
    }
    if let Some(local) = dirs::data_local_dir() {
        let chrome = local
            .join("Google")
            .join("Chrome")
            .join("Application")
            .join("chrome.exe");
        if chrome.exists() {
            return Some((chrome.to_string_lossy().to_string(), "chrome".to_string()));
        }
    }

    None
}

/// 保存悬浮窗几何信息（位置+尺寸）
#[tauri::command]
pub fn save_overlay_geometry(
    state: tauri::State<'_, SharedState>,
    geometry: WindowGeometry,
) -> Result<(), AppError> {
    GlobalConfig::save_overlay_geometry_fn(&state.app_data_dir, &geometry)
}

/// 加载悬浮窗几何信息
#[tauri::command]
pub fn load_overlay_geometry(
    state: tauri::State<'_, SharedState>,
) -> Result<Option<WindowGeometry>, AppError> {
    Ok(GlobalConfig::load_overlay_geometry_fn(&state.app_data_dir))
}

#[tauri::command]
pub fn save_stats_overlay_geometry(
    state: tauri::State<'_, SharedState>,
    geometry: WindowGeometry,
) -> Result<(), AppError> {
    GlobalConfig::save_stats_overlay_geometry_fn(&state.app_data_dir, &geometry)
}

#[tauri::command]
pub fn load_stats_overlay_geometry(
    state: tauri::State<'_, SharedState>,
) -> Result<Option<WindowGeometry>, AppError> {
    Ok(GlobalConfig::load_stats_overlay_geometry_fn(
        &state.app_data_dir,
    ))
}

/// 保存当前选中的主题
#[tauri::command]
pub fn save_theme(
    state: tauri::State<'_, SharedState>,
    theme: String,
    window: tauri::Window,
) -> Result<(), AppError> {
    let mut config_lock = state.config.write();
    if let Some(ref mut cfg) = *config_lock {
        if window.label() == "overlay" {
            cfg.theme_overlay = theme;
        } else {
            cfg.theme = theme;
        }
        cfg.save(&state.app_data_dir)?;
    }
    Ok(())
}

/// 根据选择的浏览器类型（edge 或 chrome）自动探测路径
#[tauri::command]
pub fn detect_browser_path_by_type(browser_type: String) -> Option<String> {
    if browser_type == "edge" {
        let edge_candidates = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ];
        for p in &edge_candidates {
            if std::path::Path::new(p).exists() {
                return Some(p.to_string());
            }
        }
        if let Some(local) = dirs::data_local_dir() {
            let edge = local
                .join("Microsoft")
                .join("Edge")
                .join("Application")
                .join("msedge.exe");
            if edge.exists() {
                return Some(edge.to_string_lossy().to_string());
            }
        }
    } else if browser_type == "chrome" {
        let chrome_candidates = [
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ];
        for p in &chrome_candidates {
            if std::path::Path::new(p).exists() {
                return Some(p.to_string());
            }
        }
        if let Some(local) = dirs::data_local_dir() {
            let chrome = local
                .join("Google")
                .join("Chrome")
                .join("Application")
                .join("chrome.exe");
            if chrome.exists() {
                return Some(chrome.to_string_lossy().to_string());
            }
        }
    }
    None
}
