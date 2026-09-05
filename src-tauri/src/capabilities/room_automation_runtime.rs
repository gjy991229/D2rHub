//! Tauri/Windows adapter for the optional room-automation capability.
//!
//! The sidecar controller is the only configuration authority. This adapter
//! owns every shortcut, bounded chat-binding scan, account lease, cancellation signal
//! and worker thread for exactly the capability's running lifetime.

use super::room_automation::{
    ChatKey, FollowerJoinMode, RoomAutomationConfig, WaitingMode, WorkflowPhase,
    WorkflowRecoveryAction, WorkflowStateError, WorkflowStatus, WorkflowTaskId, WorkflowTaskState,
};
use super::room_automation_config::{
    RoomAutomationConfigController, RoomAutomationConfigControllerError,
    RoomAutomationConfigSnapshot, ROOM_AUTOMATION_MODULE_ID,
};
use super::room_chat_binding::{
    validate_and_canonicalize_directories, ChatF13BindingService, ChatF13BindingStatus,
    ExplicitChatBindingConsent,
};
use super::supervisor::CapabilitySupervisor;
use crate::application::capability::{
    CapabilityDriver, CapabilityFailure, CapabilityHealth, CapabilityId,
};
use crate::application::multi_instance::{
    AccountLeaseManager, AccountOperationLeases, RunningInstance,
};
use crate::application::task_runtime::{TaskHandle, TaskRequest};
use crate::commands::account::AccountManager;
use crate::domain::config::GlobalConfig;
use crate::error::AppError;
use crate::input_listener::{
    register_unbounded_capability_shortcuts, replace_unbounded_capability_shortcuts,
    CapabilityShortcutRegistration,
};
use crate::state::SharedState;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

#[cfg(target_os = "windows")]
use super::room_automation_windows::{self as windows, CancellationCheck};

pub(crate) const ROOM_AUTOMATION_ID: CapabilityId = CapabilityId::new(ROOM_AUTOMATION_MODULE_ID);
pub(crate) const STATUS_EVENT: &str = "room-automation://status-changed";
pub(crate) const CONFIG_EVENT: &str = "room-automation://config-committed";

const PRIMARY_ACTION: &str = "start-primary";
const FOLLOWERS_ACTION: &str = "start-followers";

type PreparedWorkflow = (
    RoomAutomationConfig,
    RunningInstance,
    Vec<(String, RunningInstance)>,
);

type PreparedPrimaryWorkflow = (RoomAutomationConfig, RunningInstance);

struct CancellationSignal {
    cancelled: AtomicBool,
    state: std::sync::Mutex<()>,
    changed: Condvar,
}

impl CancellationSignal {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            state: std::sync::Mutex::new(()),
            changed: Condvar::new(),
        }
    }

    fn cancel(&self) {
        let _guard = self.state.lock().unwrap_or_else(|error| error.into_inner());
        self.cancelled.store(true, Ordering::Release);
        self.changed.notify_all();
    }

    fn check_active(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err("自动跟房流程已取消".to_string())
        } else {
            Ok(())
        }
    }

    fn wait(&self, duration: Duration) -> bool {
        if self.cancelled.load(Ordering::Acquire) {
            return true;
        }
        let guard = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let (_guard, _) = self
            .changed
            .wait_timeout_while(guard, duration, |_| !self.cancelled.load(Ordering::Acquire))
            .unwrap_or_else(|error| error.into_inner());
        self.cancelled.load(Ordering::Acquire)
    }
}

#[cfg(target_os = "windows")]
impl CancellationCheck for CancellationSignal {
    fn check(&self) -> Result<(), String> {
        self.check_active()
    }

    fn wait_cancelled(&self, duration: Duration) -> bool {
        self.wait(duration)
    }
}

trait RuntimeHost: Send + Sync {
    fn account_shortcuts(&self) -> Result<Vec<String>, String>;
    fn existing_account_ids(&self) -> Result<Vec<String>, String>;
    fn canonicalize_and_validate_accounts(
        &self,
        config: &mut RoomAutomationConfig,
    ) -> Result<(), String>;
    fn running_instance(&self, account_id: &str) -> Result<RunningInstance, String>;
    fn foreground_pid(&self) -> Option<u32>;
    fn run_primary(
        &self,
        config: &RoomAutomationConfig,
        pid: u32,
        room_name: &str,
        retrying: bool,
        cancel: &CancellationSignal,
    ) -> Result<(), String>;
    fn run_follower(
        &self,
        config: &RoomAutomationConfig,
        _account_id: &str,
        pid: u32,
        room_name: &str,
        cancel: &CancellationSignal,
    ) -> Result<(), String>;
}

struct WindowsRuntimeHost {
    state: SharedState,
}

impl WindowsRuntimeHost {
    fn global_config(&self) -> Result<GlobalConfig, String> {
        self.state
            .configuration()
            .snapshot()
            .ok_or_else(|| "尚未加载全局配置".to_string())
    }

    fn account_map(&self) -> Result<BTreeMap<String, crate::domain::account::AccountMeta>, String> {
        let config = self.global_config()?;
        let mut accounts = BTreeMap::new();
        for id in AccountManager::list_ids(&config.accounts_dir) {
            let account = match AccountManager::load_meta(&config.accounts_dir, &id) {
                Ok(account) => account,
                Err(AppError::AccountNotFound(_)) => {
                    crate::logger::log_msg(
                        "WARN",
                        "RoomAutomation",
                        &format!("忽略缺少 account.json 的残留账号目录：{id}"),
                    );
                    continue;
                }
                Err(error) => return Err(error.to_string()),
            };
            accounts.insert(id.to_ascii_lowercase(), account);
        }
        Ok(accounts)
    }
}

impl RuntimeHost for WindowsRuntimeHost {
    fn account_shortcuts(&self) -> Result<Vec<String>, String> {
        let config = self.global_config()?;
        let mut shortcuts = self
            .state
            .shortcut_map
            .read()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        if let Ok(Value::Object(bindings)) =
            serde_json::from_str::<Value>(&config.shortcut_bindings_json)
        {
            shortcuts.extend(
                bindings
                    .values()
                    .filter_map(Value::as_str)
                    .map(|shortcut| shortcut.trim().to_ascii_lowercase())
                    .filter(|shortcut| !shortcut.is_empty()),
            );
        }
        Ok(shortcuts.into_iter().collect())
    }

    fn existing_account_ids(&self) -> Result<Vec<String>, String> {
        Ok(self
            .account_map()?
            .into_values()
            .map(|account| account.id)
            .collect())
    }

    fn canonicalize_and_validate_accounts(
        &self,
        config: &mut RoomAutomationConfig,
    ) -> Result<(), String> {
        let accounts = self.account_map()?;
        canonicalize_account_references(config, &accounts)?;
        let shortcuts = self.account_shortcuts()?;
        config
            .validate(shortcuts.iter().map(String::as_str))
            .map_err(|error| error.to_string())
    }

    fn running_instance(&self, account_id: &str) -> Result<RunningInstance, String> {
        let account = self
            .account_map()?
            .remove(&account_id.to_ascii_lowercase())
            .ok_or_else(|| format!("账号“{account_id}”不存在"))?;
        if let Some(instance) = self
            .state
            .multi_instance()
            .facade()
            .instance(account_id)
            .filter(|instance| instance.launch.is_some())
        {
            if crate::infrastructure::system::find_game_hwnd(instance.pid).is_some() {
                return Ok(instance);
            }
        }

        let window_title = if account.display_name.trim().is_empty() {
            account.id.as_str()
        } else {
            account.display_name.as_str()
        };
        let pid = crate::infrastructure::system::find_unique_d2r_pid_by_exact_title(window_title)
            .ok_or_else(|| {
                format!(
                    "账号“{account_id}”没有可用的启动快照，且未找到标题为“{window_title}”的唯一 D2R 窗口"
                )
            })?;
        crate::logger::log_msg(
            "WARN",
            "RoomAutomation",
            &format!(
                "账号“{account_id}”没有可用的启动快照；已按精确窗口标题“{window_title}”兼容匹配 PID {pid}"
            ),
        );
        Ok(RunningInstance {
            account_id: account.id,
            pid,
            launch: None,
        })
    }

    fn foreground_pid(&self) -> Option<u32> {
        #[cfg(target_os = "windows")]
        {
            windows::foreground_pid()
        }
        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }

    fn run_primary(
        &self,
        config: &RoomAutomationConfig,
        pid: u32,
        room_name: &str,
        _retrying: bool,
        cancel: &CancellationSignal,
    ) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            let flow = config.flow();
            windows::fill_room_form(
                windows::RoomFormRequest {
                    pid,
                    background_text_strategy: &config.background_text_strategy,
                    chat_key: config.chat_key,
                    create: true,
                    // Every physical create-shortcut press starts a new room
                    // form. The old lobby-era duplicate-dialog confirmation
                    // path made manual waiting indistinguishable from retrying
                    // the previous native submission.
                    open_form: true,
                    name: room_name,
                    password: &config.password,
                    flow,
                },
                cancel,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (config, pid, room_name, cancel);
            Err("自动跟房仅支持 Windows".to_string())
        }
    }

    fn run_follower(
        &self,
        config: &RoomAutomationConfig,
        _account_id: &str,
        pid: u32,
        room_name: &str,
        cancel: &CancellationSignal,
    ) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            windows::fill_room_form(
                windows::RoomFormRequest {
                    pid,
                    background_text_strategy: &config.background_text_strategy,
                    chat_key: config.chat_key,
                    create: false,
                    open_form: true,
                    name: room_name,
                    password: &config.password,
                    flow: config.flow(),
                },
                cancel,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (config, account_id, pid, room_name, cancel);
            Err("自动跟房仅支持 Windows".to_string())
        }
    }
}

