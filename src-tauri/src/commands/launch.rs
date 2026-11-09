use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::application::multi_instance::{
    launch_queue_can_continue, CancellationTicket, GameWindowIdentity, GameWindowPort,
    InstanceRegistry, LaunchAccountEntry, LaunchBatchPlan, LaunchGraphicsOverride, WindowPosition,
};
use crate::application::task_runtime::{TaskHandle, TaskRequest, TaskState};
use crate::battle_net_config::update_mod_args;
use crate::commands::account::{
    copy_account_settings_to_system, recover_interrupted_replacement, remove_path_if_exists,
    replace_path_with_backup, resolve_account_runtime_snapshot, sibling_with_suffix,
    AccountManager, AccountMeta, RegistrySnapshotPath,
};
use crate::commands::utils::silent_cmd;
use crate::domain::account::{AuthMode, ClientEdition};
use crate::domain::config::GlobalConfig;
use crate::domain::window_layout::{LayoutRect, WindowLayout};
use crate::error::AppError;
use crate::infrastructure::system::{LaunchProgress, SystemGameWindowPort};
use crate::launch_context::{
    account_game_executable_identity, ContextPurpose, HostRuntimeLease, LaunchContext,
};
use crate::state::{AccountLifecycleLease, SharedState};
use crate::token_registry_change::{WebTokenChangeMonitor, WEB_TOKEN_VALUE_NAME};

const LOGIN_KEY_SEND_WINDOW: std::time::Duration = std::time::Duration::from_secs(20);

// Battle.net decides which installation to start. Identify the game by the new PID,
// then carry its actual executable through all subsequent process guards.
fn newly_started_battle_net_game(
    before_pids: &[u32],
    games: &[(u32, Option<PathBuf>)],
    configured_executable: &Path,
) -> Option<(u32, PathBuf)> {
    games
        .iter()
        .find(|(pid, _)| !before_pids.contains(pid))
        .map(|(pid, executable)| {
            (
                *pid,
                executable
                    .clone()
                    .unwrap_or_else(|| configured_executable.to_path_buf()),
            )
        })
}

fn battle_net_game_directory_warning(actual: &Path, configured: &Path) -> Option<String> {
    (!crate::infrastructure::system::executable_paths_match(actual, configured)).then(|| {
        format!(
            "当前战网运行的游戏目录与软件配置目录不一致，建议统一战网游戏目录，保持单游戏目录统一，不需要多份游戏目录。实际游戏目录：{}；软件配置目录：{}。本次启动将继续。",
            actual.parent().unwrap_or(actual).display(),
            configured.parent().unwrap_or(configured).display(),
        )
    })
}

/// 启动进度详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchResult {
    pub account_id: String,
    pub success: bool,
    pub d2r_pid: Option<u32>,
    pub error: Option<String>,
    pub mutex_killed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub(crate) enum LaunchTaskRetryPayload {
    BattleNetOnly {
        account_ids: Vec<String>,
    },
    Accounts {
        account_ids: Option<Vec<String>>,
        entries: Option<Vec<LaunchAccountEntry>>,
        #[serde(default)]
        session_id: Option<String>,
    },
}

#[derive(Default)]
struct BnetPreparationOptions<'a> {
    meta_override: Option<&'a AccountMeta>,
    preserved_default_mod_args: Option<&'a str>,
    require_settings_copy: bool,
}

struct LaunchExecutionOptions {
    persist_position_changes: bool,
    preserved_default_mod_args: Option<String>,
    graphics_override: Option<LaunchGraphicsOverride>,
    layout_rect: Option<LayoutRect>,
}

struct TokenLaunchRequest<'a> {
    cancellation_ticket: CancellationTicket,
    account_id: &'a str,
    meta: &'a AccountMeta,
    context: &'a LaunchContext,
    graphics_override: Option<&'a LaunchGraphicsOverride>,
    layout_rect: Option<LayoutRect>,
}

struct TemporarySettingsOverride {
    path: PathBuf,
    original: Vec<u8>,
}

type AccountPositionIdentity = (Option<i32>, Option<i32>, Option<String>);

/// Record ownership before a status scan can discard a still-loading window.
/// The retained OS handle also prevents PID reuse from authorizing title writes.
fn track_launched_process(
    app: &tauri::AppHandle,
    state: &SharedState,
    config: &GlobalConfig,
    account_id: &str,
    pid: u32,
    mod_args: &str,
    executable: &Path,
) {
    let process = crate::infrastructure::system::LaunchProcessGuard::capture(pid, executable)
        .map(std::sync::Arc::new);
    let alive = process.clone().map(|process| {
        std::sync::Arc::new(move || process.is_running())
            as std::sync::Arc<dyn Fn() -> bool + Send + Sync>
    });
    state
        .multi_instance()
        .instances()
        .record_launched_with_process(account_id, pid, mod_args, alive);
    let Some(process) = process else {
        return;
    };
    let state = state.clone();
    let accounts_dir = config.accounts_dir.clone();
    let account_id = account_id.to_string();
    let app = app.clone();
    let executable = executable.to_path_buf();
    let expected_layout = selected_window_layout(config).cloned();
    let session_id = config
        .launch_session
        .as_ref()
        .map(|session| session.id.clone());
    let session_mod_args = mod_args.to_string();
    tauri::async_runtime::spawn(async move {
        let mut frame_probe =
            crate::infrastructure::game_layout_windows::StartupFrameProbe::default();
        let mut frame_lease: Option<crate::application::multi_instance::FrameCalibrationLease> =
            None;
        let started = std::time::Instant::now();
        let mut session_recorded = session_id.is_none();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if !process.is_running()
                || state.multi_instance().instances().pid_for(&account_id) != Some(pid)
            {
                break;
            }
            if !session_recorded {
                let state = state.clone();
                let app = app.clone();
                let account = account_id.clone();
                let session = session_id.clone().unwrap();
                let arguments = session_mod_args.clone();
                session_recorded = tokio::task::spawn_blocking(move || {
                    crate::commands::launch_scheme_recovery::capture_process(
                        &state, &app, &session, &account, pid, &arguments,
                    )
                })
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or(false);
            }
            // Only the elected representative measures borders. Other windows
            // wait for or reuse its result; title repair has an independent life.
            if frame_probe.exhausted(started.elapsed()) {
                frame_lease.take();
            }
            if frame_probe.due(started.elapsed()) {
                use crate::application::multi_instance::FrameCalibrationClaim;
                let executable = executable.clone();
                let target = tokio::task::spawn_blocking(move || {
                    crate::infrastructure::game_layout_windows::frame_target(pid, &executable)
                })
                .await
                .ok()
                .flatten();
                if let Some(target) = target {
                    if frame_lease
                        .as_ref()
                        .is_some_and(|lease| lease.key() != &target.key)
                    {
                        frame_lease.take();
                        frame_probe.reset_sample();
                    }
                    if frame_lease.is_none() {
                        match state
                            .multi_instance()
                            .frame_calibrations()
                            .claim(target.key.clone())
                        {
                            FrameCalibrationClaim::Ready => frame_probe.finish(),
                            FrameCalibrationClaim::Busy => {}
                            FrameCalibrationClaim::Representative(lease) => {
                                frame_lease = Some(lease)
                            }
                        }
                    }
                    if let Some(lease) = frame_lease.take() {
                        // Keep the lease inside blocking work, including if this
                        // async supervisor is cancelled while the OS call runs.
                        let measured = tokio::task::spawn_blocking(move || {
                            (
                                crate::infrastructure::game_layout_windows::sample_target(&target),
                                lease,
                            )
                        })
                        .await;
                        if let Ok((sample, lease)) = measured {
                            let stable = frame_probe.observe(started.elapsed(), sample.clone());
                            if let Some(sample) = stable {
                                let state = state.clone();
                                let app = app.clone();
                                let layout = expected_layout.clone();
                                let calibrated = tokio::task::spawn_blocking(move || {
                                    let result =
                                        crate::commands::window_layout::calibrate_game_frame(
                                            &state,
                                            &app,
                                            sample,
                                            layout.as_ref(),
                                        );
                                    if result.is_ok() {
                                        lease.complete();
                                    }
                                    result
                                })
                                .await
                                .is_ok_and(|result| result.is_ok());
                                if calibrated {
                                    frame_probe.finish();
                                } else {
                                    frame_probe.reset_sample();
                                }
                            } else if sample.is_some() {
                                frame_lease = Some(lease);
                            }
                            // None drops the lease so another ready window can
                            // take over rather than waiting for this one's retries.
                        } else {
                            frame_probe.observe(started.elapsed(), None);
                        }
                    }
                } else {
                    frame_lease.take();
                    frame_probe.observe(started.elapsed(), None);
                }
            }
            let state = state.clone();
            let accounts_dir = accounts_dir.clone();
            let account_id = account_id.clone();
            let process = process.clone();
            let _ = tokio::task::spawn_blocking(move || {
                if process.is_running() {
                    let _ = refresh_launch_window_title(
                        state.multi_instance().instances(),
                        &accounts_dir,
                        &account_id,
                        pid,
                        |title| SystemGameWindowPort.rename(pid, title),
                    );
                }
            })
            .await;
        }
    });
}

/// Read the latest nickname under the same lock as explicit account renames.
/// A detached startup retry must never restore a title captured before a rename.
fn refresh_launch_window_title(
    registry: &InstanceRegistry,
    accounts_dir: &str,
    account_id: &str,
    pid: u32,
    rename: impl FnOnce(&str),
) -> Result<(), AppError> {
    registry.with_account_identity_change(account_id, |current_pid| {
        if current_pid != Some(pid) {
            return Ok(());
        }
        let account = AccountManager::load_meta(accounts_dir, account_id)?;
        rename(if account.display_name.is_empty() {
            &account.id
        } else {
            &account.display_name
        });
        Ok(())
    })
}

fn selected_window_layout(config: &GlobalConfig) -> Option<&WindowLayout> {
    config
        .window_layout_enabled
        .then_some(config.active_window_layout_id.as_deref())
        .flatten()
        .and_then(|id| config.window_layouts.iter().find(|layout| layout.id == id))
}

/// Finite startup retries must yield to a newer layout selection or account
/// position capture instead of moving a window back to an obsolete snapshot.
struct WindowGeometryRequest {
    accounts_dir: String,
    account_id: String,
    layout: Option<WindowLayout>,
    position: Option<AccountPositionIdentity>,
    monitors: Vec<crate::domain::window_layout::LayoutMonitor>,
}

impl WindowGeometryRequest {
    fn capture(config: &GlobalConfig, account_id: &str) -> Self {
        Self {
            accounts_dir: config.accounts_dir.clone(),
            account_id: account_id.into(),
            monitors: crate::commands::window_layout::configured_monitors(Some(config))
                .unwrap_or_default(),
            layout: selected_window_layout(config).cloned(),
            position: AccountManager::load_meta(&config.accounts_dir, account_id)
                .ok()
                .map(|account| {
                    (
                        account.window_x,
                        account.window_y,
                        account.active_position_id,
                    )
                }),
        }
    }

    fn calibrated_target(&self, state: &SharedState, original: LayoutRect) -> LayoutRect {
        let Some(layout) = &self.layout else {
            return original;
        };
        let Some(config) = state.configuration().snapshot() else {
            return original;
        };
        let resolved = (|| {
            let before = layout.resolve(&self.monitors).ok()?;
            let index = before.iter().position(|rect| *rect == original)?;
            let monitors = crate::infrastructure::game_layout_windows::with_frame_profiles(
                self.monitors.clone(),
                &config.window_frame_profiles,
            );
            layout.resolve(&monitors).ok()?.get(index).copied()
        })();
        resolved.unwrap_or(original)
    }

    fn matches(&self, config: &GlobalConfig, account: &AccountMeta) -> bool {
        selected_window_layout(config) == self.layout.as_ref()
            && self.position.as_ref().is_some_and(|position| {
                position
                    == &(
                        account.window_x,
                        account.window_y,
                        account.active_position_id.clone(),
                    )
            })
    }

    fn is_current(&self, state: &SharedState) -> bool {
        let Some(config) = state.configuration().snapshot() else {
            return false;
        };
        if selected_window_layout(&config) != self.layout.as_ref() {
            return false;
        }
        AccountManager::load_meta(&self.accounts_dir, &self.account_id)
            .is_ok_and(|account| self.matches(&config, &account))
    }
}

impl Drop for TemporarySettingsOverride {
    fn drop(&mut self) {
        if let Err(error) = std::fs::write(&self.path, &self.original) {
            crate::logger::log_msg(
                "ERROR",
                "Launch",
                &format!(
                    "恢复方案启动前的 Settings.json 失败 ({}): {error}",
                    self.path.display()
                ),
            );
        }
    }
}

const MUTEX_NAME: &str = "DiabloII Check For Other Instances";

/// 2026年6月 暴雪更新后常规进程数为7，未来若卡在等待登录需修改此阈值
const BNET_LOGIN_PROCESS_COUNT_THRESHOLD: usize = 7;

fn battle_net_launch_argument(product_code: &str) -> String {
    format!(r#"--exec="launch {product_code}""#)
}

fn spawn_battle_net_launch_command(
    battle_net_path: &str,
    product_code: &str,
) -> std::io::Result<std::process::Child> {
    let launch_argument = battle_net_launch_argument(product_code);
    let mut command = Command::new(battle_net_path);

    #[cfg(windows)]
    {
        // Battle.net parses this switch from the raw Windows command line and expects the
        // value, rather than the complete argument, to be quoted: --exec="launch OSI".
        use std::os::windows::process::CommandExt;
        command.raw_arg(&launch_argument);
    }

    #[cfg(not(windows))]
    command.arg(&launch_argument);

    command.spawn()
}

fn launch_login_is_ready(login_signal_ready: bool, mutex_closed: bool) -> bool {
    login_signal_ready && mutex_closed
}

fn battle_net_account_to_keep<'a>(
    configured_account_id: Option<&str>,
    results: &'a [LaunchResult],
    cancelled: bool,
) -> Option<&'a str> {
    if cancelled
        || results
            .iter()
            .any(|result| !launch_queue_can_continue(result.success, result.mutex_killed))
    {
        return None;
    }
    let configured_account_id = configured_account_id?.trim();
    results
        .iter()
        .find(|result| {
            result
                .account_id
                .eq_ignore_ascii_case(configured_account_id)
                && result.d2r_pid.is_some()
        })
        .map(|result| result.account_id.as_str())
}

#[derive(Default)]
struct MutexRemovalState {
    closed: std::sync::atomic::AtomicBool,
    found_once: std::sync::atomic::AtomicBool,
    last_error: std::sync::Mutex<Option<String>>,
}

impl MutexRemovalState {
    fn record_found(&self) {
        self.found_once.store(true, Ordering::SeqCst);
    }

    fn record_error(&self, error: impl Into<String>) {
        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = Some(error.into());
        }
    }

    fn confirm_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn diagnostics(&self) -> String {
        if self.is_closed() {
            return "已清除".to_string();
        }
        let last_error = self.last_error.lock().ok().and_then(|error| error.clone());
        if !self.found_once.load(Ordering::SeqCst) {
            return last_error
                .map(|error| format!("未检测到 (最近错误: {error})"))
                .unwrap_or_else(|| "未检测到".to_string());
        }
        last_error
            .map(|error| format!("已检测到但未能确认清除 ({error})"))
            .unwrap_or_else(|| "已检测到但未能确认清除".to_string())
    }
}

