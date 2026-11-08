use super::{Candidate, Kind, Vision};
use crate::rune_audio::catalog::{scene_name_identity, LocationCatalog, TelemetryMarker};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};
use tauri::Manager;
pub enum Evidence {
    Area(Candidate),
}
struct Pending {
    config_epoch: u64,
    created: Instant,
    rx: mpsc::Receiver<Result<Option<Candidate>, String>>,
}
#[derive(Default)]
pub struct AudioVision {
    pending: Vec<Pending>,
    next_area: Option<Instant>,
    area_candidates: Option<Vec<Candidate>>,
    fusion: SceneFusion,
}

/// Both channels map independently. Cached OCR is not a new entry, and a
/// conflicting audio loop must persist before replacing a visual entry.
#[derive(Default)]
struct SceneFusion {
    current: Option<(Candidate, Instant)>,
    visual_entry: Option<(String, Option<u64>)>,
    pending_audio: Option<(String, Instant)>,
}

fn scene_key(candidate: &Candidate) -> Option<String> {
    let location = candidate.location.clone()?.statistics_location();
    Some(format!(
        "{:?}:{}",
        location.kind,
        scene_name_identity(&location.scene_name)
    ))
}

impl SceneFusion {
    fn accept(&mut self, candidate: Candidate, observed: Instant) -> Option<Candidate> {
        let key = scene_key(&candidate)?;
        if self.current.as_ref().is_some_and(|(_, at)| observed < *at) {
            return None;
        }
        if candidate.audio_mapped {
            if let Some((current, at)) = &self.current {
                if scene_key(current).as_ref() == Some(&key) {
                    self.pending_audio = None;
                    if candidate.tz && !current.tz {
                        let mut refined = current.clone();
                        refined.tz = true;
                        self.current = Some((refined.clone(), *at));
                        return Some(refined);
                    }
                    return None;
                }
                if !current.audio_mapped {
                    let pending = self
                        .pending_audio
                        .get_or_insert_with(|| (key.clone(), observed));
                    if pending.0 != key
                        || observed.saturating_duration_since(pending.1) > Duration::from_secs(6)
                    {
                        *pending = (key, observed);
                        return None;
                    }
                    if observed.saturating_duration_since(*at) < Duration::from_secs(2)
                        || observed.saturating_duration_since(pending.1)
                            < Duration::from_millis(500)
                    {
                        return None;
                    }
                }
            }
        } else {
            let entry = (candidate.name.clone(), candidate.announcement);
            let fresh = self.visual_entry.as_ref() != Some(&entry);
            if let Some((current, at)) = &self.current {
                if observed < *at {
                    return None;
                }
                if !fresh {
                    if scene_key(current).as_ref() != Some(&key) || !candidate.tz || current.tz {
                        return None;
                    }
                    let mut refined = current.clone();
                    refined.tz = true;
                    self.current = Some((refined.clone(), *at));
                    return Some(refined);
                }
            }
            self.visual_entry = Some(entry);
        }
        self.pending_audio = None;
        self.current = Some((candidate.clone(), observed));
        Some(candidate)
    }
}

fn area_candidates(catalog: &LocationCatalog) -> Vec<Candidate> {
    let mut scenes = HashMap::new();
    for location in catalog.named_locations() {
        let mut aliases = super::area_names::aliases(&location.scene_name_en);
        aliases.extend([location.scene_name.clone(), location.scene_name_en.clone()]);
        let key = (scene_name_identity(&location.scene_name), location.kind);
        // Keep all known translations, even when distinct directory entries use
        // different English names for the same displayed scene.
        let candidate = scenes.entry(key).or_insert_with(|| Candidate {
            name: location.scene_name.clone(),
            name_en: location.scene_name_en.clone(),
            location: Some(location),
            ..Default::default()
        });
        candidate.aliases.extend(aliases);
    }
    let mut scenes: Vec<_> = scenes.into_values().collect();
    for candidate in &mut scenes {
        candidate.aliases.sort();
        candidate.aliases.dedup();
    }
    scenes.sort_by(|a, b| a.name.cmp(&b.name));
    scenes
}

