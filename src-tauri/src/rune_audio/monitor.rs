use super::catalog::{LocationCatalog, LocationKind, TelemetryMarker, MAX_AREA_ID};
use super::item_catalog::{
    gem_quality_level, ItemCatalog, ItemCatalogEntry, CATEGORY_CHARMS, CATEGORY_GEMS,
    CATEGORY_RUNES,
};
use super::protocol::{StreamingDetector, PROTOCOL_VERSION};
use super::tracking::{
    DropPresenceGate, SceneTransitionGate, SegmentTracker, TerrorZonePresenceGate, TrackedDrop,
    TrackedDropKind, TrackingSnapshot, GENERIC_TERROR_ZONE_NAME,
};
#[cfg(test)]
use crate::commands::launch::parse_windows_command_line;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
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
pub struct ItemAudioEvent {
    pub source: String,
    pub account_id: String,
    pub item_id: u32,
    pub item_code: String,
    pub category: String,
    pub item_name: String,
    pub item_name_en: String,
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
    pub tz: bool,
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
    pub captured_frames: u64,
    pub audio_peak: f32,
    pub decoded_packets: u64,
    pub rune_events: u64,
    pub item_events: u64,
    pub scene_heartbeats: u64,
    pub last_marker: Option<String>,
    pub last_confidence: Option<f32>,
    pub last_detected_at: Option<String>,
    pub diagnostic_recording: bool,
    pub diagnostic_recording_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticDetection {
    observed_at: String,
    start_frame: u64,
    marker: TelemetryMarker,
    confidence: f32,
    logical_event: bool,
}

#[derive(Debug, Serialize)]
struct DiagnosticSidecar {
    protocol_version: u8,
    started_at: String,
    stopped_at: String,
    account_id: String,
    target_pid: u32,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    wav_file: String,
    write_error: Option<String>,
    detections: Vec<DiagnosticDetection>,
}

struct DiagnosticRecording {
    path: PathBuf,
    started_at: String,
    account_id: String,
    target_pid: u32,
    writer: hound::WavWriter<BufWriter<File>>,
    write_error: Option<String>,
    detections: Vec<DiagnosticDetection>,
}

#[derive(Debug, Clone)]
struct MonitorConfig {
    account_id: String,
    character_name: String,
    target_pid: u32,
    threshold: f32,
    tracked_categories: HashSet<String>,
    min_rune_number: u32,
    min_gem_level: u32,
    tracked_charm_codes: HashSet<String>,
    catalog_directory: Option<PathBuf>,
}

fn should_record_rune(config: &MonitorConfig, tracker: &SegmentTracker, rune_number: u32) -> bool {
    tracker.accepts_drop_observations()
        && config.tracked_categories.contains(CATEGORY_RUNES)
        && rune_number >= config.min_rune_number
}

fn should_record_item(
    config: &MonitorConfig,
    tracker: &SegmentTracker,
    item: &ItemCatalogEntry,
) -> bool {
    if !tracker.accepts_drop_observations() || !config.tracked_categories.contains(&item.category) {
        return false;
    }
    match item.category.as_str() {
        CATEGORY_GEMS => {
            gem_quality_level(&item.code).is_some_and(|level| level >= config.min_gem_level)
        }
        CATEGORY_CHARMS => config
            .tracked_charm_codes
            .contains(&item.code.to_ascii_lowercase()),
        _ => true,
    }
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static STATUS: OnceLock<Mutex<RuneAudioStatus>> = OnceLock::new();
static DIAGNOSTIC_RECORDING: OnceLock<Mutex<Option<DiagnosticRecording>>> = OnceLock::new();

fn diagnostic_recording() -> &'static Mutex<Option<DiagnosticRecording>> {
    DIAGNOSTIC_RECORDING.get_or_init(|| Mutex::new(None))
}

fn status() -> &'static Mutex<RuneAudioStatus> {
    STATUS.get_or_init(|| {
        Mutex::new(RuneAudioStatus {
            running: false,
            account_id: None,
            target_pid: None,
            last_error: None,
            captured_frames: 0,
            audio_peak: 0.0,
            decoded_packets: 0,
            rune_events: 0,
            item_events: 0,
            scene_heartbeats: 0,
            last_marker: None,
            last_confidence: None,
            last_detected_at: None,
            diagnostic_recording: false,
            diagnostic_recording_path: None,
        })
    })
}