fn canonicalize_account_references(
    config: &mut RoomAutomationConfig,
    accounts: &BTreeMap<String, crate::domain::account::AccountMeta>,
) -> Result<(), String> {
    let enabled = config.enabled;
    let canonical = |account_id: &str| -> Result<Option<String>, String> {
        let Some(account) = accounts.get(&account_id.trim().to_ascii_lowercase()) else {
            return if enabled {
                Err(format!("账号“{account_id}”不存在"))
            } else {
                Ok(None)
            };
        };
        if enabled && !account.initialized {
            return Err(format!("账号“{}”尚未初始化", account.id));
        }
        Ok(Some(account.id.clone()))
    };

    if !config.primary_account_id.is_empty() {
        config.primary_account_id = canonical(&config.primary_account_id)?.unwrap_or_default();
    }
    config.follower_account_ids = config
        .follower_account_ids
        .iter()
        .filter_map(|account_id| canonical(account_id).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

trait ChatBindingPort: Send + Sync {
    fn status_for_key(&self, _key: ChatKey) -> Result<ChatF13BindingStatus, String> {
        self.status()
    }
    fn install_for_key(&self, _key: ChatKey) -> Result<ChatF13BindingStatus, String> {
        self.install()
    }
    fn resume_for_key(&self, _key: ChatKey) -> Result<ChatF13BindingStatus, String> {
        self.resume()
    }
    fn preflight_restore_for_key(&self, _key: ChatKey) -> Result<(), String> {
        self.preflight_restore()
    }
    fn restore_for_key(&self, _key: ChatKey) -> Result<ChatF13BindingStatus, String> {
        self.restore()
    }
    fn status(&self) -> Result<ChatF13BindingStatus, String>;
    fn install(&self) -> Result<ChatF13BindingStatus, String>;
    fn resume(&self) -> Result<ChatF13BindingStatus, String>;
    fn stop(&self) -> Result<ChatF13BindingStatus, String>;
    fn preflight_restore(&self) -> Result<(), String>;
    fn restore(&self) -> Result<ChatF13BindingStatus, String>;
}

struct LazyChatBinding {
    state: SharedState,
    /// Serializes service creation and every bounded key-file operation.
    operation: Mutex<()>,
    service: Mutex<Option<CachedChatBinding>>,
}

struct CachedChatBinding {
    directories: Vec<PathBuf>,
    service: Arc<ChatF13BindingService>,
}

impl LazyChatBinding {
    fn configured_directories(&self) -> Result<Vec<PathBuf>, String> {
        let config = self
            .state
            .configuration()
            .snapshot()
            .ok_or_else(|| "尚未加载全局配置".to_string())?;
        let directories = [&config.cn_saved_games_path, &config.global_saved_games_path]
            .into_iter()
            .map(|path| path.trim())
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        validate_and_canonicalize_directories(directories)
    }

    /// Must be called while `operation` is held. A directory change replaces
    /// the cached service before the next bounded scan.
    fn service(&self) -> Result<Arc<ChatF13BindingService>, String> {
        self.service_for_key(ChatKey::F13)
    }

    fn service_for_key(&self, key: ChatKey) -> Result<Arc<ChatF13BindingService>, String> {
        let directories = match self.configured_directories() {
            Ok(directories) => directories,
            Err(error) => {
                self.clear_cached_service()?;
                return Err(error);
            }
        };
        if key == ChatKey::F13 {
            self.service_for_directories(directories)
        } else {
            self.service_for_directories_and_key(directories, key)
        }
    }

    fn service_for_directories(
        &self,
        directories: Vec<PathBuf>,
    ) -> Result<Arc<ChatF13BindingService>, String> {
        self.service_for_directories_and_key(directories, ChatKey::F13)
    }

    fn service_for_directories_and_key(
        &self,
        directories: Vec<PathBuf>,
        key: ChatKey,
    ) -> Result<Arc<ChatF13BindingService>, String> {
        let previous = {
            let mut current = self.service.lock();
            if let Some(cached) = current.as_ref() {
                if cached.directories == directories && cached.service.chat_key() == key {
                    return Ok(Arc::clone(&cached.service));
                }
            }
            current.take()
        };
        let rescan_with_consent = previous.as_ref().is_some_and(|cached| {
            cached.service.chat_key() == key
                && cached
                    .service
                    .status()
                    .is_ok_and(|status| status.consent_granted)
        });
        if let Some(cached) = previous {
            cached.service.shutdown()?;
        }
        let probe = || !crate::infrastructure::system::get_d2r_pids().is_empty();
        let service = Arc::new(if key == ChatKey::F13 {
            ChatF13BindingService::new(directories.clone(), probe)?
        } else {
            ChatF13BindingService::new_with_key(directories.clone(), probe, key)?
        });
        if rescan_with_consent {
            let consent = ExplicitChatBindingConsent::from_persisted_user_consent(true)?;
            if let Err(error) = service.start_watcher_with_consent(consent) {
                crate::logger::log_msg(
                    "WARN",
                    "RoomAutomation",
                    &format!("存档目录变化后聊天键一次性扫描失败：{error}"),
                );
            }
        }
        *self.service.lock() = Some(CachedChatBinding {
            directories,
            service: Arc::clone(&service),
        });
        Ok(service)
    }

    fn clear_cached_service(&self) -> Result<(), String> {
        if let Some(cached) = self.service.lock().take() {
            cached.service.shutdown()?;
        }
        Ok(())
    }
}

impl ChatBindingPort for LazyChatBinding {
    fn status_for_key(&self, key: ChatKey) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        self.service_for_key(key)?.status()
    }

    fn install_for_key(&self, key: ChatKey) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        self.service_for_key(key)?.install()
    }

    fn resume_for_key(&self, key: ChatKey) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        let consent = ExplicitChatBindingConsent::from_persisted_user_consent(true)?;
        self.service_for_key(key)?
            .start_watcher_with_consent(consent)
    }

    fn preflight_restore_for_key(&self, key: ChatKey) -> Result<(), String> {
        let _operation = self.operation.lock();
        self.service_for_key(key)?.preflight_restore()
    }

    fn restore_for_key(&self, key: ChatKey) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        self.service_for_key(key)?.restore()
    }

    fn status(&self) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        self.service()?.status()
    }

    fn install(&self) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        self.service()?.install()
    }

    fn resume(&self) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        let consent = ExplicitChatBindingConsent::from_persisted_user_consent(true)?;
        self.service()?.start_watcher_with_consent(consent)
    }

    fn stop(&self) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        let service = self.service.lock().take();
        match service {
            Some(cached) => cached.service.stop(),
            None => Ok(ChatF13BindingStatus {
                ready: false,
                total_files: 0,
                installed_files: 0,
                eligible_files: 0,
                conflicted_files: 0,
                backup_files: 0,
                orphan_backup_files: 0,
                transaction_artifacts: 0,
                d2r_running: !crate::infrastructure::system::get_d2r_pids().is_empty(),
                consent_granted: false,
                watcher_running: false,
                auto_patch_enabled: false,
                directories: Vec::new(),
                last_watcher_error: None,
                message: "聊天键扫描服务尚未初始化".to_string(),
            }),
        }
    }

    fn preflight_restore(&self) -> Result<(), String> {
        let _operation = self.operation.lock();
        self.service()?.preflight_restore()
    }

    fn restore(&self) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        self.service()?.restore()
    }
}

trait RuntimeBridge: Send + Sync {
    fn publish_status(&self, status: &WorkflowStatus);
    fn publish_config(&self, snapshot: &RoomAutomationConfigSnapshot);
    fn apply_requested(&self, enabled: bool) -> Result<(), String>;
}

struct TauriRuntimeBridge {
    app: tauri::AppHandle,
    state: SharedState,
    unified_task: Mutex<Option<UnifiedWorkflowTask>>,
}

struct UnifiedWorkflowTask {
    workflow_id: WorkflowTaskId,
    handle: TaskHandle,
}

impl TauriRuntimeBridge {
    fn publish_unified_task(&self, status: &WorkflowStatus) {
        let Some(workflow_id) = status.task_id else {
            return;
        };
        let mut active = self.unified_task.lock();
        if active
            .as_ref()
            .is_some_and(|task| task.workflow_id != workflow_id)
        {
            if let Some(previous) = active.take() {
                let _ = previous.handle.cancelled("房间工作流已由新的重试任务替代");
            }
        }

        if active.is_none() && !status.phase.is_terminal() {
            let request = TaskRequest::new("room-automation")
                .with_conflict_key("room-automation-workflow")
                .non_retryable()
                .with_initial_status("primary", "自动跟房工作流已启动");
            match self.state.tasks().begin(match status.room_name.as_deref() {
                Some(room_name) => request.for_subject(room_name),
                None => request,
            }) {
                Ok(handle) => {
                    *active = Some(UnifiedWorkflowTask {
                        workflow_id,
                        handle,
                    });
                }
                Err(error) => {
                    crate::logger::log_msg(
                        "WARN",
                        "RoomAutomation",
                        &format!("登记统一任务失败，工作流继续按原状态机运行: {error}"),
                    );
                    return;
                }
            }
        }

        let Some(current) = active.as_ref() else {
            return;
        };
        if current.workflow_id != workflow_id {
            return;
        }
        match status.phase {
            WorkflowPhase::Primary => {
                let _ = current.handle.update(20, "primary", "主号正在创建房间");
            }
            WorkflowPhase::Waiting => {
                let message = match status.waiting_mode {
                    Some(WaitingMode::Automatic { .. }) => "等待自动启动小号跟进",
                    _ => "等待确认并启动小号跟进",
                };
                let _ = current.handle.update(45, "waiting", message);
            }
            WorkflowPhase::Followers => {
                let total = status.follower_account_ids.len().max(1);
                let completed = status.completed_follower_account_ids.len().min(total);
                let progress = 50 + ((completed * 45) / total) as u8;
                let undelivered = status.undelivered_follower_account_ids.len();
                let message = if undelivered > 0 {
                    format!("小号指令已派发 {completed}/{total}，{undelivered} 个窗口未送达")
                } else {
                    format!("小号指令已派发 {completed}/{total}")
                };
                let _ = current.handle.update(progress, "followers", &message);
            }
            WorkflowPhase::Complete => {
                if let Some(completed) = active.take() {
                    let undelivered = status.undelivered_follower_account_ids.len();
                    let message = if undelivered > 0 {
                        format!("跟房指令已全部派发，{undelivered} 个窗口未送达")
                    } else {
                        "跟房指令已全部派发".to_string()
                    };
                    let _ = completed.handle.succeed(&message);
                }
            }
            WorkflowPhase::Cancelled => {
                if let Some(cancelled) = active.take() {
                    let _ = cancelled.handle.cancelled("自动跟房工作流已取消");
                }
            }
            WorkflowPhase::Error => {
                if let Some(failed) = active.take() {
                    let message = status.last_error.as_deref().unwrap_or("自动跟房工作流失败");
                    let _ = failed.handle.fail("room-automation-failed", message);
                }
            }
            WorkflowPhase::Idle => {}
        }
    }
}

impl RuntimeBridge for TauriRuntimeBridge {
    fn publish_status(&self, status: &WorkflowStatus) {
        self.publish_unified_task(status);
        if let Err(error) = self.app.emit(STATUS_EVENT, status) {
            crate::logger::log_msg(
                "WARN",
                "RoomAutomation",
                &format!("发布任务状态失败: {error}"),
            );
        }
    }

    fn publish_config(&self, snapshot: &RoomAutomationConfigSnapshot) {
        if let Err(error) = self.app.emit(CONFIG_EVENT, snapshot) {
            crate::logger::log_msg(
                "WARN",
                "RoomAutomation",
                &format!("发布配置提交失败: {error}"),
            );
        }
    }

    fn apply_requested(&self, enabled: bool) -> Result<(), String> {
        self.state.configuration().project_current(|config| {
            let installed = self.state.optional_runtime_ready()
                && config.is_some_and(|config| {
                    config.optional_module_runtime_allowed(
                        crate::domain::config::OPTIONAL_MODULE_ROOM_AUTOMATION,
                    )
                });
            self.state
                .capabilities()
                .set_requested(ROOM_AUTOMATION_ID, installed && enabled)
                .map_err(|error| error.to_string())
        })?;
        if let Some(supervisor) = self.app.try_state::<CapabilitySupervisor>() {
            supervisor.schedule_reconcile();
        }
        Ok(())
    }
}

struct WorkflowWorker {
    task_id: WorkflowTaskId,
    cancel: Arc<CancellationSignal>,
    handle: JoinHandle<()>,
}

struct ShortcutWorker {
    registration: CapabilityShortcutRegistration,
    handle: JoinHandle<()>,
}

#[derive(Default)]
struct Lifecycle {
    started: bool,
    shortcut: Option<ShortcutWorker>,
    workflow: Option<WorkflowWorker>,
    leases: Option<AccountOperationLeases>,
    last_error: Option<CapabilityFailure>,
}

/// Concrete capability manager shared by the lifecycle registry and commands.
pub(crate) struct RoomAutomationManager {
    controller: RoomAutomationConfigController,
    snapshot: RwLock<RoomAutomationConfigSnapshot>,
    workflow: Mutex<WorkflowTaskState>,
    lifecycle: Mutex<Lifecycle>,
    operation: Mutex<()>,
    /// Linearizes a durable configuration commit with its requested-intent
    /// application without holding `operation` across supervisor callbacks.
    config_apply: Mutex<()>,
    leases: AccountLeaseManager,
    host: Arc<dyn RuntimeHost>,
    chat_binding: Arc<dyn ChatBindingPort>,
    bridge: Arc<dyn RuntimeBridge>,
    self_reference: Weak<RoomAutomationManager>,
}

/// Managed even when installation fails so IPC returns one stable error rather
/// than exposing missing Tauri state.
pub(crate) struct RoomAutomationCommandState {
    manager: Option<Arc<RoomAutomationManager>>,
    unavailable_reason: Option<String>,
}

/// A successful save means the sidecar commit is durable. Runtime application
/// is reported separately so callers never have to infer a partial success
/// from generation changes or a later reload.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct RoomAutomationSaveOutcome {
    pub snapshot: RoomAutomationConfigSnapshot,
    pub apply_warning: Option<String>,
}

