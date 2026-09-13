//! Aggregate active seconds by their actual local date, with no key history.
use chrono::{Duration, Local};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};
use std::time::Instant;

pub(crate) type ActivityBatch = BTreeMap<String, u64>;
struct Activity {
    previous_second: Option<u64>,
    pending: ActivityBatch,
}
static ACTIVITY: Mutex<Activity> = Mutex::new(Activity {
    previous_second: None,
    pending: BTreeMap::new(),
});
static ORIGIN: OnceLock<Instant> = OnceLock::new();
static OBSERVED_SECOND: AtomicU64 = AtomicU64::new(0);

pub(crate) fn note_input(active: impl FnOnce() -> bool) {
    let second = ORIGIN.get_or_init(Instant::now).elapsed().as_secs() + 1;
    if OBSERVED_SECOND.swap(second, Ordering::Relaxed) == second {
        return;
    }
    let mut activity = ACTIVITY.lock();
    // Check after acquiring the activity lock: disabling and pausing cannot
    // race with a late queued input and credit hidden time after re-enabling.
    if !active() {
        return;
    }
    let previous = activity.previous_second.replace(second);
    let Some(previous) = previous else {
        return;
    };
    let elapsed = second.saturating_sub(previous);
    if elapsed == 0 || elapsed > 30 {
        return;
    }
    let now = Local::now();
    for offset in 0..elapsed {
        let day = (now - Duration::seconds(offset as i64))
            .format("%Y-%m-%d")
            .to_string();
        let seconds = activity.pending.entry(day).or_default();
        *seconds = seconds.saturating_add(1);
    }
}

pub(crate) fn pause() {
    ACTIVITY.lock().previous_second = None;
    OBSERVED_SECOND.store(0, Ordering::Relaxed);
}
pub(crate) fn snapshot() -> ActivityBatch {
    ACTIVITY.lock().pending.clone()
}
pub(crate) fn consume(batch: &ActivityBatch) {
    let mut activity = ACTIVITY.lock();
    for (day, committed) in batch {
        if let Some(seconds) = activity.pending.get_mut(day) {
            *seconds = seconds.saturating_sub(*committed);
        }
    }
    activity.pending.retain(|_, seconds| *seconds > 0);
}
