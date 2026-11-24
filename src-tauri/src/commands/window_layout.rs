use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::Serialize;

use crate::commands::account::{AccountManager, AccountMeta};
use crate::domain::account::WindowPositionPreset;
use crate::domain::config::GlobalConfig;
use crate::domain::window_layout::{
    window_title_key, LayoutMonitor, LayoutRect, LayoutSlot, LayoutTarget, WindowFrameProfile, WindowLayout,
};
use crate::error::AppError;
use crate::infrastructure::game_layout_windows::{self as windows, GameLayoutWindow};
use crate::launch_context::account_game_executable_identity;
use crate::state::{AccountLifecycleLease, SharedState};

struct AccountWindowIdentity {
    id: String,
    title: String,
    executable: PathBuf,
    pid: Option<u32>,
}

fn identities(config: &GlobalConfig, state: &SharedState) -> Vec<AccountWindowIdentity> {
    AccountManager::list_ids(&config.accounts_dir)
        .into_iter()
        .filter_map(|id| {
            let account = AccountManager::load_meta(&config.accounts_dir, &id).ok()?;
            Some(AccountWindowIdentity {
                executable: account_game_executable_identity(config, &account).ok()?,
                title: crate::commands::launch::account_window_title(&account),
                pid: state.multi_instance().instances().pid_for(&id),
                id,
            })
        })
        .collect()
}

/// PID wins when its executable matches. Title fallback must be unique on both
/// sides and match the complete executable path, including the client edition.
fn match_accounts<'a>(
    accounts: &[AccountWindowIdentity],
    observed: &'a [GameLayoutWindow],
) -> Vec<(String, &'a GameLayoutWindow)> {
    let same_path = crate::infrastructure::system::executable_paths_match;
    let mut claimed = HashSet::new();
    let mut matches = Vec::new();
    // Reserve all verified PID matches before attempting any nickname fallback.
    for account in accounts {
        if let Some(window) = observed.iter().find(|window| {
            Some(window.pid) == account.pid && same_path(&window.executable, &account.executable)
        }) {
            if claimed.insert(window.pid) {
                matches.push((account.id.clone(), window));
            }
        }
    }
    for account in accounts {
        if matches.iter().any(|(id, _)| id == &account.id) {
            continue;
        }
        if accounts
            .iter()
            .filter(|other| {
                window_title_key(&other.title) == window_title_key(&account.title)
                    && same_path(&other.executable, &account.executable)
            })
            .count()
            != 1
        {
            continue;
        }
        let candidates: Vec<_> = observed
            .iter()
            .filter(|window| {
                !claimed.contains(&window.pid)
                    && window_title_key(&window.title) == window_title_key(&account.title)
                    && same_path(&window.executable, &account.executable)
            })
            .collect();
        if candidates.len() == 1 {
            let window = candidates[0];
            claimed.insert(window.pid);
            matches.push((account.id.clone(), window));
        }
    }
    matches.sort_by_key(|(_, window)| window_title_key(&window.title));
    matches
}

#[tauri::command]
pub async fn get_game_layout_monitors(
    state: tauri::State<'_, SharedState>,
) -> Result<Vec<LayoutMonitor>, AppError> {
    let config = state.configuration().snapshot();
    tokio::task::spawn_blocking(move || configured_monitors(config.as_ref()))
        .await
        .map_err(|error| AppError::FileError(error.to_string()))?
}

pub(crate) fn configured_monitors(
    config: Option<&GlobalConfig>,
) -> Result<Vec<LayoutMonitor>, AppError> {
    Ok(windows::with_frame_profiles(
        windows::monitors()?,
        config
            .map(|value| value.window_frame_profiles.as_slice())
            .unwrap_or_default(),
    ))
}

fn update_frame_profile(
    config: &mut GlobalConfig,
    profile: WindowFrameProfile,
    monitors: &[LayoutMonitor],
) -> bool {
    if !monitors.iter().any(|monitor| {
        monitor.id == profile.monitor_id
            && (monitor.scale_factor - profile.scale_factor).abs() < 0.001
    }) {
        return false;
    }
    if !profile.metrics.valid() || config.window_frame_profiles.contains(&profile) {
        return false;
    }
    config.window_frame_profiles.retain(|previous| {
        previous.monitor_id != profile.monitor_id
            || (previous.scale_factor - profile.scale_factor).abs() >= 0.001
            || previous.metrics.dpi != profile.metrics.dpi
            || previous.metrics.style != profile.metrics.style
            || previous.metrics.ex_style != profile.metrics.ex_style
    });
    if config.window_frame_profiles.len() >= 64 {
        config.window_frame_profiles.remove(0);
    }
    config.window_frame_profiles.push(profile);
    true
}

