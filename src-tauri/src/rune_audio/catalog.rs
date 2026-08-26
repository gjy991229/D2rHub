use serde::{Deserialize, Serialize};

pub const RUNE_COUNT: u32 = 33;

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
    pub signal_number: u32,
    pub file_stem: &'static str,
    pub scene_key: &'static str,
    pub scene_name: &'static str,
    pub scene_name_en: &'static str,
    pub kind: LocationKind,
}

/// 第一批地点协议。野外始终按原始 Area 独立计时，路线合并属于统计展示策略。
pub const TRACKED_LOCATIONS: [LocationDefinition; 8] = [
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 1 },
        signal_number: 34,
        file_stem: "a1",
        scene_key: "rogue_encampment",
        scene_name: "罗格营地",
        scene_name_en: "Rogue Encampment",
        kind: LocationKind::Town,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 6 },
        signal_number: 35,
        file_stem: "a6",
        scene_key: "black_marsh",
        scene_name: "黑色荒地",
        scene_name_en: "Black Marsh",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 21 },
        signal_number: 36,
        file_stem: "a21",
        scene_key: "tower_cellar_1",
        scene_name: "遗忘之塔地牢第1层",
        scene_name_en: "Tower Cellar Level 1",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 22 },
        signal_number: 37,
        file_stem: "a22",
        scene_key: "tower_cellar_2",
        scene_name: "遗忘之塔地牢第2层",
        scene_name_en: "Tower Cellar Level 2",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 23 },
        signal_number: 38,
        file_stem: "a23",
        scene_key: "tower_cellar_3",
        scene_name: "遗忘之塔地牢第3层",
        scene_name_en: "Tower Cellar Level 3",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 24 },
        signal_number: 39,
        file_stem: "a24",
        scene_key: "tower_cellar_4",
        scene_name: "遗忘之塔地牢第4层",
        scene_name_en: "Tower Cellar Level 4",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Area { area_id: 25 },
        signal_number: 40,
        file_stem: "a25",
        scene_key: "tower_cellar_5",
        scene_name: "遗忘之塔地牢第5层",
        scene_name_en: "Tower Cellar Level 5",
        kind: LocationKind::Wilderness,
    },
    LocationDefinition {
        marker: TelemetryMarker::Frontend,
        signal_number: 41,
        file_stem: "frontend",
        scene_key: "frontend",
        scene_name: "主界面",
        scene_name_en: "Main Menu",
        kind: LocationKind::Frontend,
    },
];

pub const TRACKED_AREA_IDS: [u32; 7] = [1, 6, 21, 22, 23, 24, 25];
pub const SIGNAL_COUNT: u32 = RUNE_COUNT + TRACKED_LOCATIONS.len() as u32;

pub fn location_definition(marker: TelemetryMarker) -> Option<&'static LocationDefinition> {
    TRACKED_LOCATIONS
        .iter()
        .find(|location| location.marker == marker)
}

pub fn area_definition(area_id: u32) -> Option<&'static LocationDefinition> {
    location_definition(TelemetryMarker::Area { area_id })
}

pub fn marker_signal_number(marker: TelemetryMarker) -> Result<u32, String> {
    match marker {
        TelemetryMarker::Rune { rune_number } if (1..=RUNE_COUNT).contains(&rune_number) => {
            Ok(rune_number)
        }
        TelemetryMarker::Rune { .. } => Err(format!("符文编号必须位于 1-{RUNE_COUNT}")),
        location => location_definition(location)
            .map(|definition| definition.signal_number)
            .ok_or_else(|| format!("地点 {location:?} 尚未加入声纹目录")),
    }
}

pub fn marker_from_signal_number(signal_number: u32) -> Option<TelemetryMarker> {
    if (1..=RUNE_COUNT).contains(&signal_number) {
        return Some(TelemetryMarker::Rune {
            rune_number: signal_number,
        });
    }
    TRACKED_LOCATIONS
        .iter()
        .find(|location| location.signal_number == signal_number)
        .map(|location| location.marker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_markers_have_unique_contiguous_signal_numbers() {
        let markers = (1..=RUNE_COUNT)
            .map(|rune_number| TelemetryMarker::Rune { rune_number })
            .chain(TRACKED_LOCATIONS.iter().map(|location| location.marker))
            .collect::<Vec<_>>();
        let signals = markers
            .iter()
            .map(|marker| marker_signal_number(*marker).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(signals, (1..=SIGNAL_COUNT).collect::<Vec<_>>());
        assert_eq!(
            markers,
            signals
                .into_iter()
                .map(|signal| marker_from_signal_number(signal).unwrap())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn catalog_classifies_town_wilderness_and_frontend() {
        assert_eq!(area_definition(1).unwrap().kind, LocationKind::Town);
        for area_id in [6, 21, 22, 23, 24, 25] {
            assert_eq!(
                area_definition(area_id).unwrap().kind,
                LocationKind::Wilderness
            );
        }
        assert_eq!(
            location_definition(TelemetryMarker::Frontend).unwrap().kind,
            LocationKind::Frontend
        );
    }
}
