//! Pure contract for features produced by the external Mod processor.
//!
//! Owns feature identity, supported recipes, requested-feature selection and
//! lossless upgrade compatibility. The host supplies its audio protocol version;
//! no filesystem, account, Tauri or telemetry runtime code belongs here.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub(crate) const AUDIO_TELEMETRY_FEATURE_ID: &str = "audio_telemetry";
pub(crate) const AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION: u32 = 5;
pub(crate) const IN_GAME_ROOM_TOOLS_FEATURE_ID: &str = "in_game_room_tools";
pub(crate) const IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION: u32 = 33;
pub(crate) const PARTY_LOOT_FEATURE_ID: &str = "party_loot";
pub(crate) const PARTY_LOOT_FINGERPRINT: &str =
    "party-loot-v4;native-party-row;portrait-scale=1.6;offset=-0.8,2.4;launch=immediate";
const V3_PARTY_LOOT_FINGERPRINT: &str =
    "party-loot-v3;native-party-row;portrait-position;launch=immediate";
const V2_PARTY_LOOT_FINGERPRINT: &str = "party-loot-v2;native-party-row;jcy=1;launch=immediate";
const LEGACY_PARTY_LOOT_FINGERPRINT: &str = "party-loot-v1;native-party-row;x=80;y=20";
pub(crate) const SCENE_SHORTCUTS_FEATURE_ID: &str = "scene_shortcuts";
const LEGACY_SCENE_SHORTCUTS_FINGERPRINT: &str =
    "scene-shortcuts-v1;mymod-wp;select_ms=100;restart_ms=200;cleanup_ms=300;tab=2;help=1";
const V2_SCENE_SHORTCUTS_FINGERPRINT: &str = "scene-shortcuts-v2;mymod-travincal;select_ms=100;restart_ms=200;cleanup_ms=300;tabs=3;tab=2;help=1;launch=immediate";
const V3_SCENE_SHORTCUTS_FINGERPRINT: &str = "scene-shortcuts-v3;mymod-two-stage;restart_tabs=4;arrival_tabs=3;tab=2;arrival_ms=1000;launch=immediate";
const V4_SCENE_SHORTCUTS_FINGERPRINT: &str = "scene-shortcuts-v4;mymod-two-stage;restart_tabs=4;arrival_tabs=3;tab=2;arrival_ms=1000;launch=immediate;help_only=1";
const V5_SCENE_SHORTCUTS_FINGERPRINT: &str = "scene-shortcuts-v5;mymod-hud-direct;restart_tabs=4;arrival_tabs=3;tab=2;arrival_ms=1000;help_only=1";
const V6_SCENE_SHORTCUTS_FINGERPRINT: &str =
    "scene-shortcuts-v6;mymod-exact-chain;hud=unload-then-arrival;arrival_ms=1000;help_only=1";
pub(crate) const SCENE_SHORTCUTS_FINGERPRINT: &str =
    "scene-shortcuts-v7;mymod-exact-chain;actinfo=a3-a4-waypoints;hud=unload-then-arrival;arrival_ms=1000;help_only=1";
pub(crate) const ESC_NEXT_GAME_FEATURE_ID: &str = "esc_next_game";
pub(crate) const ESC_NEXT_GAME_FINGERPRINT: &str =
    "esc-next-game-v4;window_ms=500;pause_timeout=1;hud_cleanup=0";
const LEGACY_ESC_NEXT_GAME_FINGERPRINT: &str = "esc-next-game-v2;window_ms=500;pause_timeout=1";
pub(crate) const PREVIOUS_IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSIONS: [u32; 12] =
    [21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32];
pub(crate) const AUTO_EXIT_ON_DEATH_FEATURE_ID: &str = "auto_exit_on_death";
pub(crate) const AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION: u32 = 1;
pub(crate) const AUTO_EXIT_ON_DEATH_FINGERPRINT: &str =
    "auto-exit-on-death-v1;trigger_ms=10;commit_ms=100";
pub(crate) const AUTO_EXIT_ON_DEATH_LEGACY_ENABLED_FINGERPRINT: &str =
    "auto-exit-on-death-v1;trigger_ms=10;commit_ms=100;enabled=1";
pub(crate) const AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT: &str =
    "auto-exit-on-death-v1;trigger_ms=10;commit_ms=100;enabled=0";