fn set_status(next: RuneAudioStatus) {
    *status().lock().unwrap_or_else(|error| error.into_inner()) = next;
}

fn write_diagnostic_samples(samples: &[f32]) {
    let mut guard = diagnostic_recording()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let Some(recording) = guard.as_mut() else {
        return;
    };
    if recording.write_error.is_some() {
        return;
    }
    for sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        if let Err(error) = recording.writer.write_sample(pcm) {
            recording.write_error = Some(format!("写入诊断 WAV 失败: {error}"));
            break;
        }
    }
}

fn append_diagnostic_detection(
    marker: TelemetryMarker,
    confidence: f32,
    start_frame: u64,
    logical_event: bool,
) {
    let mut guard = diagnostic_recording()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(recording) = guard.as_mut() {
        recording.detections.push(DiagnosticDetection {
            observed_at: chrono::Local::now().to_rfc3339(),
            start_frame,
            marker,
            confidence,
            logical_event,
        });
    }
}

fn finish_diagnostic_recording() -> Result<Option<String>, String> {
    let recording = diagnostic_recording()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    let Some(recording) = recording else {
        return Ok(None);
    };
    let DiagnosticRecording {
        path,
        started_at,
        account_id,
        target_pid,
        writer,
        write_error,
        detections,
    } = recording;
    let path_text = path.to_string_lossy().to_string();
    let wav_file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("diagnostic.wav")
        .to_string();
    let events_path = path.with_extension("events.json");
    let sidecar = DiagnosticSidecar {
        protocol_version: PROTOCOL_VERSION,
        started_at,
        stopped_at: chrono::Local::now().to_rfc3339(),
        account_id,
        target_pid,
        sample_rate: CAPTURE_SAMPLE_RATE,
        channels: 1,
        bits_per_sample: 16,
        wav_file,
        write_error: write_error.clone(),
        detections,
    };
    let sidecar_result = serde_json::to_vec_pretty(&sidecar)
        .map_err(|error| format!("序列化诊断事件失败: {error}"))
        .and_then(|bytes| {
            std::fs::write(&events_path, bytes)
                .map_err(|error| format!("写入诊断事件失败 {}: {error}", events_path.display()))
        });
    let finalize_result = writer
        .finalize()
        .map_err(|error| format!("完成诊断 WAV 失败 {}: {error}", path.display()));
    {
        let mut current = status().lock().unwrap_or_else(|error| error.into_inner());
        current.diagnostic_recording = false;
        current.diagnostic_recording_path = Some(path_text.clone());
    }
    if let Some(error) = write_error {
        return Err(error);
    }
    sidecar_result?;
    finalize_result?;
    Ok(Some(path_text))
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
    let launch_arguments = state
        .active_game_launches
        .read()
        .get(&account.id)
        .filter(|snapshot| snapshot.pid == target_pid)
        .map(|snapshot| snapshot.mod_args.clone())
        .unwrap_or_else(|| account.mod_args.clone());
    let catalog_directory = Some(crate::audio_mod::validate_runtime_audio_mod(
        &config,
        &account,
        &launch_arguments,
    )?);
    Ok(MonitorConfig {
        character_name: if account.display_name.trim().is_empty() {
            account.id.clone()
        } else {
            account.display_name.clone()
        },
        account_id: account.id,
        target_pid,
        threshold: config.rune_audio_detection_threshold.clamp(0.40, 0.95),
        tracked_categories: config.rune_audio_tracked_categories.into_iter().collect(),
        min_rune_number: config.rune_audio_min_rune_number.clamp(1, 33),
        min_gem_level: config.rune_audio_min_gem_level.clamp(1, 5),
        tracked_charm_codes: config
            .rune_audio_tracked_charm_codes
            .into_iter()
            .map(|code| code.to_ascii_lowercase())
            .collect(),
        catalog_directory,
    })
}