// Abort on every return path, including when the parent launch future is dropped.
struct LaunchBackgroundTask(tokio::task::JoinHandle<()>);
impl LaunchBackgroundTask {
    fn abort(&self) {
        self.0.abort();
    }
}
impl Drop for LaunchBackgroundTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn spawn_mutex_removal(
    state: &SharedState,
    cancellation_ticket: CancellationTicket,
    pid: u32,
    executable: &Path,
    mutex_state: std::sync::Arc<MutexRemovalState>,
) -> LaunchBackgroundTask {
    let state_clone = state.clone();
    let process = crate::infrastructure::system::LaunchProcessGuard::capture(pid, executable);
    LaunchBackgroundTask(tokio::spawn(async move {
        let mut closed_at_least_once = false;
        for _ in 0..120 {
            if is_cancelled(&state_clone, cancellation_ticket)
                || !process.as_ref().is_some_and(|process| process.is_running())
            {
                break;
            }
            match crate::infrastructure::system::find_mutex_handle(pid, MUTEX_NAME) {
                Ok(Some(hid)) => {
                    mutex_state.record_found();
                    match crate::infrastructure::system::close_handle(pid, &hid) {
                        Ok(()) => {
                            closed_at_least_once = true;
                            match crate::infrastructure::system::find_mutex_handle(pid, MUTEX_NAME)
                            {
                                Ok(None) => {
                                    mutex_state.confirm_closed();
                                    break;
                                }
                                Ok(Some(_)) => {
                                    mutex_state.record_error("关闭后仍检测到互斥句柄");
                                }
                                Err(error) => {
                                    mutex_state
                                        .record_error(format!("确认互斥句柄清除失败: {error}"));
                                }
                            }
                        }
                        Err(error) => {
                            mutex_state.record_error(error.to_string());
                        }
                    }
                }
                Ok(None) if closed_at_least_once => {
                    mutex_state.confirm_closed();
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    mutex_state.record_error(error.to_string());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }))
}

fn graphics_settings_path(
    config: &GlobalConfig,
    meta: &AccountMeta,
    context: &LaunchContext,
) -> Result<PathBuf, AppError> {
    if meta.has_customized_settings {
        Ok(
            AccountManager::account_dir_checked(&config.accounts_dir, &meta.id)?
                .join("Settings.json"),
        )
    } else {
        Ok(context
            .required_saved_games_directory()?
            .join("Settings.json"))
    }
}

fn preflight_scheme_mod(meta: &AccountMeta, context: &LaunchContext) -> Result<(), AppError> {
    let name =
        crate::audio_mod::active_mod_name(&meta.mod_args).map_err(AppError::ConfigReadError)?;
    let Some(name) = name else {
        return Ok(());
    };
    let mods_directory = context.installation.game_directory.join("mods");
    if crate::audio_mod::installed_mods(&mods_directory)
        .iter()
        .any(|installed| installed.name.eq_ignore_ascii_case(&name))
    {
        return Ok(());
    }
    Err(AppError::ConfigReadError(format!(
        "账号 {} 的方案 Mod“{}”在游戏目录 {} 中不存在，请检查该账号的客户端目录和 Mod 文件夹",
        meta.id,
        name,
        mods_directory.display()
    )))
}

fn preflight_launch_graphics(
    config: &GlobalConfig,
    meta: &AccountMeta,
    context: &LaunchContext,
    graphics: Option<&LaunchGraphicsOverride>,
    layout_active: bool,
) -> Result<(), AppError> {
    if graphics.is_none() && !layout_active {
        return Ok(());
    }
    let path = graphics_settings_path(config, meta, context)?;
    let bytes = std::fs::read(&path).map_err(|error| {
        AppError::FileError(format!(
            "读取方案画质基础配置失败 ({}): {error}",
            path.display()
        ))
    })?;
    let settings: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&bytes)
        .map_err(|error| {
            AppError::ConfigReadError(format!(
                "方案画质基础配置无效 ({}): {error}",
                path.display()
            ))
        })?;
    if settings.is_empty() {
        return Err(AppError::ConfigReadError(format!(
            "方案画质基础配置为空: {}",
            path.display()
        )));
    }
    Ok(())
}

fn apply_temporary_graphics_override(
    context: &LaunchContext,
    graphics: Option<&LaunchGraphicsOverride>,
    layout_rect: Option<LayoutRect>,
) -> Result<Option<TemporarySettingsOverride>, AppError> {
    if graphics.is_none() && layout_rect.is_none() {
        return Ok(None);
    }
    let path = context
        .required_saved_games_directory()?
        .join("Settings.json");
    apply_temporary_window_settings_at_path(path, graphics, layout_rect).map(Some)
}

#[cfg(test)]
fn apply_temporary_graphics_override_at_path(
    path: PathBuf,
    graphics: &LaunchGraphicsOverride,
) -> Result<TemporarySettingsOverride, AppError> {
    apply_temporary_window_settings_at_path(path, Some(graphics), None)
}

fn apply_temporary_window_settings_at_path(
    path: PathBuf,
    graphics: Option<&LaunchGraphicsOverride>,
    layout_rect: Option<LayoutRect>,
) -> Result<TemporarySettingsOverride, AppError> {
    let original = std::fs::read(&path).map_err(|error| {
        AppError::FileError(format!(
            "读取系统 Settings.json 失败 ({}): {error}",
            path.display()
        ))
    })?;
    let mut settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&original).map_err(|error| {
            AppError::ConfigReadError(format!(
                "系统 Settings.json 无效 ({}): {error}",
                path.display()
            ))
        })?;
    if settings.is_empty() {
        return Err(AppError::ConfigReadError(format!(
            "系统 Settings.json 为空: {}",
            path.display()
        )));
    }
    if let Some(graphics) = graphics {
        if let Some(snapshot) = &graphics.game_settings {
            settings = snapshot.clone();
        }
        settings.insert(
            "Screen Resolution (Windowed)".to_string(),
            serde_json::Value::String(graphics.resolution.clone()),
        );
        settings.insert(
            "Framerate Cap".to_string(),
            serde_json::Value::Number(serde_json::Number::from(graphics.fps)),
        );
        if settings.contains_key("Framerate Target") {
            settings.insert(
                "Framerate Target".to_string(),
                serde_json::Value::Number(serde_json::Number::from(graphics.fps)),
            );
        }
    }
    if let Some(rect) = layout_rect {
        // Layout dimensions have the same meaning as account resolution.
        // Native placement must not resize the outer window to these values.
        settings.insert("Window Mode".into(), serde_json::json!(0));
        settings.insert(
            "Screen Resolution (Windowed)".into(),
            serde_json::json!(format!("{}x{}", rect.width, rect.height)),
        );
    }
    let content = serde_json::to_vec_pretty(&settings)?;
    std::fs::write(&path, content).map_err(|error| {
        AppError::FileError(format!(
            "写入方案临时 Settings.json 失败 ({}): {error}",
            path.display()
        ))
    })?;
    Ok(TemporarySettingsOverride { path, original })
}

fn persist_window_position(
    state: &SharedState,
    accounts_dir: &str,
    account_id: &str,
    position: (i32, i32),
) -> bool {
    // A legacy position watcher may still be alive when the user enables a
    // layout. Layout movement must never overwrite the account's saved capsule.
    if state.configuration().snapshot().is_some_and(|config| {
        config.window_layout_enabled && config.active_window_layout_id.is_some()
    }) {
        return true;
    }
    let Ok(_lease) = AccountLifecycleLease::try_acquire(state, account_id) else {
        return false;
    };
    let Ok(mut meta) = AccountManager::load_meta(accounts_dir, account_id) else {
        return false;
    };
    meta.window_x = Some(position.0);
    meta.window_y = Some(position.1);
    if let Some(active_id) = meta.active_position_id.as_deref() {
        if let Some(active) = meta
            .position_presets
            .iter_mut()
            .find(|preset| preset.id == active_id)
        {
            active.x = position.0;
            active.y = position.1;
        }
    }
    AccountManager::save_meta(accounts_dir, &meta).is_ok()
}

// ── 取消启动 ──

/// 前端点「停止」时调用，后端在下一个检查点中止所有未完成的账号
#[tauri::command]
pub fn cancel_launch(state: tauri::State<'_, SharedState>) -> Result<(), AppError> {
    for task in state.tasks().snapshots().into_iter().filter(|task| {
        task.state == TaskState::Running
            && matches!(
                task.kind.as_str(),
                "account-launch"
                    | "battle-net-launch"
                    | "account-initialize"
                    | "account-reinitialize"
            )
    }) {
        let _ = state.tasks().request_cancel(task.task_id);
    }
    state.multi_instance().facade().cancel_current_operation();
    Ok(())
}

fn finish_launch_task(task: TaskHandle, result: &Result<Vec<LaunchResult>, AppError>) {
    let cancellation_requested = task.cancellation_requested();
    match result {
        Ok(_) if cancellation_requested => {
            let _ = task.cancelled("启动已取消");
        }
        Ok(results) => {
            let failures = results.iter().filter(|result| !result.success).count();
            if failures == 0 {
                let _ = task.succeed("启动任务完成");
            } else {
                let _ = task.fail(
                    "account-launch-partial-failure",
                    &format!("启动任务完成，其中 {failures} 个账号未成功"),
                );
            }
        }
        Err(error) if cancellation_requested => {
            let _ = task.cancelled(&error.to_string());
        }
        Err(error) => {
            let _ = task.fail("account-launch-failed", &error.to_string());
        }
    }
}

/// 当前操作票据是否已被后续取消请求作废。
fn is_cancelled(state: &SharedState, ticket: CancellationTicket) -> bool {
    state.multi_instance().facade().is_cancelled(ticket)
}

fn account_path_error(account_id: &str, err: AppError) -> LaunchResult {
    LaunchResult {
        account_id: account_id.to_string(),
        success: false,
        d2r_pid: None,
        error: Some(err.to_string()),
        mutex_killed: false,
    }
}

fn account_window_title(meta: &crate::commands::account::AccountMeta) -> String {
    if meta.display_name.trim().is_empty() {
        meta.id.clone()
    } else {
        meta.display_name.clone()
    }
}

fn unique_account_window_executable(
    config: &GlobalConfig,
    meta: &crate::commands::account::AccountMeta,
) -> Option<PathBuf> {
    let title = account_window_title(meta);
    let expected = account_game_executable_identity(config, meta).ok()?;
    let matching_accounts = AccountManager::list_ids(&config.accounts_dir)
        .into_iter()
        .filter_map(|id| AccountManager::load_meta(&config.accounts_dir, &id).ok())
        .filter_map(|candidate| {
            let candidate_title = account_window_title(&candidate);
            let executable = account_game_executable_identity(config, &candidate).ok()?;
            Some((candidate_title, executable))
        })
        .filter(|(candidate_title, executable)| {
            candidate_title.eq_ignore_ascii_case(&title)
                && crate::infrastructure::system::executable_paths_match(executable, &expected)
        })
        .count();
    (matching_accounts == 1).then_some(expected)
}

fn already_running_result(
    app: &tauri::AppHandle,
    state: &SharedState,
    account_id: &str,
    window_title: &str,
    pid: u32,
) -> LaunchResult {
    let message = format!("已检测到同名游戏窗口“{window_title}”(PID: {pid})，已跳过重复启动");
    crate::logger::log_msg(
        "WARN",
        "Launch",
        &format!("[Account {account_id}] {message}"),
    );
    state
        .multi_instance()
        .instances()
        .record_discovered(account_id, pid);
    let _ = app.emit(
        "launch-progress",
        LaunchProgress::new(account_id, "done", "warning", &message),
    );
    LaunchResult {
        account_id: account_id.to_string(),
        success: true,
        d2r_pid: Some(pid),
        // 保留在响应中，前端据此给出非阻断提示；success=true 表明不是启动失败。
        error: Some(message),
        // 已存在的实例不会进入后续队列安全判断，视为无需处理互斥句柄。
        mutex_killed: true,
    }
}

fn skip_existing_account_window(
    app: &tauri::AppHandle,
    state: &SharedState,
    config: &GlobalConfig,
    account_id: &str,
    meta: &crate::commands::account::AccountMeta,
) -> Option<LaunchResult> {
    let window_title = account_window_title(meta);
    let expected_executable = unique_account_window_executable(config, meta)?;
    let windows = SystemGameWindowPort;
    let pid = windows.find_unique_process(&GameWindowIdentity {
        title: window_title.clone(),
        executable: expected_executable,
    })?;
    if config.separate_game_taskbar_icons {
        let app_id = format!("D2RHub.Account.{account_id}");
        if let Err(error) = windows.set_taskbar_identity(pid, &app_id) {
            crate::logger::log_msg("WARN", "Launch", &format!("[Account {account_id}] {error}"));
            let _ = app.emit(
                "launch-progress",
                LaunchProgress::new(account_id, "window", "warning", &error),
            );
        }
    }
    Some(already_running_result(
        app,
        state,
        account_id,
        &window_title,
        pid,
    ))
}

fn checked_account_dir(
    config: &GlobalConfig,
    account_id: &str,
) -> Result<std::path::PathBuf, LaunchResult> {
    AccountManager::account_dir_checked(&config.accounts_dir, account_id)
        .map_err(|e| account_path_error(account_id, e))
}

const UNIFIED_AUTH_SUBKEY: &str = r"Software\Blizzard Entertainment\Battle.net\UnifiedAuth";
const UNIFIED_AUTH_REG_SECTION: &str =
    r"HKEY_CURRENT_USER\Software\Blizzard Entertainment\Battle.net\UnifiedAuth";

fn validate_legacy_reg_sections(content: &str) -> Result<(), String> {
    let mut section_count = 0usize;
    for (line_index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('[') {
            continue;
        }
        if !trimmed.ends_with(']') {
            return Err(format!(
                "注册表文件第 {} 行包含未闭合的 section",
                line_index + 1
            ));
        }
        let section = trimmed[1..trimmed.len() - 1].trim();
        if !section.eq_ignore_ascii_case(UNIFIED_AUTH_REG_SECTION) {
            return Err(format!("注册表文件包含不允许的 section: [{section}]"));
        }
        section_count += 1;
    }
    if section_count == 0 {
        return Err("注册表文件不包含 UnifiedAuth section".to_string());
    }
    Ok(())
}

fn clear_unified_auth_registry_strict() -> Result<(), AppError> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.delete_subkey_all(UNIFIED_AUTH_SUBKEY) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::RegistryError(format!(
            "清空 UnifiedAuth 注册表失败: {error}"
        ))),
    }
}

fn replace_bnet_roaming_snapshot(source: &Path, target: &Path) -> Result<(), AppError> {
    if !source.is_dir() {
        return Err(AppError::FileError(format!(
            "Battle.net 快照目录不存在: {}",
            source.display()
        )));
    }
    let staged = sibling_with_suffix(target, ".tmp")?;
    let backup = sibling_with_suffix(target, ".bak")?;

    // 此处已持有 HostRuntimeLease；遗留 `.tmp` 必然来自中断事务，可安全丢弃。
    // 先恢复 `.bak`，保证后续复制失败时宿主仍保有上一份完整状态。
    remove_path_if_exists(&staged)?;
    recover_interrupted_replacement(target)?;
    crate::commands::utils::copy_dir_recursive(source, &staged).map_err(|error| {
        let _ = remove_path_if_exists(&staged);
        AppError::FileError(format!(
            "暂存 Battle.net 配置失败: {} -> {} ({error})",
            source.display(),
            staged.display()
        ))
    })?;
    replace_path_with_backup(&staged, target, &backup)
}

fn validate_bnet_snapshot(
    config: &GlobalConfig,
    meta: &crate::commands::account::AccountMeta,
    expected_edition: ClientEdition,
) -> Result<(), AppError> {
    let account_dir = AccountManager::account_dir_checked(&config.accounts_dir, &meta.id)?;
    let snapshot = resolve_account_runtime_snapshot(&account_dir, meta, expected_edition)?;
    if let RegistrySnapshotPath::LegacyReg(reg_path) = snapshot.registry {
        let bytes = std::fs::read(&reg_path)?;
        if bytes.is_empty() {
            return Err(AppError::ConfigReadError(format!(
                "账号 {} 的注册表快照为空",
                meta.id
            )));
        }
        let content = decode_reg_file(&bytes).ok_or_else(|| {
            AppError::ConfigReadError(format!("账号 {} 的注册表快照编码无法识别", meta.id))
        })?;
        validate_legacy_reg_sections(&content).map_err(|error| {
            AppError::ConfigReadError(format!("账号 {} 的注册表快照无效: {error}", meta.id))
        })?;
    }
    Ok(())
}

fn preflight_accounts(
    config: &GlobalConfig,
    account_ids: &[String],
    purpose: ContextPurpose,
) -> Result<(), AppError> {
    for account_id in account_ids {
        AccountManager::validate_account_id(account_id)?;
        let meta = AccountManager::load_meta(&config.accounts_dir, account_id)?;
        preflight_account_meta(config, &meta, purpose)?;
    }
    Ok(())
}