/// Persisted processor metadata. Unknown feature groups retain their identity
/// during additive upgrades even when this Hub cannot interpret the feature.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GeneratorFeatureGroup {
    pub id: String,
    pub recipe_version: u32,
    pub fingerprint: String,
    #[serde(default)]
    pub reused_from_source: bool,
}

/// Completion payload shared by the process adapter and the disk validator.
/// Receiving this report does not establish that its output is trusted.
#[derive(Debug, Deserialize)]
pub(crate) struct GeneratorReport {
    pub(crate) protocol_version: u8,
    pub(crate) recipe_version: u32,
    pub(crate) mod_name: String,
    pub(crate) mod_directory: String,
    #[serde(default)]
    pub(crate) feature_groups: Vec<GeneratorFeatureGroup>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RequestedFeatureGroups {
    pub(crate) audio_telemetry: bool,
    pub(crate) room_tools: bool,
    pub(crate) esc_next_game: bool,
    pub(crate) scene_shortcuts: bool,
    pub(crate) party_loot: bool,
    pub(crate) hostile_auto_restart: bool,
    pub(crate) auto_exit_on_death: bool,
}

impl RequestedFeatureGroups {
    pub(crate) fn from_options(
        audio_telemetry: Option<bool>,
        room_tools: Option<bool>,
        esc_next_game: Option<bool>,
        scene_shortcuts: Option<bool>,
        party_loot: Option<bool>,
        hostile_auto_restart: Option<bool>,
        auto_exit_on_death: Option<bool>,
    ) -> Result<Self, String> {
        // Missing fields preserve the pre-r22 command contract used by older D2RHub frontends.
        let requested = Self {
            audio_telemetry: audio_telemetry.unwrap_or(true),
            room_tools: room_tools.unwrap_or(false),
            esc_next_game: esc_next_game.unwrap_or(false) || hostile_auto_restart.unwrap_or(false),
            scene_shortcuts: scene_shortcuts.unwrap_or(false),
            party_loot: party_loot.unwrap_or(false),
            hostile_auto_restart: hostile_auto_restart.unwrap_or(false),
            auto_exit_on_death: auto_exit_on_death.unwrap_or(false),
        };
        if !requested.audio_telemetry
            && !requested.room_tools
            && !requested.party_loot
            && !requested.scene_shortcuts
            && !requested.esc_next_game
            && !requested.auto_exit_on_death
        {
            return Err("请至少选择一个要加工的功能".to_string());
        }
        Ok(requested)
    }

    pub(crate) fn generator_value(self) -> String {
        let mut features = Vec::new();
        if self.audio_telemetry {
            features.push("audio");
        }
        if self.room_tools {
            features.push("rooms");
        }
        if self.hostile_auto_restart {
            features.push("hostile-restart");
        }
        if self.party_loot {
            features.push("party-loot");
        }
        if self.scene_shortcuts {
            features.push("scene-shortcuts");
        }
        if self.esc_next_game {
            features.push("esc-next-game");
        }
        if self.auto_exit_on_death {
            features.push("death-exit");
        }
        features.join(",")
    }

    pub(crate) fn validate_present(
        self,
        groups: &[GeneratorFeatureGroup],
        audio_protocol_version: u8,
    ) -> Result<(), String> {
        for (requested, id, label) in [
            (
                self.hostile_auto_restart,
                "hostile_auto_restart",
                "敌对自动重开",
            ),
            (
                self.party_loot,
                PARTY_LOOT_FEATURE_ID,
                "左侧玩家尸体拾取按钮",
            ),
            (self.scene_shortcuts, SCENE_SHORTCUTS_FEATURE_ID, "场景重开"),
            (self.audio_telemetry, AUDIO_TELEMETRY_FEATURE_ID, "声纹识别"),
            (
                self.esc_next_game,
                ESC_NEXT_GAME_FEATURE_ID,
                "双击 Esc 下一局地狱",
            ),
            (
                self.room_tools,
                IN_GAME_ROOM_TOOLS_FEATURE_ID,
                "局内房间工具",
            ),
            (
                self.auto_exit_on_death,
                AUTO_EXIT_ON_DEATH_FEATURE_ID,
                "死亡后自动退出",
            ),
        ] {
            if requested {
                let group = groups
                    .iter()
                    .find(|group| group.id == id)
                    .ok_or_else(|| format!("生成结果缺少已选择的{label}功能组"))?;
                validate_supported_feature_group(group, audio_protocol_version)
                    .map_err(|error| format!("生成结果中的{label}功能组无效：{error}"))?;
            }
        }
        Ok(())
    }

