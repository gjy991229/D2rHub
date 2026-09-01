use crate::commands::global_config::{RoomRotationConfig, RoomRotationFlowStrategy};
use crate::state::SharedState;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const STATUS_EVENT: &str = "room-rotation-status";
const MAX_GAME_NAME_LENGTH: usize = 15;

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

#[derive(Default)]
struct Runtime {
    generation: u64,
    cancelled: bool,
    pending_room_name: Option<String>,
    pending_sequence: Option<u32>,
    applied_passwords: HashMap<String, AppliedPassword>,
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
    if value.chars().count() > MAX_GAME_NAME_LENGTH {
        return Err(format!(
            "房间名“{value}”超过 D2R 的 {MAX_GAME_NAME_LENGTH} 字符限制"
        ));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err("后台房间名只支持英文字母、数字、短横线和下划线".to_string());
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

fn validate_room_tools_accounts(
    state: &SharedState,
    config: &RoomRotationConfig,
) -> Result<(), String> {
    crate::chat_key_binding::ensure_room_rotation_chat_binding_ready(state)?;
    crate::audio_mod::validate_in_game_room_tools_for_account(state, &config.primary_account_id)?;
    for account_id in &config.follower_account_ids {
        crate::audio_mod::validate_in_game_room_tools_for_account(state, account_id)?;
    }
    Ok(())
}

fn validate_primary_runtime(
    app: &tauri::AppHandle,
    config: &RoomRotationConfig,
) -> Result<u32, String> {
    validate_base_config(config)?;
    let state = app.state::<SharedState>();
    validate_room_tools_accounts(&state, config)?;
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
    validate_room_tools_accounts(&state, config)?;
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

fn validate_automatic_follower_runtime(
    app: &tauri::AppHandle,
    config: &RoomRotationConfig,
) -> Result<Vec<(String, u32)>, String> {
    validate_base_config(config)?;
    let state = app.state::<SharedState>();
    validate_room_tools_accounts(&state, config)?;
    // Automatic continuation is deliberately background-only. It verifies
    // that the primary process still exists, but never requires it to retain
    // physical foreground focus during the configured delay.
    resolve_pid(&state, &config.primary_account_id)?;
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

fn continue_with_automatic_followers(
    app: &tauri::AppHandle,
    generation: u64,
    config: RoomRotationConfig,
    completion: WorkflowCompletion,
) -> Result<WorkflowCompletion, String> {
    let (room_name, sequence) = match completion {
        WorkflowCompletion::PrimaryReady {
            room_name,
            sequence,
        } => (room_name, sequence),
        other => return Ok(other),
    };
    if !config.auto_followers_enabled {
        return Ok(WorkflowCompletion::PrimaryReady {
            room_name,
            sequence,
        });
    }

    ensure_active(generation)?;
    let delay_secs = config.auto_followers_delay_secs.clamp(2, 60);
    let status = {
        let mut current = runtime().lock().unwrap_or_else(|error| error.into_inner());
        if current.generation != generation || current.cancelled {
            return Err("换房流程已取消".to_string());
        }
        // Keep this room available for a manual retry if automatic follower
        // validation or delivery fails after the primary has already created it.
        current.pending_room_name = Some(room_name.clone());
        current.pending_sequence = Some(sequence);
        current.status.running = true;
        current.status.phase = "waiting_auto_followers".to_string();
        current.status.message = format!("主号已提交建房，{delay_secs} 秒后自动让小号并行加入");
        current.status.room_name = Some(room_name.clone());
        current.status.last_error = None;
        current.status.clone()
    };
    crate::logger::log_msg("INFO", "RoomRotation", &status.message);
    emit_status(app, &status);

    sleep_interruptible(generation, Duration::from_secs(delay_secs))?;
    let followers = validate_automatic_follower_runtime(app, &config)?;
    update_status(app, generation, |status| {
        status.phase = "joining_followers".to_string();
        status.message = format!(
            "延迟结束，正在让 {} 个小号并行加入 {room_name}",
            followers.len()
        );
        status.attempt = 1;
    });
    run_follower_workflow(
        app.clone(),
        generation,
        config,
        followers,
        room_name,
        sequence,
    )
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
                    "主号已从局内提交建房；确认进房后按小号跟进快捷键".to_string();
                current.status.room_name = Some(room_name);
                current.status.last_error = None;
            }
            Ok(WorkflowCompletion::FollowersComplete) => {
                current.pending_room_name = None;
                current.pending_sequence = None;
                current.status.phase = "complete".to_string();
                current.status.message = "所有小号已从局内并行提交加入".to_string();
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
struct RoomFormInput<'a> {
    account_id: &'a str,
    pid: u32,
    create: bool,
    open_form: bool,
    name: &'a str,
    password: &'a str,
}

#[cfg(target_os = "windows")]
fn fill_credentials(
    _app: &tauri::AppHandle,
    driver: &win::WindowDriver,
    hwnd: isize,
    flow: &RoomRotationFlowStrategy,
    input: RoomFormInput<'_>,
    generation: u64,
) -> Result<(), String> {
    let update_password =
        password_needs_update(input.account_id, input.pid, input.create, input.password);
    let form = driver.begin_form_input(hwnd)?;
    // The Mod's PausePanel gateway gives the native room form its normal first
    // field focus without any mouse input. F13 is installed as CfgChat's second
    // binding, so it enters D2R's real text-input state without submitting the
    // form like Enter would. Ordinary background key messages then reach the
    // focused room field before gameplay hotkeys.
    if input.open_form {
        form.open_room_form_with_keyboard(input.create, flow.step_delay_ms)?;
    }
    form.enter_native_chat_mode(flow.step_delay_ms)?;
    form.replace_text(input.name, flow.character_delay_ms)?;
    sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    form.advance_to_password()?;
    sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    if update_password {
        form.replace_text(input.password, flow.character_delay_ms)?;
        sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    }
    form.submit()?;
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
    let driver = win::WindowDriver::new(&config.background_text_strategy);
    let primary_hwnd = crate::commands::system::find_game_hwnd(primary_pid)
        .ok_or_else(|| "无法找到主号 D2R 窗口".to_string())?;
    let flow = config.flow_for_account(&config.primary_account_id);

    if retry_sequence.is_some() {
        update_status(&app, generation, |status| {
            status.phase = "retrying_primary".to_string();
            status.message = "正在确认重名弹窗并使用下一个序号重试".to_string();
        });
        driver.press_return(primary_hwnd)?;
        sleep_interruptible(generation, Duration::from_millis(flow.step_delay_ms))?;
    } else {
        update_status(&app, generation, |status| {
            status.phase = "opening_primary_room_form".to_string();
            status.message = "正在用纯键盘入口打开主号创建房间面板".to_string();
        });
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
            open_form: retry_sequence.is_none(),
            name: &candidate,
            password: &config.password,
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
    app: tauri::AppHandle,
    generation: u64,
    config: RoomRotationConfig,
    account_id: String,
    pid: u32,
    room_name: String,
) -> Result<(), String> {
    let driver = win::WindowDriver::new(&config.background_text_strategy);
    let hwnd = crate::commands::system::find_game_hwnd(pid)
        .ok_or_else(|| format!("无法找到小号“{account_id}”的 D2R 窗口"))?;
    let flow = config.flow_for_account(&account_id);
    fill_credentials(
        &app,
        &driver,
        hwnd,
        flow,
        RoomFormInput {
            account_id: &account_id,
            pid,
            create: false,
            open_form: true,
            name: &room_name,
            password: &config.password,
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
                worker_app,
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
                config.clone(),
                primary_pid,
                retry_sequence,
            )
            .and_then(|completion| {
                continue_with_automatic_followers(&worker_app, generation, config, completion)
            });
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

#[cfg(target_os = "windows")]
mod win {
    use super::MAX_GAME_NAME_LENGTH;

    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const SMTO_BLOCK: u32 = 0x0001;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;
    const SMTO_ERRORONEXIT: u32 = 0x0020;
    const VK_BACK: u16 = 0x08;
    const VK_TAB: u16 = 0x09;
    const VK_RETURN: u16 = 0x0D;
    const VK_SHIFT: u16 = 0x10;
    const VK_ESCAPE: u16 = 0x1B;
    const VK_END: u16 = 0x23;
    const VK_LEFT: u16 = 0x25;
    const VK_RIGHT: u16 = 0x27;
    const VK_F13: u16 = 0x7C;
    const VK_OEM_MINUS: u16 = 0xBD;
    const MAPVK_VK_TO_VSC: u32 = 0;
    const KEY_DOWN_HOLD_MS: u64 = 14;
    const MIN_CHARACTER_GAP_MS: u64 = 10;
    const PUNCTUATION_GAP_MS: u64 = 18;
    const FIELD_CLEAR_SETTLE_MS: u64 = 24;
    const CHAT_MODE_SETTLE_MS: u64 = 120;
    const GATEWAY_DIRECTION_REPETITIONS: usize = 2;
    // The native fields accept at most 15 characters. One extra Backspace is
    // enough to guarantee an empty field after moving to End.
    const FIELD_CLEAR_COUNT: usize = MAX_GAME_NAME_LENGTH + 1;
    const ROOM_FORM_SETTLE_MS: u64 = 200;

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
        fn MapVirtualKeyW(uCode: u32, uMapType: u32) -> u32;
        fn IsIconic(hWnd: isize) -> i32;
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum BackgroundTextStrategy {
        PostKeys,
        SendKeys,
    }

    impl BackgroundTextStrategy {
        fn from_value(value: &str) -> Self {
            match value {
                "send_keys" | "send_keys_chat" | "send_paste" => Self::SendKeys,
                _ => Self::PostKeys,
            }
        }

        fn is_synchronous(self) -> bool {
            matches!(self, Self::SendKeys)
        }
    }

    pub struct WindowDriver {
        background_text_strategy: BackgroundTextStrategy,
    }

    pub(super) struct FormInputSession<'a> {
        driver: &'a WindowDriver,
        hwnd: isize,
    }

    impl FormInputSession<'_> {
        pub fn enter_native_chat_mode(&self, step_delay_ms: u64) -> Result<(), String> {
            let step = step_delay_ms.clamp(60, 500);
            deliver_background_key(
                self.hwnd,
                VK_F13,
                false,
                self.driver.background_text_strategy,
                20,
            )?;
            // F13 changes CfgChat's global input routing. Its key messages can
            // return before that state is visible to the next room-field key,
            // so keep a small state-transition guard independent of UI speed.
            std::thread::sleep(std::time::Duration::from_millis(
                step.max(CHAT_MODE_SETTLE_MS),
            ));
            Ok(())
        }

        pub fn open_room_form_with_keyboard(
            &self,
            create: bool,
            step_delay_ms: u64,
        ) -> Result<(), String> {
            let step = step_delay_ms.clamp(60, 500);
            deliver_background_key(
                self.hwnd,
                VK_ESCAPE,
                false,
                self.driver.background_text_strategy,
                20,
            )?;
            std::thread::sleep(std::time::Duration::from_millis(step));

            let direction = if create { VK_LEFT } else { VK_RIGHT };
            for _ in 0..GATEWAY_DIRECTION_REPETITIONS {
                deliver_background_key(
                    self.hwnd,
                    direction,
                    false,
                    self.driver.background_text_strategy,
                    20,
                )?;
                std::thread::sleep(std::time::Duration::from_millis(step));
            }
            deliver_background_key(
                self.hwnd,
                VK_RETURN,
                false,
                self.driver.background_text_strategy,
                20,
            )?;
            std::thread::sleep(std::time::Duration::from_millis(step));
            // The hidden gateway closes PausePanel and opens a timer-driven
            // native room panel. Give D2R several frames to finish that panel
            // transition before F13 and text messages enter the queue.
            std::thread::sleep(std::time::Duration::from_millis(ROOM_FORM_SETTLE_MS));
            Ok(())
        }

        pub fn submit(&self) -> Result<(), String> {
            deliver_background_key(
                self.hwnd,
                VK_RETURN,
                false,
                self.driver.background_text_strategy,
                20,
            )
        }

        pub fn advance_to_password(&self) -> Result<(), String> {
            deliver_background_key(
                self.hwnd,
                VK_TAB,
                false,
                self.driver.background_text_strategy,
                20,
            )
        }

        pub fn replace_text(&self, value: &str, character_delay_ms: u64) -> Result<(), String> {
            replace_background_text(
                self.hwnd,
                value,
                character_delay_ms,
                self.driver.background_text_strategy,
            )
        }
    }

    impl WindowDriver {
        pub fn new(background_text_strategy: &str) -> Self {
            Self {
                background_text_strategy: BackgroundTextStrategy::from_value(
                    background_text_strategy,
                ),
            }
        }

        pub(super) fn begin_form_input(&self, hwnd: isize) -> Result<FormInputSession<'_>, String> {
            self.validate_target(hwnd)?;
            Ok(FormInputSession { driver: self, hwnd })
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

        pub fn press_return(&self, hwnd: isize) -> Result<(), String> {
            self.validate_target(hwnd)?;
            deliver_background_key(hwnd, VK_RETURN, false, self.background_text_strategy, 20)
        }
    }

    fn replace_background_text(
        hwnd: isize,
        value: &str,
        character_delay_ms: u64,
        strategy: BackgroundTextStrategy,
    ) -> Result<(), String> {
        validate_text_value(value)?;
        let key_gap_ms = character_delay_ms.clamp(MIN_CHARACTER_GAP_MS, 250);

        // CfgChat's native text-input state suppresses gameplay bindings before
        // these ordinary key messages reach the currently focused room field.
        deliver_background_key(hwnd, VK_END, false, strategy, key_gap_ms)?;
        for _ in 0..FIELD_CLEAR_COUNT {
            // Keep the key-down pulse, but do not add a release gap
            // between consecutive Backspaces. This is the largest avoidable
            // delay in every field replacement.
            deliver_background_key(hwnd, VK_BACK, false, strategy, 0)?;
        }
        // D2R occasionally consumed the first value key while the final
        // Backspace was still being applied by the native text controller.
        std::thread::sleep(std::time::Duration::from_millis(FIELD_CLEAR_SETTLE_MS));
        for character in value.chars() {
            let (vk, shift) = character_key(character)?;
            let release_gap_ms = if matches!(character, '-' | '_') {
                key_gap_ms.max(PUNCTUATION_GAP_MS)
            } else {
                key_gap_ms
            };
            deliver_background_key(hwnd, vk, shift, strategy, release_gap_ms)?;
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
            _ => Err(format!("后台原生聊天态输入暂不支持字符：{character}")),
        }
    }

    fn deliver_background_key(
        hwnd: isize,
        vk: u16,
        shift: bool,
        strategy: BackgroundTextStrategy,
        release_gap_ms: u64,
    ) -> Result<(), String> {
        if shift {
            deliver_key_message(hwnd, VK_SHIFT, true, strategy)?;
        }
        deliver_key_message(hwnd, vk, true, strategy)?;
        std::thread::sleep(std::time::Duration::from_millis(KEY_DOWN_HOLD_MS));
        deliver_key_message(hwnd, vk, false, strategy)?;
        if shift {
            deliver_key_message(hwnd, VK_SHIFT, false, strategy)?;
        }
        std::thread::sleep(std::time::Duration::from_millis(release_gap_ms));
        Ok(())
    }

    fn deliver_key_message(
        hwnd: isize,
        vk: u16,
        pressed: bool,
        strategy: BackgroundTextStrategy,
    ) -> Result<(), String> {
        let message = if pressed { WM_KEYDOWN } else { WM_KEYUP };
        let l_param = key_lparam(vk, pressed);
        if strategy.is_synchronous() {
            send_window_message(hwnd, message, usize::from(vk), l_param)
        } else {
            post_window_message(hwnd, message, usize::from(vk), l_param)
        }
    }

    fn key_lparam(vk: u16, pressed: bool) -> isize {
        let scan_code = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } & 0xFF;
        let extended = vk == VK_END;
        let mut value = 1u32 | (scan_code << 16);
        if extended {
            value |= 1 << 24;
        }
        if !pressed {
            value |= (1 << 30) | (1 << 31);
        }
        value as isize
    }

    fn validate_text_value(value: &str) -> Result<(), String> {
        if value.chars().count() > MAX_GAME_NAME_LENGTH {
            return Err("输入内容超过 15 个字符".to_string());
        }
        if !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        }) {
            return Err("后台原生聊天态输入只支持英文字母、数字、短横线和下划线".to_string());
        }
        Ok(())
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
    fn room_name_rejects_a_unicode_prefix_for_background_key_delivery() {
        let config = RoomRotationConfig {
            name_prefix: "巴尔-".to_string(),
            sequence_width: 3,
            ..RoomRotationConfig::default()
        };
        assert!(room_name(&config, 7).is_err());
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
    fn room_name_rejects_values_over_game_limit() {
        let config = RoomRotationConfig {
            name_prefix: "abcdefghijklmn".to_string(),
            sequence_width: 3,
            ..RoomRotationConfig::default()
        };
        assert!(room_name(&config, 1).is_err());
    }
}
