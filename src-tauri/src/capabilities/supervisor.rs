use crate::application::capability::{CapabilityRegistry, CapabilityStatusSnapshot};
use std::sync::{mpsc, Arc, Mutex};
use tauri::Emitter;

const STATUS_EVENT: &str = "capability-status-updated";
const HEALTH_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

enum SupervisorMessage {
    Reconcile,
    Shutdown,
}

/// Serializes lifecycle hooks away from configuration transactions and emits
/// immutable status snapshots after each reconciliation pass.
pub(crate) struct CapabilitySupervisor {
    sender: mpsc::SyncSender<SupervisorMessage>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl CapabilitySupervisor {
    pub(crate) fn start(
        app: tauri::AppHandle,
        registry: Arc<CapabilityRegistry>,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("capability-supervisor".to_string())
            .spawn(move || loop {
                let message = match receiver.recv_timeout(HEALTH_POLL_INTERVAL) {
                    Ok(message) => message,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        refresh_health_and_publish(&app, &registry);
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                match message {
                    SupervisorMessage::Reconcile => {
                        let mut shutdown_requested = false;
                        while let Ok(queued) = receiver.try_recv() {
                            if matches!(queued, SupervisorMessage::Shutdown) {
                                shutdown_requested = true;
                                break;
                            }
                        }
                        if shutdown_requested {
                            shutdown_and_publish(&app, &registry);
                            break;
                        }
                        reconcile_and_publish(&app, &registry);
                    }
                    SupervisorMessage::Shutdown => {
                        shutdown_and_publish(&app, &registry);
                        break;
                    }
                }
            })
            .map_err(|error| format!("启动 capability supervisor 失败: {error}"))?;

        Ok(Self {
            sender,
            worker: Mutex::new(Some(worker)),
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

    pub(crate) fn shutdown(&self) {
        let worker = self.worker.lock().ok().and_then(|mut worker| worker.take());
        let Some(worker) = worker else {
            return;
        };
        let _ = self.sender.send(SupervisorMessage::Shutdown);
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

fn refresh_health_and_publish(app: &tauri::AppHandle, registry: &CapabilityRegistry) {
    let previous_revision = registry.snapshot().revision;
    match registry.refresh_running_health() {
        Ok(snapshot) if snapshot.revision > previous_revision => publish_snapshot(app, &snapshot),
        Ok(_) => {}
        Err(error) => crate::logger::log_msg(
            "WARN",
            "Capabilities",
            &format!("刷新 capability 健康状态失败: {error}"),
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