pub(crate) fn preflight_account_meta(
    config: &GlobalConfig,
    meta: &AccountMeta,
    purpose: ContextPurpose,
) -> Result<(), AppError> {
    let account_id = &meta.id;
    AccountManager::validate_account_id(account_id)?;
    let auth_mode = AuthMode::parse(meta.auth_mode.as_deref())?;
    if purpose == ContextPurpose::BattleNetOnly && auth_mode == AuthMode::Token {
        return Err(AppError::ConfigReadError(format!(
            "Token 认证账号不支持仅启动 Battle.net: {account_id}"
        )));
    }
    if !meta.initialized {
        return Err(AppError::ConfigReadError(format!(
            "账号 {account_id} 尚未初始化"
        )));
    }

    let context = LaunchContext::for_account(config, meta, purpose)?;
    // Settings.json 是可选能力。缺失、损坏或存档目录不可用时，具体启动流程
    // 会跳过画质覆盖并发出 warning，不能在这里阻断核心认证和 D2R.exe 启动。
    match auth_mode {
        AuthMode::Token => {
            let token = meta
                .token
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AppError::ConfigReadError(format!("账号 {account_id} 缺少 Token"))
                })?;
            let bytes = crate::commands::crypto::hex_decode(token).map_err(|error| {
                AppError::ConfigReadError(format!("账号 {account_id} 的 Token 数据损坏: {error}"))
            })?;
            if bytes.is_empty() {
                return Err(AppError::ConfigReadError(format!(
                    "账号 {account_id} 的 Token 数据为空"
                )));
            }
            if purpose == ContextPurpose::LaunchGame && !meta.mod_args.trim().is_empty() {
                parse_windows_command_line(&meta.mod_args).map_err(|error| {
                    AppError::ConfigReadError(format!(
                        "账号 {account_id} 的 Mod 启动参数无效: {error}"
                    ))
                })?;
            }
        }
        AuthMode::BattleNet => {
            validate_bnet_snapshot(config, meta, context.installation.edition)?;
        }
    }
    if purpose == ContextPurpose::LaunchGame {
        crate::mpq_mod::ensure_no_pending_conversion(
            &context.installation.game_directory.join("mods"),
            &meta.mod_args,
        )
        .map_err(AppError::ConfigReadError)?;
    }
    Ok(())
}

/// Terminate only Battle.net processes owned by this installation profile.
/// `/T` includes descendants of the matched process without claiming unrelated Agent.exe trees.
fn kill_battle_net_for_context(context: &LaunchContext, flush_before_kill: bool) {
    let Ok(expected_path) = context.battle_net_executable() else {
        return;
    };
    if flush_before_kill {
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }

    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);
    let process_ids: Vec<u32> = system
        .processes()
        .values()
        .filter(|process| {
            process
                .name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Battle.net.exe")
                && process.exe().is_some_and(|actual| {
                    crate::infrastructure::system::executable_paths_match(actual, expected_path)
                })
        })
        .map(|process| process.pid().as_u32())
        .collect();
    for process_id in process_ids {
        let _ = silent_cmd("taskkill")
            .args(["/F", "/T", "/PID", &process_id.to_string()])
            .output();
    }
}

/// 取消前检查战网状态：
/// - 已登录（进程≥7）：先优雅关闭战网让其 flush 注册表，再回写备份
/// - 运行中但未登录：直接强杀，不回写（注册表中是刚恢复的旧数据，无保存价值）
async fn cancel_with_cleanup(
    config: &GlobalConfig,
    context: &LaunchContext,
    account_id: &str,
    preserved_default_mod_args: Option<&str>,
) -> LaunchResult {
    // 取消清理只能认领当前 Launch Context 对应的 Battle.net，绝不回退到其他版本。
    let bnet_count = context
        .battle_net_executable()
        .ok()
        .map(|path| {
            crate::infrastructure::system::count_bnet_processes_for_path(&path.to_string_lossy())
        })
        .unwrap_or(0);
    let bnet_logged_in = bnet_count >= BNET_LOGIN_PROCESS_COUNT_THRESHOLD;

    if bnet_logged_in {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!(
                "[Account {}] 取消时检测到战网已登录 ({}进程)，先关闭战网再回写认证状态...",
                account_id, bnet_count
            ),
        );

        // 给当前版本的 BNet 时间 flush，再只终止其进程树。
        kill_battle_net_for_context(context, true);

        let account_dir = match checked_account_dir(config, account_id) {
            Ok(dir) => dir,
            Err(res) => return res,
        };
        let config_clone = config.clone();
        let account_dir_clone = account_dir.clone();
        let preserved_default_mod_args = preserved_default_mod_args.map(str::to_string);
        let sync_res =
            tokio::task::spawn_blocking(move || match preserved_default_mod_args.as_deref() {
                Some(default_mod_args) => {
                    crate::commands::account::sync_back_to_account_preserving_mod(
                        &account_dir_clone,
                        &config_clone,
                        default_mod_args,
                    )
                }
                None => crate::commands::account::sync_back_to_account(
                    &account_dir_clone,
                    &config_clone,
                ),
            })
            .await;

        match sync_res {
            Ok(Ok(())) => {
                crate::logger::log_msg(
                    "INFO",
                    "Launch",
                    &format!("[Account {}] 取消完成，认证状态已回写", account_id),
                );
            }
            Ok(Err(e)) => {
                crate::logger::log_msg(
                    "WARN",
                    "Launch",
                    &format!(
                        "[Account {}] 取消完成，但认证状态回写失败: {}",
                        account_id, e
                    ),
                );
            }
            Err(e) => {
                crate::logger::log_msg(
                    "WARN",
                    "Launch",
                    &format!(
                        "[Account {}] 取消完成，但认证状态回写线程异常: {:?}",
                        account_id, e
                    ),
                );
            }
        }
    } else if bnet_count > 0 {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!(
                "[Account {}] 取消时战网未登录 ({}进程)，直接关闭不保存",
                account_id, bnet_count
            ),
        );
        kill_battle_net_for_context(context, false);
    }

    LaunchResult {
        account_id: account_id.to_string(),
        success: false,
        d2r_pid: None,
        error: Some("启动已被用户取消".to_string()),
        mutex_killed: false,
    }
}

// ── 一键启动 ──

/// 只启动战网（不走游戏、互斥、连接等后续步骤）
#[tauri::command]
pub async fn launch_battle_net_only(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_ids: Vec<String>,
) -> Result<Vec<LaunchResult>, AppError> {
    let shared = state.inner().clone();
    let _admission = shared
        .launch_admission
        .try_lock()
        .map_err(|_| AppError::ConfigReadError("另一个启动任务正在执行".into()))?;
    launch_battle_net_only_task(app, state, account_ids, None).await
}

async fn launch_battle_net_only_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_ids: Vec<String>,
    retry_of: Option<u64>,
) -> Result<Vec<LaunchResult>, AppError> {
    crate::mod_catalog::recover_before_launch(state.inner(), &app)
        .map_err(AppError::ConfigReadError)?;
    let cancellation_ticket = state.multi_instance().facade().cancellation_ticket();
    let cancel_state = state.inner().clone();
    let retry_payload = serde_json::to_string(&LaunchTaskRetryPayload::BattleNetOnly {
        account_ids: account_ids.clone(),
    })
    .map_err(|error| AppError::Unknown(format!("创建任务重试数据失败: {error}")))?;
    let mut request = TaskRequest::new("battle-net-launch")
        .with_conflict_key("host-runtime-launch")
        .with_retry_payload(retry_payload)
        .with_cancel_hook(move || {
            cancel_state
                .multi_instance()
                .facade()
                .cancel(cancellation_ticket);
        })
        .with_initial_status("preflight", "正在检查 Battle.net 启动请求");
    if let Some(retry_of) = retry_of {
        request = request.with_retry_of(retry_of);
    }
    let task = state
        .tasks()
        .begin(request)
        .map_err(|error| AppError::Unknown(error.to_string()))?;
    let completion_state = state.inner().clone();
    let result = launch_battle_net_only_impl(app, state, account_ids, cancellation_ticket).await;
    completion_state
        .multi_instance()
        .facade()
        .complete(cancellation_ticket);
    finish_launch_task(task, &result);
    result
}

async fn launch_battle_net_only_impl(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_ids: Vec<String>,
    cancellation_ticket: CancellationTicket,
) -> Result<Vec<LaunchResult>, AppError> {
    let plan = LaunchBatchPlan::from_request(Some(account_ids), None)?;
    if plan.is_empty() {
        return Ok(Vec::new());
    }
    let account_ids = plan.account_ids();
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let mut results = Vec::new();
    let total = account_ids.len();
    let mut host_runtime_lease: Option<HostRuntimeLease> = None;

    for (i, account_id) in account_ids.iter().enumerate() {
        // 批次创建时已取得取消票据；只有之后发生的取消请求会使其失效。
        // 新批次不会继承旧取消状态，也不能清除另一个仍在运行批次的取消结果。
        if host_runtime_lease.is_some() && is_cancelled(&state, cancellation_ticket) {
            emit_cancelled(&app, account_id);
            results.push(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("启动已被用户取消".to_string()),
                mutex_killed: false,
            });
            for remaining in &account_ids[i + 1..] {
                emit_cancelled(&app, remaining);
                results.push(LaunchResult {
                    account_id: remaining.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some("启动已被用户取消".to_string()),
                    mutex_killed: false,
                });
            }
            return Ok(results);
        }

        let _account_lease = match AccountLifecycleLease::try_acquire(state.inner(), account_id) {
            Ok(lease) => lease,
            Err(error) => {
                let message = error.to_string();
                let _ = app.emit(
                    "launch-progress",
                    LaunchProgress::new(account_id, "done", "error", &message),
                );
                results.push(account_path_error(account_id, error));
                continue;
            }
        };
        if let Err(error) = preflight_accounts(
            &config,
            std::slice::from_ref(account_id),
            ContextPurpose::BattleNetOnly,
        ) {
            let message = error.to_string();
            let _ = app.emit(
                "launch-progress",
                LaunchProgress::new(account_id, "done", "error", &message),
            );
            results.push(account_path_error(account_id, error));
            continue;
        }

        if host_runtime_lease.is_none() {
            match HostRuntimeLease::try_acquire_for_launch(state.inner().as_ref()) {
                Ok(lease) => {
                    host_runtime_lease = Some(lease);
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = app.emit(
                        "launch-progress",
                        LaunchProgress::new(account_id, "done", "error", &message),
                    );
                    results.push(account_path_error(account_id, error));
                    continue;
                }
            }
        }

        if is_cancelled(&state, cancellation_ticket) {
            emit_cancelled(&app, account_id);
            results.push(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("启动已被用户取消".to_string()),
                mutex_killed: false,
            });
            for remaining in &account_ids[i + 1..] {
                emit_cancelled(&app, remaining);
                results.push(LaunchResult {
                    account_id: remaining.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some("启动已被用户取消".to_string()),
                    mutex_killed: false,
                });
            }
            return Ok(results);
        }

        let msg = format!("[{}/{}] 仅启动战网", i + 1, total);
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [queue] [running]: {}", account_id, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, "queue", "running", &msg),
        );

        let result =
            launch_single_bnet_only(&app, &config, &state, cancellation_ticket, account_id).await;
        crate::logger::log_msg(
            if result.success { "INFO" } else { "ERROR" },
            "Launch",
            &format!(
                "[Account {}] 仅启动战网结果: success={}, error={:?}",
                account_id, result.success, result.error
            ),
        );
        results.push(result);

        if i + 1 < total && !is_cancelled(&state, cancellation_ticket) {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    Ok(results)
}

async fn prepare_bnet_environment(
    app: &tauri::AppHandle,
    config: &GlobalConfig,
    state: &SharedState,
    cancellation_ticket: CancellationTicket,
    account_id: &str,
    wait_login: bool,
    options: BnetPreparationOptions<'_>,
) -> Result<LaunchContext, LaunchResult> {
    let BnetPreparationOptions {
        meta_override,
        preserved_default_mod_args,
        require_settings_copy,
    } = options;
    // Resolve the complete account context before any process, file, or registry mutation.
    let meta = match meta_override {
        Some(meta) => meta.clone(),
        None => AccountManager::load_meta(&config.accounts_dir, account_id)
            .map_err(|error| account_path_error(account_id, error))?,
    };
    let auth_mode = AuthMode::parse(meta.auth_mode.as_deref())
        .map_err(|error| account_path_error(account_id, error.into()))?;
    if auth_mode != AuthMode::BattleNet {
        return Err(account_path_error(
            account_id,
            AppError::ConfigReadError("Token 认证账号不能进入 Battle.net 启动流程".to_string()),
        ));
    }
    let purpose = if wait_login {
        ContextPurpose::BattleNetOnly
    } else {
        ContextPurpose::LaunchGame
    };
    let context = LaunchContext::for_account(config, &meta, purpose)
        .map_err(|error| account_path_error(account_id, error))?;
    let battle_net_path = context
        .battle_net_executable()
        .map_err(|error| account_path_error(account_id, error))?
        .to_string_lossy()
        .to_string();
    let saved_games_path = context.installation.saved_games_directory.clone();
    let battle_net_config_game_key = context.edition.battle_net_config_game_key;
    let has_customized_settings = meta.has_customized_settings;
    let mod_args = meta.mod_args.clone();

    let emit = |step: &str, status: &str, msg: &str| {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [{}] [{}]: {}", account_id, step, status, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, step, status, msg),
        );
    };

    let cancelled = || -> LaunchResult {
        LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some("启动已被用户取消".to_string()),
            mutex_killed: false,
        }
    };

    // ── Step 1: 环境清理 ──
    emit("clean", "running", "正在清理战网和 Agent 进程...");
    let clean_res = tokio::task::spawn_blocking(|| {
        crate::commands::utils::kill_processes_by_name(&["Battle.net.exe", "Agent.exe"])
    })
    .await;

    match clean_res {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let message = format!("清理共享进程失败: {error}");
            emit("clean", "error", &message);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(message),
                mutex_killed: false,
            });
        }
        Err(error) => {
            let message = format!("清理环境线程异常: {error}");
            emit("clean", "error", &message);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(message),
                mutex_killed: false,
            });
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    emit("clean", "ok", "环境已清理");

    if is_cancelled(state, cancellation_ticket) {
        emit("done", "error", "已取消");
        return Err(cancelled());
    }

    // ── Step 2: 配置覆盖 ──
    emit("copy", "running", "正在覆盖配置文件...");

    let accounts_dir = config.accounts_dir.clone();
    let app_data_roaming_bnet_path = config.app_data_roaming_bnet_path.clone();
    let account_id_str = account_id.to_string();
    let snapshot_meta = meta.clone();
    let snapshot_edition = context.installation.edition;

    let copy_res = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        let account_dir = AccountManager::account_dir_checked(&accounts_dir, &account_id_str)
            .map_err(|e| e.to_string())?;
        let mut optional_warning = None;

        let snapshot =
            resolve_account_runtime_snapshot(&account_dir, &snapshot_meta, snapshot_edition)
                .map_err(|error| format!("解析账号运行时快照失败: {error}"))?;

        // 2.1 精确替换 Battle.net Roaming 配置
        let bnet_dst = Path::new(&app_data_roaming_bnet_path);
        replace_bnet_roaming_snapshot(&snapshot.bnet_directory, bnet_dst)
            .map_err(|error| error.to_string())?;

        // 2.2 导入注册表（已初始化账号必须成功，否则战网无法自动登录）
        match snapshot.registry {
            RegistrySnapshotPath::Json(json_path) => {
                // JSON 恢复函数在验证完整快照后自行清空并写入，调用方不得重复预清空。
                crate::commands::account::restore_registry_from_json(&json_path)
                    .map_err(|e| format!("恢复注册表失败: {e}"))?;
            }
            RegistrySnapshotPath::LegacyReg(reg_path) => {
                let reg_bytes =
                    std::fs::read(&reg_path).map_err(|e| format!("读取注册表文件失败: {e}"))?;
                if reg_bytes.is_empty() {
                    return Err("注册表文件为空，导入被拒绝".to_string());
                }
                let reg_content = decode_reg_file(&reg_bytes).ok_or_else(|| {
                    "注册表文件编码无法识别（需 UTF-8 或 UTF-16LE），导入被拒绝".to_string()
                })?;
                validate_legacy_reg_sections(&reg_content)?;
                clear_unified_auth_registry_strict().map_err(|error| error.to_string())?;
                let output = silent_cmd("reg")
                    .args(["import", &reg_path.to_string_lossy()])
                    .output()
                    .map_err(|e| format!("执行 reg import 失败: {e}"))?;
                if !output.status.success() {
                    let import_error = format!(
                        "reg import 返回非零退出码: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    return match clear_unified_auth_registry_strict() {
                        Ok(()) => Err(import_error),
                        Err(cleanup_error) => Err(format!(
                            "{import_error}；清理部分导入也失败: {cleanup_error}"
                        )),
                    };
                }
            }
        }

        // 2.3 覆盖 Settings.json
        if has_customized_settings {
            match saved_games_path.as_deref() {
                Some(saved_games_path) => {
                    if let Err(error) =
                        copy_account_settings_to_system(&account_dir, saved_games_path)
                    {
                        if require_settings_copy {
                            return Err(format!("独立 Settings.json 覆盖失败: {error}"));
                        }
                        optional_warning = Some(format!(
                            "独立 Settings.json 覆盖失败，已继续使用系统配置: {error}"
                        ));
                    }
                }
                None => {
                    if require_settings_copy {
                        return Err("未配置可用的存档目录，无法应用方案画质".to_string());
                    }
                    optional_warning =
                        Some("未配置可用的存档目录，已跳过独立 Settings.json 覆盖".to_string());
                }
            }
        } else {
            crate::logger::log_msg(
                "INFO",
                "Launch",
                &format!(
                    "[Account {}] 使用系统 Settings.json，跳过账号画质配置覆盖",
                    account_id_str
                ),
            );
        }

        let bnet_config_path = bnet_dst.join("Battle.net.config");

        // 2.3.5 注入 Mod 参数
        if bnet_config_path.exists() {
            update_mod_args(&bnet_config_path, battle_net_config_game_key, &mod_args)
                .map_err(|error| format!("注入 Mod 参数失败: {error}"))?;
        }

        // 2.4 强制确保 SingleInstance
        if let Err(e) = crate::commands::account::enforce_single_instance(&bnet_config_path) {
            crate::logger::log_msg(
                "WARN",
                "Launch",
                &format!(
                    "[Account {}] SingleInstance 校验失败: {}",
                    account_id_str, e
                ),
            );
        }

        Ok(optional_warning)
    })
    .await;

    match copy_res {
        Ok(Ok(optional_warning)) => {
            if let Some(warning) = optional_warning {
                emit("copy", "warning", &warning);
            }
            emit("copy", "ok", "配置文件覆盖完成");
        }
        Ok(Err(e)) => {
            emit("copy", "error", &e);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(e),
                mutex_killed: false,
            });
        }
        Err(_) => {
            let msg = "执行配置覆盖线程异常".to_string();
            emit("copy", "error", &msg);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(msg),
                mutex_killed: false,
            });
        }
    }

    if is_cancelled(state, cancellation_ticket) {
        emit("done", "error", "已取消");
        return Err(cancelled());
    }

    // ── Step 3: 启动战网并等待登录 ──
    emit("launch", "running", "正在启动战网客户端...");

    // 基础安全校验：确保路径指向预期的可执行文件
    if !battle_net_path.to_lowercase().ends_with("battle.net.exe") {
        let msg = format!("战网路径异常，预期 Battle.net.exe: {}", battle_net_path);
        emit("launch", "error", &msg);
        return Err(LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some(msg),
            mutex_killed: false,
        });
    }
    let battle_net_spawn_path = battle_net_path.clone();
    let spawn_res =
        tokio::task::spawn_blocking(move || Command::new(&battle_net_spawn_path).spawn()).await;

    match spawn_res {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            let msg = format!("启动战网失败: {}", e);
            emit("launch", "error", &msg);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(msg),
                mutex_killed: false,
            });
        }
        Err(_) => {
            let msg = "启动战网线程异常".to_string();
            emit("launch", "error", &msg);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(msg),
                mutex_killed: false,
            });
        }
    }

    if !wait_login {
        // 游戏新进程启动前由 launch_single 主循环重复检查并强杀 Agent.exe
        emit("launch", "ok", "战网客户端已启动，进入进程与 Agent 监控...");
    } else {
        let mut bnet_ready = false;
        for i in 1..=60 {
            if is_cancelled(state, cancellation_ticket) {
                emit("done", "error", "已取消，正在保存状态...");
                return Err(cancel_with_cleanup(
                    config,
                    &context,
                    account_id,
                    preserved_default_mod_args,
                )
                .await);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let count =
                crate::infrastructure::system::count_bnet_processes_for_path(&battle_net_path);
            if count >= BNET_LOGIN_PROCESS_COUNT_THRESHOLD {
                bnet_ready = true;
                emit(
                    "launch",
                    "ok",
                    &format!("战网已登录 ({}s, {}进程)", i, count),
                );
                break;
            }
            if i % 5 == 0 {
                emit(
                    "launch",
                    "running",
                    &format!("等待战网登录... ({}s, {}进程)", i, count),
                );
            }
        }
        if !bnet_ready {
            emit("launch", "warning", "未检测到战网登录，但战网已启动");
        } else {
            // 登录成功，立即回写 token 快照作为检查点
            // 后续无论游戏崩溃、用户取消、还是进程被杀，至少保留一份刚登录时的有效 token
            emit("checkpoint", "running", "登录成功，保存认证检查点...");
            let account_dir = checked_account_dir(config, account_id)?;
            let config_clone = config.clone();
            let account_dir_clone = account_dir.clone();
            let sync_res = tokio::task::spawn_blocking(move || {
                crate::commands::account::sync_back_to_account(&account_dir_clone, &config_clone)
            })
            .await;
            match sync_res {
                Ok(Ok(())) => {
                    emit("checkpoint", "ok", "认证检查点已保存");
                }
                Ok(Err(e)) => {
                    emit(
                        "checkpoint",
                        "warning",
                        &format!("保存认证检查点失败: {}", e),
                    );
                }
                Err(_) => {
                    emit("checkpoint", "warning", "保存认证检查点线程异常");
                }
            }
        }
    }

    if is_cancelled(state, cancellation_ticket) {
        emit("done", "error", "已取消，正在保存状态...");
        return Err(
            cancel_with_cleanup(config, &context, account_id, preserved_default_mod_args).await,
        );
    }

    Ok(context)
}

