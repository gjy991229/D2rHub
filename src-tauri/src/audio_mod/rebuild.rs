//! Recover a recipe, never old generated assets, when rebuilding a Mod.
use crate::domain::mod_processing::{
    RequestedFeatureGroups, AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
    AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION, IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
};

#[derive(Debug, Clone)]
pub(super) struct AudioOptions {
    pub areas: String,
    pub track: String,
    pub gain: f32,
}

impl Default for AudioOptions {
    fn default() -> Self {
        Self {
            areas: "all".into(),
            track: "all".into(),
            gain: -30.0,
        }
    }
}

pub(super) fn recipe(
    document: &serde_json::Value,
    requested: RequestedFeatureGroups,
) -> Result<(RequestedFeatureGroups, AudioOptions), String> {
    if document["recipe_version"].as_u64().is_some_and(|version| {
        version > super::validation::REQUIRED_AUDIO_MOD_RECIPE_VERSION as u64
    }) {
        return Err("旧 Mod 使用更新的加工配方，请升级 Hub 后重做，旧 Mod 未修改".into());
    }
    let groups = crate::domain::mod_processing::parse_feature_groups(document)?;
    for group in &groups {
        let supported = match group.id.as_str() {
            "audio_telemetry" => AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
            "in_game_room_tools" => IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
            "esc_next_game" => 4,
            "scene_shortcuts" => 7,
            "party_loot" => 3,
            "hostile_auto_restart" => 1,
            "auto_exit_on_death" => AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION,
            _ => {
                return Err(format!(
                    "无法重做未知模块 {}；请使用支持该模块的 Hub，旧 Mod 未修改",
                    group.id
                ))
            }
        };
        if group.recipe_version > supported {
            return Err(format!(
                "模块 {} 的配方 r{} 高于当前支持的 r{}，请升级 Hub 后重做，旧 Mod 未修改",
                group.id, group.recipe_version, supported
            ));
        }
    }
    let mut features = requested.include_existing_known(&groups);
    if groups.is_empty() && document["recipe_version"].as_u64().unwrap_or(0) < 22 {
        features.audio_telemetry = true;
    }
    Ok((features, audio_options(document)?))
}

pub(super) fn audio_options(document: &serde_json::Value) -> Result<AudioOptions, String> {
    let groups = crate::domain::mod_processing::parse_feature_groups(document)?;
    let mut options = AudioOptions::default();
    if let Some(group) = groups.iter().find(|g| g.id == "audio_telemetry") {
        let parts: Vec<_> = group.fingerprint.split(';').collect();
        let field = |key: &str| parts.iter().find_map(|part| part.strip_prefix(key));
        options.areas = match field("areas=") {
            Some("all_areas") => "all",
            Some("none") => "none",
            Some("countess_route") => "countess",
            _ => return Err("旧声纹模块缺少可恢复的覆盖范围".into()),
        }
        .into();
        let track = field("track=").ok_or("旧声纹模块缺少追踪类别")?;
        if !track.is_empty()
            && track.split(',').any(|s| {
                ![
                    "runes", "gems", "charms", "jewels", "keys", "organs", "essences",
                ]
                .contains(&s)
            })
        {
            return Err("旧声纹模块包含未知追踪类别".into());
        }
        options.track = if track.is_empty() { "none" } else { track }.into();
        options.gain = field("gain_mdb=")
            .ok_or("旧声纹模块缺少增益")?
            .parse::<f32>()
            .map_err(|_| "旧声纹模块增益无效")?
            / 1000.0;
        if !options.gain.is_finite() || !(-42.0..=-12.0).contains(&options.gain) {
            return Err("旧声纹模块增益超出支持范围".into());
        }
    } else if document["area_coverage"] == "countess_route" {
        options.areas = "countess".into();
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebuild_does_not_downgrade_future_known_recipes() {
        let request = RequestedFeatureGroups::from_options(
            Some(false),
            Some(true),
            Some(false),
            None,
            None,
            None,
            Some(false),
        )
        .unwrap();
        for id in [
            "audio_telemetry",
            "in_game_room_tools",
            "esc_next_game",
            "auto_exit_on_death",
        ] {
            let doc = serde_json::json!({"feature_groups":[{"id":id,"recipe_version":999,"fingerprint":"future-settings"}]});
            assert!(recipe(&doc, request).unwrap_err().contains("请升级 Hub"));
        }
        assert!(recipe(&serde_json::json!({"recipe_version":999}), request).is_err());
    }
    #[test]
    fn rebuild_keeps_old_audio_options_across_protocol_changes() {
        let request = RequestedFeatureGroups::from_options(
            Some(false),
            Some(true),
            Some(false),
            None,
            None,
            None,
            Some(false),
        )
        .unwrap();
        let doc = serde_json::json!({"feature_groups":[{"id":"audio_telemetry","recipe_version":3,
            "fingerprint":"audio-v3;protocol=6;areas=countess_route;track=gems,runes;gain_mdb=-24000"}]});
        let (features, options) = recipe(&doc, request).unwrap();
        assert!(features.audio_telemetry && features.room_tools);
        assert_eq!(
            (options.areas.as_str(), options.track.as_str(), options.gain),
            ("countess", "gems,runes", -24.0)
        );
        let unknown = serde_json::json!({"feature_groups":[{"id":"future","recipe_version":1,"fingerprint":"future-v1"}]});
        assert!(recipe(&unknown, request).is_err());
    }
}