#[cfg(test)]
fn active_mod_name(mod_args: &str) -> Result<Option<String>, String> {
    if mod_args.trim().is_empty() {
        return Ok(None);
    }
    let args = parse_windows_command_line(mod_args)
        .map_err(|error| format!("无法解析声纹账号的 Mod 参数: {error}"))?;
    let value = args.iter().enumerate().find_map(|(index, argument)| {
        if argument.eq_ignore_ascii_case("-mod") {
            args.get(index + 1).cloned()
        } else {
            argument
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("-mod="))
                .then(|| argument[5..].to_string())
        }
    });
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let path = std::path::Path::new(&value);
    if path.components().count() != 1
        || path.file_name().and_then(|name| name.to_str()) != Some(value.as_str())
    {
        return Err("-mod 名称不能包含目录路径".to_string());
    }
    Ok(Some(value))
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

fn marker_status_label(
    marker: TelemetryMarker,
    catalog: &LocationCatalog,
    item_catalog: &ItemCatalog,
) -> String {
    match marker {
        TelemetryMarker::Rune { rune_number } => {
            let name = crate::rune_data::get_rune_name(rune_number).unwrap_or("未知符文");
            format!("符文 #{rune_number:02} {name}")
        }
        TelemetryMarker::Item { item_id } => item_catalog
            .resolve(item_id)
            .map(|item| format!("物品 {} ({})", item.name, item.code))
            .unwrap_or_else(|| format!("物品 #{item_id}")),
        TelemetryMarker::Area {
            area_id: MAX_AREA_ID,
        } => "恐怖区域信号".to_string(),
        marker @ (TelemetryMarker::Area { .. } | TelemetryMarker::Frontend) => catalog
            .resolve(marker)
            .map(|location| location.scene_name)
            .unwrap_or_else(|| format!("{marker:?}")),
    }
}

fn record_decoded_packet(
    marker: TelemetryMarker,
    confidence: f32,
    start_frame: u64,
    catalog: &LocationCatalog,
    item_catalog: &ItemCatalog,
    logical_event: bool,
) {
    {
        let mut current = status().lock().unwrap_or_else(|error| error.into_inner());
        current.decoded_packets += 1;
        match marker {
            TelemetryMarker::Rune { .. } if logical_event => current.rune_events += 1,
            TelemetryMarker::Rune { .. } => {}
            TelemetryMarker::Item { .. } if logical_event => current.item_events += 1,
            TelemetryMarker::Item { .. } => {}
            TelemetryMarker::Area { .. } | TelemetryMarker::Frontend => {
                current.scene_heartbeats += 1
            }
        }
        current.last_marker = Some(marker_status_label(marker, catalog, item_catalog));
        current.last_confidence = Some(confidence);
        current.last_detected_at = Some(chrono::Local::now().to_rfc3339());
    }
    append_diagnostic_detection(marker, confidence, start_frame, logical_event);
}