    pub(crate) fn include_existing_known(mut self, groups: &[GeneratorFeatureGroup]) -> Self {
        self.audio_telemetry |= groups
            .iter()
            .any(|group| group.id == AUDIO_TELEMETRY_FEATURE_ID);
        self.room_tools |= groups
            .iter()
            .any(|group| group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID);
        self.esc_next_game |= groups
            .iter()
            .any(|group| group.id == ESC_NEXT_GAME_FEATURE_ID);
        self.hostile_auto_restart |= groups.iter().any(|g| g.id == "hostile_auto_restart");
        self.party_loot |= groups.iter().any(|group| group.id == PARTY_LOOT_FEATURE_ID);
        self.scene_shortcuts |= groups
            .iter()
            .any(|group| group.id == SCENE_SHORTCUTS_FEATURE_ID);
        self.auto_exit_on_death |= groups
            .iter()
            .any(|group| group.id == AUTO_EXIT_ON_DEATH_FEATURE_ID);
        self
    }

    pub(crate) fn all_present(self, groups: &[GeneratorFeatureGroup]) -> bool {
        (!self.hostile_auto_restart || groups.iter().any(|g| g.id == "hostile_auto_restart"))
            && (!self.audio_telemetry
                || groups
                    .iter()
                    .any(|group| group.id == AUDIO_TELEMETRY_FEATURE_ID))
            && (!self.room_tools
                || groups
                    .iter()
                    .any(|group| group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID))
            && (!self.esc_next_game
                || groups
                    .iter()
                    .any(|group| group.id == ESC_NEXT_GAME_FEATURE_ID))
            && (!self.party_loot || groups.iter().any(|group| group.id == PARTY_LOOT_FEATURE_ID))
            && (!self.scene_shortcuts
                || groups
                    .iter()
                    .any(|group| group.id == SCENE_SHORTCUTS_FEATURE_ID))
            && (!self.auto_exit_on_death
                || groups
                    .iter()
                    .any(|group| group.id == AUTO_EXIT_ON_DEATH_FEATURE_ID))
    }
}

pub(crate) fn parse_feature_groups(
    manifest: &serde_json::Value,
) -> Result<Vec<GeneratorFeatureGroup>, String> {
    let groups = match manifest.get("feature_groups") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(value) => serde_json::from_value::<Vec<GeneratorFeatureGroup>>(value.clone())
            .map_err(|_| "D2RHub Mod 清单的功能组信息无效，请重新加工".to_string())?,
    };
    validate_feature_group_metadata(&groups)?;
    Ok(groups)
}

fn validate_feature_group_metadata(groups: &[GeneratorFeatureGroup]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for group in groups {
        if group.id.is_empty()
            || group.id.len() > 128
            || !group.id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
        {
            return Err("D2RHub Mod 清单包含无效的功能组标识，请重新加工".to_string());
        }
        if !ids.insert(group.id.as_str()) {
            return Err(format!("D2RHub Mod 清单重复声明功能组：{}", group.id));
        }
        if group.recipe_version == 0
            || group.fingerprint.trim().is_empty()
            || group.fingerprint.len() > 4_096
        {
            return Err(format!("D2RHub Mod 功能组元数据无效：{}", group.id));
        }
    }
    Ok(())
}

pub(crate) fn validate_feature_group_entries(
    groups: &[GeneratorFeatureGroup],
    audio_protocol_version: u8,
) -> Result<(), String> {
    validate_feature_group_metadata(groups)?;
    for group in groups {
        validate_supported_feature_group(group, audio_protocol_version)?;
    }
    Ok(())
}

