pub mod capturer;
pub mod engine;
pub mod fuzzy;
pub mod game_data;
pub mod pipeline;
pub mod preprocess;

use crate::commands::account::AccountMeta;
use crate::commands::global_config::GlobalConfig;
use pipeline::OcrMonitor;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::Manager;

const OCR_POLL_INTERVAL_MIN_MS: u64 = 200;
const OCR_POLL_INTERVAL_MAX_MS: u64 = 2_000;
const OCR_ADAPTIVE_INTERVAL_MULTIPLIER: u32 = 4;
const OCR_WATCHDOG_MIN_TIMEOUT_MS: u64 = 5_000;
const OCR_WATCHDOG_INTERVAL_MULTIPLIER: u64 = 5;

fn normalize_poll_interval_ms(value: u64) -> u64 {
    value.clamp(OCR_POLL_INTERVAL_MIN_MS, OCR_POLL_INTERVAL_MAX_MS)
}

fn watchdog_timeout_ms(poll_interval_ms: u64) -> u64 {
    std::cmp::max(
        OCR_WATCHDOG_MIN_TIMEOUT_MS,
        poll_interval_ms.saturating_mul(OCR_WATCHDOG_INTERVAL_MULTIPLIER),
    )
}

#[derive(Debug, Clone)]
pub struct OcrConfig {
    pub window_title: String,
    /// 目标游戏进程 PID（优先于 window_title，用于精确定位窗口）
    pub target_pid: Option<u32>,
    pub poll_interval_ms: u64,
    pub debug_output: bool,
    pub text_matcher_threshold: u8,
    pub rune_matcher_threshold: u8,
    pub scene_text_color_rgb: [u8; 3],
    pub scene_text_color_range: [u8; 3],
    pub rune_text_color_rgb: [u8; 3],
    pub rune_text_color_range: [u8; 3],
    pub rune_background_color_rgb: [u8; 3],
    pub rune_background_color_range: [u8; 3],
}

impl OcrConfig {
    fn from_account(
        global_config: &GlobalConfig,
        account: &AccountMeta,
        target_pid: Option<u32>,
    ) -> Self {
        let window_title = if account.display_name.trim().is_empty() {
            account.id.clone()
        } else {
            account.display_name.clone()
        };

        Self {
            window_title,
            target_pid,
            poll_interval_ms: global_config.ocr_poll_interval_ms,
            debug_output: global_config.ocr_debug_output,
            text_matcher_threshold: default_text_matcher_threshold(),
            rune_matcher_threshold: default_rune_matcher_threshold(),
            scene_text_color_rgb: default_scene_text_color_rgb(),
            scene_text_color_range: default_scene_text_color_range(),
            rune_text_color_rgb: default_rune_text_color_rgb(),
            rune_text_color_range: default_rune_text_color_range(),
            rune_background_color_rgb: default_rune_background_color_rgb(),
            rune_background_color_range: default_rune_background_color_range(),
        }
    }
}

fn default_text_matcher_threshold() -> u8 {
    67
}
fn default_rune_matcher_threshold() -> u8 {
    67
}
fn default_scene_text_color_rgb() -> [u8; 3] {
    [202, 23, 0]
}
fn default_scene_text_color_range() -> [u8; 3] {
    [10, 55, 55]
}
fn default_rune_text_color_rgb() -> [u8; 3] {
    [255, 168, 0]
}
fn default_rune_text_color_range() -> [u8; 3] {
    [10, 100, 75]
}
fn default_rune_background_color_rgb() -> [u8; 3] {
    [0, 71, 141]
}
fn default_rune_background_color_range() -> [u8; 3] {
    [10, 55, 55]
}

/// 单次 OCR 文本结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrTextItem {
    pub text: String,
    pub source: String,
    pub timestamp: String,
    /// 符文编号（1-33），仅通道B 识别到符文时填充
    pub rune_number: Option<u32>,
    /// 高级符文截图相对路径（相对于 stateData 目录），仅 #24+ 符文填充
    pub screenshot_path: Option<String>,
    #[serde(default)]
    pub is_town: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rune_name_en: Option<String>,
}