impl RoomAutomationCommandState {
    pub(crate) fn available(manager: Arc<RoomAutomationManager>) -> Self {
        Self {
            manager: Some(manager),
            unavailable_reason: None,
        }
    }

    pub(crate) fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            manager: None,
            unavailable_reason: Some(reason.into()),
        }
    }

    pub(crate) fn manager(&self) -> Result<&Arc<RoomAutomationManager>, String> {
        self.manager.as_ref().ok_or_else(|| {
            self.unavailable_reason
                .clone()
                .unwrap_or_else(|| "自动跟房 capability 不可用".to_string())
        })
    }
}

impl RoomAutomationManager {
    pub(crate) fn install(app: &tauri::App) -> Result<Arc<Self>, CapabilityFailure> {
        #[cfg(not(target_os = "windows"))]
        return Err(CapabilityFailure::new(
            "platform-unsupported",
            "room automation is only available on Windows",
        ));

        #[cfg(target_os = "windows")]
        {
            let state = app.state::<SharedState>().inner().clone();
            let global = state
                .configuration()
                .snapshot()
                .ok_or_else(|| CapabilityFailure::new("config-unavailable", "尚未加载全局配置"))?;
            let host: Arc<dyn RuntimeHost> = Arc::new(WindowsRuntimeHost {
                state: state.clone(),
            });
            let shortcuts = host
                .account_shortcuts()
                .map_err(|error| CapabilityFailure::new("shortcut-config-invalid", error))?;
            let controller =
                RoomAutomationConfigController::new(&state.app_data_dir).map_err(config_failure)?;
            let legacy = global
                .preserved_unknown_fields
                .get("room_rotation")
                .cloned();
            let mut snapshot = controller
                .load_or_initialize(legacy, &shortcuts)
                .map_err(config_failure)?;
            crate::input_listener::with_shortcut_routing_transaction(|| {
                crate::input_listener::replace_saved_capability_shortcuts(
                    ROOM_AUTOMATION_MODULE_ID,
                    [
                        snapshot.config.shortcut.as_str(),
                        snapshot.config.join_shortcut.as_str(),
                    ],
                );
            });
            snapshot = prune_missing_accounts(&controller, snapshot, host.as_ref())?;
            if !global
                .optional_module_installed(crate::domain::config::OPTIONAL_MODULE_ROOM_AUTOMATION)
                && snapshot.config.enabled
            {
                let mut disabled = snapshot.config.clone();
                disabled.enabled = false;
                snapshot = controller
                    .save(snapshot.generation, disabled, &shortcuts)
                    .map_err(config_failure)?;
            }

            let mut validated = snapshot.config.clone();
            host.canonicalize_and_validate_accounts(&mut validated)
                .map_err(|error| CapabilityFailure::new("account-config-invalid", error))?;
            if validated != snapshot.config {
                snapshot = controller
                    .save(snapshot.generation, validated, &shortcuts)
                    .map_err(config_failure)?;
            }

            let chat_binding: Arc<dyn ChatBindingPort> = Arc::new(LazyChatBinding {
                state: state.clone(),
                operation: Mutex::new(()),
                service: Mutex::new(None),
            });
            let bridge: Arc<dyn RuntimeBridge> = Arc::new(TauriRuntimeBridge {
                app: app.handle().clone(),
                state: state.clone(),
                unified_task: Mutex::new(None),
            });
            let leases = state.multi_instance().account_leases().clone();
            Ok(Self::new(
                controller,
                snapshot,
                leases,
                host,
                chat_binding,
                bridge,
            ))
        }
    }