async fn launch_single_bnet_only(
    app: &tauri::AppHandle,
    config: &GlobalConfig,
    state: &SharedState,
    cancellation_ticket: CancellationTicket,
    account_id: &str,
) -> LaunchResult {
    let emit = |step: &str, status: &str, msg: &str| {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [{}] [{}]: {}", account_id, step, status, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, step, status, msg),
        );
    };

    if let Err(res) = prepare_bnet_environment(
        app,
        config,
        state,
        cancellation_ticket,
        account_id,
        true,
        BnetPreparationOptions::default(),
    )
    .await
    {
        return res;
    }

    // Step 4: 回写最新认证状态（战网登录后 token 可能已刷新）
    emit("cleanup", "running", "正在回写认证状态...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let account_dir = match checked_account_dir(config, account_id) {
        Ok(dir) => dir,
        Err(res) => return res,
    };

    let config_clone = config.clone();
    let account_dir_clone = account_dir.clone();
    let sync_res = tokio::task::spawn_blocking(move || {
        crate::commands::account::sync_back_to_account(&account_dir_clone, &config_clone)
    })
    .await;

    match sync_res {
        Ok(Ok(())) => {
            emit("cleanup", "ok", "认证状态已同步");
        }
        Ok(Err(e)) => {
            emit("cleanup", "warning", &format!("回写状态失败: {}", e));
        }
        Err(_) => {
            emit("cleanup", "warning", "回写状态线程异常");
        }
    }

    emit("done", "ok", "战网启动完成（仅启动战网）");
    LaunchResult {
        account_id: account_id.to_string(),
        success: true,
        d2r_pid: None,
        error: None,
        mutex_killed: false,
    }
}

async fn reopen_battle_net_after_launch<'a>(
    app: &tauri::AppHandle,
    config: &GlobalConfig,
    state: &'a SharedState,
    cancellation_ticket: CancellationTicket,
    account_id: &str,
    host_runtime_lease: &mut Option<HostRuntimeLease<'a>>,
) -> LaunchResult {
    // The game's lifecycle lease has been released. Reacquire it and revalidate
    // the saved account before restoring shared Battle.net files and registry.
    let _account_lease = match AccountLifecycleLease::try_acquire(state, account_id) {
        Ok(lease) => lease,
        Err(error) => return account_path_error(account_id, error),
    };
    if let Err(error) = preflight_accounts(
        config,
        &[account_id.to_string()],
        ContextPurpose::BattleNetOnly,
    ) {
        return account_path_error(account_id, error);
    }
    // A batch consisting entirely of existing windows has not acquired the
    // host lease yet; reopening Battle.net still needs exclusive ownership.
    if host_runtime_lease.is_none() {
        match HostRuntimeLease::try_acquire_for_launch(state.as_ref()) {
            Ok(lease) => *host_runtime_lease = Some(lease),
            Err(error) => return account_path_error(account_id, error),
        }
    }
    if is_cancelled(state, cancellation_ticket) {
        return account_path_error(account_id, AppError::Unknown("启动已被用户取消".into()));
    }

    let _ = app.emit(
        "launch-progress",
        LaunchProgress::new(
            account_id,
            "keep-battle-net",
            "running",
            "多开流程已完成，正在重新打开所选账号的战网...",
        ),
    );
    launch_single_bnet_only(app, config, state, cancellation_ticket, account_id).await
}

/// 一键启动选中的账号列表
/// 逐个串行启动：一个账号完整走完（清理→覆盖→启动战网→游戏→清互斥→连接→关战网）
/// 再开始下一个。两个账号之间留 2 秒缓冲。
#[tauri::command]
pub async fn launch_accounts(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_ids: Option<Vec<String>>,
    entries: Option<Vec<LaunchAccountEntry>>,
) -> Result<Vec<LaunchResult>, AppError> {
    let shared = state.inner().clone();
    let _admission = shared
        .launch_admission
        .try_lock()
        .map_err(|_| AppError::ConfigReadError("另一个启动任务正在执行".into()))?;
    let (account_ids, entries) = resolve_session_launch(&app, &state, account_ids, entries)?;
    launch_accounts_task(app, state, account_ids, entries, None).await
}

type AccountLaunchRequest = (Option<Vec<String>>, Option<Vec<LaunchAccountEntry>>);

fn resolve_session_launch(
    _app: &tauri::AppHandle,
    _state: &tauri::State<'_, SharedState>,
    ids: Option<Vec<String>>,
    entries: Option<Vec<LaunchAccountEntry>>,
) -> Result<AccountLaunchRequest, AppError> {
    Ok((ids, entries))
}

#[tauri::command]
pub(crate) async fn launch_saved_scheme(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    id: String,
    revision: u64,
) -> Result<Vec<LaunchResult>, AppError> {
    let shared = state.inner().clone();
    let _admission = shared
        .launch_admission
        .try_lock()
        .map_err(|_| AppError::ConfigReadError("另一个启动任务正在执行".into()))?;
    let entries =
        crate::commands::launch_scheme::prepare_scheme_launch(&app, &state, &id, revision)?;
    let result = launch_accounts_task(app.clone(), state.clone(), None, Some(entries), None).await;
    let survivors = crate::commands::account::list_accounts(state.clone())?
        .iter()
        .any(|account| account.is_running);
    if !survivors {
        crate::commands::global_config::mutate_loaded_global_config(
            state.inner(),
            &app,
            |config| {
                config.launch_session = None;
                Ok(true)
            },
        )?;
    }
    result
}

pub(crate) fn preflight_scheme_entries(
    config: &GlobalConfig,
    state: &SharedState,
    entries: &[LaunchAccountEntry],
) -> Result<(), AppError> {
    let plan = LaunchBatchPlan::from_request(None, Some(entries.to_vec()))?;
    if plan.is_empty() {
        return Err(AppError::ConfigReadError("方案至少需要一个账号".into()));
    }
    crate::commands::window_layout::prepare_launch_layout(config, state, plan.account_ids())?;
    for id in plan.account_ids() {
        let account = AccountManager::load_meta(&config.accounts_dir, id)?;
        let (account, graphics) = plan.apply_for(id, account, config.window_layout_enabled)?;
        preflight_account_meta(config, &account, ContextPurpose::LaunchGame)?;
        let context = LaunchContext::for_account(config, &account, ContextPurpose::LaunchGame)?;
        preflight_scheme_mod(&account, &context)?;
        preflight_launch_graphics(
            config,
            &account,
            &context,
            graphics.as_ref(),
            config.window_layout_enabled,
        )?;
    }
    Ok(())
}

async fn launch_accounts_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_ids: Option<Vec<String>>,
    entries: Option<Vec<LaunchAccountEntry>>,
    retry_of: Option<u64>,
) -> Result<Vec<LaunchResult>, AppError> {
    crate::mod_catalog::recover_before_launch(state.inner(), &app)
        .map_err(AppError::ConfigReadError)?;
    let cancellation_ticket = state.multi_instance().facade().cancellation_ticket();
    let cancel_state = state.inner().clone();
    let retry_payload = serde_json::to_string(&LaunchTaskRetryPayload::Accounts {
        session_id: state
            .configuration()
            .snapshot()
            .and_then(|config| config.launch_session)
            .map(|session| session.id),
        account_ids: account_ids.clone(),
        entries: entries.clone(),
    })
    .map_err(|error| AppError::Unknown(format!("创建任务重试数据失败: {error}")))?;
    let mut request = TaskRequest::new("account-launch")
        .with_conflict_key("host-runtime-launch")
        .with_retry_payload(retry_payload)
        .with_cancel_hook(move || {
            cancel_state
                .multi_instance()
                .facade()
                .cancel(cancellation_ticket);
        })
        .with_initial_status("preflight", "正在检查账号启动请求");
    if let Some(retry_of) = retry_of {
        request = request.with_retry_of(retry_of);
    }
    let task = state
        .tasks()
        .begin(request)
        .map_err(|error| AppError::Unknown(error.to_string()))?;
    let completion_state = state.inner().clone();
    let events = app.clone();
    let result = launch_accounts_impl(app, state, account_ids, entries, cancellation_ticket).await;
    if let Ok(results) = &result {
        for result in results {
            let _ = events.emit(
                "account-settings-updated",
                serde_json::json!({ "accountId": result.account_id }),
            );
        }
    }
    completion_state
        .multi_instance()
        .facade()
        .complete(cancellation_ticket);
    finish_launch_task(task, &result);
    result
}

pub(crate) async fn retry_launch_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    retry_of: u64,
    payload: LaunchTaskRetryPayload,
) -> Result<Vec<LaunchResult>, AppError> {
    let shared = state.inner().clone();
    let _admission = shared
        .launch_admission
        .try_lock()
        .map_err(|_| AppError::ConfigReadError("另一个启动任务正在执行".into()))?;
    match payload {
        LaunchTaskRetryPayload::BattleNetOnly { account_ids } => {
            launch_battle_net_only_task(app, state, account_ids, Some(retry_of)).await
        }
        LaunchTaskRetryPayload::Accounts {
            account_ids,
            entries,
            session_id,
        } => {
            let _ = session_id; // Retry the original action, independent of the latest recipe.
            launch_accounts_task(
                app,
                state,
                if entries.is_some() { None } else { account_ids },
                entries,
                Some(retry_of),
            )
            .await
        }
    }
}

