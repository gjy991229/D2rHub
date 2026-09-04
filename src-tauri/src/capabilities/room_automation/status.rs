use super::model::{RoomAutomationConfig, RoomAutomationConfigError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPhase {
    #[default]
    Idle,
    Primary,
    Waiting,
    Followers,
    Complete,
    Cancelled,
    Error,
}

impl WorkflowPhase {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Complete | Self::Cancelled | Self::Error
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum WaitingMode {
    Manual,
    Automatic { delay_secs: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRecoveryAction {
    RetryPrimary,
    ResumeFollowers,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingRoom {
    pub name: String,
    pub sequence: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkflowTaskId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowStatus {
    pub revision: u64,
    pub task_id: Option<WorkflowTaskId>,
    pub running: bool,
    pub phase: WorkflowPhase,
    pub waiting_mode: Option<WaitingMode>,
    pub room_name: Option<String>,
    pub room_sequence: Option<u32>,
    pub attempt: u32,
    pub primary_account_id: Option<String>,
    pub follower_account_ids: Vec<String>,
    /// Accounts whose native command attempt has finished. This does not
    /// assert that the game joined or finished loading the room.
    pub completed_follower_account_ids: Vec<String>,
    /// Accounts whose native join command could not be delivered. This is
    /// diagnostic only: interval dispatch continues on its fixed schedule.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undelivered_follower_account_ids: Vec<String>,
    /// Timestamp is supplied by the application adapter. The pure state
    /// machine never reads a clock.
    pub started_at: Option<String>,
    pub last_error: Option<String>,
    /// Explicit recovery semantics survive the terminal `error/cancelled`
    /// phase, where the previous execution stage would otherwise be lost.
    pub recovery_action: Option<WorkflowRecoveryAction>,
}

impl Default for WorkflowStatus {
    fn default() -> Self {
        Self {
            revision: 0,
            task_id: None,
            running: false,
            phase: WorkflowPhase::Idle,
            waiting_mode: None,
            room_name: None,
            room_sequence: None,
            attempt: 0,
            primary_account_id: None,
            follower_account_ids: Vec::new(),
            completed_follower_account_ids: Vec::new(),
            undelivered_follower_account_ids: Vec::new(),
            started_at: None,
            last_error: None,
            recovery_action: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimaryTask {
    pub id: WorkflowTaskId,
    pub room: PendingRoom,
    pub retrying: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowersTask {
    pub id: WorkflowTaskId,
    pub room: PendingRoom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorkflowTaskState {
    status: WorkflowStatus,
    pending_room: Option<PendingRoom>,
    /// Passwords stay runtime-private and never enter the published status.
    #[serde(skip)]
    pending_password: Option<String>,
    #[serde(skip)]
    primary_password: Option<String>,
    next_task_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkflowStateError {
    #[error("room automation is disabled")]
    ConfigDisabled,
    #[error(transparent)]
    InvalidConfig(#[from] RoomAutomationConfigError),
    #[error("a room workflow is already running")]
    Busy,
    #[error("workflow task id space is exhausted")]
    TaskIdExhausted,
    #[error("workflow status revision space is exhausted")]
    RevisionExhausted,
    #[error("room sequence space is exhausted")]
    SequenceExhausted,
    #[error("task {actual:?} is stale; active task is {expected:?}")]
    StaleTask {
        expected: Option<WorkflowTaskId>,
        actual: WorkflowTaskId,
    },
    #[error("cannot perform {operation} while workflow is in {phase:?}")]
    InvalidTransition {
        operation: &'static str,
        phase: WorkflowPhase,
    },
    #[error("workflow has no pending room")]
    MissingPendingRoom,
    #[error("follower account {0:?} is not part of this task")]
    UnknownFollower(String),
    #[error("workflow error message is empty")]
    EmptyError,
}

impl WorkflowTaskState {
    pub fn status(&self) -> &WorkflowStatus {
        &self.status
    }

    pub fn snapshot(&self) -> WorkflowStatus {
        self.status.clone()
    }

    pub fn pending_room(&self) -> Option<&PendingRoom> {
        self.pending_room.as_ref()
    }

    pub fn pending_password(&self) -> Option<&str> {
        self.pending_password.as_deref()
    }

    /// Starts a primary task. A second primary shortcut during manual waiting
    /// supersedes the pending room and consumes the next durable sequence.
    /// Automatic waiting remains worker-owned.
    pub fn begin_primary(
        &mut self,
        config: &RoomAutomationConfig,
        started_at: Option<String>,
    ) -> Result<PrimaryTask, WorkflowStateError> {
        if !config.enabled {
            return Err(WorkflowStateError::ConfigDisabled);
        }
        config.validate_for_activation(std::iter::empty())?;
        if self.status.running {
            return Err(WorkflowStateError::Busy);
        }
        let primary_allowed = matches!(
            self.status.phase,
            WorkflowPhase::Idle
                | WorkflowPhase::Complete
                | WorkflowPhase::Error
                | WorkflowPhase::Cancelled
        ) || (self.status.phase == WorkflowPhase::Waiting
            && self.status.waiting_mode == Some(WaitingMode::Manual));
        if !primary_allowed {
            return Err(WorkflowStateError::InvalidTransition {
                operation: "begin primary",
                phase: self.status.phase,
            });
        }

        // `retrying` is retained as an internal supersession marker for task
        // history. Native input no longer treats it as a duplicate-dialog
        // confirmation: every shortcut opens a fresh create form.
        let retrying = self.status.phase == WorkflowPhase::Waiting
            && self.status.waiting_mode == Some(WaitingMode::Manual)
            && self.pending_room.is_some();
        let sequence = if retrying {
            let room = self
                .pending_room
                .as_ref()
                .ok_or(WorkflowStateError::MissingPendingRoom)?;
            config.next_sequence.max(
                room.sequence
                    .checked_add(1)
                    .ok_or(WorkflowStateError::SequenceExhausted)?,
            )
        } else {
            config.next_sequence
        };
        let room = PendingRoom {
            name: config.generate_room_name(sequence)?,
            sequence,
        };
        let task_id = self.reserve_task_id()?;
        let revision = self.next_revision()?;

        self.next_task_id = task_id.0;
        self.status = WorkflowStatus {
            revision,
            task_id: Some(task_id),
            running: true,
            phase: WorkflowPhase::Primary,
            waiting_mode: None,
            room_name: Some(room.name.clone()),
            room_sequence: Some(room.sequence),
            attempt: 1,
            primary_account_id: Some(config.primary_account_id.clone()),
            follower_account_ids: config.follower_account_ids.clone(),
            completed_follower_account_ids: Vec::new(),
            undelivered_follower_account_ids: Vec::new(),
            started_at,
            last_error: None,
            recovery_action: None,
        };
        self.primary_password = Some(config.password.clone());

        Ok(PrimaryTask {
            id: task_id,
            room,
            retrying,
        })
    }

    /// Commits the room produced by the primary task and enters the waiting
    /// phase. Manual waiting is observable but not actively running.
    pub fn primary_ready(
        &mut self,
        task_id: WorkflowTaskId,
        mode: WaitingMode,
    ) -> Result<WorkflowStatus, WorkflowStateError> {
        self.require_task_phase(task_id, WorkflowPhase::Primary, "mark primary ready")?;
        let room = PendingRoom {
            name: self
                .status
                .room_name
                .clone()
                .ok_or(WorkflowStateError::MissingPendingRoom)?,
            sequence: self
                .status
                .room_sequence
                .ok_or(WorkflowStateError::MissingPendingRoom)?,
        };
        let revision = self.next_revision()?;
        let password = self
            .primary_password
            .take()
            .ok_or(WorkflowStateError::MissingPendingRoom)?;

        self.pending_room = Some(room);
        self.pending_password = Some(password);
        self.status.revision = revision;
        self.status.phase = WorkflowPhase::Waiting;
        self.status.running = matches!(mode, WaitingMode::Automatic { .. });
        self.status.waiting_mode = Some(mode);
        self.status.last_error = None;
        self.status.recovery_action = None;
        Ok(self.snapshot())
    }

    #[allow(dead_code)] // retained as the all-configured-followers state-machine entry point
    pub fn begin_followers(
        &mut self,
        task_id: WorkflowTaskId,
    ) -> Result<WorkflowStatus, WorkflowStateError> {
        let follower_account_ids = self.status.follower_account_ids.clone();
        self.begin_selected_followers(task_id, follower_account_ids)
    }

    /// Starts the follower stage for the configured accounts that currently
    /// have usable game windows. Offline accounts remain configured for later
    /// rooms but do not keep this task from reaching `complete`.
    pub fn begin_selected_followers(
        &mut self,
        task_id: WorkflowTaskId,
        follower_account_ids: Vec<String>,
    ) -> Result<WorkflowStatus, WorkflowStateError> {
        self.require_task_phase(task_id, WorkflowPhase::Waiting, "begin followers")?;
        if self.pending_room.is_none() {
            return Err(WorkflowStateError::MissingPendingRoom);
        }
        if follower_account_ids.is_empty() {
            return Err(RoomAutomationConfigError::MissingFollowerAccount.into());
        }
        if let Some(unknown) = follower_account_ids.iter().find(|account_id| {
            !self
                .status
                .follower_account_ids
                .iter()
                .any(|configured| configured.eq_ignore_ascii_case(account_id))
        }) {
            return Err(WorkflowStateError::UnknownFollower(unknown.clone()));
        }
        let revision = self.next_revision()?;

        self.status.revision = revision;
        self.status.phase = WorkflowPhase::Followers;
        self.status.running = true;
        self.status.waiting_mode = None;
        self.status.follower_account_ids = follower_account_ids;
        self.status.completed_follower_account_ids.clear();
        self.status.undelivered_follower_account_ids.clear();
        self.status.last_error = None;
        self.status.recovery_action = None;
        Ok(self.snapshot())
    }

    /// Starts a fresh follower task for a room retained after a cancellation
    /// or follower failure. Manual waiting keeps using [`Self::begin_followers`]
    /// so its original task identity remains stable.
    pub fn resume_followers(
        &mut self,
        config: &RoomAutomationConfig,
        started_at: Option<String>,
    ) -> Result<FollowersTask, WorkflowStateError> {
        if !config.enabled {
            return Err(WorkflowStateError::ConfigDisabled);
        }
        config.validate_for_activation(std::iter::empty())?;
        if self.status.running {
            return Err(WorkflowStateError::Busy);
        }
        if !matches!(
            self.status.phase,
            WorkflowPhase::Error | WorkflowPhase::Cancelled
        ) || self.status.recovery_action != Some(WorkflowRecoveryAction::ResumeFollowers)
        {
            return Err(WorkflowStateError::InvalidTransition {
                operation: "resume followers",
                phase: self.status.phase,
            });
        }
        let room = self
            .pending_room
            .clone()
            .ok_or(WorkflowStateError::MissingPendingRoom)?;
        let task_id = self.reserve_task_id()?;
        let revision = self.next_revision()?;

        self.next_task_id = task_id.0;
        self.status = WorkflowStatus {
            revision,
            task_id: Some(task_id),
            running: true,
            phase: WorkflowPhase::Followers,
            waiting_mode: None,
            room_name: Some(room.name.clone()),
            room_sequence: Some(room.sequence),
            attempt: 1,
            primary_account_id: Some(config.primary_account_id.clone()),
            follower_account_ids: config.follower_account_ids.clone(),
            completed_follower_account_ids: Vec::new(),
            undelivered_follower_account_ids: Vec::new(),
            started_at,
            last_error: None,
            recovery_action: None,
        };

        Ok(FollowersTask { id: task_id, room })
    }

    /// Records one idempotent follower completion. The final follower moves
    /// the task to `complete` and consumes the pending room.
    pub fn record_follower_complete(
        &mut self,
        task_id: WorkflowTaskId,
        account_id: &str,
    ) -> Result<WorkflowStatus, WorkflowStateError> {
        self.record_follower_dispatch(task_id, account_id, true)
    }

    /// Records completion of one native command attempt. `delivered` only
    /// describes whether the window message sequence was sent; it must never
    /// be interpreted as proof that the account entered or finished loading
    /// the room.
    pub fn record_follower_dispatch(
        &mut self,
        task_id: WorkflowTaskId,
        account_id: &str,
        delivered: bool,
    ) -> Result<WorkflowStatus, WorkflowStateError> {
        self.require_task_phase(
            task_id,
            WorkflowPhase::Followers,
            "record follower dispatch",
        )?;
        if !self
            .status
            .follower_account_ids
            .iter()
            .any(|candidate| candidate == account_id)
        {
            return Err(WorkflowStateError::UnknownFollower(account_id.to_string()));
        }
        if self
            .status
            .completed_follower_account_ids
            .iter()
            .any(|candidate| candidate == account_id)
        {
            return Ok(self.snapshot());
        }
        let revision = self.next_revision()?;

        self.status.revision = revision;
        if !delivered {
            self.status
                .undelivered_follower_account_ids
                .push(account_id.to_string());
        }
        self.status
            .completed_follower_account_ids
            .push(account_id.to_string());
        if self.status.completed_follower_account_ids.len()
            == self.status.follower_account_ids.len()
        {
            self.finish_complete();
        }
        Ok(self.snapshot())
    }

    pub fn cancel(
        &mut self,
        task_id: WorkflowTaskId,
    ) -> Result<WorkflowStatus, WorkflowStateError> {
        self.require_task(task_id)?;
        if !matches!(
            self.status.phase,
            WorkflowPhase::Primary | WorkflowPhase::Waiting | WorkflowPhase::Followers
        ) {
            return Err(WorkflowStateError::InvalidTransition {
                operation: "cancel",
                phase: self.status.phase,
            });
        }
        let revision = self.next_revision()?;
        let recovery_action = match self.status.phase {
            WorkflowPhase::Primary => WorkflowRecoveryAction::RetryPrimary,
            WorkflowPhase::Waiting | WorkflowPhase::Followers => {
                WorkflowRecoveryAction::ResumeFollowers
            }
            _ => unreachable!("phase was validated above"),
        };

        self.status.revision = revision;
        self.status.phase = WorkflowPhase::Cancelled;
        self.status.running = false;
        self.status.waiting_mode = None;
        self.status.last_error = None;
        self.status.recovery_action = Some(recovery_action);
        Ok(self.snapshot())
    }

    /// Marks the active task as failed. A committed pending room is retained so
    /// follower delivery or primary retry remains possible.
    pub fn fail(
        &mut self,
        task_id: WorkflowTaskId,
        error: impl Into<String>,
    ) -> Result<WorkflowStatus, WorkflowStateError> {
        self.require_task(task_id)?;
        if self.status.phase.is_terminal() {
            return Err(WorkflowStateError::InvalidTransition {
                operation: "fail",
                phase: self.status.phase,
            });
        }
        let error = error.into().trim().to_string();
        if error.is_empty() {
            return Err(WorkflowStateError::EmptyError);
        }
        let revision = self.next_revision()?;
        let recovery_action = match self.status.phase {
            WorkflowPhase::Primary => WorkflowRecoveryAction::RetryPrimary,
            WorkflowPhase::Waiting | WorkflowPhase::Followers => {
                WorkflowRecoveryAction::ResumeFollowers
            }
            _ => unreachable!("terminal phases were rejected above"),
        };

        self.status.revision = revision;
        self.status.phase = WorkflowPhase::Error;
        self.status.running = false;
        self.status.waiting_mode = None;
        self.status.last_error = Some(error);
        self.status.recovery_action = Some(recovery_action);
        Ok(self.snapshot())
    }

    fn finish_complete(&mut self) {
        self.status.phase = WorkflowPhase::Complete;
        self.status.running = false;
        self.status.waiting_mode = None;
        self.status.last_error = None;
        self.status.recovery_action = None;
        self.pending_room = None;
        self.pending_password = None;
        self.primary_password = None;
    }

    fn require_task(&self, actual: WorkflowTaskId) -> Result<(), WorkflowStateError> {
        if self.status.task_id != Some(actual) {
            return Err(WorkflowStateError::StaleTask {
                expected: self.status.task_id,
                actual,
            });
        }
        Ok(())
    }

    fn require_task_phase(
        &self,
        task_id: WorkflowTaskId,
        phase: WorkflowPhase,
        operation: &'static str,
    ) -> Result<(), WorkflowStateError> {
        self.require_task(task_id)?;
        if self.status.phase != phase {
            return Err(WorkflowStateError::InvalidTransition {
                operation,
                phase: self.status.phase,
            });
        }
        Ok(())
    }

    fn reserve_task_id(&self) -> Result<WorkflowTaskId, WorkflowStateError> {
        self.next_task_id
            .checked_add(1)
            .map(WorkflowTaskId)
            .ok_or(WorkflowStateError::TaskIdExhausted)
    }

    fn next_revision(&self) -> Result<u64, WorkflowStateError> {
        self.status
            .revision
            .checked_add(1)
            .ok_or(WorkflowStateError::RevisionExhausted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_workflow_preserves_v16_primary_then_followers_behavior() {
        let config = enabled_config();
        let mut state = WorkflowTaskState::default();

        let primary = state
            .begin_primary(&config, Some("2026-09-01T12:00:00+08:00".to_string()))
            .unwrap();
        assert_eq!(primary.room.name, "run-001");
        assert_eq!(state.status().phase, WorkflowPhase::Primary);
        assert!(state.status().running);

        state
            .primary_ready(primary.id, WaitingMode::Manual)
            .unwrap();
        assert_eq!(state.status().phase, WorkflowPhase::Waiting);
        assert!(!state.status().running);
        assert_eq!(state.pending_room(), Some(&primary.room));

        state.begin_followers(primary.id).unwrap();
        state.record_follower_complete(primary.id, "one").unwrap();
        assert_eq!(state.status().phase, WorkflowPhase::Followers);
        state.record_follower_complete(primary.id, "two").unwrap();
        assert_eq!(state.status().phase, WorkflowPhase::Complete);
        assert!(!state.status().running);
        assert!(state.pending_room().is_none());
    }

    #[test]
    fn automatic_waiting_remains_running_until_followers_begin() {
        let config = enabled_config();
        let mut state = WorkflowTaskState::default();
        let task = state.begin_primary(&config, None).unwrap();

        state
            .primary_ready(task.id, WaitingMode::Automatic { delay_secs: 5 })
            .unwrap();

        assert_eq!(state.status().phase, WorkflowPhase::Waiting);
        assert!(state.status().running);
        assert_eq!(
            state.status().waiting_mode,
            Some(WaitingMode::Automatic { delay_secs: 5 })
        );
    }

    #[test]
    fn revisions_are_strictly_monotonic_for_observable_mutations() {
        let config = enabled_config();
        let mut state = WorkflowTaskState::default();
        let idle_revision = state.status().revision;
        let task = state.begin_primary(&config, None).unwrap();
        let primary_revision = state.status().revision;
        state.primary_ready(task.id, WaitingMode::Manual).unwrap();
        let waiting_revision = state.status().revision;
        state.begin_followers(task.id).unwrap();
        let followers_revision = state.status().revision;
        state.record_follower_complete(task.id, "one").unwrap();
        let partial_revision = state.status().revision;
        state.record_follower_complete(task.id, "one").unwrap();

        assert!(idle_revision < primary_revision);
        assert!(primary_revision < waiting_revision);
        assert!(waiting_revision < followers_revision);
        assert!(followers_revision < partial_revision);
        assert_eq!(state.status().revision, partial_revision);
    }

    #[test]
    fn stale_task_cannot_overwrite_a_newer_run() {
        let config = enabled_config();
        let mut state = WorkflowTaskState::default();
        let old = state.begin_primary(&config, None).unwrap();
        state.cancel(old.id).unwrap();
        let current = state.begin_primary(&config, None).unwrap();

        assert!(matches!(
            state.fail(old.id, "late failure"),
            Err(WorkflowStateError::StaleTask { .. })
        ));
        assert_eq!(state.status().task_id, Some(current.id));
        assert_eq!(state.status().phase, WorkflowPhase::Primary);
    }

    #[test]
    fn manual_waiting_primary_retries_with_the_next_durable_sequence() {
        let mut config = enabled_config();
        let mut state = WorkflowTaskState::default();
        let first = state.begin_primary(&config, None).unwrap();
        state.primary_ready(first.id, WaitingMode::Manual).unwrap();
        config.next_sequence = 2;

        let retry = state.begin_primary(&config, None).unwrap();
        assert!(retry.retrying);
        assert_eq!(retry.room.name, "run-002");
        assert!(retry.id > first.id);
    }

    #[test]
    fn automatic_waiting_rejects_manual_primary_takeover() {
        let config = enabled_config();
        let mut state = WorkflowTaskState::default();
        let first = state.begin_primary(&config, None).unwrap();
        state
            .primary_ready(first.id, WaitingMode::Automatic { delay_secs: 5 })
            .unwrap();

        assert_eq!(
            state.begin_primary(&config, None),
            Err(WorkflowStateError::Busy)
        );
    }

    #[test]
    fn primary_failure_reopens_the_form_with_the_next_durable_sequence() {
        let mut config = enabled_config();
        let mut state = WorkflowTaskState::default();
        let first = state.begin_primary(&config, None).unwrap();
        state.fail(first.id, "primary input failed").unwrap();
        assert_eq!(
            state.status().recovery_action,
            Some(WorkflowRecoveryAction::RetryPrimary)
        );
        assert!(state.resume_followers(&config, None).is_err());

        config.next_sequence = 2;
        let retried = state.begin_primary(&config, None).unwrap();
        assert_eq!(retried.room.name, "run-002");
        assert!(!retried.retrying);
    }

    #[test]
    fn follower_failure_keeps_pending_room_for_manual_retry() {
        let config = enabled_config();
        let mut state = WorkflowTaskState::default();
        let task = state.begin_primary(&config, None).unwrap();
        state.primary_ready(task.id, WaitingMode::Manual).unwrap();
        state.begin_followers(task.id).unwrap();

        state.fail(task.id, "one follower failed").unwrap();

        assert_eq!(state.status().phase, WorkflowPhase::Error);
        assert_eq!(state.pending_room(), Some(&task.room));
        assert_eq!(
            state.status().recovery_action,
            Some(WorkflowRecoveryAction::ResumeFollowers)
        );

        let resumed = state
            .resume_followers(&config, Some("2026-09-01T13:00:00+08:00".to_string()))
            .unwrap();
        assert!(resumed.id > task.id);
        assert_eq!(resumed.room, task.room);
        assert_eq!(state.status().phase, WorkflowPhase::Followers);
        assert!(state.status().running);
        assert_eq!(state.status().recovery_action, None);
    }

    #[test]
    fn follower_failure_can_be_abandoned_for_a_fresh_primary_room() {
        let mut config = enabled_config();
        let mut state = WorkflowTaskState::default();
        let first = state.begin_primary(&config, None).unwrap();
        state.primary_ready(first.id, WaitingMode::Manual).unwrap();
        state.begin_followers(first.id).unwrap();
        state.fail(first.id, "join failed").unwrap();
        config.next_sequence = 2;

        let replacement = state.begin_primary(&config, None).unwrap();

        assert_eq!(replacement.room.name, "run-002");
        assert_eq!(state.status().phase, WorkflowPhase::Primary);
        assert!(replacement.id > first.id);
    }

    #[test]
    fn phase_wire_values_are_stable_and_exhaustive() {
        let cases = [
            (WorkflowPhase::Idle, "idle"),
            (WorkflowPhase::Primary, "primary"),
            (WorkflowPhase::Waiting, "waiting"),
            (WorkflowPhase::Followers, "followers"),
            (WorkflowPhase::Complete, "complete"),
            (WorkflowPhase::Cancelled, "cancelled"),
            (WorkflowPhase::Error, "error"),
        ];

        for (phase, wire) in cases {
            assert_eq!(serde_json::to_value(phase).unwrap(), wire);
        }
    }

    fn enabled_config() -> RoomAutomationConfig {
        RoomAutomationConfig {
            enabled: true,
            primary_account_id: "main".to_string(),
            follower_account_ids: vec!["one".to_string(), "two".to_string()],
            ..RoomAutomationConfig::default()
        }
    }
}