    fn new(
        controller: RoomAutomationConfigController,
        snapshot: RoomAutomationConfigSnapshot,
        leases: AccountLeaseManager,
        host: Arc<dyn RuntimeHost>,
        chat_binding: Arc<dyn ChatBindingPort>,
        bridge: Arc<dyn RuntimeBridge>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|self_reference| Self {
            controller,
            snapshot: RwLock::new(snapshot),
            workflow: Mutex::new(WorkflowTaskState::default()),
            lifecycle: Mutex::new(Lifecycle::default()),
            operation: Mutex::new(()),
            config_apply: Mutex::new(()),
            leases,
            host,
            chat_binding,
            bridge,
            self_reference: self_reference.clone(),
        })
    }

    pub(crate) fn requested_enabled(&self) -> bool {
        self.snapshot.read().config.enabled
    }

    pub(crate) fn get_config(&self) -> RoomAutomationConfigSnapshot {
        self.snapshot.read().clone()
    }

    pub(crate) fn get_status(&self) -> WorkflowStatus {
        self.workflow.lock().snapshot()
    }

    pub(crate) fn save_config(
        &self,
        expected_generation: u64,
        mut candidate: RoomAutomationConfig,
    ) -> Result<RoomAutomationSaveOutcome, String> {
        let _config_apply = self.config_apply.lock();
        self.join_finished_workflow();
        if self.workflow_is_reserved() {
            self.cancel()?;
        }
        let operation = self.operation.lock();

        let previous = self.get_config();
        let chat_key_changed = candidate.chat_key != previous.config.chat_key;
        if chat_key_changed {
            // A key-file edit cannot take effect in already-running clients.
            // Reject before committing a sender key that their cached binding lacks.
            if self
                .chat_binding
                .status_for_key(previous.config.chat_key)?
                .d2r_running
            {
                return Err(
                    "请先关闭全部 D2R 窗口，再切换聊天按键；游戏会缓存并覆盖键位文件".to_string(),
                );
            }
            candidate.chat_f13_auto_patch_enabled = true;
        }
        let consent_granted_while_enabling = !previous.config.enabled
            && candidate.enabled
            && !previous.config.chat_f13_auto_patch_enabled
            && candidate.chat_f13_auto_patch_enabled;
        if candidate.chat_f13_auto_patch_enabled != previous.config.chat_f13_auto_patch_enabled
            && !consent_granted_while_enabling
            && !chat_key_changed
        {
            return Err("聊天键授权只能通过安装或恢复操作修改".to_string());
        }
        candidate
            .normalize_legacy()
            .map_err(|error| error.to_string())?;
        let saved = crate::input_listener::with_shortcut_routing_transaction(|| {
            self.host
                .canonicalize_and_validate_accounts(&mut candidate)?;
            // Reject conflicts before durable save, even when routes are dormant.
            // Unchanged disabled drafts remain editable/recoverable.
            if candidate.enabled
                || candidate.shortcut != previous.config.shortcut
                || candidate.join_shortcut != previous.config.join_shortcut
            {
                crate::input_listener::validate_saved_capability_shortcuts(
                    ROOM_AUTOMATION_MODULE_ID,
                    [
                        candidate.shortcut.as_str(),
                        candidate.join_shortcut.as_str(),
                    ],
                )?;
            }
            let shortcuts = self.host.account_shortcuts()?;
            let saved = self
                .controller
                .save(expected_generation, candidate, &shortcuts)
                .map_err(|error| error.to_string())?;
            crate::input_listener::replace_saved_capability_shortcuts(
                ROOM_AUTOMATION_MODULE_ID,
                [
                    saved.config.shortcut.as_str(),
                    saved.config.join_shortcut.as_str(),
                ],
            );
            *self.snapshot.write() = saved.clone();
            Ok::<_, String>(saved)
        })?;
        self.bridge.publish_config(&saved);

        let old_shortcut =
            if previous.config.enabled && saved.config.enabled && self.lifecycle.lock().started {
                match self.replace_shortcuts(&saved.config) {
                    Ok(old) => old,
                    Err(error) => {
                        self.lifecycle.lock().last_error = Some(CapabilityFailure::new(
                            "shortcut-reload-failed",
                            error.clone(),
                        ));
                        drop(operation);
                        let warning = self.pause_after_committed_apply_failure(format!(
                            "快捷键重载失败：{error}"
                        ));
                        return Ok(RoomAutomationSaveOutcome {
                            snapshot: saved,
                            apply_warning: Some(warning),
                        });
                    }
                }
            } else {
                None
            };
        let binding_warning = if (saved.config.enabled || chat_key_changed)
            && saved.config.chat_f13_auto_patch_enabled
        {
            match self.chat_binding.resume_for_key(saved.config.chat_key) {
                Ok(_) => None,
                Err(error) => Some(format!("聊天键一次性扫描未完成：{error}")),
            }
        } else {
            None
        };
        drop(operation);
        join_shortcut(old_shortcut);
        let lifecycle_warning =
            self.bridge
                .apply_requested(saved.config.enabled)
                .err()
                .map(|error| {
                    self.pause_after_committed_apply_failure(format!(
                        "配置生命周期应用失败：{error}"
                    ))
                });
        let apply_warning = [binding_warning, lifecycle_warning]
            .into_iter()
            .flatten()
            .reduce(|left, right| format!("{left}；{right}"));
        Ok(RoomAutomationSaveOutcome {
            snapshot: saved,
            apply_warning,
        })
    }

    /// The sidecar has already committed when this is called, so every error
    /// becomes a warning. Stop locally as a fail-safe as well as clearing the
    /// requested intent; this prevents stale shortcuts or workers surviving if
    /// supervisor scheduling itself is the failing component.
    fn pause_after_committed_apply_failure(&self, initial: String) -> String {
        let mut details = vec![initial];
        self.lifecycle.lock().last_error = Some(CapabilityFailure::new(
            "config-apply-failed",
            details[0].clone(),
        ));
        if let Err(error) = self.bridge.apply_requested(false) {
            details.push(format!("请求暂停失败：{error}"));
        }
        if let Err(error) = <Self as CapabilityDriver>::stop(self) {
            details.push(format!("本地安全停止失败：{}", error.message));
        }
        format!("配置已保存，但模块已安全暂停：{}", details.join("；"))
    }

    pub(crate) fn start_primary(&self) -> Result<WorkflowStatus, String> {
        let _operation = self.operation.lock();
        self.join_finished_workflow();
        self.require_started()?;
        let mut status_before = self.workflow.lock().snapshot();
        validate_primary_trigger(&status_before)?;
        self.join_previous_workflow(status_before.task_id)?;
        status_before = self.workflow.lock().snapshot();
        validate_primary_trigger(&status_before)?;

        if self.lifecycle.lock().leases.is_some() {
            return Err("检测到尚未释放的自动跟房账号租约，已拒绝启动".to_string());
        }
        let raw_config = self.workflow_config_snapshot()?;
        // 建房阶段只操作主号。跟随号会在真正跟进前重新解析并单独取得租约，
        // 不应在主号操作或人工等待期间被提前锁住。
        let primary_lease = self.acquire_primary_lease(&raw_config)?;
        let (config, primary) = self.prepare_primary_workflow(raw_config, true)?;
        let task = self
            .workflow
            .lock()
            .begin_primary(&config, Some(chrono::Local::now().to_rfc3339()))
            .map_err(|error| error.to_string())?;
        self.lifecycle.lock().leases = Some(primary_lease);
        self.lifecycle.lock().last_error = None;
        let status = self.workflow.lock().snapshot();
        self.bridge.publish_status(&status);

        let cancel = Arc::new(CancellationSignal::new());
        let manager = self.self_reference.clone();
        let task_id = task.id;
        let room = task.room.clone();
        let retrying = task.retrying;
        let worker_cancel = Arc::clone(&cancel);
        let handle = std::thread::Builder::new()
            .name("room-automation-primary".to_string())
            .spawn(move || {
                if let Some(manager) = manager.upgrade() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        manager.run_primary_worker(
                            task_id,
                            config,
                            primary,
                            room.name,
                            room.sequence,
                            retrying,
                            worker_cancel,
                        );
                    }));
                    if result.is_err() {
                        manager.handle_worker_panic(task_id, "主号建房 worker");
                    }
                }
            })
            .map_err(|error| {
                let message = format!("创建主号建房线程失败: {error}");
                self.fail_and_release(task_id, &message);
                message
            })?;
        self.lifecycle.lock().workflow = Some(WorkflowWorker {
            task_id,
            cancel,
            handle,
        });
        Ok(status)
    }

    pub(crate) fn start_followers(&self) -> Result<WorkflowStatus, String> {
        let _operation = self.operation.lock();
        self.join_finished_workflow();
        self.require_started()?;
        let mut snapshot = self.workflow.lock().snapshot();
        validate_follower_trigger(&snapshot)?;
        self.join_previous_workflow(snapshot.task_id)?;
        snapshot = self.workflow.lock().snapshot();
        validate_follower_trigger(&snapshot)?;

        let previously_completed = snapshot.completed_follower_account_ids.clone();
        let previously_undelivered = snapshot.undelivered_follower_account_ids.clone();
        let pending_password = self
            .workflow
            .lock()
            .pending_password()
            .map(str::to_owned)
            .ok_or_else(|| "尚无待跟进房间密码".to_string())?;
        let mut raw_config = self.workflow_config_snapshot()?;
        raw_config.password = pending_password;
        if self.lifecycle.lock().leases.is_some() {
            return Err("检测到尚未释放的自动跟房账号租约，已拒绝继续".to_string());
        }
        // 快捷键执行只解析当前窗口并取得实际参与账号的租约。Mod 兼容性
        // 在加工和配置阶段报告，不能阻断运行中的原生输入序列。
        let ((config, _primary, mut followers), acquired_leases) =
            self.prepare_and_reserve_workflow(raw_config, true)?;
        let (task_id, room) = match snapshot.phase {
            WorkflowPhase::Waiting => {
                let task_id = snapshot
                    .task_id
                    .ok_or_else(|| "等待任务缺少 ID".to_string())?;
                self.workflow
                    .lock()
                    .begin_selected_followers(task_id, config.follower_account_ids.clone())
                    .map_err(|error| error.to_string())?;
                let room = self
                    .workflow
                    .lock()
                    .pending_room()
                    .cloned()
                    .ok_or_else(|| "尚无待跟进房间".to_string())?;
                (task_id, room)
            }
            WorkflowPhase::Error | WorkflowPhase::Cancelled => {
                let task = self
                    .workflow
                    .lock()
                    .resume_followers(&config, Some(chrono::Local::now().to_rfc3339()))
                    .map_err(|error| error.to_string())?;
                (task.id, task.room)
            }
            _ => unreachable!("follower trigger was validated above"),
        };
        self.lifecycle.lock().leases = Some(acquired_leases);
        self.lifecycle.lock().last_error = None;

        // A follower-stage retry resumes the same pending room and skips
        // accounts already confirmed during the failed attempt.
        for account_id in &previously_completed {
            if config
                .follower_account_ids
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(account_id))
            {
                let delivered = !previously_undelivered
                    .iter()
                    .any(|undelivered| undelivered.eq_ignore_ascii_case(account_id));
                let completion = self
                    .workflow
                    .lock()
                    .record_follower_dispatch(task_id, account_id, delivered);
                match completion {
                    Ok(status) => self.bridge.publish_status(&status),
                    Err(error) => {
                        let message = error.to_string();
                        self.fail_and_release(task_id, &message);
                        return Err(message);
                    }
                }
            }
        }
        followers.retain(|(account_id, _)| {
            !previously_completed
                .iter()
                .any(|completed| completed.eq_ignore_ascii_case(account_id))
        });

        let status = self.workflow.lock().snapshot();
        if status.phase == WorkflowPhase::Complete {
            self.lifecycle.lock().leases = None;
            return Ok(status);
        }
        self.bridge.publish_status(&status);
        self.spawn_followers_worker(task_id, config, followers, room.name, room.sequence)?;
        Ok(status)
    }

    pub(crate) fn retry(&self) -> Result<WorkflowStatus, String> {
        match self.get_status().recovery_action {
            Some(WorkflowRecoveryAction::RetryPrimary) => self.start_primary(),
            Some(WorkflowRecoveryAction::ResumeFollowers) => self.start_followers(),
            None => Err("当前任务没有可重试的失败阶段".to_string()),
        }
    }

    pub(crate) fn cancel(&self) -> Result<WorkflowStatus, String> {
        let operation = self.operation.lock();
        let worker = {
            let mut lifecycle = self.lifecycle.lock();
            if let Some(worker) = lifecycle.workflow.as_ref() {
                worker.cancel.cancel();
            }
            lifecycle.workflow.take()
        };
        let current = self.workflow.lock().snapshot();
        let status = if let Some(task_id) = current.task_id {
            if matches!(
                current.phase,
                WorkflowPhase::Primary | WorkflowPhase::Waiting | WorkflowPhase::Followers
            ) {
                self.workflow
                    .lock()
                    .cancel(task_id)
                    .map_err(|error| error.to_string())?
            } else {
                current
            }
        } else {
            current
        };
        self.bridge.publish_status(&status);
        drop(operation);
        join_workflow(worker)?;
        self.lifecycle.lock().leases = None;
        Ok(status)
    }

    pub(crate) fn get_chat_binding(&self) -> Result<ChatF13BindingStatus, String> {
        let _operation = self.operation.lock();
        self.chat_binding
            .status_for_key(self.get_config().config.chat_key)
    }

    pub(crate) fn install_chat_binding(&self) -> Result<ChatF13BindingStatus, String> {
        let _config_apply = self.config_apply.lock();
        let _operation = self.operation.lock();
        if self.workflow_is_reserved() {
            return Err("请先完成或取消当前自动跟房任务".to_string());
        }
        let installed = self
            .chat_binding
            .install_for_key(self.get_config().config.chat_key)?;
        match self.controller.set_chat_binding_consent(true) {
            Ok(snapshot) => {
                *self.snapshot.write() = snapshot.clone();
                self.bridge.publish_config(&snapshot);
                Ok(installed)
            }
            Err(error) => {
                let _ = self.chat_binding.stop();
                Err(format!("聊天键已安装但扫描授权保存失败：{error}"))
            }
        }
    }

    pub(crate) fn restore_chat_binding(&self) -> Result<ChatF13BindingStatus, String> {
        let _config_apply = self.config_apply.lock();
        let _operation = self.operation.lock();
        if self.workflow_is_reserved() {
            return Err("请先完成或取消当前自动跟房任务".to_string());
        }
        // Only revoke durable consent after a read-only filesystem preflight.
        // If the following transaction rolls back, restore the exact previous
        // scan consent as compensation.
        let chat_key = self.get_config().config.chat_key;
        self.chat_binding.preflight_restore_for_key(chat_key)?;
        let previous_consent = self.snapshot.read().config.chat_f13_auto_patch_enabled;
        let revoked = self
            .controller
            .set_chat_binding_consent(false)
            .map_err(|error| error.to_string())?;
        *self.snapshot.write() = revoked.clone();
        self.bridge.publish_config(&revoked);
        match self.chat_binding.restore_for_key(chat_key) {
            Ok(restored) => Ok(restored),
            Err(error) => match self.controller.set_chat_binding_consent(previous_consent) {
                Ok(snapshot) => {
                    *self.snapshot.write() = snapshot.clone();
                    self.bridge.publish_config(&snapshot);
                    Err(format!("{error}；授权状态已恢复"))
                }
                Err(compensation_error) => Err(format!(
                    "{error}；扫描授权状态回滚失败：{compensation_error}"
                )),
            },
        }
    }

    /// Optional cleanup hook. Core account deletion must never depend on it.
    pub(crate) fn remove_account_reference(&self, account_id: &str) -> Result<(), String> {
        let _config_apply = self.config_apply.lock();
        let operation = self.operation.lock();
        let snapshot = self
            .controller
            .remove_account(account_id)
            .map_err(|error| error.to_string())?;
        *self.snapshot.write() = snapshot.clone();
        self.bridge.publish_config(&snapshot);
        drop(operation);
        self.bridge.apply_requested(snapshot.config.enabled)
    }

    fn workflow_config_snapshot(&self) -> Result<RoomAutomationConfig, String> {
        let config = self.get_config().config;
        if !config.enabled {
            return Err("自动跟房模块尚未启用".to_string());
        }
        let binding = self.chat_binding.status_for_key(config.chat_key)?;
        if !binding.ready {
            return Err(format!(
                "{} 聊天键绑定尚未就绪，请扫描安装并重新启动游戏使其生效：{}",
                config.chat_key.label(),
                binding.message
            ));
        }
        Ok(config)
    }

    fn prepare_primary_workflow(
        &self,
        mut config: RoomAutomationConfig,
        require_primary_foreground: bool,
    ) -> Result<PreparedPrimaryWorkflow, String> {
        self.host.canonicalize_and_validate_accounts(&mut config)?;
        let primary = self.host.running_instance(&config.primary_account_id)?;
        if require_primary_foreground && self.host.foreground_pid() != Some(primary.pid) {
            return Err("请先切到主号 D2R 窗口再执行自动跟房".to_string());
        }
        Ok((config, primary))
    }

    fn prepare_workflow(
        &self,
        config: RoomAutomationConfig,
        require_primary_foreground: bool,
    ) -> Result<PreparedWorkflow, String> {
        let (config, primary) =
            self.prepare_primary_workflow(config, require_primary_foreground)?;
        let mut config = config;
        let mut followers = Vec::with_capacity(config.follower_account_ids.len());
        let mut skipped = Vec::new();
        for account_id in &config.follower_account_ids {
            let prepared = self.host.running_instance(account_id);
            match prepared {
                Ok(instance) => followers.push((account_id.clone(), instance)),
                Err(error) => {
                    skipped.push(format!("{account_id}: {error}"));
                    crate::logger::log_msg(
                        "WARN",
                        "RoomAutomation",
                        &format!("跳过不可用的跟随账号“{account_id}”：{error}"),
                    );
                }
            }
        }
        if followers.is_empty() {
            return Err(format!(
                "没有可加入房间的运行中跟随账号：{}",
                skipped.join("；")
            ));
        }
        config.follower_account_ids = followers
            .iter()
            .map(|(account_id, _)| account_id.clone())
            .collect();
        Ok((config, primary, followers))
    }

    fn acquire_participant_leases(
        &self,
        config: &RoomAutomationConfig,
    ) -> Result<AccountOperationLeases, String> {
        self.leases
            .try_acquire_many(
                std::iter::once(config.primary_account_id.as_str())
                    .chain(config.follower_account_ids.iter().map(String::as_str)),
            )
            .map_err(|error| error.to_string())
    }

    fn acquire_primary_lease(
        &self,
        config: &RoomAutomationConfig,
    ) -> Result<AccountOperationLeases, String> {
        self.leases
            .try_acquire_many(std::iter::once(config.primary_account_id.as_str()))
            .map_err(|error| error.to_string())
    }

    fn prepare_and_reserve_workflow(
        &self,
        config: RoomAutomationConfig,
        require_primary_foreground: bool,
    ) -> Result<(PreparedWorkflow, AccountOperationLeases), String> {
        // 第一次解析筛掉当前没有可用窗口的跟随号，避免它们无意义地参与锁竞争；
        // 取得实际参与者租约后再解析一次，关闭窗口/账号状态变化的竞态。
        let (candidate_config, _, _) = self.prepare_workflow(config, require_primary_foreground)?;
        let leases = self.acquire_participant_leases(&candidate_config)?;
        let prepared = self.prepare_workflow(candidate_config, require_primary_foreground)?;
        Ok((prepared, leases))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_primary_worker(
        &self,
        task_id: WorkflowTaskId,
        config: RoomAutomationConfig,
        primary: RunningInstance,
        room_name: String,
        sequence: u32,
        retrying: bool,
        cancel: Arc<CancellationSignal>,
    ) {
        let room_password = config.password.clone();
        if cancel.check_active().is_err() {
            self.fail_and_release(task_id, "自动跟房流程已取消");
            return;
        }
        // Reserve/consume the room sequence durably before the first external
        // keyboard side effect. A failed reservation is therefore safe to
        // retry, while every attempted form submission consumes its name.
        if self.persist_used_sequence(task_id, sequence).is_err() {
            return;
        }
        let result = self
            .host
            .run_primary(&config, primary.pid, &room_name, retrying, &cancel);
        if let Err(error) = result {
            self.fail_and_release(task_id, &error);
            return;
        }
        if cancel.check_active().is_err() {
            self.fail_and_release(task_id, "自动跟房流程已取消");
            return;
        }

        let waiting_mode = if config.auto_followers_enabled {
            WaitingMode::Automatic {
                delay_secs: config.auto_followers_delay_secs,
            }
        } else {
            WaitingMode::Manual
        };
        let ready = self.workflow.lock().primary_ready(task_id, waiting_mode);
        let Ok(status) = ready else {
            self.lifecycle.lock().leases = None;
            return;
        };
        self.bridge.publish_status(&status);
        // 主号阶段已经结束。无论是人工等待还是自动延时，都不跨等待状态
        // 持有账号租约；进入跟随阶段时会基于最新运行状态重新获取。
        self.lifecycle.lock().leases = None;
        if !config.auto_followers_enabled {
            return;
        }

        if cancel.wait(Duration::from_secs(config.auto_followers_delay_secs)) {
            self.fail_and_release(task_id, "自动跟房流程已取消");
            return;
        }
        let mut raw_config = match self.workflow_config_snapshot() {
            Ok(config) => config,
            Err(error) => {
                self.fail_and_release(task_id, &error);
                return;
            }
        };
        raw_config.password = room_password;
        let ((fresh_config, _primary, followers), leases) =
            match self.prepare_and_reserve_workflow(raw_config, false) {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.fail_and_release(task_id, &error);
                    return;
                }
            };
        self.lifecycle.lock().leases = Some(leases);
        match self
            .workflow
            .lock()
            .begin_selected_followers(task_id, fresh_config.follower_account_ids.clone())
        {
            Ok(status) => self.bridge.publish_status(&status),
            Err(_) => {
                self.lifecycle.lock().leases = None;
                return;
            }
        }
        self.run_followers(task_id, fresh_config, followers, room_name, cancel);
    }

    fn spawn_followers_worker(
        &self,
        task_id: WorkflowTaskId,
        config: RoomAutomationConfig,
        followers: Vec<(String, RunningInstance)>,
        room_name: String,
        _sequence: u32,
    ) -> Result<(), String> {
        let cancel = Arc::new(CancellationSignal::new());
        let worker_cancel = Arc::clone(&cancel);
        let manager = self.self_reference.clone();
        let handle = std::thread::Builder::new()
            .name("room-automation-followers".to_string())
            .spawn(move || {
                if let Some(manager) = manager.upgrade() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        manager.run_followers(task_id, config, followers, room_name, worker_cancel);
                    }));
                    if result.is_err() {
                        manager.handle_worker_panic(task_id, "小号跟进 worker");
                    }
                }
            })
            .map_err(|error| {
                let message = format!("创建小号跟进线程失败: {error}");
                self.fail_and_release(task_id, &message);
                message
            })?;
        self.lifecycle.lock().workflow = Some(WorkflowWorker {
            task_id,
            cancel,
            handle,
        });
        Ok(())
    }

    fn run_followers(
        &self,
        task_id: WorkflowTaskId,
        config: RoomAutomationConfig,
        followers: Vec<(String, RunningInstance)>,
        room_name: String,
        cancel: Arc<CancellationSignal>,
    ) {
        if config.follower_join_mode == FollowerJoinMode::Interval {
            self.run_interval_followers(task_id, config, followers, room_name, cancel);
            return;
        }

        let host = Arc::clone(&self.host);
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(followers.len());
            for (account_id, instance) in followers {
                let account_config = config.clone();
                let account_room = room_name.clone();
                let account_cancel = Arc::clone(&cancel);
                let account_host = Arc::clone(&host);
                handles.push((
                    account_id.clone(),
                    scope.spawn(move || {
                        account_host.run_follower(
                            &account_config,
                            &account_id,
                            instance.pid,
                            &account_room,
                            &account_cancel,
                        )
                    }),
                ));
            }
            handles
                .into_iter()
                .map(|(account_id, handle)| {
                    let result = handle
                        .join()
                        .unwrap_or_else(|_| Err("小号输入线程异常退出".to_string()));
                    (account_id, result)
                })
                .collect::<Vec<_>>()
        });

        let mut failures = Vec::new();
        for (account_id, result) in results {
            match result {
                Ok(()) => match self
                    .workflow
                    .lock()
                    .record_follower_complete(task_id, &account_id)
                {
                    Ok(status) => self.bridge.publish_status(&status),
                    Err(WorkflowStateError::StaleTask { .. })
                    | Err(WorkflowStateError::InvalidTransition { .. }) => {}
                    Err(error) => failures.push(format!("{account_id}: {error}")),
                },
                Err(error) => failures.push(format!("{account_id}: {error}")),
            }
        }
        if failures.is_empty() {
            self.lifecycle.lock().leases = None;
        } else {
            self.fail_and_release(
                task_id,
                &format!("部分小号执行失败：{}", failures.join("；")),
            );
        }
    }

    fn run_interval_followers(
        &self,
        task_id: WorkflowTaskId,
        config: RoomAutomationConfig,
        followers: Vec<(String, RunningInstance)>,
        room_name: String,
        cancel: Arc<CancellationSignal>,
    ) {
        let host = Arc::clone(&self.host);
        let interval = Duration::from_secs(config.follower_join_interval_secs);
        let dispatch_started_at = Instant::now();

        std::thread::scope(|scope| {
            let (result_sender, result_receiver) = std::sync::mpsc::channel();
            for (index, (account_id, instance)) in followers.into_iter().enumerate() {
                let account_config = config.clone();
                let account_room = room_name.clone();
                let account_cancel = Arc::clone(&cancel);
                let account_host = Arc::clone(&host);
                let account_result_sender = result_sender.clone();
                let dispatch_at = dispatch_started_at + interval.saturating_mul(index as u32);
                scope.spawn(move || {
                    let wait = dispatch_at.saturating_duration_since(Instant::now());
                    if account_cancel.wait(wait) {
                        return;
                    }
                    let result = account_host.run_follower(
                        &account_config,
                        &account_id,
                        instance.pid,
                        &account_room,
                        &account_cancel,
                    );
                    let _ = account_result_sender.send((account_id, result));
                });
            }
            drop(result_sender);

            for (account_id, result) in result_receiver {
                let delivered = result.is_ok();
                if let Err(error) = &result {
                    crate::logger::log_msg(
                        "WARN",
                        "RoomAutomation",
                        &format!("跟随账号“{account_id}”的进房指令未送达，队列继续：{error}"),
                    );
                }
                match self
                    .workflow
                    .lock()
                    .record_follower_dispatch(task_id, &account_id, delivered)
                {
                    Ok(status) => self.bridge.publish_status(&status),
                    Err(WorkflowStateError::StaleTask { .. })
                    | Err(WorkflowStateError::InvalidTransition { .. }) => {}
                    Err(error) => crate::logger::log_msg(
                        "WARN",
                        "RoomAutomation",
                        &format!("记录跟随账号“{account_id}”的指令派发结果失败：{error}"),
                    ),
                }
            }
        });

        self.lifecycle.lock().leases = None;
    }

    fn persist_used_sequence(&self, task_id: WorkflowTaskId, sequence: u32) -> Result<(), String> {
        match self.controller.advance_sequence_at_least(sequence) {
            Ok(snapshot) => {
                *self.snapshot.write() = snapshot.clone();
                self.bridge.publish_config(&snapshot);
                Ok(())
            }
            Err(error) => {
                let message = format!("保存下一房间序号失败：{error}");
                self.fail_and_release(task_id, &message);
                Err(message)
            }
        }
    }

    fn fail_and_release(&self, task_id: WorkflowTaskId, error: &str) {
        if let Ok(status) = self.workflow.lock().fail(task_id, error) {
            self.bridge.publish_status(&status);
            self.lifecycle.lock().leases = None;
        }
    }

    fn handle_worker_panic(&self, task_id: WorkflowTaskId, worker: &str) {
        if self.workflow.lock().status().task_id != Some(task_id) {
            return;
        }
        let message = format!("{worker} 异常退出");
        self.lifecycle.lock().last_error =
            Some(CapabilityFailure::new("workflow-panicked", message.clone()));
        self.fail_and_release(task_id, &message);
    }

    fn workflow_is_reserved(&self) -> bool {
        let status = self.workflow.lock().snapshot();
        let lifecycle = self.lifecycle.lock();
        lifecycle.leases.is_some()
            || lifecycle
                .workflow
                .as_ref()
                .is_some_and(|worker| !worker.handle.is_finished())
            || matches!(
                status.phase,
                WorkflowPhase::Primary | WorkflowPhase::Waiting | WorkflowPhase::Followers
            )
    }

    fn require_started(&self) -> Result<(), String> {
        if self.lifecycle.lock().started {
            Ok(())
        } else {
            Err("自动跟房 capability 尚未运行".to_string())
        }
    }

    fn join_finished_workflow(&self) {
        let worker = {
            let mut lifecycle = self.lifecycle.lock();
            if lifecycle
                .workflow
                .as_ref()
                .is_some_and(|worker| worker.handle.is_finished())
            {
                lifecycle.workflow.take()
            } else {
                None
            }
        };
        if let Some(worker) = worker {
            let task_id = worker.task_id;
            if worker.handle.join().is_err() {
                self.handle_worker_panic(task_id, "自动跟房 worker");
            }
        }
    }

    /// A terminal/manual-waiting status can become visible just before its
    /// worker returns. Explicitly taking and joining that slot prevents the
    /// next task from overwriting the only JoinHandle and detaching the old
    /// worker.
    fn join_previous_workflow(
        &self,
        expected_task_id: Option<WorkflowTaskId>,
    ) -> Result<(), String> {
        let worker = {
            let mut lifecycle = self.lifecycle.lock();
            match lifecycle.workflow.as_ref() {
                Some(worker) if Some(worker.task_id) != expected_task_id => {
                    return Err("检测到不匹配的自动跟房 worker，已拒绝覆盖".to_string());
                }
                Some(_) => lifecycle.workflow.take(),
                None => None,
            }
        };
        let task_id = worker.as_ref().map(|worker| worker.task_id);
        if let Err(error) = join_workflow(worker) {
            if let Some(task_id) = task_id {
                self.handle_worker_panic(task_id, "自动跟房 worker");
            }
            return Err(error);
        }
        Ok(())
    }

    fn build_shortcuts(
        &self,
        config: &RoomAutomationConfig,
        replace: bool,
    ) -> Result<ShortcutWorker, String> {
        // Shortcut delivery must not share one bounded slot between the primary
        // and follower actions. Key-repeat suppression happens in the keyboard
        // hook; this unbounded channel preserves every distinct physical press.
        let (sender, receiver) = std::sync::mpsc::channel();
        let manager = self.self_reference.clone();
        let handle = std::thread::Builder::new()
            .name("room-automation-shortcuts".to_string())
            .spawn(move || shortcut_dispatch_loop(manager, receiver))
            .map_err(|error| format!("启动快捷键 dispatcher 失败: {error}"))?;
        let routes = [
            (config.shortcut.clone(), PRIMARY_ACTION),
            (config.join_shortcut.clone(), FOLLOWERS_ACTION),
        ];
        let registration = if replace {
            replace_unbounded_capability_shortcuts(ROOM_AUTOMATION_MODULE_ID, routes, sender)
        } else {
            register_unbounded_capability_shortcuts(ROOM_AUTOMATION_MODULE_ID, routes, sender)
        };
        match registration {
            Ok(registration) => Ok(ShortcutWorker {
                registration,
                handle,
            }),
            Err(error) => {
                let _ = handle.join();
                Err(error)
            }
        }
    }

    fn replace_shortcuts(
        &self,
        config: &RoomAutomationConfig,
    ) -> Result<Option<ShortcutWorker>, String> {
        let replacement = self.build_shortcuts(config, true)?;
        let mut lifecycle = self.lifecycle.lock();
        lifecycle.last_error = None;
        Ok(lifecycle.shortcut.replace(replacement))
    }
}