/// Called only after two equal normal-window samples. Same parameters are a
/// no-op; profile persistence uses the normal atomic configuration transaction.
pub(crate) fn calibrate_game_frame(
    state: &SharedState,
    app: &tauri::AppHandle,
    sample: windows::GameFrameSample,
    expected_layout: Option<&WindowLayout>,
) -> Result<(), AppError> {
    // The hot no-change path is an in-memory comparison only: no monitor/window
    // enumeration, repository access, write transaction or event publication.
    if state
        .configuration()
        .snapshot()
        .is_some_and(|config| config.window_frame_profiles.contains(&sample.profile))
    {
        return Ok(());
    }
    let mut old_monitors = Vec::new();
    let mut updated = false;
    crate::commands::global_config::mutate_loaded_global_config(state, app, |config| {
        old_monitors = configured_monitors(Some(config))?;
        updated = update_frame_profile(config, sample.profile, &old_monitors);
        Ok(updated)
    })?;
    if !updated {
        return Ok(());
    }
    let Some(config) = state.configuration().snapshot() else {
        return Ok(());
    };
    let Some(layout) = config.window_layouts.iter().find(|layout| {
        config.window_layout_enabled
            && Some(layout.id.as_str()) == config.active_window_layout_id.as_deref()
            && Some(*layout) == expected_layout
    }) else {
        return Ok(());
    };
    let before = layout.resolve(&old_monitors)?;
    let after = layout.resolve(&configured_monitors(Some(&config))?)?;
    if before == after {
        return Ok(());
    }
    let observed = windows::snapshot()?;
    for (id, window) in match_accounts(&identities(&config, state), &observed) {
        let Some(index) = layout.slot_index_for_title(&window.title) else {
            continue;
        };
        if !unique_observed_title(&observed, &window.title) {
            continue;
        }
        let (Some(old), Some(new)) = (before.get(index), after.get(index)) else {
            continue;
        };
        if old == new || window.minimized {
            continue;
        }
        let Some(current) = windows::visible_window_rect(window.handle) else {
            continue;
        };
        // Respect a user's drag/capture/restore instead of pulling their window back.
        if (i64::from(current.x) - i64::from(old.x)).abs() > 2
            || (i64::from(current.y) - i64::from(old.y)).abs() > 2
        {
            continue;
        }
        state
            .multi_instance()
            .instances()
            .apply_initial_window_geometry(&id, window.pid, || {
                windows::apply(
                    window,
                    LayoutTarget {
                        rect: *new,
                        fullscreen: false,
                    },
                    false,
                )
            });
    }
    Ok(())
}

/// List identified accounts by window title, including after an app restart.
#[tauri::command]
pub async fn get_game_layout_account_order(
    state: tauri::State<'_, SharedState>,
) -> Result<Vec<String>, AppError> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let config = state
            .configuration()
            .snapshot()
            .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".into()))?;
        let observed = windows::snapshot()?;
        if observed.is_empty() {
            return Ok(Vec::new());
        }
        Ok(match_accounts(&identities(&config, &state), &observed)
            .into_iter()
            .map(|(id, _)| id)
            .collect())
    })
    .await
    .map_err(|error| AppError::FileError(error.to_string()))?
}