async fn launch_accounts_impl(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_ids: Option<Vec<String>>,
    entries: Option<Vec<LaunchAccountEntry>>,
    cancellation_ticket: CancellationTicket,
) -> Result<Vec<LaunchResult>, AppError> {
    let plan = LaunchBatchPlan::from_request(account_ids, entries)?;
    if plan.is_empty() {
        return Ok(Vec::new());
    }
    let account_ids = plan.account_ids();
    let config = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;

    // 启动方案必须在触碰共享注册表、Battle.net 或 Settings.json 前整组完成预检。
    let layout_config = config.clone();
    let layout_state = state.inner().clone();
    let layout_ids = account_ids.to_vec();
    let mut layout_plan = tokio::task::spawn_blocking(move || {
        crate::commands::window_layout::prepare_launch_layout(
            &layout_config,
            &layout_state,
            &layout_ids,
        )
    })
    .await
    .map_err(|error| AppError::FileError(error.to_string()))??;
    if plan.has_overrides() || layout_plan.is_some() {
        for account_id in account_ids {
            if !plan.has_overrides()
                && layout_plan
                    .as_ref()
                    .is_some_and(|layout| layout.is_existing(account_id))
            {
                continue;
            }
            let meta = AccountManager::load_meta(&config.accounts_dir, account_id)?;
            let (effective_meta, graphics) =
                plan.apply_for(account_id, meta, layout_plan.is_some())?;
            preflight_account_meta(&config, &effective_meta, ContextPurpose::LaunchGame)?;
            let context =
                LaunchContext::for_account(&config, &effective_meta, ContextPurpose::LaunchGame)?;
            if plan.has_overrides() {
                preflight_scheme_mod(&effective_meta, &context)?;
            }
            preflight_launch_graphics(
                &config,
                &effective_meta,
                &context,
                graphics.as_ref(),
                layout_plan.is_some(),
            )?;
        }
    }
    let mut results = Vec::new();
    let total = account_ids.len();
    #[cfg(target_os = "windows")]
    let mut memory_trim = crate::infrastructure::memory_trim::BatchMemoryTrim::new(total);
    // 同名窗口检查不修改共享状态，因此不应被宿主运行时租约阻断。直到确实有账号
    // 要启动时才取得租约，并一直持有到本批次结束。
    let mut host_runtime_lease: Option<HostRuntimeLease> = None;

    for (i, account_id) in account_ids.iter().enumerate() {
        // 先按精确窗口标题检查。命中后不执行注册表、Settings、Battle.net 或 D2R
        // 的任何启动步骤，且不会阻断同一批次中的其他账号。
        let meta = match AccountManager::load_meta(&config.accounts_dir, account_id) {
            Ok(meta) => meta,
            Err(error) => {
                let result = account_path_error(account_id, error);
                let message = result
                    .error
                    .clone()
                    .unwrap_or_else(|| "账号配置无效".to_string());
                let _ = app.emit(
                    "launch-progress",
                    LaunchProgress::new(account_id, "done", "error", &message),
                );
                results.push(result);
                continue;
            }
        };
        if let Some(result) =
            skip_existing_account_window(&app, state.inner(), &config, account_id, &meta)
        {
            results.push(result);
            continue;
        }

        // 同名窗口检查是只读的，优先执行；只有确实需要启动时才取得该账号的生命周期
        // 租约。账号编辑/导入占用只影响当前账号，不阻断同批次的其他账号。
        let _account_lease = match AccountLifecycleLease::try_acquire(state.inner(), account_id) {
            Ok(lease) => lease,
            Err(error) => {
                let message = error.to_string();
                let _ = app.emit(
                    "launch-progress",
                    LaunchProgress::new(account_id, "done", "error", &message),
                );
                results.push(account_path_error(account_id, error));
                continue;
            }
        };
        // 关闭首次只读检查与取得租约之间的竞态窗口，租约内重新读取一次元数据。
        let meta = match AccountManager::load_meta(&config.accounts_dir, account_id) {
            Ok(meta) => meta,
            Err(error) => {
                results.push(account_path_error(account_id, error));
                continue;
            }
        };
        let preserved_default_mod_args =
            plan.override_for(account_id).map(|_| meta.mod_args.clone());
        let (meta, graphics_override) =
            match plan.apply_for(account_id, meta, layout_plan.is_some()) {
                Ok(effective) => effective,
                Err(error) => {
                    results.push(account_path_error(account_id, error));
                    continue;
                }
            };
        if let Some(result) =
            skip_existing_account_window(&app, state.inner(), &config, account_id, &meta)
        {
            results.push(result);
            continue;
        }

        // 单个账号的只读预检失败只影响该账号；未触碰共享宿主状态，可以继续下一项。
        if let Err(error) = preflight_account_meta(&config, &meta, ContextPurpose::LaunchGame) {
            let message = error.to_string();
            let _ = app.emit(
                "launch-progress",
                LaunchProgress::new(account_id, "done", "error", &message),
            );
            results.push(account_path_error(account_id, error));
            continue;
        }
        let preflight_context =
            match LaunchContext::for_account(&config, &meta, ContextPurpose::LaunchGame) {
                Ok(context) => context,
                Err(error) => {
                    results.push(account_path_error(account_id, error));
                    continue;
                }
            };
        if plan.override_for(account_id).is_some() {
            if let Err(error) = preflight_scheme_mod(&meta, &preflight_context) {
                results.push(account_path_error(account_id, error));
                continue;
            }
        }
        if let Err(error) = preflight_launch_graphics(
            &config,
            &meta,
            &preflight_context,
            graphics_override.as_ref(),
            layout_plan.is_some(),
        ) {
            results.push(account_path_error(account_id, error));
            continue;
        }

        if host_runtime_lease.is_none() {
            // Token 启动同样会覆盖机器级 Launch Options\OSI 与 Settings.json；只在
            // 真正启动前取得宿主租约，避免并发流程串号或串配置。
            match HostRuntimeLease::try_acquire_for_launch(state.inner().as_ref()) {
                Ok(lease) => {
                    host_runtime_lease = Some(lease);
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = app.emit(
                        "launch-progress",
                        LaunchProgress::new(account_id, "done", "error", &message),
                    );
                    results.push(account_path_error(account_id, error));
                    continue;
                }
            }
        }

        // 每个真正需要启动的账号开始前检查取消标志；已存在的同名窗口仍按上方
        // 逻辑直接跳过并提示，不会被取消状态误报为失败。
        if is_cancelled(&state, cancellation_ticket) {
            emit_cancelled(&app, account_id);
            results.push(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("启动已被用户取消".to_string()),
                mutex_killed: false,
            });
            // 剩余未启动的账号也标记为取消
            for remaining in &account_ids[i + 1..] {
                emit_cancelled(&app, remaining);
                results.push(LaunchResult {
                    account_id: remaining.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some("启动已被用户取消".to_string()),
                    mutex_killed: false,
                });
            }
            break;
        }

        let msg = format!("[{}/{}] 开始启动账号", i + 1, total);
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [queue] [running]: {}", account_id, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, "queue", "running", &msg),
        );

        // 当前账号的生命周期租约保证本次启动期间元数据稳定。
        // 只有默认启动且明确选择了位置胶囊时，才把用户后续拖动写回该胶囊。
        let layout_rect = layout_plan
            .as_ref()
            .and_then(|layout| layout.target(account_id));
        let persist_position_changes =
            layout_rect.is_none() && plan.should_persist_position_changes_for(account_id, &meta);
        let result = launch_single(
            &app,
            &config,
            &state,
            cancellation_ticket,
            account_id,
            meta,
            LaunchExecutionOptions {
                persist_position_changes,
                preserved_default_mod_args,
                graphics_override,
                layout_rect,
            },
            #[cfg(target_os = "windows")]
            &mut memory_trim,
        )
        .await;
        if result.d2r_pid.is_some() {
            if let Some(layout) = &mut layout_plan {
                layout.record_started(account_id);
            }
        }
        let killed = result.mutex_killed;
        let success = result.success;
        let can_continue = launch_queue_can_continue(success, killed);
        let result_level = if success { "INFO" } else { "ERROR" };
        let pid = result.d2r_pid;
        let err = result.error.clone();
        crate::logger::log_msg(
            result_level,
            "Launch",
            &format!(
                "[Account {}] 启动结果: success={}, pid={:?}, error={:?}, mutex_killed={}",
                account_id, success, pid, err, killed
            ),
        );
        results.push(result);
        #[cfg(target_os = "windows")]
        if success {
            memory_trim.trim_halfway(&state, cancellation_ticket).await;
        }

        // 必须确认 WEB_TOKEN 变化并清除互斥句柄，失败或超时停止队列。
        if !can_continue && i + 1 < total {
            let (queue_message, remaining_error) = if success {
                ("互斥句柄未清除，后续账号暂停启动", "互斥句柄未清除，已暂停")
            } else {
                (
                    "当前账号启动失败，后续账号暂停启动",
                    "前序账号启动失败，已暂停",
                )
            };
            let _ = app.emit(
                "launch-progress",
                LaunchProgress::new(account_id, "queue", "warning", queue_message),
            );
            for remaining in &account_ids[i + 1..] {
                emit_cancelled(&app, remaining);
                results.push(LaunchResult {
                    account_id: remaining.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some(remaining_error.to_string()),
                    mutex_killed: false,
                });
            }
            break;
        }

        // 如果还有下一个账号，等 2 秒让系统稳定
        if i + 1 < total && !is_cancelled(&state, cancellation_ticket) {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    #[cfg(target_os = "windows")]
    memory_trim.trim(&state, cancellation_ticket).await;

    let restore_config = state
        .configuration()
        .snapshot()
        .unwrap_or_else(|| config.clone());
    if selected_window_layout(&restore_config).is_some()
        && !is_cancelled(&state, cancellation_ticket)
    {
        let restore_state = state.inner().clone();
        if let Ok(Err(error)) = tokio::task::spawn_blocking(move || {
            crate::commands::window_layout::restore_layout(&restore_config, &restore_state)
        })
        .await
        {
            crate::logger::log_msg("WARN", "Layout", &error.to_string());
        }
    }

    // This runs inside the same task and host lease, after every game's cleanup,
    // the final memory trim, and window arrangement. Only reopen the client.
    let retained_account_id = battle_net_account_to_keep(
        config.keep_battle_net_account_id.as_deref(),
        &results,
        is_cancelled(&state, cancellation_ticket),
    )
    .map(str::to_string);
    if let Some(account_id) = retained_account_id {
        let retained = reopen_battle_net_after_launch(
            &app,
            &config,
            &state,
            cancellation_ticket,
            &account_id,
            &mut host_runtime_lease,
        )
        .await;
        let (status, message) = if retained.success {
            ("ok", "多开完成，所选账号的战网已保留".to_string())
        } else {
            let warning = format!(
                "游戏启动已完成，但保留战网失败: {}",
                retained.error.as_deref().unwrap_or("未知错误"),
            );
            // Keep the original game's success, PID, and mutex result. The
            // frontend presents success + error as a nonblocking warning.
            if let Some(result) = results
                .iter_mut()
                .find(|result| result.account_id == account_id)
            {
                result.error = Some(match result.error.take() {
                    Some(previous) => format!("{previous}；{warning}"),
                    None => warning.clone(),
                });
            }
            ("warning", warning)
        };
        crate::logger::log_msg(
            if retained.success { "INFO" } else { "WARN" },
            "Launch",
            &format!("[Account {account_id}] [keep-battle-net] [{status}]: {message}"),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(&account_id, "done", status, &message),
        );
    }

    Ok(results)
}

fn emit_cancelled(app: &tauri::AppHandle, account_id: &str) {
    crate::logger::log_msg(
        "INFO",
        "Launch",
        &format!("[Account {}] 启动已被用户取消", account_id),
    );
    let _ = app.emit(
        "launch-progress",
        LaunchProgress::new(account_id, "done", "error", "已取消"),
    );
}