fn handle_rune_detection(
    app: &tauri::AppHandle,
    config: &MonitorConfig,
    tracker: &mut SegmentTracker,
    rune_number: u32,
    confidence: f32,
) {
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
    let rune_code = format!("r{rune_number:02}");
    let observation_id = match crate::stats::insert_drop_observation(
        &state,
        crate::stats::NewDropObservation {
            observed_at: &event.timestamp,
            account_id: &event.account_id,
            kind: "rune",
            telemetry_id: event.rune_number,
            item_code: Some(&rune_code),
            category: CATEGORY_RUNES,
            display_name: &event.rune_name,
            display_name_en: &event.rune_name_en,
            rune_number: Some(event.rune_number),
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
    let snapshot = tracker.observe_drop(TrackedDrop {
        observation_id,
        kind: TrackedDropKind::Rune,
        telemetry_id: rune_number,
        code: Some(rune_code),
        category: CATEGORY_RUNES.to_string(),
        name: event.rune_name.clone(),
        name_en: event.rune_name_en.clone(),
        rune_number: Some(rune_number),
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

fn handle_item_detection(
    app: &tauri::AppHandle,
    config: &MonitorConfig,
    tracker: &mut SegmentTracker,
    item_catalog: &ItemCatalog,
    item_id: u32,
    confidence: f32,
) {
    let Some(item) = item_catalog.resolve(item_id) else {
        crate::logger::log_msg(
            "WARN",
            "RuneAudio",
            &format!("识别到物品声纹 #{item_id}，但本机物品目录未登记"),
        );
        return;
    };
    if !config.tracked_categories.contains(&item.category) {
        return;
    }
    let event = ItemAudioEvent {
        source: "item_audio".to_string(),
        account_id: config.account_id.clone(),
        item_id,
        item_code: item.code.clone(),
        category: item.category.clone(),
        item_name: item.name.clone(),
        item_name_en: item.name_en.clone(),
        confidence,
        timestamp: chrono::Local::now().to_rfc3339(),
    };
    let state = app.state::<crate::state::SharedState>();
    let observation_id = match crate::stats::insert_drop_observation(
        &state,
        crate::stats::NewDropObservation {
            observed_at: &event.timestamp,
            account_id: &event.account_id,
            kind: "item",
            telemetry_id: event.item_id,
            item_code: Some(&event.item_code),
            category: &event.category,
            display_name: &event.item_name,
            display_name_en: &event.item_name_en,
            rune_number: None,
            confidence: event.confidence,
            source: &event.source,
        },
    ) {
        Ok(id) => id,
        Err(error) => {
            crate::logger::log_msg(
                "ERROR",
                "RuneAudio",
                &format!("保存物品声纹事件失败: {error}"),
            );
            -1
        }
    };
    let snapshot = tracker.observe_drop(TrackedDrop {
        observation_id,
        kind: TrackedDropKind::Item,
        telemetry_id: item_id,
        code: Some(event.item_code.clone()),
        category: event.category.clone(),
        name: event.item_name.clone(),
        name_en: event.item_name_en.clone(),
        rune_number: None,
    });
    if let Err(error) = app.emit("item-audio-detected", &event) {
        crate::logger::log_msg(
            "WARN",
            "RuneAudio",
            &format!("推送物品声纹事件失败: {error}"),
        );
    }
    emit_tracking_snapshot(app, &snapshot);
}

fn handle_location_detection(
    app: &tauri::AppHandle,
    config: &MonitorConfig,
    tracker: &mut SegmentTracker,
    catalog: &LocationCatalog,
    marker: TelemetryMarker,
    observed_at_frame: u64,
    confidence: f32,
) {
    let Some(location) = catalog.resolve(marker) else {
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
            TelemetryMarker::Rune { .. }
            | TelemetryMarker::Item { .. }
            | TelemetryMarker::Frontend => None,
        },
        scene_key: location.scene_key,
        scene_name: location.scene_name,
        scene_name_en: location.scene_name_en,
        tz: false,
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

fn handle_terror_zone_detection(
    app: &tauri::AppHandle,
    config: &MonitorConfig,
    tracker: &mut SegmentTracker,
    observed_at_frame: u64,
    confidence: f32,
) {
    let current_zone = crate::commands::terror_zone::cached_current_terror_zone();
    let (scene_name, scene_name_en) = current_zone
        .map(|zone| (zone.location_name, "Terror Zone".to_string()))
        .unwrap_or_else(|| {
            (
                GENERIC_TERROR_ZONE_NAME.to_string(),
                "Terror Zone".to_string(),
            )
        });
    let now = chrono::Local::now();
    let outcome = tracker.observe_terror_zone(
        scene_name,
        scene_name_en,
        observed_at_frame,
        now.timestamp_millis(),
        now.format("%Y/%m/%d/%H:%M:%S").to_string(),
    );
    if !outcome.changed {
        return;
    }
    if let Some(completed_segment) = &outcome.completed_segment {
        let state = app.state::<crate::state::SharedState>();
        if let Err(error) = crate::stats::save_completed_segment(&state, completed_segment) {
            crate::logger::log_msg(
                "ERROR",
                "RuneAudio",
                &format!("保存 TZ 自动野外分段失败: {error}"),
            );
        }
    }
    emit_terror_zone_tracking_update(app, config, &outcome.snapshot, confidence);
}

fn emit_terror_zone_tracking_update(
    app: &tauri::AppHandle,
    config: &MonitorConfig,
    snapshot: &TrackingSnapshot,
    confidence: f32,
) {
    let event = LocationAudioEvent {
        source: "terror_zone_audio".to_string(),
        account_id: config.account_id.clone(),
        area_id: None,
        scene_key: snapshot
            .current_run_key
            .clone()
            .unwrap_or_else(|| "terror_zone:unknown".to_string()),
        scene_name: snapshot.current_scene.clone(),
        scene_name_en: snapshot.current_scene_en.clone(),
        tz: true,
        location_kind: LocationKind::Wilderness,
        is_town: false,
        is_frontend: false,
        confidence,
        timestamp: chrono::Local::now().to_rfc3339(),
    };
    if let Err(error) = app.emit("location-audio-detected", &event) {
        crate::logger::log_msg(
            "WARN",
            "RuneAudio",
            &format!("推送 TZ 场景声纹事件失败: {error}"),
        );
    }
    emit_tracking_snapshot(app, snapshot);
}

fn upgrade_cached_terror_zone_name(
    app: &tauri::AppHandle,
    config: &MonitorConfig,
    tracker: &mut SegmentTracker,
    confidence: f32,
) {
    let Some(zone) = crate::commands::terror_zone::cached_current_terror_zone() else {
        return;
    };
    let Some(snapshot) =
        tracker.upgrade_current_terror_zone(zone.location_name, "Terror Zone".to_string())
    else {
        return;
    };
    emit_terror_zone_tracking_update(app, config, &snapshot, confidence);
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
    let (catalog, item_catalog) = if let Some(directory) = config.catalog_directory.as_deref() {
        let catalog = LocationCatalog::load_from_directory(directory).unwrap_or_else(|error| {
            crate::logger::log_msg(
                "WARN",
                "RuneAudio",
                &format!("当前 Mod 没有可用的地图协议清单，将仅显示内置地点: {error}"),
            );
            LocationCatalog::default()
        });
        let item_catalog = ItemCatalog::load_from_directory(directory).unwrap_or_else(|error| {
            crate::logger::log_msg(
                "WARN",
                "RuneAudio",
                &format!("当前 Mod 没有可用的物品协议清单，将使用协议内置名称: {error}"),
            );
            ItemCatalog::default()
        });
        (catalog, item_catalog)
    } else {
        (LocationCatalog::default(), ItemCatalog::default())
    };
    let mut tracker = SegmentTracker::with_catalog(
        config.account_id.clone(),
        config.character_name.clone(),
        CAPTURE_SAMPLE_RATE,
        catalog.clone(),
    );
    let mut scene_gate = SceneTransitionGate::new(CAPTURE_SAMPLE_RATE);
    let mut drop_gate = DropPresenceGate::new(CAPTURE_SAMPLE_RATE);
    let mut terror_zone_gate = TerrorZonePresenceGate::new(CAPTURE_SAMPLE_RATE);
    let tz_refresh_generation = generation;
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(error) = crate::commands::terror_zone::get_terror_zone_snapshot().await {
                crate::logger::log_msg(
                    "WARN",
                    "RuneAudio",
                    &format!("刷新当前 TZ 名称失败，将在声纹触发时使用通用名称: {error}"),
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(5 * 60)).await;
            if !RUNNING.load(Ordering::SeqCst)
                || GENERATION.load(Ordering::SeqCst) != tz_refresh_generation
            {
                break;
            }
        }
    });
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
        let peak = chunk
            .iter()
            .fold(0.0f32, |current, sample| current.max(sample.abs()));
        {
            let mut current = status().lock().unwrap_or_else(|error| error.into_inner());
            current.captured_frames += chunk.len() as u64;
            current.audio_peak = peak;
        }
        write_diagnostic_samples(&chunk);
        for detection in detector.push(&chunk) {
            if detection.marker
                == (TelemetryMarker::Area {
                    area_id: MAX_AREA_ID,
                })
            {
                let logical_event = terror_zone_gate.observe(detection.start_frame);
                record_decoded_packet(
                    detection.marker,
                    detection.confidence,
                    detection.start_frame,
                    &catalog,
                    &item_catalog,
                    logical_event,
                );
                if logical_event {
                    drop_gate.clear();
                    scene_gate.clear();
                    handle_terror_zone_detection(
                        &app,
                        &config,
                        &mut tracker,
                        detection.start_frame,
                        detection.confidence,
                    );
                } else {
                    upgrade_cached_terror_zone_name(
                        &app,
                        &config,
                        &mut tracker,
                        detection.confidence,
                    );
                }
                continue;
            }
            match detection.marker {
                marker @ TelemetryMarker::Rune { rune_number } => {
                    let tracked = should_record_rune(&config, &tracker, rune_number);
                    let logical_event = tracked && drop_gate.observe(marker, detection.start_frame);
                    record_decoded_packet(
                        detection.marker,
                        detection.confidence,
                        detection.start_frame,
                        &catalog,
                        &item_catalog,
                        logical_event,
                    );
                    if logical_event {
                        handle_rune_detection(
                            &app,
                            &config,
                            &mut tracker,
                            rune_number,
                            detection.confidence,
                        );
                    }
                }
                marker @ TelemetryMarker::Item { item_id } => {
                    let tracked = item_catalog
                        .resolve(item_id)
                        .is_some_and(|item| should_record_item(&config, &tracker, item));
                    let logical_event = tracked && drop_gate.observe(marker, detection.start_frame);
                    record_decoded_packet(
                        detection.marker,
                        detection.confidence,
                        detection.start_frame,
                        &catalog,
                        &item_catalog,
                        logical_event,
                    );
                    if logical_event {
                        handle_item_detection(
                            &app,
                            &config,
                            &mut tracker,
                            &item_catalog,
                            item_id,
                            detection.confidence,
                        );
                    }
                }
                marker @ (TelemetryMarker::Area { .. } | TelemetryMarker::Frontend) => {
                    let logical_event = scene_gate.observe(marker, detection.start_frame);
                    record_decoded_packet(
                        detection.marker,
                        detection.confidence,
                        detection.start_frame,
                        &catalog,
                        &item_catalog,
                        logical_event,
                    );
                    if logical_event {
                        drop_gate.clear();
                        handle_location_detection(
                            &app,
                            &config,
                            &mut tracker,
                            &catalog,
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
    if let Err(error) = finish_diagnostic_recording() {
        crate::logger::log_msg("ERROR", "RuneAudio", &error);
    }
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
        captured_frames: 0,
        audio_peak: 0.0,
        decoded_packets: 0,
        rune_events: 0,
        item_events: 0,
        scene_heartbeats: 0,
        last_marker: None,
        last_confidence: None,
        last_detected_at: None,
        diagnostic_recording: false,
        diagnostic_recording_path: None,
    });

    std::thread::Builder::new()
        .name("rune-audio-capture".to_string())
        .spawn(move || {
            let result = capture_loop(app, config.clone(), generation, ready_tx);
            RUNNING.store(false, Ordering::SeqCst);
            WORKER_ACTIVE.store(false, Ordering::SeqCst);
            if GENERATION.load(Ordering::SeqCst) == generation {
                let previous = status()
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .clone();
                set_status(RuneAudioStatus {
                    running: false,
                    account_id: Some(config.account_id),
                    target_pid: Some(config.target_pid),
                    last_error: result.as_ref().err().cloned(),
                    captured_frames: previous.captured_frames,
                    audio_peak: previous.audio_peak,
                    decoded_packets: previous.decoded_packets,
                    rune_events: previous.rune_events,
                    item_events: previous.item_events,
                    scene_heartbeats: previous.scene_heartbeats,
                    last_marker: previous.last_marker,
                    last_confidence: previous.last_confidence,
                    last_detected_at: previous.last_detected_at,
                    diagnostic_recording: previous.diagnostic_recording,
                    diagnostic_recording_path: previous.diagnostic_recording_path,
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
pub fn start_rune_audio_diagnostic_recording(app: tauri::AppHandle) -> Result<String, String> {
    if !RUNNING.load(Ordering::SeqCst) {
        return Err("请先启动音频声纹监控".to_string());
    }
    let config = resolve_monitor_config(&app)?;
    let mut guard = diagnostic_recording()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(recording) = guard.as_ref() {
        return Ok(recording.path.to_string_lossy().to_string());
    }
    let app_data_dir = app
        .state::<crate::state::SharedState>()
        .app_data_dir
        .clone();
    let directory = PathBuf::from(app_data_dir).join("audio-diagnostics");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("创建诊断录音目录失败 {}: {error}", directory.display()))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let path = directory.join(format!(
        "d2rhub-audio-{stamp}-pid{}-{suffix}.wav",
        config.target_pid
    ));
    let writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 1,
            sample_rate: CAPTURE_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|error| format!("创建诊断 WAV 失败 {}: {error}", path.display()))?;
    let path_text = path.to_string_lossy().to_string();
    *guard = Some(DiagnosticRecording {
        path,
        started_at: chrono::Local::now().to_rfc3339(),
        account_id: config.account_id,
        target_pid: config.target_pid,
        writer,
        write_error: None,
        detections: Vec::new(),
    });
    drop(guard);
    let mut current = status().lock().unwrap_or_else(|error| error.into_inner());
    current.diagnostic_recording = true;
    current.diagnostic_recording_path = Some(path_text.clone());
    Ok(path_text)
}

#[tauri::command]
pub fn stop_rune_audio_diagnostic_recording() -> Result<Option<String>, String> {
    finish_diagnostic_recording()
}

#[tauri::command]
pub fn get_rune_audio_status() -> RuneAudioStatus {
    status()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_monitor_config(min_rune_number: u32) -> MonitorConfig {
        MonitorConfig {
            account_id: "account-1".to_string(),
            character_name: "test".to_string(),
            target_pid: 42,
            threshold: 0.56,
            tracked_categories: HashSet::from([CATEGORY_RUNES.to_string(), "gems".to_string()]),
            min_rune_number,
            min_gem_level: 1,
            tracked_charm_codes: HashSet::from([
                "cm1".to_string(),
                "cm2".to_string(),
                "cm3".to_string(),
            ]),
            catalog_directory: None,
        }
    }

    #[test]
    fn drop_filters_require_wilderness_and_apply_the_inclusive_rune_minimum() {
        let config = test_monitor_config(20);
        let mut tracker = SegmentTracker::new(
            "account-1".to_string(),
            "test".to_string(),
            CAPTURE_SAMPLE_RATE,
        );

        assert!(!should_record_rune(&config, &tracker, 20));
        let gem = ItemCatalog::builtin().resolve(1).unwrap().clone();
        assert!(!should_record_item(&config, &tracker, &gem));
        tracker
            .observe_location(
                TelemetryMarker::Area { area_id: 1 },
                100,
                100,
                "town".to_string(),
            )
            .unwrap();
        assert!(!should_record_rune(&config, &tracker, 24));
        assert!(!should_record_item(&config, &tracker, &gem));

        tracker
            .observe_location(
                TelemetryMarker::Area { area_id: 6 },
                200,
                200,
                "wild".to_string(),
            )
            .unwrap();
        assert!(!should_record_rune(&config, &tracker, 19));
        assert!(should_record_rune(&config, &tracker, 20));
        assert!(should_record_rune(&config, &tracker, 24));
        assert!(should_record_item(&config, &tracker, &gem));
        let key = ItemCatalog::builtin().resolve(40).unwrap().clone();
        assert!(!should_record_item(&config, &tracker, &key));
    }

    #[test]
    fn detailed_gem_and_charm_filters_are_applied_after_scene_validation() {
        let mut config = test_monitor_config(1);
        config
            .tracked_categories
            .insert(CATEGORY_CHARMS.to_string());
        config.min_gem_level = 4;
        config.tracked_charm_codes = HashSet::from(["cm1".to_string(), "cm3".to_string()]);
        let mut tracker = SegmentTracker::new(
            "account-1".to_string(),
            "test".to_string(),
            CAPTURE_SAMPLE_RATE,
        );
        tracker
            .observe_location(
                TelemetryMarker::Area { area_id: 6 },
                200,
                200,
                "wild".to_string(),
            )
            .unwrap();
        let catalog = ItemCatalog::builtin();
        assert!(!should_record_item(
            &config,
            &tracker,
            catalog.resolve(1).unwrap()
        ));
        assert!(should_record_item(
            &config,
            &tracker,
            catalog.resolve(4).unwrap()
        ));
        assert!(should_record_item(
            &config,
            &tracker,
            catalog.resolve(36).unwrap()
        ));
        assert!(!should_record_item(
            &config,
            &tracker,
            catalog.resolve(37).unwrap()
        ));
        assert!(should_record_item(
            &config,
            &tracker,
            catalog.resolve(38).unwrap()
        ));
    }

    #[test]
    fn resolves_active_mod_name_without_accepting_paths() {
        assert_eq!(
            active_mod_name(r#"-mod "jcy-AudioTelemetry" -txt"#).unwrap(),
            Some("jcy-AudioTelemetry".to_string())
        );
        assert_eq!(
            active_mod_name("-MOD=AudioTelemetry -txt").unwrap(),
            Some("AudioTelemetry".to_string())
        );
        assert_eq!(active_mod_name("-txt").unwrap(), None);
        assert!(active_mod_name(r#"-mod "..\\outside" -txt"#).is_err());
    }

    #[test]
    fn terror_zone_probe_rearms_only_after_a_real_absence() {
        let mut gate = TerrorZonePresenceGate::new(CAPTURE_SAMPLE_RATE);
        assert!(gate.observe(10_000));
        assert!(!gate.observe(10_000 + CAPTURE_SAMPLE_RATE as u64));
        assert!(!gate.observe(10_000 + CAPTURE_SAMPLE_RATE as u64 * 3));
        assert!(gate.observe(10_000 + CAPTURE_SAMPLE_RATE as u64 * 8));
    }

    #[test]
    fn diagnostic_recording_writes_wav_and_detection_sidecar() {
        let root =
            std::env::temp_dir().join(format!("d2rhub-audio-recording-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sample.wav");
        let writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: CAPTURE_SAMPLE_RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        *diagnostic_recording()
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(DiagnosticRecording {
            path: path.clone(),
            started_at: "2026-08-26T00:00:00+08:00".to_string(),
            account_id: "test-account".to_string(),
            target_pid: 42,
            writer,
            write_error: None,
            detections: Vec::new(),
        });
        write_diagnostic_samples(&[0.0, 0.25, -0.25]);
        append_diagnostic_detection(TelemetryMarker::Rune { rune_number: 24 }, 0.91, 1_234, true);
        assert_eq!(
            finish_diagnostic_recording().unwrap(),
            Some(path.to_string_lossy().to_string())
        );

        let mut reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().sample_format, hound::SampleFormat::Int);
        assert_eq!(reader.samples::<i16>().count(), 3);
        let sidecar: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path.with_extension("events.json")).unwrap())
                .unwrap();
        assert_eq!(sidecar["detections"].as_array().unwrap().len(), 1);
        assert_eq!(sidecar["detections"][0]["start_frame"], 1_234);
        assert_eq!(sidecar["detections"][0]["logical_event"], true);
        std::fs::remove_dir_all(root).unwrap();
    }
}