#[tauri::command]
pub async fn capture_game_window_layout(
    state: tauri::State<'_, SharedState>,
) -> Result<LayoutCaptureResult, AppError> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let config = state
            .configuration()
            .snapshot()
            .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".into()))?;
        let observed = windows::snapshot()?;
        let matched = match_accounts(&identities(&config, &state), &observed);
        if matched.is_empty() {
            return Err(AppError::FileError(
                "未找到可识别的账号游戏窗口，请先启动游戏".into(),
            ));
        }
        let monitors = configured_monitors(Some(&config))?;
        let mut slots = Vec::new();
        let mut account_ids = Vec::new();
        for (id, window) in matched {
            if window.title.trim().is_empty() || !unique_observed_title(&observed, &window.title) {
                return Err(AppError::FileError(format!("窗口名称为空或重复：{}", window.title)));
            }
            if let Some(monitor) = monitors.iter().find(|monitor| {
                !window.minimized
                    && monitor.bounds == window.rect
                    && windows::is_borderless(window.handle)
                    && windows::visible_window_rect(window.handle) == Some(monitor.bounds)
            }) {
                slots.push(LayoutSlot {
                    monitor_id: monitor.id.clone(),
                    window_title: Some(window.title.clone()),
                    fullscreen: true,
                    x: monitor.bounds.x - monitor.work_area.x,
                    y: monitor.bounds.y - monitor.work_area.y,
                    width: monitor.bounds.width,
                    height: monitor.bounds.height,
                });
                account_ids.push(id);
                continue;
            }
            let sample = windows::frame_target(window.pid, &window.executable)
                .filter(|target| target.handle == window.handle)
                .and_then(|target| {
                    let first = windows::sample_target(&target)?;
                    let second = windows::sample_target(&target)?;
                    (first == second).then_some(second)
                })
                .ok_or_else(|| {
                    AppError::FileError(format!(
                        "{}：请使用普通窗口或覆盖整个显示器的无边框全屏，停止拖动后重试",
                        window.title
                    ))
                })?;
            let mut slot = captured_slot(&monitors, &sample)?;
            slot.window_title = Some(window.title.clone());
            slots.push(slot);
            account_ids.push(id);
        }
        let layout = WindowLayout {
            id: uuid::Uuid::new_v4().to_string(),
            name: "当前窗口布局".into(),
            monitors,
            windows: slots,
        };
        layout.validate().map_err(|error| {
            AppError::FileError(format!(
                "无法原样保存当前布局：{error}。请将窗口移回显示器工作区后重试"
            ))
        })?;
        Ok(LayoutCaptureResult {
            layout,
            account_ids,
        })
    })
    .await
    .map_err(|error| AppError::FileError(error.to_string()))?
}

#[derive(Serialize)]
pub struct LayoutCaptureResult {
    pub layout: WindowLayout,
    pub account_ids: Vec<String>,
}

fn captured_slot(
    monitors: &[LayoutMonitor],
    sample: &windows::GameFrameSample,
) -> Result<LayoutSlot, AppError> {
    let monitor = monitors
        .iter()
        .find(|monitor| {
            monitor.id == sample.profile.monitor_id
                && (monitor.scale_factor - sample.profile.scale_factor).abs() < 0.001
        })
        .ok_or_else(|| AppError::FileError("显示器配置已变化，请重新捕获布局".into()))?;
    Ok(LayoutSlot {
        monitor_id: monitor.id.clone(),
        window_title: None,
        fullscreen: false,
        x: sample.visible_rect.x - monitor.work_area.x,
        y: sample.visible_rect.y - monitor.work_area.y,
        width: sample.client_width,
        height: sample.client_height,
    })
}

#[tauri::command]
pub async fn capture_account_window_position(
    _app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    activate: Option<bool>,
) -> Result<AccountMeta, AppError> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let _lease = AccountLifecycleLease::try_acquire(&state, &account_id)?;
        let config = state
            .configuration()
            .snapshot()
            .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".into()))?;
        let mut account = AccountManager::load_meta(&config.accounts_dir, &account_id)?;
        let observed = windows::snapshot()?;
        let accounts = identities(&config, &state);
        let matched = match_accounts(&accounts, &observed);
        let (_, window) = matched
            .iter()
            .find(|(id, _)| id.eq_ignore_ascii_case(&account_id))
            .ok_or_else(|| {
                AppError::FileError("未找到此账号的游戏窗口；请启动游戏，并确认窗口昵称唯一".into())
            })?;
        if window.minimized {
            return Err(AppError::FileError(
                "请先还原游戏窗口，再保存当前位置".into(),
            ));
        }
        let mut name = "当前位置".to_string();
        let mut suffix = 2;
        while account
            .position_presets
            .iter()
            .any(|preset| preset.name == name)
        {
            name = format!("当前位置 {suffix}");
            suffix += 1;
        }
        let preset = WindowPositionPreset {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            x: window.rect.x,
            y: window.rect.y,
        };
        let active_id = if activate.unwrap_or(true) {
            Some(preset.id.clone())
        } else {
            account.active_position_id.clone()
        };
        let mut presets = account.position_presets.clone();
        presets.push(preset);
        account
            .replace_position_presets(active_id, presets)
            .map_err(|error| AppError::ConfigWriteError(error.to_string()))?;
        AccountManager::save_meta(&config.accounts_dir, &account)?;
        state
            .multi_instance()
            .instances()
            .record_discovered(&account_id, window.pid);
        state
            .multi_instance()
            .instances()
            .override_window_geometry(&account_id, window.pid, || ());
        account.token = None;
        Ok(account)
    })
    .await
    .map_err(|error| AppError::FileError(error.to_string()))?
}

