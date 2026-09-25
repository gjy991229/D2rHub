//! Aggregate active seconds by their actual local date, with no key history.
use chrono::{Duration, Local};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Clone, Default)]
pub(crate) struct ActivityDelta {
    pub seconds: u64,
    pub inputs: u64,
}
pub(crate) type ActivityBatch = BTreeMap<String, ActivityDelta>;
struct Activity {
    previous_second: Option<u64>,
    pending: ActivityBatch,
}
static ACTIVITY: Mutex<Activity> = Mutex::new(Activity {
    previous_second: None,
    pending: BTreeMap::new(),
});
static ORIGIN: OnceLock<Instant> = OnceLock::new();

pub(crate) fn note_input(active: impl FnOnce() -> bool) {
    let second = ORIGIN.get_or_init(Instant::now).elapsed().as_secs() + 1;
    let mut activity = ACTIVITY.lock();
    // Check after acquiring the activity lock: disabling and pausing cannot
    // race with a late queued input and credit hidden time after re-enabling.
    if !active() {
        return;
    }
    let now = Local::now();
    let day = now.format("%Y-%m-%d").to_string();
    let delta = activity.pending.entry(day).or_default();
    delta.inputs = delta.inputs.saturating_add(1);
    let previous = activity.previous_second.replace(second);
    let Some(previous) = previous else {
        return;
    };
    let elapsed = second.saturating_sub(previous);
    if elapsed == 0 || elapsed > 30 {
        return;
    }
    for offset in 0..elapsed {
        let day = (now - Duration::seconds(offset as i64))
            .format("%Y-%m-%d")
            .to_string();
        let seconds = activity.pending.entry(day).or_default();
        seconds.seconds = seconds.seconds.saturating_add(1);
    }
}

pub(crate) fn pause() {
    ACTIVITY.lock().previous_second = None;
}
pub(crate) fn snapshot() -> ActivityBatch {
    ACTIVITY.lock().pending.clone()
}
pub(crate) fn consume(batch: &ActivityBatch) {
    let mut activity = ACTIVITY.lock();
    for (day, committed) in batch {
        if let Some(seconds) = activity.pending.get_mut(day) {
            seconds.seconds = seconds.seconds.saturating_sub(committed.seconds);
            seconds.inputs = seconds.inputs.saturating_sub(committed.inputs);
        }
    }
    activity
        .pending
        .retain(|_, delta| delta.seconds > 0 || delta.inputs > 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_bursts_count_individually_and_commits_retain_later_input() {
        ACTIVITY.lock().pending.clear();
        pause();
        note_input(|| false);
        assert!(snapshot().is_empty());
        note_input(|| true);
        note_input(|| true);
        let batch = snapshot();
        assert_eq!(batch.values().map(|v| v.inputs).sum::<u64>(), 2);
        note_input(|| true);
        consume(&batch);
        assert_eq!(snapshot().values().map(|v| v.inputs).sum::<u64>(), 1);
        consume(&snapshot());
        assert!(snapshot().is_empty());
        pause();
    }
}
