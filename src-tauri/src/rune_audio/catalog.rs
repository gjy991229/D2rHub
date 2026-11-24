use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const TRAVINCAL_AREA_ID: u32 = 83;
pub const CHAOS_SANCTUARY_AREA_ID: u32 = 108;

pub fn is_timed_scene(area_id: u32) -> bool {
    matches!(area_id, TRAVINCAL_AREA_ID | CHAOS_SANCTUARY_AREA_ID)
}

pub const RUNE_COUNT: u32 = 33;
/// Gold-code slots 34-127 are reserved for generated, catalog-backed item identities.
pub const MAX_ITEM_ID: u32 = 94;
/// The v7 location packet layout reserves ten bits for an Area Id.
pub const MAX_AREA_ID: u32 = 1023;
pub const AREA_CATALOG_FILE_NAME: &str = "audio-telemetry-area-catalog.json";
pub const LEGACY_AREA_CATALOG_FILE_NAME: &str = "rune-audio-area-catalog.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TelemetryMarker {
    Rune { rune_number: u32 },
    Item { item_id: u32 },
    Area { area_id: u32 },
    Frontend,
    Hostile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    Town,
    Wilderness,
    Frontend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocationDefinition {
    pub marker: TelemetryMarker,
    pub file_stem: &'static str,
    pub scene_key: &'static str,
    pub scene_name: &'static str,
    pub scene_name_en: &'static str,
    pub kind: LocationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaCatalogEntry {
    pub area_id: u32,
    pub scene_key: String,
    pub scene_name: String,
    pub scene_name_en: String,
    pub kind: LocationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaCatalogFile {
    pub protocol_version: u8,
    pub source_levels: String,
    pub areas: Vec<AreaCatalogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLocation {
    pub marker: TelemetryMarker,
    pub scene_key: String,
    pub scene_name: String,
    pub scene_name_en: String,
    pub kind: LocationKind,
}

/// OCR identifies a named scene, not an individual game Area Id. Several game
/// areas can intentionally share this identity (the seven tombs, both sewers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedLocation {
    pub scene_name: String,
    pub scene_name_en: String,
    pub kind: LocationKind,
}

impl NamedLocation {
    /// Keep exact floor names for recognition; collapse floors only at the
    /// statistics boundary. No fuzzy matching of unrelated area names.
    pub fn statistics_location(mut self) -> Self {
        if self.kind == LocationKind::Wilderness {
            self.scene_name = floor_family(&self.scene_name);
            self.scene_name_en = floor_family(&self.scene_name_en);
        }
        self
    }
}

fn floor_family(name: &str) -> String {
    let name = name.trim();
    if let Some(body) = name.strip_suffix('层').or_else(|| name.strip_suffix('層')) {
        let base = body.trim_end_matches(|c: char| {
            c.is_ascii_digit()
                || "０１２３４５６７８９一二三四五六七八九十百〇零壹贰貳叁參肆伍陆陸柒捌玖拾"
                    .contains(c)
        });
        if base.len() != body.len() {
            let base = base.strip_suffix('第').unwrap_or(base).trim_end();
            if !base.is_empty() {
                return base.to_owned();
            }
        }
    }
    let lower = name.to_ascii_lowercase();
    if let Some(index) = lower.rfind(" level ") {
        let number = name[index + 7..].trim();
        if index > 0 && !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
            return name[..index].trim_end().to_owned();
        }
    }
    name.to_owned()
}

#[cfg(test)]
#[test]
fn statistics_groups_numbered_floors_without_merging_distinct_places() {
    for (input, expected) in [
        ("地下墓穴一层", "地下墓穴"),
        ("地下墓穴第三層", "地下墓穴"),
        ("遗忘之塔地牢第5层", "遗忘之塔地牢"),
        ("Worldstone Keep Level 3", "Worldstone Keep"),
        ("塔·拉夏之墓", "塔·拉夏之墓"),
        ("塔·拉夏的墓室", "塔·拉夏的墓室"),
        ("库拉斯特下层区", "库拉斯特下层区"),
        ("先祖之路", "先祖之路"),
    ] {
        assert_eq!(floor_family(input), expected);
    }
}

pub fn scene_name_identity(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Locations with localized product behavior. Every other valid Area Id is
/// still decoded and is resolved through the catalog produced with the mod.
pub const TRACKED_LOCATIONS: [LocationDefinition; 10] = [
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 1 },
        file_stem: "a1",
        scene_key: "rogue_encampment",
        scene_name: "罗格营地",
        scene_name_en: "Rogue Encampment",
        kind: LocationKind::Town,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 6 },
        file_stem: "a6",
        scene_key: "black_marsh",
        scene_name: "黑色荒地",
        scene_name_en: "Black Marsh",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 21 },
        file_stem: "a21",
        scene_key: "tower_cellar_1",
        scene_name: "遗忘之塔地牢第1层",
        scene_name_en: "Tower Cellar Level 1",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 22 },
        file_stem: "a22",
        scene_key: "tower_cellar_2",
        scene_name: "遗忘之塔地牢第2层",
        scene_name_en: "Tower Cellar Level 2",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 23 },
        file_stem: "a23",
        scene_key: "tower_cellar_3",
        scene_name: "遗忘之塔地牢第3层",
        scene_name_en: "Tower Cellar Level 3",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 24 },
        file_stem: "a24",
        scene_key: "tower_cellar_4",
        scene_name: "遗忘之塔地牢第4层",
        scene_name_en: "Tower Cellar Level 4",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 25 },
        file_stem: "a25",
        scene_key: "tower_cellar_5",
        scene_name: "遗忘之塔地牢第5层",
        scene_name_en: "Tower Cellar Level 5",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: TRAVINCAL_AREA_ID },
        file_stem: "a83",
        scene_key: "travincal",
        scene_name: "崔凡克",
        scene_name_en: "Travincal",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: CHAOS_SANCTUARY_AREA_ID },
        file_stem: "a108",
        scene_key: "chaos_sanctuary",
        scene_name: "混沌魔殿",
        scene_name_en: "Chaos Sanctuary",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Frontend,
        file_stem: "frontend",
        scene_key: "frontend",
        scene_name: "主界面",
        scene_name_en: "Main Menu",
        kind: LocationKind::Frontend,
    },
];