pub(crate) struct LaunchLayoutPlan {
    targets: HashMap<String, (String, LayoutTarget)>,
    existing: HashSet<String>,
}

impl LaunchLayoutPlan {
    pub fn is_existing(&self, account_id: &str) -> bool {
        self.existing.contains(&account_id.to_ascii_lowercase())
    }

    pub fn target(&self, account: &AccountMeta) -> Option<LayoutTarget> {
        let (title, target) = self.targets.get(&account.id.to_ascii_lowercase())?;
        let current_title = crate::commands::launch::account_window_title(account);
        (window_title_key(&current_title) == *title).then_some(*target)
    }
}

fn unique_observed_title(observed: &[GameLayoutWindow], title: &str) -> bool {
    let key = window_title_key(title);
    observed.iter().filter(|window| window_title_key(&window.title) == key).count() == 1
}

pub(crate) fn prepare_launch_layout(
    config: &GlobalConfig,
    state: &SharedState,
    requested: &[String],
) -> Result<Option<LaunchLayoutPlan>, AppError> {
    if !config.window_layout_enabled {
        return Ok(None);
    }
    let Some(id) = config.active_window_layout_id.as_deref() else {
        return Ok(None);
    };
    let layout = config
        .window_layouts
        .iter()
        .find(|layout| layout.id == id)
        .ok_or_else(|| AppError::ConfigReadError("所选窗口布局不存在，请重新选择".into()))?;
    let positions = layout.resolve_targets(&configured_monitors(Some(config))?)?;
    let observed = windows::snapshot()?;
    let accounts = identities(config, state);
    let existing = match_accounts(&accounts, &observed)
        .into_iter()
        .map(|(id, _)| id.to_ascii_lowercase())
        .collect();
    let mut targets = HashMap::new();
    for account in &accounts {
        if !requested.iter().any(|id| id.eq_ignore_ascii_case(&account.id)) {
            continue;
        }
        let Some(index) = layout.slot_index_for_title(&account.title) else {
            continue;
        };
        let key = window_title_key(&account.title);
        if accounts.iter().filter(|other| window_title_key(&other.title) == key).count() == 1 {
            targets.insert(account.id.to_ascii_lowercase(), (key, positions[index]));
        }
    }
    Ok(Some(LaunchLayoutPlan { targets, existing }))
}

#[derive(Serialize)]
pub struct LayoutRestoreResult {
    pub applied: Vec<String>,
    pub failures: Vec<String>,
}

