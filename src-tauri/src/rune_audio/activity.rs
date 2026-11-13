use super::catalog::LocationKind;
use std::time::{Duration, Instant};

// Recipe r8 emits every 500 ms. Allow a missed heartbeat plus capture jitter.
const EXCLUSION_LEASE: Duration = Duration::from_millis(1500);

/// One game's accumulated audible time outside town and frontend.
#[derive(Debug, Clone)]
pub struct Activity {
    pub elapsed: Duration,
    pub running: bool,
    pub excluded: Option<(LocationKind, Instant)>,
    last_tick: Instant,
    sound_until: Instant,
    pub entered_game: bool,
    last_kind: Option<LocationKind>,
}

impl Activity {
    pub fn new(now: Instant) -> Self {
        Self {
            elapsed: Duration::ZERO,
            running: false,
            excluded: None,
            last_tick: now,
            sound_until: now,
            entered_game: false,
            last_kind: None,
        }
    }

    pub fn mark(&mut self, kind: LocationKind, now: Instant) {
        if kind != LocationKind::Frontend
            && self
                .excluded
                .is_some_and(|(current, until)| current == LocationKind::Frontend && now < until)
        {
            return;
        }
        match kind {
            LocationKind::Town | LocationKind::Wilderness => self.entered_game = true,
            LocationKind::Frontend => self.entered_game = false,
        }
        if matches!(kind, LocationKind::Town | LocationKind::Frontend) {
            self.excluded = Some((kind, now + EXCLUSION_LEASE));
        }
    }

    pub fn tick(&mut self, now: Instant, audible: bool) -> bool {
        if self.running {
            let end = now.min(self.sound_until);
            self.elapsed += end.saturating_duration_since(self.last_tick);
        }
        self.last_tick = now;
        if audible {
            self.sound_until = now + Duration::from_millis(750);
        }
        if self
            .excluded
            .is_some_and(|(kind, until)| kind != LocationKind::Frontend && now >= until)
        {
            self.excluded = None;
        }
        let running = self.entered_game && self.excluded.is_none() && now < self.sound_until;
        let kind = self.excluded.map(|(kind, _)| kind);
        let changed = running != self.running || kind != self.last_kind;
        self.last_kind = kind;
        self.running = running;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn town_tolerates_one_missing_heartbeat_then_leaves_within_one_and_a_half_seconds() {
        let now = Instant::now();
        let mut timer = Activity::new(now);
        timer.mark(LocationKind::Town, now);
        timer.tick(now + Duration::from_millis(1000), true);
        assert!(!timer.running);
        timer.mark(LocationKind::Town, now + Duration::from_millis(1000));
        timer.tick(now + Duration::from_millis(2499), true);
        assert!(!timer.running);
        timer.tick(now + Duration::from_millis(2500), true);
        assert!(timer.running);
    }

    #[test]
    fn frontend_remains_paused_after_lease_but_town_can_replace_it() {
        let now = Instant::now();
        let mut timer = Activity::new(now);
        timer.mark(LocationKind::Frontend, now);
        timer.tick(now + Duration::from_secs(5), true);
        assert!(!timer.running);
        assert!(!timer.entered_game);
        timer.mark(LocationKind::Town, now + Duration::from_secs(5));
        timer.tick(now + Duration::from_secs(5), true);
        assert_eq!(timer.excluded.unwrap().0, LocationKind::Town);
        assert!(timer.entered_game);
    }
    #[test]
    fn accumulates_across_town_and_stops_when_capture_is_silent() {
        let start = Instant::now();
        let mut timer = Activity::new(start);
        timer.mark(LocationKind::Wilderness, start);
        timer.tick(start, true);
        timer.mark(LocationKind::Town, start + Duration::from_millis(500));
        timer.tick(start + Duration::from_millis(500), true);
        assert!(!timer.running);
        assert_eq!(timer.elapsed, Duration::from_millis(500));
        timer.tick(start + Duration::from_secs(7), true);
        timer.tick(start + Duration::from_secs(9), false);
        assert!(!timer.running);
        assert_eq!(timer.elapsed, Duration::from_millis(1250));
    }

    #[test]
    fn startup_and_frontend_sounds_never_start_combat() {
        let now = Instant::now();
        let mut timer = Activity::new(now);
        timer.tick(now, true);
        assert!(!timer.running);
        timer.mark(LocationKind::Town, now);
        timer.tick(now + Duration::from_secs(7), true);
        assert!(timer.running);
        timer.mark(LocationKind::Frontend, now + Duration::from_secs(8));
        timer.tick(now + Duration::from_secs(20), true);
        assert!(!timer.running);
    }

    #[test]
    fn frontend_has_priority_over_its_background_town_audio() {
        let now = Instant::now();
        let mut timer = Activity::new(now);
        timer.mark(LocationKind::Frontend, now);
        timer.mark(LocationKind::Town, now);
        timer.tick(now, true);
        assert_eq!(timer.excluded.unwrap().0, LocationKind::Frontend);
        assert!(!timer.running);
        timer.mark(LocationKind::Town, now + Duration::from_secs(7));
        assert!(timer.tick(now + Duration::from_secs(7), true));
        assert_eq!(timer.excluded.unwrap().0, LocationKind::Town);
        assert!(!timer.running);
    }
}