/// 通道结果环形缓冲区
static CH_A_RESULTS: std::sync::OnceLock<Mutex<Vec<OcrTextItem>>> = std::sync::OnceLock::new();
static CH_B_RESULTS: std::sync::OnceLock<Mutex<Vec<OcrTextItem>>> = std::sync::OnceLock::new();
static RUNNING: AtomicBool = AtomicBool::new(false);
static WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn resolve_monitor_config(app: &tauri::AppHandle) -> Result<OcrConfig, String> {
    let state = app.state::<crate::state::SharedState>();
    let (global_config, account) = {
        let config = state.config.read();
        let global_config = config
            .as_ref()
            .ok_or_else(|| "尚未完成首次配置".to_string())?;
        let account = global_config
            .resolve_ocr_target_account()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OCR 尚未启用".to_string())?;
        (global_config.clone(), account)
    };
    let target_pid = state
        .active_games
        .read()
        .get(&account.id)
        .copied()
        .or(account.running_pid);

    Ok(OcrConfig::from_account(
        &global_config,
        &account,
        target_pid,
    ))
}

fn mark_generation_stopped(generation: u64) {
    if GENERATION.load(Ordering::SeqCst) == generation {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

struct ActiveOcrWorkerGuard {
    generation: u64,
}

impl Drop for ActiveOcrWorkerGuard {
    fn drop(&mut self) {
        mark_generation_stopped(self.generation);
        WORKER_ACTIVE.store(false, Ordering::SeqCst);
    }
}

struct ComApartmentGuard;

impl ComApartmentGuard {
    fn initialize_mta() -> Result<Self, String> {
        unsafe {
            let result = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            );
            result
                .ok()
                .map_err(|error| format!("初始化 OCR worker COM MTA 失败: {error}"))?;
        }
        Ok(Self)
    }
}

impl Drop for ComApartmentGuard {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}

struct OcrEngineGuard;

impl OcrEngineGuard {
    fn initialize(
        app_data_dir: &str,
        resource_dir: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        engine::init_engine(app_data_dir, resource_dir)?;
        Ok(Self)
    }
}

impl Drop for OcrEngineGuard {
    fn drop(&mut self) {
        engine::release_engine();
    }
}

fn push_result(buf: &std::sync::OnceLock<Mutex<Vec<OcrTextItem>>>, item: OcrTextItem) {
    if let Some(lock) = buf.get() {
        let mut v = lock.lock().unwrap_or_else(|e| e.into_inner());
        v.push(item);
        if v.len() > 200 {
            let drop = v.len() - 200;
            v.drain(0..drop);
        }
    }
}

/// 获取通道A 最新结果并清空缓冲区
#[tauri::command]
pub fn get_ocr_ch_a_results() -> Vec<OcrTextItem> {
    let lock = CH_A_RESULTS.get_or_init(|| Mutex::new(Vec::new()));
    let mut buf = lock.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *buf)
}

/// 获取通道B 最新结果并清空缓冲区
#[tauri::command]
pub fn get_ocr_ch_b_results() -> Vec<OcrTextItem> {
    let lock = CH_B_RESULTS.get_or_init(|| Mutex::new(Vec::new()));
    let mut buf = lock.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *buf)
}

/// 启动 OCR 轮询 (2Hz)
#[tauri::command]
pub async fn start_ocr_monitor(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start_ocr_monitor_blocking(app))
        .await
        .map_err(|error| format!("等待 OCR worker 初始化失败: {error}"))?
}

