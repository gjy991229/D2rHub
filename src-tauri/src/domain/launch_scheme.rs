//! Recipes own account choices and launch differences; operational preferences are shared.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{
    config::{GlobalConfig, LaunchGroup},
    window_layout::WindowLayout,
};
use crate::{domain::room_automation::RoomAutomationConfig, error::AppError};

pub const RUNTIME_LAYOUT_ID: &str = "__launch_session_layout__";
pub const SETTING_KEYS: &[&str] = &["rune_audio_target_account", "keep_battle_net_account_id"];
pub const SCHEME_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SchemeRoomAccounts {
    pub primary_account_id: String,
    pub follower_account_ids: Vec<String>,
}

impl SchemeRoomAccounts {
    pub fn project(&self, common: &RoomAutomationConfig) -> RoomAutomationConfig {
        let mut room = common.clone();
        room.primary_account_id = self.primary_account_id.clone();
        room.follower_account_ids = self.follower_account_ids.clone();
        room.join_only = false;
        room
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemeEnvironment {
    #[serde(default)]
    pub schema_version: u32,
    pub settings: Map<String, Value>,
    pub room: SchemeRoomAccounts,
    pub layout: Option<WindowLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountLaunchSnapshot {
    /// Empty recipes inherit account defaults. Sessions resolve a complete map at launch.
    pub game_settings: Map<String, Value>,
    pub position: Option<(i32, i32)>,
    pub region: Option<String>,
    pub language: Option<String>,
    pub voicelanguage: Option<String>,
}

/// Provenance and immutable retry input. Current module settings remain in their
/// existing durable stores; they never write back into this recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchSession {
    pub id: String,
    pub scheme: LaunchGroup,
    pub started_at: String,
    #[serde(default)]
    pub processes: Vec<SessionProcess>,
    /// Working account settings. The recipe above is immutable provenance.
    #[serde(default)]
    pub members: Vec<super::config::LaunchGroupMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProcess {
    pub account_id: String,
    pub pid: u32,
    pub started_at: u64,
    pub executable: std::path::PathBuf,
    pub mod_args: String,
}

pub fn settings_snapshot(config: &GlobalConfig) -> Map<String, Value> {
    let all = serde_json::to_value(config).expect("serializable configuration");
    SETTING_KEYS
        .iter()
        .map(|key| ((*key).into(), all[*key].clone()))
        .collect()
}

/// Run on raw JSON before deserialization so a durable backup can retain every old value.
pub fn migrate_schemes(value: &mut Value) -> bool {
    let mut changed = false;
    if let Some(groups) = value.get_mut("launch_groups").and_then(Value::as_array_mut) {
        for group in groups {
            let Some(env) = group.get_mut("environment").and_then(Value::as_object_mut) else {
                continue;
            };
            if env
                .get("schema_version")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                >= u64::from(SCHEME_SCHEMA_VERSION)
            {
                continue;
            }
            if let Some(settings) = env.get_mut("settings").and_then(Value::as_object_mut) {
                if settings.get("rune_audio_enabled").and_then(Value::as_bool) == Some(false)
                    || settings
                        .get("rune_audio_external_target")
                        .is_some_and(|target| !target.is_null())
                {
                    settings.insert("rune_audio_target_account".into(), "".into());
                }
                settings.retain(|key, _| SETTING_KEYS.contains(&key.as_str()));
                settings
                    .entry("rune_audio_target_account")
                    .or_insert_with(|| "".into());
                settings
                    .entry("keep_battle_net_account_id")
                    .or_insert(Value::Null);
            }
            if let Some(room) = env.get_mut("room").and_then(Value::as_object_mut) {
                if room.get("enabled").and_then(Value::as_bool) == Some(false) {
                    room.insert("primary_account_id".into(), "".into());
                    room.insert("follower_account_ids".into(), Value::Array(vec![]));
                }
                room.retain(|key, _| {
                    matches!(key.as_str(), "primary_account_id" | "follower_account_ids")
                });
            }
            env.insert("schema_version".into(), SCHEME_SCHEMA_VERSION.into());
            // Previously these were implicit full copies, rather than explicit overrides.
            if let Some(members) = group.get_mut("members").and_then(Value::as_array_mut) {
                for member in members {
                    if let Some(snapshot) =
                        member.get_mut("snapshot").and_then(Value::as_object_mut)
                    {
                        snapshot.insert("game_settings".into(), Value::Object(Map::new()));
                    }
                }
            }
            changed = true;
        }
    }
    // Active sessions retain their already resolved settings until they finish.
    if let Some(session) = value
        .get_mut("launch_session")
        .and_then(Value::as_object_mut)
    {
        if let Some(scheme) = session.get_mut("scheme") {
            let mut wrapper = serde_json::json!({"launch_groups": [scheme.clone()]});
            if migrate_schemes(&mut wrapper) {
                *scheme = wrapper["launch_groups"][0].take();
                changed = true;
            }
        }
    }
    changed
}

/// The working copy lives beside library templates for compatibility with the
/// layout UI. Give only that copy a distinct display name, using the same
/// whitespace/case rules and 40-character bound as layout validation.
fn runtime_layout_name(source: &str, library: &[WindowLayout]) -> String {
    let occupied: std::collections::HashSet<_> = library
        .iter()
        .map(|layout| layout.name.trim().to_lowercase())
        .collect();
    for index in 1..=library.len() + 1 {
        let suffix = if index == 1 {
            "（本次运行）".to_string()
        } else {
            format!("（本次运行 {index}）")
        };
        let base: String = source
            .trim()
            .chars()
            .take(40 - suffix.chars().count())
            .collect();
        let candidate = format!("{}{suffix}", base.trim_end());
        if !occupied.contains(&candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!("a finite layout library cannot occupy every generated name")
}

impl SchemeEnvironment {
    pub fn project(&self, current: &GlobalConfig) -> Result<GlobalConfig, AppError> {
        if self.schema_version != SCHEME_SCHEMA_VERSION
            || self.settings.len() != SETTING_KEYS.len()
            || self
                .settings
                .keys()
                .any(|key| !SETTING_KEYS.contains(&key.as_str()))
        {
            return Err(AppError::ConfigReadError(
                "方案设置不完整，请编辑并保存方案".into(),
            ));
        }
        let mut value = serde_json::to_value(current)?;
        value.as_object_mut().unwrap().extend(self.settings.clone());
        let mut config: GlobalConfig = serde_json::from_value(value)?;
        config.rune_audio_external_target = None;
        config
            .window_layouts
            .retain(|layout| layout.id != RUNTIME_LAYOUT_ID);
        config.window_layout_enabled = self.layout.is_some();
        config.active_window_layout_id = self.layout.as_ref().map(|_| RUNTIME_LAYOUT_ID.into());
        if let Some(mut layout) = self.layout.clone() {
            // Do not hide invalid saved names by truncating them during projection.
            layout.validate()?;
            layout.id = RUNTIME_LAYOUT_ID.into();
            layout.name = runtime_layout_name(&layout.name, &config.window_layouts);
            config.window_layouts.push(layout);
        }
        // Validate the complete projected state before any module settings are
        // committed, not only the standalone layout held in the recipe.
        super::window_layout::validate_layout_configuration(
            &config.window_layouts,
            config.active_window_layout_id.as_deref(),
        )?;
        Ok(config)
    }

    pub fn validate(&self, group: &LaunchGroup, current: &GlobalConfig) -> Result<(), AppError> {
        let config = self.project(current)?;
        let contains = |id: &str| group.members.iter().any(|m| m.account_id == id);
        if !config.rune_audio_target_account.is_empty()
            && !contains(&config.rune_audio_target_account)
        {
            return Err(AppError::ConfigReadError("请选择方案内的识别账号".into()));
        }
        if config
            .keep_battle_net_account_id
            .as_deref()
            .is_some_and(|id| !contains(id))
        {
            return Err(AppError::ConfigReadError(
                "保留战网的账号必须属于本方案".into(),
            ));
        }
        {
            if (!self.room.primary_account_id.is_empty()
                && !contains(&self.room.primary_account_id))
                || self
                    .room
                    .follower_account_ids
                    .iter()
                    .any(|id| !contains(id))
            {
                return Err(AppError::ConfigReadError(
                    "跟房主号和小号必须属于本方案".into(),
                ));
            }
            let mut seen = std::collections::HashSet::new();
            for id in &self.room.follower_account_ids {
                if id == &self.room.primary_account_id || !seen.insert(id) {
                    return Err(AppError::ConfigReadError("跟随账号重复或与主号相同".into()));
                }
            }
        }
        if let Some(layout) = &self.layout {
            super::window_layout::validate_layout_configuration(
                std::slice::from_ref(layout),
                Some(&layout.id),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::window_layout::{
        validate_layout_configuration, LayoutMonitor, LayoutRect, LayoutSlot,
    };
    use serde_json::json;

    fn layout(id: &str, name: &str) -> WindowLayout {
        let bounds = LayoutRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        WindowLayout {
            id: id.into(),
            name: name.into(),
            monitors: vec![LayoutMonitor {
                id: "display".into(),
                name: "Display".into(),
                bounds,
                work_area: bounds,
                primary: true,
                scale_factor: 1.0,
                frame: None,
            }],
            windows: vec![LayoutSlot {
                monitor_id: "display".into(),
            account_id: None,
            window_title: None,
                fullscreen: false,
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            }],
        }
    }

    #[test]
    fn launching_a_copied_library_layout_keeps_both_layouts_valid_and_independent() {
        let template = layout("desk", "日常多开");
        let config = GlobalConfig {
            window_layouts: vec![template.clone()],
            ..Default::default()
        };
        let mut recipe = environment(&config);
        recipe.layout = Some(template.clone());
        let mut projected = recipe.project(&config).unwrap();
        validate_layout_configuration(
            &projected.window_layouts,
            projected.active_window_layout_id.as_deref(),
        )
        .unwrap();
        assert_eq!(projected.window_layouts[0], template);
        assert_eq!(
            projected.active_window_layout_id.as_deref(),
            Some(RUNTIME_LAYOUT_ID)
        );
        assert_ne!(projected.window_layouts[1].name, template.name);
        projected.window_layouts[1].windows[0].x = 100;
        assert_eq!(recipe.layout, Some(template.clone()));
        assert_eq!(
            config.window_layouts.as_slice(),
            std::slice::from_ref(&template)
        );

        let relaunched = recipe.project(&projected).unwrap();
        assert_eq!(relaunched.window_layouts.len(), 2);
        assert_eq!(relaunched.window_layouts[1].windows, template.windows);
        validate_layout_configuration(
            &relaunched.window_layouts,
            relaunched.active_window_layout_id.as_deref(),
        )
        .unwrap();
    }

    #[test]
    fn runtime_name_handles_case_whitespace_and_existing_numbered_names() {
        let config = GlobalConfig {
            window_layouts: vec![
                layout("one", " DESK（本次运行） "),
                layout("two", "Desk（本次运行 2）"),
            ],
            ..Default::default()
        };
        let mut recipe = environment(&config);
        recipe.layout = Some(layout("source", "desk"));
        let first = recipe.project(&config).unwrap();
        assert_eq!(
            first.window_layouts.last().unwrap().name,
            "desk（本次运行 3）"
        );
        let second = recipe.project(&first).unwrap();
        assert_eq!(first.window_layouts, second.window_layouts);
        recipe.layout = None;
        let disabled = recipe.project(&second).unwrap();
        assert_eq!(disabled.window_layouts, config.window_layouts);
        assert!(!disabled.window_layout_enabled);
        assert!(disabled.active_window_layout_id.is_none());
    }

    #[test]
    fn runtime_names_respect_unicode_length_without_changing_the_saved_name() {
        let name = "窗".repeat(40);
        let template = layout("long", &name);
        let config = GlobalConfig {
            window_layouts: vec![template.clone()],
            ..Default::default()
        };
        let mut recipe = environment(&config);
        recipe.layout = Some(template);
        let first = recipe.project(&config).unwrap();
        let mut collision = first.window_layouts.last().unwrap().clone();
        assert_eq!(collision.name.chars().count(), 40);
        collision.id = "another-template".into();
        let mut occupied = config.clone();
        occupied.window_layouts.push(collision);
        let projected = recipe.project(&occupied).unwrap();
        assert_eq!(
            projected
                .window_layouts
                .last()
                .unwrap()
                .name
                .chars()
                .count(),
            40
        );
        assert_eq!(recipe.layout.as_ref().unwrap().name, name);
        validate_layout_configuration(
            &projected.window_layouts,
            projected.active_window_layout_id.as_deref(),
        )
        .unwrap();
    }

    #[test]
    fn projection_rejects_invalid_library_before_any_settings_can_be_applied() {
        let config = GlobalConfig {
            window_layouts: vec![layout("one", "duplicate"), layout("two", " DUPLICATE ")],
            ..Default::default()
        };
        assert!(environment(&config).project(&config).is_err());
        let valid = GlobalConfig::default();
        let mut recipe = environment(&valid);
        recipe.layout = Some(layout("source", &"窗".repeat(41)));
        assert!(recipe.project(&valid).is_err());
    }

    fn environment(config: &GlobalConfig) -> SchemeEnvironment {
        SchemeEnvironment {
            schema_version: SCHEME_SCHEMA_VERSION,
            settings: settings_snapshot(config),
            room: Default::default(),
            layout: None,
        }
    }

    #[test]
    fn repeated_launch_uses_account_choices_and_latest_common_preferences() {
        let defaults = GlobalConfig {
            rune_audio_target_account: "a".into(),
            ..Default::default()
        };
        let recipe = environment(&defaults);
        let mut runtime = recipe.project(&defaults).unwrap();
        runtime.rune_audio_target_account = "b".into();
        runtime.rune_audio_tracked_categories.clear();
        let next_launch = recipe.project(&runtime).unwrap();
        assert_eq!(next_launch.rune_audio_target_account, "a");
        assert_eq!(
            next_launch.rune_audio_tracked_categories,
            runtime.rune_audio_tracked_categories
        );
        assert_eq!(recipe.settings["rune_audio_target_account"], "a");
    }

    #[test]
    fn projection_preserves_public_resources_and_other_schemes() {
        let mut config = GlobalConfig {
            theme: "forest".into(),
            accounts_dir: "accounts".into(),
            ..Default::default()
        };
        config.launch_groups.push(LaunchGroup {
            id: "other".into(),
            name: "Other".into(),
            ..Default::default()
        });
        let projected = environment(&GlobalConfig::default())
            .project(&config)
            .unwrap();
        assert_eq!(projected.theme, config.theme);
        assert_eq!(projected.accounts_dir, config.accounts_dir);
        assert_eq!(projected.launch_groups, config.launch_groups);
    }

    #[test]
    fn missing_or_injected_fields_fail_closed() {
        let config = GlobalConfig::default();
        let mut recipe = environment(&config);
        recipe.settings.remove("rune_audio_target_account");
        assert!(recipe.project(&config).is_err());
        recipe
            .settings
            .insert("accounts_dir".into(), json!("unexpected"));
        assert!(recipe.project(&config).is_err());
    }

    #[test]
    fn recognition_and_room_targets_must_belong_to_recipe() {
        let config = GlobalConfig::default();
        let mut recipe = environment(&config);
        let scheme = LaunchGroup::default();
        recipe
            .settings
            .insert("rune_audio_target_account".into(), json!("outside"));
        assert!(recipe.validate(&scheme, &config).is_err());
        recipe
            .settings
            .insert("rune_audio_target_account".into(), json!(""));
        recipe.room.primary_account_id = "outside".into();
        assert!(recipe.validate(&scheme, &config).is_err());
    }

    #[test]
    fn launching_preserves_all_public_preferences_even_with_no_accounts_selected() {
        let mut common = GlobalConfig::default();
        let recipe = environment(&common);
        common.rune_audio_enabled = true;
        common.rune_audio_detection_threshold = 0.73;
        common.rune_audio_min_rune_number = 24;
        common.auto_close_browser = false;
        common.agent_delay_secs = 4.5;
        common.enable_stats_overlay = true;
        let projected = recipe.project(&common).unwrap();
        assert!(projected.rune_audio_enabled);
        assert!(projected.rune_audio_target_account.is_empty());
        assert_eq!(projected.rune_audio_detection_threshold, 0.73);
        assert_eq!(projected.rune_audio_min_rune_number, 24);
        assert_eq!(projected.auto_close_browser, common.auto_close_browser);
        assert_eq!(projected.agent_delay_secs, common.agent_delay_secs);
        assert!(projected.enable_stats_overlay);
    }

    #[test]
    fn room_selection_preserves_public_rules_timing_switch_and_live_sequence() {
        let common = RoomAutomationConfig {
            enabled: true,
            next_sequence: 42,
            auto_followers_delay_secs: 6.5,
            shortcut: "Ctrl+F9".into(),
            ..Default::default()
        };
        let empty = SchemeRoomAccounts::default().project(&common);
        assert!(empty.enabled);
        assert!(!empty.runtime_enabled());
        empty.validate(std::iter::empty::<&str>()).unwrap();
        let selected = SchemeRoomAccounts {
            primary_account_id: "a".into(),
            follower_account_ids: vec!["c".into(), "b".into()],
        }
        .project(&empty);
        assert!(selected.runtime_enabled());
        assert_eq!(selected.next_sequence, 42);
        assert_eq!(selected.auto_followers_delay_secs, 6.5);
        assert_eq!(selected.shortcut, "Ctrl+F9");
        assert_eq!(selected.flow, common.flow);
        assert_eq!(selected.follower_account_ids, ["c", "b"]);
    }

    #[test]
    fn migration_removes_copied_parameters_but_preserves_active_session_settings() {
        let old = json!({
            "id": "one", "name": "daily", "account_ids": ["a"],
            "members": [{"account_id": "a", "snapshot": {"game_settings": {"Quality": 1}}}],
            "environment": {
                "settings": {"rune_audio_enabled": true, "rune_audio_target_account": "a", "agent_delay_secs": 9},
                "room": {"enabled": true, "primary_account_id": "a", "follower_account_ids": ["b"], "next_sequence": 1, "shortcut": "F9"},
                "layout": null
            }
        });
        let mut value = json!({"launch_groups": [old.clone()], "launch_session": {"scheme": old, "members": [{"snapshot": {"game_settings": {"Quality": 3}}}]}});
        assert!(migrate_schemes(&mut value));
        let env: SchemeEnvironment =
            serde_json::from_value(value["launch_groups"][0]["environment"].clone()).unwrap();
        assert_eq!(env.settings.len(), 2);
        assert_eq!(env.settings["rune_audio_target_account"], "a");
        assert_eq!(env.room.follower_account_ids, ["b"]);
        assert_eq!(
            value["launch_groups"][0]["members"][0]["snapshot"]["game_settings"],
            json!({})
        );
        assert_eq!(
            value["launch_session"]["members"][0]["snapshot"]["game_settings"]["Quality"],
            3
        );
        assert!(!migrate_schemes(&mut value));
    }

    #[test]
    fn migration_keeps_disabled_features_unselected() {
        let mut value = json!({"launch_groups": [{"environment": {
            "settings": {"rune_audio_enabled": false, "rune_audio_target_account": "a"},
            "room": {"enabled": false, "primary_account_id": "a", "follower_account_ids": ["b"]}, "layout": null
        }}]});
        assert!(migrate_schemes(&mut value));
        let env: SchemeEnvironment =
            serde_json::from_value(value["launch_groups"][0]["environment"].clone()).unwrap();
        assert_eq!(env.settings["rune_audio_target_account"], "");
        assert!(env.room.primary_account_id.is_empty());
        assert!(env.room.follower_account_ids.is_empty());
    }
}
