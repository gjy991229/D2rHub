use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::commands::account::{recover_account_transactions, AccountManager, AccountMeta};
use crate::error::AppError;
use crate::state::SharedState;

const CURRENT_CONFIG_VERSION: u32 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
enum LegacyRegionPathMigration {
    NotNeeded,
    Migrated,
    Pending(LegacyPathMigration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyBattleNetPathMigration {
    changed: bool,
    pending_path: Option<String>,
}

/// 无法自动判断归属的旧版单客户端路径。仅用于把迁移候选交给设置向导；
/// 用户明确选择国服或国际服之前，不得写回并覆盖旧配置文件。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyPathMigration {
    #[serde(default)]
    pub game_path: String,
    #[serde(default)]
    pub saved_games_path: String,
    #[serde(default)]
    pub battle_net_path: String,
}

/// 全局配置
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchGroupMember {
    pub account_id: String,
    /// None 表示由旧版启动组迁移而来，继续继承账号默认 Mod；Some("") 表示明确不使用 Mod。
    #[serde(default)]
    pub mod_args: Option<String>,
    /// 位置胶囊引用；None 可表示“不指定位置”。
    #[serde(default)]
    pub position_preset_id: Option<String>,
    /// 区分旧版缺失位置配置与新版明确选择“不指定位置”。
    #[serde(default)]
    pub position_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub account_ids: Vec<String>,
    /// 新版启动方案成员配置。account_ids 作为旧版本兼容镜像继续保留。
    #[serde(default)]
    pub members: Vec<LaunchGroupMember>,
}

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
    /// 待用户确认归属的旧版路径。该状态存在时禁止持久化配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_path_migration: Option<LegacyPathMigration>,
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
    /// 音频遥测场景计时与掉落统计悬浮窗。旧配置从 enable_overlay 迁移。
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
    /// 是否启用按 D2R 进程捕获的符文声纹识别。
    #[serde(default)]
    pub rune_audio_enabled: bool,
    /// 被监控的账号 ID（对应 account.json 中的 id）。
    #[serde(default)]
    pub rune_audio_target_account: String,
    /// Gold 码相关识别阈值。
    #[serde(default = "default_rune_audio_detection_threshold")]
    pub rune_audio_detection_threshold: f32,
    /// Drop categories decoded and persisted by the audio monitor.
    #[serde(default = "default_rune_audio_tracked_categories")]
    pub rune_audio_tracked_categories: Vec<String>,
    /// 最低记录符文编号（含）；低于该编号的有效声纹只诊断、不入库。
    #[serde(default = "default_rune_audio_min_rune_number")]
    pub rune_audio_min_rune_number: u32,
    /// 最低记录宝石等级（1=碎裂，5=完美）。
    #[serde(default = "default_rune_audio_min_gem_level")]
    pub rune_audio_min_gem_level: u32,
    /// 独立记录的护身符基础代码；旧配置默认三种全部记录。
    #[serde(default = "default_rune_audio_tracked_charm_codes")]
    pub rune_audio_tracked_charm_codes: Vec<String>,
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
    /// 可复用的账号启动组合。账号启动顺序由账号卡片当前排序决定。
    #[serde(default)]
    pub launch_groups: Vec<LaunchGroup>,
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

fn default_rune_audio_detection_threshold() -> f32 {
    0.56
}

fn default_rune_audio_tracked_categories() -> Vec<String> {
    crate::rune_audio::item_catalog::default_tracked_categories()
}

fn default_rune_audio_min_rune_number() -> u32 {
    1
}

fn default_rune_audio_min_gem_level() -> u32 {
    1
}

fn default_rune_audio_tracked_charm_codes() -> Vec<String> {
    crate::rune_audio::item_catalog::default_tracked_charm_codes()
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
        validate_installation_paths, GlobalConfig, LaunchGroup, LaunchGroupMember,
        LegacyPathMigration, CURRENT_CONFIG_VERSION,
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
    fn launch_groups_are_trimmed_and_deduplicated_without_changing_member_order() {
        let mut config = GlobalConfig {
            launch_groups: vec![LaunchGroup {
                id: "  primary  ".to_string(),
                name: "  主力队  ".to_string(),
                account_ids: vec![
                    " account-b ".to_string(),
                    "account-a".to_string(),
                    "account-b".to_string(),
                    " ".to_string(),
                ],
                members: Vec::new(),
            }],
            ..GlobalConfig::default()
        };

        assert!(config.normalize_launch_groups());
        assert_eq!(config.launch_groups[0].id, "primary");
        assert_eq!(config.launch_groups[0].name, "主力队");
        assert_eq!(
            config.launch_groups[0].account_ids,
            vec!["account-b".to_string(), "account-a".to_string()]
        );
        assert!(config.validate_launch_groups().is_ok());
    }

    #[test]
    fn launch_group_members_are_authoritative_and_keep_the_legacy_id_mirror() {
        let mut config = GlobalConfig {
            launch_groups: vec![LaunchGroup {
                id: " plan ".to_string(),
                name: " 方案 ".to_string(),
                account_ids: vec!["stale-account".to_string()],
                members: vec![
                    LaunchGroupMember {
                        account_id: " account-b ".to_string(),
                        mod_args: Some(" -mod b ".to_string()),
                        position_preset_id: Some(" right ".to_string()),
                        position_configured: true,
                    },
                    LaunchGroupMember {
                        account_id: "account-b".to_string(),
                        mod_args: Some("ignored duplicate".to_string()),
                        position_preset_id: None,
                        position_configured: true,
                    },
                    LaunchGroupMember {
                        account_id: "account-a".to_string(),
                        mod_args: Some(String::new()),
                        position_preset_id: None,
                        position_configured: true,
                    },
                ],
            }],
            ..GlobalConfig::default()
        };

        assert!(config.normalize_launch_groups());
        let group = &config.launch_groups[0];
        assert_eq!(group.account_ids, ["account-b", "account-a"]);
        assert_eq!(group.members[0].mod_args.as_deref(), Some("-mod b"));
        assert_eq!(
            group.members[0].position_preset_id.as_deref(),
            Some("right")
        );
        assert!(config.validate_launch_groups().is_ok());
    }

    #[test]
    fn launch_group_names_and_ids_must_be_unique() {
        let mut config = GlobalConfig {
            launch_groups: vec![
                LaunchGroup {
                    id: "group-1".to_string(),
                    name: "Farm".to_string(),
                    account_ids: vec!["account-a".to_string()],
                    members: Vec::new(),
                },
                LaunchGroup {
                    id: "group-2".to_string(),
                    name: "farm".to_string(),
                    account_ids: vec!["account-b".to_string()],
                    members: Vec::new(),
                },
            ],
            ..GlobalConfig::default()
        };
        config.normalize_launch_groups();
        assert!(config.validate_launch_groups().is_err());

        config.launch_groups[1].name = "副队".to_string();
        config.launch_groups[1].id = "group-1".to_string();
        assert!(config.validate_launch_groups().is_err());
    }

    #[test]
    fn deleting_an_account_removes_it_from_every_group_but_keeps_empty_groups() {
        let mut config = GlobalConfig {
            launch_groups: vec![
                LaunchGroup {
                    id: "only".to_string(),
                    name: "单账号组".to_string(),
                    account_ids: vec!["account-a".to_string()],
                    members: Vec::new(),
                },
                LaunchGroup {
                    id: "mixed".to_string(),
                    name: "混合组".to_string(),
                    account_ids: vec!["account-a".to_string(), "account-b".to_string()],
                    members: vec![
                        LaunchGroupMember {
                            account_id: "account-a".to_string(),
                            mod_args: Some("-mod a".to_string()),
                            position_preset_id: None,
                            position_configured: true,
                        },
                        LaunchGroupMember {
                            account_id: "account-b".to_string(),
                            mod_args: Some(String::new()),
                            position_preset_id: None,
                            position_configured: true,
                        },
                    ],
                },
            ],
            ..GlobalConfig::default()
        };

        assert!(config.remove_account_from_launch_groups("account-a"));
        assert_eq!(config.launch_groups.len(), 2);
        assert!(config.launch_groups[0].account_ids.is_empty());
        assert_eq!(
            config.launch_groups[1].account_ids,
            vec!["account-b".to_string()]
        );
        assert_eq!(config.launch_groups[1].members.len(), 1);
        assert_eq!(config.launch_groups[1].members[0].account_id, "account-b");
        assert!(!config.remove_account_from_launch_groups("missing"));
    }

    #[test]
    fn enabled_rune_audio_requires_a_selected_account() {
        let config = GlobalConfig {
            rune_audio_enabled: true,
            ..GlobalConfig::default()
        };

        assert!(config.resolve_rune_audio_target_account().is_err());
    }

    #[test]
    fn disabled_rune_audio_does_not_require_a_target_account() {
        let config = GlobalConfig::default();

        assert!(config
            .resolve_rune_audio_target_account()
            .unwrap()
            .is_none());
    }

    #[test]
    fn enabled_rune_audio_requires_an_initialized_account() {
        let accounts_dir = temp_dir("rune_audio_uninitialized_account");
        let account = AccountMeta::new("acount1");
        std::fs::create_dir_all(accounts_dir.join(&account.id)).unwrap();
        AccountManager::save_meta(accounts_dir.to_str().unwrap(), &account).unwrap();

        let config = GlobalConfig {
            accounts_dir: accounts_dir.to_string_lossy().to_string(),
            rune_audio_enabled: true,
            rune_audio_target_account: account.id,
            ..GlobalConfig::default()
        };

        assert!(config.resolve_rune_audio_target_account().is_err());
        let _ = std::fs::remove_dir_all(accounts_dir);
    }

    #[test]
    fn enabled_rune_audio_accepts_an_initialized_account() {
        let accounts_dir = temp_dir("rune_audio_initialized_account");
        let mut account = AccountMeta::new("acount1");
        account.initialized = true;
        std::fs::create_dir_all(accounts_dir.join(&account.id)).unwrap();
        AccountManager::save_meta(accounts_dir.to_str().unwrap(), &account).unwrap();

        let config = GlobalConfig {
            accounts_dir: accounts_dir.to_string_lossy().to_string(),
            rune_audio_enabled: true,
            rune_audio_target_account: account.id.clone(),
            ..GlobalConfig::default()
        };

        let resolved = config.resolve_rune_audio_target_account().unwrap().unwrap();
        assert_eq!(resolved.id, account.id);
        let _ = std::fs::remove_dir_all(accounts_dir);
    }

    #[test]
    fn invalid_rune_audio_configuration_is_disabled() {
        let mut config = GlobalConfig {
            rune_audio_enabled: true,
            ..GlobalConfig::default()
        };

        assert!(config.normalize_rune_audio_configuration());
        assert!(!config.rune_audio_enabled);
    }

    #[test]
    fn missing_tracking_categories_use_the_full_default_catalog() {
        let mut value = serde_json::to_value(GlobalConfig::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("rune_audio_tracked_categories");

        let config: GlobalConfig = serde_json::from_value(value).unwrap();

        assert_eq!(
            config.rune_audio_tracked_categories,
            crate::rune_audio::item_catalog::default_tracked_categories()
        );
    }

    #[test]
    fn missing_minimum_rune_number_records_all_runes_for_backward_compatibility() {
        let mut value = serde_json::to_value(GlobalConfig::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("rune_audio_min_rune_number");

        let config: GlobalConfig = serde_json::from_value(value).unwrap();

        assert_eq!(config.rune_audio_min_rune_number, 1);
    }

    #[test]
    fn minimum_rune_number_is_normalized_to_the_protocol_range() {
        let mut below = GlobalConfig {
            rune_audio_min_rune_number: 0,
            ..GlobalConfig::default()
        };
        assert!(below.normalize_rune_audio_configuration());
        assert_eq!(below.rune_audio_min_rune_number, 1);

        let mut above = GlobalConfig {
            rune_audio_min_rune_number: 99,
            ..GlobalConfig::default()
        };
        assert!(above.normalize_rune_audio_configuration());
        assert_eq!(above.rune_audio_min_rune_number, 33);
    }

    #[test]
    fn missing_detailed_item_filters_preserve_legacy_recording_behavior() {
        let mut value = serde_json::to_value(GlobalConfig::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("rune_audio_min_gem_level");
        object.remove("rune_audio_tracked_charm_codes");

        let config: GlobalConfig = serde_json::from_value(value).unwrap();

        assert_eq!(config.rune_audio_min_gem_level, 1);
        assert_eq!(
            config.rune_audio_tracked_charm_codes,
            crate::rune_audio::item_catalog::default_tracked_charm_codes()
        );
    }

    #[test]
    fn detailed_item_filters_are_normalized() {
        let mut config = GlobalConfig {
            rune_audio_min_gem_level: 99,
            rune_audio_tracked_charm_codes: vec![
                " CM3 ".to_string(),
                "unknown".to_string(),
                "cm1".to_string(),
            ],
            ..GlobalConfig::default()
        };

        assert!(config.normalize_rune_audio_configuration());
        assert_eq!(config.rune_audio_min_gem_level, 5);
        assert_eq!(config.rune_audio_tracked_charm_codes, ["cm1", "cm3"]);
    }

    #[test]
    fn tracking_categories_are_normalized_before_use() {
        let mut config = GlobalConfig {
            rune_audio_tracked_categories: vec![
                " keys ".to_string(),
                "unknown".to_string(),
                "runes".to_string(),
                "KEYS".to_string(),
            ],
            ..GlobalConfig::default()
        };

        assert!(config.normalize_rune_audio_configuration());
        assert_eq!(config.rune_audio_tracked_categories, ["runes", "keys"]);
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
            theme: "onyx".to_string(),
            auto_close_browser: false,
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
        assert_eq!(config.theme, "onyx");
        assert!(!config.auto_close_browser);
        assert_eq!(
            config.legacy_path_migration,
            Some(LegacyPathMigration {
                game_path: r"D:\Games\D2R".to_string(),
                saved_games_path: r"D:\Saves\D2R".to_string(),
                battle_net_path: r"D:\Battle.net\Battle.net.exe".to_string(),
            })
        );
        assert!(config.save(root.to_str().unwrap()).is_err());
        let preserved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(preserved["version"], 1);
        assert_eq!(preserved["saved_games_path"], r"D:\Saves\D2R");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_recovers_account_transaction_before_normalizing_rune_audio_target() {
        let root = temp_dir("rune_audio_after_account_recovery");
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
            rune_audio_enabled: true,
            rune_audio_target_account: account.id,
            ..GlobalConfig::default()
        };
        config.save(root.to_str().unwrap()).unwrap();

        let loaded = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert!(loaded.rune_audio_enabled);
        assert!(accounts.join("acount1").is_dir());
        assert!(!backup.exists());
        assert!(!staged.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_legacy_battle_net_path_is_returned_for_confirmation() {
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

        let migration = GlobalConfig::migrate_legacy_battle_net_paths(&mut legacy);
        assert!(legacy.get("battle_net_path").is_some());
        let config: GlobalConfig = serde_json::from_value(legacy).unwrap();

        assert!(config.cn_battle_net_path.is_empty());
        assert_eq!(
            migration.pending_path.as_deref(),
            Some(r"C:\Battle.net\Battle.net.exe")
        );
    }

    #[test]
    fn mixed_legacy_and_empty_new_keys_fill_the_new_profile() {
        let root = temp_dir("mixed_empty_new_keys");
        let config_path = root.join("global_config.json");
        let mut mixed = serde_json::to_value(GlobalConfig::default()).unwrap();
        let object = mixed.as_object_mut().unwrap();
        object.insert("version".to_string(), serde_json::json!(1));
        object.insert(
            "game_path".to_string(),
            serde_json::json!(r"D:\Games\D2R-CN"),
        );
        object.insert(
            "saved_games_path".to_string(),
            serde_json::json!(r"C:\Users\Tester\Saved Games\Diablo II Resurrected (CN)"),
        );
        object.insert(
            "battle_net_path".to_string(),
            serde_json::json!(r"C:\Battle.net\Battle.net.exe"),
        );
        std::fs::write(&config_path, serde_json::to_vec_pretty(&mixed).unwrap()).unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();

        assert_eq!(config.cn_game_path, r"D:\Games\D2R-CN");
        assert_eq!(config.cn_battle_net_path, r"C:\Battle.net\Battle.net.exe");
        assert!(config.legacy_path_migration.is_none());
        assert!(persisted.get("game_path").is_none());
        assert!(persisted.get("battle_net_path").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn conflicting_mixed_paths_stay_pending_instead_of_dropping_the_legacy_value() {
        let root = temp_dir("mixed_conflicting_keys");
        let config_path = root.join("global_config.json");
        let mut mixed = serde_json::to_value(GlobalConfig {
            global_game_path: r"E:\Current\D2R".to_string(),
            global_saved_games_path: r"C:\Users\Tester\Saved Games\Diablo II Resurrected"
                .to_string(),
            ..GlobalConfig::default()
        })
        .unwrap();
        let object = mixed.as_object_mut().unwrap();
        object.insert("version".to_string(), serde_json::json!(1));
        object.insert("game_path".to_string(), serde_json::json!(r"D:\Legacy\D2R"));
        object.insert(
            "saved_games_path".to_string(),
            serde_json::json!(r"C:\Users\Tester\Saved Games\Diablo II Resurrected"),
        );
        std::fs::write(&config_path, serde_json::to_vec_pretty(&mixed).unwrap()).unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();
        let preserved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();

        assert_eq!(config.global_game_path, r"E:\Current\D2R");
        assert_eq!(
            config
                .legacy_path_migration
                .as_ref()
                .map(|candidate| candidate.game_path.as_str()),
            Some(r"D:\Legacy\D2R")
        );
        assert_eq!(preserved["game_path"], r"D:\Legacy\D2R");
        let _ = std::fs::remove_dir_all(root);
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

    #[test]
    fn config_save_rotates_the_previous_primary_into_a_backup() {
        let root = temp_dir("global_config_backup_rotation");
        let first = GlobalConfig {
            theme: "light".to_string(),
            ..GlobalConfig::default()
        };
        first.save(root.to_str().unwrap()).unwrap();
        let second = GlobalConfig {
            theme: "onyx".to_string(),
            ..first.clone()
        };
        second.save(root.to_str().unwrap()).unwrap();

        let primary: GlobalConfig =
            serde_json::from_slice(&std::fs::read(root.join("global_config.json")).unwrap())
                .unwrap();
        let backup: GlobalConfig =
            serde_json::from_slice(&std::fs::read(root.join("global_config.json.bak")).unwrap())
                .unwrap();
        assert_eq!(primary.theme, "onyx");
        assert_eq!(backup.theme, "light");
        assert!(!root.join("global_config.json.tmp").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_primary_recovers_the_last_good_backup_and_archives_the_bad_file() {
        let root = temp_dir("malformed_primary_recovery");
        let first = GlobalConfig {
            theme: "light".to_string(),
            ..GlobalConfig::default()
        };
        first.save(root.to_str().unwrap()).unwrap();
        GlobalConfig {
            theme: "onyx".to_string(),
            ..first
        }
        .save(root.to_str().unwrap())
        .unwrap();
        std::fs::write(root.join("global_config.json"), "{broken").unwrap();

        let recovered = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert_eq!(recovered.theme, "light");
        assert!(root.join("global_config.json.bak").is_file());
        let corrupt = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("global_config.corrupt-"))
            })
            .unwrap();
        assert_eq!(std::fs::read_to_string(corrupt).unwrap(), "{broken");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn wrong_field_type_also_recovers_from_the_last_good_backup() {
        let root = temp_dir("wrong_field_type_recovery");
        let first = GlobalConfig::default();
        first.save(root.to_str().unwrap()).unwrap();
        GlobalConfig {
            theme: "onyx".to_string(),
            ..first
        }
        .save(root.to_str().unwrap())
        .unwrap();
        let mut invalid: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("global_config.json")).unwrap())
                .unwrap();
        invalid["enable_overlay"] = serde_json::json!("not-a-boolean");
        std::fs::write(
            root.join("global_config.json"),
            serde_json::to_vec_pretty(&invalid).unwrap(),
        )
        .unwrap();

        let recovered = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert_eq!(recovered.theme, GlobalConfig::default().theme);
        assert!(std::fs::read_dir(&root).unwrap().flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("global_config.corrupt-")
        }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_primary_without_a_backup_fails_closed_and_stays_untouched() {
        let root = temp_dir("corrupt_without_backup");
        let config_path = root.join("global_config.json");
        std::fs::write(&config_path, "{broken").unwrap();

        assert!(GlobalConfig::load(root.to_str().unwrap()).is_err());
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), "{broken");
        assert!(!root.join("global_config.json.bak").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn wrong_legacy_path_type_fails_closed_instead_of_becoming_an_empty_path() {
        let root = temp_dir("wrong_legacy_path_type");
        let config_path = root.join("global_config.json");
        let invalid = serde_json::json!({
            "version": 1,
            "game_path": 42,
            "saved_games_path": r"C:\Saved Games\Diablo II Resurrected",
            "battle_net_path": r"C:\Battle.net\Battle.net.exe",
            "program_data_agent_path": r"C:\ProgramData\Battle.net\Agent",
            "app_data_roaming_bnet_path": r"C:\Roaming\Battle.net",
            "accounts_dir": r"C:\D2RHub\accounts",
            "first_run_complete": true
        });
        let bytes = serde_json::to_vec_pretty(&invalid).unwrap();
        std::fs::write(&config_path, &bytes).unwrap();

        assert!(GlobalConfig::load(root.to_str().unwrap()).is_err());
        assert_eq!(std::fs::read(&config_path).unwrap(), bytes);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn completed_marker_without_any_game_path_returns_to_setup_without_losing_settings() {
        let root = temp_dir("completed_without_game_path");
        let config_path = root.join("global_config.json");
        let broken = GlobalConfig {
            first_run_complete: true,
            theme: "onyx".to_string(),
            font_scale: "large".to_string(),
            ..GlobalConfig::default()
        };
        std::fs::write(&config_path, serde_json::to_vec_pretty(&broken).unwrap()).unwrap();

        let loaded = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert!(!loaded.first_run_complete);
        assert_eq!(loaded.theme, "onyx");
        assert_eq!(loaded.font_scale, "large");
        let persisted: GlobalConfig =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        assert!(!persisted.first_run_complete);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_primary_is_restored_from_the_last_good_backup() {
        let root = temp_dir("missing_primary_recovery");
        let first = GlobalConfig {
            theme: "light".to_string(),
            ..GlobalConfig::default()
        };
        first.save(root.to_str().unwrap()).unwrap();
        GlobalConfig {
            theme: "onyx".to_string(),
            ..first
        }
        .save(root.to_str().unwrap())
        .unwrap();
        std::fs::remove_file(root.join("global_config.json")).unwrap();

        let recovered = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert_eq!(recovered.theme, "light");
        assert!(root.join("global_config.json").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn synced_staging_file_wins_when_save_was_interrupted_after_primary_rotation() {
        let root = temp_dir("interrupted_after_primary_rotation");
        let original = GlobalConfig {
            theme: "light".to_string(),
            ..GlobalConfig::default()
        };
        original.save(root.to_str().unwrap()).unwrap();
        let latest = GlobalConfig {
            theme: "onyx".to_string(),
            ..original
        };
        std::fs::write(
            root.join("global_config.json.tmp"),
            serde_json::to_vec_pretty(&latest).unwrap(),
        )
        .unwrap();
        std::fs::rename(
            root.join("global_config.json"),
            root.join("global_config.json.bak"),
        )
        .unwrap();

        let recovered = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert_eq!(recovered.theme, "onyx");
        assert!(root.join("global_config.json").is_file());
        assert!(!root.join("global_config.json.tmp").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unreadable_backup_without_a_primary_fails_closed_instead_of_returning_defaults() {
        let root = temp_dir("unreadable_backup_only");
        let backup = root.join("global_config.json.bak");
        std::fs::write(&backup, "{broken-backup").unwrap();

        assert!(GlobalConfig::load(root.to_str().unwrap()).is_err());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{broken-backup");
        assert!(!root.join("global_config.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn future_config_version_fails_closed_even_when_an_older_backup_exists() {
        let root = temp_dir("future_version");
        let first = GlobalConfig::default();
        first.save(root.to_str().unwrap()).unwrap();
        GlobalConfig {
            theme: "onyx".to_string(),
            ..first
        }
        .save(root.to_str().unwrap())
        .unwrap();
        let config_path = root.join("global_config.json");
        let mut future: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        future["version"] = serde_json::json!(CURRENT_CONFIG_VERSION + 1);
        future["future_only_setting"] = serde_json::json!("must-survive");
        std::fs::write(&config_path, serde_json::to_vec_pretty(&future).unwrap()).unwrap();

        assert!(GlobalConfig::load(root.to_str().unwrap()).is_err());
        let preserved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(preserved["version"], CURRENT_CONFIG_VERSION + 1);
        assert_eq!(preserved["future_only_setting"], "must-survive");
        assert!(root.join("global_config.json.bak").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn v6_launch_groups_load_unchanged_and_gain_the_v7_member_field() {
        let root = temp_dir("v6_launch_groups");
        let config_path = root.join("global_config.json");
        let mut legacy = serde_json::to_value(GlobalConfig::default()).unwrap();
        legacy["version"] = serde_json::json!(6);
        legacy["launch_groups"] = serde_json::json!([{
            "id": "legacy-plan",
            "name": "旧多选",
            "account_ids": ["acount1", "acount2"]
        }]);
        std::fs::write(&config_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let loaded = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert_eq!(loaded.version, CURRENT_CONFIG_VERSION);
        assert_eq!(loaded.launch_groups.len(), 1);
        assert_eq!(loaded.launch_groups[0].account_ids, ["acount1", "acount2"]);
        assert!(loaded.launch_groups[0].members.is_empty());
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(persisted["version"], CURRENT_CONFIG_VERSION);
        assert_eq!(
            persisted["launch_groups"][0]["members"],
            serde_json::json!([])
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn real_v1_shape_migrates_without_requiring_modern_fields() {
        let root = temp_dir("real_v1_shape");
        let config_path = root.join("global_config.json");
        let legacy = serde_json::json!({
            "version": 1,
            "battle_net_path": r"C:\Battle.net\Battle.net.exe",
            "game_path": r"D:\Games\D2R-CN",
            "saved_games_path": r"C:\Users\Tester\Saved Games\Diablo II Resurrected (CN)",
            "program_data_agent_path": r"C:\ProgramData\Battle.net\Agent",
            "app_data_roaming_bnet_path": r"C:\Users\Tester\AppData\Roaming\Battle.net",
            "accounts_dir": r"D:\Portable\config\accounts",
            "first_run_complete": true,
            "theme": "onyx",
            "enable_overlay": false,
            "ocr_enabled": true,
            "ocr_target_account": "acount1"
        });
        std::fs::write(&config_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert_eq!(config.cn_game_path, r"D:\Games\D2R-CN");
        assert_eq!(config.cn_battle_net_path, r"C:\Battle.net\Battle.net.exe");
        assert_eq!(config.theme, "onyx");
        assert!(!config.enable_tz_overlay);
        assert!(!config.enable_stats_overlay);
        assert!(config.legacy_path_migration.is_none());
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        assert!(persisted.get("game_path").is_none());
        assert!(persisted.get("ocr_enabled").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn hypothetical_v2_single_path_shape_uses_the_same_safe_migration() {
        let root = temp_dir("v2_single_path_shape");
        let config_path = root.join("global_config.json");
        let legacy = serde_json::json!({
            "version": 2,
            "battle_net_path": r"C:\Battle.net\Battle.net.exe",
            "game_path": r"D:\Games\D2R-Global",
            "saved_games_path": r"C:\Users\Tester\Saved Games\Diablo II Resurrected",
            "program_data_agent_path": r"C:\ProgramData\Battle.net\Agent",
            "app_data_roaming_bnet_path": r"C:\Roaming\Battle.net",
            "accounts_dir": r"D:\Portable\config\accounts",
            "first_run_complete": true
        });
        std::fs::write(&config_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert_eq!(config.global_game_path, r"D:\Games\D2R-Global");
        assert!(config.cn_game_path.is_empty());
        assert!(config.cn_battle_net_path.is_empty());
        assert!(config.legacy_path_migration.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn real_v3_shape_removes_only_the_deprecated_global_battle_net_path() {
        let root = temp_dir("real_v3_shape");
        let config_path = root.join("global_config.json");
        let legacy = serde_json::json!({
            "version": 3,
            "cn_battle_net_path": r"C:\Battle.net-CN\Battle.net.exe",
            "cn_game_path": r"C:\Games\D2R-CN",
            "cn_saved_games_path": r"C:\Saves\D2R-CN",
            "global_battle_net_path": r"D:\Battle.net\Battle.net.exe",
            "global_game_path": r"D:\Games\D2R-Global",
            "global_saved_games_path": r"D:\Saves\D2R-Global",
            "program_data_agent_path": r"C:\ProgramData\Battle.net\Agent",
            "app_data_roaming_bnet_path": r"C:\Roaming\Battle.net",
            "accounts_dir": r"D:\Portable\config\accounts",
            "first_run_complete": true,
            "enable_overlay": true,
            "theme": "light"
        });
        std::fs::write(&config_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let config = GlobalConfig::load(root.to_str().unwrap()).unwrap();
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();

        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert_eq!(
            config.cn_battle_net_path,
            r"C:\Battle.net-CN\Battle.net.exe"
        );
        assert_eq!(config.global_game_path, r"D:\Games\D2R-Global");
        assert!(persisted.get("global_battle_net_path").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn v4_and_v5_shapes_gain_later_defaults_and_overlay_split_idempotently() {
        for version in [4, 5] {
            let root = temp_dir(&format!("real_v{version}_shape"));
            let config_path = root.join("global_config.json");
            let legacy = serde_json::json!({
                "version": version,
                "cn_battle_net_path": "",
                "cn_game_path": "",
                "cn_saved_games_path": "",
                "global_game_path": r"D:\Games\D2R-Global",
                "global_saved_games_path": r"D:\Saves\D2R-Global",
                "program_data_agent_path": r"C:\ProgramData\Battle.net\Agent",
                "app_data_roaming_bnet_path": r"C:\Roaming\Battle.net",
                "accounts_dir": r"D:\Portable\config\accounts",
                "first_run_complete": true,
                "enable_overlay": false,
                "theme": "light"
            });
            std::fs::write(&config_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

            let first = GlobalConfig::load(root.to_str().unwrap()).unwrap();
            let first_bytes = std::fs::read(&config_path).unwrap();
            let second = GlobalConfig::load(root.to_str().unwrap()).unwrap();
            let second_bytes = std::fs::read(&config_path).unwrap();

            assert_eq!(first.version, CURRENT_CONFIG_VERSION);
            assert!(!first.enable_tz_overlay);
            assert!(!first.enable_stats_overlay);
            assert_eq!(first.global_game_path, second.global_game_path);
            assert_eq!(first_bytes, second_bytes);
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn persisted_pending_marker_survives_restart_until_the_user_resolves_it() {
        let root = temp_dir("persisted_pending_marker");
        let config_path = root.join("global_config.json");
        let pending = LegacyPathMigration {
            game_path: r"D:\Unknown\D2R".to_string(),
            saved_games_path: r"D:\Unknown\Saves".to_string(),
            battle_net_path: r"D:\Unknown\Battle.net.exe".to_string(),
        };
        let config = GlobalConfig {
            first_run_complete: true,
            legacy_path_migration: Some(pending.clone()),
            ..GlobalConfig::default()
        };
        std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

        let loaded = GlobalConfig::load(root.to_str().unwrap()).unwrap();

        assert!(!loaded.first_run_complete);
        assert_eq!(loaded.legacy_path_migration, Some(pending));
        assert!(loaded.save(root.to_str().unwrap()).is_err());
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
            legacy_path_migration: None,
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
            rune_audio_enabled: false,
            rune_audio_target_account: String::new(),
            rune_audio_detection_threshold: default_rune_audio_detection_threshold(),
            rune_audio_tracked_categories: default_rune_audio_tracked_categories(),
            rune_audio_min_rune_number: default_rune_audio_min_rune_number(),
            rune_audio_min_gem_level: default_rune_audio_min_gem_level(),
            rune_audio_tracked_charm_codes: default_rune_audio_tracked_charm_codes(),
            shortcut_bindings_json: r#"{"1":"Ctrl+1","2":"Ctrl+2","3":"Ctrl+3"}"#.to_string(),
            overlay_opacity: 95,
            main_opacity: 95,
            font_scale: "default".to_string(),
            separate_game_taskbar_icons: false,
            app_language: "zh-CN".to_string(),
            agent_mode: 1,
            agent_delay_secs: 1.0,
            agent_threshold: 5,
            launch_groups: Vec::new(),
        }
    }
}

impl GlobalConfig {
    pub(crate) fn remove_account_from_launch_groups(&mut self, account_id: &str) -> bool {
        let mut removed = false;
        for group in &mut self.launch_groups {
            let previous_len = group.account_ids.len();
            group
                .account_ids
                .retain(|member_id| member_id != account_id);
            removed |= group.account_ids.len() != previous_len;
            let previous_member_len = group.members.len();
            group
                .members
                .retain(|member| member.account_id != account_id);
            removed |= group.members.len() != previous_member_len;
        }
        removed
    }

    fn normalize_launch_groups(&mut self) -> bool {
        let original = self.launch_groups.clone();
        for group in &mut self.launch_groups {
            group.id = group.id.trim().to_string();
            group.name = group.name.trim().to_string();
            let mut seen = std::collections::HashSet::new();
            group.account_ids = group
                .account_ids
                .iter()
                .map(|account_id| account_id.trim())
                .filter(|account_id| !account_id.is_empty())
                .filter(|account_id| seen.insert((*account_id).to_string()))
                .map(str::to_string)
                .collect();

            let mut seen_members = std::collections::HashSet::new();
            group.members = group
                .members
                .iter()
                .cloned()
                .filter_map(|mut member| {
                    member.account_id = member.account_id.trim().to_string();
                    if member.account_id.is_empty()
                        || !seen_members.insert(member.account_id.clone())
                    {
                        return None;
                    }
                    member.mod_args = member.mod_args.map(|args| args.trim().to_string());
                    member.position_preset_id = member
                        .position_preset_id
                        .map(|id| id.trim().to_string())
                        .filter(|id| !id.is_empty());
                    Some(member)
                })
                .collect();

            // 新版成员是权威来源；同步 account_ids 让旧版字段始终可读。
            if !group.members.is_empty() {
                group.account_ids = group
                    .members
                    .iter()
                    .map(|member| member.account_id.clone())
                    .collect();
            }
        }
        original != self.launch_groups
    }

    fn validate_launch_groups(&self) -> Result<(), AppError> {
        let mut group_ids = std::collections::HashSet::new();
        let mut group_names = std::collections::HashSet::new();
        for group in &self.launch_groups {
            if group.id.is_empty() {
                return Err(AppError::ConfigWriteError(
                    "启动方案缺少唯一标识".to_string(),
                ));
            }
            if !group_ids.insert(group.id.clone()) {
                return Err(AppError::ConfigWriteError(format!(
                    "启动方案唯一标识重复: {}",
                    group.id
                )));
            }
            if group.name.is_empty() {
                return Err(AppError::ConfigWriteError(
                    "启动方案名称不能为空".to_string(),
                ));
            }
            let comparable_name = group.name.to_lowercase();
            if !group_names.insert(comparable_name) {
                return Err(AppError::ConfigWriteError(format!(
                    "启动方案名称重复: {}",
                    group.name
                )));
            }
            if group
                .members
                .iter()
                .any(|member| member.account_id.is_empty())
            {
                return Err(AppError::ConfigWriteError(format!(
                    "启动方案“{}”包含无效账号",
                    group.name
                )));
            }
        }
        Ok(())
    }

    /// 解析并验证当前声纹识别目标。识别关闭时不要求配置目标账号。
    pub(crate) fn resolve_rune_audio_target_account(
        &self,
    ) -> Result<Option<AccountMeta>, AppError> {
        if !self.rune_audio_enabled {
            return Ok(None);
        }

        let account_id = self.rune_audio_target_account.trim();
        if account_id.is_empty() {
            return Err(AppError::ConfigWriteError(
                "启用符文声纹识别前请先选择目标账号".to_string(),
            ));
        }

        let account = AccountManager::load_meta(&self.accounts_dir, account_id)
            .map_err(|_| AppError::ConfigWriteError(format!("声纹目标账号不存在: {account_id}")))?;
        if !account.initialized {
            return Err(AppError::ConfigWriteError(format!(
                "声纹目标账号尚未初始化: {account_id}"
            )));
        }

        Ok(Some(account))
    }

    /// 兼容旧配置：无效目标不能保持声纹识别启用状态。
    fn normalize_rune_audio_configuration(&mut self) -> bool {
        let normalized = crate::rune_audio::item_catalog::normalize_tracked_categories(
            &self.rune_audio_tracked_categories,
        );
        let mut changed = normalized != self.rune_audio_tracked_categories;
        self.rune_audio_tracked_categories = normalized;
        let minimum_rune = self.rune_audio_min_rune_number.clamp(1, 33);
        if minimum_rune != self.rune_audio_min_rune_number {
            self.rune_audio_min_rune_number = minimum_rune;
            changed = true;
        }
        let minimum_gem = self.rune_audio_min_gem_level.clamp(1, 5);
        if minimum_gem != self.rune_audio_min_gem_level {
            self.rune_audio_min_gem_level = minimum_gem;
            changed = true;
        }
        let charm_codes = crate::rune_audio::item_catalog::normalize_tracked_charm_codes(
            &self.rune_audio_tracked_charm_codes,
        );
        if charm_codes != self.rune_audio_tracked_charm_codes {
            self.rune_audio_tracked_charm_codes = charm_codes;
            changed = true;
        }
        if self.rune_audio_enabled && self.resolve_rune_audio_target_account().is_err() {
            self.rune_audio_enabled = false;
            changed = true;
        }
        changed
    }

    fn config_path(app_data_dir: &str) -> PathBuf {
        Path::new(app_data_dir).join("global_config.json")
    }

    fn config_backup_path(app_data_dir: &str) -> PathBuf {
        Path::new(app_data_dir).join("global_config.json.bak")
    }

    fn config_staging_path(app_data_dir: &str) -> PathBuf {
        Path::new(app_data_dir).join("global_config.json.tmp")
    }

    fn unique_corrupt_config_path(app_data_dir: &str) -> PathBuf {
        let dir = Path::new(app_data_dir);
        let stem = format!("global_config.corrupt-{}", std::process::id());
        for suffix in 0..1000 {
            let name = if suffix == 0 {
                format!("{stem}.json")
            } else {
                format!("{stem}-{suffix}.json")
            };
            let candidate = dir.join(name);
            if !candidate.exists() {
                return candidate;
            }
        }
        dir.join(format!("{stem}-overflow.json"))
    }

    fn config_file_is_readable(path: &Path) -> bool {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str::<GlobalConfig>(&content).ok())
            .is_some()
    }

    fn restore_missing_primary_from_backup(app_data_dir: &str) -> bool {
        let path = Self::config_path(app_data_dir);
        let backup = Self::config_backup_path(app_data_dir);
        let staging = Self::config_staging_path(app_data_dir);
        if path.exists() {
            return false;
        }

        for candidate in [&staging, &backup] {
            if !candidate.is_file() || !Self::config_file_is_readable(candidate) {
                continue;
            }
            match std::fs::rename(candidate, &path) {
                Ok(()) => {
                    log::warn!("主配置缺失，已从 {} 恢复", candidate.display());
                    return true;
                }
                Err(error) => {
                    log::error!("从配置候选 {} 恢复失败: {error}", candidate.display());
                }
            }
        }
        false
    }

    fn archive_corrupt_primary_and_restore_backup(app_data_dir: &str) -> bool {
        let path = Self::config_path(app_data_dir);
        let backup = Self::config_backup_path(app_data_dir);
        let staging = Self::config_staging_path(app_data_dir);
        if !path.exists() {
            return false;
        }

        let recovery = if backup.is_file() && Self::config_file_is_readable(&backup) {
            &backup
        } else if staging.is_file() && Self::config_file_is_readable(&staging) {
            &staging
        } else {
            return false;
        };

        let corrupt = Self::unique_corrupt_config_path(app_data_dir);
        if let Err(error) = std::fs::rename(&path, &corrupt) {
            log::error!("归档损坏配置 {} 失败: {error}", path.display());
            return false;
        }
        if let Err(error) = std::fs::rename(recovery, &path) {
            let _ = std::fs::rename(&corrupt, &path);
            log::error!("安装配置恢复候选 {} 失败: {error}", recovery.display());
            return false;
        }
        log::warn!(
            "主配置无法读取，已恢复可用候选；损坏文件保留在 {}",
            corrupt.display()
        );
        true
    }

    fn geometry_path(app_data_dir: &str) -> PathBuf {
        Path::new(app_data_dir).join("window_geometry.json")
    }

    /// 从磁盘加载配置
    pub fn load(app_data_dir: &str) -> Result<Self, AppError> {
        Self::load_inner(app_data_dir, true)
    }

    fn load_inner(app_data_dir: &str, allow_backup_recovery: bool) -> Result<Self, AppError> {
        let path = Self::config_path(app_data_dir);
        if allow_backup_recovery {
            Self::restore_missing_primary_from_backup(app_data_dir);
        }
        if !path.exists() {
            let backup = Self::config_backup_path(app_data_dir);
            let staging = Self::config_staging_path(app_data_dir);
            if backup.exists() || staging.exists() {
                return Err(AppError::ConfigReadError(
                    "主配置缺失，且遗留的备份或暂存配置无法安全读取".to_string(),
                ));
            }
            return Ok(Self::default());
        }
        let content =
            std::fs::read_to_string(&path).map_err(|e| AppError::ConfigReadError(e.to_string()))?;
        let mut value: serde_json::Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(_error)
                if allow_backup_recovery
                    && Self::archive_corrupt_primary_and_restore_backup(app_data_dir) =>
            {
                return Self::load_inner(app_data_dir, false);
            }
            Err(error) => return Err(error.into()),
        };
        if value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|version| version > u64::from(CURRENT_CONFIG_VERSION))
        {
            return Err(AppError::ConfigReadError(format!(
                "配置版本高于当前程序支持的 v{CURRENT_CONFIG_VERSION}，请使用更新版本的 D2RHub 打开"
            )));
        }
        if let Some(invalid_key) = [
            "game_path",
            "saved_games_path",
            "battle_net_path",
            "global_battle_net_path",
        ]
        .into_iter()
        .find(|key| value.get(*key).is_some_and(|field| !field.is_string()))
        {
            if allow_backup_recovery
                && Self::archive_corrupt_primary_and_restore_backup(app_data_dir)
            {
                return Self::load_inner(app_data_dir, false);
            }
            return Err(AppError::ConfigReadError(format!(
                "旧版配置字段 {invalid_key} 类型无效，已停止迁移以防数据丢失"
            )));
        }
        let legacy_overlay_enabled = value
            .get("enable_overlay")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_else(default_enable_overlay);
        let had_raw_legacy_paths = value.get("game_path").is_some()
            || value.get("saved_games_path").is_some()
            || value.get("battle_net_path").is_some();
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
        let mut pending_legacy_paths = match &region_path_migration {
            LegacyRegionPathMigration::Pending(candidate) => Some(candidate.clone()),
            _ => None,
        };
        let mut migrated = overlay_split_migrated
            || matches!(region_path_migration, LegacyRegionPathMigration::Migrated);
        if pending_legacy_paths.is_none() {
            let battle_net_migration = Self::migrate_legacy_battle_net_paths(&mut value);
            migrated |= battle_net_migration.changed;
            if let Some(battle_net_path) = battle_net_migration.pending_path {
                pending_legacy_paths = Some(LegacyPathMigration {
                    battle_net_path,
                    ..LegacyPathMigration::default()
                });
            }
        }
        let mut config: GlobalConfig = match serde_json::from_value(value) {
            Ok(config) => config,
            Err(_error)
                if allow_backup_recovery
                    && Self::archive_corrupt_primary_and_restore_backup(app_data_dir) =>
            {
                return Self::load_inner(app_data_dir, false);
            }
            Err(error) => return Err(error.into()),
        };
        let persisted_pending_paths = config.legacy_path_migration.take();
        config.legacy_path_migration = pending_legacy_paths.or({
            if had_raw_legacy_paths {
                None
            } else {
                persisted_pending_paths
            }
        });

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

        // 声纹目标依赖账号目录。必须先回滚中断的账号目录交换，再判断目标是否有效。
        recover_account_transactions(&config.accounts_dir);

        // 迁移：从未配置过快捷键的旧用户，自动写入默认值
        if config.shortcut_bindings_json.is_empty() {
            config.shortcut_bindings_json =
                r#"{"1":"Ctrl+1","2":"Ctrl+2","3":"Ctrl+3"}"#.to_string();
            migrated = true;
        }
        // 迁移：去除旧版本可能存在的 Win/Meta/Cmd 修饰键（v0.6.6 起不再支持）
        migrated |= Self::strip_win_modifiers(&mut config.shortcut_bindings_json);

        if config.normalize_rune_audio_configuration() {
            log::warn!("检测到无效的声纹目标配置，已自动关闭自动识别");
            migrated = true;
        }

        if config.normalize_launch_groups() {
            migrated = true;
        }

        if config.first_run_complete
            && config.cn_game_path.trim().is_empty()
            && config.global_game_path.trim().is_empty()
        {
            config.first_run_complete = false;
            migrated = true;
            log::warn!("配置标记为已完成但没有任何游戏目录，已要求重新确认设置");
        }

        if config.legacy_path_migration.is_some() {
            config.first_run_complete = false;
            log::warn!("旧版路径无法无歧义迁移，保留原始配置并等待用户确认归属");
        } else if migrated {
            config.save(app_data_dir)?;
        }
        Ok(config)
    }

    fn migrate_legacy_region_paths(value: &mut serde_json::Value) -> LegacyRegionPathMigration {
        let Some(object) = value.as_object_mut() else {
            return LegacyRegionPathMigration::NotNeeded;
        };
        if !object.contains_key("game_path") && !object.contains_key("saved_games_path") {
            return LegacyRegionPathMigration::NotNeeded;
        }

        let legacy_string = |key: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let game_path = legacy_string("game_path");
        let saved_games_path = legacy_string("saved_games_path");
        let battle_net_path = legacy_string("battle_net_path");

        if game_path.trim().is_empty() && saved_games_path.trim().is_empty() {
            object.remove("game_path");
            object.remove("saved_games_path");
            return LegacyRegionPathMigration::Migrated;
        }

        let Some(edition) = Self::infer_legacy_saved_games_edition(&saved_games_path) else {
            return LegacyRegionPathMigration::Pending(LegacyPathMigration {
                game_path,
                saved_games_path,
                battle_net_path,
            });
        };
        let is_cn = edition == crate::launch_context::ClientEdition::Cn;
        let (game_key, saves_key) = if is_cn {
            ("cn_game_path", "cn_saved_games_path")
        } else {
            ("global_game_path", "global_saved_games_path")
        };

        let conflicts_with_new_value = |key: &str, legacy: &str| {
            !legacy.trim().is_empty()
                && object
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|current| {
                        !current.trim().is_empty() && !current.eq_ignore_ascii_case(legacy)
                    })
        };
        if conflicts_with_new_value(game_key, &game_path)
            || conflicts_with_new_value(saves_key, &saved_games_path)
        {
            return LegacyRegionPathMigration::Pending(LegacyPathMigration {
                game_path,
                saved_games_path,
                battle_net_path,
            });
        }

        object.remove("game_path");
        object.remove("saved_games_path");
        for (key, legacy) in [(game_key, game_path), (saves_key, saved_games_path)] {
            let current_is_empty = object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .is_none_or(|current| current.trim().is_empty());
            if current_is_empty {
                object.insert(key.to_string(), serde_json::Value::String(legacy));
            }
        }
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

    fn migrate_legacy_battle_net_paths(
        value: &mut serde_json::Value,
    ) -> LegacyBattleNetPathMigration {
        let Some(object) = value.as_object_mut() else {
            return LegacyBattleNetPathMigration {
                changed: false,
                pending_path: None,
            };
        };
        let changed = object.remove("global_battle_net_path").is_some();
        if !object.contains_key("battle_net_path") {
            return LegacyBattleNetPathMigration {
                changed,
                pending_path: None,
            };
        }

        let battle_net_path = object
            .get("battle_net_path")
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default();
        if battle_net_path.trim().is_empty() {
            object.remove("battle_net_path");
            return LegacyBattleNetPathMigration {
                changed: true,
                pending_path: None,
            };
        }
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

        let current_cn_battle_net = object
            .get("cn_battle_net_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        if !current_cn_battle_net.trim().is_empty() {
            object.remove("battle_net_path");
            return LegacyBattleNetPathMigration {
                changed: true,
                pending_path: None,
            };
        }

        if cn_configured && !global_configured {
            object.remove("battle_net_path");
            object.insert(
                "cn_battle_net_path".to_string(),
                serde_json::Value::String(battle_net_path),
            );
            return LegacyBattleNetPathMigration {
                changed: true,
                pending_path: None,
            };
        }

        if global_configured && !cn_configured {
            // 国际服从 v4 起仅支持 Token，旧 Battle.net 路径不再参与启动。
            object.remove("battle_net_path");
            return LegacyBattleNetPathMigration {
                changed: true,
                pending_path: None,
            };
        }

        LegacyBattleNetPathMigration {
            changed,
            pending_path: Some(battle_net_path),
        }
    }

    /// 保存配置到磁盘
    pub fn save(&self, app_data_dir: &str) -> Result<(), AppError> {
        if self.legacy_path_migration.is_some() {
            return Err(AppError::ConfigWriteError(
                "旧版路径尚未确认归属，已阻止覆盖原配置".to_string(),
            ));
        }
        let dir = Path::new(app_data_dir);
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        }
        let path = Self::config_path(app_data_dir);
        let backup = Self::config_backup_path(app_data_dir);
        let staging = Self::config_staging_path(app_data_dir);
        let content = serde_json::to_string_pretty(self)?;
        if staging.exists() {
            std::fs::remove_file(&staging)
                .map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        }
        std::fs::write(&staging, content).map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&staging) {
            file.sync_all()
                .map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        }

        let had_primary = path.exists();
        if had_primary {
            if backup.exists() {
                std::fs::remove_file(&backup)
                    .map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
            }
            std::fs::rename(&path, &backup)
                .map_err(|e| AppError::ConfigWriteError(e.to_string()))?;
        }
        if let Err(error) = std::fs::rename(&staging, &path) {
            if had_primary {
                let _ = std::fs::rename(&backup, &path);
            }
            let _ = std::fs::remove_file(&staging);
            return Err(AppError::ConfigWriteError(error.to_string()));
        }
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
            let _config_io = state.config_io.lock();
            if let Some(loaded) = state.config.read().clone() {
                return Ok(loaded);
            }
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
) -> Result<GlobalConfig, AppError> {
    let _config_io = state.config_io.lock();
    if config.legacy_path_migration.is_some() {
        return Err(AppError::ConfigWriteError(
            "请先确认旧版路径属于国服还是国际服".to_string(),
        ));
    }
    let existing_config_artifact = GlobalConfig::config_path(&state.app_data_dir).exists()
        || GlobalConfig::config_backup_path(&state.app_data_dir).exists()
        || GlobalConfig::config_staging_path(&state.app_data_dir).exists();
    if state.config.read().is_none() && existing_config_artifact {
        return Err(AppError::ConfigWriteError(
            "现有配置未能安全加载，已阻止用新配置覆盖；请先检查日志和 global_config.json"
                .to_string(),
        ));
    }
    let previous = state.config.read().clone();
    let mut cfg = config.clone();
    cfg.version = CURRENT_CONFIG_VERSION;
    // 保留旧字段作为向后兼容总状态，新代码只读取两个独立开关。
    cfg.enable_overlay = cfg.enable_tz_overlay || cfg.enable_stats_overlay;
    cfg.accounts_dir = app_accounts_dir(&state.app_data_dir)
        .to_string_lossy()
        .to_string();
    cfg.rune_audio_tracked_categories =
        crate::rune_audio::item_catalog::normalize_tracked_categories(
            &cfg.rune_audio_tracked_categories,
        );
    cfg.rune_audio_min_rune_number = cfg.rune_audio_min_rune_number.clamp(1, 33);
    cfg.rune_audio_min_gem_level = cfg.rune_audio_min_gem_level.clamp(1, 5);
    cfg.rune_audio_tracked_charm_codes =
        crate::rune_audio::item_catalog::normalize_tracked_charm_codes(
            &cfg.rune_audio_tracked_charm_codes,
        );
    cfg.normalize_launch_groups();
    cfg.validate_launch_groups()?;

    if should_validate_installation_paths(previous.as_ref(), &cfg) {
        validate_installation_paths(&cfg)?;
    }
    cfg.resolve_rune_audio_target_account()?;

    cfg.save(&state.app_data_dir)?;
    cfg.ensure_dirs()?;
    let mut stored = state.config.write();
    *stored = Some(cfg.clone());
    update_shortcut_map(&state, &cfg);
    crate::input_listener::set_bongo_cat_input_enabled(cfg.enable_bongo_cat);
    Ok(cfg)
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
    let _config_io = state.config_io.lock();
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