fn start_ocr_monitor_blocking(app: tauri::AppHandle) -> Result<(), String> {
    let config = resolve_monitor_config(&app)?;
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("OCR 监控器已在运行中".to_string());
    }

    if WORKER_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        RUNNING.store(false, Ordering::SeqCst);
        return Err(
            "上一个 OCR 工作线程仍在退出，请稍后重试，避免重复占用捕获/GPU 资源".to_string(),
        );
    }

    // 从 AppState 获取 app_data_dir（用于所有 debug 输出，避免依赖 exe 路径）
    let app_data_dir = {
        let state = app.state::<crate::state::SharedState>();
        state.app_data_dir.clone()
    };
    let debug_out_dir = std::path::Path::new(&app_data_dir).join("test");

    // 清理上一次的调试输出（在写新日志之前）
    if config.debug_output {
        let _ = std::fs::remove_dir_all(&debug_out_dir);
        if let Err(e) = std::fs::create_dir_all(&debug_out_dir) {
            eprintln!(
                "[OCR Debug] 创建调试输出目录失败: {} ({})",
                debug_out_dir.display(),
                e
            );
        }
    }

    // DO NOT call engine::init_engine() here!
    // If we call it here, OcrEngine is created on the Tauri Main Thread (STA),
    // which causes costly cross-apartment marshaling and deadlocks when accessed from the MTA worker thread.

    CH_A_RESULTS.get_or_init(|| Mutex::new(Vec::new()));
    CH_B_RESULTS.get_or_init(|| Mutex::new(Vec::new()));

    let configured_poll_ms = config.poll_interval_ms;
    let poll_ms = normalize_poll_interval_ms(configured_poll_ms);
    if poll_ms != configured_poll_ms {
        crate::logger::log_msg(
            "WARN",
            "OCR",
            &format!(
                "OCR 轮询间隔 {}ms 超出安全范围，已钳制为 {}ms",
                configured_poll_ms, poll_ms
            ),
        );
    }
    let is_debug = config.debug_output;

    let my_gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    std::thread::Builder::new()
        .name("ocr-worker".into())
        .spawn(move || {
            let active_guard = ActiveOcrWorkerGuard { generation: my_gen };

            let com_guard = match ComApartmentGuard::initialize_mta() {
                Ok(guard) => guard,
                Err(error) => {
                    crate::logger::log_msg("ERROR", "OCR", &error);
                    drop(active_guard);
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };

            let resource_dir = app.path().resource_dir().ok();
            let engine_guard =
                match OcrEngineGuard::initialize(&app_data_dir, resource_dir.as_deref()) {
                    Ok(guard) => guard,
                    Err(error) => {
                        crate::logger::log_msg("ERROR", "OCR", &error);
                        drop(com_guard);
                        drop(active_guard);
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };

            // Capturer may create WinRT/xcap resources, so construct it only after
            // the worker owns an initialized MTA apartment.
            let mut monitor = match OcrMonitor::new(config, app_data_dir.clone()) {
                Ok(monitor) => monitor,
                Err(error) => {
                    crate::logger::log_msg(
                        "ERROR",
                        "OCR",
                        &format!("OcrMonitor::new 失败: {error}"),
                    );
                    drop(engine_guard);
                    drop(com_guard);
                    drop(active_guard);
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };

            let interval = std::time::Duration::from_millis(poll_ms);
            if is_debug {
                eprintln!("[OCR Debug] 监控器线程已启动，轮询间隔: {}ms", poll_ms);
            }

            let (watchdog_tx, watchdog_rx) = std::sync::mpsc::channel();
            let timeout_ms = watchdog_timeout_ms(poll_ms);

            let watchdog_handle = match std::thread::Builder::new()
                .name("ocr-watchdog".into())
                .spawn(move || loop {
                    match watchdog_rx
                        .recv_timeout(std::time::Duration::from_millis(timeout_ms))
                    {
                        Ok(true) => { /* Heartbeat received */ }
                        Ok(false) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            eprintln!(
                                "[OCR Error] OCR poll timeout ({}ms). Worker thread may be deadlocked.",
                                timeout_ms
                            );
                            mark_generation_stopped(my_gen);
                            break;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                })
            {
                Ok(handle) => handle,
                Err(error) => {
                    let error = format!("创建 OCR watchdog 线程失败: {error}");
                    crate::logger::log_msg("ERROR", "OCR", &error);
                    drop(monitor);
                    drop(engine_guard);
                    drop(com_guard);
                    drop(active_guard);
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };

            if !RUNNING.load(Ordering::SeqCst) || GENERATION.load(Ordering::SeqCst) != my_gen {
                let _ = watchdog_tx.send(false);
                let _ = watchdog_handle.join();
                drop(monitor);
                drop(engine_guard);
                drop(com_guard);
                drop(active_guard);
                let _ = ready_tx.send(Err("OCR 启动在初始化期间被取消".to_string()));
                return;
            }

            let _ = ready_tx.send(Ok(()));

            let mut current_interval = interval;

            loop {
                if !RUNNING.load(Ordering::SeqCst) || GENERATION.load(Ordering::SeqCst) != my_gen {
                    break;
                }
                let start = std::time::Instant::now();

                if watchdog_tx.send(true).is_err() {
                    break;
                }

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    monitor.poll();
                }));

                if let Err(err) = result {
                    eprintln!("[OCR Error] poll() panicked: {:?}", err);
                    break;
                }
                if watchdog_tx.send(true).is_err() {
                    break;
                }

                let elapsed = start.elapsed();

                // Self-adaptive dynamic frequency scaling of the polling interval
                if elapsed > current_interval / 2 {
                    current_interval = std::cmp::min(
                        current_interval + std::time::Duration::from_millis(100),
                        interval * OCR_ADAPTIVE_INTERVAL_MULTIPLIER,
                    );
                } else {
                    current_interval = std::cmp::max(
                        current_interval.saturating_sub(std::time::Duration::from_millis(50)),
                        interval,
                    );
                }

                if elapsed < current_interval {
                    std::thread::sleep(current_interval - elapsed);
                }
            }

            let _ = watchdog_tx.send(false);
            if watchdog_handle.join().is_err() {
                crate::logger::log_msg("WARN", "OCR", "OCR watchdog 线程异常退出");
            }

            // Keep teardown explicit: capture resources first, then the OCR engine,
            // then COM, and only then publish WORKER_ACTIVE=false.
            drop(monitor);
            drop(engine_guard);
            drop(com_guard);
            drop(active_guard);
            eprintln!("[OCR Info] Worker thread exited");
        })
        .map_err(|e| {
            mark_generation_stopped(my_gen);
            WORKER_ACTIVE.store(false, Ordering::SeqCst);
            format!("创建工作线程失败: {}", e)
        })?;

    match ready_rx.recv() {
        Ok(result) => result,
        Err(_) => Err("OCR worker 在报告初始化结果前异常退出".to_string()),
    }
}

/// 停止 OCR 轮询
#[tauri::command]
pub fn stop_ocr_monitor() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    RUNNING.store(false, Ordering::SeqCst);
}

