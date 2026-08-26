use super::catalog::{location_definition, LocationKind, TelemetryMarker};
use super::protocol::StreamingDetector;
use super::tracking::{SceneTransitionGate, SegmentTracker, TrackedRuneDrop, TrackingSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{Emitter, Manager};

const CAPTURE_SAMPLE_RATE: u32 = 48_000;
const CAPTURE_CHANNELS: usize = 2;
const SCAN_INTERVAL_FRAMES: usize = 4_800;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneAudioEvent {
    pub source: String,
    pub account_id: String,
    pub rune_number: u32,
    pub rune_name: String,
    pub rune_name_en: String,
    pub confidence: f32,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationAudioEvent {
    pub source: String,
    pub account_id: String,
    pub area_id: Option<u32>,
    pub scene_key: String,
    pub scene_name: String,
    pub scene_name_en: String,
    pub location_kind: LocationKind,
    pub is_town: bool,
    pub is_frontend: bool,
    pub confidence: f32,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneAudioStatus {
    pub running: bool,
    pub account_id: Option<String>,
    pub target_pid: Option<u32>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
struct MonitorConfig {
    account_id: String,
    character_name: String,
    target_pid: u32,
    threshold: f32,
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static STATUS: OnceLock<Mutex<RuneAudioStatus>> = OnceLock::new();

fn status() -> &'static Mutex<RuneAudioStatus> {
    STATUS.get_or_init(|| {
        Mutex::new(RuneAudioStatus {
            running: false,
            account_id: None,
            target_pid: None,
            last_error: None,
        })
    })
}

fn set_status(next: RuneAudioStatus) {
    *status().lock().unwrap_or_else(|error| error.into_inner()) = next;
}

fn resolve_monitor_config(app: &tauri::AppHandle) -> Result<MonitorConfig, String> {
    let state = app.state::<crate::state::SharedState>();
    let (config, account) = {
        let guard = state.config.read();
        let config = guard
            .as_ref()
            .ok_or_else(|| "尚未完成首次配置".to_string())?;
        let account = config
            .resolve_rune_audio_target_account()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "符文声纹识别尚未启用".to_string())?;
        (config.clone(), account)
    };
    let target_pid = state
        .active_games
        .read()
        .get(&account.id)
        .copied()
        .or(account.running_pid)
        .ok_or_else(|| format!("目标账号“{}”的 D2R 尚未运行", account.display_name))?;
    Ok(MonitorConfig {
        character_name: if account.display_name.trim().is_empty() {
            account.id.clone()
        } else {
            account.display_name.clone()
        },
        account_id: account.id,
        target_pid,
        threshold: config.rune_audio_detection_threshold.clamp(0.45, 0.95),
    })
}

fn emit_tracking_snapshot(app: &tauri::AppHandle, snapshot: &TrackingSnapshot) {
    if let Err(error) = app.emit("audio-tracking-state", snapshot) {
        crate::logger::log_msg(
            "WARN",
            "RuneAudio",
            &format!("推送自动统计状态失败: {error}"),
        );
    }
}

fn handle_rune_detection(
    app: &tauri::AppHandle,
    config: &MonitorConfig,
    tracker: &mut SegmentTracker,
    rune_number: u32,
    confidence: f32,
) {
    if !tracker.accepts_rune_observation() {
        return;
    }
    let rune_name = crate::rune_data::get_rune_name(rune_number)
        .unwrap_or("未知符文")
        .to_string();
    let rune_name_en = crate::rune_data::RUNE_NAMES_EN
        .get((rune_number.saturating_sub(1)) as usize)
        .copied()
        .unwrap_or("Unknown")
        .to_string();
    let event = RuneAudioEvent {
        source: "rune_audio".to_string(),
        account_id: config.account_id.clone(),
        rune_number,
        rune_name,
        rune_name_en,
        confidence,
        timestamp: chrono::Local::now().to_rfc3339(),
    };
    let state = app.state::<crate::state::SharedState>();
    let observation_id = match crate::stats::insert_rune_drop_observation(
        &state,
        crate::stats::NewRuneDropObservation {
            observed_at: &event.timestamp,
            account_id: &event.account_id,
            rune_number: event.rune_number,
            rune_name: &event.rune_name,
            rune_name_en: &event.rune_name_en,
            confidence: event.confidence,
            source: &event.source,
        },
    ) {
        Ok(id) => id,
        Err(error) => {
            crate::logger::log_msg(
                "ERROR",
                "RuneAudio",
                &format!("保存符文声纹事件失败: {error}"),
            );
            -1
        }
    };
    let snapshot = tracker.observe_rune(TrackedRuneDrop {
        observation_id,
        rune_number,
        rune_name: event.rune_name.clone(),
        rune_name_en: event.rune_name_en.clone(),
    });
    if let Err(error) = app.emit("rune-audio-detected", &event) {
        crate::logger::log_msg(
            "WARN",
            "RuneAudio",
            &format!("推送符文声纹事件失败: {error}"),
        );
    }
    emit_tracking_snapshot(app, &snapshot);
}

fn handle_location_detection(
    app: &tauri::AppHandle,
    config: &MonitorConfig,
    tracker: &mut SegmentTracker,
    marker: TelemetryMarker,
    observed_at_frame: u64,
    confidence: f32,
) {
    let Some(location) = location_definition(marker) else {
        return;
    };
    let now = chrono::Local::now();
    let outcome = match tracker.observe_location(
        marker,
        observed_at_frame,
        now.timestamp_millis(),
        now.format("%Y/%m/%d/%H:%M:%S").to_string(),
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            crate::logger::log_msg("ERROR", "RuneAudio", &format!("处理场景状态失败: {error}"));
            return;
        }
    };
    // 环境事件会周期性重播；只有真实场景变化才进入计时与统计链路。
    if !outcome.changed {
        return;
    }
    if let Some(completed_segment) = &outcome.completed_segment {
        let state = app.state::<crate::state::SharedState>();
        if let Err(error) = crate::stats::save_completed_segment(&state, completed_segment) {
            crate::logger::log_msg(
                "ERROR",
                "RuneAudio",
                &format!("保存自动野外分段失败: {error}"),
            );
        }
    }
    let event = LocationAudioEvent {
        source: "location_audio".to_string(),
        account_id: config.account_id.clone(),
        area_id: match marker {
            TelemetryMarker::Area { area_id } => Some(area_id),
            TelemetryMarker::Rune { .. } | TelemetryMarker::Frontend => None,
        },
        scene_key: location.scene_key.to_string(),
        scene_name: location.scene_name.to_string(),
        scene_name_en: location.scene_name_en.to_string(),
        location_kind: location.kind,
        is_town: location.kind == LocationKind::Town,
        is_frontend: location.kind == LocationKind::Frontend,
        confidence,
        timestamp: now.to_rfc3339(),
    };
    if let Err(error) = app.emit("location-audio-detected", &event) {
        crate::logger::log_msg(
            "WARN",
            "RuneAudio",
            &format!("推送场景声纹事件失败: {error}"),
        );
    }
    emit_tracking_snapshot(app, &outcome.snapshot);
}

#[cfg(target_os = "windows")]
fn capture_loop(
    app: tauri::AppHandle,
    config: MonitorConfig,
    generation: u64,
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    use wasapi::{initialize_mta, AudioClient, Direction, SampleType, StreamMode, WaveFormat};

    initialize_mta()
        .ok()
        .map_err(|error| format!("初始化 WASAPI COM 失败: {error}"))?;
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            wasapi::deinitialize();
        }
    }
    let _com_guard = ComGuard;
    let desired_format = WaveFormat::new(
        32,
        32,
        &SampleType::Float,
        CAPTURE_SAMPLE_RATE as usize,
        CAPTURE_CHANNELS,
        None,
    );
    let block_align = desired_format.get_blockalign() as usize;
    let mut audio_client = AudioClient::new_application_loopback_client(config.target_pid, true)
        .map_err(|error| format!("创建 D2R 进程音频捕获失败: {error}"))?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 200_000,
    };
    audio_client
        .initialize_client(&desired_format, &Direction::Capture, &mode)
        .map_err(|error| format!("初始化 D2R 音频流失败: {error}"))?;
    let event_handle = audio_client
        .set_get_eventhandle()
        .map_err(|error| format!("创建音频事件句柄失败: {error}"))?;
    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|error| format!("获取音频捕获接口失败: {error}"))?;
    audio_client
        .start_stream()
        .map_err(|error| format!("启动 D2R 音频流失败: {error}"))?;

    let _ = ready.send(Ok(()));
    let mut bytes = VecDeque::new();
    let mut pending_mono = Vec::<f32>::with_capacity(SCAN_INTERVAL_FRAMES * 2);
    let mut detector = StreamingDetector::new(CAPTURE_SAMPLE_RATE, config.threshold)?;
    let mut tracker = SegmentTracker::new(
        config.account_id.clone(),
        config.character_name.clone(),
        CAPTURE_SAMPLE_RATE,
    );
    let mut scene_gate = SceneTransitionGate::new(CAPTURE_SAMPLE_RATE);
    let mut process_system = sysinfo::System::new();
    let process_id = sysinfo::Pid::from(config.target_pid as usize);
    let mut process_check_ticks = 0u8;

    while RUNNING.load(Ordering::SeqCst) && GENERATION.load(Ordering::SeqCst) == generation {
        let _ = event_handle.wait_for_event(500);
        process_check_ticks = process_check_ticks.saturating_add(1);
        if process_check_ticks >= 4 {
            process_check_ticks = 0;
            process_system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[process_id]));
            if process_system.process(process_id).is_none() {
                break;
            }
        }
        loop {
            let packet_frames = capture_client
                .get_next_packet_size()
                .map_err(|error| format!("查询音频包失败: {error}"))?
                .unwrap_or(0);
            if packet_frames == 0 {
                break;
            }
            let required = packet_frames as usize * block_align;
            bytes.reserve(required.saturating_sub(bytes.capacity() - bytes.len()));
            capture_client
                .read_from_device_to_deque(&mut bytes)
                .map_err(|error| format!("读取 D2R 音频失败: {error}"))?;
        }

        while bytes.len() >= block_align {
            let mut channel_sum = 0.0f32;
            for _ in 0..CAPTURE_CHANNELS {
                let raw = [
                    bytes.pop_front().unwrap_or(0),
                    bytes.pop_front().unwrap_or(0),
                    bytes.pop_front().unwrap_or(0),
                    bytes.pop_front().unwrap_or(0),
                ];
                channel_sum += f32::from_le_bytes(raw);
            }
            for _ in CAPTURE_CHANNELS * 4..block_align {
                let _ = bytes.pop_front();
            }
            pending_mono.push(channel_sum / CAPTURE_CHANNELS as f32);
        }

        if pending_mono.len() < SCAN_INTERVAL_FRAMES {
            continue;
        }
        let chunk = std::mem::replace(
            &mut pending_mono,
            Vec::with_capacity(SCAN_INTERVAL_FRAMES * 2),
        );
        for detection in detector.push(&chunk) {
            match detection.marker {
                TelemetryMarker::Rune { rune_number } => handle_rune_detection(
                    &app,
                    &config,
                    &mut tracker,
                    rune_number,
                    detection.confidence,
                ),
                marker @ (TelemetryMarker::Area { .. } | TelemetryMarker::Frontend) => {
                    if scene_gate.observe(marker, detection.start_frame) {
                        handle_location_detection(
                            &app,
                            &config,
                            &mut tracker,
                            marker,
                            detection.start_frame,
                            detection.confidence,
                        );
                    }
                }
            }
        }
    }

    let _ = audio_client.stop_stream();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn capture_loop(
    _app: tauri::AppHandle,
    _config: MonitorConfig,
    _generation: u64,
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    let error = "符文声纹实时捕获目前只支持 Windows".to_string();
    let _ = ready.send(Err(error.clone()));
    Err(error)
}