pub(crate) fn restore_layout(
    config: &GlobalConfig,
    state: &SharedState,
) -> Result<LayoutRestoreResult, AppError> {
    if !config.window_layout_enabled {
        return Err(AppError::FileError("请先开启窗口布局".into()));
    }
    let layout = config
        .active_window_layout_id
        .as_ref()
        .and_then(|id| config.window_layouts.iter().find(|layout| &layout.id == id))
        .ok_or_else(|| AppError::FileError("请先选择一个已保存的窗口布局".into()))?;
    let targets = layout.resolve_targets(&configured_monitors(Some(config))?)?;
    let observed = windows::snapshot()?;
    let matched = match_accounts(&identities(config, state), &observed);
    let mut result = LayoutRestoreResult {
        applied: Vec::new(),
        failures: Vec::new(),
    };
    let mut placements = Vec::new();
    for (id, window) in matched {
        let Some(index) = layout.slot_index_for_title(&window.title) else {
            continue;
        };
        if !unique_observed_title(&observed, &window.title) {
            result.failures.push(format!("{}：窗口名称重复", window.title));
            continue;
        }
        placements.push((index, id, window));
    }
    placements.sort_by_key(|(index, _, _)| std::cmp::Reverse(*index));
    for (index, id, window) in placements {
        let target = targets[index];
        let registry = state.multi_instance().instances();
        registry.record_discovered(&id, window.pid);
        let applied = registry
            .override_window_geometry(&id, window.pid, || windows::apply(window, target, true))
            .unwrap_or_else(|| Err(AppError::FileError("账号窗口状态已变化，请重试".into())));
        match applied {
            Ok(()) => {
                result.applied.push(id);
            }
            Err(error) => result.failures.push(format!("{}：{error}", window.title)),
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn restore_game_window_layout(
    state: tauri::State<'_, SharedState>,
) -> Result<LayoutRestoreResult, AppError> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let config = state
            .configuration()
            .snapshot()
            .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".into()))?;
        restore_layout(&config, &state)
    })
    .await
    .map_err(|error| AppError::FileError(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_preserves_physical_origin_and_client_resolution_on_negative_monitor() {
        use crate::domain::window_layout::{FrameInsets, WindowFrameMetrics};
        let metrics = WindowFrameMetrics {
            dpi: 144,
            style: 0,
            ex_style: 0,
            visible: FrameInsets {
                left: 2,
                top: 46,
                right: 2,
                bottom: 2,
            },
            invisible: FrameInsets::default(),
        };
        let monitor = LayoutMonitor {
            id: "left".into(),
            name: "Left".into(),
            primary: true,
            scale_factor: 1.5,
            bounds: LayoutRect {
                x: -2560,
                y: -200,
                width: 2560,
                height: 1440,
            },
            work_area: LayoutRect {
                x: -2520,
                y: -180,
                width: 2520,
                height: 1380,
            },
            frame: Some(metrics),
        };
        let sample = windows::GameFrameSample {
            profile: WindowFrameProfile {
                monitor_id: "left".into(),
                scale_factor: 1.5,
                metrics,
            },
            visible_rect: LayoutRect {
                x: -2400,
                y: -100,
                width: 1284,
                height: 768,
            },
            client_width: 1280,
            client_height: 720,
            handle: 1,
        };
        let monitors = vec![monitor];
        let slot = captured_slot(&monitors, &sample).unwrap();
        assert_eq!(
            (slot.x, slot.y, slot.width, slot.height),
            (120, 80, 1280, 720)
        );
        let layout = WindowLayout {
            id: "captured".into(),
            name: "Captured".into(),
            monitors,
            windows: vec![slot],
        };
        let restored = layout.resolve(&layout.monitors).unwrap()[0];
        assert_eq!(
            (restored.x, restored.y, restored.width, restored.height),
            (-2400, -100, 1280, 720)
        );
        let mut changed = sample;
        changed.profile.scale_factor = 1.0;
        assert!(captured_slot(&layout.monitors, &changed).is_err());
        changed.profile.monitor_id = "disconnected".into();
        assert!(captured_slot(&layout.monitors, &changed).is_err());
    }

    #[test]
    fn identical_frame_calibration_does_not_write_and_dpi_profiles_stay_separate() {
        use crate::domain::window_layout::{FrameInsets, WindowFrameMetrics};
        let rect = LayoutRect {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
        };
        let metrics = WindowFrameMetrics {
            dpi: 96,
            style: 0,
            ex_style: 0,
            visible: FrameInsets {
                left: 1,
                top: 31,
                right: 1,
                bottom: 1,
            },
            invisible: FrameInsets::default(),
        };
        let monitor = LayoutMonitor {
            id: "main".into(),
            name: "main".into(),
            bounds: rect,
            work_area: rect,
            primary: true,
            scale_factor: 1.0,
            frame: Some(metrics),
        };
        let mut config = GlobalConfig::default();
        // Older config files have neither persisted profiles nor monitor frames.
        let mut legacy = serde_json::to_value(&config).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("window_frame_profiles");
        let migrated: GlobalConfig = serde_json::from_value(legacy).unwrap();
        assert!(migrated.window_frame_profiles.is_empty());
        let roundtrip: GlobalConfig =
            serde_json::from_value(serde_json::to_value(&migrated).unwrap()).unwrap();
        assert!(roundtrip.window_frame_profiles.is_empty());
        let mut profile = WindowFrameProfile {
            monitor_id: "main".into(),
            scale_factor: 1.0,
            metrics,
        };
        // Even a measurement equal to the preset is persisted once, so startup
        // can use the measured profile without re-deriving it after a restart.
        assert!(update_frame_profile(
            &mut config,
            profile.clone(),
            std::slice::from_ref(&monitor)
        ));
        assert_eq!(config.window_frame_profiles, vec![profile.clone()]);
        let directory =
            std::env::temp_dir().join(format!("d2r-frame-profile-{}", uuid::Uuid::new_v4()));
        config.save(directory.to_str().unwrap()).unwrap();
        let config_file = directory.join("global_config.json");
        let mut config: GlobalConfig =
            serde_json::from_slice(&std::fs::read(&config_file).unwrap()).unwrap();
        std::fs::remove_file(config_file).unwrap();
        std::fs::remove_dir(directory).unwrap();
        assert_eq!(config.window_frame_profiles, vec![profile.clone()]);
        assert!(!update_frame_profile(
            &mut config,
            profile.clone(),
            std::slice::from_ref(&monitor)
        ));
        profile.metrics.visible.top = 33;
        assert!(update_frame_profile(
            &mut config,
            profile.clone(),
            std::slice::from_ref(&monitor)
        ));
        let monitors =
            windows::with_frame_profiles(vec![monitor.clone()], &config.window_frame_profiles);
        assert!(!update_frame_profile(
            &mut config,
            profile.clone(),
            &monitors
        ));
        assert_eq!(config.window_frame_profiles.len(), 1);
        let mut other_style = profile.clone();
        other_style.metrics.style = 2;
        assert!(update_frame_profile(
            &mut config,
            other_style.clone(),
            &monitors
        ));
        assert_eq!(config.window_frame_profiles.len(), 2);
        assert!(config.window_frame_profiles.contains(&profile));
        assert!(config.window_frame_profiles.contains(&other_style));
        assert!(!update_frame_profile(&mut config, other_style, &monitors));
        let changed_dpi = LayoutMonitor {
            scale_factor: 1.5,
            ..monitor
        };
        let monitors =
            windows::with_frame_profiles(vec![changed_dpi], &config.window_frame_profiles);
        assert_eq!(monitors[0].frame, Some(metrics));
        assert!(!update_frame_profile(&mut config, profile, &monitors));
    }
    fn account(id: &str, title: &str, pid: Option<u32>) -> AccountWindowIdentity {
        AccountWindowIdentity {
            id: id.into(),
            title: title.into(),
            executable: PathBuf::from(r"C:\Game\D2R.exe"),
            pid,
        }
    }
    fn window(pid: u32, title: &str, started_at: u64) -> GameLayoutWindow {
        GameLayoutWindow {
            handle: 1,
            pid,
            title: title.into(),
            executable: PathBuf::from(r"C:\Game\D2R.exe"),
            started_at,
            rect: LayoutRect {
                x: 1,
                y: 2,
                width: 1280,
                height: 720,
            },
            minimized: false,
        }
    }
    #[test]
    fn pid_identifies_accounts_and_titles_decide_presentation_order() {
        let accounts = [
            account("acount1", "renamed", Some(10)),
            account("acount2", "second", None),
        ];
        let windows = [window(10, "Diablo II", 200), window(20, "second", 100)];
        let matches = match_accounts(&accounts, &windows);
        assert_eq!(
            matches
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>(),
            ["acount1", "acount2"]
        );
    }
    #[test]
    fn ambiguous_names_and_wrong_executables_are_never_moved() {
        let accounts = [
            account("acount1", "same", None),
            account("acount2", "same", None),
        ];
        assert!(match_accounts(&accounts, &[window(10, "same", 1)]).is_empty());
        let mut wrong = window(10, "same", 1);
        wrong.executable = PathBuf::from(r"D:\Other\D2R.exe");
        assert!(match_accounts(&[account("acount1", "same", Some(10))], &[wrong]).is_empty());
        assert!(match_accounts(
            &[account("acount1", "same", None)],
            &[window(10, "same", 1), window(20, "same", 2)]
        )
        .is_empty());
    }
    #[test]
    fn saved_titles_match_independently_of_process_order() {
        let accounts = [account("a", "Alpha", Some(10)), account("b", "Beta", Some(20))];
        for windows in [
            [window(10, "Alpha", 1), window(20, "Beta", 2)],
            [window(20, "Beta", 1), window(10, "Alpha", 2)],
        ] {
            let matched = match_accounts(&accounts, &windows);
            assert_eq!(matched.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
            assert!(unique_observed_title(&windows, " alpha "));
        }
    }
}