/// 使用已保存的全局配置原子地重启 OCR。
#[tauri::command]
pub async fn restart_ocr_monitor(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || restart_ocr_monitor_blocking(app))
        .await
        .map_err(|error| format!("等待 OCR worker 重启失败: {error}"))?
}

fn restart_ocr_monitor_blocking(app: tauri::AppHandle) -> Result<(), String> {
    stop_ocr_monitor();
    for _ in 0..60 {
        if !WORKER_ACTIVE.load(Ordering::SeqCst) {
            return start_ocr_monitor_blocking(app);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err("上一个 OCR 工作线程未能在 3 秒内退出；为避免重复占用捕获/GPU 资源，已取消重启".to_string())
}

#[cfg(test)]
mod lifecycle_tests {
    use super::{
        normalize_poll_interval_ms, watchdog_timeout_ms, OCR_ADAPTIVE_INTERVAL_MULTIPLIER,
        OCR_POLL_INTERVAL_MAX_MS, OCR_POLL_INTERVAL_MIN_MS,
    };

    #[test]
    fn poll_interval_is_clamped_to_the_supported_range() {
        assert_eq!(normalize_poll_interval_ms(0), OCR_POLL_INTERVAL_MIN_MS);
        assert_eq!(
            normalize_poll_interval_ms(OCR_POLL_INTERVAL_MIN_MS),
            OCR_POLL_INTERVAL_MIN_MS
        );
        assert_eq!(
            normalize_poll_interval_ms(OCR_POLL_INTERVAL_MAX_MS),
            OCR_POLL_INTERVAL_MAX_MS
        );
        assert_eq!(
            normalize_poll_interval_ms(u64::MAX),
            OCR_POLL_INTERVAL_MAX_MS
        );
    }

    #[test]
    fn watchdog_timeout_exceeds_the_longest_adaptive_sleep() {
        for configured in [0, 200, 500, 1_000, 2_000, u64::MAX] {
            let poll_ms = normalize_poll_interval_ms(configured);
            let longest_sleep_ms =
                poll_ms.saturating_mul(u64::from(OCR_ADAPTIVE_INTERVAL_MULTIPLIER));
            assert!(watchdog_timeout_ms(poll_ms) > longest_sleep_ms);
        }
    }
}
