use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const RUNE_COUNT: u32 = 33;
/// The v5 location packet layout reserves ten bits for an Area Id.
pub const MAX_AREA_ID: u32 = 1023;
pub const AREA_CATALOG_FILE_NAME: &str = "rune-audio-area-catalog.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TelemetryMarker {
    Rune { rune_number: u32 },
    Area { area_id: u32 },
    Frontend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Locations with localized product behavior. Every other valid Area Id is
/// still decoded and is resolved through the catalog produced with the mod.
pub const TRACKED_LOCATIONS: [LocationDefinition; 8] = [
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
        TelemetryMarker::Area { area_id } if (1..=MAX_AREA_ID).contains(&area_id) => Ok(()),
        TelemetryMarker::Area { .. } => Err(format!("Area Id 必须位于 1-{MAX_AREA_ID}")),
        TelemetryMarker::Frontend => Ok(()),
    }
}

pub fn marker_sort_key(marker: TelemetryMarker) -> u32 {
    match marker {
        TelemetryMarker::Rune { rune_number } => rune_number,
        TelemetryMarker::Area { area_id } => 1_000 + area_id,
        TelemetryMarker::Frontend => u32::MAX,
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocationCatalog {
    areas: HashMap<u32, AreaCatalogEntry>,
}

impl LocationCatalog {
    pub fn load(app_data_dir: &str) -> Self {
        let path = Path::new(app_data_dir).join(AREA_CATALOG_FILE_NAME);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        let Ok(file) = serde_json::from_slice::<AreaCatalogFile>(&bytes) else {
            crate::logger::log_msg(
                "WARN",
                "RuneAudio",
                &format!("地图声纹目录无效，回退到 Area Id: {}", path.display()),
            );
            return Self::default();
        };
        Self {
            areas: file
                .areas
                .into_iter()
                .map(|entry| (entry.area_id, entry))
                .collect(),
        }
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
            | TelemetryMarker::Area { .. }
            | TelemetryMarker::Frontend => None,
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
