use crate::commands::global_config::{
    RoomRotationConfig, RoomRotationFlowStrategy, RoomRotationPoint,
};
use crate::state::SharedState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const STATUS_EVENT: &str = "room-rotation-status";
const MAX_GAME_NAME_LENGTH: usize = 15;

fn client_coordinate(length: i32, permille: u16) -> i32 {
    let last_pixel = length.saturating_sub(1).max(0);
    (length.saturating_mul(i32::from(permille.min(1_000))) / 1_000).min(last_pixel)
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomRotationStatus {
    pub running: bool,
    pub phase: String,
    pub message: String,
    pub room_name: Option<String>,
    pub attempt: u8,
    pub primary_account_id: Option<String>,
    pub follower_account_ids: Vec<String>,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRotationInputTestRequest {
    account_id: String,
    action: String,
    sample: Option<String>,
    click_variant: Option<String>,
    text_variant: Option<String>,
    flow_strategy: Option<String>,
    point_override: Option<RoomRotationPoint>,
}

impl Default for RoomRotationStatus {
    fn default() -> Self {
        Self {
            running: false,
            phase: "idle".to_string(),
            message: "等待快捷键".to_string(),
            room_name: None,
            attempt: 0,
            primary_account_id: None,
            follower_account_ids: Vec::new(),
            started_at: None,
            last_error: None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct AppliedPassword {
    pid: u32,
    create: bool,
    value: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SelectedFormTab {
    pid: u32,
    create: bool,
}

#[derive(Default)]
struct Runtime {
    generation: u64,
    cancelled: bool,
    pending_room_name: Option<String>,
    pending_sequence: Option<u32>,
    applied_passwords: HashMap<String, AppliedPassword>,
    selected_form_tabs: HashMap<String, SelectedFormTab>,
    status: RoomRotationStatus,
}

fn runtime() -> &'static Mutex<Runtime> {
    static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();
    RUNTIME.get_or_init(|| Mutex::new(Runtime::default()))
}

fn emit_status(app: &tauri::AppHandle, status: &RoomRotationStatus) {
    if let Err(error) = app.emit(STATUS_EVENT, status) {
        crate::logger::log_msg(
            "WARN",
            "RoomRotation",
            &format!("推送换房状态失败: {error}"),
        );
    }
}

fn update_status(
    app: &tauri::AppHandle,
    generation: u64,
    update: impl FnOnce(&mut RoomRotationStatus),
) {
    let status = {
        let mut current = runtime().lock().unwrap_or_else(|error| error.into_inner());
        if current.generation != generation {
            return;
        }
        update(&mut current.status);
        current.status.clone()
    };
    crate::logger::log_msg(
        "INFO",
        "RoomRotation",
        &format!("{}: {}", status.phase, status.message),
    );
    emit_status(app, &status);
}

fn ensure_active(generation: u64) -> Result<(), String> {
    let current = runtime().lock().unwrap_or_else(|error| error.into_inner());
    if current.generation != generation || current.cancelled {
        Err("换房流程已取消".to_string())
    } else {
        Ok(())
    }
}

fn sleep_interruptible(generation: u64, duration: Duration) -> Result<(), String> {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        ensure_active(generation)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }
    Ok(())
}

fn room_name(config: &RoomRotationConfig, sequence: u32) -> Result<String, String> {
    let value = format!(
        "{}{:0width$}",
        config.name_prefix,
        sequence,
        width = usize::from(config.sequence_width)
    );
    if value.len() > MAX_GAME_NAME_LENGTH {
        return Err(format!(
            "房间名“{value}”超过 D2R 的 {MAX_GAME_NAME_LENGTH} 字符限制"
        ));
    }
    Ok(value)
}

fn password_needs_update(account_id: &str, pid: u32, create: bool, password: &str) -> bool {
    let current = runtime().lock().unwrap_or_else(|error| error.into_inner());
    current
        .applied_passwords
        .get(account_id)
        .is_none_or(|applied| {
            applied.pid != pid || applied.create != create || applied.value != password
        })
}

fn remember_applied_password(account_id: &str, pid: u32, create: bool, password: &str) {
    let mut current = runtime().lock().unwrap_or_else(|error| error.into_inner());
    current.applied_passwords.insert(
        account_id.to_string(),
        AppliedPassword {
            pid,
            create,
            value: password.to_string(),
        },
    );
}

fn form_tab_needs_select(account_id: &str, pid: u32, create: bool) -> bool {
    let current = runtime().lock().unwrap_or_else(|error| error.into_inner());
    current
        .selected_form_tabs
        .get(account_id)
        .is_none_or(|selected| selected.pid != pid || selected.create != create)
}

fn remember_selected_form_tab(account_id: &str, pid: u32, create: bool) {
    let mut current = runtime().lock().unwrap_or_else(|error| error.into_inner());
    current
        .selected_form_tabs
        .insert(account_id.to_string(), SelectedFormTab { pid, create });
}

fn resolve_pid(state: &SharedState, account_id: &str) -> Result<u32, String> {
    state
        .active_games
        .read()
        .get(account_id)
        .copied()
        .ok_or_else(|| format!("账号“{account_id}”没有已识别的运行中 D2R 进程"))
}

fn validate_base_config(config: &RoomRotationConfig) -> Result<(), String> {
    if !config.enabled {
        return Err("自动换房尚未启用".to_string());
    }
    if config.primary_account_id.trim().is_empty() {
        return Err("尚未选择主操作账号".to_string());
    }
    if config.follower_account_ids.is_empty() {
        return Err("尚未选择小号".to_string());
    }
    Ok(())
}

fn validate_primary_runtime(
    app: &tauri::AppHandle,
    config: &RoomRotationConfig,
) -> Result<u32, String> {
    validate_base_config(config)?;
    let state = app.state::<SharedState>();
    let primary_pid = resolve_pid(&state, &config.primary_account_id)?;
    #[cfg(target_os = "windows")]
    if win::foreground_pid() != Some(primary_pid) {
        return Err("请先切到主号 D2R 窗口，再按主号建房快捷键".to_string());
    }
    Ok(primary_pid)
}

fn validate_follower_runtime(
    app: &tauri::AppHandle,
    config: &RoomRotationConfig,
) -> Result<Vec<(String, u32)>, String> {
    validate_base_config(config)?;
    let state = app.state::<SharedState>();
    let primary_pid = resolve_pid(&state, &config.primary_account_id)?;
    #[cfg(target_os = "windows")]
    if win::foreground_pid() != Some(primary_pid) {
        return Err("请在主号已经进入新房间后按小号跟进快捷键".to_string());
    }
    let mut followers = Vec::with_capacity(config.follower_account_ids.len());
    for account_id in &config.follower_account_ids {
        followers.push((account_id.clone(), resolve_pid(&state, account_id)?));
    }
    Ok(followers)
}

fn commit_next_sequence(app: &tauri::AppHandle, sequence: u32) -> Result<(), String> {
    let state = app.state::<SharedState>();
    let saved = {
        let _config_io = state.config_io.lock();
        let mut stored = state.config.write();
        let config = stored
            .as_mut()
            .ok_or_else(|| "尚未加载全局配置".to_string())?;
        config.room_rotation.next_sequence = config
            .room_rotation
            .next_sequence
            .max(sequence.saturating_add(1));
        config
            .save(&state.app_data_dir)
            .map_err(|error| error.to_string())?;
        config.clone()
    };
    let _ = app.emit("global-config-updated", saved);
    Ok(())
}

enum WorkflowCompletion {
    PrimaryReady { room_name: String, sequence: u32 },
    FollowersComplete,
}

fn finish(app: &tauri::AppHandle, generation: u64, result: Result<WorkflowCompletion, String>) {
    let status = {
        let mut current = runtime().lock().unwrap_or_else(|error| error.into_inner());
        if current.generation != generation {
            return;
        }
        current.status.running = false;
        match result {
            Ok(WorkflowCompletion::PrimaryReady {
                room_name,
                sequence,
            }) => {
                current.pending_room_name = Some(room_name.clone());
                current.pending_sequence = Some(sequence);
                current.status.phase = "ready_for_followers".to_string();
                current.status.message =
                    "主号建房指令已发送；确认主号进房后按小号跟进快捷键".to_string();
                current.status.room_name = Some(room_name);
                current.status.last_error = None;
            }
            Ok(WorkflowCompletion::FollowersComplete) => {
                current.pending_room_name = None;
                current.pending_sequence = None;
                current.status.phase = "complete".to_string();
                current.status.message = "所有小号的加入指令已并行发送".to_string();
                current.status.last_error = None;
            }
            Err(error) => {
                let cancelled = current.cancelled;
                current.status.phase = if cancelled { "cancelled" } else { "error" }.to_string();
                current.status.message = error.clone();
                current.status.last_error = Some(error);
            }
        }
        current.cancelled = false;
        current.status.clone()
    };
    crate::logger::log_msg(
        if status.last_error.is_some() {
            "ERROR"
        } else {
            "INFO"
        },
        "RoomRotation",
        &status.message,
    );
    emit_status(app, &status);
}

#[cfg(target_os = "windows")]
fn exit_game(
    driver: &win::WindowDriver,
    hwnd: isize,
    flow: &RoomRotationFlowStrategy,
    generation: u64,
) -> Result<(), String> {
    driver.key(hwnd, win::VK_ESCAPE)?;
    sleep_interruptible(generation, Duration::from_millis(flow.escape_to_exit_ms))?;
    driver.click(hwnd, flow.ui_profile.save_and_exit)?;
    sleep_interruptible(generation, Duration::from_millis(flow.exit_load_ms))?;
    Ok(())
}

#[cfg(target_os = "windows")]
struct RoomFormInput<'a> {
    account_id: &'a str,
    pid: u32,
    create: bool,
    name: &'a str,
    password: &'a str,
    text_strategy: &'a str,
}

#[cfg(target_os = "windows")]
fn fill_credentials(
    app: &tauri::AppHandle,
    driver: &win::WindowDriver,
    hwnd: isize,
    flow: &RoomRotationFlowStrategy,
    input: RoomFormInput<'_>,
    generation: u64,
) -> Result<(), String> {
    // Keep each form transaction together so parallel followers cannot replace
    // one another's clipboard or cursor target. Cursor-guard/background modes
    // stay unfocused; only the explicit focus fallback activates the window.
    let update_password =
        password_needs_update(input.account_id, input.pid, input.create, input.password);
    let select_tab = form_tab_needs_select(input.account_id, input.pid, input.create);
    let form = driver.begin_form_input(hwnd)?;
    let (tab, game_name_field, password_field) = if input.create {
        (
            flow.ui_profile.create_tab,
            flow.ui_profile.create_game_name_field,
            flow.ui_profile.create_password_field,
        )
    } else {
        (
            flow.ui_profile.join_tab,
            flow.ui_profile.join_game_name_field,
            flow.ui_profile.join_password_field,
        )
    };
    if select_tab {
        form.click(tab)?;
        remember_selected_form_tab(input.account_id, input.pid, input.create);
        sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    }
    form.click(game_name_field)?;
    sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    form.paste_text(
        app,
        input.name,
        flow.character_delay_ms,
        input.text_strategy,
    )?;
    sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    if update_password {
        form.click(password_field)?;
        sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
        form.paste_text(
            app,
            input.password,
            flow.character_delay_ms,
            input.text_strategy,
        )?;
        sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    }
    // The join form can display the posted password text before D2R copies it
    // into the value used by validation. A final guarded click on that field
    // reproduces the user-confirmed commit gesture without changing its text.
    if !input.create {
        form.click(password_field)?;
        sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    }
    form.key(win::VK_RETURN, input.text_strategy)?;
    if update_password {
        remember_applied_password(input.account_id, input.pid, input.create, input.password);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_primary_workflow(
    app: tauri::AppHandle,
    generation: u64,
    config: RoomRotationConfig,
    primary_pid: u32,
    retry_sequence: Option<u32>,
) -> Result<WorkflowCompletion, String> {
    let driver = win::WindowDriver::new(
        &config.input_mode,
        &config.background_click_strategy,
        config.cursor_lease_ms,
    );
    let primary_hwnd = crate::commands::system::find_game_hwnd(primary_pid)
        .ok_or_else(|| "无法找到主号 D2R 窗口".to_string())?;
    let flow = config.flow_for_account(&config.primary_account_id);

    if retry_sequence.is_some() {
        update_status(&app, generation, |status| {
            status.phase = "retrying_primary".to_string();
            status.message = "正在确认重名弹窗并使用下一个序号重试".to_string();
        });
        driver.click(primary_hwnd, flow.ui_profile.dialog_confirm)?;
        sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    } else {
        update_status(&app, generation, |status| {
            status.phase = "primary_exiting".to_string();
            status.message = "主号正在按绑定策略退出并进入大厅".to_string();
        });
        exit_game(&driver, primary_hwnd, flow, generation)?;
    }

    let sequence = retry_sequence.unwrap_or(config.next_sequence);
    let candidate = room_name(&config, sequence)?;
    update_status(&app, generation, |status| {
        status.phase = "creating_primary".to_string();
        status.message = format!("正在填写并创建 {candidate}");
        status.room_name = Some(candidate.clone());
        status.attempt = 1;
    });
    fill_credentials(
        &app,
        &driver,
        primary_hwnd,
        flow,
        RoomFormInput {
            account_id: &config.primary_account_id,
            pid: primary_pid,
            create: true,
            name: &candidate,
            password: &config.password,
            text_strategy: &config.background_text_strategy,
        },
        generation,
    )?;
    Ok(WorkflowCompletion::PrimaryReady {
        room_name: candidate,
        sequence,
    })
}

#[cfg(target_os = "windows")]
fn run_one_follower(
    app: &tauri::AppHandle,
    generation: u64,
    config: RoomRotationConfig,
    account_id: String,
    pid: u32,
    room_name: String,
) -> Result<(), String> {
    // Focus mode uses process-global keyboard and mouse state. Keep the whole
    // per-window workflow together so parallel workers cannot type into each
    // other's window. CursorGuard/background modes still run concurrently.
    static FOCUS_WORKFLOW_LOCK: Mutex<()> = Mutex::new(());
    let _focus_guard = (config.input_mode == "focus").then(|| {
        FOCUS_WORKFLOW_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    });
    let driver = win::WindowDriver::new(
        &config.input_mode,
        &config.background_click_strategy,
        config.cursor_lease_ms,
    );
    let hwnd = crate::commands::system::find_game_hwnd(pid)
        .ok_or_else(|| format!("无法找到小号“{account_id}”的 D2R 窗口"))?;
    let flow = config.flow_for_account(&account_id);
    exit_game(&driver, hwnd, flow, generation)?;
    fill_credentials(
        app,
        &driver,
        hwnd,
        flow,
        RoomFormInput {
            account_id: &account_id,
            pid,
            create: false,
            name: &room_name,
            password: &config.password,
            text_strategy: &config.background_text_strategy,
        },
        generation,
    )
}

#[cfg(target_os = "windows")]
fn run_follower_workflow(
    app: tauri::AppHandle,
    generation: u64,
    config: RoomRotationConfig,
    followers: Vec<(String, u32)>,
    room_name: String,
    sequence: u32,
) -> Result<WorkflowCompletion, String> {
    if let Err(error) = commit_next_sequence(&app, sequence) {
        crate::logger::log_msg(
            "WARN",
            "RoomRotation",
            &format!("主号已由用户确认进房，但保存下一个序号失败: {error}"),
        );
    }
    let total = followers.len();
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut handles = Vec::with_capacity(total);
    for (account_id, pid) in followers {
        let worker_app = app.clone();
        let worker_config = config.clone();
        let worker_room = room_name.clone();
        let worker_sender = sender.clone();
        let worker_account = account_id.clone();
        handles.push(std::thread::spawn(move || {
            let result = run_one_follower(
                &worker_app,
                generation,
                worker_config,
                worker_account.clone(),
                pid,
                worker_room,
            );
            let _ = worker_sender.send((worker_account, result));
        }));
    }
    drop(sender);

    let mut failures = Vec::new();
    for completed in 1..=total {
        let (account_id, result) = receiver
            .recv()
            .map_err(|_| "等待小号工作线程返回时通道已关闭".to_string())?;
        if let Err(error) = result {
            failures.push(format!("{account_id}: {error}"));
        }
        update_status(&app, generation, |status| {
            status.message = format!("小号并行处理中：已完成 {completed}/{total}");
        });
    }
    for handle in handles {
        let _ = handle.join();
    }
    if config.input_mode == "focus" {
        if let Some(primary_hwnd) = crate::commands::system::find_game_hwnd(resolve_pid(
            &app.state::<SharedState>(),
            &config.primary_account_id,
        )?) {
            crate::commands::system::bring_window_to_foreground_raw(primary_hwnd);
        }
    }
    if failures.is_empty() {
        Ok(WorkflowCompletion::FollowersComplete)
    } else {
        Err(format!("部分小号执行失败：{}", failures.join("；")))
    }
}

#[cfg(not(target_os = "windows"))]
fn run_primary_workflow(
    _app: tauri::AppHandle,
    _generation: u64,
    _config: RoomRotationConfig,
    _primary_pid: u32,
    _retry_sequence: Option<u32>,
) -> Result<WorkflowCompletion, String> {
    Err("自动换房测试版仅支持 Windows".to_string())
}

#[cfg(not(target_os = "windows"))]
fn run_follower_workflow(
    _app: tauri::AppHandle,
    _generation: u64,
    _config: RoomRotationConfig,
    _followers: Vec<(String, u32)>,
    _room_name: String,
    _sequence: u32,
) -> Result<WorkflowCompletion, String> {
    Err("自动换房测试版仅支持 Windows".to_string())
}

fn load_rotation_config(app: &tauri::AppHandle) -> Result<RoomRotationConfig, String> {
    app.state::<SharedState>()
        .config
        .read()
        .as_ref()
        .map(|config| config.room_rotation.clone())
        .ok_or_else(|| "尚未加载全局配置".to_string())
}

pub fn start_primary(app: tauri::AppHandle) -> Result<RoomRotationStatus, String> {
    let config = load_rotation_config(&app)?;
    let primary_pid = validate_primary_runtime(&app, &config)?;
    let (generation, status, retry_sequence) = {
        let mut current = runtime().lock().unwrap_or_else(|error| error.into_inner());
        if current.status.running {
            return Err("已有一轮自动换房正在运行".to_string());
        }
        let retry_sequence = current
            .pending_sequence
            .map(|sequence| sequence.saturating_add(1));
        current.generation = current.generation.saturating_add(1);
        current.cancelled = false;
        if retry_sequence.is_none() {
            current.pending_room_name = None;
            current.pending_sequence = None;
        }
        current.status = RoomRotationStatus {
            running: true,
            phase: if retry_sequence.is_some() {
                "retrying_primary"
            } else {
                "starting_primary"
            }
            .to_string(),
            message: if retry_sequence.is_some() {
                "正在使用下一个房间序号重试建房"
            } else {
                "正在启动主号建房流程"
            }
            .to_string(),
            room_name: None,
            attempt: 0,
            primary_account_id: Some(config.primary_account_id.clone()),
            follower_account_ids: config.follower_account_ids.clone(),
            started_at: Some(chrono::Local::now().to_rfc3339()),
            last_error: None,
        };
        (current.generation, current.status.clone(), retry_sequence)
    };
    emit_status(&app, &status);
    let worker_app = app.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("room-rotation-primary".to_string())
        .spawn(move || {
            let result = run_primary_workflow(
                worker_app.clone(),
                generation,
                config,
                primary_pid,
                retry_sequence,
            );
            finish(&worker_app, generation, result);
        })
    {
        let message = format!("创建换房工作线程失败: {error}");
        finish(&app, generation, Err(message.clone()));
        return Err(message);
    }
    Ok(status)
}

pub fn start_followers(app: tauri::AppHandle) -> Result<RoomRotationStatus, String> {
    let config = load_rotation_config(&app)?;
    let followers = validate_follower_runtime(&app, &config)?;
    let (room_name, sequence) = {
        let current = runtime().lock().unwrap_or_else(|error| error.into_inner());
        if current.status.running {
            return Err("当前流程仍在执行，请等待主号建房指令发送完成".to_string());
        }
        let room_name = current
            .pending_room_name
            .clone()
            .ok_or_else(|| "尚无待跟进房间，请先按主号建房快捷键".to_string())?;
        let sequence = current
            .pending_sequence
            .ok_or_else(|| "待跟进房间缺少序号，请重新执行主号建房".to_string())?;
        (room_name, sequence)
    };
    let (generation, status) = {
        let mut current = runtime().lock().unwrap_or_else(|error| error.into_inner());
        if current.status.running {
            return Err("已有一轮自动换房正在运行".to_string());
        }
        current.generation = current.generation.saturating_add(1);
        current.cancelled = false;
        current.status = RoomRotationStatus {
            running: true,
            phase: "joining_followers".to_string(),
            message: format!("正在让 {} 个小号并行加入 {room_name}", followers.len()),
            room_name: Some(room_name.clone()),
            attempt: 1,
            primary_account_id: Some(config.primary_account_id.clone()),
            follower_account_ids: config.follower_account_ids.clone(),
            started_at: Some(chrono::Local::now().to_rfc3339()),
            last_error: None,
        };
        (current.generation, current.status.clone())
    };
    emit_status(&app, &status);
    let worker_app = app.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("room-rotation-followers".to_string())
        .spawn(move || {
            let result = run_follower_workflow(
                worker_app.clone(),
                generation,
                config,
                followers,
                room_name,
                sequence,
            );
            finish(&worker_app, generation, result);
        })
    {
        let message = format!("创建小号跟进工作线程失败: {error}");
        finish(&app, generation, Err(message.clone()));
        return Err(message);
    }
    Ok(status)
}

fn emit_shortcut_error(app: &tauri::AppHandle, error: String) {
    crate::logger::log_msg("ERROR", "RoomRotation", &error);
    let status = RoomRotationStatus {
        phase: "error".to_string(),
        message: error.clone(),
        last_error: Some(error),
        ..RoomRotationStatus::default()
    };
    emit_status(app, &status);
}

pub fn start_primary_from_shortcut(app: tauri::AppHandle) {
    if let Err(error) = start_primary(app.clone()) {
        emit_shortcut_error(&app, error);
    }
}

pub fn start_followers_from_shortcut(app: tauri::AppHandle) {
    if let Err(error) = start_followers(app.clone()) {
        emit_shortcut_error(&app, error);
    }
}

#[tauri::command]
pub fn start_room_rotation(app: tauri::AppHandle) -> Result<RoomRotationStatus, String> {
    start_primary(app)
}

#[tauri::command]
pub fn join_room_rotation_followers(app: tauri::AppHandle) -> Result<RoomRotationStatus, String> {
    start_followers(app)
}

#[tauri::command]
pub fn cancel_room_rotation(app: tauri::AppHandle) -> RoomRotationStatus {
    let status = {
        let mut current = runtime().lock().unwrap_or_else(|error| error.into_inner());
        if current.status.running {
            current.cancelled = true;
            current.status.message = "正在停止自动换房".to_string();
        }
        current.status.clone()
    };
    emit_status(&app, &status);
    status
}

#[tauri::command]
pub fn get_room_rotation_status() -> RoomRotationStatus {
    runtime()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .status
        .clone()
}

#[tauri::command]
pub fn test_room_rotation_input(
    app: tauri::AppHandle,
    request: RoomRotationInputTestRequest,
) -> Result<String, String> {
    let RoomRotationInputTestRequest {
        account_id,
        action,
        sample,
        click_variant,
        text_variant,
        flow_strategy,
        point_override,
    } = request;
    #[cfg(target_os = "windows")]
    {
        let state = app.state::<SharedState>();
        let (config, pid) = {
            let stored = state.config.read();
            let config = stored
                .as_ref()
                .ok_or_else(|| "尚未加载全局配置".to_string())?
                .room_rotation
                .clone();
            let pid = resolve_pid(&state, &account_id)?;
            (config, pid)
        };
        let hwnd = crate::commands::system::find_game_hwnd(pid)
            .ok_or_else(|| format!("无法找到账号“{account_id}”的 D2R 窗口"))?;
        let flow = match flow_strategy.as_deref() {
            Some("direct_lobby") => config.direct_lobby_flow.clone(),
            Some("standard") => config.standard_flow.clone(),
            _ => config.flow_for_account(&account_id).clone(),
        };
        if let Some(message) = win::test_background_key_action(hwnd, &action)? {
            return Ok(format!("账号“{account_id}”：{message}"));
        }
        let original = win::foreground_window();
        let driver = if let Some(strategy) = click_variant.as_deref() {
            win::WindowDriver::for_click_test(strategy)
        } else if text_variant.is_some() {
            win::WindowDriver::new(
                "cursor_guard",
                &config.background_click_strategy,
                config.cursor_lease_ms,
            )
        } else {
            win::WindowDriver::new(
                &config.input_mode,
                &config.background_click_strategy,
                config.cursor_lease_ms,
            )
        };
        let configured_point = match action.as_str() {
            "save_exit" => Some(flow.ui_profile.save_and_exit),
            "lobby" => Some(flow.ui_profile.character_select_lobby),
            "create_tab" => Some(flow.ui_profile.create_tab),
            "join_tab" => Some(flow.ui_profile.join_tab),
            "create_game_name_field" => Some(flow.ui_profile.create_game_name_field),
            "create_password_field" => Some(flow.ui_profile.create_password_field),
            "create_submit" => Some(flow.ui_profile.create_submit_button),
            "join_game_name_field" => Some(flow.ui_profile.join_game_name_field),
            "join_password_field" => Some(flow.ui_profile.join_password_field),
            "join_submit" => Some(flow.ui_profile.join_submit_button),
            "confirm" => Some(flow.ui_profile.dialog_confirm),
            _ => None,
        };
        let tested_point = configured_point.map(|point| point_override.unwrap_or(point));
        let mut click_report = None;
        let text_strategy = text_variant
            .as_deref()
            .unwrap_or(&config.background_text_strategy);
        if !matches!(
            text_strategy,
            "post_keys_paced"
                | "post_keys_1ms"
                | "post_ctrl_v"
                | "send_ctrl_v"
                | "post_paste"
                | "send_paste"
        ) {
            return Err("未知的后台填字方案".to_string());
        }
        let mut text_report = None;
        match action.as_str() {
            "escape" => driver.key(hwnd, win::VK_ESCAPE)?,
            "save_exit"
            | "lobby"
            | "create_tab"
            | "join_tab"
            | "create_game_name_field"
            | "create_password_field"
            | "create_submit"
            | "join_game_name_field"
            | "join_password_field"
            | "join_submit"
            | "confirm" => {
                let point = tested_point.ok_or_else(|| "缺少点击测试坐标".to_string())?;
                click_report = Some(driver.click_report(hwnd, point)?);
            }
            "create_name" | "join_name" => {
                let form = driver.begin_form_input(hwnd)?;
                let (tab, game_name_field) = if action == "create_name" {
                    (
                        flow.ui_profile.create_tab,
                        flow.ui_profile.create_game_name_field,
                    )
                } else {
                    (
                        flow.ui_profile.join_tab,
                        flow.ui_profile.join_game_name_field,
                    )
                };
                form.click(tab)?;
                remember_selected_form_tab(&account_id, pid, action == "create_name");
                std::thread::sleep(Duration::from_millis(flow.step_delay_ms));
                form.click(game_name_field)?;
                std::thread::sleep(Duration::from_millis(flow.step_delay_ms));
                let value = sample.unwrap_or_else(|| "d2rtest123".to_string());
                if !value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                }) {
                    return Err("测试文字只能包含英文字母、数字、短横线和下划线".to_string());
                }
                form.paste_text(&app, &value, flow.character_delay_ms, text_strategy)?;
                text_report = Some(win::background_text_strategy_label(text_strategy));
            }
            "create_password_text" | "join_password_text" | "password_text" => {
                let form = driver.begin_form_input(hwnd)?;
                let create = action != "join_password_text";
                let (tab, password_field) = if create {
                    (
                        flow.ui_profile.create_tab,
                        flow.ui_profile.create_password_field,
                    )
                } else {
                    (
                        flow.ui_profile.join_tab,
                        flow.ui_profile.join_password_field,
                    )
                };
                form.click(tab)?;
                remember_selected_form_tab(&account_id, pid, create);
                std::thread::sleep(Duration::from_millis(flow.step_delay_ms));
                form.click(password_field)?;
                std::thread::sleep(Duration::from_millis(flow.step_delay_ms));
                let value = sample.unwrap_or_else(|| config.password.clone());
                if !value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                }) {
                    return Err("测试密码只能包含英文字母、数字、短横线和下划线".to_string());
                }
                form.paste_text(&app, &value, flow.character_delay_ms, text_strategy)?;
                text_report = Some(win::background_text_strategy_label(text_strategy));
            }
            _ => return Err("未知的后台输入测试动作".to_string()),
        }
        if driver.focuses_windows() && original != 0 {
            crate::commands::system::bring_window_to_foreground_raw(original);
        }
        if let Some(point) = tested_point {
            let (pixel_x, pixel_y) = win::client_point(hwnd, point)?;
            let report = click_report.ok_or_else(|| "未生成点击诊断结果".to_string())?;
            Ok(format!(
                "{}：X {:.1}%、Y {:.1}% → 客户区 ({pixel_x}, {pixel_y}) px；{} HWND=0x{:X}",
                report.strategy_label,
                f32::from(point.x) / 10.0,
                f32::from(point.y) / 10.0,
                if report.used_child {
                    "命中子窗口"
                } else {
                    "D2R 主窗口"
                },
                report.target_hwnd,
            ))
        } else if let Some(text_report) = text_report {
            let focus_report = if driver.focuses_windows() {
                "短暂聚焦回退"
            } else {
                "未主动激活窗口"
            };
            Ok(format!(
                "已向账号“{account_id}”发送测试动作：{action}；填字={text_report}；{focus_report}"
            ))
        } else {
            Ok(format!("已向账号“{account_id}”发送测试动作：{action}"))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (
            app,
            account_id,
            action,
            sample,
            click_variant,
            text_variant,
            flow_strategy,
            point_override,
        );
        Err("后台输入测试仅支持 Windows".to_string())
    }
}

#[cfg(target_os = "windows")]
mod win {
    use super::{client_coordinate, RoomRotationPoint, MAX_GAME_NAME_LENGTH};
    use std::sync::{Mutex, MutexGuard};

    pub const VK_RETURN: u16 = 0x0D;
    pub const VK_ESCAPE: u16 = 0x1B;
    const VK_CONTROL: u16 = 0x11;
    const VK_A: u16 = 0x41;
    const VK_V: u16 = 0x56;
    const VK_BACK: u16 = 0x08;
    const VK_END: u16 = 0x23;
    const VK_TAB: u16 = 0x09;
    const VK_SHIFT: u16 = 0x10;
    const VK_SPACE: u16 = 0x20;
    const VK_LEFT: u16 = 0x25;
    const VK_UP: u16 = 0x26;
    const VK_RIGHT: u16 = 0x27;
    const VK_DOWN: u16 = 0x28;
    const VK_OEM_MINUS: u16 = 0xBD;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const WM_MOUSEMOVE: u32 = 0x0200;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_PASTE: u32 = 0x0302;
    const MK_LBUTTON: usize = 0x0001;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    const CWP_SKIPINVISIBLE: u32 = 0x0001;
    const CWP_SKIPDISABLED: u32 = 0x0002;
    const CWP_SKIPTRANSPARENT: u32 = 0x0004;
    const SMTO_BLOCK: u32 = 0x0001;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;
    const SMTO_ERRORONEXIT: u32 = 0x0020;
    const MAPVK_VK_TO_VSC: u32 = 0;
    // D2R may sample key state instead of consuming every queued key message.
    // Keep each synthetic key down long enough to cross a rendered frame and
    // leave a release gap so repeated digits such as `00` are not coalesced.
    const FORM_KEY_DOWN_MS: u64 = 30;
    const FORM_KEY_UP_GAP_MS: u64 = 10;
    const FORM_FOCUS_SETTLE_MS: u64 = 80;

    static GLOBAL_INPUT_LOCK: Mutex<()> = Mutex::new(());

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    extern "system" {
        fn PostMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> i32;
        fn SendMessageTimeoutW(
            hWnd: isize,
            Msg: u32,
            wParam: usize,
            lParam: isize,
            fuFlags: u32,
            uTimeout: u32,
            lpdwResult: *mut usize,
        ) -> isize;
        fn ChildWindowFromPointEx(hWndParent: isize, Point: Point, uFlags: u32) -> isize;
        fn MapWindowPoints(
            hWndFrom: isize,
            hWndTo: isize,
            lpPoints: *mut Point,
            cPoints: u32,
        ) -> i32;
        fn MapVirtualKeyW(uCode: u32, uMapType: u32) -> u32;
        fn GetClientRect(hWnd: isize, lpRect: *mut Rect) -> i32;
        fn ClientToScreen(hWnd: isize, lpPoint: *mut Point) -> i32;
        fn IsIconic(hWnd: isize) -> i32;
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
        fn GetCursorPos(lpPoint: *mut Point) -> i32;
        fn GetClipCursor(lpRect: *mut Rect) -> i32;
        fn ClipCursor(lpRect: *const Rect) -> i32;
        fn SetCursorPos(X: i32, Y: i32) -> i32;
        fn keybd_event(bVk: u8, bScan: u8, dwFlags: u32, dwExtraInfo: usize);
        fn mouse_event(dwFlags: u32, dx: u32, dy: u32, dwData: u32, dwExtraInfo: usize);
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum InputMode {
        Background,
        CursorGuard,
        Focus,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum BackgroundClickStrategy {
        PostTop,
        SendTop,
        PostChild,
        SendChild,
    }

    impl BackgroundClickStrategy {
        fn from_value(value: &str) -> Self {
            match value {
                "send_top" => Self::SendTop,
                "post_child" => Self::PostChild,
                "send_child" => Self::SendChild,
                _ => Self::PostTop,
            }
        }

        fn uses_child(self) -> bool {
            matches!(self, Self::PostChild | Self::SendChild)
        }

        fn is_synchronous(self) -> bool {
            matches!(self, Self::SendTop | Self::SendChild)
        }

        fn label(self) -> &'static str {
            match self {
                Self::PostTop => "A 异步主窗口",
                Self::SendTop => "B 同步主窗口",
                Self::PostChild => "C 异步命中窗口",
                Self::SendChild => "D 同步命中窗口",
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum BackgroundTextStrategy {
        PostKeysPaced,
        PostCtrlV,
        SendCtrlV,
        PostPaste,
        SendPaste,
    }

    impl BackgroundTextStrategy {
        fn from_value(value: &str) -> Self {
            match value {
                "post_keys_paced" | "post_keys_1ms" => Self::PostKeysPaced,
                "send_ctrl_v" => Self::SendCtrlV,
                "post_paste" => Self::PostPaste,
                "send_paste" => Self::SendPaste,
                "post_ctrl_v" => Self::PostCtrlV,
                _ => Self::PostKeysPaced,
            }
        }

        fn is_synchronous(self) -> bool {
            matches!(self, Self::SendCtrlV | Self::SendPaste)
        }

        fn uses_paste_message(self) -> bool {
            matches!(self, Self::PostPaste | Self::SendPaste)
        }

        fn uses_fast_keys(self) -> bool {
            self == Self::PostKeysPaced
        }

        fn label(self) -> &'static str {
            match self {
                Self::PostKeysPaced => "L 后台跨帧逐字",
                Self::PostCtrlV => "H 异步 Ctrl+V",
                Self::SendCtrlV => "I 同步 Ctrl+V",
                Self::PostPaste => "J 异步 WM_PASTE",
                Self::SendPaste => "K 同步 WM_PASTE",
            }
        }
    }

    pub(super) fn background_text_strategy_label(value: &str) -> &'static str {
        BackgroundTextStrategy::from_value(value).label()
    }

    pub struct ClickReport {
        pub strategy_label: String,
        pub target_hwnd: isize,
        pub used_child: bool,
    }

    pub struct WindowDriver {
        mode: InputMode,
        background_click_strategy: BackgroundClickStrategy,
        cursor_lease_ms: u64,
    }

    pub(super) struct FormInputSession<'a> {
        driver: &'a WindowDriver,
        hwnd: isize,
        original_hwnd: Option<isize>,
        _guard: MutexGuard<'static, ()>,
    }

    impl FormInputSession<'_> {
        fn ensure_foreground(&self) -> Result<(), String> {
            if activate_form_window(self.hwnd) {
                Ok(())
            } else {
                Err("无法激活目标 D2R 窗口，已停止表单操作".to_string())
            }
        }

        pub fn click(&self, point: RoomRotationPoint) -> Result<(), String> {
            let (x, y) = client_point(self.hwnd, point)?;
            match self.driver.mode {
                InputMode::Background => {
                    background_click(self.hwnd, x, y, self.driver.background_click_strategy)
                        .map(|_| ())
                }
                InputMode::CursorGuard => {
                    guarded_cursor_click_unlocked(self.hwnd, x, y, self.driver.cursor_lease_ms)
                        .map(|_| ())
                }
                InputMode::Focus => {
                    self.ensure_foreground()?;
                    guarded_real_click(self.hwnd, x, y)
                }
            }
        }

        fn replace_real_text(&self, value: &str, character_delay_ms: u64) -> Result<(), String> {
            validate_text_value(value)?;
            self.ensure_foreground()?;
            clear_real_text();
            let character_delay_ms = character_delay_ms.clamp(1, 250);
            for character in value.chars() {
                real_character(character)?;
                std::thread::sleep(std::time::Duration::from_millis(character_delay_ms));
            }
            Ok(())
        }

        pub fn paste_text(
            &self,
            app: &tauri::AppHandle,
            value: &str,
            fallback_character_delay_ms: u64,
            text_strategy: &str,
        ) -> Result<(), String> {
            use tauri_plugin_clipboard_manager::ClipboardExt;

            validate_text_value(value)?;
            let strategy = BackgroundTextStrategy::from_value(text_strategy);
            if strategy.uses_fast_keys() {
                return match self.driver.mode {
                    InputMode::Background | InputMode::CursorGuard => {
                        replace_background_text_paced(self.hwnd, value, fallback_character_delay_ms)
                    }
                    InputMode::Focus => self.replace_real_text(value, 1),
                };
            }

            let previous_text = app.clipboard().read_text().ok();
            if let Err(error) = app.clipboard().write_text(value) {
                return if self.driver.mode == InputMode::Focus {
                    self.replace_real_text(value, fallback_character_delay_ms)
                        .map_err(|fallback_error| {
                            format!("写入剪贴板失败（{error}），键盘回退也失败：{fallback_error}")
                        })
                } else {
                    Err(format!("写入剪贴板失败，无法执行后台粘贴：{error}"))
                };
            }

            let result = (|| -> Result<(), String> {
                match self.driver.mode {
                    InputMode::Background | InputMode::CursorGuard => {
                        clear_background_text(self.hwnd, strategy.is_synchronous())?;
                        if !value.is_empty() {
                            deliver_background_paste(self.hwnd, strategy)?;
                        }
                        Ok(())
                    }
                    InputMode::Focus => {
                        self.ensure_foreground()?;
                        clear_real_text();
                        if !value.is_empty() {
                            real_key_event(VK_CONTROL, 0);
                            std::thread::sleep(std::time::Duration::from_millis(
                                FORM_KEY_UP_GAP_MS,
                            ));
                            real_key_fast(VK_V);
                            real_key_event(VK_CONTROL, KEYEVENTF_KEYUP);
                        }
                        Ok(())
                    }
                }
            })();
            let clipboard_settle_ms = if strategy.is_synchronous() { 60 } else { 180 };
            std::thread::sleep(std::time::Duration::from_millis(clipboard_settle_ms));
            if let Some(previous_text) = previous_text {
                let _ = app.clipboard().write_text(previous_text);
            }
            result
        }

        pub fn key(&self, vk: u16, text_strategy: &str) -> Result<(), String> {
            match self.driver.mode {
                InputMode::Background | InputMode::CursorGuard => deliver_background_key(
                    self.hwnd,
                    vk,
                    false,
                    BackgroundTextStrategy::from_value(text_strategy).is_synchronous(),
                )?,
                InputMode::Focus => {
                    self.ensure_foreground()?;
                    real_key(vk);
                }
            }
            Ok(())
        }
    }

    impl Drop for FormInputSession<'_> {
        fn drop(&mut self) {
            if let Some(original_hwnd) = self.original_hwnd {
                if original_hwnd != 0 && original_hwnd != self.hwnd {
                    crate::commands::system::bring_window_to_foreground_raw(original_hwnd);
                }
            }
        }
    }

    impl WindowDriver {
        pub fn new(
            input_mode: &str,
            background_click_strategy: &str,
            cursor_lease_ms: u64,
        ) -> Self {
            Self {
                mode: match input_mode {
                    "cursor_guard" => InputMode::CursorGuard,
                    "focus" => InputMode::Focus,
                    _ => InputMode::Background,
                },
                background_click_strategy: BackgroundClickStrategy::from_value(
                    background_click_strategy,
                ),
                cursor_lease_ms: cursor_lease_ms.clamp(4, 50),
            }
        }

        pub fn for_click_test(strategy: &str) -> Self {
            let cursor_lease_ms = match strategy {
                "cursor_guard_8" => Some(8),
                "cursor_guard_16" => Some(16),
                "cursor_guard_32" => Some(32),
                _ => None,
            };
            if let Some(cursor_lease_ms) = cursor_lease_ms {
                Self {
                    mode: InputMode::CursorGuard,
                    background_click_strategy: BackgroundClickStrategy::PostTop,
                    cursor_lease_ms,
                }
            } else {
                Self {
                    mode: InputMode::Background,
                    background_click_strategy: BackgroundClickStrategy::from_value(strategy),
                    cursor_lease_ms: 16,
                }
            }
        }

        pub fn focuses_windows(&self) -> bool {
            self.mode == InputMode::Focus
        }

        pub(super) fn begin_form_input(&self, hwnd: isize) -> Result<FormInputSession<'_>, String> {
            self.validate_target(hwnd)?;
            let guard = GLOBAL_INPUT_LOCK
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let original_hwnd = if self.mode == InputMode::Focus {
                let original_hwnd = foreground_window();
                if !activate_form_window(hwnd) {
                    if original_hwnd != 0 && original_hwnd != hwnd {
                        crate::commands::system::bring_window_to_foreground_raw(original_hwnd);
                    }
                    return Err("无法激活目标 D2R 窗口，未执行表单操作".to_string());
                }
                Some(original_hwnd)
            } else {
                None
            };
            Ok(FormInputSession {
                driver: self,
                hwnd,
                original_hwnd,
                _guard: guard,
            })
        }

        fn validate_target(&self, hwnd: isize) -> Result<(), String> {
            if hwnd == 0 {
                return Err("目标 D2R 窗口不存在".to_string());
            }
            if unsafe { IsIconic(hwnd) } != 0 {
                return Err("目标 D2R 窗口已最小化，请先恢复窗口".to_string());
            }
            Ok(())
        }

        fn prepare(&self, hwnd: isize) -> Result<(), String> {
            self.validate_target(hwnd)?;
            if self.mode == InputMode::Focus {
                crate::commands::system::bring_window_to_foreground_raw(hwnd);
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            Ok(())
        }

        pub fn key(&self, hwnd: isize, vk: u16) -> Result<(), String> {
            self.prepare(hwnd)?;
            match self.mode {
                InputMode::Background | InputMode::CursorGuard => {
                    deliver_background_key(hwnd, vk, false, false)?
                }
                InputMode::Focus => real_key(vk),
            }
            Ok(())
        }

        pub fn click(&self, hwnd: isize, point: RoomRotationPoint) -> Result<(), String> {
            self.click_report(hwnd, point).map(|_| ())
        }

        pub fn click_report(
            &self,
            hwnd: isize,
            point: RoomRotationPoint,
        ) -> Result<ClickReport, String> {
            self.prepare(hwnd)?;
            let (x, y) = client_point(hwnd, point)?;
            match self.mode {
                InputMode::Background => {
                    background_click(hwnd, x, y, self.background_click_strategy)
                }
                InputMode::CursorGuard => guarded_cursor_click(hwnd, x, y, self.cursor_lease_ms),
                InputMode::Focus => {
                    real_click(hwnd, x, y)?;
                    Ok(ClickReport {
                        strategy_label: "前台物理输入".to_string(),
                        target_hwnd: hwnd,
                        used_child: false,
                    })
                }
            }
        }
    }

    pub(super) fn client_point(
        hwnd: isize,
        point: RoomRotationPoint,
    ) -> Result<(i32, i32), String> {
        let mut rect = Rect::default();
        if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
            return Err("无法读取 D2R 客户区尺寸".to_string());
        }
        let width = rect.right.saturating_sub(rect.left);
        let height = rect.bottom.saturating_sub(rect.top);
        if width <= 0 || height <= 0 {
            return Err("D2R 客户区尺寸无效".to_string());
        }
        Ok((
            client_coordinate(width, point.x),
            client_coordinate(height, point.y),
        ))
    }

    fn make_lparam(x: i32, y: i32) -> isize {
        let packed = (u32::try_from(x).unwrap_or_default() & 0xFFFF)
            | ((u32::try_from(y).unwrap_or_default() & 0xFFFF) << 16);
        packed as isize
    }

    pub(super) fn test_background_key_action(
        hwnd: isize,
        action: &str,
    ) -> Result<Option<String>, String> {
        let Some(spec) = action.strip_prefix("key_") else {
            return Ok(None);
        };
        let Some((delivery, key_name)) = spec.split_once('_') else {
            return Err("后台按键测试动作格式无效".to_string());
        };
        let synchronous = match delivery {
            "post" => false,
            "send" => true,
            _ => return Err("未知的后台按键投递方案".to_string()),
        };
        let (vk, shift, label) = match key_name {
            "escape" => (VK_ESCAPE, false, "Esc"),
            "tab" => (VK_TAB, false, "Tab"),
            "shift_tab" => (VK_TAB, true, "Shift+Tab"),
            "left" => (VK_LEFT, false, "←"),
            "up" => (VK_UP, false, "↑"),
            "right" => (VK_RIGHT, false, "→"),
            "down" => (VK_DOWN, false, "↓"),
            "space" => (VK_SPACE, false, "Space"),
            "enter" => (VK_RETURN, false, "Enter"),
            _ => return Err("未知的后台按键".to_string()),
        };
        if hwnd == 0 || unsafe { IsIconic(hwnd) } != 0 {
            return Err("目标 D2R 窗口不存在或已最小化".to_string());
        }
        deliver_background_key(hwnd, vk, shift, synchronous)?;
        Ok(Some(format!(
            "已用{}投递 {label}，未移动鼠标、未激活窗口",
            if synchronous {
                "同步 SendMessageTimeout"
            } else {
                "异步 PostMessage"
            }
        )))
    }

    fn deliver_background_key(
        hwnd: isize,
        vk: u16,
        shift: bool,
        synchronous: bool,
    ) -> Result<(), String> {
        if shift {
            deliver_key_message(hwnd, VK_SHIFT, true, synchronous)?;
            std::thread::sleep(std::time::Duration::from_millis(12));
        }
        deliver_key_message(hwnd, vk, true, synchronous)?;
        std::thread::sleep(std::time::Duration::from_millis(35));
        deliver_key_message(hwnd, vk, false, synchronous)?;
        if shift {
            std::thread::sleep(std::time::Duration::from_millis(12));
            deliver_key_message(hwnd, VK_SHIFT, false, synchronous)?;
        }
        Ok(())
    }

    fn character_key(character: char) -> Result<(u16, bool), String> {
        match character {
            'a'..='z' => Ok((character.to_ascii_uppercase() as u16, false)),
            'A'..='Z' => Ok((character as u16, true)),
            '0'..='9' => Ok((character as u16, false)),
            '-' => Ok((VK_OEM_MINUS, false)),
            '_' => Ok((VK_OEM_MINUS, true)),
            _ => Err(format!("不支持输入字符：{character}")),
        }
    }

    fn deliver_background_key_paced(
        hwnd: isize,
        vk: u16,
        shift: bool,
        release_gap_ms: u64,
    ) -> Result<(), String> {
        if shift {
            deliver_key_message(hwnd, VK_SHIFT, true, false)?;
        }
        deliver_key_message(hwnd, vk, true, false)?;
        std::thread::sleep(std::time::Duration::from_millis(FORM_KEY_DOWN_MS));
        deliver_key_message(hwnd, vk, false, false)?;
        if shift {
            deliver_key_message(hwnd, VK_SHIFT, false, false)?;
        }
        std::thread::sleep(std::time::Duration::from_millis(
            release_gap_ms.clamp(5, 250),
        ));
        Ok(())
    }

    fn replace_background_text_paced(
        hwnd: isize,
        value: &str,
        release_gap_ms: u64,
    ) -> Result<(), String> {
        // Move to the end and erase the entire bounded field without Ctrl+A,
        // because D2R may ignore synthetic modifier state in a background window.
        deliver_background_key_paced(hwnd, VK_END, false, release_gap_ms)?;
        for _ in 0..MAX_GAME_NAME_LENGTH {
            deliver_background_key_paced(hwnd, VK_BACK, false, release_gap_ms)?;
        }
        for character in value.chars() {
            let (vk, shift) = character_key(character)?;
            deliver_background_key_paced(hwnd, vk, shift, release_gap_ms)?;
        }
        Ok(())
    }

    fn deliver_background_chord(
        hwnd: isize,
        modifier: u16,
        vk: u16,
        synchronous: bool,
    ) -> Result<(), String> {
        deliver_key_message(hwnd, modifier, true, synchronous)?;
        std::thread::sleep(std::time::Duration::from_millis(12));
        deliver_key_message(hwnd, vk, true, synchronous)?;
        std::thread::sleep(std::time::Duration::from_millis(35));
        deliver_key_message(hwnd, vk, false, synchronous)?;
        std::thread::sleep(std::time::Duration::from_millis(12));
        deliver_key_message(hwnd, modifier, false, synchronous)
    }

    fn clear_background_text(hwnd: isize, synchronous: bool) -> Result<(), String> {
        deliver_background_chord(hwnd, VK_CONTROL, VK_A, synchronous)?;
        std::thread::sleep(std::time::Duration::from_millis(15));
        deliver_background_key(hwnd, VK_BACK, false, synchronous)
    }

    fn deliver_background_paste(
        hwnd: isize,
        strategy: BackgroundTextStrategy,
    ) -> Result<(), String> {
        if strategy.uses_paste_message() {
            if strategy.is_synchronous() {
                send_window_message(hwnd, WM_PASTE, 0, 0)
            } else {
                post_window_message(hwnd, WM_PASTE, 0, 0)
            }
        } else {
            deliver_background_chord(hwnd, VK_CONTROL, VK_V, strategy.is_synchronous())
        }
    }

    fn deliver_key_message(
        hwnd: isize,
        vk: u16,
        pressed: bool,
        synchronous: bool,
    ) -> Result<(), String> {
        let message = if pressed { WM_KEYDOWN } else { WM_KEYUP };
        let l_param = key_lparam(vk, pressed);
        if synchronous {
            send_window_message(hwnd, message, usize::from(vk), l_param)
        } else {
            post_window_message(hwnd, message, usize::from(vk), l_param)
        }
    }

    fn key_lparam(vk: u16, pressed: bool) -> isize {
        let scan_code = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } & 0xFF;
        let extended = matches!(vk, VK_END | VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN);
        let mut value = 1u32 | (scan_code << 16);
        if extended {
            value |= 1 << 24;
        }
        if !pressed {
            value |= (1 << 30) | (1 << 31);
        }
        value as isize
    }

    fn background_click(
        top_hwnd: isize,
        x: i32,
        y: i32,
        strategy: BackgroundClickStrategy,
    ) -> Result<ClickReport, String> {
        let (target_hwnd, target_x, target_y) = if strategy.uses_child() {
            deepest_child_at_point(top_hwnd, x, y)
        } else {
            (top_hwnd, x, y)
        };
        let position = make_lparam(target_x, target_y);
        if strategy.is_synchronous() {
            send_window_message(target_hwnd, WM_MOUSEMOVE, 0, position)?;
            send_window_message(target_hwnd, WM_LBUTTONDOWN, MK_LBUTTON, position)?;
            std::thread::sleep(std::time::Duration::from_millis(35));
            send_window_message(target_hwnd, WM_LBUTTONUP, 0, position)?;
        } else {
            post_window_message(target_hwnd, WM_MOUSEMOVE, 0, position)?;
            post_window_message(target_hwnd, WM_LBUTTONDOWN, MK_LBUTTON, position)?;
            std::thread::sleep(std::time::Duration::from_millis(35));
            post_window_message(target_hwnd, WM_LBUTTONUP, 0, position)?;
        }
        Ok(ClickReport {
            strategy_label: strategy.label().to_string(),
            target_hwnd,
            used_child: target_hwnd != top_hwnd,
        })
    }

    struct CursorLease {
        original_position: Point,
        original_clip: Rect,
    }

    impl CursorLease {
        fn acquire(target: Point) -> Result<Self, String> {
            let mut original_position = Point::default();
            if unsafe { GetCursorPos(&mut original_position) } == 0 {
                return Err("无法保存系统光标位置".to_string());
            }
            let mut original_clip = Rect::default();
            if unsafe { GetClipCursor(&mut original_clip) } == 0 {
                return Err("无法读取系统光标裁剪区域".to_string());
            }
            let target_clip = Rect {
                left: target.x,
                top: target.y,
                right: target.x.saturating_add(1),
                bottom: target.y.saturating_add(1),
            };
            if unsafe { ClipCursor(&target_clip) } == 0 {
                return Err("无法锁定系统光标到 D2R 点击位置".to_string());
            }
            if unsafe { SetCursorPos(target.x, target.y) } == 0 {
                unsafe {
                    ClipCursor(&original_clip);
                }
                return Err("无法移动系统光标到 D2R 点击位置".to_string());
            }
            Ok(Self {
                original_position,
                original_clip,
            })
        }
    }

    impl Drop for CursorLease {
        fn drop(&mut self) {
            unsafe {
                ClipCursor(&self.original_clip);
                SetCursorPos(self.original_position.x, self.original_position.y);
            }
        }
    }

    fn guarded_cursor_click(
        hwnd: isize,
        x: i32,
        y: i32,
        lease_ms: u64,
    ) -> Result<ClickReport, String> {
        let _lock = GLOBAL_INPUT_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        guarded_cursor_click_unlocked(hwnd, x, y, lease_ms)
    }

    fn guarded_cursor_click_unlocked(
        hwnd: isize,
        x: i32,
        y: i32,
        lease_ms: u64,
    ) -> Result<ClickReport, String> {
        let mut screen = Point { x, y };
        if unsafe { ClientToScreen(hwnd, &mut screen) } == 0 {
            return Err("无法换算 D2R 光标租约坐标".to_string());
        }
        let _lease = CursorLease::acquire(screen)?;
        let position = make_lparam(x, y);
        send_window_message_with_timeout(hwnd, WM_MOUSEMOVE, 0, position, 40)?;
        send_window_message_with_timeout(hwnd, WM_LBUTTONDOWN, MK_LBUTTON, position, 40)?;
        std::thread::sleep(std::time::Duration::from_millis(lease_ms.clamp(4, 50)));
        if let Err(error) = send_window_message_with_timeout(hwnd, WM_LBUTTONUP, 0, position, 40) {
            let _ = post_window_message(hwnd, WM_LBUTTONUP, 0, position);
            return Err(error);
        }
        Ok(ClickReport {
            strategy_label: format!("光标租约 {lease_ms}ms"),
            target_hwnd: hwnd,
            used_child: false,
        })
    }

    fn guarded_real_click(hwnd: isize, x: i32, y: i32) -> Result<(), String> {
        let mut screen = Point { x, y };
        if unsafe { ClientToScreen(hwnd, &mut screen) } == 0 {
            return Err("无法换算 D2R 真实点击坐标".to_string());
        }
        let _lease = CursorLease::acquire(screen)?;
        unsafe { mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0) };
        std::thread::sleep(std::time::Duration::from_millis(25));
        unsafe { mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0) };
        Ok(())
    }

    fn activate_form_window(hwnd: isize) -> bool {
        if foreground_window() == hwnd {
            return true;
        }
        crate::commands::system::bring_window_to_foreground_raw(hwnd);
        for _ in 0..12 {
            std::thread::sleep(std::time::Duration::from_millis(25));
            if foreground_window() == hwnd {
                std::thread::sleep(std::time::Duration::from_millis(FORM_FOCUS_SETTLE_MS));
                return true;
            }
        }
        false
    }

    fn validate_text_value(value: &str) -> Result<(), String> {
        if value.len() > MAX_GAME_NAME_LENGTH {
            return Err("输入内容超过 15 个字符".to_string());
        }
        if !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        }) {
            return Err("输入内容只能包含英文字母、数字、短横线和下划线".to_string());
        }
        Ok(())
    }

    fn deepest_child_at_point(top_hwnd: isize, x: i32, y: i32) -> (isize, i32, i32) {
        let mut current = top_hwnd;
        let mut point = Point { x, y };
        let flags = CWP_SKIPINVISIBLE | CWP_SKIPDISABLED | CWP_SKIPTRANSPARENT;
        for _ in 0..8 {
            let child = unsafe { ChildWindowFromPointEx(current, point, flags) };
            if child == 0 || child == current {
                break;
            }
            unsafe {
                MapWindowPoints(current, child, &mut point, 1);
            }
            current = child;
        }
        (current, point.x, point.y)
    }

    fn post_window_message(
        hwnd: isize,
        message: u32,
        w_param: usize,
        l_param: isize,
    ) -> Result<(), String> {
        if unsafe { PostMessageW(hwnd, message, w_param, l_param) } == 0 {
            return Err(format!("PostMessage 发送失败：消息 0x{message:X}"));
        }
        Ok(())
    }

    fn send_window_message(
        hwnd: isize,
        message: u32,
        w_param: usize,
        l_param: isize,
    ) -> Result<(), String> {
        send_window_message_with_timeout(hwnd, message, w_param, l_param, 250)
    }

    fn send_window_message_with_timeout(
        hwnd: isize,
        message: u32,
        w_param: usize,
        l_param: isize,
        timeout_ms: u32,
    ) -> Result<(), String> {
        let mut result = 0usize;
        let flags = SMTO_BLOCK | SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT;
        if unsafe {
            SendMessageTimeoutW(
                hwnd,
                message,
                w_param,
                l_param,
                flags,
                timeout_ms,
                &mut result,
            )
        } == 0
        {
            return Err(format!(
                "SendMessageTimeout 发送失败或超时：消息 0x{message:X}"
            ));
        }
        Ok(())
    }

    fn real_key_event(vk: u16, flags: u32) {
        let scan_code = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as u8;
        unsafe { keybd_event(vk as u8, scan_code, flags, 0) };
    }

    fn real_key(vk: u16) {
        real_key_event(vk, 0);
        std::thread::sleep(std::time::Duration::from_millis(30));
        real_key_event(vk, KEYEVENTF_KEYUP);
    }

    fn real_key_fast(vk: u16) {
        real_key_event(vk, 0);
        std::thread::sleep(std::time::Duration::from_millis(FORM_KEY_DOWN_MS));
        real_key_event(vk, KEYEVENTF_KEYUP);
        std::thread::sleep(std::time::Duration::from_millis(FORM_KEY_UP_GAP_MS));
    }

    fn clear_real_text() {
        real_key_event(VK_CONTROL, 0);
        real_key_fast(VK_A);
        real_key_event(VK_CONTROL, KEYEVENTF_KEYUP);
        std::thread::sleep(std::time::Duration::from_millis(FORM_KEY_UP_GAP_MS));
        real_key_fast(VK_BACK);
    }

    fn real_character(character: char) -> Result<(), String> {
        let (vk, shift) = character_key(character)?;
        if shift {
            real_key_event(VK_SHIFT, 0);
            std::thread::sleep(std::time::Duration::from_millis(FORM_KEY_UP_GAP_MS));
        }
        real_key_fast(vk);
        if shift {
            real_key_event(VK_SHIFT, KEYEVENTF_KEYUP);
            std::thread::sleep(std::time::Duration::from_millis(FORM_KEY_UP_GAP_MS));
        }
        Ok(())
    }

    fn real_click(hwnd: isize, x: i32, y: i32) -> Result<(), String> {
        let mut screen = Point { x, y };
        if unsafe { ClientToScreen(hwnd, &mut screen) } == 0 {
            return Err("无法换算 D2R 点击坐标".to_string());
        }
        let mut original = Point::default();
        let has_original = unsafe { GetCursorPos(&mut original) } != 0;
        unsafe {
            SetCursorPos(screen.x, screen.y);
            mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        }
        std::thread::sleep(std::time::Duration::from_millis(35));
        unsafe {
            mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
            if has_original {
                SetCursorPos(original.x, original.y);
            }
        }
        Ok(())
    }

    pub fn foreground_window() -> isize {
        unsafe { GetForegroundWindow() }
    }

    pub fn foreground_pid() -> Option<u32> {
        let hwnd = foreground_window();
        if hwnd == 0 {
            return None;
        }
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        (pid != 0).then_some(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_name_uses_zero_padded_sequence() {
        let config = RoomRotationConfig {
            name_prefix: "chaos-".to_string(),
            sequence_width: 3,
            ..RoomRotationConfig::default()
        };
        assert_eq!(room_name(&config, 7).unwrap(), "chaos-007");
        assert_eq!(room_name(&config, 1234).unwrap(), "chaos-1234");
    }

    #[test]
    fn password_cache_is_scoped_to_account_process_form_and_value() {
        let account_id = "password-cache-test-account";
        assert!(password_needs_update(account_id, 101, true, "1"));
        remember_applied_password(account_id, 101, true, "1");
        assert!(!password_needs_update(account_id, 101, true, "1"));
        assert!(password_needs_update(account_id, 101, true, "2"));
        assert!(password_needs_update(account_id, 101, false, "1"));
        assert!(password_needs_update(account_id, 202, true, "1"));
    }

    #[test]
    fn form_tab_cache_is_scoped_to_account_process_and_mode() {
        let account_id = "form-tab-cache-test-account";
        assert!(form_tab_needs_select(account_id, 101, true));
        remember_selected_form_tab(account_id, 101, true);
        assert!(!form_tab_needs_select(account_id, 101, true));
        assert!(form_tab_needs_select(account_id, 101, false));
        assert!(form_tab_needs_select(account_id, 202, true));
    }

    #[test]
    fn room_name_rejects_values_over_game_limit() {
        let config = RoomRotationConfig {
            name_prefix: "abcdefghijklmn".to_string(),
            sequence_width: 3,
            ..RoomRotationConfig::default()
        };
        assert!(room_name(&config, 1).is_err());
    }

    #[test]
    fn client_coordinates_keep_the_full_percentage_inside_the_window() {
        assert_eq!(client_coordinate(1_280, 0), 0);
        assert_eq!(client_coordinate(1_280, 500), 640);
        assert_eq!(client_coordinate(1_280, 1_000), 1_279);
        assert_eq!(client_coordinate(720, u16::MAX), 719);
    }
}