/// Runs one account launch. `memory_trim` stays a separate `&mut` borrow that
/// outlives this call, so the arguments are passed flat.
#[allow(clippy::too_many_arguments)]
async fn launch_single(
    app: &tauri::AppHandle,
    config: &GlobalConfig,
    state: &SharedState,
    cancellation_ticket: CancellationTicket,
    account_id: &str,
    meta: AccountMeta,
    options: LaunchExecutionOptions,
    #[cfg(target_os = "windows")]
    memory_trim: &mut crate::infrastructure::memory_trim::BatchMemoryTrim,
) -> LaunchResult {
    let LaunchExecutionOptions {
        persist_position_changes,
        preserved_default_mod_args,
        graphics_override,
        layout_rect,
    } = options;
    let emit = |step: &str, status: &str, msg: &str| {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [{}] [{}]: {}", account_id, step, status, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, step, status, msg),
        );
    };

    let preflight_context =
        match LaunchContext::for_account(config, &meta, ContextPurpose::LaunchGame) {
            Ok(context) => context,
            Err(error) => return account_path_error(account_id, error),
        };

    if preflight_context.auth_mode == AuthMode::Token {
        return launch_single_token(
            app,
            config,
            state,
            TokenLaunchRequest {
                cancellation_ticket,
                account_id,
                meta: &meta,
                context: &preflight_context,
                graphics_override: graphics_override.as_ref(),
                layout_rect,
            },
            #[cfg(target_os = "windows")]
            memory_trim,
        )
        .await;
    }

    let context = match prepare_bnet_environment(
        app,
        config,
        state,
        cancellation_ticket,
        account_id,
        false,
        BnetPreparationOptions {
            meta_override: Some(&meta),
            preserved_default_mod_args: preserved_default_mod_args.as_deref(),
            require_settings_copy: graphics_override.is_some() || layout_rect.is_some(),
        },
    )
    .await
    {
        Ok(context) => context,
        Err(result) => return result,
    };
    let _settings_override = match apply_temporary_graphics_override(
        &context,
        graphics_override.as_ref(),
        layout_rect,
    ) {
        Ok(settings_override) => settings_override,
        Err(error) => return account_path_error(account_id, error),
    };
    let product_code = context.edition.battle_net_launch_product;
    let battle_net_path = match context.battle_net_executable() {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(error) => return account_path_error(account_id, error),
    };
    let expected_game_path = context.installation.game_executable.clone();

    let mut token_change_monitor = None;

    // ── Step 4: 记录当前 D2R 进程快照 ──
    let before_pids = crate::infrastructure::system::snapshot_processes("D2R.exe".to_string());

    // ── Step 5 & 6: 发送游戏启动指令并等待新 D2R 进程 ──
    emit("game", "running", "正在启动游戏进程...");

    let mut locked_agent_pid: Option<u32> = None;
    let mut first_agent_killed = false;
    let mut agent_locked_at: Option<std::time::Instant> = None;
    let mut last_launch_sent: Option<std::time::Instant> = None;
    let mut last_launch_error: Option<String> = None;
    let mut d2r_pid_opt: Option<u32> = None;
    let mut actual_game_path = expected_game_path.clone();
    let mut sys = sysinfo::System::new(); // 优化：复用 System 实例以提高效率
                                          // 跟踪已尝试 kill 的 Agent PID，避免重复日志洪水
    let mut killed_agent_pids: std::collections::HashSet<u32> = std::collections::HashSet::new();

    let wait_start = std::time::Instant::now();
    let timeout_secs = 60;

    while wait_start.elapsed().as_secs() < timeout_secs {
        if is_cancelled(state, cancellation_ticket) {
            emit("done", "error", "已取消，正在保存状态...");
            return cancel_with_cleanup(
                config,
                &context,
                account_id,
                preserved_default_mod_args.as_deref(),
            )
            .await;
        }

        struct SysStatus {
            agent_pids: Vec<u32>,
            bnet_count: usize,
            d2r_games: Vec<(u32, Option<PathBuf>)>,
        }

        let monitored_bnet_path = battle_net_path.clone();
        let (status, sys_ret) = tokio::task::spawn_blocking(move || {
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All);

            let mut agent_pids = Vec::new();
            let mut bnet_count = 0;
            let mut d2r_games = Vec::new();

            for (pid, proc) in sys.processes() {
                let name = proc.name().to_string_lossy();
                if name.eq_ignore_ascii_case("Agent.exe") {
                    agent_pids.push(pid.as_u32());
                } else if name.eq_ignore_ascii_case("Battle.net.exe") {
                    if proc.exe().is_some_and(|actual| {
                        crate::infrastructure::system::executable_paths_match(
                            actual,
                            Path::new(&monitored_bnet_path),
                        )
                    }) {
                        bnet_count += 1;
                    }
                } else if name.eq_ignore_ascii_case("D2R.exe") {
                    d2r_games.push((pid.as_u32(), proc.exe().map(Path::to_path_buf)));
                }
            }

            (
                SysStatus {
                    agent_pids,
                    bnet_count,
                    d2r_games,
                },
                sys,
            )
        })
        .await
        .unwrap_or((
            SysStatus {
                agent_pids: Vec::new(),
                bnet_count: 0,
                d2r_games: Vec::new(),
            },
            sysinfo::System::new(),
        ));
        sys = sys_ret;

        // 1. 以新增游戏进程为准；目录不一致仅警告，不阻断启动。
        if let Some((pid, executable)) =
            newly_started_battle_net_game(&before_pids, &status.d2r_games, &expected_game_path)
        {
            d2r_pid_opt = Some(pid);
            actual_game_path = executable;
            if let Some(warning) =
                battle_net_game_directory_warning(&actual_game_path, &expected_game_path)
            {
                emit("game", "warning", &warning);
            }
            break;
        }

        // 2. 检测 Agent.exe 进程并锁定 (模式3跳过)
        if config.agent_mode != 3 && !first_agent_killed {
            if let Some(pid) = locked_agent_pid {
                if !status.agent_pids.contains(&pid) {
                    locked_agent_pid = None;
                    agent_locked_at = None;
                }
            }
            if locked_agent_pid.is_none() {
                if let Some(&first_pid) = status.agent_pids.first() {
                    locked_agent_pid = Some(first_pid);
                    agent_locked_at = Some(std::time::Instant::now());
                    emit(
                        "game",
                        "running",
                        &format!("已锁定战网 Agent 进程 (PID: {})", first_pid),
                    );
                }
            }
        }

        // 3. Agent 杀一次逻辑：根据多开模式决定何时杀 (模式3跳过)
        if config.agent_mode != 3 {
            if !first_agent_killed {
                if let Some(agent_pid) = locked_agent_pid {
                    let should_kill = match config.agent_mode {
                        2 => status.bnet_count >= config.agent_threshold as usize,
                        _ => {
                            // 模式1 (默认): 从 Agent 被锁定起等待 agent_delay_secs 秒
                            agent_locked_at
                                .map(|t| t.elapsed().as_secs_f64() >= config.agent_delay_secs)
                                .unwrap_or(false)
                        }
                    };
                    if should_kill {
                        let agent_pid_copy = agent_pid;
                        let _ = tokio::task::spawn_blocking(move || {
                            let _ = silent_cmd("taskkill")
                                .args(["/F", "/PID", &agent_pid_copy.to_string()])
                                .output();
                        })
                        .await;
                        first_agent_killed = true;
                        locked_agent_pid = None; // Reset
                        if config.agent_mode == 1 {
                            emit(
                                "game",
                                "running",
                                &format!(
                                    "Agent 存活 {}s 后已终止 (PID: {})",
                                    config.agent_delay_secs, agent_pid
                                ),
                            );
                        } else {
                            emit(
                                "game",
                                "running",
                                &format!(
                                    "战网进程数达到 {} (≥{})，已终止 Agent (PID: {})",
                                    status.bnet_count, config.agent_threshold, agent_pid
                                ),
                            );
                        }
                    }
                }
            } else {
                // 后续追着杀：查到就秒杀（每次都执行 taskkill，仅首次 emit 日志）
                for &pid in &status.agent_pids {
                    let first_seen = killed_agent_pids.insert(pid);
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = silent_cmd("taskkill")
                            .args(["/F", "/PID", &pid.to_string()])
                            .output();
                    })
                    .await;
                    if first_seen {
                        emit(
                            "game",
                            "running",
                            &format!("检测到新生 Agent 进程，已立即秒杀 (PID: {})", pid),
                        );
                    }
                }
            }
        }

        // 4. 判断 Battle.net.exe 数量是否大于 5，只有大于 5 才发送游戏启动指令
        if status.bnet_count > 5 {
            let should_send = match last_launch_sent {
                None => true,
                Some(last) => last.elapsed() >= std::time::Duration::from_secs(5),
            };

            if should_send {
                if token_change_monitor.is_none() {
                    token_change_monitor = Some(
                        match WebTokenChangeMonitor::capture(
                            &context.token_registry_path(),
                            account_id,
                            2,
                        ) {
                            Ok(monitor) => monitor,
                            Err(error) => return account_path_error(account_id, error),
                        },
                    );
                    emit(
                        "connect",
                        "running",
                        "已在战网启动游戏前建立 WEB_TOKEN 基线",
                    );
                }
                let battle_net_path = battle_net_path.clone();
                emit(
                    "game",
                    "running",
                    &format!(
                        "战网进程数达到 {} (>5)，正在发送游戏启动指令 ({})...",
                        status.bnet_count, product_code
                    ),
                );
                let launch_result = tokio::task::spawn_blocking(move || {
                    spawn_battle_net_launch_command(&battle_net_path, product_code)
                })
                .await;
                match launch_result {
                    Ok(Ok(_child)) => {
                        last_launch_error = None;
                        emit(
                            "game",
                            "running",
                            &format!("已向战网提交游戏启动指令 ({})", product_code),
                        );
                    }
                    Ok(Err(error)) => {
                        let message = format!("发送游戏启动指令失败: {error}");
                        last_launch_error = Some(message.clone());
                        emit("game", "warning", &message);
                    }
                    Err(error) => {
                        let message = format!("发送游戏启动指令线程异常: {error}");
                        last_launch_error = Some(message.clone());
                        emit("game", "warning", &message);
                    }
                }
                last_launch_sent = Some(std::time::Instant::now());
            }
        } else {
            if last_launch_sent.is_none() {
                emit(
                    "game",
                    "running",
                    &format!(
                        "等待战网客户端加载... (当前战网进程数: {})",
                        status.bnet_count
                    ),
                );
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let d2r_pid = match d2r_pid_opt {
        Some(pid) => {
            emit("game", "ok", &format!("游戏进程已启动 (PID: {})", pid));
            track_launched_process(
                app,
                state,
                config,
                account_id,
                pid,
                &meta.mod_args,
                &actual_game_path,
            );
            crate::audio_mod::emit_runtime_compatibility_warning(
                app,
                state,
                config,
                &meta,
                pid,
                &meta.mod_args,
            );
            // 将游戏窗口标题改为账号昵称，并调整窗口位置（如已配置）。
            // 方案覆盖不允许反向污染账号默认位置，因此关闭位置轮询写回。
            {
                let win_x = meta.window_x;
                let win_y = meta.window_y;
                // 延迟重试 + 位置持续轮询
                let pid_copy = pid;
                let accounts_dir = config.accounts_dir.clone();
                let account_id_owned = account_id.to_string();
                let state_for_position = state.clone();
                let separate_taskbar_icon = config.separate_game_taskbar_icons;
                let app_user_model_id = format!("D2RHub.Account.{account_id}");
                let app_for_window = app.clone();
                let account_id_for_window = account_id.to_string();
                let layout_executable = actual_game_path.clone();
                let geometry_request = WindowGeometryRequest::capture(config, account_id);
                tokio::task::spawn_blocking(move || {
                    let Some(process) = crate::infrastructure::system::LaunchProcessGuard::capture(
                        pid_copy,
                        &layout_executable,
                    ) else {
                        return;
                    };
                    // Phase 1: 10 次重试重命名 + 初始定位
                    let mut taskbar_configured = !separate_taskbar_icon;
                    let mut taskbar_error = None;
                    for _ in 0..10 {
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        if !process.is_running() {
                            return;
                        }
                        let windows = SystemGameWindowPort;
                        let _ = refresh_launch_window_title(
                            state_for_position.multi_instance().instances(),
                            &accounts_dir,
                            &account_id_owned,
                            pid_copy,
                            |title| windows.rename(pid_copy, title),
                        );
                        if !taskbar_configured {
                            match windows.set_taskbar_identity(pid_copy, &app_user_model_id) {
                                Ok(()) => taskbar_configured = true,
                                Err(error) => taskbar_error = Some(error),
                            }
                        }
                        if (layout_rect.is_none() && (win_x.is_none() || win_y.is_none()))
                            || state_for_position
                                .multi_instance()
                                .instances()
                                .initial_window_geometry_superseded(&account_id_owned, pid_copy)
                            || !geometry_request.is_current(&state_for_position)
                        {
                            continue;
                        }
                        let calibrated_rect = layout_rect.map(|rect| {
                            geometry_request.calibrated_target(&state_for_position, rect)
                        });
                        state_for_position
                            .multi_instance()
                            .instances()
                            .apply_initial_window_geometry(&account_id_owned, pid_copy, || {
                                if let Some(rect) = calibrated_rect {
                                    crate::infrastructure::game_layout_windows::apply_for_pid(
                                        pid_copy,
                                        &layout_executable,
                                        rect,
                                    );
                                } else if let (Some(x), Some(y)) = (win_x, win_y) {
                                    windows.move_to(pid_copy, WindowPosition { x, y });
                                }
                            });
                    }
                    if !taskbar_configured {
                        let message = taskbar_error
                            .unwrap_or_else(|| "游戏窗口任务栏独立分组设置失败".to_string());
                        crate::logger::log_msg(
                            "WARN",
                            "Launch",
                            &format!("[Account {account_id_for_window}] {message}"),
                        );
                        let _ = app_for_window.emit(
                            "launch-progress",
                            LaunchProgress::new(
                                &account_id_for_window,
                                "window",
                                "warning",
                                &message,
                            ),
                        );
                    }
                    if !persist_position_changes {
                        return;
                    }
                    // Phase 2: 默认启动时轮询窗口位置，拖动停止后反向写入账号配置
                    let mut last_pos: Option<(i32, i32)> = None;
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        if !process.is_running()
                            || state_for_position
                                .multi_instance()
                                .instances()
                                .pid_for(&account_id_owned)
                                != Some(pid_copy)
                            || state_for_position
                                .multi_instance()
                                .instances()
                                .initial_window_geometry_superseded(&account_id_owned, pid_copy)
                        {
                            break;
                        }
                        let windows = SystemGameWindowPort;
                        if let Some(position) = windows.position(pid_copy) {
                            let pos = (position.x, position.y);
                            // 过滤最小化时的异常坐标（Windows 对最小化窗口返回 ~-32000）
                            if pos.0 > -10000 && pos.1 > -10000 && last_pos != Some(pos) {
                                // 只有真正落盘后才更新 last_pos；租约忙时保留旧值，
                                // 下一轮会在启动事务释放账号租约后自动重试。
                                if persist_window_position(
                                    &state_for_position,
                                    &accounts_dir,
                                    &account_id_owned,
                                    pos,
                                ) {
                                    last_pos = Some(pos);
                                }
                            }
                        }
                    }
                });
            }
            pid
        }
        None => {
            let error = last_launch_error.unwrap_or_else(|| "等待游戏进程启动超时".to_string());
            emit("game", "error", &error);
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(error),
                mutex_killed: false,
            };
        }
    };

    if is_cancelled(state, cancellation_ticket) {
        emit("done", "error", "已取消，正在保存状态...");
        return cancel_with_cleanup(
            config,
            &context,
            account_id,
            preserved_default_mod_args.as_deref(),
        )
        .await;
    }

    let game_process =
        crate::infrastructure::system::LaunchProcessGuard::capture(d2r_pid, &actual_game_path);

    #[cfg(target_os = "windows")]
    let pending_memory_trim = crate::infrastructure::memory_trim::PendingMemoryTrim::prepare(
        config,
        &actual_game_path,
        account_id,
        d2r_pid,
    );

    // 延迟 2 秒发送登录按键，达到该模式的变化次数后停止。
    let d2r_started_at = std::time::Instant::now();

    // ── 杀 Mutex ──
    let mutex_state = std::sync::Arc::new(MutexRemovalState::default());
    let mutex_task = spawn_mutex_removal(
        state,
        cancellation_ticket,
        d2r_pid,
        &actual_game_path,
        mutex_state.clone(),
    );

    emit(
        "connect",
        "running",
        "等待 WEB_TOKEN 两次实际变化（首次刷新不放行）...",
    );
    emit("mutex", "running", "后台清除互斥句柄中（必须成功）...");
    let Some(monitor) = token_change_monitor.as_ref() else {
        mutex_task.abort();
        return account_path_error(
            account_id,
            AppError::RegistryError("未建立启动前 WEB_TOKEN 基线，已停止队列".into()),
        );
    };
    let start = std::time::Instant::now();
    let key_start = d2r_started_at + std::time::Duration::from_secs(2);
    let key_deadline = key_start + LOGIN_KEY_SEND_WINDOW;
    let mut next_key_send = key_start;
    let mut change_logged = false;
    let mut key_attempts = 0u32;
    let mut mutex_logged = false;
    let failure = loop {
        if is_cancelled(state, cancellation_ticket) {
            mutex_task.abort();
            return cancel_with_cleanup(
                config,
                &context,
                account_id,
                preserved_default_mod_args.as_deref(),
            )
            .await;
        }
        if !game_process
            .as_ref()
            .is_some_and(|process| process.is_running())
        {
            if game_process
                .as_ref()
                .is_some_and(|process| process.has_exited())
            {
                state
                    .multi_instance()
                    .instances()
                    .remove_if_pid(account_id, d2r_pid);
            }
            break Some("原游戏进程已退出或无法确认存活，已停止启动队列".to_string());
        }
        let changed = match monitor.changed() {
            Ok(changed) => changed,
            Err(error) => break Some(error),
        };
        if changed && !change_logged {
            emit("connect", "ok", "WEB_TOKEN 已完成两次变化，停止登录按键");
            change_logged = true;
        }
        if mutex_state.is_closed() && !mutex_logged {
            emit("mutex", "ok", "互斥句柄已清除");
            mutex_logged = true;
        }
        if launch_login_is_ready(changed, mutex_state.is_closed()) {
            break None;
        }
        if start.elapsed() >= std::time::Duration::from_secs(60) {
            break Some(format!(
                "等待登录与互斥清除超时：WEB_TOKEN 已变化 {}/2 次；互斥句柄 {}；已停止队列",
                monitor.change_count().unwrap_or_default(),
                mutex_state.diagnostics()
            ));
        }
        let now = std::time::Instant::now();
        if !changed && now >= next_key_send && now < key_deadline {
            let sent = crate::infrastructure::system::send_keys_to_window(d2r_pid);
            key_attempts += 1;
            if key_attempts == 1 || key_attempts.is_multiple_of(10) {
                emit("connect", "running", &format!("持续发送登录按键：第 {key_attempts} 次，结果 {sent:?}，WEB_TOKEN 已变化 {}/2 次", monitor.change_count().unwrap_or_default()));
            }
            next_key_send = now + std::time::Duration::from_millis(500);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    mutex_task.abort();
    if let Some(error) = failure {
        emit("done", "error", &error);
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: Some(d2r_pid),
            error: Some(error),
            mutex_killed: mutex_state.is_closed(),
        };
    }

    // ── Step 9: 优雅关闭战网 → 等待退出 → 回写最新状态 ──
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let graceful = crate::infrastructure::system::graceful_kill_bnet(30);
    if !graceful {
        emit("cleanup", "warning", "战网未能优雅关闭，已回退强制关闭");
    }
    emit("cleanup", "running", "正在回写最新认证状态...");

    let account_dir = match checked_account_dir(config, account_id) {
        Ok(dir) => dir,
        Err(res) => return res,
    };
    let config_clone = config.clone();
    let account_dir_clone = account_dir.clone();
    let preserved_default_mod_args = preserved_default_mod_args.clone();
    let sync_res =
        tokio::task::spawn_blocking(move || match preserved_default_mod_args.as_deref() {
            Some(default_mod_args) => {
                crate::commands::account::sync_back_to_account_preserving_mod(
                    &account_dir_clone,
                    &config_clone,
                    default_mod_args,
                )
            }
            None => {
                crate::commands::account::sync_back_to_account(&account_dir_clone, &config_clone)
            }
        })
        .await;

    let sync_error = match sync_res {
        Ok(Ok(())) => {
            emit("cleanup", "ok", "战网已关闭，状态已同步");
            None
        }
        Ok(Err(e)) => Some(format!("回写认证状态失败，已停止队列: {e}")),
        Err(_) => Some("回写认证状态线程异常，已停止队列".to_string()),
    };
    if let Some(error) = sync_error {
        emit("cleanup", "error", &error);
        emit("done", "error", &error);
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: Some(d2r_pid),
            error: Some(error),
            mutex_killed: mutex_state.is_closed(),
        };
    }

    // 更新最后启动时间
    let accounts_dir_clone = config.accounts_dir.clone();
    let account_id_clone = account_id.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(mut meta) = AccountManager::load_meta(&accounts_dir_clone, &account_id_clone) {
            meta.last_launched_at = Some(chrono::Utc::now().to_rfc3339());
            let _ = AccountManager::save_meta(&accounts_dir_clone, &meta);
        }
    })
    .await;

    mutex_task.abort();

    if !game_process
        .as_ref()
        .is_some_and(|process| process.is_running())
    {
        if game_process
            .as_ref()
            .is_some_and(|process| process.has_exited())
        {
            state
                .multi_instance()
                .instances()
                .remove_if_pid(account_id, d2r_pid);
        }
        let error = "原游戏进程已退出或无法确认存活，已停止启动队列";
        emit("done", "error", error);
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some(error.to_string()),
            mutex_killed: false,
        };
    }

    if is_cancelled(state, cancellation_ticket) {
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: Some(d2r_pid),
            error: Some("已取消".into()),
            mutex_killed: mutex_state.is_closed(),
        };
    }
    #[cfg(target_os = "windows")]
    memory_trim.confirm(pending_memory_trim);
    emit("done", "ok", "启动完成");
    LaunchResult {
        account_id: account_id.to_string(),
        success: true,
        d2r_pid: Some(d2r_pid),
        error: None,
        mutex_killed: mutex_state.is_closed(),
    }
}

