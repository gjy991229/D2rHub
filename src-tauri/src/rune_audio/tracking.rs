use super::catalog::{
    LocationCatalog, LocationKind, ResolvedLocation, TelemetryMarker, MAX_AREA_ID,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Deduplicates location markers while accepting a different location immediately.
#[derive(Debug, Clone)]
pub struct SceneTransitionGate {
    confirmed: Option<TelemetryMarker>,
}

impl SceneTransitionGate {
    pub fn new(_sample_rate: u32) -> Self {
        Self { confirmed: None }
    }

    /// Returns true on the first valid marker for a different location.
    pub fn observe(&mut self, marker: TelemetryMarker, _observed_at_frame: u64) -> bool {
        if !matches!(
            marker,
            TelemetryMarker::Area { area_id } if (1..=MAX_AREA_ID).contains(&area_id)
        ) && marker != TelemetryMarker::Frontend
        {
            return false;
        }
        if self.confirmed == Some(marker) {
            return false;
        }
        self.confirmed = Some(marker);
        true
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackedRuneDrop {
    pub observation_id: i64,
    pub rune_number: u32,
    pub rune_name: String,
    pub rune_name_en: String,
}

/// One persisted raw wilderness segment. Display strategies may merge adjacent
/// segments sharing a journey_id, but this source record is never pre-merged.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedSegment {
    pub absolute_time: String,
    pub character_name: String,
    pub scene_key: String,
    pub scene_name: String,
    pub scene_name_en: String,
    pub journey_id: String,
    pub segment_index: u32,
    pub timer_seconds: f64,
    pub drops: Vec<TrackedRuneDrop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackingSnapshot {
    pub revision: u64,
    pub account_id: String,
    pub current_area_id: Option<u32>,
    pub current_scene: String,
    pub current_scene_en: String,
    pub location_kind: Option<LocationKind>,
    pub is_town: bool,
    pub is_frontend: bool,
    pub is_timing: bool,
    pub timer_started_at_ms: Option<i64>,
    pub current_run_key: Option<String>,
    pub current_run_name: Option<String>,
    pub current_run_name_en: Option<String>,
    pub current_run_drops: Vec<TrackedRuneDrop>,
    pub session_runs: HashMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackingOutcome {
    pub changed: bool,
    pub snapshot: TrackingSnapshot,
    pub completed_segment: Option<CompletedSegment>,
}

#[derive(Debug, Clone)]
struct ActiveSegment {
    scene_key: String,
    scene_name: String,
    scene_name_en: String,
    journey_id: String,
    segment_index: u32,
    started_at_frame: u64,
    started_at_ms: i64,
    absolute_time: String,
    drops: Vec<TrackedRuneDrop>,
}

/// The deep timing module: callers submit confirmed locations and rune drops;
/// town/frontend stopping, wilderness splitting and journey continuity stay hidden.
#[derive(Debug, Clone)]
pub struct SegmentTracker {
    account_id: String,
    character_name: String,
    current_location: Option<TelemetryMarker>,
    active_segment: Option<ActiveSegment>,
    journey_id: Option<String>,
    next_segment_index: u32,
    session_runs: HashMap<String, u32>,
    revision: u64,
    sample_rate: u32,
    catalog: LocationCatalog,
}

impl SegmentTracker {
    #[cfg(test)]
    pub fn new(account_id: String, character_name: String, sample_rate: u32) -> Self {
        Self::with_catalog(
            account_id,
            character_name,
            sample_rate,
            LocationCatalog::default(),
        )
    }

    pub fn with_catalog(
        account_id: String,
        character_name: String,
        sample_rate: u32,
        catalog: LocationCatalog,
    ) -> Self {
        Self {
            account_id,
            character_name,
            current_location: None,
            active_segment: None,
            journey_id: None,
            next_segment_index: 0,
            session_runs: HashMap::new(),
            revision: 0,
            sample_rate,
            catalog,
        }
    }

    pub fn observe_location(
        &mut self,
        marker: TelemetryMarker,
        observed_at_frame: u64,
        observed_at_ms: i64,
        absolute_time: String,
    ) -> Result<TrackingOutcome, String> {
        let location = self
            .catalog
            .resolve(marker)
            .ok_or_else(|| format!("{marker:?} 不是已登记的地点声纹"))?;
        if self.current_location == Some(marker) {
            return Ok(TrackingOutcome {
                changed: false,
                snapshot: self.snapshot(),
                completed_segment: None,
            });
        }

        let completed_segment = self.finish_active_segment(observed_at_frame, observed_at_ms);
        match location.kind {
            LocationKind::Town | LocationKind::Frontend => {
                self.journey_id = None;
                self.next_segment_index = 0;
            }
            LocationKind::Wilderness => {
                let journey_id = self
                    .journey_id
                    .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
                    .clone();
                self.active_segment = Some(Self::start_segment(
                    &location,
                    journey_id,
                    self.next_segment_index,
                    observed_at_frame,
                    observed_at_ms,
                    absolute_time,
                ));
                self.next_segment_index += 1;
            }
        }

        self.current_location = Some(marker);
        self.revision += 1;
        Ok(TrackingOutcome {
            changed: true,
            snapshot: self.snapshot(),
            completed_segment,
        })
    }

    pub fn observe_rune(&mut self, drop: TrackedRuneDrop) -> TrackingSnapshot {
        if let Some(active_segment) = &mut self.active_segment {
            active_segment.drops.push(drop);
            self.revision += 1;
        }
        self.snapshot()
    }

    pub fn snapshot(&self) -> TrackingSnapshot {
        let location = self
            .current_location
            .and_then(|marker| self.catalog.resolve(marker));
        TrackingSnapshot {
            revision: self.revision,
            account_id: self.account_id.clone(),
            current_area_id: self.current_location.and_then(|marker| match marker {
                TelemetryMarker::Area { area_id } => Some(area_id),
                TelemetryMarker::Rune { .. } | TelemetryMarker::Frontend => None,
            }),
            current_scene: location
                .as_ref()
                .map(|item| item.scene_name.to_string())
                .unwrap_or_else(|| "等待识别...".to_string()),
            current_scene_en: location
                .as_ref()
                .map(|item| item.scene_name_en.to_string())
                .unwrap_or_else(|| "Waiting for location...".to_string()),
            location_kind: location.as_ref().map(|item| item.kind),
            is_town: location
                .as_ref()
                .is_some_and(|item| item.kind == LocationKind::Town),
            is_frontend: location
                .as_ref()
                .is_some_and(|item| item.kind == LocationKind::Frontend),
            is_timing: self.active_segment.is_some(),
            timer_started_at_ms: self
                .active_segment
                .as_ref()
                .map(|segment| segment.started_at_ms),
            current_run_key: self
                .active_segment
                .as_ref()
                .map(|segment| segment.scene_key.clone()),
            current_run_name: self
                .active_segment
                .as_ref()
                .map(|segment| segment.scene_name.clone()),
            current_run_name_en: self
                .active_segment
                .as_ref()
                .map(|segment| segment.scene_name_en.clone()),
            current_run_drops: self
                .active_segment
                .as_ref()
                .map(|segment| segment.drops.clone())
                .unwrap_or_default(),
            session_runs: self.session_runs.clone(),
        }
    }

    fn start_segment(
        location: &ResolvedLocation,
        journey_id: String,
        segment_index: u32,
        observed_at_frame: u64,
        observed_at_ms: i64,
        absolute_time: String,
    ) -> ActiveSegment {
        ActiveSegment {
            scene_key: location.scene_key.to_string(),
            scene_name: location.scene_name.to_string(),
            scene_name_en: location.scene_name_en.to_string(),
            journey_id,
            segment_index,
            started_at_frame: observed_at_frame,
            started_at_ms: observed_at_ms,
            absolute_time,
            drops: Vec::new(),
        }
    }

    fn finish_active_segment(
        &mut self,
        observed_at_frame: u64,
        observed_at_ms: i64,
    ) -> Option<CompletedSegment> {
        let active = self.active_segment.take()?;
        let elapsed_frames = observed_at_frame.checked_sub(active.started_at_frame);
        let timer_seconds = elapsed_frames
            .map(|frames| frames as f64 / self.sample_rate as f64)
            .unwrap_or_else(|| (observed_at_ms - active.started_at_ms).max(0) as f64 / 1000.0);
        *self
            .session_runs
            .entry(active.scene_key.clone())
            .or_default() += 1;
        Some(CompletedSegment {
            absolute_time: active.absolute_time,
            character_name: self.character_name.clone(),
            scene_key: active.scene_key,
            scene_name: active.scene_name,
            scene_name_en: active.scene_name_en,
            journey_id: active.journey_id,
            segment_index: active.segment_index,
            timer_seconds: (timer_seconds * 10.0).round() / 10.0,
            drops: active.drops,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(area_id: u32) -> TelemetryMarker {
        TelemetryMarker::Area { area_id }
    }

    fn tracker() -> SegmentTracker {
        SegmentTracker::new("account-1".to_string(), "测试角色".to_string(), 48_000)
    }

    #[test]
    fn every_different_wilderness_is_an_independent_segment_in_one_journey() {
        let mut tracker = tracker();
        let black_marsh = tracker
            .observe_location(area(6), 48_000, 1_000, "2026/08/26/12:00:00".to_string())
            .unwrap();
        assert_eq!(
            black_marsh.snapshot.current_run_name.as_deref(),
            Some("黑色荒地")
        );
        tracker.observe_rune(TrackedRuneDrop {
            observation_id: 7,
            rune_number: 24,
            rune_name: "伊斯特".to_string(),
            rune_name_en: "Ist".to_string(),
        });

        let tower_one = tracker
            .observe_location(area(21), 144_000, 3_000, "2026/08/26/12:00:02".to_string())
            .unwrap();
        let first = tower_one.completed_segment.unwrap();
        assert_eq!(first.scene_name, "黑色荒地");
        assert_eq!(first.segment_index, 0);
        assert_eq!(first.timer_seconds, 2.0);
        assert_eq!(first.drops.len(), 1);
        assert_eq!(
            tower_one.snapshot.current_run_name.as_deref(),
            Some("遗忘之塔地牢第1层")
        );

        let town = tracker
            .observe_location(area(1), 240_000, 5_000, "ignored".to_string())
            .unwrap();
        let second = town.completed_segment.unwrap();
        assert_eq!(second.scene_name, "遗忘之塔地牢第1层");
        assert_eq!(second.segment_index, 1);
        assert_eq!(first.journey_id, second.journey_id);
        assert!(!town.snapshot.is_timing);
    }

    #[test]
    fn town_and_frontend_end_the_journey_and_never_start_timing() {
        let mut tracker = tracker();
        let town = tracker
            .observe_location(area(1), 100, 100, "town".to_string())
            .unwrap();
        assert!(!town.snapshot.is_timing);
        tracker
            .observe_location(area(6), 200, 200, "wild".to_string())
            .unwrap();
        let frontend = tracker
            .observe_location(TelemetryMarker::Frontend, 48_200, 1_200, "menu".to_string())
            .unwrap();
        assert!(frontend.completed_segment.is_some());
        assert!(frontend.snapshot.is_frontend);
        assert!(!frontend.snapshot.is_timing);
    }

    #[test]
    fn returning_to_town_splits_the_next_wilderness_into_a_new_journey() {
        let mut tracker = tracker();
        tracker
            .observe_location(area(6), 0, 0, "first".to_string())
            .unwrap();
        let first = tracker
            .observe_location(area(1), 48_000, 1_000, "town".to_string())
            .unwrap()
            .completed_segment
            .unwrap();
        tracker
            .observe_location(area(6), 96_000, 2_000, "second".to_string())
            .unwrap();
        let second = tracker
            .observe_location(area(1), 144_000, 3_000, "town".to_string())
            .unwrap()
            .completed_segment
            .unwrap();
        assert_ne!(first.journey_id, second.journey_id);
        assert_eq!(second.segment_index, 0);
    }

    #[test]
    fn repeated_heartbeat_is_idempotent() {
        let mut tracker = tracker();
        let first = tracker
            .observe_location(area(6), 100, 100, "start".to_string())
            .unwrap();
        let repeated = tracker
            .observe_location(area(6), 50_000, 1_100, "repeat".to_string())
            .unwrap();
        assert!(first.changed);
        assert!(!repeated.changed);
        assert_eq!(first.snapshot.revision, repeated.snapshot.revision);
    }

    #[test]
    fn rune_outside_wilderness_stays_raw() {
        let mut tracker = tracker();
        tracker.observe_rune(TrackedRuneDrop {
            observation_id: 1,
            rune_number: 1,
            rune_name: "艾尔".to_string(),
            rune_name_en: "El".to_string(),
        });
        let started = tracker
            .observe_location(area(21), 1_000, 1_000, "start".to_string())
            .unwrap();
        assert!(started.snapshot.current_run_drops.is_empty());
    }

    #[test]
    fn transition_gate_supports_areas_and_frontend() {
        let mut gate = SceneTransitionGate::new(48_000);
        assert!(gate.observe(area(1), 0));
        assert!(!gate.observe(area(1), 24_000));
        assert!(gate.observe(TelemetryMarker::Frontend, 48_000));
        assert!(gate.observe(area(1), 60_000));
        assert!(gate.observe(TelemetryMarker::Frontend, 72_000));
        assert!(!gate.observe(TelemetryMarker::Rune { rune_number: 1 }, 96_000));
    }

    #[test]
    fn competing_locations_are_accepted_without_a_confirmation_delay() {
        let mut gate = SceneTransitionGate::new(48_000);
        assert!(gate.observe(area(6), 0));
        assert!(!gate.observe(area(6), 48_000 * 4));
        assert!(gate.observe(area(21), 48_000 * 5));
        assert!(gate.observe(area(6), 48_000 * 6));
    }
}