pub(crate) fn validate_upgrade_source_feature_group_entries(
    groups: &[GeneratorFeatureGroup],
    audio_protocol_version: u8,
) -> Result<(), String> {
    validate_feature_group_metadata(groups)?;
    for group in groups {
        if group.id == IN_GAME_ROOM_TOOLS_FEATURE_ID
            && PREVIOUS_IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSIONS.contains(&group.recipe_version)
        {
            if group.fingerprint != format!("room-tools-v{}", group.recipe_version) {
                return Err("上一版局内房间工具指纹无效，不能作为原位升级来源".to_string());
            }
        } else if group.id == SCENE_SHORTCUTS_FEATURE_ID && matches!(group.recipe_version, 1..=6) {
            if group.fingerprint
                != if group.recipe_version == 1 {
                    LEGACY_SCENE_SHORTCUTS_FINGERPRINT
                } else if group.recipe_version == 2 {
                    V2_SCENE_SHORTCUTS_FINGERPRINT
                } else if group.recipe_version == 3 {
                    V3_SCENE_SHORTCUTS_FINGERPRINT
                } else if group.recipe_version == 4 {
                    V4_SCENE_SHORTCUTS_FINGERPRINT
                } else if group.recipe_version == 5 {
                    V5_SCENE_SHORTCUTS_FINGERPRINT
                } else {
                    V6_SCENE_SHORTCUTS_FINGERPRINT
                }
            {
                return Err("旧版场景重开指纹无效".into());
            }
        } else if group.id == PARTY_LOOT_FEATURE_ID && matches!(group.recipe_version, 1..=3) {
            if group.fingerprint
                != if group.recipe_version == 1 {
                    LEGACY_PARTY_LOOT_FINGERPRINT
                } else if group.recipe_version == 2 {
                    V2_PARTY_LOOT_FINGERPRINT
                } else {
                    V3_PARTY_LOOT_FINGERPRINT
                }
            {
                return Err("旧版拾取按钮指纹无效".into());
            }
        } else if group.id == ESC_NEXT_GAME_FEATURE_ID && matches!(group.recipe_version, 2 | 3) {
            let expected = if group.recipe_version == 2 {
                LEGACY_ESC_NEXT_GAME_FINGERPRINT
            } else {
                "esc-next-game-v3;window_ms=500;pause_timeout=1;hud_cleanup=0"
            };
            if group.fingerprint != expected {
                return Err("上一版双击 Esc 功能组指纹无效，不能作为升级来源".to_string());
            }
        } else {
            validate_supported_feature_group(group, audio_protocol_version)?;
        }
    }
    Ok(())
}

pub(crate) fn validate_preserved_feature_groups(
    existing: &[GeneratorFeatureGroup],
    candidate: &[GeneratorFeatureGroup],
) -> Result<(), String> {
    for required in existing {
        let preserved = candidate.iter().any(|actual| {
            if required.id == SCENE_SHORTCUTS_FEATURE_ID
                && matches!(required.recipe_version, 1..=6)
                && required.fingerprint
                    == if required.recipe_version == 1 {
                        LEGACY_SCENE_SHORTCUTS_FINGERPRINT
                    } else if required.recipe_version == 2 {
                        V2_SCENE_SHORTCUTS_FINGERPRINT
                    } else if required.recipe_version == 3 {
                        V3_SCENE_SHORTCUTS_FINGERPRINT
                    } else if required.recipe_version == 4 {
                        V4_SCENE_SHORTCUTS_FINGERPRINT
                    } else if required.recipe_version == 5 {
                        V5_SCENE_SHORTCUTS_FINGERPRINT
                    } else {
                        V6_SCENE_SHORTCUTS_FINGERPRINT
                    }
                && actual.id == SCENE_SHORTCUTS_FEATURE_ID
                && actual.recipe_version == 7
                && actual.fingerprint == SCENE_SHORTCUTS_FINGERPRINT
            {
                return true;
            }

            if required.id == PARTY_LOOT_FEATURE_ID
                && matches!(required.recipe_version, 1..=3)
                && required.fingerprint
                    == if required.recipe_version == 1 {
                        LEGACY_PARTY_LOOT_FINGERPRINT
                    } else if required.recipe_version == 2 {
                        V2_PARTY_LOOT_FINGERPRINT
                    } else {
                        V3_PARTY_LOOT_FINGERPRINT
                    }
                && actual.id == PARTY_LOOT_FEATURE_ID
                && actual.recipe_version == 4
                && actual.fingerprint == PARTY_LOOT_FINGERPRINT
            {
                return true;
            }
            if required.id == ESC_NEXT_GAME_FEATURE_ID
                && required.recipe_version == 2
                && required.fingerprint == LEGACY_ESC_NEXT_GAME_FINGERPRINT
                && actual.id == ESC_NEXT_GAME_FEATURE_ID
                && actual.recipe_version == 4
                && actual.fingerprint == ESC_NEXT_GAME_FINGERPRINT
            {
                return true;
            }
            actual.id == required.id
                && actual.recipe_version == required.recipe_version
                && (actual.fingerprint == required.fingerprint
                    // Normalize the short-lived stateful r1 fingerprints once. This is a metadata
                    // migration during additive processing, not an activation toggle.
                    || (required.id == AUTO_EXIT_ON_DEATH_FEATURE_ID
                        && actual.fingerprint == AUTO_EXIT_ON_DEATH_FINGERPRINT
                        && matches!(
                            required.fingerprint.as_str(),
                            AUTO_EXIT_ON_DEATH_LEGACY_ENABLED_FINGERPRINT
                                | AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT
                        )))
        });
        if !preserved {
            return Err(format!(
                "生成结果未无损保留现有功能组“{}”（r{}）；为避免删除未来版本数据，已停止原位更新",
                required.id, required.recipe_version
            ));
        }
    }
    Ok(())
}