async fn launch_single_token(
    app: &tauri::AppHandle,
    config: &GlobalConfig,
    state: &SharedState,
    request: TokenLaunchRequest<'_>,
    #[cfg(target_os = "windows")]
    memory_trim: &mut crate::infrastructure::memory_trim::BatchMemoryTrim,
) -> LaunchResult {
    let TokenLaunchRequest {
        cancellation_ticket,
        account_id,
        meta,
        context,
        graphics_override,
        layout_rect,
    } = request;
    let emit = |step: &str, status: &str, msg: &str| {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [{}] [{}]: {}", account_id, step, status, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, step, status, msg),
        );
    };

    let cancelled = || -> LaunchResult {
        LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some("启动已被用户取消".to_string()),
            mutex_killed: false,
        }
    };

    emit("clean", "running", "正在清理环境变量...");
    if is_cancelled(state, cancellation_ticket) {
        emit("done", "error", "已取消");
        return cancelled();
    }

    emit("copy", "running", "正在准备注册表与 Settings.json...");
    let account_dir = match checked_account_dir(config, account_id) {
        Ok(dir) => dir,
        Err(res) => return res,
    };

    // 1. 覆盖 Settings.json
    if meta.has_customized_settings {
        match context.installation.saved_games_directory.as_deref() {
            Some(saved_games_directory) => {
                if let Err(error) =
                    copy_account_settings_to_system(&account_dir, saved_games_directory)
                {
                    if graphics_override.is_some() || layout_rect.is_some() {
                        return account_path_error(account_id, error);
                    }
                    emit(
                        "copy",
                        "warning",
                        &format!("独立 Settings.json 覆盖失败，已继续使用系统配置: {error}"),
                    );
                }
            }
            None => emit(
                "copy",
                "warning",
                "未配置可用的存档目录，已跳过独立 Settings.json 覆盖",
            ),
        }
    } else {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!(
                "[Account {}] Token 启动使用系统 Settings.json，跳过账号画质配置覆盖",
                account_id
            ),
        );
    }

    let _settings_override =
        match apply_temporary_graphics_override(context, graphics_override, layout_rect) {
            Ok(settings_override) => settings_override,
            Err(error) => return account_path_error(account_id, error),
        };

    // 2. 写入 Token 到注册表
    let protected_bytes = match &meta.token {
        Some(t) => {
            // Token 在 account.json 中以 hex(DPAPI加密结果) 形式存储，
            // 直接解码后写入注册表即可，D2R 会自行调用 CryptUnprotectData 解密
            match crate::commands::crypto::hex_decode(t) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return LaunchResult {
                        account_id: account_id.to_string(),
                        success: false,
                        d2r_pid: None,
                        error: Some(format!("Token 解码失败: {}", e)),
                        mutex_killed: false,
                    };
                }
            }
        }
        None => {
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("账号缺少 Token".to_string()),
                mutex_killed: false,
            };
        }
    };

    let token_registry_path = context.token_registry_path();
    let registry_result = (|| -> Result<(), AppError> {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(&token_registry_path).map_err(|error| {
            AppError::RegistryError(format!("打开 Token 注册表路径失败: {error}"))
        })?;
        key.set_value("REGION", &context.region.registry_region)
            .map_err(|error| AppError::RegistryError(format!("写入 REGION 失败: {error}")))?;
        let locale = meta
            .language
            .as_deref()
            .unwrap_or(context.region.default_locale);
        let audio = meta
            .voicelanguage
            .as_deref()
            .unwrap_or(context.region.default_locale);
        key.set_value("LOCALE", &locale)
            .map_err(|error| AppError::RegistryError(format!("写入 LOCALE 失败: {error}")))?;
        key.set_value("LOCALE_AUDIO", &audio)
            .map_err(|error| AppError::RegistryError(format!("写入 LOCALE_AUDIO 失败: {error}")))?;

        let val = winreg::RegValue {
            bytes: protected_bytes.clone(),
            vtype: RegType::REG_BINARY,
        };
        key.set_raw_value(WEB_TOKEN_VALUE_NAME, &val)
            .map_err(|error| {
                AppError::RegistryError(format!("写入 {WEB_TOKEN_VALUE_NAME} 失败: {error}"))
            })?;
        Ok(())
    })();
    if let Err(error) = registry_result {
        return account_path_error(account_id, error);
    }
    emit("copy", "ok", "配置覆盖完成");
    let token_change_monitor =
        WebTokenChangeMonitor::start(token_registry_path, protected_bytes, account_id);
    emit("connect", "running", "已以本次注入值建立 WEB_TOKEN 基线");

    // 3. 记录之前存在的 D2R 进程
    let before_pids = crate::infrastructure::system::snapshot_processes("D2R.exe".to_string());

    emit("game", "running", "正在直接启动 D2R.exe...");
    // 4. 启动 D2R.exe
    let game_path = context.installation.game_executable.clone();
    let expected_game_path = game_path.clone();
    // The executable UID is edition-specific; the shared token registry key is not.
    let uid_arg = context.edition.token_auth_app;

    let mut cmd = Command::new(&game_path);
    cmd.current_dir(&context.installation.game_directory);
    cmd.arg("-uid").arg(uid_arg);

    if !meta.mod_args.is_empty() {
        match parse_windows_command_line(&meta.mod_args) {
            Ok(args) => {
                cmd.args(args);
            }
            Err(error) => {
                return account_path_error(account_id, AppError::ConfigReadError(error));
            }
        }
    }

    let spawn_res = tokio::task::spawn_blocking(move || cmd.spawn()).await;
    match spawn_res {
        Ok(Ok(_)) => {}
        _ => {
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("启动 D2R.exe 失败".to_string()),
                mutex_killed: false,
            };
        }
    }

    // 5. 等待进程
    let mut d2r_pid_opt: Option<u32> = None;
    let mut sys = sysinfo::System::new();
    let wait_start = std::time::Instant::now();
    let timeout_secs = 60;

    while wait_start.elapsed().as_secs() < timeout_secs {
        if is_cancelled(state, cancellation_ticket) {
            return cancelled();
        }

        let monitored_game_path = expected_game_path.clone();
        let process_refresh = tokio::task::spawn_blocking(move || {
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
            let mut pids = Vec::new();
            for (pid, proc) in sys.processes() {
                if proc
                    .name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("D2R.exe")
                    && proc.exe().is_some_and(|actual| {
                        crate::infrastructure::system::executable_paths_match(
                            actual,
                            &monitored_game_path,
                        )
                    })
                {
                    pids.push(pid.as_u32());
                }
            }
            (pids, sys)
        })
        .await;
        let (d2r_pids, sys_ret) = match process_refresh {
            Ok(result) => result,
            Err(error) => {
                return LaunchResult {
                    account_id: account_id.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some(format!("刷新游戏进程列表失败: {error}")),
                    mutex_killed: false,
                };
            }
        };
        sys = sys_ret;

        for pid in &d2r_pids {
            if !before_pids.contains(pid) {
                d2r_pid_opt = Some(*pid);
                break;
            }
        }
        if d2r_pid_opt.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let d2r_pid = match d2r_pid_opt {
        Some(pid) => {
            emit("game", "ok", &format!("游戏进程已启动 (PID: {})", pid));
            track_launched_process(
                app,
                state,
                config,
                account_id,
                pid,
                &meta.mod_args,
                &context.installation.game_executable,
            );
            crate::audio_mod::emit_runtime_compatibility_warning(
                app,
                state,
                config,
                meta,
                pid,
                &meta.mod_args,
            );

            let accounts_dir = config.accounts_dir.clone();
            let win_x = meta.window_x;
            let win_y = meta.window_y;
            let pid_copy = pid;
            let separate_taskbar_icon = config.separate_game_taskbar_icons;
            let app_user_model_id = format!("D2RHub.Account.{account_id}");
            let app_for_window = app.clone();
            let account_id_for_window = account_id.to_string();
            let layout_executable = context.installation.game_executable.clone();
            let geometry_request = WindowGeometryRequest::capture(config, account_id);
            let state_for_geometry = state.clone();
            tokio::task::spawn_blocking(move || {
                let Some(process) = crate::infrastructure::system::LaunchProcessGuard::capture(
                    pid_copy,
                    &layout_executable,
                ) else {
                    return;
                };
                let mut taskbar_configured = !separate_taskbar_icon;
                let mut taskbar_error = None;
                for _ in 0..10 {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    if !process.is_running() {
                        return;
                    }
                    let windows = SystemGameWindowPort;
                    let _ = refresh_launch_window_title(
                        state_for_geometry.multi_instance().instances(),
                        &accounts_dir,
                        &account_id_for_window,
                        pid_copy,
                        |title| windows.rename(pid_copy, title),
                    );
                    if !taskbar_configured {
                        match windows.set_taskbar_identity(pid_copy, &app_user_model_id) {
                            Ok(()) => taskbar_configured = true,
                            Err(error) => taskbar_error = Some(error),
                        }
                    }
                    if (layout_rect.is_none() && (win_x.is_none() || win_y.is_none()))
                        || state_for_geometry
                            .multi_instance()
                            .instances()
                            .initial_window_geometry_superseded(&account_id_for_window, pid_copy)
                        || !geometry_request.is_current(&state_for_geometry)
                    {
                        continue;
                    }
                    let calibrated_rect = layout_rect
                        .map(|rect| geometry_request.calibrated_target(&state_for_geometry, rect));
                    state_for_geometry
                        .multi_instance()
                        .instances()
                        .apply_initial_window_geometry(&account_id_for_window, pid_copy, || {
                            if let Some(rect) = calibrated_rect {
                                crate::infrastructure::game_layout_windows::apply_for_pid(
                                    pid_copy,
                                    &layout_executable,
                                    rect,
                                );
                            } else if let (Some(x), Some(y)) = (win_x, win_y) {
                                windows.move_to(pid_copy, WindowPosition { x, y });
                            }
                        });
                }
                if !taskbar_configured {
                    let message = taskbar_error
                        .unwrap_or_else(|| "游戏窗口任务栏独立分组设置失败".to_string());
                    crate::logger::log_msg(
                        "WARN",
                        "Launch",
                        &format!("[Account {account_id_for_window}] {message}"),
                    );
                    let _ = app_for_window.emit(
                        "launch-progress",
                        LaunchProgress::new(&account_id_for_window, "window", "warning", &message),
                    );
                }
            });
            pid
        }
        None => {
            emit("game", "error", "等待游戏进程启动超时");
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("等待游戏进程启动超时".to_string()),
                mutex_killed: false,
            };
        }
    };

    // 延迟 2 秒发送登录按键，达到该模式的变化次数后停止。
    let d2r_started_at = std::time::Instant::now();

    let game_process = crate::infrastructure::system::LaunchProcessGuard::capture(
        d2r_pid,
        &context.installation.game_executable,
    );

    #[cfg(target_os = "windows")]
    let pending_memory_trim = crate::infrastructure::memory_trim::PendingMemoryTrim::prepare(
        config,
        &context.installation.game_executable,
        account_id,
        d2r_pid,
    );

    // ── 杀 Mutex ──
    let mutex_state = std::sync::Arc::new(MutexRemovalState::default());
    let mutex_task = spawn_mutex_removal(
        state,
        cancellation_ticket,
        d2r_pid,
        &context.installation.game_executable,
        mutex_state.clone(),
    );

    emit(
        "connect",
        "running",
        "等待 WEB_TOKEN 相对启动基线发生变化...",
    );
    emit("mutex", "running", "后台清除互斥句柄中（必须成功）...");
    let monitor = &token_change_monitor;
    let start = std::time::Instant::now();
    let key_start = d2r_started_at + std::time::Duration::from_secs(2);
    let key_deadline = key_start + LOGIN_KEY_SEND_WINDOW;
    let mut next_key_send = key_start;
    let mut change_logged = false;
    let mut mutex_logged = false;
    let failure = loop {
        if is_cancelled(state, cancellation_ticket) {
            mutex_task.abort();
            return cancelled();
        }
        if !game_process
            .as_ref()
            .is_some_and(|process| process.is_running())
        {
            if game_process
                .as_ref()
                .is_some_and(|process| process.has_exited())
            {
                state
                    .multi_instance()
                    .instances()
                    .remove_if_pid(account_id, d2r_pid);
            }
            break Some("原游戏进程已退出或无法确认存活，已停止启动队列".to_string());
        }
        let changed = match monitor.changed() {
            Ok(changed) => changed,
            Err(error) => break Some(error),
        };
        if changed && !change_logged {
            emit("connect", "ok", "WEB_TOKEN 已变化，停止跳过动画按键");
            change_logged = true;
        }
        if mutex_state.is_closed() && !mutex_logged {
            emit("mutex", "ok", "互斥句柄已清除");
            mutex_logged = true;
        }
        if launch_login_is_ready(changed, mutex_state.is_closed()) {
            break None;
        }
        if start.elapsed() >= std::time::Duration::from_secs(60) {
            break Some(format!(
                "等待登录与互斥清除超时：WEB_TOKEN {}；互斥句柄 {}；已停止队列",
                if changed { "已变化" } else { "未变化" },
                mutex_state.diagnostics()
            ));
        }
        let now = std::time::Instant::now();
        if !changed && now >= next_key_send && now < key_deadline {
            let _ = crate::infrastructure::system::send_keys_to_window(d2r_pid);
            next_key_send = now + std::time::Duration::from_millis(500);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    mutex_task.abort();
    if let Some(error) = failure {
        emit("done", "error", &error);
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: Some(d2r_pid),
            error: Some(error),
            mutex_killed: mutex_state.is_closed(),
        };
    }

    // 更新最后启动时间
    let accounts_dir_clone = config.accounts_dir.clone();
    let account_id_clone = account_id.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(mut meta) = AccountManager::load_meta(&accounts_dir_clone, &account_id_clone) {
            meta.last_launched_at = Some(chrono::Utc::now().to_rfc3339());
            let _ = AccountManager::save_meta(&accounts_dir_clone, &meta);
        }
    })
    .await;

    if !game_process
        .as_ref()
        .is_some_and(|process| process.is_running())
    {
        if game_process
            .as_ref()
            .is_some_and(|process| process.has_exited())
        {
            state
                .multi_instance()
                .instances()
                .remove_if_pid(account_id, d2r_pid);
        }
        let error = "原游戏进程已退出或无法确认存活，已停止启动队列";
        emit("done", "error", error);
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some(error.to_string()),
            mutex_killed: false,
        };
    }

    if is_cancelled(state, cancellation_ticket) {
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: Some(d2r_pid),
            error: Some("已取消".into()),
            mutex_killed: mutex_state.is_closed(),
        };
    }
    #[cfg(target_os = "windows")]
    memory_trim.confirm(pending_memory_trim);
    emit("done", "ok", "启动完成");
    LaunchResult {
        account_id: account_id.to_string(),
        success: true,
        d2r_pid: Some(d2r_pid),
        error: None,
        mutex_killed: mutex_state.is_closed(),
    }
}

// ── 工具函数 ──

// Compatibility path for existing command consumers.
pub(crate) use crate::domain::mod_arguments::parse_windows_command_line;

