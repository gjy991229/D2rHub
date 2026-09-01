//! Shared application model for long-running work.
//!
//! The registry contains no Tauri, thread-pool, filesystem, or Windows code.
//! Adapters decide where work executes and publish snapshots through the
//! observer port. Every task gets one monotonic timeline, cooperative
//! cancellation, conflict serialization, and explicit retry lineage.

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskSnapshot {
    pub revision: u64,
    pub task_id: u64,
    pub kind: String,
    pub subject: Option<String>,
    pub conflict_key: Option<String>,
    pub state: TaskState,
    pub progress: u8,
    pub step: String,
    pub message: String,
    pub error_code: Option<String>,
    pub cancel_requested: bool,
    pub retryable: bool,
    pub retry_of: Option<u64>,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskTimelineEntry {
    pub revision: u64,
    pub timestamp_ms: u64,
    pub state: TaskState,
    pub progress: u8,
    pub step: String,
    pub message: String,
    pub error_code: Option<String>,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRequest {
    pub kind: String,
    pub subject: Option<String>,
    pub conflict_key: Option<String>,
    pub retryable: bool,
    pub retry_of: Option<u64>,
    pub initial_step: String,
    pub initial_message: String,
}

impl TaskRequest {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            subject: None,
            conflict_key: None,
            retryable: true,
            retry_of: None,
            initial_step: "queued".to_string(),
            initial_message: String::new(),
        }
    }

    pub fn for_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    pub fn with_conflict_key(mut self, conflict_key: impl Into<String>) -> Self {
        self.conflict_key = Some(conflict_key.into());
        self
    }

    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn with_initial_status(
        mut self,
        step: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        self.initial_step = step.into();
        self.initial_message = message.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskRuntimeError {
    #[error("任务类型标识无效: {0}")]
    InvalidKind(String),
    #[error("任务冲突键不能为空")]
    EmptyConflictKey,
    #[error("任务 {0} 不存在")]
    NotFound(u64),
    #[error("任务冲突，已有同类操作正在运行: {0}")]
    Conflict(String),
    #[error("任务 {0} 已结束，不能继续更新")]
    AlreadyTerminal(u64),
    #[error("任务进度不能从 {current} 回退到 {requested}")]
    ProgressRegression { current: u8, requested: u8 },
    #[error("任务 {0} 不支持重试")]
    NotRetryable(u64),
    #[error("任务 {0} 尚未结束，不能重试")]
    NotTerminal(u64),
}

pub trait TaskClock: Send + Sync {
    fn now_ms(&self) -> u64;
}

#[derive(Default)]
pub struct SystemTaskClock;

impl TaskClock for SystemTaskClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
}

pub trait TaskObserver: Send + Sync {
    fn task_updated(&self, snapshot: &TaskSnapshot);
}

struct TaskRecord {
    snapshot: TaskSnapshot,
    timeline: Vec<TaskTimelineEntry>,
    cancellation: Arc<AtomicBool>,
}

struct TaskRuntimeInner {
    revision: u64,
    next_task_id: u64,
    records: BTreeMap<u64, TaskRecord>,
}

struct TaskRuntimeShared {
    inner: Mutex<TaskRuntimeInner>,
    observer: RwLock<Option<Arc<dyn TaskObserver>>>,
    clock: Arc<dyn TaskClock>,
    max_completed: usize,
}

#[derive(Clone)]
pub struct TaskRuntime {
    shared: Arc<TaskRuntimeShared>,
}

impl Default for TaskRuntime {
    fn default() -> Self {
        Self::new(100)
    }
}

impl TaskRuntime {
    pub fn new(max_completed: usize) -> Self {
        Self::with_clock(max_completed, Arc::new(SystemTaskClock))
    }

    pub fn with_clock(max_completed: usize, clock: Arc<dyn TaskClock>) -> Self {
        Self {
            shared: Arc::new(TaskRuntimeShared {
                inner: Mutex::new(TaskRuntimeInner {
                    revision: 0,
                    next_task_id: 1,
                    records: BTreeMap::new(),
                }),
                observer: RwLock::new(None),
                clock,
                max_completed,
            }),
        }
    }

    pub fn set_observer(&self, observer: Option<Arc<dyn TaskObserver>>) {
        *self.shared.observer.write() = observer;
    }

    pub fn begin(&self, mut request: TaskRequest) -> Result<TaskHandle, TaskRuntimeError> {
        request.kind = normalize_kind(&request.kind)?;
        request.subject = normalize_optional(request.subject);
        request.conflict_key = normalize_conflict_key(request.conflict_key)?;
        request.initial_step = request.initial_step.trim().to_string();
        request.initial_message = request.initial_message.trim().to_string();

        let timestamp_ms = self.shared.clock.now_ms();
        let (snapshot, cancellation) = {
            let mut inner = self.shared.inner.lock();
            if let Some(conflict_key) = request.conflict_key.as_deref() {
                if inner.records.values().any(|record| {
                    !record.snapshot.state.is_terminal()
                        && record.snapshot.conflict_key.as_deref() == Some(conflict_key)
                }) {
                    return Err(TaskRuntimeError::Conflict(conflict_key.to_string()));
                }
            }
            prune_completed(&mut inner.records, self.shared.max_completed);
            let task_id = inner.next_task_id;
            inner.next_task_id = inner.next_task_id.saturating_add(1);
            inner.revision = inner.revision.saturating_add(1);
            let snapshot = TaskSnapshot {
                revision: inner.revision,
                task_id,
                kind: request.kind,
                subject: request.subject,
                conflict_key: request.conflict_key,
                state: TaskState::Running,
                progress: 0,
                step: request.initial_step,
                message: request.initial_message,
                error_code: None,
                cancel_requested: false,
                retryable: request.retryable,
                retry_of: request.retry_of,
                started_at_ms: timestamp_ms,
                finished_at_ms: None,
            };
            let timeline = vec![timeline_from(&snapshot, timestamp_ms)];
            let cancellation = Arc::new(AtomicBool::new(false));
            inner.records.insert(
                task_id,
                TaskRecord {
                    snapshot: snapshot.clone(),
                    timeline,
                    cancellation: Arc::clone(&cancellation),
                },
            );
            (snapshot, cancellation)
        };
        self.publish(&snapshot);
        Ok(TaskHandle {
            runtime: self.clone(),
            task_id: snapshot.task_id,
            cancellation,
            terminal: false,
        })
    }

    pub fn snapshots(&self) -> Vec<TaskSnapshot> {
        self.shared
            .inner
            .lock()
            .records
            .values()
            .map(|record| record.snapshot.clone())
            .collect()
    }

    pub fn snapshot(&self, task_id: u64) -> Option<TaskSnapshot> {
        self.shared
            .inner
            .lock()
            .records
            .get(&task_id)
            .map(|record| record.snapshot.clone())
    }

    pub fn timeline(&self, task_id: u64) -> Result<Vec<TaskTimelineEntry>, TaskRuntimeError> {
        self.shared
            .inner
            .lock()
            .records
            .get(&task_id)
            .map(|record| record.timeline.clone())
            .ok_or(TaskRuntimeError::NotFound(task_id))
    }

    pub fn request_cancel(&self, task_id: u64) -> Result<TaskSnapshot, TaskRuntimeError> {
        let timestamp_ms = self.shared.clock.now_ms();
        let snapshot = {
            let mut inner = self.shared.inner.lock();
            let current = inner
                .records
                .get(&task_id)
                .ok_or(TaskRuntimeError::NotFound(task_id))?;
            if current.snapshot.state.is_terminal() {
                return Err(TaskRuntimeError::AlreadyTerminal(task_id));
            }
            current.cancellation.store(true, Ordering::Release);
            inner.revision = inner.revision.saturating_add(1);
            let revision = inner.revision;
            let record = inner.records.get_mut(&task_id).expect("task was checked");
            record.snapshot.revision = revision;
            record.snapshot.cancel_requested = true;
            record.snapshot.step = "cancelling".to_string();
            if record.snapshot.message.is_empty() {
                record.snapshot.message = "取消请求已提交".to_string();
            }
            record
                .timeline
                .push(timeline_from(&record.snapshot, timestamp_ms));
            record.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub fn retry_request(&self, task_id: u64) -> Result<TaskRequest, TaskRuntimeError> {
        let inner = self.shared.inner.lock();
        let record = inner
            .records
            .get(&task_id)
            .ok_or(TaskRuntimeError::NotFound(task_id))?;
        if !record.snapshot.state.is_terminal() {
            return Err(TaskRuntimeError::NotTerminal(task_id));
        }
        if !record.snapshot.retryable || record.snapshot.state == TaskState::Succeeded {
            return Err(TaskRuntimeError::NotRetryable(task_id));
        }
        Ok(TaskRequest {
            kind: record.snapshot.kind.clone(),
            subject: record.snapshot.subject.clone(),
            conflict_key: record.snapshot.conflict_key.clone(),
            retryable: true,
            retry_of: Some(task_id),
            initial_step: "retrying".to_string(),
            initial_message: "正在重试".to_string(),
        })
    }

    fn update(
        &self,
        task_id: u64,
        progress: u8,
        step: &str,
        message: &str,
    ) -> Result<TaskSnapshot, TaskRuntimeError> {
        let timestamp_ms = self.shared.clock.now_ms();
        let snapshot = {
            let mut inner = self.shared.inner.lock();
            let current = inner
                .records
                .get(&task_id)
                .ok_or(TaskRuntimeError::NotFound(task_id))?;
            if current.snapshot.state.is_terminal() {
                return Err(TaskRuntimeError::AlreadyTerminal(task_id));
            }
            if progress < current.snapshot.progress {
                return Err(TaskRuntimeError::ProgressRegression {
                    current: current.snapshot.progress,
                    requested: progress,
                });
            }
            inner.revision = inner.revision.saturating_add(1);
            let revision = inner.revision;
            let record = inner.records.get_mut(&task_id).expect("task was checked");
            record.snapshot.revision = revision;
            record.snapshot.progress = progress.min(99);
            record.snapshot.step = step.trim().to_string();
            record.snapshot.message = message.trim().to_string();
            record
                .timeline
                .push(timeline_from(&record.snapshot, timestamp_ms));
            record.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    fn finish(
        &self,
        task_id: u64,
        state: TaskState,
        error_code: Option<&str>,
        message: &str,
    ) -> Result<TaskSnapshot, TaskRuntimeError> {
        debug_assert!(state.is_terminal());
        let timestamp_ms = self.shared.clock.now_ms();
        let snapshot = {
            let mut inner = self.shared.inner.lock();
            let current = inner
                .records
                .get(&task_id)
                .ok_or(TaskRuntimeError::NotFound(task_id))?;
            if current.snapshot.state.is_terminal() {
                return Err(TaskRuntimeError::AlreadyTerminal(task_id));
            }
            inner.revision = inner.revision.saturating_add(1);
            let revision = inner.revision;
            let record = inner.records.get_mut(&task_id).expect("task was checked");
            record.snapshot.revision = revision;
            record.snapshot.state = state;
            record.snapshot.progress = if state == TaskState::Succeeded {
                100
            } else {
                record.snapshot.progress
            };
            record.snapshot.step = match state {
                TaskState::Succeeded => "completed",
                TaskState::Failed => "failed",
                TaskState::Cancelled => "cancelled",
                TaskState::Running => unreachable!(),
            }
            .to_string();
            record.snapshot.message = message.trim().to_string();
            record.snapshot.error_code = error_code.map(str::to_string);
            record.snapshot.finished_at_ms = Some(timestamp_ms);
            record
                .timeline
                .push(timeline_from(&record.snapshot, timestamp_ms));
            record.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    fn publish(&self, snapshot: &TaskSnapshot) {
        let observer = self.shared.observer.read().clone();
        if let Some(observer) = observer {
            observer.task_updated(snapshot);
        }
    }
}

pub struct TaskHandle {
    runtime: TaskRuntime,
    task_id: u64,
    cancellation: Arc<AtomicBool>,
    terminal: bool,
}

impl TaskHandle {
    pub fn task_id(&self) -> u64 {
        self.task_id
    }

    pub fn cancellation_requested(&self) -> bool {
        self.cancellation.load(Ordering::Acquire)
    }

    pub fn update(
        &self,
        progress: u8,
        step: &str,
        message: &str,
    ) -> Result<TaskSnapshot, TaskRuntimeError> {
        self.runtime.update(self.task_id, progress, step, message)
    }

    pub fn succeed(mut self, message: &str) -> Result<TaskSnapshot, TaskRuntimeError> {
        self.terminal = true;
        self.runtime
            .finish(self.task_id, TaskState::Succeeded, None, message)
    }

    pub fn fail(
        mut self,
        error_code: &str,
        message: &str,
    ) -> Result<TaskSnapshot, TaskRuntimeError> {
        self.terminal = true;
        self.runtime
            .finish(self.task_id, TaskState::Failed, Some(error_code), message)
    }

    pub fn cancelled(mut self, message: &str) -> Result<TaskSnapshot, TaskRuntimeError> {
        self.terminal = true;
        self.runtime
            .finish(self.task_id, TaskState::Cancelled, None, message)
    }
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        if !self.terminal {
            let _ = self.runtime.finish(
                self.task_id,
                TaskState::Failed,
                Some("task-abandoned"),
                "任务执行器在完成前退出",
            );
        }
    }
}

fn normalize_kind(kind: &str) -> Result<String, TaskRuntimeError> {
    let kind = kind.trim();
    if kind.is_empty()
        || kind.len() > 64
        || !kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || kind.starts_with('-')
        || kind.ends_with('-')
    {
        return Err(TaskRuntimeError::InvalidKind(kind.to_string()));
    }
    Ok(kind.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_conflict_key(value: Option<String>) -> Result<Option<String>, TaskRuntimeError> {
    match value {
        None => Ok(None),
        Some(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                Err(TaskRuntimeError::EmptyConflictKey)
            } else {
                Ok(Some(normalized))
            }
        }
    }
}

fn timeline_from(snapshot: &TaskSnapshot, timestamp_ms: u64) -> TaskTimelineEntry {
    TaskTimelineEntry {
        revision: snapshot.revision,
        timestamp_ms,
        state: snapshot.state,
        progress: snapshot.progress,
        step: snapshot.step.clone(),
        message: snapshot.message.clone(),
        error_code: snapshot.error_code.clone(),
        cancel_requested: snapshot.cancel_requested,
    }
}

fn prune_completed(records: &mut BTreeMap<u64, TaskRecord>, max_completed: usize) {
    let completed = records
        .iter()
        .filter(|(_, record)| record.snapshot.state.is_terminal())
        .map(|(task_id, _)| *task_id)
        .collect::<Vec<_>>();
    let remove_count = completed.len().saturating_sub(max_completed);
    for task_id in completed.into_iter().take(remove_count) {
        records.remove(&task_id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use super::{
        TaskClock, TaskObserver, TaskRequest, TaskRuntime, TaskRuntimeError, TaskSnapshot,
        TaskState,
    };

    #[derive(Default)]
    struct FakeClock(AtomicU64);

    impl TaskClock for FakeClock {
        fn now_ms(&self) -> u64 {
            self.0.fetch_add(10, Ordering::SeqCst) + 10
        }
    }

    #[derive(Default)]
    struct RecordingObserver(Mutex<Vec<TaskSnapshot>>);

    impl TaskObserver for RecordingObserver {
        fn task_updated(&self, snapshot: &TaskSnapshot) {
            self.0.lock().unwrap().push(snapshot.clone());
        }
    }

    fn runtime(max_completed: usize) -> TaskRuntime {
        TaskRuntime::with_clock(max_completed, Arc::new(FakeClock::default()))
    }

    #[test]
    fn progress_revisions_are_monotonic_and_terminal_state_is_immutable() {
        let runtime = runtime(10);
        let observer = Arc::new(RecordingObserver::default());
        runtime.set_observer(Some(observer.clone()));
        let handle = runtime
            .begin(
                TaskRequest::new("account-launch")
                    .for_subject("account-a")
                    .with_initial_status("preflight", "准备启动"),
            )
            .unwrap();
        handle.update(25, "prepare", "准备环境").unwrap();
        assert!(matches!(
            handle.update(24, "prepare", "bad"),
            Err(TaskRuntimeError::ProgressRegression { .. })
        ));
        let completed = handle.succeed("启动完成").unwrap();

        assert_eq!(completed.state, TaskState::Succeeded);
        assert_eq!(completed.progress, 100);
        let timeline = runtime.timeline(completed.task_id).unwrap();
        assert_eq!(timeline.len(), 3);
        assert!(timeline
            .windows(2)
            .all(|pair| pair[0].revision < pair[1].revision));
        assert_eq!(observer.0.lock().unwrap().len(), 3);
        assert!(matches!(
            runtime.request_cancel(completed.task_id),
            Err(TaskRuntimeError::AlreadyTerminal(_))
        ));
    }

    #[test]
    fn conflict_keys_serialize_related_tasks_and_release_after_failure() {
        let runtime = runtime(10);
        let first = runtime
            .begin(TaskRequest::new("account-reset").with_conflict_key(" Account:A "))
            .unwrap();
        assert!(matches!(
            runtime.begin(TaskRequest::new("account-reset").with_conflict_key("account:a")),
            Err(TaskRuntimeError::Conflict(_))
        ));
        first.fail("reset-failed", "reset failed").unwrap();
        let second = runtime
            .begin(TaskRequest::new("account-reset").with_conflict_key("account:a"))
            .unwrap();
        second.succeed("done").unwrap();
    }

    #[test]
    fn cancellation_is_cooperative_and_retry_keeps_lineage() {
        let runtime = runtime(10);
        let handle = runtime
            .begin(TaskRequest::new("mod-install").for_subject("audio"))
            .unwrap();
        let task_id = handle.task_id();
        let requested = runtime.request_cancel(task_id).unwrap();
        assert!(requested.cancel_requested);
        assert!(handle.cancellation_requested());
        handle.cancelled("cancelled").unwrap();

        let retry = runtime.retry_request(task_id).unwrap();
        assert_eq!(retry.retry_of, Some(task_id));
        let retry_handle = runtime.begin(retry).unwrap();
        assert_eq!(
            runtime.snapshot(retry_handle.task_id()).unwrap().retry_of,
            Some(task_id)
        );
        retry_handle.fail("still-failed", "failed again").unwrap();
    }

    #[test]
    fn abandoned_tasks_fail_closed_and_completed_history_is_bounded() {
        let runtime = runtime(1);
        let abandoned_id = {
            let handle = runtime.begin(TaskRequest::new("path-scan")).unwrap();
            handle.task_id()
        };
        assert_eq!(
            runtime
                .snapshot(abandoned_id)
                .unwrap()
                .error_code
                .as_deref(),
            Some("task-abandoned")
        );
        runtime
            .begin(TaskRequest::new("path-scan"))
            .unwrap()
            .succeed("done")
            .unwrap();
        let active = runtime.begin(TaskRequest::new("path-scan")).unwrap();

        assert_eq!(runtime.snapshots().len(), 2);
        assert!(runtime.snapshot(abandoned_id).is_none());
        drop(active);
    }

    #[test]
    fn identifiers_and_retry_policy_fail_closed() {
        let runtime = runtime(10);
        assert!(runtime.begin(TaskRequest::new("Bad Kind")).is_err());
        assert!(runtime
            .begin(TaskRequest::new("valid").with_conflict_key("  "))
            .is_err());
        let succeeded = runtime
            .begin(TaskRequest::new("valid"))
            .unwrap()
            .succeed("done")
            .unwrap();
        assert!(matches!(
            runtime.retry_request(succeeded.task_id),
            Err(TaskRuntimeError::NotRetryable(_))
        ));
    }
}