fn validate_supported_feature_group(
    group: &GeneratorFeatureGroup,
    audio_protocol_version: u8,
) -> Result<(), String> {
    match group.id.as_str() {
        "hostile_auto_restart" => {
            if group.recipe_version != 1
                || group.fingerprint != "hostile-restart-v1;control=1;esc=4"
            {
                return Err("敌对自动重开配方不受支持".into());
            }
            Ok(())
        }
        AUDIO_TELEMETRY_FEATURE_ID => {
            if !matches!(group.recipe_version, 4 | 5) {
                return Err(format!(
                    "D2RHub Mod 的声纹识别功能组配方 r{} 不受支持（需要 r{AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION}）",
                    group.recipe_version
                ));
            }
            validate_audio_feature_fingerprint(&group.fingerprint, audio_protocol_version)
        }
        IN_GAME_ROOM_TOOLS_FEATURE_ID => {
            if group.recipe_version != IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION {
                return Err(format!(
                    "D2RHub Mod 的局内房间工具配方 r{} 不受支持（需要 r{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}）",
                    group.recipe_version
                ));
            }
            let expected = format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}");
            if group.fingerprint != expected {
                return Err("D2RHub Mod 的局内房间工具指纹无效，请重新加工".to_string());
            }
            Ok(())
        }
        PARTY_LOOT_FEATURE_ID => {
            if group.recipe_version != 4 || group.fingerprint != PARTY_LOOT_FINGERPRINT {
                return Err("左侧拾取按钮功能组无效，请重新加工".into());
            }
            Ok(())
        }
        SCENE_SHORTCUTS_FEATURE_ID => {
            if group.recipe_version != 7 || group.fingerprint != SCENE_SHORTCUTS_FINGERPRINT {
                return Err("场景重开功能组无效，请重新加工".into());
            }
            Ok(())
        }
        ESC_NEXT_GAME_FEATURE_ID => {
            if group.recipe_version != 4 || group.fingerprint != ESC_NEXT_GAME_FINGERPRINT {
                return Err("双击 Esc 下一局地狱功能组无效，请重新加工".to_string());
            }
            Ok(())
        }
        AUTO_EXIT_ON_DEATH_FEATURE_ID => validate_auto_exit_on_death_feature_group(group),
        // Unknown groups are intentionally preserved and accepted. Their owner is responsible for
        // interpreting the recipe and fingerprint once D2RHub learns that feature.
        _ => Ok(()),
    }
}

fn validate_auto_exit_on_death_feature_group(group: &GeneratorFeatureGroup) -> Result<(), String> {
    if group.recipe_version != AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION {
        return Err(format!(
            "D2RHub Mod 的死亡后自动退出配方 r{} 不受支持（需要 r{AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION}）",
            group.recipe_version
        ));
    }
    match group.fingerprint.as_str() {
        AUTO_EXIT_ON_DEATH_FINGERPRINT
        | AUTO_EXIT_ON_DEATH_LEGACY_ENABLED_FINGERPRINT
        | AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT => Ok(()),
        _ => Err("D2RHub Mod 的死亡后自动退出指纹无效，请重新加工".to_string()),
    }
}

