use super::catalog::{
    LocationCatalog, LocationKind, ResolvedLocation, TelemetryMarker, MAX_AREA_ID, MAX_ITEM_ID,
    RUNE_COUNT,
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

    /// The generic TZ marker represents a new logical location. Clearing the
    /// exact-area latch lets a concurrently playing exact marker win on its
    /// next heartbeat without allowing TZ heartbeats to fight it afterwards.
    pub fn clear(&mut self) {
        self.confirmed = None;
    }
}

/// Converts the continuously repeating generic TZ marker into one logical
/// activation. A later exact area remains authoritative until the TZ marker
/// has disappeared long enough to represent a genuinely new activation.
#[derive(Debug, Clone)]
pub struct TerrorZonePresenceGate {
    sample_rate: u32,
    last_seen: Option<u64>,
}

impl TerrorZonePresenceGate {
    const ABSENCE_SECONDS: f32 = 4.0;

    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            last_seen: None,
        }
    }

    pub fn observe(&mut self, observed_at_frame: u64) -> bool {
        let absence_frames = (self.sample_rate as f32 * Self::ABSENCE_SECONDS) as u64;
        let is_new_activation = self
            .last_seen
            .is_none_or(|last| observed_at_frame.saturating_sub(last) >= absence_frames);
        self.last_seen = Some(observed_at_frame);
        is_new_activation
    }
}

/// Converts repeated Flippy packets into one logical ground-presence event.
/// A rune becomes eligible again after its heartbeat has been absent long
/// enough for two ordinary ground animation cycles to have been missed.
#[derive(Debug, Clone)]
pub struct DropPresenceGate {
    sample_rate: u32,
    last_seen: HashMap<TelemetryMarker, u64>,
}