impl AudioVision {
    pub fn observe_audio_area(
        &mut self,
        marker: TelemetryMarker,
        catalog: &LocationCatalog,
        confidence: f32,
    ) -> Option<Candidate> {
        let location = catalog.resolve(marker)?;
        let named = catalog
            .named_locations()
            .into_iter()
            .find(|v| v.scene_name == location.scene_name && v.kind == location.kind);
        let named = named?;
        let candidate = Candidate {
            name: named.scene_name.clone(),
            name_en: named.scene_name_en.clone(),
            location: Some(named),
            confidence,
            audio_mapped: true,
            ..Default::default()
        };
        self.fusion.accept(candidate, Instant::now())
    }
    pub fn tick_area(&mut self, app: &tauri::AppHandle, pid: u32, catalog: &LocationCatalog) {
        if !super::configuration().area_enabled
            || self.next_area.is_some_and(|next| Instant::now() < next)
        {
            return;
        }
        self.next_area = Some(Instant::now() + Duration::from_secs(1));
        if !self.pending.is_empty() {
            return;
        }
        let candidates = self
            .area_candidates
            .get_or_insert_with(|| area_candidates(catalog))
            .clone();
        self.submit(
            app,
            pid,
            Kind::Area(
                candidates,
                self.fusion.current.as_ref().map(|(v, _)| v.clone()),
            ),
        );
    }
    fn submit(&mut self, app: &tauri::AppHandle, pid: u32, kind: Kind) {
        let diagnostic = super::configuration().diagnostic_enabled;
        if self.pending.len() >= 8 {
            super::diagnostics::event(
                diagnostic,
                "submit_rejected",
                serde_json::json!({"pid":pid,"reason":"pending_full"}),
            );
            return;
        }
        let Some(service) = app.try_state::<Arc<Vision>>() else {
            return;
        };
        match service.submit(pid, kind) {
            Ok(rx) => {
                super::diagnostics::event(diagnostic, "submitted", serde_json::json!({"pid":pid}));
                self.pending.push(Pending {
                    config_epoch: super::config_epoch(),
                    created: Instant::now(),
                    rx,
                });
            }
            Err(error) => {
                super::diagnostics::event(
                    diagnostic,
                    "submit_rejected",
                    serde_json::json!({"pid":pid,"error":super::diagnostics::short(&error)}),
                );
                service.status.lock().message = error;
            }
        }
    }
    pub fn poll(&mut self) -> Vec<Evidence> {
        let c = super::configuration();
        let mut out = Vec::new();
        self.pending.retain(|p| {
            if p.config_epoch != super::config_epoch()
                || p.created.elapsed() > Duration::from_secs(6)
            {
                super::diagnostics::event(c.diagnostic_enabled,"result_discarded",serde_json::json!({"age_ms":p.created.elapsed().as_secs_f64()*1000.0,"reason":if p.config_epoch!=super::config_epoch(){"config_changed"}else{"timeout"}}));
                return false;
            }
            match p.rx.try_recv() {
                Ok(Ok(Some(candidate))) => {
                    if c.area_enabled {
                        if let Some(candidate) = self.fusion.accept(candidate, p.created) {
                            out.push(Evidence::Area(candidate));
                        }
                    }
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                other => {
                    let reason=match other {Ok(Ok(None))=>"unmatched",Ok(Err(_))=>"unavailable",Err(_)=>"worker_disconnected",_=>"discarded"};
                    super::diagnostics::event(c.diagnostic_enabled,"result_not_applied",serde_json::json!({"reason":reason}));
                    false
                },
            }
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rune_audio::catalog::{
        AreaCatalogEntry, AreaCatalogFile, LocationKind, AREA_CATALOG_FILE_NAME,
    };

    fn evidence(name: &str, audio: bool, announcement: u64) -> Candidate {
        Candidate {
            name: name.into(),
            location: Some(crate::rune_audio::catalog::NamedLocation {
                scene_name: name.into(),
                scene_name_en: name.into(),
                kind: LocationKind::Wilderness,
            }),
            audio_mapped: audio,
            announcement: (!audio).then_some(announcement),
            confidence: 0.95,
            ..Default::default()
        }
    }

    #[test]
    fn audio_scene_accepts_tz_once_and_rejects_old_tz_after_transition() {
        let mut fusion = SceneFusion::default();
        let now = Instant::now();
        let mut first = evidence("洞穴", true, 0);
        assert!(fusion.accept(first.clone(), now).is_some());
        first.tz = true;
        assert!(fusion.accept(first.clone(), now).unwrap().tz);
        assert!(fusion.accept(first.clone(), now).is_none());
        assert!(fusion
            .accept(evidence("石块旷野", false, 2), now + Duration::from_secs(1))
            .is_some());
        assert!(fusion.accept(first, now).is_none());
    }

    #[test]
    fn independent_channels_accept_either_source_without_three_second_ocr_lockout() {
        let now = Instant::now();
        let mut fusion = SceneFusion::default();
        assert!(fusion.accept(evidence("先祖之路", true, 0), now).is_some());
        // A new visual entry is useful even immediately after an audio entry.
        assert!(fusion
            .accept(
                evidence("水晶通道", false, 1),
                now + Duration::from_millis(100)
            )
            .is_some());
        assert!(fusion
            .accept(evidence("水晶通道", false, 1), now + Duration::from_secs(1))
            .is_none());
        let mut visual_only = SceneFusion::default();
        assert!(visual_only
            .accept(evidence("先祖之路", false, 2), now)
            .is_some());
    }

    #[test]
    fn conflicting_audio_requires_fresh_confirmation_and_cached_ocr_cannot_revert_it() {
        let now = Instant::now();
        let mut fusion = SceneFusion::default();
        let visual = evidence("先祖之路", false, 1);
        fusion.accept(visual.clone(), now).unwrap();
        let audio = evidence("水晶通道", true, 0);
        assert!(fusion
            .accept(audio.clone(), now + Duration::from_secs(2))
            .is_none());
        assert!(fusion
            .accept(audio.clone(), now + Duration::from_millis(2100))
            .is_none());
        assert!(fusion.accept(audio, now + Duration::from_secs(3)).is_some());
        assert!(fusion
            .accept(visual, now + Duration::from_secs(4))
            .is_none());
        assert!(fusion
            .accept(evidence("先祖之路", false, 2), now + Duration::from_secs(5))
            .is_some());
    }

    #[test]
    fn stale_inflight_ocr_is_ignored_and_late_tz_refines_current_entry() {
        let now = Instant::now();
        let mut fusion = SceneFusion::default();
        fusion
            .accept(evidence("水晶通道", true, 0), now + Duration::from_secs(1))
            .unwrap();
        assert!(fusion.accept(evidence("先祖之路", false, 1), now).is_none());
        let mut visual = evidence("水晶通道", false, 2);
        fusion
            .accept(visual.clone(), now + Duration::from_secs(2))
            .unwrap();
        visual.tz = true;
        assert!(
            fusion
                .accept(visual.clone(), now + Duration::from_secs(3))
                .unwrap()
                .tz
        );
        assert!(fusion
            .accept(visual, now + Duration::from_secs(4))
            .is_none());
    }

    #[test]
    fn floor_evidence_has_one_statistical_identity() {
        let now = Instant::now();
        let mut fusion = SceneFusion::default();
        let first = evidence("地下墓穴一层", false, 1);
        let second = evidence("地下墓穴二层", true, 0);
        assert_eq!(scene_key(&first), scene_key(&second));
        fusion.accept(first, now).unwrap();
        assert!(fusion
            .accept(second, now + Duration::from_secs(1))
            .is_none());
        assert!(fusion
            .accept(
                evidence("地下墓穴二层", false, 2),
                now + Duration::from_secs(2)
            )
            .is_some());
    }

    fn catalog_with_names(names: &[(u32, &str, &str)]) -> LocationCatalog {
        let directory = std::env::temp_dir().join(format!("named-scenes-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let file = AreaCatalogFile {
            protocol_version: crate::rune_audio::protocol::PROTOCOL_VERSION,
            source_levels: "named scene regression fixture".into(),
            areas: names
                .iter()
                .map(|(id, name, english)| AreaCatalogEntry {
                    area_id: *id,
                    scene_key: format!("area_{id}"),
                    scene_name: (*name).into(),
                    scene_name_en: (*english).into(),
                    kind: if *id == 40 {
                        LocationKind::Town
                    } else {
                        LocationKind::Wilderness
                    },
                })
                .collect(),
        };
        let path = directory.join(AREA_CATALOG_FILE_NAME);
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let catalog = LocationCatalog::load_from_directory(&directory).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
        catalog
    }

    #[test]
    fn audio_accepts_only_named_locations_and_repeated_ambience_does_not_renew_priority() {
        let catalog = catalog_with_names(&[
            (104, "绝望平原", "Plains of Despair"),
            (105, "地图 Area 105", "Area 105"),
        ]);
        let mut vision = AudioVision::default();
        let (_tx, rx) = mpsc::channel();
        vision.pending.push(Pending {
            config_epoch: 0,
            created: Instant::now(),
            rx,
        });
        assert!(vision
            .observe_audio_area(TelemetryMarker::Area { area_id: 104 }, &catalog, 0.95)
            .is_some());
        let first = vision.fusion.current.as_ref().unwrap().1;
        assert!(vision
            .observe_audio_area(TelemetryMarker::Area { area_id: 104 }, &catalog, 0.95)
            .is_none());
        assert_eq!(vision.fusion.current.as_ref().unwrap().1, first);
        assert!(vision
            .observe_audio_area(TelemetryMarker::Area { area_id: 105 }, &catalog, 0.95)
            .is_none());
        assert_eq!(vision.fusion.current.as_ref().unwrap().0.name, "绝望平原");
        assert_eq!(vision.pending.len(), 1, "unknown audio must not cancel OCR");
    }

    #[test]
    fn log_regression_tombs_and_sewers_split_timers_by_name_without_area_ids() {
        use crate::rune_audio::tracking::SegmentTracker;
        let catalog = catalog_with_names(&[
            (40, "鲁·高因", "Lut Gholein"),
            (46, "法师峡谷", "Canyon of the Magi"),
            (47, "下水道一层", "Sewers Level 1"),
            (66, "塔·拉夏之墓", "Tal Rasha's Tomb"),
            (67, "塔·拉夏之墓", "Tal Rasha's Tomb"),
            (68, "塔·拉夏之墓", "Tal Rasha's Tomb"),
            (69, "塔·拉夏之墓", "Tal Rasha's Tomb"),
            (70, "塔·拉夏之墓", "Tal Rasha's Tomb"),
            (71, "塔·拉夏之墓", "Tal Rasha's Tomb"),
            (72, "塔·拉夏之墓", "Tal Rasha's Tomb"),
            (73, "塔·拉夏的墓室", "Tal Rasha's Chamber"),
            (92, "下水道一层", "Kurast Sewers Level 1"),
        ]);
        let candidates = area_candidates(&catalog);
        for name in ["塔·拉夏之墓", "下水道一层"] {
            assert_eq!(candidates.iter().filter(|v| v.name == name).count(), 1);
        }
        let resolve = |text| {
            super::super::area::resolve(text, &candidates)
                .unwrap()
                .location
                .unwrap()
        };
        let tomb = resolve("进入塔·拉夏之墓");
        assert_eq!(resolve("进入塔拉夏之墓"), tomb);
        assert_eq!(resolve("進入塔拉夏的古墓"), tomb);
        assert_eq!(resolve("Entering Tal Rasha's Tomb"), tomb);
        assert_ne!(resolve("进入塔·拉夏的墓室"), tomb);
        let sewer = resolve("进入下水道一层");
        assert_eq!(resolve("Entering Kurast Sewers Level 1"), sewer);
        assert_eq!(resolve("Entering Sewers Level 1"), sewer);

        let mut tracker =
            SegmentTracker::with_catalog("test".into(), "test".into(), 48_000, catalog);
        let canyon = tracker.observe_visual_location(
            resolve("进入法师峡谷"),
            true,
            Some(1),
            1_000,
            "start".into(),
        );
        assert_eq!(canyon.snapshot.current_area_id, None);
        let entered =
            tracker.observe_visual_location(tomb.clone(), true, Some(2), 87_000, "tomb".into());
        let canyon = entered.completed_segment.unwrap();
        assert_eq!(canyon.scene_name, "法师峡谷");
        assert_eq!(canyon.timer_seconds, 86.0);
        assert_eq!(entered.snapshot.current_scene, "塔·拉夏之墓");
        assert_eq!(entered.snapshot.current_area_id, None);
        let repeat = tracker.observe_visual_location(tomb, true, Some(2), 88_000, "repeat".into());
        assert!(!repeat.changed);
        assert_eq!(repeat.snapshot.timer_started_at_ms, Some(87_000));
        let town = tracker.observe_visual_location(
            resolve("进入鲁高因"),
            false,
            Some(3),
            513_700,
            "town".into(),
        );
        let tomb = town.completed_segment.unwrap();
        assert_eq!(tomb.scene_name, "塔·拉夏之墓");
        assert_eq!(tomb.timer_seconds, 426.7);
        assert!(!town.snapshot.is_timing);
        assert_eq!(town.snapshot.current_area_id, None);
        tracker.observe_visual_location(sewer, false, Some(4), 600_000, "sewer".into());
        let stopped = tracker.observe_visual_location(
            resolve("进入鲁高因"),
            false,
            Some(5),
            610_000,
            "town".into(),
        );
        let sewer = stopped.completed_segment.unwrap();
        assert_eq!(sewer.scene_name, "下水道");
        assert_eq!(sewer.timer_seconds, 10.0);
    }

    #[test]
    fn localized_ocr_resolves_named_scenes_and_merges_duplicate_directory_entries() {
        let directory = std::env::temp_dir().join(format!("area-aliases-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let file = AreaCatalogFile {
            protocol_version: crate::rune_audio::protocol::PROTOCOL_VERSION,
            source_levels: "translation regression fixture".into(),
            areas: [
                (8, "邪恶洞穴", "Den of Evil"),
                (9, "洞穴一层", "Cave Level 1"),
                (12, "深坑一层", "Pit Level 1"),
                (16, "深坑二层", "Pit Level 2"),
                (38, "崔斯特姆", "Tristram"),
                (136, "崔斯特姆", "Tristram"),
            ]
            .into_iter()
            .map(|(area_id, chinese, english)| AreaCatalogEntry {
                area_id,
                scene_key: format!("area_{area_id}"),
                scene_name: chinese.into(),
                scene_name_en: english.into(),
                kind: LocationKind::Wilderness,
            })
            .collect(),
        };
        let path = directory.join(AREA_CATALOG_FILE_NAME);
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let catalog = LocationCatalog::load_from_directory(&directory).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
        let candidates = area_candidates(&catalog);
        for (text, area_id) in [
            ("进入流亡者营地", 1),
            ("进入罗格营地", 1),
            ("进入萝格营地", 1),
            ("進入俠盜營地", 1),
            ("进入深渊一层", 12),
            ("进入深坑一层", 12),
            ("进入“地穴 第一层”", 12),
            ("進入地穴第一層", 12),
            ("进入地穴第一层[07/39/85]", 12),
            ("进入深渊二层", 16),
            ("進入地穴第二層", 16),
        ] {
            let found = super::super::area::resolve(text, &candidates)
                .unwrap_or_else(|| panic!("unmatched: {text}"));
            let expected = catalog.resolve(TelemetryMarker::Area { area_id }).unwrap();
            assert_eq!(found.location.unwrap().scene_name, expected.scene_name);
        }
        let tristram = super::super::area::resolve("进入崔斯特姆", &candidates).unwrap();
        assert_eq!(tristram.location.unwrap().scene_name, "崔斯特姆");
        assert_eq!(
            candidates.iter().filter(|v| v.name == "崔斯特姆").count(),
            1
        );
        for text in [
            "深渊一层",
            "进入深渊三层",
            "进入深渊立层",
            "进入外廊",
            "进入洞穴一层",
        ] {
            assert!(
                super::super::area::resolve(text, &candidates).is_none(),
                "{text}"
            );
        }
    }
}