impl CapabilityDriver for RoomAutomationManager {
    fn start(&self) -> Result<(), CapabilityFailure> {
        let _operation = self.operation.lock();
        self.join_finished_workflow();
        if self.lifecycle.lock().started {
            return Ok(());
        }

        let current = self.get_config();
        let pruned = prune_missing_accounts(&self.controller, current, self.host.as_ref())?;
        if pruned != self.get_config() {
            *self.snapshot.write() = pruned.clone();
            self.bridge.publish_config(&pruned);
        }
        if !pruned.config.enabled {
            return Err(CapabilityFailure::new(
                "config-disabled-after-prune",
                "自动跟房引用的账号已不存在，配置已安全停用",
            ));
        }
        let mut config = pruned.config.clone();
        self.host
            .canonicalize_and_validate_accounts(&mut config)
            .map_err(|error| CapabilityFailure::new("account-config-invalid", error))?;

        let shortcut = self
            .build_shortcuts(&config, false)
            .map_err(|error| CapabilityFailure::new("shortcut-registration-failed", error))?;
        let mut lifecycle = self.lifecycle.lock();
        lifecycle.started = true;
        lifecycle.shortcut = Some(shortcut);
        lifecycle.last_error = None;
        Ok(())
    }

    fn stop(&self) -> Result<(), CapabilityFailure> {
        let operation = self.operation.lock();
        let (shortcut, workflow) = {
            let mut lifecycle = self.lifecycle.lock();
            lifecycle.started = false;
            if let Some(worker) = lifecycle.workflow.as_ref() {
                worker.cancel.cancel();
            }
            (lifecycle.shortcut.take(), lifecycle.workflow.take())
        };
        let current = self.workflow.lock().snapshot();
        let status = current.task_id.and_then(|task_id| {
            if matches!(
                current.phase,
                WorkflowPhase::Primary | WorkflowPhase::Waiting | WorkflowPhase::Followers
            ) {
                self.workflow.lock().cancel(task_id).ok()
            } else {
                None
            }
        });
        if let Some(status) = status {
            self.bridge.publish_status(&status);
        }
        drop(operation);

        join_shortcut(shortcut);
        join_workflow(workflow)
            .map_err(|error| CapabilityFailure::new("workflow-stop-failed", error))?;
        self.lifecycle.lock().leases = None;
        self.chat_binding
            .stop()
            .map_err(|error| CapabilityFailure::new("chat-binding-stop-failed", error))?;
        Ok(())
    }