/// 解码 .reg 注册表文件内容为 String。
/// Windows regedit 导出默认 UTF-16LE（BOM 0xFF 0xFE），也兼容 UTF-8（含或不含 BOM）。
/// 返回 None 表示文件编码无法识别或解码失败——调用方应拒绝导入（Fail-Safe）。
fn decode_reg_file(raw: &[u8]) -> Option<String> {
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        // UTF-16LE with BOM
        let u16_bytes = &raw[2..];
        if !u16_bytes.len().is_multiple_of(2) {
            return None;
        }
        let u16_words: Vec<u16> = u16_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| u16::from_le_bytes(*bytes))
            .collect();
        String::from_utf16(&u16_words).ok()
    } else if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        // UTF-8 with BOM
        String::from_utf8(raw[3..].to_vec()).ok()
    } else {
        // Assume UTF-8 without BOM (or plain ASCII)
        String::from_utf8(raw.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_temporary_graphics_override_at_path, battle_net_account_to_keep,
        battle_net_launch_argument, launch_login_is_ready, parse_windows_command_line,
        persist_window_position, preflight_accounts, replace_bnet_roaming_snapshot,
        unique_account_window_executable, validate_legacy_reg_sections, LaunchGraphicsOverride,
    };
    use crate::commands::account::{AccountManager, AccountMeta};
    use crate::domain::config::GlobalConfig;
    use crate::launch_context::ContextPurpose;
    use crate::state::{AccountLifecycleLease, AppState};

    #[test]
    fn battle_net_game_uses_new_pid_and_actual_path_even_when_directory_differs() {
        let configured = std::path::PathBuf::from(r"C:\configured\D2R.exe");
        let actual = std::path::PathBuf::from(r"D:\battle-net-game\D2R.exe");
        let games = vec![(10, Some(configured.clone())), (20, Some(actual.clone()))];
        assert_eq!(
            super::newly_started_battle_net_game(&[10], &games, &configured),
            Some((20, actual.clone()))
        );
        let warning = super::battle_net_game_directory_warning(&actual, &configured).unwrap();
        assert!(warning.contains("本次启动将继续"));
        assert!(warning.contains("当前战网运行的游戏目录与软件配置目录不一致"));
        assert!(super::battle_net_game_directory_warning(&actual, &actual).is_none());
        assert!(super::newly_started_battle_net_game(&[10, 20], &games, &configured).is_none());
    }

    fn successful_game_launch(id: &str) -> super::LaunchResult {
        super::LaunchResult {
            account_id: id.into(),
            success: true,
            d2r_pid: Some(123),
            error: None,
            mutex_killed: true,
        }
    }

    #[test]
    fn retained_battle_net_uses_the_selected_account_even_when_it_was_not_last() {
        let results = vec![
            successful_game_launch("acount1"),
            successful_game_launch("acount2"),
        ];
        assert_eq!(
            battle_net_account_to_keep(Some("  ACOUNT1  "), &results, false),
            Some("acount1")
        );
        assert_eq!(battle_net_account_to_keep(None, &results, false), None);
        assert_eq!(battle_net_account_to_keep(Some(""), &results, false), None);
        assert_eq!(
            battle_net_account_to_keep(Some("acount3"), &results, false),
            None
        );
        assert_eq!(
            battle_net_account_to_keep(Some("acount1"), &[], false),
            None
        );
    }

    #[test]
    fn retained_battle_net_is_skipped_for_cancelled_failed_or_unsafe_batches() {
        let mut results = vec![
            successful_game_launch("acount1"),
            successful_game_launch("acount2"),
        ];
        assert_eq!(
            battle_net_account_to_keep(Some("acount1"), &results, true),
            None
        );
        results[1].success = false;
        assert_eq!(
            battle_net_account_to_keep(Some("acount1"), &results, false),
            None
        );
        results[1].success = true;
        results[1].mutex_killed = false;
        assert_eq!(
            battle_net_account_to_keep(Some("acount1"), &results, false),
            None
        );
        results[1].mutex_killed = true;
        results[0].d2r_pid = None;
        assert_eq!(
            battle_net_account_to_keep(Some("acount1"), &results, false),
            None
        );
    }

    #[test]
    fn retained_battle_net_rechecks_auth_mode_before_restoring_the_environment() {
        let root = temp_dir("retained_bnet_token");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let error = preflight_accounts(&config, &["acount1".into()], ContextPurpose::BattleNetOnly)
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("Token 认证账号不支持仅启动 Battle.net"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dropping_launch_scope_aborts_background_mutex_worker() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(async {
            let task = super::LaunchBackgroundTask(tokio::spawn(std::future::pending::<()>()));
            let abort = task.0.abort_handle();
            drop(task);
            tokio::task::yield_now().await;
            assert!(abort.is_finished());
        });
    }

    #[test]
    fn token_change_and_closed_mutex_are_both_required_for_launch() {
        assert!(!launch_login_is_ready(false, false));
        assert!(!launch_login_is_ready(false, true));
        assert!(!launch_login_is_ready(true, false));
        assert!(launch_login_is_ready(true, true));
    }

    #[test]
    fn temporary_scheme_graphics_restore_the_exact_system_settings() {
        let root = std::env::temp_dir().join(format!(
            "d2rhub_scheme_settings_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("Settings.json");
        let original = br#"{
  "Screen Resolution (Windowed)": "1280x720",
  "Framerate Cap": 60,
  "Framerate Target": 30,
  "VSync": true
}"#;
        std::fs::write(&path, original).unwrap();

        {
            let _guard = apply_temporary_graphics_override_at_path(
                path.clone(),
                &LaunchGraphicsOverride {
                    game_settings: None,
                    resolution: "2560x1440".to_string(),
                    fps: 144,
                },
            )
            .unwrap();
            let temporary: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(temporary["Screen Resolution (Windowed)"], "2560x1440");
            assert_eq!(temporary["Framerate Cap"], 144);
            assert_eq!(temporary["Framerate Target"], 144);
            assert_eq!(temporary["VSync"], true);
        }

        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn complete_scheme_settings_replace_mutable_defaults_and_restore_exact_bytes() {
        let root = temp_dir("complete_scheme_settings");
        let path = root.join("Settings.json");
        let original = br#"{"Texture Quality":0,"Master Volume":10}"#;
        std::fs::write(&path, original).unwrap();
        let graphics = LaunchGraphicsOverride {
            game_settings: Some(
                serde_json::from_value(
                    serde_json::json!({ "Texture Quality": 3, "Master Volume": 80 }),
                )
                .unwrap(),
            ),
            resolution: "1920x1080".into(),
            fps: 60,
        };
        {
            let _guard =
                apply_temporary_graphics_override_at_path(path.clone(), &graphics).unwrap();
            let effective: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(effective["Texture Quality"], 3);
            assert_eq!(effective["Master Volume"], 80);
            assert_eq!(effective["Framerate Cap"], 60);
        }
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn layout_game_resolution_is_temporary_and_preserves_fps_intent() {
        let root = temp_dir("layout_window_settings");
        let path = root.join("Settings.json");
        let original = br#"{"Window Mode":1,"Screen Resolution (Windowed)":"2560x1440","Framerate Cap":75,"VSync":true}"#;
        let graphics = LaunchGraphicsOverride {
            game_settings: None,
            resolution: "3840x2160".into(),
            fps: 144,
        };
        let rect = crate::domain::window_layout::LayoutRect {
            x: -1600,
            y: 200,
            width: 1280,
            height: 720,
        };
        for override_graphics in [None, Some(&graphics)] {
            std::fs::write(&path, original).unwrap();
            {
                let _guard = super::apply_temporary_window_settings_at_path(
                    path.clone(),
                    override_graphics,
                    Some(rect),
                )
                .unwrap();
                let temporary: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                assert_eq!(temporary["Window Mode"], 0);
                assert_eq!(temporary["Screen Resolution (Windowed)"], "1280x720");
                assert_eq!(
                    temporary["Framerate Cap"],
                    if override_graphics.is_some() { 144 } else { 75 }
                );
                assert_eq!(temporary["VSync"], true);
            }
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_geometry_yields_to_new_account_positions_and_layout_enablement() {
        let mut account = AccountMeta::new("acount1");
        account.window_x = Some(120);
        account.window_y = Some(240);
        account.active_position_id = Some("original".into());
        let config = GlobalConfig::default();
        let request = super::WindowGeometryRequest {
            monitors: Vec::new(),
            accounts_dir: String::new(),
            account_id: account.id.clone(),
            layout: None,
            position: Some((
                account.window_x,
                account.window_y,
                account.active_position_id.clone(),
            )),
        };
        assert!(request.matches(&config, &account));
        account.active_position_id = Some("captured".into());
        assert!(!request.matches(&config, &account));
        account.active_position_id = Some("original".into());
        let mut changed = config.clone();
        changed.window_layout_enabled = true;
        changed.active_window_layout_id = Some("layout".into());
        changed
            .window_layouts
            .push(crate::domain::window_layout::WindowLayout {
                id: "layout".into(),
                name: "Layout".into(),
                monitors: vec![],
                windows: vec![],
            });
        assert!(!request.matches(&changed, &account));
        let layout_request = super::WindowGeometryRequest {
            layout: changed.window_layouts.first().cloned(),
            ..request
        };
        assert!(layout_request.matches(&changed, &account));
        changed.window_layouts[0].name = "Updated".into();
        assert!(!layout_request.matches(&changed, &account));
    }

    #[test]
    fn battle_net_launch_argument_quotes_only_the_exec_value() {
        assert_eq!(battle_net_launch_argument("OSI"), r#"--exec="launch OSI""#);
    }

    #[test]
    fn startup_title_retry_waits_for_rename_and_reads_committed_name() {
        let root = temp_dir("startup_title_rename");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let registry = super::InstanceRegistry::default();
        registry.record_launched("acount1", 42, "");
        let title = std::sync::Mutex::new(String::new());

        std::thread::scope(|scope| {
            let (started, waiting) = std::sync::mpsc::channel();
            registry
                .with_account_identity_change("acount1", |_| {
                    let registry = &registry;
                    let accounts_dir = &config.accounts_dir;
                    let title = &title;
                    scope.spawn(move || {
                        started.send(()).unwrap();
                        super::refresh_launch_window_title(
                            registry,
                            accounts_dir,
                            "acount1",
                            42,
                            |current| *title.lock().unwrap() = current.into(),
                        )
                        .unwrap();
                    });
                    waiting.recv().unwrap();
                    let mut account =
                        AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
                    account.display_name = "Renamed while startup retries".into();
                    *title.lock().unwrap() = account.display_name.clone();
                    AccountManager::save_meta(&config.accounts_dir, &account)
                })
                .unwrap();
        });
        assert_eq!(*title.lock().unwrap(), "Renamed while startup retries");

        registry.record_launched("acount1", 43, "");
        super::refresh_launch_window_title(&registry, &config.accounts_dir, "acount1", 42, |_| {
            panic!("an old startup worker must not rename a replacement instance")
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn window_position_retries_after_account_lease_is_released() {
        let root = temp_dir("window_position_retry");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let state = std::sync::Arc::new(AppState::new());
        let blocking_lease = AccountLifecycleLease::try_acquire(&state, "acount1").unwrap();

        assert!(!persist_window_position(
            &state,
            &config.accounts_dir,
            "acount1",
            (120, 240),
        ));
        drop(blocking_lease);
        assert!(persist_window_position(
            &state,
            &config.accounts_dir,
            "acount1",
            (120, 240),
        ));

        let meta = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        assert_eq!((meta.window_x, meta.window_y), (Some(120), Some(240)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_window_identities_are_not_assigned_to_an_account() {
        let root = temp_dir("duplicate_window_identity");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let first = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        assert!(unique_account_window_executable(&config, &first).is_some());

        save_account(&config, "acount2", "token", Some("00"));
        for id in ["acount1", "acount2"] {
            let mut meta = AccountManager::load_meta(&config.accounts_dir, id).unwrap();
            meta.display_name = "同名账号".to_string();
            AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();
        }
        let first = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        assert!(unique_account_window_executable(&config, &first).is_none());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn windows_mod_arguments_preserve_quoted_spaces() {
        assert_eq!(
            parse_windows_command_line(r#"-mod "My Mod" -txt"#).unwrap(),
            vec!["-mod", "My Mod", "-txt"]
        );
    }

    #[test]
    fn windows_mod_arguments_preserve_escaped_quotes_and_backslashes() {
        assert_eq!(
            parse_windows_command_line(r#"--label "say \"hello\"" C:\Mods\D2RMM"#).unwrap(),
            vec!["--label", "say \"hello\"", r"C:\Mods\D2RMM"]
        );
    }

    #[test]
    fn windows_mod_arguments_reject_unclosed_quotes() {
        assert!(parse_windows_command_line(r#"-mod "unfinished"#).is_err());
    }

    #[test]
    fn legacy_reg_accepts_only_the_unified_auth_section() {
        let content = concat!(
            "Windows Registry Editor Version 5.00\n\n",
            "[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Battle.net\\UnifiedAuth]\n",
            "\"US\"=hex:01,02\n",
            "[hkey_current_user\\software\\blizzard entertainment\\battle.NET\\unifiedauth]\n",
            "\"EU\"=hex:03,04\n"
        );
        assert!(validate_legacy_reg_sections(content).is_ok());
    }

    #[test]
    fn legacy_reg_rejects_any_additional_or_nested_section() {
        let additional = concat!(
            "[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Battle.net\\UnifiedAuth]\n",
            "[HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run]\n"
        );
        let nested =
            "[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Battle.net\\UnifiedAuth\\Child]\n";
        assert!(validate_legacy_reg_sections(additional).is_err());
        assert!(validate_legacy_reg_sections(nested).is_err());
        assert!(validate_legacy_reg_sections("Windows Registry Editor Version 5.00").is_err());
    }

    #[test]
    fn replacing_bnet_snapshot_removes_stale_target_files() {
        let root = temp_dir("replace_bnet");
        let source = root.join("source");
        let target = root.join("target");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(source.join("new.json"), b"new").unwrap();
        std::fs::write(target.join("stale.json"), b"stale").unwrap();

        replace_bnet_roaming_snapshot(&source, &target).unwrap();

        assert!(target.join("new.json").is_file());
        assert!(!target.join("stale.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replacing_bnet_snapshot_repairs_a_non_directory_target() {
        let root = temp_dir("replace_bnet_repairs_target");
        let source = root.join("source");
        let target = root.join("target");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("new.json"), b"new").unwrap();
        std::fs::write(&target, b"not a directory").unwrap();

        replace_bnet_roaming_snapshot(&source, &target).unwrap();

        assert!(target.is_dir());
        assert_eq!(std::fs::read(target.join("new.json")).unwrap(), b"new");
        assert!(!root.join("target.tmp").exists());
        assert!(!root.join("target.bak").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replacing_bnet_snapshot_recovers_an_interrupted_host_swap() {
        let root = temp_dir("replace_bnet_recovers_host");
        let source = root.join("source");
        let target = root.join("target");
        let staged = root.join("target.tmp");
        let backup = root.join("target.bak");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(source.join("new.json"), b"new").unwrap();
        std::fs::write(staged.join("partial.json"), b"partial").unwrap();
        std::fs::write(backup.join("old.json"), b"old").unwrap();

        replace_bnet_roaming_snapshot(&source, &target).unwrap();

        assert_eq!(std::fs::read(target.join("new.json")).unwrap(), b"new");
        assert!(!target.join("old.json").exists());
        assert!(!staged.exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "d2rhub_launch_preflight_{name}_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn configure_global_install(root: &std::path::Path, with_bnet: bool) -> GlobalConfig {
        let game = root.join("game");
        let saves = root.join("saves");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(game.join("D2R.exe"), b"test").unwrap();

        let cn_battle_net_path = if with_bnet {
            let bnet = root.join("Battle.net.exe");
            std::fs::write(&bnet, b"test").unwrap();
            bnet.to_string_lossy().to_string()
        } else {
            String::new()
        };
        GlobalConfig {
            accounts_dir: root.join("accounts").to_string_lossy().to_string(),
            cn_battle_net_path,
            cn_game_path: if with_bnet {
                game.to_string_lossy().to_string()
            } else {
                String::new()
            },
            cn_saved_games_path: if with_bnet {
                saves.to_string_lossy().to_string()
            } else {
                String::new()
            },
            global_game_path: if with_bnet {
                String::new()
            } else {
                game.to_string_lossy().to_string()
            },
            global_saved_games_path: if with_bnet {
                String::new()
            } else {
                saves.to_string_lossy().to_string()
            },
            ..GlobalConfig::default()
        }
    }

    fn save_account(config: &GlobalConfig, id: &str, auth_mode: &str, token: Option<&str>) {
        let mut meta = AccountMeta::new(id);
        meta.region = Some(if auth_mode == "bnet" { "CN" } else { "NA" }.to_string());
        meta.auth_mode = Some(auth_mode.to_string());
        meta.token = token.map(str::to_string);
        meta.initialized = true;
        meta.last_reset_at = Some(chrono::Utc::now().to_rfc3339());
        let account_dir = AccountManager::account_dir_checked(&config.accounts_dir, id).unwrap();
        std::fs::create_dir_all(account_dir).unwrap();
        AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();
    }

    #[test]
    fn batch_preflight_rejects_a_later_account_before_launch_side_effects() {
        let root = temp_dir("token_batch");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        save_account(&config, "acount2", "token", None);

        let result = preflight_accounts(
            &config,
            &["acount1".to_string(), "acount2".to_string()],
            ContextPurpose::LaunchGame,
        );
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn batch_preflight_allows_missing_customized_settings() {
        let root = temp_dir("missing_custom_settings");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let mut meta = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        meta.has_customized_settings = true;
        AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();

        let result = preflight_accounts(
            &config,
            &["acount1".to_string()],
            ContextPurpose::LaunchGame,
        );

        assert!(result.is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn batch_preflight_rejects_invalid_token_mod_arguments() {
        let root = temp_dir("invalid_mod_args");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let mut meta = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        meta.mod_args = r#"-mod "unterminated"#.to_string();
        AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();

        let error = preflight_accounts(
            &config,
            &["acount1".to_string()],
            ContextPurpose::LaunchGame,
        )
        .unwrap_err();

        assert!(error.to_string().contains("Mod 启动参数无效"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bnet_preflight_rejects_an_empty_auth_snapshot() {
        let root = temp_dir("bnet_snapshot");
        let config = configure_global_install(&root, true);
        save_account(&config, "acount3", "bnet", None);
        let account_dir =
            AccountManager::account_dir_checked(&config.accounts_dir, "acount3").unwrap();
        let bnet_dir = account_dir.join("Battle.net");
        std::fs::create_dir_all(&bnet_dir).unwrap();
        std::fs::write(bnet_dir.join("Battle.net.config"), b"{}").unwrap();
        std::fs::write(account_dir.join("unified_auth.json"), b"[]").unwrap();
        std::fs::write(
            account_dir.join("unified_auth.reg"),
            "[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Battle.net\\UnifiedAuth]\n\"US\"=hex:01",
        )
        .unwrap();

        assert!(preflight_accounts(
            &config,
            &["acount3".to_string()],
            ContextPurpose::LaunchGame,
        )
        .is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