fn start_blocking(app: tauri::AppHandle) -> Result<(), String> {
    let config = resolve_monitor_config(&app)?;
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    if WORKER_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        RUNNING.store(false, Ordering::SeqCst);
        return Err("上一个符文声纹工作线程仍在退出".to_string());
    }

    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    set_status(RuneAudioStatus {
        running: false,
        account_id: Some(config.account_id.clone()),
        target_pid: Some(config.target_pid),
        last_error: None,
    });

    std::thread::Builder::new()
        .name("rune-audio-capture".to_string())
        .spawn(move || {
            let result = capture_loop(app, config.clone(), generation, ready_tx);
            RUNNING.store(false, Ordering::SeqCst);
            WORKER_ACTIVE.store(false, Ordering::SeqCst);
            if GENERATION.load(Ordering::SeqCst) == generation {
                set_status(RuneAudioStatus {
                    running: false,
                    account_id: Some(config.account_id),
                    target_pid: Some(config.target_pid),
                    last_error: result.as_ref().err().cloned(),
                });
            }
            if let Err(error) = result {
                crate::logger::log_msg("ERROR", "RuneAudio", &error);
            }
        })
        .map_err(|error| {
            RUNNING.store(false, Ordering::SeqCst);
            WORKER_ACTIVE.store(false, Ordering::SeqCst);
            format!("创建符文声纹工作线程失败: {error}")
        })?;

    match ready_rx.recv() {
        Ok(Ok(())) => {
            let mut current = status().lock().unwrap_or_else(|error| error.into_inner());
            current.running = true;
            Ok(())
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err("符文声纹工作线程在初始化期间异常退出".to_string()),
    }
}

#[tauri::command]
pub async fn start_rune_audio_monitor(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start_blocking(app))
        .await
        .map_err(|error| format!("等待符文声纹监控器启动失败: {error}"))?
}

#[tauri::command]
pub fn stop_rune_audio_monitor() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    RUNNING.store(false, Ordering::SeqCst);
    let mut current = status().lock().unwrap_or_else(|error| error.into_inner());
    current.running = false;
}

#[tauri::command]
pub async fn restart_rune_audio_monitor(app: tauri::AppHandle) -> Result<(), String> {
    stop_rune_audio_monitor();
    tauri::async_runtime::spawn_blocking(move || {
        for _ in 0..60 {
            if !WORKER_ACTIVE.load(Ordering::SeqCst) {
                return start_blocking(app);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err("上一个符文声纹工作线程未能在 3 秒内退出".to_string())
    })
    .await
    .map_err(|error| format!("等待符文声纹监控器重启失败: {error}"))?
}

#[tauri::command]
pub fn get_rune_audio_status() -> RuneAudioStatus {
    status()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}
