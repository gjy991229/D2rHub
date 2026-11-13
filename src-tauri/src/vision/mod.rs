//! Shared, bounded visual evidence worker. Audio never waits for image inference.
#[cfg(test)]
mod area;
#[cfg(test)]
mod area_names;
#[cfg(test)]
#[allow(dead_code)]
pub mod audio;
pub(crate) mod capture;
mod chat_ocr;
pub mod commands;
pub mod config;
pub(crate) mod diagnostics;
mod ocr;
pub(crate) mod processing;
mod resources;
mod signal_history;

use crate::application::capability::{
    CapabilityDriver, CapabilityFailure, CapabilityHealth, CapabilityId,
};
use crate::state::SharedState;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
#[cfg(test)]
use std::sync::mpsc;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::Manager;
pub const ID: CapabilityId = CapabilityId::new("visual-recognition");
static CONFIG: OnceLock<RwLock<config::VisionConfig>> = OnceLock::new();
static CONFIG_EPOCH: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
pub fn config_epoch() -> u64 {
    CONFIG_EPOCH.load(Ordering::Acquire)
}
pub fn configuration() -> config::VisionConfig {
    CONFIG
        .get_or_init(|| RwLock::new(config::VisionConfig::default()))
        .read()
        .clone()
}
fn effective_configuration(app: &tauri::AppHandle) -> config::VisionConfig {
    use crate::domain::room_automation::CollaborationMode;
    let mut config = configuration();
    let (enabled, mode) = app
        .try_state::<crate::capabilities::room_automation_runtime::RoomAutomationCommandState>()
        .and_then(|slot| {
            slot.manager().ok().map(|m| {
                let room = m.get_config().config;
                (room.enabled, room.collaboration_mode)
            })
        })
        .unwrap_or_default();
    config.chat_send = enabled && mode == CollaborationMode::Host;
    config.chat_follow = enabled && mode == CollaborationMode::Follower;
    config
}

pub(crate) fn refresh_room_mode(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(service) = app.try_state::<Arc<Vision>>() {
        let _operation = service.operation.lock();
        let follow = effective_configuration(app).chat_follow;
        let restart = service
            .worker
            .lock()
            .as_ref()
            .is_some_and(|w| w.chat_follow != follow);
        if restart {
            service.stop_worker()?;
        }
        project(app);
    }
    Ok(())
}