    fn health(&self) -> CapabilityHealth {
        let lifecycle = self.lifecycle.lock();
        if let Some(failure) = lifecycle.last_error.clone() {
            return CapabilityHealth::Degraded(failure);
        }
        if !lifecycle.started {
            return CapabilityHealth::Failed(CapabilityFailure::new(
                "runtime-stopped",
                "room automation runtime is stopped",
            ));
        }
        if lifecycle
            .shortcut
            .as_ref()
            .is_none_or(|shortcut| shortcut.handle.is_finished())
        {
            return CapabilityHealth::Failed(CapabilityFailure::new(
                "shortcut-dispatcher-stopped",
                "room automation shortcut dispatcher is stopped",
            ));
        }
        drop(lifecycle);
        CapabilityHealth::Healthy
    }

    fn account_removed(&self, account_id: &str) -> Result<(), CapabilityFailure> {
        self.remove_account_reference(account_id)
            .map_err(|error| CapabilityFailure::new("account-reference-cleanup-failed", error))
    }
}

fn validate_primary_trigger(status: &WorkflowStatus) -> Result<(), String> {
    if status.phase == WorkflowPhase::Waiting
        && matches!(status.waiting_mode, Some(WaitingMode::Automatic { .. }))
    {
        return Err("正在自动等待小号跟进；请先取消当前流程，不能人工重试建房".to_string());
    }
    if status.running {
        return Err("已有一轮自动跟房正在运行".to_string());
    }
    let allowed = matches!(
        status.phase,
        WorkflowPhase::Idle
            | WorkflowPhase::Complete
            | WorkflowPhase::Error
            | WorkflowPhase::Cancelled
    ) || (status.phase == WorkflowPhase::Waiting
        && status.waiting_mode == Some(WaitingMode::Manual));
    if allowed {
        Ok(())
    } else {
        Err(format!("当前状态 {:?} 不能启动主号建房", status.phase))
    }
}

fn validate_follower_trigger(status: &WorkflowStatus) -> Result<(), String> {
    if status.phase == WorkflowPhase::Waiting
        && matches!(status.waiting_mode, Some(WaitingMode::Automatic { .. }))
    {
        return Err("正在自动等待小号跟进；无需手动启动，若要接管请先取消当前流程".to_string());
    }
    if status.running {
        return Err("已有一轮自动跟房正在运行".to_string());
    }
    let allowed = (status.phase == WorkflowPhase::Waiting
        && status.waiting_mode == Some(WaitingMode::Manual))
        || (matches!(
            status.phase,
            WorkflowPhase::Error | WorkflowPhase::Cancelled
        ) && status.recovery_action == Some(WorkflowRecoveryAction::ResumeFollowers));
    if allowed {
        Ok(())
    } else {
        Err(format!("当前状态 {:?} 没有可跟进的房间", status.phase))
    }
}

fn shortcut_dispatch_loop(
    manager: Weak<RoomAutomationManager>,
    receiver: std::sync::mpsc::Receiver<&'static str>,
) {
    while let Ok(action) = receiver.recv() {
        let Some(manager) = manager.upgrade() else {
            break;
        };
        crate::logger::log_msg(
            "INFO",
            "RoomAutomation",
            &format!("快捷键 dispatcher 已接收动作：{action}"),
        );
        let result = match action {
            PRIMARY_ACTION => manager.start_primary(),
            FOLLOWERS_ACTION => manager.start_followers(),
            _ => continue,
        };
        match result {
            Ok(status) => crate::logger::log_msg(
                "INFO",
                "RoomAutomation",
                &format!(
                    "快捷键动作已启动：{action}，task={:?}，phase={:?}",
                    status.task_id, status.phase
                ),
            ),
            Err(error) => crate::logger::log_msg("WARN", "RoomAutomation", &error),
        }
    }
}

fn join_shortcut(worker: Option<ShortcutWorker>) {
    let Some(worker) = worker else {
        return;
    };
    let ShortcutWorker {
        registration,
        handle,
    } = worker;
    drop(registration);
    if handle.join().is_err() {
        crate::logger::log_msg(
            "WARN",
            "RoomAutomation",
            "快捷键 dispatcher 退出时发生 panic",
        );
    }
}

fn join_workflow(worker: Option<WorkflowWorker>) -> Result<(), String> {
    let Some(worker) = worker else {
        return Ok(());
    };
    worker.cancel.cancel();
    worker
        .handle
        .join()
        .map_err(|_| format!("自动跟房任务 {:?} 退出时发生 panic", worker.task_id))
}

fn config_failure(error: RoomAutomationConfigControllerError) -> CapabilityFailure {
    CapabilityFailure::new("module-config-invalid", error.to_string())
}

