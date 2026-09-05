use crate::application::capability::{CapabilityRegistry, CapabilityStatusSnapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use tauri::Emitter;

const STATUS_EVENT: &str = "capability-status-updated";
const HEALTH_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

enum SupervisorMessage {
    Reconcile,
    ReconcileAndReply(mpsc::SyncSender<Result<CapabilityStatusSnapshot, String>>),
    Shutdown,
}

/// Serializes lifecycle hooks away from configuration transactions and emits
/// immutable status snapshots after each reconciliation pass.
pub(crate) struct CapabilitySupervisor {
    sender: mpsc::SyncSender<SupervisorMessage>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    worker_done: Mutex<Option<mpsc::Receiver<()>>>,
    shutdown_requested: Arc<AtomicBool>,
}

impl CapabilitySupervisor {
    pub(crate) fn start(
        app: tauri::AppHandle,
        registry: Arc<CapabilityRegistry>,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let (worker_done_sender, worker_done_receiver) = mpsc::sync_channel(1);
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown_requested);
        let worker = std::thread::Builder::new()
            .name("capability-supervisor".to_string())
            .spawn(move || {
                loop {
                    if worker_shutdown.load(Ordering::Acquire) {
                        shutdown_and_publish(&app, &registry);
                        break;
                    }
                    let message = match receiver.recv_timeout(HEALTH_POLL_INTERVAL) {
                        Ok(message) => message,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            reconcile_if_changed_and_publish(&app, &registry);
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    match message {
                        SupervisorMessage::Reconcile => {
                            if worker_shutdown.load(Ordering::Acquire) {
                                shutdown_and_publish(&app, &registry);
                                break;
                            }
                            reconcile_and_publish(&app, &registry);
                        }
                        SupervisorMessage::ReconcileAndReply(reply) => {
                            let result = registry.reconcile_all().map_err(|error| error.to_string());
                            if let Ok(snapshot) = &result {
                                publish_snapshot(&app, snapshot);
                            }
                            let _ = reply.send(result);
                        }
                        SupervisorMessage::Shutdown => {
                            shutdown_and_publish(&app, &registry);
                            break;
                        }
                    }
                }

                let _ = worker_done_sender.send(());
            })
            .map_err(|error| format!("启动 capability supervisor 失败: {error}"))?;

        Ok(Self {
            sender,
            worker: Mutex::new(Some(worker)),
            worker_done: Mutex::new(Some(worker_done_receiver)),
            shutdown_requested,
        })
    }

    pub(crate) fn schedule_reconcile(&self) {
        match self.sender.try_send(SupervisorMessage::Reconcile) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => crate::logger::log_msg(
                "ERROR",
                "Capabilities",
                "capability supervisor 已停止，无法应用最新模块开关",
            ),
        }
    }

    /// Wait on the same lifecycle worker; a queued request is never mistaken
    /// for completed cleanup. Neither configuration nor window locks may be held.
    pub(crate) fn reconcile_and_wait(&self) -> Result<CapabilityStatusSnapshot, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let (reply, result) = mpsc::sync_channel(1);
        let mut message = SupervisorMessage::ReconcileAndReply(reply);
        loop {
            if self.shutdown_requested.load(Ordering::Acquire) {
                return Err("应用正在退出，无法切换模式".to_string());
            }
            match self.sender.try_send(message) {
                Ok(()) => break,
                Err(mpsc::TrySendError::Full(pending)) => message = pending,
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err("模块生命周期服务已停止".to_string());
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err("等待模块停止超时，模式未修改".to_string());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        result.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|error| format!("等待模块停止失败，模式未修改：{error}"))?
    }

    pub(crate) fn shutdown(&self) {
        let worker = self.worker.lock().ok().and_then(|mut worker| worker.take());
        let Some(worker) = worker else {
            return;
        };
        self.shutdown_requested.store(true, Ordering::Release);
        match self.sender.try_send(SupervisorMessage::Shutdown) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => {}
        }
        let worker_done = match self.worker_done.lock() {
            Ok(mut worker_done) => worker_done.take(),
            Err(_) => {
                crate::logger::log_msg(
                    "ERROR",
                    "Capabilities",
                    "capability supervisor 完成通知状态损坏；退出流程将继续",
                );
                return;
            }
        };
        if let Some(worker_done) = worker_done {
            if matches!(
                worker_done.recv_timeout(std::time::Duration::from_secs(3)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                crate::logger::log_msg(
                    "ERROR",
                    "Capabilities",
                    "capability supervisor 未能在 3 秒内停止；退出流程将继续",
                );
                return;
            }
        }
        if worker.join().is_err() {
            crate::logger::log_msg(
                "ERROR",
                "Capabilities",
                "capability supervisor 退出时发生 panic",
            );
        }
    }
}

fn reconcile_and_publish(app: &tauri::AppHandle, registry: &CapabilityRegistry) {
    match registry.reconcile_all() {
        Ok(snapshot) => publish_snapshot(app, &snapshot),
        Err(error) => crate::logger::log_msg(
            "ERROR",
            "Capabilities",
            &format!("模块生命周期协调失败: {error}"),
        ),
    }
}

fn shutdown_and_publish(app: &tauri::AppHandle, registry: &CapabilityRegistry) {
    registry.disable_all();
    reconcile_and_publish(app, registry);
}

fn reconcile_if_changed_and_publish(app: &tauri::AppHandle, registry: &CapabilityRegistry) {
    let previous_revision = registry.snapshot().revision;
    // A periodic full reconciliation both probes healthy drivers and retries
    // cleanup/start after a transient lifecycle failure. A health-only pass
    // cannot leave the `cleanup_required` state by design.
    match registry.reconcile_all() {
        Ok(snapshot) if snapshot.revision > previous_revision => publish_snapshot(app, &snapshot),
        Ok(_) => {}
        Err(error) => crate::logger::log_msg(
            "WARN",
            "Capabilities",
            &format!("周期协调 capability 生命周期失败: {error}"),
        ),
    }
}

fn publish_snapshot(app: &tauri::AppHandle, snapshot: &CapabilityStatusSnapshot) {
    if let Err(error) = app.emit(STATUS_EVENT, snapshot) {
        crate::logger::log_msg(
            "WARN",
            "Capabilities",
            &format!("发布 capability 状态失败: {error}"),
        );
    }
}