impl DropPresenceGate {
    const ABSENCE_SECONDS: f32 = 6.0;

    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            last_seen: HashMap::new(),
        }
    }

    /// Returns true only when this drop identity was not recently present on the ground.
    pub fn observe(&mut self, marker: TelemetryMarker, observed_at_frame: u64) -> bool {
        if !matches!(
            marker,
            TelemetryMarker::Rune { rune_number } if (1..=RUNE_COUNT).contains(&rune_number)
        ) && !matches!(
            marker,
            TelemetryMarker::Item { item_id } if (1..=MAX_ITEM_ID).contains(&item_id)
        ) {
            return false;
        }
        let absence_frames = (self.sample_rate as f32 * Self::ABSENCE_SECONDS) as u64;
        let is_new_presence = self
            .last_seen
            .get(&marker)
            .is_none_or(|last| observed_at_frame.saturating_sub(*last) >= absence_frames);
        self.last_seen.insert(marker, observed_at_frame);
        is_new_presence
    }

    /// A confirmed scene transition removes every ground item from scope.
    pub fn clear(&mut self) {
        self.last_seen.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackedDropKind {
    Rune,
    Item,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackedDrop {
    pub observation_id: i64,
    pub kind: TrackedDropKind,
    pub telemetry_id: u32,
    pub code: Option<String>,
    pub category: String,
    pub name: String,
    pub name_en: String,
    pub rune_number: Option<u32>,
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
    pub tz: bool,
    pub journey_id: String,
    pub segment_index: u32,
    pub timer_seconds: f64,
    pub drops: Vec<TrackedDrop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackingSnapshot {
    pub revision: u64,
    pub account_id: String,
    pub current_area_id: Option<u32>,
    pub current_scene: String,
    pub current_scene_en: String,
    #[serde(default)]
    pub tz: bool,
    pub location_kind: Option<LocationKind>,
    pub is_town: bool,
    pub is_frontend: bool,
    pub is_timing: bool,
    pub timer_started_at_ms: Option<i64>,
    pub current_run_key: Option<String>,
    pub current_run_name: Option<String>,
    pub current_run_name_en: Option<String>,
    pub current_run_drops: Vec<TrackedDrop>,
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
    tz: bool,
    journey_id: String,
    segment_index: u32,
    started_at_frame: u64,
    started_at_ms: i64,
    absolute_time: String,
    drops: Vec<TrackedDrop>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrackedLocation {
    marker: Option<TelemetryMarker>,
    area_id: Option<u32>,
    scene_key: String,
    scene_name: String,
    scene_name_en: String,
    kind: LocationKind,
    tz: bool,
}

impl TrackedLocation {
    fn exact(location: ResolvedLocation) -> Self {
        let area_id = match location.marker {
            TelemetryMarker::Area { area_id } => Some(area_id),
            TelemetryMarker::Rune { .. }
            | TelemetryMarker::Item { .. }
            | TelemetryMarker::Frontend => None,
        };
        Self {
            marker: Some(location.marker),
            area_id,
            scene_key: location.scene_key,
            scene_name: location.scene_name,
            scene_name_en: location.scene_name_en,
            kind: location.kind,
            tz: false,
        }
    }

    fn terror_zone(scene_name: String, scene_name_en: String) -> Self {
        let scene_name = if scene_name.trim().is_empty() {
            "恐怖区域".to_string()
        } else {
            scene_name.trim().to_string()
        };
        let scene_name_en = if scene_name_en.trim().is_empty() {
            "Terror Zone".to_string()
        } else {
            scene_name_en.trim().to_string()
        };
        Self {
            marker: None,
            area_id: None,
            scene_key: format!("terror_zone:{scene_name}"),
            scene_name,
            scene_name_en,
            kind: LocationKind::Wilderness,
            tz: true,
        }
    }
}

/// The deep timing module: callers submit confirmed locations and rune drops;
/// town/frontend stopping, wilderness splitting and journey continuity stay hidden.
#[derive(Debug, Clone)]
pub struct SegmentTracker {
    account_id: String,
    character_name: String,
    current_location: Option<TrackedLocation>,
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
        Ok(self.observe_tracked_location(
            TrackedLocation::exact(location),
            observed_at_frame,
            observed_at_ms,
            absolute_time,
        ))
    }

    pub fn observe_terror_zone(
        &mut self,
        scene_name: String,
        scene_name_en: String,
        observed_at_frame: u64,
        observed_at_ms: i64,
        absolute_time: String,
    ) -> TrackingOutcome {
        self.observe_tracked_location(
            TrackedLocation::terror_zone(scene_name, scene_name_en),
            observed_at_frame,
            observed_at_ms,
            absolute_time,
        )
    }

    fn observe_tracked_location(
        &mut self,
        location: TrackedLocation,
        observed_at_frame: u64,
        observed_at_ms: i64,
        absolute_time: String,
    ) -> TrackingOutcome {
        if self.current_location.as_ref() == Some(&location) {
            return TrackingOutcome {
                changed: false,
                snapshot: self.snapshot(),
                completed_segment: None,
            };
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

        self.current_location = Some(location);
        self.revision += 1;
        TrackingOutcome {
            changed: true,
            snapshot: self.snapshot(),
            completed_segment,
        }
    }

    pub fn observe_drop(&mut self, drop: TrackedDrop) -> TrackingSnapshot {
        if let Some(active_segment) = &mut self.active_segment {
            active_segment.drops.push(drop);
            self.revision += 1;
        }
        self.snapshot()
    }

    /// Drops are recordable only after a wilderness/dungeon segment is active.
    /// Town, frontend and the pre-location state intentionally reject them.
    pub fn accepts_drop_observations(&self) -> bool {
        self.active_segment.is_some()
    }

    pub fn snapshot(&self) -> TrackingSnapshot {
        let location = self.current_location.as_ref();
        TrackingSnapshot {
            revision: self.revision,
            account_id: self.account_id.clone(),
            current_area_id: location.and_then(|item| item.area_id),
            current_scene: location
                .as_ref()
                .map(|item| item.scene_name.clone())
                .unwrap_or_else(|| "等待识别...".to_string()),
            current_scene_en: location
                .as_ref()
                .map(|item| item.scene_name_en.clone())
                .unwrap_or_else(|| "Waiting for location...".to_string()),
            tz: location.is_some_and(|item| item.tz),
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
        location: &TrackedLocation,
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
            tz: location.tz,
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
            tz: active.tz,
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

    fn rune_drop(observation_id: i64, rune_number: u32, name: &str, name_en: &str) -> TrackedDrop {
        TrackedDrop {
            observation_id,
            kind: TrackedDropKind::Rune,
            telemetry_id: rune_number,
            code: Some(format!("r{rune_number:02}")),
            category: "runes".to_string(),
            name: name.to_string(),
            name_en: name_en.to_string(),
            rune_number: Some(rune_number),
        }
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
        tracker.observe_drop(rune_drop(7, 24, "伊斯特", "Ist"));

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
    fn generic_terror_zone_starts_a_tz_segment_and_exact_area_can_win() {
        let mut tracker = tracker();
        let terror_zone = tracker.observe_terror_zone(
            "营房".to_string(),
            "Terror Zone".to_string(),
            0,
            0,
            "2026/08/28/12:00:00".to_string(),
        );
        assert!(terror_zone.changed);
        assert!(terror_zone.snapshot.tz);
        assert_eq!(terror_zone.snapshot.current_scene, "营房");
        assert_eq!(
            terror_zone.snapshot.current_run_key.as_deref(),
            Some("terror_zone:营房")
        );

        let repeated = tracker.observe_terror_zone(
            "营房".to_string(),
            "Terror Zone".to_string(),
            24_000,
            500,
            "ignored".to_string(),
        );
        assert!(!repeated.changed);

        let exact = tracker
            .observe_location(area(6), 48_000, 1_000, "exact".to_string())
            .unwrap();
        assert!(!exact.snapshot.tz);
        assert_eq!(exact.snapshot.current_scene, "黑色荒地");
        let completed = exact.completed_segment.unwrap();
        assert!(completed.tz);
        assert_eq!(completed.scene_name, "营房");
        assert_eq!(completed.timer_seconds, 1.0);
    }

    #[test]
    fn drops_are_accepted_only_during_a_wilderness_segment() {
        let mut tracker = tracker();
        assert!(!tracker.accepts_drop_observations());
        tracker.observe_drop(rune_drop(1, 1, "艾尔", "El"));

        tracker
            .observe_location(area(1), 500, 500, "town".to_string())
            .unwrap();
        assert!(!tracker.accepts_drop_observations());

        let started = tracker
            .observe_location(area(21), 1_000, 1_000, "start".to_string())
            .unwrap();
        assert!(tracker.accepts_drop_observations());
        assert!(started.snapshot.current_run_drops.is_empty());

        tracker
            .observe_location(TelemetryMarker::Frontend, 2_000, 2_000, "menu".to_string())
            .unwrap();
        assert!(!tracker.accepts_drop_observations());
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

    #[test]
    fn rune_presence_heartbeats_emit_once_until_the_signal_is_absent() {
        let mut gate = DropPresenceGate::new(48_000);
        let marker = TelemetryMarker::Rune { rune_number: 7 };
        assert!(gate.observe(marker, 0));
        assert!(!gate.observe(marker, 48_000 * 2));
        assert!(!gate.observe(marker, 48_000 * 5));
        assert!(gate.observe(marker, 48_000 * 12));
    }

    #[test]
    fn rune_presence_is_independent_per_rune_and_resets_on_scene_change() {
        let mut gate = DropPresenceGate::new(48_000);
        let rune7 = TelemetryMarker::Rune { rune_number: 7 };
        let rune20 = TelemetryMarker::Rune { rune_number: 20 };
        assert!(gate.observe(rune7, 0));
        assert!(gate.observe(rune20, 100));
        assert!(!gate.observe(rune7, 200));
        gate.clear();
        assert!(gate.observe(rune7, 300));
        assert!(!gate.observe(TelemetryMarker::Rune { rune_number: 0 }, 400));
        assert!(!gate.observe(TelemetryMarker::Rune { rune_number: 34 }, 500));
        assert!(gate.observe(TelemetryMarker::Item { item_id: 40 }, 600));
    }
}
