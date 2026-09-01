use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Current on-disk configuration schema version.
///
/// The command/application layer owns migrations, while this domain module owns
/// the stable serialized shape and defaults.
pub(crate) const CURRENT_CONFIG_VERSION: u32 = 9;

/// These values are persistence defaults, not runtime feature registration.
/// Keeping them in the schema layer prevents the core configuration model from
/// depending on the optional audio telemetry implementation.
pub(crate) const DEFAULT_RUNE_AUDIO_TRACKED_CATEGORIES: [&str; 7] = [
    "runes", "gems", "charms", "jewels", "keys", "organs", "essences",
];
pub(crate) const DEFAULT_RUNE_AUDIO_TRACKED_CHARM_CODES: [&str; 3] = ["cm1", "cm2", "cm3"];

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
    /// 区分 v7 及更早方案的“继承账号画质”与 v8 明确保存的独立画质。
    #[serde(default)]
    pub graphics_configured: bool,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub fps: Option<u32>,
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

/// Stable global configuration schema.
///
/// Fields intentionally remain flat for on-disk compatibility. Optional
/// features may interpret their own fields, but the core can deserialize an old
/// configuration without loading those feature implementations.
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
    /// 固定在主界面操作栏上的常用启动方案，顺序即展示顺序。
    #[serde(default)]
    pub favorite_launch_group_ids: Vec<String>,
    /// Same-version fields owned by a branch or optional module that this build
    /// does not understand yet. Flattening keeps unrelated saves lossless until
    /// the owning module can import them into its versioned sidecar.
    #[serde(default, flatten)]
    pub(crate) preserved_unknown_fields: BTreeMap<String, Value>,
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
    DEFAULT_RUNE_AUDIO_TRACKED_CATEGORIES
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn default_rune_audio_min_rune_number() -> u32 {
    1
}

fn default_rune_audio_min_gem_level() -> u32 {
    1
}

fn default_rune_audio_tracked_charm_codes() -> Vec<String> {
    DEFAULT_RUNE_AUDIO_TRACKED_CHARM_CODES
        .into_iter()
        .map(str::to_string)
        .collect()
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

pub(crate) fn default_enable_overlay() -> bool {
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
            favorite_launch_group_ids: Vec::new(),
            preserved_unknown_fields: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GlobalConfig, CURRENT_CONFIG_VERSION};

    #[test]
    fn legacy_minimal_shape_accepts_missing_optional_fields_and_unknown_fields() {
        let legacy = serde_json::json!({
            "version": 1,
            "program_data_agent_path": "C:\\ProgramData\\Battle.net\\Agent",
            "app_data_roaming_bnet_path": "C:\\Users\\tester\\AppData\\Roaming\\Battle.net",
            "accounts_dir": "C:\\D2RHub\\accounts",
            "first_run_complete": true,
            "game_path": "D:\\Games\\Diablo II Resurrected",
            "future_optional_module": { "enabled": true }
        });

        let config: GlobalConfig = serde_json::from_value(legacy)
            .expect("old config with unknown fields must remain readable");

        assert_eq!(config.version, 1);
        assert!(config.browser_path.is_empty());
        assert!(config.enable_overlay);
        assert!(config.enable_tz_overlay);
        assert!(config.enable_stats_overlay);
        assert_eq!(config.theme, "light");
        assert_eq!(
            config.rune_audio_tracked_categories,
            ["runes", "gems", "charms", "jewels", "keys", "organs", "essences"]
        );
        assert_eq!(config.rune_audio_tracked_charm_codes, ["cm1", "cm2", "cm3"]);
        assert_eq!(
            config.preserved_unknown_fields["future_optional_module"],
            serde_json::json!({ "enabled": true })
        );
    }

    #[test]
    fn schema_round_trip_is_idempotent_after_defaults_are_materialized() {
        let legacy = serde_json::json!({
            "version": CURRENT_CONFIG_VERSION,
            "program_data_agent_path": "agent",
            "app_data_roaming_bnet_path": "roaming",
            "accounts_dir": "accounts",
            "first_run_complete": false,
            "enable_overlay": false,
            "unknown_legacy_marker": "ignored"
        });

        let first: GlobalConfig = serde_json::from_value(legacy).unwrap();
        let first_serialized = serde_json::to_value(&first).unwrap();
        let second: GlobalConfig = serde_json::from_value(first_serialized.clone()).unwrap();
        let second_serialized = serde_json::to_value(&second).unwrap();

        assert_eq!(first_serialized, second_serialized);
        assert_eq!(first_serialized["unknown_legacy_marker"], "ignored");
        assert!(first_serialized.get("legacy_path_migration").is_none());
    }
}