fn prune_missing_accounts(
    controller: &RoomAutomationConfigController,
    mut snapshot: RoomAutomationConfigSnapshot,
    host: &dyn RuntimeHost,
) -> Result<RoomAutomationConfigSnapshot, CapabilityFailure> {
    let existing = host
        .existing_account_ids()
        .map_err(|message| CapabilityFailure::new("account-catalog-unavailable", message))?
        .into_iter()
        .map(|account_id| account_id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut referenced = Vec::new();
    if !snapshot.config.primary_account_id.is_empty() {
        referenced.push(snapshot.config.primary_account_id.clone());
    }
    referenced.extend(snapshot.config.follower_account_ids.clone());
    referenced.sort_by_key(|account_id| account_id.to_ascii_lowercase());
    referenced.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    for account_id in referenced {
        if !existing.contains(&account_id.to_ascii_lowercase()) {
            snapshot = controller
                .remove_account(&account_id)
                .map_err(config_failure)?;
        }
    }
    Ok(snapshot)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_listener::register_capability_shortcuts;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::{Receiver, SyncSender};
    use std::time::Instant;

    static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "d2rhub_room_runtime_{label}_{}",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct FakeHost {
        fail_primary: AtomicBool,
        panic_primary: AtomicBool,
        fail_follower_once: AtomicBool,
        primary_calls: Mutex<Vec<(String, bool)>>,
        follower_calls: Mutex<Vec<(String, String)>>,
        preflight_gate: Mutex<Option<(SyncSender<()>, Receiver<()>)>>,
    }

    impl FakeHost {
        fn new(fail_primary: bool) -> Self {
            Self {
                fail_primary: AtomicBool::new(fail_primary),
                panic_primary: AtomicBool::new(false),
                fail_follower_once: AtomicBool::new(false),
                primary_calls: Mutex::new(Vec::new()),
                follower_calls: Mutex::new(Vec::new()),
                preflight_gate: Mutex::new(None),
            }
        }

        fn instance(account_id: &str) -> RunningInstance {
            let pid = if account_id.eq_ignore_ascii_case("main") {
                101
            } else {
                202
            };
            RunningInstance {
                account_id: account_id.to_string(),
                pid,
                launch: None,
            }
        }
    }

    impl RuntimeHost for FakeHost {
        fn account_shortcuts(&self) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }

        fn existing_account_ids(&self) -> Result<Vec<String>, String> {
            Ok(vec![
                "main".to_string(),
                "follower".to_string(),
                "follower-a".to_string(),
                "follower-b".to_string(),
            ])
        }

        fn canonicalize_and_validate_accounts(
            &self,
            config: &mut RoomAutomationConfig,
        ) -> Result<(), String> {
            if let Some((started, release)) = self.preflight_gate.lock().take() {
                let _ = started.send(());
                let _ = release.recv();
            }
            config
                .validate(std::iter::empty::<&str>())
                .map_err(|error| error.to_string())
        }

        fn running_instance(&self, account_id: &str) -> Result<RunningInstance, String> {
            Ok(Self::instance(account_id))
        }

        fn foreground_pid(&self) -> Option<u32> {
            Some(101)
        }

        fn run_primary(
            &self,
            _config: &RoomAutomationConfig,
            _pid: u32,
            room_name: &str,
            retrying: bool,
            cancel: &CancellationSignal,
        ) -> Result<(), String> {
            cancel.check_active()?;
            self.primary_calls
                .lock()
                .push((room_name.to_string(), retrying));
            assert!(
                !self.panic_primary.load(Ordering::Acquire),
                "injected primary panic"
            );
            if self.fail_primary.load(Ordering::Acquire) {
                Err("injected primary failure".to_string())
            } else {
                Ok(())
            }
        }

        fn run_follower(
            &self,
            _config: &RoomAutomationConfig,
            account_id: &str,
            _pid: u32,
            room_name: &str,
            cancel: &CancellationSignal,
        ) -> Result<(), String> {
            cancel.check_active()?;
            self.follower_calls
                .lock()
                .push((account_id.to_string(), room_name.to_string()));
            if account_id == "follower-b" && self.fail_follower_once.swap(false, Ordering::AcqRel) {
                Err("injected follower failure".to_string())
            } else {
                Ok(())
            }
        }
    }

    struct FakeBinding;

    impl FakeBinding {
        fn status(watcher_running: bool) -> ChatF13BindingStatus {
            ChatF13BindingStatus {
                ready: true,
                total_files: 1,
                installed_files: 1,
                eligible_files: 0,
                conflicted_files: 0,
                backup_files: 1,
                orphan_backup_files: 0,
                transaction_artifacts: 0,
                d2r_running: false,
                consent_granted: watcher_running,
                watcher_running,
                auto_patch_enabled: watcher_running,
                directories: Vec::new(),
                last_watcher_error: None,
                message: "ready".to_string(),
            }
        }
    }

    impl ChatBindingPort for FakeBinding {
        fn status(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(Self::status(false))
        }

        fn install(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(Self::status(true))
        }

        fn resume(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(Self::status(true))
        }

        fn stop(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(Self::status(false))
        }

        fn preflight_restore(&self) -> Result<(), String> {
            Ok(())
        }

        fn restore(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(Self::status(false))
        }
    }

    struct PreflightFailBinding;

    impl ChatBindingPort for PreflightFailBinding {
        fn status(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(FakeBinding::status(true))
        }

        fn install(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(FakeBinding::status(true))
        }

        fn resume(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(FakeBinding::status(true))
        }

        fn stop(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(FakeBinding::status(false))
        }

        fn preflight_restore(&self) -> Result<(), String> {
            Err("injected restore preflight failure".to_string())
        }

        fn restore(&self) -> Result<ChatF13BindingStatus, String> {
            panic!("restore must not run after a failed preflight")
        }
    }

    struct RestoreFailBinding;

    impl ChatBindingPort for RestoreFailBinding {
        fn status(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(FakeBinding::status(true))
        }

        fn install(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(FakeBinding::status(true))
        }

        fn resume(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(FakeBinding::status(true))
        }

        fn stop(&self) -> Result<ChatF13BindingStatus, String> {
            Ok(FakeBinding::status(false))
        }

        fn preflight_restore(&self) -> Result<(), String> {
            Ok(())
        }

        fn restore(&self) -> Result<ChatF13BindingStatus, String> {
            Err("injected restore transaction failure".to_string())
        }
    }

    #[derive(Default)]
    struct FakeBridge {
        requested: AtomicBool,
        fail_apply: AtomicBool,
        statuses: Mutex<Vec<WorkflowStatus>>,
    }

    impl RuntimeBridge for FakeBridge {
        fn publish_status(&self, status: &WorkflowStatus) {
            self.statuses.lock().push(status.clone());
        }

        fn publish_config(&self, _snapshot: &RoomAutomationConfigSnapshot) {}

        fn apply_requested(&self, enabled: bool) -> Result<(), String> {
            if self.fail_apply.load(Ordering::Acquire) {
                return Err("injected lifecycle apply failure".to_string());
            }
            self.requested.store(enabled, Ordering::Release);
            Ok(())
        }
    }

    struct BlockingApplyBridge {
        requested: AtomicBool,
        calls: Mutex<Vec<bool>>,
        first_started: Mutex<Option<SyncSender<()>>>,
        release_first: Mutex<Option<Receiver<()>>>,
    }

    impl RuntimeBridge for BlockingApplyBridge {
        fn publish_status(&self, _status: &WorkflowStatus) {}

        fn publish_config(&self, _snapshot: &RoomAutomationConfigSnapshot) {}

        fn apply_requested(&self, enabled: bool) -> Result<(), String> {
            let is_first = {
                let mut calls = self.calls.lock();
                calls.push(enabled);
                calls.len() == 1
            };
            if is_first {
                if let Some(started) = self.first_started.lock().take() {
                    let _ = started.send(());
                }
                if let Some(release) = self.release_first.lock().take() {
                    let _ = release.recv();
                }
            }
            self.requested.store(enabled, Ordering::Release);
            Ok(())
        }
    }

    struct BlockingWaitingBridge {
        waiting_started: Mutex<Option<SyncSender<()>>>,
        release_waiting: Mutex<Option<Receiver<()>>>,
    }

    impl RuntimeBridge for BlockingWaitingBridge {
        fn publish_status(&self, status: &WorkflowStatus) {
            if status.phase != WorkflowPhase::Waiting {
                return;
            }
            if let Some(started) = self.waiting_started.lock().take() {
                let _ = started.send(());
            }
            if let Some(release) = self.release_waiting.lock().take() {
                let _ = release.recv();
            }
        }

        fn publish_config(&self, _snapshot: &RoomAutomationConfigSnapshot) {}

        fn apply_requested(&self, _enabled: bool) -> Result<(), String> {
            Ok(())
        }
    }

    fn manager(
        label: &str,
        auto_followers: bool,
        fail_primary: bool,
    ) -> (
        TestDirectory,
        Arc<RoomAutomationManager>,
        AccountLeaseManager,
        Arc<FakeHost>,
    ) {
        let root = TestDirectory::new(label);
        let controller = RoomAutomationConfigController::new(&root.0).unwrap();
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let config = RoomAutomationConfig {
            enabled: true,
            primary_account_id: "main".to_string(),
            follower_account_ids: vec!["follower".to_string()],
            auto_followers_enabled: auto_followers,
            auto_followers_delay_secs: 2,
            shortcut: "Ctrl+Alt+F21".to_string(),
            join_shortcut: "Ctrl+Alt+F22".to_string(),
            ..RoomAutomationConfig::default()
        };
        let snapshot = controller.save(initial.generation, config, &[]).unwrap();
        let leases = AccountLeaseManager::default();
        let host = Arc::new(FakeHost::new(fail_primary));
        let manager = RoomAutomationManager::new(
            controller,
            snapshot,
            leases.clone(),
            host.clone(),
            Arc::new(FakeBinding),
            Arc::new(FakeBridge::default()),
        );
        (root, manager, leases, host)
    }

    fn wait_for_phase(manager: &RoomAutomationManager, phase: WorkflowPhase) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if manager.get_status().phase == phase {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!(
            "timed out waiting for {phase:?}: {:?}",
            manager.get_status()
        );
    }

    #[test]
    fn manual_waiting_releases_leases_until_followers_start_and_stop_owns_shortcuts() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let (root, manager, leases, _host) = manager("manual", false, false);
        manager.start().unwrap();

        let (conflict_sender, _receiver) = std::sync::mpsc::sync_channel(1);
        assert!(register_capability_shortcuts(
            "conflicting-test",
            [("ctrl+alt+f21".to_string(), "conflict")],
            conflict_sender,
        )
        .is_err());

        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Waiting);
        assert!(leases.is_empty());
        assert_eq!(manager.get_config().config.next_sequence, 2);
        let reloaded = RoomAutomationConfigController::new(&root.0)
            .unwrap()
            .load_or_initialize(None, &[])
            .unwrap();
        assert_eq!(reloaded.config.next_sequence, 2);
        manager.cancel().unwrap();
        assert!(leases.is_empty());
        manager.stop().unwrap();

        let (sender, _receiver) = std::sync::mpsc::sync_channel(1);
        let registration = register_capability_shortcuts(
            "after-stop-test",
            [("ctrl+alt+f21".to_string(), "available")],
            sender,
        )
        .unwrap();
        drop(registration);
    }

    #[test]
    fn primary_failure_releases_every_account_lease() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let (_root, manager, leases, host) = manager("failure", false, true);
        manager.start().unwrap();
        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Error);
        assert!(leases.is_empty());
        assert_eq!(manager.get_config().config.next_sequence, 2);

        host.fail_primary.store(false, Ordering::Release);
        manager.retry().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Waiting);
        assert_eq!(
            host.primary_calls
                .lock()
                .iter()
                .map(|(room, retrying)| (room.as_str(), *retrying))
                .collect::<Vec<_>>(),
            [("run-001", false), ("run-002", false)]
        );
        manager.cancel().unwrap();
        manager.stop().unwrap();
    }

    #[test]
    fn primary_lease_is_held_before_runtime_preflight() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let (_root, manager, leases, host) = manager("leased_preflight", false, false);
        manager.start().unwrap();
        let (preflight_started_tx, preflight_started_rx) = std::sync::mpsc::sync_channel(1);
        let (release_preflight_tx, release_preflight_rx) = std::sync::mpsc::sync_channel(1);
        *host.preflight_gate.lock() = Some((preflight_started_tx, release_preflight_rx));
        let start_manager = manager.clone();
        let start = std::thread::spawn(move || start_manager.start_primary());
        preflight_started_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();

        assert!(leases.try_acquire("main").is_err());
        assert!(leases.try_acquire("FOLLOWER").is_ok());

        release_preflight_tx.send(()).unwrap();
        start.join().unwrap().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Waiting);
        manager.cancel().unwrap();
        manager.stop().unwrap();
    }

    #[test]
    fn worker_panic_immediately_fails_health_and_releases_leases() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let (_root, manager, leases, host) = manager("worker_panic", false, false);
        manager.start().unwrap();
        host.panic_primary.store(true, Ordering::Release);
        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Error);

        assert!(leases.is_empty());
        assert!(matches!(
            manager.health(),
            CapabilityHealth::Degraded(CapabilityFailure { reason_code, .. })
                if reason_code == "workflow-panicked"
        ));
        manager.stop().unwrap();
    }

    #[test]
    fn sequence_persistence_failure_fails_task_before_follower_delivery() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let (root, manager, leases, host) = manager("sequence_failure", false, false);
        manager.start().unwrap();
        let module_dir = root.0.join("modules").join(ROOM_AUTOMATION_MODULE_ID);
        std::fs::write(module_dir.join("config.json"), "{broken-primary").unwrap();
        std::fs::write(module_dir.join("config.json.bak"), "{broken-backup").unwrap();
        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Error);

        assert!(manager
            .get_status()
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("保存下一房间序号失败")));
        assert_eq!(manager.get_status().phase, WorkflowPhase::Error);
        assert!(host.primary_calls.lock().is_empty());
        assert!(leases.is_empty());
        manager.stop().unwrap();
    }

    #[test]
    fn durable_save_returns_snapshot_and_warning_when_runtime_apply_fails() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let root = TestDirectory::new("save_apply_warning");
        let controller = RoomAutomationConfigController::new(&root.0).unwrap();
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let bridge = Arc::new(FakeBridge::default());
        bridge.fail_apply.store(true, Ordering::Release);
        let manager = RoomAutomationManager::new(
            controller.clone(),
            initial.clone(),
            AccountLeaseManager::default(),
            Arc::new(FakeHost::new(false)),
            Arc::new(FakeBinding),
            bridge,
        );
        let mut candidate = initial.config.clone();
        candidate.name_prefix = "saved-".to_string();

        let outcome = manager.save_config(initial.generation, candidate).unwrap();

        assert_eq!(outcome.snapshot.config.name_prefix, "saved-");
        assert!(outcome
            .apply_warning
            .as_deref()
            .is_some_and(|warning| warning.contains("配置已保存")));
        let reloaded = controller.load_or_initialize(None, &[]).unwrap();
        assert_eq!(reloaded.generation, outcome.snapshot.generation);
        assert_eq!(reloaded.config.name_prefix, "saved-");
    }

    #[test]
    fn concurrent_saves_apply_requested_intent_in_commit_order() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let root = TestDirectory::new("save_apply_order");
        let controller = RoomAutomationConfigController::new(&root.0).unwrap();
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let (first_started_tx, first_started_rx) = std::sync::mpsc::sync_channel(1);
        let (release_first_tx, release_first_rx) = std::sync::mpsc::sync_channel(1);
        let bridge = Arc::new(BlockingApplyBridge {
            requested: AtomicBool::new(false),
            calls: Mutex::new(Vec::new()),
            first_started: Mutex::new(Some(first_started_tx)),
            release_first: Mutex::new(Some(release_first_rx)),
        });
        let manager = RoomAutomationManager::new(
            controller,
            initial.clone(),
            AccountLeaseManager::default(),
            Arc::new(FakeHost::new(false)),
            Arc::new(FakeBinding),
            bridge.clone(),
        );
        let mut first = initial.config.clone();
        first.enabled = true;
        first.primary_account_id = "main".to_string();
        first.follower_account_ids = vec!["follower".to_string()];
        first.name_prefix = "first-".to_string();
        let first_manager = manager.clone();
        let first_save =
            std::thread::spawn(move || first_manager.save_config(initial.generation, first));
        first_started_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();

        let first_commit = manager.get_config();
        assert_eq!(first_commit.config.name_prefix, "first-");
        let mut second = first_commit.config.clone();
        second.enabled = false;
        second.name_prefix = "second-".to_string();
        let second_manager = manager.clone();
        let (second_attempted_tx, second_attempted_rx) = std::sync::mpsc::sync_channel(1);
        let second_save = std::thread::spawn(move || {
            let _ = second_attempted_tx.send(());
            second_manager.save_config(first_commit.generation, second)
        });
        second_attempted_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(manager.get_config().config.name_prefix, "first-");

        release_first_tx.send(()).unwrap();
        assert!(first_save.join().unwrap().unwrap().apply_warning.is_none());
        let second_outcome = second_save.join().unwrap().unwrap();
        assert_eq!(second_outcome.snapshot.config.name_prefix, "second-");
        assert_eq!(*bridge.calls.lock(), [true, false]);
        assert!(!bridge.requested.load(Ordering::Acquire));
    }

    #[test]
    fn lazy_chat_binding_stop_is_serialized_by_the_outer_operation_lock() {
        let binding = Arc::new(LazyChatBinding {
            state: Arc::new(crate::state::AppState::new()),
            operation: Mutex::new(()),
            service: Mutex::new(None),
        });
        let stop_binding = binding.clone();
        let operation = binding.operation.lock();
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
        let stop = std::thread::spawn(move || {
            let result = stop_binding.stop();
            let _ = finished_tx.send(());
            result
        });

        assert!(finished_rx.recv_timeout(Duration::from_millis(40)).is_err());
        drop(operation);
        finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!stop.join().unwrap().unwrap().watcher_running);
    }

    #[test]
    fn lazy_chat_binding_replaces_cached_paths_and_accepts_a_later_directory() {
        let first = TestDirectory::new("chat_path_first");
        let second = TestDirectory::new("chat_path_second");
        let later = second.0.join("created-later");
        let binding = LazyChatBinding {
            state: Arc::new(crate::state::AppState::new()),
            operation: Mutex::new(()),
            service: Mutex::new(None),
        };

        let first_service = {
            let _operation = binding.operation.lock();
            binding
                .service_for_directories(
                    validate_and_canonicalize_directories(vec![first.0.clone()]).unwrap(),
                )
                .unwrap()
        };
        let first_again = {
            let _operation = binding.operation.lock();
            binding
                .service_for_directories(
                    validate_and_canonicalize_directories(vec![first.0.clone()]).unwrap(),
                )
                .unwrap()
        };
        assert!(Arc::ptr_eq(&first_service, &first_again));

        let second_service = {
            let _operation = binding.operation.lock();
            binding
                .service_for_directories(
                    validate_and_canonicalize_directories(vec![second.0.clone()]).unwrap(),
                )
                .unwrap()
        };
        assert!(!Arc::ptr_eq(&first_service, &second_service));
        assert_eq!(
            second_service.status().unwrap().directories,
            vec![second.0.canonicalize().unwrap().to_string_lossy()]
        );

        {
            let _operation = binding.operation.lock();
            assert!(binding
                .service_for_directories(vec![later.clone()])
                .is_err());
        }
        std::fs::create_dir_all(&later).unwrap();
        let later_service = {
            let _operation = binding.operation.lock();
            binding
                .service_for_directories(
                    validate_and_canonicalize_directories(vec![later.clone()]).unwrap(),
                )
                .unwrap()
        };
        assert_eq!(
            later_service.status().unwrap().directories,
            vec![later.canonicalize().unwrap().to_string_lossy()]
        );
    }

    #[test]
    fn failed_restore_preflight_preserves_durable_consent() {
        let root = TestDirectory::new("restore_preflight_consent");
        let controller = RoomAutomationConfigController::new(&root.0).unwrap();
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let consent = controller.set_chat_binding_consent(true).unwrap();
        let manager = RoomAutomationManager::new(
            controller,
            consent,
            AccountLeaseManager::default(),
            Arc::new(FakeHost::new(false)),
            Arc::new(PreflightFailBinding),
            Arc::new(FakeBridge::default()),
        );

        let error = manager.restore_chat_binding().unwrap_err();

        assert!(error.contains("injected restore preflight failure"));
        assert!(manager.get_config().config.chat_f13_auto_patch_enabled);
        assert_eq!(manager.get_config().generation, initial.generation + 1);
    }

    #[test]
    fn failed_restore_transaction_compensates_durable_consent() {
        let root = TestDirectory::new("restore_transaction_consent");
        let controller = RoomAutomationConfigController::new(&root.0).unwrap();
        controller.load_or_initialize(None, &[]).unwrap();
        let consent = controller.set_chat_binding_consent(true).unwrap();
        let manager = RoomAutomationManager::new(
            controller,
            consent,
            AccountLeaseManager::default(),
            Arc::new(FakeHost::new(false)),
            Arc::new(RestoreFailBinding),
            Arc::new(FakeBridge::default()),
        );

        let error = manager.restore_chat_binding().unwrap_err();

        assert!(error.contains("injected restore transaction failure"));
        assert!(error.contains("授权状态已恢复"));
        assert!(manager.get_config().config.chat_f13_auto_patch_enabled);
    }

    #[test]
    fn capability_stop_cancels_and_joins_automatic_wait_without_delay() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let (_root, manager, leases, _host) = manager("auto_cancel", true, false);
        manager.start().unwrap();
        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Waiting);
        let started = Instant::now();
        manager.stop().unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(manager.get_status().phase, WorkflowPhase::Cancelled);
        assert!(leases.is_empty());
    }

    #[test]
    fn automatic_wait_rejects_manual_followers_without_replacing_worker_handle() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let (_root, manager, _leases, _host) = manager("auto_manual_reject", true, false);
        manager.start().unwrap();
        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Waiting);
        let task_id = manager.get_status().task_id.unwrap();
        let worker_thread = manager
            .lifecycle
            .lock()
            .workflow
            .as_ref()
            .map(|worker| worker.handle.thread().id())
            .unwrap();

        let error = manager.start_followers().unwrap_err();

        assert!(error.contains("自动等待"));
        let lifecycle = manager.lifecycle.lock();
        let worker = lifecycle.workflow.as_ref().unwrap();
        assert_eq!(worker.task_id, task_id);
        assert_eq!(worker.handle.thread().id(), worker_thread);
        drop(lifecycle);
        manager.cancel().unwrap();
        manager.stop().unwrap();
    }

    #[test]
    fn manual_wait_primary_shortcut_confirms_retry_and_consumes_next_name() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let (_root, manager, leases, host) = manager("manual_primary_retry", false, false);
        manager.start().unwrap();
        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Waiting);

        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Waiting);

        assert_eq!(
            *host.primary_calls.lock(),
            [
                ("run-001".to_string(), false),
                ("run-002".to_string(), true),
            ]
        );
        assert_eq!(manager.get_config().config.next_sequence, 3);
        assert!(leases.is_empty());
        manager.cancel().unwrap();
        manager.stop().unwrap();
    }

    #[test]
    fn manual_followers_join_the_visible_primary_worker_before_reusing_its_slot() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let root = TestDirectory::new("manual_worker_handoff");
        let controller = RoomAutomationConfigController::new(&root.0).unwrap();
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let config = RoomAutomationConfig {
            enabled: true,
            primary_account_id: "main".to_string(),
            follower_account_ids: vec!["follower".to_string()],
            shortcut: "Ctrl+Alt+F21".to_string(),
            join_shortcut: "Ctrl+Alt+F22".to_string(),
            ..RoomAutomationConfig::default()
        };
        let snapshot = controller.save(initial.generation, config, &[]).unwrap();
        let (waiting_started_tx, waiting_started_rx) = std::sync::mpsc::sync_channel(1);
        let (release_waiting_tx, release_waiting_rx) = std::sync::mpsc::sync_channel(1);
        let manager = RoomAutomationManager::new(
            controller,
            snapshot,
            AccountLeaseManager::default(),
            Arc::new(FakeHost::new(false)),
            Arc::new(FakeBinding),
            Arc::new(BlockingWaitingBridge {
                waiting_started: Mutex::new(Some(waiting_started_tx)),
                release_waiting: Mutex::new(Some(release_waiting_rx)),
            }),
        );
        manager.start().unwrap();
        manager.start_primary().unwrap();
        waiting_started_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert_eq!(manager.get_status().phase, WorkflowPhase::Waiting);

        let follower_manager = manager.clone();
        let (attempted_tx, attempted_rx) = std::sync::mpsc::sync_channel(1);
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
        let followers = std::thread::spawn(move || {
            let _ = attempted_tx.send(());
            let result = follower_manager.start_followers();
            let _ = finished_tx.send(());
            result
        });
        attempted_rx.recv().unwrap();
        assert!(finished_rx.recv_timeout(Duration::from_millis(40)).is_err());

        release_waiting_tx.send(()).unwrap();
        assert!(followers.join().unwrap().is_ok());
        wait_for_phase(&manager, WorkflowPhase::Complete);
        manager.stop().unwrap();
    }

    #[test]
    fn disabled_legacy_references_stay_editable_without_initialized_accounts() {
        let mut uninitialized = crate::domain::account::AccountMeta::new("Stored-Account");
        uninitialized.initialized = false;
        let accounts = BTreeMap::from([("stored-account".to_string(), uninitialized)]);
        let mut config = RoomAutomationConfig {
            enabled: false,
            primary_account_id: " stored-account ".to_string(),
            follower_account_ids: vec!["missing-account".to_string()],
            ..RoomAutomationConfig::default()
        };

        canonicalize_account_references(&mut config, &accounts).unwrap();

        assert_eq!(config.primary_account_id, "Stored-Account");
        assert!(config.follower_account_ids.is_empty());
        config.validate(std::iter::empty::<&str>()).unwrap();
    }

    #[test]
    fn retry_after_follower_failure_resumes_the_same_room_and_skips_completed_accounts() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let root = TestDirectory::new("follower_retry");
        let controller = RoomAutomationConfigController::new(&root.0).unwrap();
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let config = RoomAutomationConfig {
            enabled: true,
            primary_account_id: "main".to_string(),
            follower_account_ids: vec!["follower-a".to_string(), "follower-b".to_string()],
            next_sequence: 7,
            shortcut: "Ctrl+Alt+F21".to_string(),
            join_shortcut: "Ctrl+Alt+F22".to_string(),
            ..RoomAutomationConfig::default()
        };
        let snapshot = controller.save(initial.generation, config, &[]).unwrap();
        let leases = AccountLeaseManager::default();
        let host = Arc::new(FakeHost::new(false));
        host.fail_follower_once.store(true, Ordering::Release);
        let manager = RoomAutomationManager::new(
            controller,
            snapshot,
            leases.clone(),
            host.clone(),
            Arc::new(FakeBinding),
            Arc::new(FakeBridge::default()),
        );
        manager.start().unwrap();
        manager.start_primary().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Waiting);
        manager.start_followers().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Error);
        let failed = manager.get_status();
        assert_eq!(failed.room_name.as_deref(), Some("run-007"));
        assert_eq!(
            failed.recovery_action,
            Some(WorkflowRecoveryAction::ResumeFollowers)
        );
        assert_eq!(failed.completed_follower_account_ids, ["follower-a"]);
        assert!(leases.is_empty());

        manager.retry().unwrap();
        wait_for_phase(&manager, WorkflowPhase::Complete);
        assert_eq!(manager.get_status().room_name.as_deref(), Some("run-007"));
        assert_eq!(manager.get_config().config.next_sequence, 8);
        let calls = host.follower_calls.lock().clone();
        assert_eq!(
            calls
                .iter()
                .filter(|(account, _)| account == "follower-a")
                .count(),
            1
        );
        assert_eq!(
            calls
                .iter()
                .filter(|(account, _)| account == "follower-b")
                .count(),
            2
        );
        assert!(calls.iter().all(|(_, room)| room == "run-007"));
        manager.stop().unwrap();
    }
}