pub const TRACKED_AREA_IDS: [u32; 7] = [1, 6, 21, 22, 23, 24, 25];

pub fn location_definition(marker: TelemetryMarker) -> Option<&'static LocationDefinition> {
    TRACKED_LOCATIONS
        .iter()
        .find(|location| location.marker == marker)
}

pub fn area_definition(area_id: u32) -> Option<&'static LocationDefinition> {
    location_definition(TelemetryMarker::Area { area_id })
}

pub fn validate_marker(marker: TelemetryMarker) -> Result<(), String> {
    match marker {
        TelemetryMarker::Rune { rune_number } if (1..=RUNE_COUNT).contains(&rune_number) => Ok(()),
        TelemetryMarker::Rune { .. } => Err(format!("符文编号必须位于 1-{RUNE_COUNT}")),
        TelemetryMarker::Item { item_id } if (1..=MAX_ITEM_ID).contains(&item_id) => Ok(()),
        TelemetryMarker::Item { .. } => Err(format!("物品编号必须位于 1-{MAX_ITEM_ID}")),
        TelemetryMarker::Area { area_id } if (1..=MAX_AREA_ID).contains(&area_id) => Ok(()),
        TelemetryMarker::Area { .. } => Err(format!("Area Id 必须位于 1-{MAX_AREA_ID}")),
        TelemetryMarker::Frontend | TelemetryMarker::Hostile => Ok(()),
    }
}