fn validate_audio_feature_fingerprint(
    fingerprint: &str,
    audio_protocol_version: u8,
) -> Result<(), String> {
    let parts = fingerprint.split(';').collect::<Vec<_>>();
    let expected_protocol = format!("protocol={audio_protocol_version}");
    if parts.len() != 5
        || !matches!(parts[0], "audio-v4" | "audio-v5")
        || parts[1] != expected_protocol
        || !matches!(
            parts[2],
            "areas=countess_route" | "areas=all_areas" | "areas=none"
        )
        || !parts[3].starts_with("track=")
        || !parts[4].starts_with("gain_mdb=")
    {
        return Err("D2RHub Mod 的声纹识别功能组指纹无效，请重新加工".to_string());
    }

    let categories = parts[3].trim_start_matches("track=");
    let supported = [
        "charms", "essences", "gems", "jewels", "keys", "organs", "runes",
    ];
    let requested = if categories.is_empty() {
        Vec::new()
    } else {
        categories.split(',').collect::<Vec<_>>()
    };
    if requested.windows(2).any(|pair| pair[0] >= pair[1])
        || requested
            .iter()
            .any(|category| !supported.contains(category))
        || parts[4]
            .trim_start_matches("gain_mdb=")
            .parse::<i32>()
            .is_err()
    {
        return Err("D2RHub Mod 的声纹识别功能组指纹无效，请重新加工".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_AUDIO_PROTOCOL: u8 = 7;

    #[test]
    fn party_loot_r3_can_upgrade_but_is_not_current() {
        let old = GeneratorFeatureGroup {
            id: PARTY_LOOT_FEATURE_ID.into(),
            recipe_version: 3,
            fingerprint: V3_PARTY_LOOT_FINGERPRINT.into(),
            reused_from_source: true,
        };
        let new = GeneratorFeatureGroup {
            recipe_version: 4,
            fingerprint: PARTY_LOOT_FINGERPRINT.into(),
            ..old.clone()
        };
        assert!(
            validate_feature_group_entries(std::slice::from_ref(&old), TEST_AUDIO_PROTOCOL)
                .is_err()
        );
        validate_upgrade_source_feature_group_entries(
            std::slice::from_ref(&old),
            TEST_AUDIO_PROTOCOL,
        )
        .unwrap();
        validate_feature_group_entries(std::slice::from_ref(&new), TEST_AUDIO_PROTOCOL).unwrap();
        validate_preserved_feature_groups(std::slice::from_ref(&old), std::slice::from_ref(&new))
            .unwrap();
        let forged = GeneratorFeatureGroup {
            fingerprint: "unknown".into(),
            ..old
        };
        assert!(validate_upgrade_source_feature_group_entries(
            std::slice::from_ref(&forged),
            TEST_AUDIO_PROTOCOL
        )
        .is_err());
        assert!(validate_preserved_feature_groups(&[forged], &[new]).is_err());
    }

    fn test_audio_fingerprint() -> String {
        format!("audio-v{AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION};protocol={TEST_AUDIO_PROTOCOL};areas=all_areas;track=runes;gain_mdb=-30000")
    }

    #[test]
    fn hostile_processing_implies_only_esc_dependency() {
        let selected = RequestedFeatureGroups::from_options(
            Some(false),
            Some(false),
            Some(false),
            None,
            None,
            Some(true),
            Some(false),
        )
        .unwrap();
        assert!(!selected.audio_telemetry && !selected.room_tools);
        assert!(selected.esc_next_game && selected.hostile_auto_restart);
        assert_eq!(selected.generator_value(), "hostile-restart,esc-next-game");
        assert!(!selected.all_present(&[]));
    }

    #[test]
    fn scene_r1_is_an_upgrade_source_but_not_a_current_result() {
        let old = GeneratorFeatureGroup {
            id: SCENE_SHORTCUTS_FEATURE_ID.into(),
            recipe_version: 1,
            fingerprint: LEGACY_SCENE_SHORTCUTS_FINGERPRINT.into(),
            reused_from_source: true,
        };
        let new = GeneratorFeatureGroup {
            recipe_version: 7,
            fingerprint: SCENE_SHORTCUTS_FINGERPRINT.into(),
            ..old.clone()
        };
        validate_upgrade_source_feature_group_entries(
            std::slice::from_ref(&old),
            TEST_AUDIO_PROTOCOL,
        )
        .unwrap();
        assert!(
            validate_feature_group_entries(std::slice::from_ref(&old), TEST_AUDIO_PROTOCOL)
                .is_err()
        );
        validate_preserved_feature_groups(&[old], &[new]).unwrap();
    }

    #[test]
    fn scene_shortcuts_are_independent_and_validate_recipe() {
        let requested = RequestedFeatureGroups::from_options(
            Some(false),
            Some(false),
            Some(false),
            Some(true),
            None,
            None,
            Some(false),
        )
        .unwrap();
        assert_eq!(requested.generator_value(), "scene-shortcuts");
        assert!(requested
            .validate_present(&[], TEST_AUDIO_PROTOCOL)
            .is_err());
        let mut group = GeneratorFeatureGroup {
            id: SCENE_SHORTCUTS_FEATURE_ID.into(),
            recipe_version: 7,
            fingerprint: SCENE_SHORTCUTS_FINGERPRINT.into(),
            reused_from_source: false,
        };
        assert!(requested
            .validate_present(&[group.clone()], TEST_AUDIO_PROTOCOL)
            .is_ok());
        group.recipe_version = 8;
        assert!(requested
            .validate_present(&[group], TEST_AUDIO_PROTOCOL)
            .is_err());
    }

    #[test]
    fn omitted_feature_flags_keep_the_legacy_audio_command_contract() {
        let requested =
            RequestedFeatureGroups::from_options(None, None, None, None, None, None, None).unwrap();
        assert!(requested.audio_telemetry);
        assert!(!requested.room_tools);
        assert!(!requested.auto_exit_on_death);
        assert_eq!(requested.generator_value(), "audio");
    }

    #[test]
    fn audio_only_mod_can_be_upgraded_in_place_to_audio_and_room_tools() {
        let audio = GeneratorFeatureGroup {
            id: AUDIO_TELEMETRY_FEATURE_ID.to_string(),
            recipe_version: AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
            fingerprint: test_audio_fingerprint(),
            reused_from_source: false,
        };
        let requested = RequestedFeatureGroups::from_options(
            Some(true),
            Some(true),
            Some(false),
            None,
            None,
            None,
            Some(false),
        )
        .unwrap()
        .include_existing_known(std::slice::from_ref(&audio));
        assert!(!requested.all_present(std::slice::from_ref(&audio)));
        assert_eq!(requested.generator_value(), "audio,rooms");

        let room = GeneratorFeatureGroup {
            id: IN_GAME_ROOM_TOOLS_FEATURE_ID.to_string(),
            recipe_version: IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
            fingerprint: format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}"),
            reused_from_source: false,
        };
        assert!(requested.all_present(&[audio, room]));
    }

    #[test]
    fn legacy_esc_can_upgrade_but_is_not_a_current_result() {
        let old = GeneratorFeatureGroup {
            id: ESC_NEXT_GAME_FEATURE_ID.to_string(),
            recipe_version: 2,
            fingerprint: LEGACY_ESC_NEXT_GAME_FINGERPRINT.to_string(),
            reused_from_source: false,
        };
        let current = GeneratorFeatureGroup {
            recipe_version: 4,
            fingerprint: ESC_NEXT_GAME_FINGERPRINT.to_string(),
            ..old.clone()
        };
        validate_upgrade_source_feature_group_entries(
            std::slice::from_ref(&old),
            TEST_AUDIO_PROTOCOL,
        )
        .unwrap();
        assert!(
            validate_feature_group_entries(std::slice::from_ref(&old), TEST_AUDIO_PROTOCOL)
                .is_err()
        );
        validate_feature_group_entries(std::slice::from_ref(&current), TEST_AUDIO_PROTOCOL)
            .unwrap();
        validate_preserved_feature_groups(
            std::slice::from_ref(&old),
            std::slice::from_ref(&current),
        )
        .unwrap();
        let mut corrupt = current;
        corrupt.fingerprint = "invalid".to_string();
        assert!(validate_preserved_feature_groups(&[old], &[corrupt]).is_err());
    }

    #[test]
    fn malformed_or_duplicate_metadata_is_rejected_before_feature_interpretation() {
        let future = serde_json::json!({
            "id": "future_feature", "recipe_version": 77, "fingerprint": "opaque-v77"
        });
        assert!(parse_feature_groups(&serde_json::json!({
            "feature_groups": [future.clone(), future.clone()]
        }))
        .is_err());
        for (key, value) in [
            ("id", serde_json::json!("../future")),
            ("recipe_version", serde_json::json!(0)),
            ("fingerprint", serde_json::json!(" ")),
        ] {
            let mut malformed = future.clone();
            malformed[key] = value;
            assert!(parse_feature_groups(&serde_json::json!({
                "feature_groups": [malformed]
            }))
            .is_err());
        }
    }

    #[test]
    fn unknown_features_are_accepted_but_must_survive_an_upgrade_unchanged() {
        let groups = parse_feature_groups(&serde_json::json!({
            "feature_groups": [{
                "id": "future_feature", "recipe_version": 77, "fingerprint": "opaque-v77"
            }]
        }))
        .unwrap();
        validate_feature_group_entries(&groups, TEST_AUDIO_PROTOCOL).unwrap();
        validate_preserved_feature_groups(&groups, &groups).unwrap();
        assert!(validate_preserved_feature_groups(&groups, &[]).is_err());
        let mut changed = groups.clone();
        changed[0].fingerprint = "opaque-v78".into();
        assert!(validate_preserved_feature_groups(&groups, &changed).is_err());
    }

    #[test]
    fn previous_room_recipe_is_an_upgrade_source_but_not_a_current_result() {
        let previous = IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION - 1;
        let mut group = GeneratorFeatureGroup {
            id: IN_GAME_ROOM_TOOLS_FEATURE_ID.into(),
            recipe_version: previous,
            fingerprint: format!("room-tools-v{previous}"),
            reused_from_source: false,
        };
        validate_upgrade_source_feature_group_entries(
            std::slice::from_ref(&group),
            TEST_AUDIO_PROTOCOL,
        )
        .unwrap();
        assert!(
            validate_feature_group_entries(std::slice::from_ref(&group), TEST_AUDIO_PROTOCOL)
                .is_err()
        );
        group.fingerprint = "forged-room-tools".into();
        assert!(
            validate_upgrade_source_feature_group_entries(&[group], TEST_AUDIO_PROTOCOL).is_err()
        );
    }

    #[test]
    fn requested_audio_must_match_the_host_protocol_and_a_supported_fingerprint() {
        let requested =
            RequestedFeatureGroups::from_options(Some(true), None, None, None, None, None, None)
                .unwrap();
        let mut group = GeneratorFeatureGroup {
            id: AUDIO_TELEMETRY_FEATURE_ID.into(),
            recipe_version: AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
            fingerprint: test_audio_fingerprint(),
            reused_from_source: false,
        };
        requested
            .validate_present(std::slice::from_ref(&group), TEST_AUDIO_PROTOCOL)
            .unwrap();
        assert!(requested
            .validate_present(&[], TEST_AUDIO_PROTOCOL)
            .is_err());
        assert!(requested
            .validate_present(std::slice::from_ref(&group), TEST_AUDIO_PROTOCOL + 1)
            .is_err());
        for tracking in ["runes,runes", "runes,gems", "unrecognized"] {
            group.fingerprint =
                test_audio_fingerprint().replace("track=runes", &format!("track={tracking}"));
            assert!(requested
                .validate_present(std::slice::from_ref(&group), TEST_AUDIO_PROTOCOL)
                .is_err());
        }
        group.fingerprint = test_audio_fingerprint();
        group.recipe_version += 1;
        assert!(requested
            .validate_present(&[group], TEST_AUDIO_PROTOCOL)
            .is_err());
    }

    #[test]
    fn legacy_death_exit_state_can_normalize_without_losing_its_feature_identity() {
        let normalized = GeneratorFeatureGroup {
            id: AUTO_EXIT_ON_DEATH_FEATURE_ID.into(),
            recipe_version: AUTO_EXIT_ON_DEATH_FEATURE_RECIPE_VERSION,
            fingerprint: AUTO_EXIT_ON_DEATH_FINGERPRINT.into(),
            reused_from_source: false,
        };
        for fingerprint in [
            AUTO_EXIT_ON_DEATH_LEGACY_ENABLED_FINGERPRINT,
            AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT,
        ] {
            let legacy = GeneratorFeatureGroup {
                fingerprint: fingerprint.into(),
                ..normalized.clone()
            };
            validate_feature_group_entries(std::slice::from_ref(&legacy), TEST_AUDIO_PROTOCOL)
                .unwrap();
            validate_preserved_feature_groups(&[legacy], std::slice::from_ref(&normalized))
                .unwrap();
        }
        let different_recipe = GeneratorFeatureGroup {
            recipe_version: normalized.recipe_version + 1,
            ..normalized.clone()
        };
        assert!(validate_preserved_feature_groups(&[different_recipe], &[normalized]).is_err());
    }
}
