//! Lazy wardrobe storage; no worker, timer, polling loop or input history.
use super::pet_activity::{self, ActivityBatch};
use crate::domain::pet::{Action, Reward, Wardrobe};
use crate::infrastructure::module_config::{ModuleConfigError, ModuleConfigStore};
use crate::state::SharedState;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

static MENU_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) fn set_menu_open(open: bool) {
    MENU_OPEN.store(open, Ordering::Relaxed);
}
pub(crate) fn menu_open() -> bool {
    MENU_OPEN.load(Ordering::Relaxed)
}

#[derive(Clone, Serialize)]
pub(crate) struct Snapshot {
    pub generation: u64,
    // Process-local ordering survives disk generation rollback after recovery.
    pub revision: u64,
    pub wardrobe: Wardrobe,
}
#[derive(Serialize)]
pub(crate) struct Outcome {
    pub snapshot: Snapshot,
    pub rewards: Vec<Reward>,
}
struct Runtime {
    store: ModuleConfigStore,
    snapshot: Snapshot,
}
static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

/// Lifecycle shutdown can flush an already initialized wardrobe even after the
/// module flag changes. It never creates storage for an unused companion.
pub(crate) fn flush_activity(app: &tauri::AppHandle) -> Result<(), String> {
    let mut runtime = RUNTIME.lock();
    let Some(runtime) = runtime.as_mut() else {
        return Ok(());
    };
    let activity = pet_activity::snapshot();
    if activity.is_empty() {
        return Ok(());
    }
    let mut next = runtime.snapshot.wardrobe.clone();
    for (day, seconds) in &activity {
        let _ = next.advance(*seconds, day, random_below);
    }
    commit(app, runtime, next, &activity)
}

fn commit(
    app: &tauri::AppHandle,
    runtime: &mut Runtime,
    next: Wardrobe,
    activity: &ActivityBatch,
) -> Result<(), String> {
    let saved = match runtime
        .store
        .save_if_generation(runtime.snapshot.generation, next)
    {
        Ok(saved) => saved,
        Err(error) => {
            if matches!(error, ModuleConfigError::GenerationConflict { .. }) {
                // Recovery or an external restore may have changed generation.
                // Reload the confirmed disk snapshot so a retry can make progress.
                if let Ok(Some(saved)) = runtime.store.load::<Wardrobe>() {
                    runtime.snapshot = Snapshot {
                        generation: saved.generation,
                        revision: runtime.snapshot.revision + 1,
                        wardrobe: saved.payload,
                    };
                    let _ = app.emit("pet-wardrobe-updated", &runtime.snapshot);
                }
            }
            return Err(error.to_string());
        }
    };
    runtime.snapshot = Snapshot {
        generation: saved.generation,
        revision: runtime.snapshot.revision + 1,
        wardrobe: saved.payload,
    };
    pet_activity::consume(activity);
    let _ = app.emit("pet-wardrobe-updated", &runtime.snapshot);
    Ok(())
}

fn random_below(bound: u32) -> u32 {
    // UUID v4 already uses the OS random source; no additional RNG dependency.
    let value = uuid::Uuid::new_v4().as_u128() as u64;
    (value % u64::from(bound.max(1))) as u32
}

pub(crate) fn transact(
    app: &tauri::AppHandle,
    state: &SharedState,
    action: Option<Action>,
    settle: bool,
) -> Result<Outcome, String> {
    let config = state.configuration().snapshot().ok_or("全局设置尚未加载")?;
    if !state.optional_runtime_ready()
        || !config.optional_module_runtime_allowed(crate::domain::config::OPTIONAL_MODULE_PET)
    {
        return Err("桌宠模块尚未启用".into());
    }
    let mut runtime = RUNTIME.lock();
    if runtime.is_none() {
        let store = ModuleConfigStore::new(&state.app_data_dir, "pet-wardrobe", 1)
            .map_err(|e| e.to_string())?;
        let saved = match store.load::<Wardrobe>().map_err(|e| e.to_string())? {
            Some(saved) => saved,
            None => store
                .save_with_legacy_import_if_generation(
                    0,
                    Wardrobe::from_legacy(&config.bongo_cat_skin, &config.bongo_cat_unlocked_skins),
                    serde_json::json!({"source": "global_config.bongo_cat_skin"}),
                )
                .map_err(|e| e.to_string())?,
        };
        *runtime = Some(Runtime {
            store,
            snapshot: Snapshot {
                generation: saved.generation,
                revision: 0,
                wardrobe: saved.payload,
            },
        });
    }
    let runtime = runtime.as_mut().ok_or("桌宠存储不可用")?;
    let mut next = runtime.snapshot.wardrobe.clone();
    let activity = if settle {
        pet_activity::snapshot()
    } else {
        ActivityBatch::new()
    };
    let mut rewards = Vec::new();
    for (day, seconds) in &activity {
        rewards.extend(next.advance(*seconds, day, random_below));
    }
    let changed = !activity.is_empty() || action.is_some();
    if let Some(action) = action {
        next.apply(action)?;
    }
    if changed {
        // Failed saves keep the pending time. Concurrent input remains pending.
        commit(app, runtime, next, &activity)?;
    }
    Ok(Outcome {
        snapshot: runtime.snapshot.clone(),
        rewards,
    })
}