pub fn marker_sort_key(marker: TelemetryMarker) -> u32 {
    match marker {
        TelemetryMarker::Rune { rune_number } => rune_number,
        TelemetryMarker::Item { item_id } => 100 + item_id,
        TelemetryMarker::Area { area_id } => 1_000 + area_id,
        TelemetryMarker::Frontend => u32::MAX,
        TelemetryMarker::Hostile => u32::MAX - 1,
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocationCatalog {
    areas: HashMap<u32, AreaCatalogEntry>,
}

impl LocationCatalog {
    /// Adapt the legacy directory at the boundary. No Area Id or area-specific
    /// scene key is carried into OCR, matching, or visual timing.
    pub fn named_locations(&self) -> Vec<NamedLocation> {
        let mut names: Vec<_> = (1..=MAX_AREA_ID)
            .filter(|id| self.has_name(*id))
            .filter_map(|area_id| self.resolve(TelemetryMarker::Area { area_id }))
            .filter(|location| {
                let name = scene_name_identity(&location.scene_name);
                let english = scene_name_identity(&location.scene_name_en);
                !name.is_empty()
                    && !english.starts_with("area")
                    && !name.starts_with("地图area")
                    && !name.starts_with("场景area")
            })
            .map(|location| NamedLocation {
                scene_name: location.scene_name,
                scene_name_en: location.scene_name_en,
                kind: location.kind,
            })
            .collect();
        names.sort_by(|a, b| {
            (&a.scene_name, &a.scene_name_en).cmp(&(&b.scene_name, &b.scene_name_en))
        });
        names.dedup();
        names
    }

    pub fn has_name(&self, area_id: u32) -> bool {
        self.areas.contains_key(&area_id)
            || location_definition(TelemetryMarker::Area { area_id }).is_some()
    }
    pub fn load_from_directory(directory: &Path) -> Result<Self, String> {
        let path = [AREA_CATALOG_FILE_NAME, LEGACY_AREA_CATALOG_FILE_NAME]
            .into_iter()
            .map(|name| directory.join(name))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| format!("目录中没有协议地图清单: {}", directory.display()))?;
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("读取地图协议清单失败 {}: {error}", path.display()))?;
        let file = serde_json::from_slice::<AreaCatalogFile>(&bytes)
            .map_err(|error| format!("解析地图协议清单失败 {}: {error}", path.display()))?;
        if file.protocol_version != super::protocol::PROTOCOL_VERSION {
            return Err(format!(
                "地图清单协议版本 {} 与接收端 v{} 不兼容: {}",
                file.protocol_version,
                super::protocol::PROTOCOL_VERSION,
                path.display()
            ));
        }
        let areas = file
            .areas
            .into_iter()
            .filter(|entry| (1..=MAX_AREA_ID).contains(&entry.area_id))
            .map(|entry| (entry.area_id, entry))
            .collect();
        Ok(Self { areas })
    }

    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    pub fn resolve(&self, marker: TelemetryMarker) -> Option<ResolvedLocation> {
        if let Some(definition) = location_definition(marker) {
            return Some(ResolvedLocation {
                marker,
                scene_key: definition.scene_key.to_string(),
                scene_name: definition.scene_name.to_string(),
                scene_name_en: definition.scene_name_en.to_string(),
                kind: definition.kind,
            });
        }
        match marker {
            TelemetryMarker::Area { area_id } if (1..=MAX_AREA_ID).contains(&area_id) => {
                let entry = self.areas.get(&area_id);
                Some(ResolvedLocation {
                    marker,
                    scene_key: entry
                        .map(|item| item.scene_key.clone())
                        .unwrap_or_else(|| format!("area_{area_id}")),
                    scene_name: entry
                        .map(|item| item.scene_name.clone())
                        .unwrap_or_else(|| format!("地图 Area {area_id}")),
                    scene_name_en: entry
                        .map(|item| item.scene_name_en.clone())
                        .unwrap_or_else(|| format!("Area {area_id}")),
                    kind: entry
                        .map(|item| item.kind)
                        .unwrap_or(LocationKind::Wilderness),
                })
            }
            TelemetryMarker::Rune { .. }
            | TelemetryMarker::Item { .. }
            | TelemetryMarker::Area { .. }
            | TelemetryMarker::Frontend
            | TelemetryMarker::Hostile => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_packet_ranges() {
        assert!(validate_marker(TelemetryMarker::Rune { rune_number: 1 }).is_ok());
        assert!(validate_marker(TelemetryMarker::Rune { rune_number: 34 }).is_err());
        assert!(validate_marker(TelemetryMarker::Item { item_id: 1 }).is_ok());
        assert!(validate_marker(TelemetryMarker::Item {
            item_id: MAX_ITEM_ID
        })
        .is_ok());
        assert!(validate_marker(TelemetryMarker::Item {
            item_id: MAX_ITEM_ID + 1
        })
        .is_err());
        assert!(validate_marker(TelemetryMarker::Area { area_id: 137 }).is_ok());
        assert!(validate_marker(TelemetryMarker::Area { area_id: 1024 }).is_err());
    }

    #[test]
    fn catalog_prefers_localized_routes_and_falls_back_for_every_area() {
        let catalog = LocationCatalog::default();
        assert_eq!(
            catalog
                .resolve(TelemetryMarker::Area { area_id: 1 })
                .unwrap()
                .kind,
            LocationKind::Town
        );
        assert_eq!(
            catalog
                .resolve(TelemetryMarker::Area { area_id: 6 })
                .unwrap()
                .scene_name,
            "黑色荒地"
        );
        assert_eq!(
            catalog
                .resolve(TelemetryMarker::Area { area_id: 99 })
                .unwrap()
                .scene_name_en,
            "Area 99"
        );
    }
}