#[derive(Clone, Serialize, Default)]
pub struct Status {
    pub running: bool,
    pub ready: bool,
    pub message: String,
    pub checked: u64,
    pub matched: u64,
    pub rejected: u64,
    pub unavailable: u64,
}
#[cfg(test)]
#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct Candidate {
    pub audio_mapped: bool,
    pub name: String,
    pub name_en: String,
    pub aliases: Vec<String>,
    pub location: Option<crate::rune_audio::catalog::NamedLocation>,
    pub tz: bool,
    pub confidence: f32,
    pub announcement: Option<u64>,
}
#[cfg(test)]
pub enum Kind {
    Area(Vec<Candidate>, Option<Candidate>),
}
#[cfg(test)]
#[allow(dead_code)]
pub struct Request {
    pub pid: u32,
    pub kind: Kind,
    pub reply: mpsc::SyncSender<Result<Option<Candidate>, String>>,
    pub created: Instant,
}
struct Worker {
    chat_follow: bool,
    stop: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<()>,
    #[cfg(test)]
    sender: mpsc::SyncSender<Request>,
}
pub struct Vision {
    app: tauri::AppHandle,
    pub store: crate::infrastructure::module_config::ModuleConfigStore,
    worker: Mutex<Option<Worker>>,
    pub status: Arc<Mutex<Status>>,
    pub operation: Mutex<()>,
}
impl Vision {
    pub fn install(app: &tauri::AppHandle) -> Result<Arc<Self>, String> {
        let store = config::store(&app.state::<SharedState>().app_data_dir)?;
        let saved = config::load(&store)?;
        saved.config.validate()?;
        *CONFIG
            .get_or_init(|| RwLock::new(config::VisionConfig::default()))
            .write() = saved.config;
        let service = Arc::new(Self {
            app: app.clone(),
            store,
            worker: Mutex::new(None),
            status: Arc::new(Mutex::new(Status::default())),
            operation: Mutex::new(()),
        });
        app.manage(service.clone());
        Ok(service)
    }
    #[cfg(test)]
    pub fn submit(
        &self,
        pid: u32,
        kind: Kind,
    ) -> Result<mpsc::Receiver<Result<Option<Candidate>, String>>, String> {
        if !self.status.lock().ready {
            return Err("视觉资源尚未就绪".into());
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = self.worker.lock();
        let worker = worker.as_ref().ok_or("视觉服务未启用")?;
        worker
            .sender
            .try_send(Request {
                pid,
                kind,
                reply: tx,
                created: Instant::now(),
            })
            .map_err(|_| "视觉核验队列已满".to_string())?;
        Ok(rx)
    }
    pub fn stop_worker(&self) -> Result<(), String> {
        if let Some(worker) = self.worker.lock().take() {
            worker.stop.store(true, Ordering::Release);
            worker.handle.join().map_err(|_| "视觉线程异常退出")?;
        }
        capture::release_sessions();
        let mut s = self.status.lock();
        s.running = false;
        s.ready = false;
        Ok(())
    }
}
pub fn project(app: &tauri::AppHandle) {
    let global = app.state::<SharedState>().configuration().snapshot();
    if let Some(global) = global {
        project_configuration(app, &global);
    }
}
/// Called inside the global configuration transaction: never re-enter its lock.
pub fn project_configuration(app: &tauri::AppHandle, g: &crate::domain::config::GlobalConfig) {
    let state = app.state::<SharedState>();
    let c = effective_configuration(app);
    let requested = state.optional_runtime_ready() && {
        let room = g.optional_module_runtime_allowed(
            crate::domain::config::OPTIONAL_MODULE_ROOM_AUTOMATION,
        ) && app
            .try_state::<crate::capabilities::room_automation_runtime::RoomAutomationCommandState>()
            .is_some_and(|s| s.manager().is_ok_and(|m| m.requested_enabled()))
            && c.chat_follow;
        room
    };
    let _ = state.capabilities().set_requested(ID, requested);
}
impl CapabilityDriver for Vision {
    fn start(&self) -> Result<(), CapabilityFailure> {
        let _operation = self.operation.lock();
        let mut slot = self.worker.lock();
        if slot.is_some() {
            return Ok(());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let status = self.status.clone();
        let app = self.app.clone();
        let config = effective_configuration(&app);
        let chat_follow = config.chat_follow;
        let root = resources::resource_root(&app.state::<SharedState>().app_data_dir);
        #[cfg(test)]
        let (tx, _rx) = mpsc::sync_channel(8);
        let handle=std::thread::Builder::new().name("visual-recognition".into()).spawn(move||{
            status.lock().running=true;
            let preparation=Instant::now();
            diagnostics::event(configuration().diagnostic_enabled,"worker_start",serde_json::json!({"chat":config.chat_follow,"area":config.area_enabled,"poll_ms":config.poll_ms,"chat_roi":config.chat_roi,"area_roi":config.area_roi,"needs_ocr":config.needs_ocr()}));
            let result=if config.needs_ocr() { resources::installed(&root).and_then(|files|ocr::Ocr::new(&files)).map(Some) } else { Ok(None) };
            match result {
                Ok(mut ocr)=>{
                    diagnostics::event(configuration().diagnostic_enabled,"worker_ready",serde_json::json!({"preparation_ms":preparation.elapsed().as_secs_f64()*1000.0,"ocr_loaded":ocr.is_some()}));
                    status.lock().ready=ocr.is_some();
                    run(&app,&config,&stopped,&mut ocr,&status);
                },
                Err(error)=>{diagnostics::event(configuration().diagnostic_enabled,"resource_or_model_error",serde_json::json!({"error":diagnostics::short(&error),"preparation_ms":preparation.elapsed().as_secs_f64()*1000.0}));status.lock().message=error;},
            }
            diagnostics::event(configuration().diagnostic_enabled,"worker_stop",serde_json::json!({}));
            let mut s=status.lock();s.ready=false;s.running=false;
        }).map_err(|e|CapabilityFailure::new("vision-start",e.to_string()))?;
        *slot = Some(Worker {
            chat_follow,
            stop,
            handle,
            #[cfg(test)]
            sender: tx,
        });
        Ok(())
    }
    fn stop(&self) -> Result<(), CapabilityFailure> {
        let _operation = self.operation.lock();
        self.stop_worker()
            .map_err(|e| CapabilityFailure::new("vision-stop", e))
    }
    fn health(&self) -> CapabilityHealth {
        let worker = self.worker.lock();
        if worker.is_none() {
            return CapabilityHealth::Failed(CapabilityFailure::new(
                "vision-reconfigure",
                "视觉配置已更新，正在重新准备资源",
            ));
        }
        if worker.as_ref().is_some_and(|w| w.handle.is_finished()) {
            return CapabilityHealth::Degraded(CapabilityFailure::new(
                "vision-unavailable",
                self.status.lock().message.clone(),
            ));
        }
        // Resource failure is visible but not retried every five seconds. Save/retry explicitly.
        CapabilityHealth::Healthy
    }
}
fn run(
    app: &tauri::AppHandle,
    c: &config::VisionConfig,
    stop: &AtomicBool,
    ocr: &mut Option<ocr::Ocr>,
    status: &Mutex<Status>,
) {
    let mut chat = ChatState::default();
    let mut next_chat = Instant::now();
    let mut last_chat_error: Option<(String, Instant)> = None;
    while !stop.load(Ordering::Acquire) {
        if c.chat_follow
            && (Instant::now() >= next_chat || chat.pending.is_some())
            && !stop.load(Ordering::Acquire)
        {
            next_chat = Instant::now() + Duration::from_millis(c.poll_ms);
            match chat.tick(app, c, stop, ocr.as_mut()) {
                Err(e) => {
                    if last_chat_error.as_ref().is_none_or(|(previous, time)| {
                        previous != &e || time.elapsed() >= Duration::from_secs(10)
                    }) {
                        diagnostics::event(
                            configuration().diagnostic_enabled,
                            "chat_error",
                            serde_json::json!({"error":diagnostics::short(&e)}),
                        );
                        last_chat_error = Some((e.clone(), Instant::now()));
                    }
                    status.lock().message = e;
                }
                Ok(()) => {
                    status.lock().message = if chat.pending.is_some() {
                        "已检测到聊天信号，等待按本机规则跟房".into()
                    } else {
                        "聊天信号监听中（先检测特征，再读取轮次后缀）".into()
                    };
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[cfg(test)]
#[allow(dead_code)]
fn evaluate_area(
    r: &Request,
    c: &config::VisionConfig,
    ocr: Option<&mut ocr::Ocr>,
    gate: &mut area::Gate,
    trace: &mut diagnostics::Trace,
) -> Result<Option<Candidate>, String> {
    let Kind::Area(candidates, current) = &r.kind;
    let captured = trace.measure_result("capture", || capture::capture(r.pid, Some(c.area_roi)))?;
    trace.fields["capture_metrics"] = serde_json::json!(captured.metrics);
    let feature = trace.measure("red_text_gate", || area::feature(&captured.image));
    trace.fields["feature"] = serde_json::json!(feature);
    trace.fields["ocr_calls"] = serde_json::json!(0);
    trace.fields["strategy"] = serde_json::json!("red_entry_rgb_1s");
    let Some(line) = area::crop(&captured.image, &feature) else {
        gate.absent();
        trace.fields["ocr_skip"] = serde_json::json!("no_red_text");
        let Some(mut current) = current.clone().filter(|v| !v.tz) else {
            return Ok(None);
        };
        let Some(tz) = area::tz_crop(&captured.image, &feature) else {
            return Ok(None);
        };
        let (text, _) = ocr.ok_or("OCR 资源尚未就绪")?.recognize(&tz)?;
        if !area::terror_text(&text) {
            return Ok(None);
        }
        current.tz = true;
        return Ok(Some(current));
    };
    let tz = area::tz_crop(&captured.image, &feature);
    trace.fields["tz_candidate"] = serde_json::json!(tz.is_some());
    let ocr = ocr.ok_or("OCR 资源尚未就绪")?;
    let result = gate.read(&line, Instant::now(), candidates, tz.as_ref(), |line| {
        trace.fields["ocr_calls"] =
            serde_json::json!(trace.fields["ocr_calls"].as_u64().unwrap_or(0) + 1);
        let result = trace.measure_result("ocr", || ocr.recognize(line))?;
        trace.sample(&result.0, result.1, false);
        Ok(result)
    });
    if let Some(skip) = gate.last_skip {
        trace.fields["ocr_skip"] = serde_json::json!(skip);
    }
    if let Some(reason) = gate.last_rejection {
        trace.fields["name_rejection"] = serde_json::json!(reason);
    }
    if matches!(result, Ok(None)) {
        if let (Some(mut current), Some(tz)) = (current.clone().filter(|v| !v.tz), tz.as_ref()) {
            let (text, _) = ocr.recognize(tz)?;
            if area::terror_text(&text) {
                current.tz = true;
                return Ok(Some(current));
            }
        }
    }
    result
}

#[derive(Default)]
struct ChatState {
    history: Option<signal_history::SignalHistory>,
    ocr_gate: chat_ocr::ChatOcr,
    pending_sequence: Option<u32>,
    target: Option<(String, u32)>,
    pending: Option<Instant>,
    next_attempt: Option<Instant>,
}
impl ChatState {
    fn bind_target(&mut self, account: &str, pid: u32) {
        let target = (account.to_string(), pid);
        if self.target.as_ref() != Some(&target) {
            *self = Self {
                target: Some(target),
                ..Self::default()
            };
        }
    }

    fn bind_history(&mut self, root: &str, scope: String, epoch: u64) -> Result<(), String> {
        if self
            .history
            .as_ref()
            .is_none_or(|h| h.scope != scope || h.epoch != epoch)
        {
            self.pending = None;
            self.pending_sequence = None;
            self.next_attempt = None;
            self.ocr_gate = chat_ocr::ChatOcr::default();
            self.history = Some(signal_history::SignalHistory::open(root, scope, epoch)?);
        }
        Ok(())
    }

    fn tick(
        &mut self,
        app: &tauri::AppHandle,
        c: &config::VisionConfig,
        stop: &AtomicBool,
        ocr: Option<&mut ocr::Ocr>,
    ) -> Result<(), String> {
        use crate::capabilities::room_automation_runtime::RoomAutomationCommandState;
        let slot = app
            .try_state::<RoomAutomationCommandState>()
            .ok_or("自动跟房未加载")?;
        let manager = slot.manager()?;
        let room = manager.get_config().config;
        if !room.enabled {
            *self = Self::default();
            return Err("聊天监听需要启用自动跟房模块".into());
        }
        let Some(account) =
            (!room.primary_account_id.is_empty()).then_some(&room.primary_account_id)
        else {
            *self = Self::default();
            return Err("拼房跟随需要先配置主号".into());
        };
        let state = app.state::<SharedState>();
        let Some(instance) = state.multi_instance().facade().instance(account) else {
            *self = Self::default();
            return Err("负责聊天识别的主号没有运行中的游戏实例".into());
        };
        self.bind_target(account, instance.pid);
        let scope = signal_history::scope(&room.name_prefix, &room.password, room.sequence_width);
        self.bind_history(&state.app_data_dir, scope, room.chat_signal_epoch)?;
        if let Some(due) = self.pending {
            if self.pending_sequence != Some(room.next_sequence) {
                self.pending = None;
                self.pending_sequence = None;
                return Ok(());
            }
            if due.elapsed() > Duration::from_secs(20) {
                self.pending = None;
                return Err("聊天跟房等待超时，请检查游戏窗口及本机规则".into());
            }
            if Instant::now() >= due
                && self.next_attempt.is_none_or(|time| Instant::now() >= time)
                && !stop.load(Ordering::Acquire)
            {
                self.next_attempt = Some(Instant::now() + Duration::from_secs(1));
                let result = manager.start_followers_for_chat(Some(room.chat_signal_epoch));
                diagnostics::event(
                    configuration().diagnostic_enabled,
                    "chat_follow_dispatch",
                    serde_json::json!({"ok":result.is_ok(),"late_ms":due.elapsed().as_secs_f64()*1000.0,"error":result.as_ref().err().map(|e|diagnostics::short(e))}),
                );
                result?;
                self.pending = None;
            }
            return Ok(());
        }
        let mut trace =
            diagnostics::Trace::new(configuration().diagnostic_enabled, "chat", instance.pid);
        let captured = trace.measure_result("capture", || {
            capture::capture(instance.pid, Some(c.chat_roi))
        })?;
        trace.fields["frame_width"] = serde_json::json!(captured.metrics.client_width);
        trace.fields["frame_height"] = serde_json::json!(captured.metrics.client_height);
        let capture_metrics = serde_json::json!(captured.metrics);
        let image = captured.image;
        let lines = trace.measure("segmentation", || processing::chat_lines(&image));
        let history = self.history.as_mut().ok_or("聊天信号去重状态未加载")?;
        let ocr = ocr.ok_or("聊天后缀 OCR 资源尚未就绪")?;
        let tokens = self.ocr_gate.scan_new(
            &lines,
            &[],
            Instant::now(),
            if history.initialized {
                history.watermark
            } else {
                u32::MAX
            },
            |line| ocr.recognize(line),
            &mut trace,
        )?;
        let present = trace.fields["present"].as_bool().unwrap_or(false);
        let reports = trace.fields["line_analysis"].clone();
        let baseline = !history.initialized;
        // An unreadable visible marker must not arm the initial baseline: a later
        // successful retry could otherwise dispatch an old message.
        let token = if !history.initialized
            && tokens.is_empty()
            && (present || trace.fields["ocr_skip"] == "cooldown")
        {
            None
        } else {
            trace.measure_result("dedup_persist", || history.observe(tokens))?
        };
        let trigger = token.is_some();
        let ocr_diagnostics = trace.fields.clone();
        trace.fields = serde_json::json!({"outcome":"checked","capture_metrics":capture_metrics,"frame_width":trace.fields["frame_width"],"frame_height":trace.fields["frame_height"],"roi":c.chat_roi,"candidate_lines":lines.len(),"line_analysis":reports,"present":present,"local_sequence":room.next_sequence,"dedup_token":token,"dedup_watermark":history.watermark,"baseline":baseline,"trigger":trigger,"ocr":ocr_diagnostics});
        if trigger {
            self.pending_sequence = Some(room.next_sequence);
            self.pending =
                Some(Instant::now() + Duration::from_secs_f64(room.auto_followers_delay_secs));
            self.next_attempt = None;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_chat_epoch_cancels_pending_and_reestablishes_baseline() {
        let root = std::env::temp_dir().join(format!("chat-reset-{}", uuid::Uuid::new_v4()));
        let mut state = ChatState::default();
        state
            .bind_history(root.to_str().unwrap(), "rule".into(), 0)
            .unwrap();
        state.history.as_mut().unwrap().observe([900]).unwrap();
        state.pending = Some(Instant::now());
        state.pending_sequence = Some(5);
        state.next_attempt = Some(Instant::now());
        state
            .bind_history(root.to_str().unwrap(), "rule".into(), 0)
            .unwrap();
        assert!(state.pending.is_some());
        state
            .bind_history(root.to_str().unwrap(), "rule".into(), 1)
            .unwrap();
        assert!(state.pending.is_none());
        assert!(state.pending_sequence.is_none());
        assert!(state.next_attempt.is_none());
        let history = state.history.as_mut().unwrap();
        assert!(!history.initialized);
        assert_eq!(history.watermark, 0);
        assert_eq!(history.observe([1]).unwrap(), None);
        assert_eq!(history.observe([2]).unwrap(), Some(2));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn follower_or_process_change_discards_previous_pending_signal() {
        let mut state = ChatState::default();
        state.bind_target("first", 10);
        state.pending = Some(Instant::now());
        state.bind_target("first", 10);
        assert!(state.pending.is_some());
        state.bind_target("next", 20);
        assert!(state.pending.is_none());
        assert!(state.history.is_none());
        state.pending = Some(Instant::now());
        state.bind_target("next", 21);
        assert!(state.pending.is_none());
    }
}
