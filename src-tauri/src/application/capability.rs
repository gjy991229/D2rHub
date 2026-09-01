//! Pure application control plane for optional capability lifecycles.
//!
//! The registry owns stable IDs, dependency validation, desired state and
//! observed runtime state. Host integrations implement [`CapabilityDriver`];
//! no Tauri, windowing or filesystem APIs are allowed in this module.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapabilityId(&'static str);

impl CapabilityId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Clone, Debug)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub version: &'static str,
    pub category: CapabilityCategory,
    pub dependencies: Vec<CapabilityId>,
    pub enabled_by_default: bool,
    pub config_schema_version: u32,
    pub settings_section: &'static str,
    pub commands: &'static [&'static str],
    pub events: &'static [&'static str],
}

impl CapabilityDescriptor {
    pub fn first_party(
        id: CapabilityId,
        category: CapabilityCategory,
        config_schema_version: u32,
        settings_section: &'static str,
        commands: &'static [&'static str],
        events: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            version: env!("CARGO_PKG_VERSION"),
            category,
            dependencies: Vec::new(),
            enabled_by_default: false,
            config_schema_version,
            settings_section,
            commands,
            events,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCategory {
    Automation,
    Companion,
    Overlay,
    Telemetry,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityDescriptorSnapshot {
    pub id: String,
    pub version: String,
    pub category: CapabilityCategory,
    pub dependencies: Vec<String>,
    pub enabled_by_default: bool,
    pub config_schema_version: u32,
    pub settings_section: String,
    pub commands: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Disabled,
    Stopped,
    Starting,
    Running,
    Degraded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityFailure {
    pub reason_code: String,
    pub message: String,
}

impl CapabilityFailure {
    pub fn new(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            reason_code: reason_code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum CapabilityHealth {
    #[default]
    Healthy,
    Degraded(CapabilityFailure),
    Failed(CapabilityFailure),
}

pub trait CapabilityDriver: Send + Sync {
    fn start(&self) -> Result<(), CapabilityFailure>;

    fn stop(&self) -> Result<(), CapabilityFailure>;

    fn health(&self) -> CapabilityHealth;

    /// Best-effort domain notification for capability-owned account
    /// references. The core account transaction is already committed before
    /// this hook runs, so implementations must be idempotent and may not
    /// require the deleted account directory to remain present.
    fn account_removed(&self, _account_id: &str) -> Result<(), CapabilityFailure> {
        Ok(())
    }
}

pub struct CapabilityRegistration {
    pub descriptor: CapabilityDescriptor,
    pub driver: Arc<dyn CapabilityDriver>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityStatus {
    pub id: String,
    pub requested_enabled: bool,
    pub state: CapabilityState,
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityStatusSnapshot {
    pub revision: u64,
    pub capabilities: Vec<CapabilityStatus>,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum CapabilityRegistryError {
    #[error("capability ID is already registered: {0}")]
    DuplicateId(CapabilityId),
    #[error("capability {capability} depends on an unknown capability: {dependency}")]
    MissingDependency {
        capability: CapabilityId,
        dependency: CapabilityId,
    },
    #[error("capability dependency cycle contains: {0}")]
    DependencyCycle(CapabilityId),
    #[error("unknown capability: {0}")]
    UnknownCapability(CapabilityId),
}

struct CapabilityEntry {
    descriptor: CapabilityDescriptor,
    driver: Arc<dyn CapabilityDriver>,
    requested_enabled: bool,
    state: CapabilityState,
    reason_code: Option<String>,
    last_error: Option<String>,
    /// A failed start, stop or health probe may leave resources partially
    /// owned. The next reconciliation must run `stop` before another `start`.
    cleanup_required: bool,
    operation: Arc<Mutex<()>>,
}

#[derive(Default)]
struct RegistryInner {
    revision: u64,
    entries: BTreeMap<CapabilityId, CapabilityEntry>,
}

#[derive(Default)]
pub struct CapabilityRegistry {
    inner: Mutex<RegistryInner>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomically registers a batch after validating the complete dependency
    /// graph. A failed batch leaves the existing registry untouched.
    pub fn register_all(
        &self,
        registrations: Vec<CapabilityRegistration>,
    ) -> Result<CapabilityStatusSnapshot, CapabilityRegistryError> {
        let mut inner = self.inner.lock();
        let mut descriptors: BTreeMap<CapabilityId, CapabilityDescriptor> = inner
            .entries
            .iter()
            .map(|(id, entry)| (*id, entry.descriptor.clone()))
            .collect();
        let mut batch_ids = BTreeSet::new();

        for registration in &registrations {
            let id = registration.descriptor.id;
            if descriptors.contains_key(&id) || !batch_ids.insert(id) {
                return Err(CapabilityRegistryError::DuplicateId(id));
            }
            descriptors.insert(id, registration.descriptor.clone());
        }

        validate_dependencies(&descriptors)?;

        if registrations.is_empty() {
            return Ok(snapshot_from_inner(&inner));
        }

        for registration in registrations {
            let requested_enabled = registration.descriptor.enabled_by_default;
            inner.entries.insert(
                registration.descriptor.id,
                CapabilityEntry {
                    descriptor: registration.descriptor,
                    driver: registration.driver,
                    requested_enabled,
                    state: if requested_enabled {
                        CapabilityState::Stopped
                    } else {
                        CapabilityState::Disabled
                    },
                    reason_code: None,
                    last_error: None,
                    cleanup_required: false,
                    operation: Arc::new(Mutex::new(())),
                },
            );
        }
        bump_revision(&mut inner);
        Ok(snapshot_from_inner(&inner))
    }

    /// Records user intent without executing lifecycle hooks. This method is
    /// bounded and is therefore safe to call from a configuration observer;
    /// a supervisor must schedule [`Self::reconcile_all`] separately.
    pub fn set_requested(
        &self,
        id: CapabilityId,
        requested_enabled: bool,
    ) -> Result<CapabilityStatusSnapshot, CapabilityRegistryError> {
        let mut inner = self.inner.lock();
        let entry = inner
            .entries
            .get_mut(&id)
            .ok_or(CapabilityRegistryError::UnknownCapability(id))?;
        if entry.requested_enabled == requested_enabled {
            return Ok(snapshot_from_inner(&inner));
        }

        entry.requested_enabled = requested_enabled;
        entry.reason_code = None;
        if requested_enabled && entry.state == CapabilityState::Disabled {
            entry.state = CapabilityState::Stopped;
        } else if !requested_enabled && entry.state == CapabilityState::Stopped {
            entry.state = CapabilityState::Disabled;
        }
        bump_revision(&mut inner);
        Ok(snapshot_from_inner(&inner))
    }

    /// Records a process-shutdown target for every registered capability.
    /// Actual stop hooks remain the supervisor's responsibility.
    pub fn disable_all(&self) -> CapabilityStatusSnapshot {
        let mut inner = self.inner.lock();
        let mut changed = false;
        for entry in inner.entries.values_mut() {
            if entry.requested_enabled {
                entry.requested_enabled = false;
                entry.reason_code = None;
                if entry.state == CapabilityState::Stopped {
                    entry.state = CapabilityState::Disabled;
                }
                changed = true;
            }
        }
        if changed {
            bump_revision(&mut inner);
        }
        snapshot_from_inner(&inner)
    }

    /// Reconciles entries in three phases: unsafe/disabled entries quiesce in
    /// reverse dependency order, eligible entries activate in dependency
    /// order, then any failure observed while activating is propagated back to
    /// dependents. Driver calls are never made while the global registry lock
    /// is held.
    pub fn reconcile_all(&self) -> Result<CapabilityStatusSnapshot, CapabilityRegistryError> {
        let order = {
            let inner = self.inner.lock();
            topological_order(&inner.entries)
        };

        let mut quiesced = BTreeSet::new();
        for id in order.iter().rev() {
            if self.needs_quiesce(*id)? {
                self.reconcile_one(*id)?;
                quiesced.insert(*id);
            }
        }
        for id in &order {
            if self.is_activation_eligible(*id)? {
                self.reconcile_one(*id)?;
            }
        }
        self.quiesce_blocked_dependents(&order, &quiesced)?;
        Ok(self.snapshot())
    }

    /// Samples already-running drivers without starting failed or stopped
    /// capabilities. Supervisors may call this periodically to detect worker
    /// exits even when configuration remains unchanged.
    pub fn refresh_running_health(
        &self,
    ) -> Result<CapabilityStatusSnapshot, CapabilityRegistryError> {
        let ids = {
            let inner = self.inner.lock();
            inner
                .entries
                .iter()
                .filter_map(|(id, entry)| {
                    (entry.requested_enabled
                        && !entry.cleanup_required
                        && dependency_blocker(&inner, *id).is_none()
                        && matches!(
                            entry.state,
                            CapabilityState::Running | CapabilityState::Degraded
                        ))
                    .then_some(*id)
                })
                .collect::<Vec<_>>()
        };
        for id in ids {
            self.refresh_one_health(id)?;
        }
        let order = {
            let inner = self.inner.lock();
            topological_order(&inner.entries)
        };
        self.quiesce_blocked_dependents(&order, &BTreeSet::new())?;
        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> CapabilityStatusSnapshot {
        snapshot_from_inner(&self.inner.lock())
    }

    pub fn descriptors(&self) -> Vec<CapabilityDescriptorSnapshot> {
        self.inner
            .lock()
            .entries
            .values()
            .map(|entry| CapabilityDescriptorSnapshot {
                id: entry.descriptor.id.to_string(),
                version: entry.descriptor.version.to_string(),
                category: entry.descriptor.category,
                dependencies: entry
                    .descriptor
                    .dependencies
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                enabled_by_default: entry.descriptor.enabled_by_default,
                config_schema_version: entry.descriptor.config_schema_version,
                settings_section: entry.descriptor.settings_section.to_string(),
                commands: entry
                    .descriptor
                    .commands
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                events: entry
                    .descriptor
                    .events
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            })
            .collect()
    }

    /// Broadcasts an account-removal notification without coupling the core
    /// command layer to concrete optional modules. Each driver uses the same
    /// per-capability operation lock as lifecycle reconciliation; panics and
    /// failures are isolated and returned for logging by the caller.
    pub fn notify_account_removed(
        &self,
        account_id: &str,
    ) -> Vec<(CapabilityId, CapabilityFailure)> {
        let drivers = {
            let inner = self.inner.lock();
            topological_order(&inner.entries)
                .into_iter()
                .filter_map(|id| {
                    inner
                        .entries
                        .get(&id)
                        .map(|entry| (id, Arc::clone(&entry.operation), Arc::clone(&entry.driver)))
                })
                .collect::<Vec<_>>()
        };

        drivers
            .into_iter()
            .filter_map(|(id, operation, driver)| {
                let _operation = operation.lock();
                call_lifecycle_hook("account_removed", || driver.account_removed(account_id))
                    .err()
                    .map(|failure| (id, failure))
            })
            .collect()
    }

    #[cfg(test)]
    fn last_error(&self, id: CapabilityId) -> Option<String> {
        self.inner
            .lock()
            .entries
            .get(&id)
            .and_then(|entry| entry.last_error.clone())
    }

    fn needs_quiesce(&self, id: CapabilityId) -> Result<bool, CapabilityRegistryError> {
        let inner = self.inner.lock();
        let entry = inner
            .entries
            .get(&id)
            .ok_or(CapabilityRegistryError::UnknownCapability(id))?;
        Ok(!entry.requested_enabled
            || entry.cleanup_required
            || dependency_blocker(&inner, id).is_some())
    }

    fn is_activation_eligible(&self, id: CapabilityId) -> Result<bool, CapabilityRegistryError> {
        let inner = self.inner.lock();
        let entry = inner
            .entries
            .get(&id)
            .ok_or(CapabilityRegistryError::UnknownCapability(id))?;
        Ok(entry.requested_enabled
            && !entry.cleanup_required
            && dependency_blocker(&inner, id).is_none())
    }

    fn quiesce_blocked_dependents(
        &self,
        order: &[CapabilityId],
        already_quiesced: &BTreeSet<CapabilityId>,
    ) -> Result<(), CapabilityRegistryError> {
        for id in order.iter().rev() {
            if already_quiesced.contains(id) {
                continue;
            }
            let blocked = {
                let inner = self.inner.lock();
                let entry = inner
                    .entries
                    .get(id)
                    .ok_or(CapabilityRegistryError::UnknownCapability(*id))?;
                entry.requested_enabled && dependency_blocker(&inner, *id).is_some()
            };
            if blocked {
                self.reconcile_one(*id)?;
            }
        }
        Ok(())
    }

    fn refresh_one_health(&self, id: CapabilityId) -> Result<(), CapabilityRegistryError> {
        let (operation, driver) = {
            let inner = self.inner.lock();
            let entry = inner
                .entries
                .get(&id)
                .ok_or(CapabilityRegistryError::UnknownCapability(id))?;
            (Arc::clone(&entry.operation), Arc::clone(&entry.driver))
        };
        let _operation = operation.lock();
        let should_probe = {
            let inner = self.inner.lock();
            inner.entries.get(&id).is_some_and(|entry| {
                entry.requested_enabled
                    && !entry.cleanup_required
                    && dependency_blocker(&inner, id).is_none()
                    && matches!(
                        entry.state,
                        CapabilityState::Running | CapabilityState::Degraded
                    )
            })
        };
        if should_probe {
            self.commit_health(id, call_health_hook(|| driver.health()))?;
        }
        Ok(())
    }

    fn reconcile_one(&self, id: CapabilityId) -> Result<(), CapabilityRegistryError> {
        let operation = {
            let inner = self.inner.lock();
            Arc::clone(
                &inner
                    .entries
                    .get(&id)
                    .ok_or(CapabilityRegistryError::UnknownCapability(id))?
                    .operation,
            )
        };
        let _operation = operation.lock();

        let action = {
            let mut inner = self.inner.lock();
            let dependency_blocker = dependency_blocker(&inner, id);
            let dependent_blocker = active_dependent_blocker(&inner, id);
            let entry = inner
                .entries
                .get_mut(&id)
                .ok_or(CapabilityRegistryError::UnknownCapability(id))?;

            if entry.requested_enabled {
                if dependency_blocker.is_some() {
                    if !entry.cleanup_required
                        && !matches!(
                            entry.state,
                            CapabilityState::Running
                                | CapabilityState::Degraded
                                | CapabilityState::Starting
                                | CapabilityState::Failed
                        )
                    {
                        let changed = entry.state != CapabilityState::Stopped
                            || entry.reason_code != dependency_blocker;
                        entry.state = CapabilityState::Stopped;
                        entry.reason_code = dependency_blocker;
                        if changed {
                            bump_revision(&mut inner);
                        }
                        LifecycleAction::None
                    } else if let Some(blocker) = dependent_blocker {
                        let next_state = if entry.state == CapabilityState::Failed {
                            CapabilityState::Failed
                        } else {
                            CapabilityState::Degraded
                        };
                        let changed = entry.state != next_state
                            || entry.reason_code.as_deref() != Some(blocker.as_str());
                        entry.state = next_state;
                        entry.reason_code = Some(blocker);
                        if changed {
                            bump_revision(&mut inner);
                        }
                        LifecycleAction::None
                    } else if matches!(
                        entry.state,
                        CapabilityState::Running
                            | CapabilityState::Degraded
                            | CapabilityState::Starting
                            | CapabilityState::Failed
                    ) || entry.cleanup_required
                    {
                        LifecycleAction::Stop(Arc::clone(&entry.driver))
                    } else {
                        LifecycleAction::None
                    }
                } else if entry.cleanup_required {
                    if let Some(blocker) = dependent_blocker {
                        let next_state = if entry.state == CapabilityState::Failed {
                            CapabilityState::Failed
                        } else {
                            CapabilityState::Degraded
                        };
                        let changed = entry.state != next_state
                            || entry.reason_code.as_deref() != Some(blocker.as_str());
                        entry.state = next_state;
                        entry.reason_code = Some(blocker);
                        if changed {
                            bump_revision(&mut inner);
                        }
                        LifecycleAction::None
                    } else {
                        LifecycleAction::Stop(Arc::clone(&entry.driver))
                    }
                } else if matches!(
                    entry.state,
                    CapabilityState::Stopped
                        | CapabilityState::Disabled
                        | CapabilityState::Failed
                        | CapabilityState::Starting
                ) {
                    entry.state = CapabilityState::Starting;
                    entry.reason_code = None;
                    entry.last_error = None;
                    // A start hook may partially acquire resources before it
                    // returns or panics. Success is the only transition that
                    // clears this conservative cleanup requirement.
                    entry.cleanup_required = true;
                    let driver = Arc::clone(&entry.driver);
                    bump_revision(&mut inner);
                    LifecycleAction::Start(driver)
                } else {
                    LifecycleAction::Health(Arc::clone(&entry.driver))
                }
            } else if let Some(blocker) = dependent_blocker {
                let next_state = if matches!(
                    entry.state,
                    CapabilityState::Starting
                        | CapabilityState::Running
                        | CapabilityState::Degraded
                ) {
                    CapabilityState::Degraded
                } else {
                    entry.state
                };
                let changed = entry.state != next_state
                    || entry.reason_code.as_deref() != Some(blocker.as_str());
                entry.state = next_state;
                entry.reason_code = Some(blocker);
                if changed {
                    bump_revision(&mut inner);
                }
                LifecycleAction::None
            } else if matches!(entry.state, CapabilityState::Disabled) {
                LifecycleAction::None
            } else if entry.state == CapabilityState::Stopped {
                entry.state = CapabilityState::Disabled;
                entry.reason_code = None;
                bump_revision(&mut inner);
                LifecycleAction::None
            } else {
                LifecycleAction::Stop(Arc::clone(&entry.driver))
            }
        };

        match action {
            LifecycleAction::None => {}
            LifecycleAction::Start(driver) => {
                let result = call_lifecycle_hook("start", || driver.start());
                let mut inner = self.inner.lock();
                let entry = inner
                    .entries
                    .get_mut(&id)
                    .ok_or(CapabilityRegistryError::UnknownCapability(id))?;
                match result {
                    Ok(()) => {
                        entry.state = CapabilityState::Running;
                        entry.reason_code = None;
                        entry.last_error = None;
                        entry.cleanup_required = false;
                    }
                    Err(failure) => {
                        entry.state = CapabilityState::Failed;
                        entry.reason_code = Some(failure.reason_code);
                        entry.last_error = Some(failure.message);
                        entry.cleanup_required = true;
                    }
                }
                bump_revision(&mut inner);
            }
            LifecycleAction::Stop(driver) => {
                let result = call_lifecycle_hook("stop", || driver.stop());
                let mut inner = self.inner.lock();
                let dependency_blocker = dependency_blocker(&inner, id);
                let entry = inner
                    .entries
                    .get_mut(&id)
                    .ok_or(CapabilityRegistryError::UnknownCapability(id))?;
                match result {
                    Ok(()) => {
                        entry.state = if entry.requested_enabled {
                            CapabilityState::Stopped
                        } else {
                            CapabilityState::Disabled
                        };
                        entry.reason_code = if entry.requested_enabled {
                            dependency_blocker
                        } else {
                            None
                        };
                        entry.last_error = None;
                        entry.cleanup_required = false;
                    }
                    Err(failure) => {
                        entry.state = CapabilityState::Failed;
                        entry.reason_code = Some(failure.reason_code);
                        entry.last_error = Some(failure.message);
                        entry.cleanup_required = true;
                    }
                }
                bump_revision(&mut inner);
            }
            LifecycleAction::Health(driver) => {
                let health = call_health_hook(|| driver.health());
                self.commit_health(id, health)?;
            }
        }
        Ok(())
    }

    fn commit_health(
        &self,
        id: CapabilityId,
        health: CapabilityHealth,
    ) -> Result<(), CapabilityRegistryError> {
        let mut inner = self.inner.lock();
        let entry = inner
            .entries
            .get_mut(&id)
            .ok_or(CapabilityRegistryError::UnknownCapability(id))?;
        if entry.cleanup_required {
            return Ok(());
        }
        let (state, reason_code, last_error, cleanup_required) = match health {
            CapabilityHealth::Healthy => (CapabilityState::Running, None, None, false),
            CapabilityHealth::Degraded(failure) => (
                CapabilityState::Degraded,
                Some(failure.reason_code),
                Some(failure.message),
                false,
            ),
            CapabilityHealth::Failed(failure) => (
                CapabilityState::Failed,
                Some(failure.reason_code),
                Some(failure.message),
                true,
            ),
        };
        if entry.state != state
            || entry.reason_code != reason_code
            || entry.last_error != last_error
            || entry.cleanup_required != cleanup_required
        {
            entry.state = state;
            entry.reason_code = reason_code;
            entry.last_error = last_error;
            entry.cleanup_required = cleanup_required;
            bump_revision(&mut inner);
        }
        Ok(())
    }
}

fn call_lifecycle_hook(
    operation: &'static str,
    call: impl FnOnce() -> Result<(), CapabilityFailure>,
) -> Result<(), CapabilityFailure> {
    catch_unwind(AssertUnwindSafe(call)).unwrap_or_else(|_| {
        Err(CapabilityFailure::new(
            "driver-panicked",
            format!("capability driver panicked during {operation}"),
        ))
    })
}

fn call_health_hook(call: impl FnOnce() -> CapabilityHealth) -> CapabilityHealth {
    catch_unwind(AssertUnwindSafe(call)).unwrap_or_else(|_| {
        CapabilityHealth::Failed(CapabilityFailure::new(
            "driver-panicked",
            "capability driver panicked during health",
        ))
    })
}

enum LifecycleAction {
    None,
    Start(Arc<dyn CapabilityDriver>),
    Stop(Arc<dyn CapabilityDriver>),
    Health(Arc<dyn CapabilityDriver>),
}

fn bump_revision(inner: &mut RegistryInner) {
    inner.revision = inner.revision.saturating_add(1);
}

fn snapshot_from_inner(inner: &RegistryInner) -> CapabilityStatusSnapshot {
    CapabilityStatusSnapshot {
        revision: inner.revision,
        capabilities: inner
            .entries
            .iter()
            .map(|(id, entry)| CapabilityStatus {
                id: id.as_str().to_string(),
                requested_enabled: entry.requested_enabled,
                state: entry.state,
                reason_code: entry.reason_code.clone(),
            })
            .collect(),
    }
}

fn dependency_blocker(inner: &RegistryInner, id: CapabilityId) -> Option<String> {
    inner.entries.get(&id).and_then(|entry| {
        entry
            .descriptor
            .dependencies
            .iter()
            .find(|dependency| {
                !is_transitively_available(inner, **dependency, &mut BTreeSet::new())
            })
            .map(|dependency| format!("dependency-unavailable:{}", dependency.as_str()))
    })
}

fn is_transitively_available(
    inner: &RegistryInner,
    id: CapabilityId,
    visiting: &mut BTreeSet<CapabilityId>,
) -> bool {
    if !visiting.insert(id) {
        return false;
    }
    let available = inner.entries.get(&id).is_some_and(|entry| {
        entry.requested_enabled
            && !entry.cleanup_required
            && matches!(
                entry.state,
                CapabilityState::Running | CapabilityState::Degraded
            )
            && entry
                .descriptor
                .dependencies
                .iter()
                .all(|dependency| is_transitively_available(inner, *dependency, visiting))
    });
    visiting.remove(&id);
    available
}

fn active_dependent_blocker(inner: &RegistryInner, id: CapabilityId) -> Option<String> {
    let mut frontier = vec![id];
    let mut visited = BTreeSet::new();
    while let Some(dependency) = frontier.pop() {
        if !visited.insert(dependency) {
            continue;
        }
        for (candidate_id, candidate) in &inner.entries {
            if !candidate.descriptor.dependencies.contains(&dependency) {
                continue;
            }
            if matches!(
                candidate.state,
                CapabilityState::Starting
                    | CapabilityState::Running
                    | CapabilityState::Degraded
                    | CapabilityState::Failed
            ) {
                return Some(format!("dependent-active:{}", candidate_id.as_str()));
            }
            frontier.push(*candidate_id);
        }
    }
    None
}

fn validate_dependencies(
    descriptors: &BTreeMap<CapabilityId, CapabilityDescriptor>,
) -> Result<(), CapabilityRegistryError> {
    for descriptor in descriptors.values() {
        for dependency in &descriptor.dependencies {
            if !descriptors.contains_key(dependency) {
                return Err(CapabilityRegistryError::MissingDependency {
                    capability: descriptor.id,
                    dependency: *dependency,
                });
            }
        }
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in descriptors.keys().copied() {
        visit_dependency(id, descriptors, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_dependency(
    id: CapabilityId,
    descriptors: &BTreeMap<CapabilityId, CapabilityDescriptor>,
    visiting: &mut BTreeSet<CapabilityId>,
    visited: &mut BTreeSet<CapabilityId>,
) -> Result<(), CapabilityRegistryError> {
    if visited.contains(&id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(CapabilityRegistryError::DependencyCycle(id));
    }
    if let Some(descriptor) = descriptors.get(&id) {
        for dependency in &descriptor.dependencies {
            visit_dependency(*dependency, descriptors, visiting, visited)?;
        }
    }
    visiting.remove(&id);
    visited.insert(id);
    Ok(())
}

fn topological_order(entries: &BTreeMap<CapabilityId, CapabilityEntry>) -> Vec<CapabilityId> {
    fn visit(
        id: CapabilityId,
        entries: &BTreeMap<CapabilityId, CapabilityEntry>,
        visited: &mut BTreeSet<CapabilityId>,
        ordered: &mut Vec<CapabilityId>,
    ) {
        if !visited.insert(id) {
            return;
        }
        if let Some(entry) = entries.get(&id) {
            for dependency in &entry.descriptor.dependencies {
                visit(*dependency, entries, visited, ordered);
            }
        }
        ordered.push(id);
    }

    let mut ordered = Vec::with_capacity(entries.len());
    let mut visited = BTreeSet::new();
    for id in entries.keys().copied() {
        visit(id, entries, &mut visited, &mut ordered);
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const PLATFORM: CapabilityId = CapabilityId::new("platform");
    const MODULE: CapabilityId = CapabilityId::new("module");
    const LEAF: CapabilityId = CapabilityId::new("leaf");

    #[derive(Default)]
    struct FakeDriver {
        starts: AtomicUsize,
        stops: AtomicUsize,
        removed_accounts: Mutex<Vec<String>>,
        health: Mutex<CapabilityHealth>,
        fail_start: Mutex<Option<CapabilityFailure>>,
        fail_stop: Mutex<Option<CapabilityFailure>>,
        fail_account_removed: Mutex<Option<CapabilityFailure>>,
        panic_start: AtomicBool,
        panic_account_removed: AtomicBool,
    }

    impl FakeDriver {
        fn healthy() -> Arc<Self> {
            Arc::new(Self {
                health: Mutex::new(CapabilityHealth::Healthy),
                ..Self::default()
            })
        }
    }

    impl CapabilityDriver for FakeDriver {
        fn start(&self) -> Result<(), CapabilityFailure> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            assert!(
                !self.panic_start.load(Ordering::SeqCst),
                "injected start panic"
            );
            if let Some(failure) = self.fail_start.lock().clone() {
                return Err(failure);
            }
            Ok(())
        }

        fn stop(&self) -> Result<(), CapabilityFailure> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            if let Some(failure) = self.fail_stop.lock().clone() {
                return Err(failure);
            }
            Ok(())
        }

        fn health(&self) -> CapabilityHealth {
            self.health.lock().clone()
        }

        fn account_removed(&self, account_id: &str) -> Result<(), CapabilityFailure> {
            self.removed_accounts.lock().push(account_id.to_string());
            assert!(
                !self.panic_account_removed.load(Ordering::SeqCst),
                "injected account removal panic"
            );
            if let Some(failure) = self.fail_account_removed.lock().clone() {
                return Err(failure);
            }
            Ok(())
        }
    }

    fn registration(
        id: CapabilityId,
        dependencies: Vec<CapabilityId>,
        enabled_by_default: bool,
        driver: Arc<FakeDriver>,
    ) -> CapabilityRegistration {
        CapabilityRegistration {
            descriptor: CapabilityDescriptor {
                id,
                version: "test",
                category: CapabilityCategory::Automation,
                dependencies,
                enabled_by_default,
                config_schema_version: 1,
                settings_section: "test",
                commands: &[],
                events: &[],
            },
            driver,
        }
    }

    #[test]
    fn registration_is_atomic_for_duplicates_missing_dependencies_and_cycles() {
        let registry = CapabilityRegistry::new();
        let driver = FakeDriver::healthy();
        let duplicate = registry.register_all(vec![
            registration(MODULE, vec![], false, Arc::clone(&driver)),
            registration(MODULE, vec![], false, Arc::clone(&driver)),
        ]);
        assert_eq!(duplicate, Err(CapabilityRegistryError::DuplicateId(MODULE)));
        assert!(registry.snapshot().capabilities.is_empty());

        let missing = registry.register_all(vec![registration(
            MODULE,
            vec![PLATFORM],
            false,
            Arc::clone(&driver),
        )]);
        assert!(matches!(
            missing,
            Err(CapabilityRegistryError::MissingDependency { .. })
        ));
        assert!(registry.snapshot().capabilities.is_empty());

        let cycle = registry.register_all(vec![
            registration(PLATFORM, vec![MODULE], true, Arc::clone(&driver)),
            registration(MODULE, vec![PLATFORM], true, driver),
        ]);
        assert!(matches!(
            cycle,
            Err(CapabilityRegistryError::DependencyCycle(_))
        ));
        assert!(registry.snapshot().capabilities.is_empty());
    }

    #[test]
    fn descriptor_snapshot_exposes_the_complete_internal_module_contract() {
        let registry = CapabilityRegistry::new();
        registry
            .register_all(vec![registration(
                MODULE,
                vec![],
                false,
                FakeDriver::healthy(),
            )])
            .unwrap();

        let descriptors = registry.descriptors();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].id, "module");
        assert_eq!(descriptors[0].version, "test");
        assert_eq!(descriptors[0].category, CapabilityCategory::Automation);
        assert_eq!(descriptors[0].config_schema_version, 1);
        assert_eq!(descriptors[0].settings_section, "test");
        assert!(descriptors[0].commands.is_empty());
        assert!(descriptors[0].events.is_empty());
    }

    #[test]
    fn reconcile_starts_dependencies_first_and_stops_dependents_first() {
        let registry = CapabilityRegistry::new();
        let platform = FakeDriver::healthy();
        let module = FakeDriver::healthy();
        registry
            .register_all(vec![
                registration(PLATFORM, vec![], true, Arc::clone(&platform)),
                registration(MODULE, vec![PLATFORM], true, Arc::clone(&module)),
            ])
            .unwrap();

        let running = registry.reconcile_all().unwrap();
        assert!(running
            .capabilities
            .iter()
            .all(|status| status.state == CapabilityState::Running));
        assert_eq!(platform.starts.load(Ordering::SeqCst), 1);
        assert_eq!(module.starts.load(Ordering::SeqCst), 1);

        registry.set_requested(PLATFORM, false).unwrap();
        let blocked = registry.reconcile_all().unwrap();
        let blocked_module = blocked
            .capabilities
            .iter()
            .find(|status| status.id == MODULE.as_str())
            .unwrap();
        assert!(blocked_module.requested_enabled);
        assert_eq!(blocked_module.state, CapabilityState::Stopped);
        assert_eq!(
            blocked_module.reason_code.as_deref(),
            Some("dependency-unavailable:platform")
        );
        assert_eq!(module.stops.load(Ordering::SeqCst), 1);
        assert_eq!(platform.stops.load(Ordering::SeqCst), 1);

        registry.set_requested(MODULE, false).unwrap();
        let stopped = registry.reconcile_all().unwrap();
        assert!(stopped
            .capabilities
            .iter()
            .all(|status| status.state == CapabilityState::Disabled));
    }

    #[test]
    fn repeated_and_concurrent_reconcile_starts_a_driver_once() {
        let registry = Arc::new(CapabilityRegistry::new());
        let driver = FakeDriver::healthy();
        registry
            .register_all(vec![registration(
                MODULE,
                vec![],
                true,
                Arc::clone(&driver),
            )])
            .unwrap();

        let mut threads = Vec::new();
        for _ in 0..8 {
            let registry = Arc::clone(&registry);
            threads.push(std::thread::spawn(move || {
                registry.reconcile_all().unwrap()
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }

        assert_eq!(driver.starts.load(Ordering::SeqCst), 1);
        assert_eq!(
            registry.snapshot().capabilities[0].state,
            CapabilityState::Running
        );
    }

    #[test]
    fn account_removal_notifications_include_disabled_drivers_and_isolate_failures() {
        let registry = CapabilityRegistry::new();
        let healthy = FakeDriver::healthy();
        let failing = FakeDriver::healthy();
        let panicking = FakeDriver::healthy();
        *failing.fail_account_removed.lock() = Some(CapabilityFailure::new(
            "cleanup-failed",
            "injected cleanup failure",
        ));
        panicking
            .panic_account_removed
            .store(true, Ordering::SeqCst);
        registry
            .register_all(vec![
                registration(PLATFORM, vec![], false, Arc::clone(&healthy)),
                registration(MODULE, vec![], false, Arc::clone(&failing)),
                registration(LEAF, vec![], false, Arc::clone(&panicking)),
            ])
            .unwrap();

        let failures = registry.notify_account_removed("Account-A");

        assert_eq!(healthy.removed_accounts.lock().as_slice(), ["Account-A"]);
        assert_eq!(failing.removed_accounts.lock().as_slice(), ["Account-A"]);
        assert_eq!(panicking.removed_accounts.lock().as_slice(), ["Account-A"]);
        assert_eq!(failures.len(), 2);
        assert!(failures
            .iter()
            .any(|(id, failure)| *id == MODULE && failure.reason_code == "cleanup-failed"));
        assert!(failures
            .iter()
            .any(|(id, failure)| *id == LEAF && failure.reason_code == "driver-panicked"));
        assert!(registry
            .snapshot()
            .capabilities
            .iter()
            .all(|status| status.state == CapabilityState::Disabled));
    }

    #[test]
    fn a_dependency_stays_running_when_its_dependent_cannot_stop() {
        let registry = CapabilityRegistry::new();
        let platform = FakeDriver::healthy();
        let module = FakeDriver::healthy();
        registry
            .register_all(vec![
                registration(PLATFORM, vec![], true, Arc::clone(&platform)),
                registration(MODULE, vec![PLATFORM], true, Arc::clone(&module)),
            ])
            .unwrap();
        registry.reconcile_all().unwrap();

        *module.fail_stop.lock() = Some(CapabilityFailure::new("stop-failed", "busy"));
        registry.set_requested(PLATFORM, false).unwrap();
        let blocked = registry.reconcile_all().unwrap();
        let platform_status = blocked
            .capabilities
            .iter()
            .find(|status| status.id == PLATFORM.as_str())
            .unwrap();
        assert_eq!(platform_status.state, CapabilityState::Degraded);
        assert_eq!(
            platform_status.reason_code.as_deref(),
            Some("dependent-active:module")
        );
        assert_eq!(platform.stops.load(Ordering::SeqCst), 0);
        assert_eq!(module.stops.load(Ordering::SeqCst), 1);

        *module.fail_stop.lock() = None;
        registry.reconcile_all().unwrap();
        assert_eq!(module.stops.load(Ordering::SeqCst), 2);
        assert_eq!(platform.stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn start_failure_is_isolated_and_health_can_degrade_then_recover() {
        let registry = CapabilityRegistry::new();
        let failing = FakeDriver::healthy();
        *failing.fail_start.lock() = Some(CapabilityFailure::new("start-failed", "boom"));
        let healthy = FakeDriver::healthy();
        registry
            .register_all(vec![
                registration(PLATFORM, vec![], true, Arc::clone(&failing)),
                registration(MODULE, vec![], true, Arc::clone(&healthy)),
            ])
            .unwrap();

        registry.reconcile_all().unwrap();
        let snapshot = registry.snapshot();
        assert_eq!(
            snapshot
                .capabilities
                .iter()
                .find(|status| status.id == PLATFORM.as_str())
                .unwrap()
                .state,
            CapabilityState::Failed
        );
        assert_eq!(
            snapshot
                .capabilities
                .iter()
                .find(|status| status.id == MODULE.as_str())
                .unwrap()
                .state,
            CapabilityState::Running
        );
        assert_eq!(registry.last_error(PLATFORM).as_deref(), Some("boom"));

        *healthy.health.lock() =
            CapabilityHealth::Degraded(CapabilityFailure::new("temporarily-limited", "limited"));
        registry.refresh_running_health().unwrap();
        assert_eq!(
            registry
                .snapshot()
                .capabilities
                .iter()
                .find(|status| status.id == MODULE.as_str())
                .unwrap()
                .state,
            CapabilityState::Degraded
        );
        *healthy.health.lock() = CapabilityHealth::Healthy;
        registry.refresh_running_health().unwrap();
        assert_eq!(
            registry
                .snapshot()
                .capabilities
                .iter()
                .find(|status| status.id == MODULE.as_str())
                .unwrap()
                .state,
            CapabilityState::Running
        );
    }

    #[test]
    fn a_panicking_start_is_isolated_and_cleaned_before_retry() {
        let registry = CapabilityRegistry::new();
        let driver = FakeDriver::healthy();
        driver.panic_start.store(true, Ordering::SeqCst);
        registry
            .register_all(vec![registration(
                MODULE,
                vec![],
                true,
                Arc::clone(&driver),
            )])
            .unwrap();

        let failed = registry.reconcile_all().unwrap();
        assert_eq!(failed.capabilities[0].state, CapabilityState::Failed);
        assert_eq!(
            failed.capabilities[0].reason_code.as_deref(),
            Some("driver-panicked")
        );

        driver.panic_start.store(false, Ordering::SeqCst);
        let recovered = registry.reconcile_all().unwrap();
        assert_eq!(recovered.capabilities[0].state, CapabilityState::Running);
        assert_eq!(driver.stops.load(Ordering::SeqCst), 1);
        assert_eq!(driver.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn runtime_dependency_failure_stops_and_blocks_its_dependent() {
        let registry = CapabilityRegistry::new();
        let platform = FakeDriver::healthy();
        let module = FakeDriver::healthy();
        registry
            .register_all(vec![
                registration(PLATFORM, vec![], true, Arc::clone(&platform)),
                registration(MODULE, vec![PLATFORM], true, Arc::clone(&module)),
            ])
            .unwrap();
        registry.reconcile_all().unwrap();

        *platform.health.lock() =
            CapabilityHealth::Failed(CapabilityFailure::new("worker-exited", "gone"));
        let failed = registry.refresh_running_health().unwrap();
        let platform_status = failed
            .capabilities
            .iter()
            .find(|status| status.id == PLATFORM.as_str())
            .unwrap();
        let module_status = failed
            .capabilities
            .iter()
            .find(|status| status.id == MODULE.as_str())
            .unwrap();

        assert_eq!(platform_status.state, CapabilityState::Failed);
        assert_eq!(
            platform_status.reason_code.as_deref(),
            Some("worker-exited")
        );
        assert_eq!(module_status.state, CapabilityState::Stopped);
        assert_eq!(
            module_status.reason_code.as_deref(),
            Some("dependency-unavailable:platform")
        );
        assert_eq!(module.stops.load(Ordering::SeqCst), 1);
        assert_eq!(platform.stops.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn failed_dependency_is_not_restarted_while_a_dependent_cannot_stop() {
        let registry = CapabilityRegistry::new();
        let platform = FakeDriver::healthy();
        let module = FakeDriver::healthy();
        registry
            .register_all(vec![
                registration(PLATFORM, vec![], true, Arc::clone(&platform)),
                registration(MODULE, vec![PLATFORM], true, Arc::clone(&module)),
            ])
            .unwrap();
        registry.reconcile_all().unwrap();

        *module.fail_stop.lock() = Some(CapabilityFailure::new("stop-failed", "busy"));
        *platform.health.lock() =
            CapabilityHealth::Failed(CapabilityFailure::new("worker-exited", "gone"));
        registry.refresh_running_health().unwrap();
        let blocked = registry.reconcile_all().unwrap();
        let platform_status = blocked
            .capabilities
            .iter()
            .find(|status| status.id == PLATFORM.as_str())
            .unwrap();

        assert_eq!(platform_status.state, CapabilityState::Failed);
        assert_eq!(
            platform_status.reason_code.as_deref(),
            Some("dependent-active:module")
        );
        assert_eq!(platform.starts.load(Ordering::SeqCst), 1);
        assert_eq!(platform.stops.load(Ordering::SeqCst), 0);

        *module.fail_stop.lock() = None;
        registry.reconcile_all().unwrap();
        assert_eq!(platform.stops.load(Ordering::SeqCst), 1);
        assert_eq!(platform.starts.load(Ordering::SeqCst), 2);
        assert_eq!(module.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn transitive_dependency_failure_quiesces_the_whole_chain() {
        let registry = CapabilityRegistry::new();
        let platform = FakeDriver::healthy();
        let module = FakeDriver::healthy();
        let leaf = FakeDriver::healthy();
        registry
            .register_all(vec![
                registration(PLATFORM, vec![], true, Arc::clone(&platform)),
                registration(MODULE, vec![PLATFORM], true, Arc::clone(&module)),
                registration(LEAF, vec![MODULE], true, Arc::clone(&leaf)),
            ])
            .unwrap();
        registry.reconcile_all().unwrap();

        *platform.health.lock() =
            CapabilityHealth::Failed(CapabilityFailure::new("worker-exited", "gone"));
        let failed = registry.refresh_running_health().unwrap();
        let module_status = failed
            .capabilities
            .iter()
            .find(|status| status.id == MODULE.as_str())
            .unwrap();
        let leaf_status = failed
            .capabilities
            .iter()
            .find(|status| status.id == LEAF.as_str())
            .unwrap();

        assert_eq!(module_status.state, CapabilityState::Stopped);
        assert_eq!(leaf_status.state, CapabilityState::Stopped);
        assert_eq!(module.stops.load(Ordering::SeqCst), 1);
        assert_eq!(leaf.stops.load(Ordering::SeqCst), 1);
        assert_eq!(platform.stops.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn transitive_cleanup_waits_for_the_last_dependent_to_stop() {
        let registry = CapabilityRegistry::new();
        let platform = FakeDriver::healthy();
        let module = FakeDriver::healthy();
        let leaf = FakeDriver::healthy();
        registry
            .register_all(vec![
                registration(PLATFORM, vec![], true, Arc::clone(&platform)),
                registration(MODULE, vec![PLATFORM], true, Arc::clone(&module)),
                registration(LEAF, vec![MODULE], true, Arc::clone(&leaf)),
            ])
            .unwrap();
        registry.reconcile_all().unwrap();

        *leaf.fail_stop.lock() = Some(CapabilityFailure::new("stop-failed", "busy"));
        *platform.health.lock() =
            CapabilityHealth::Failed(CapabilityFailure::new("worker-exited", "gone"));
        registry.refresh_running_health().unwrap();
        registry.reconcile_all().unwrap();

        assert_eq!(platform.starts.load(Ordering::SeqCst), 1);
        assert_eq!(platform.stops.load(Ordering::SeqCst), 0);
        assert_eq!(module.starts.load(Ordering::SeqCst), 1);
        assert_eq!(module.stops.load(Ordering::SeqCst), 0);

        *leaf.fail_stop.lock() = None;
        registry.reconcile_all().unwrap();
        assert_eq!(platform.stops.load(Ordering::SeqCst), 1);
        assert_eq!(module.stops.load(Ordering::SeqCst), 1);
        assert_eq!(platform.starts.load(Ordering::SeqCst), 2);
        assert_eq!(module.starts.load(Ordering::SeqCst), 2);
        assert_eq!(leaf.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn revisions_are_monotonic_and_snapshots_are_sorted() {
        let registry = CapabilityRegistry::new();
        let first = CapabilityId::new("z-last");
        let second = CapabilityId::new("a-first");
        registry
            .register_all(vec![
                registration(first, vec![], false, FakeDriver::healthy()),
                registration(second, vec![], false, FakeDriver::healthy()),
            ])
            .unwrap();
        let registered = registry.snapshot();
        let requested = registry.set_requested(first, true).unwrap();
        let running = registry.reconcile_all().unwrap();

        assert!(registered.revision < requested.revision);
        assert!(requested.revision < running.revision);
        assert_eq!(
            running
                .capabilities
                .iter()
                .map(|status| status.id.as_str())
                .collect::<Vec<_>>(),
            ["a-first", "z-last"]
        );
    }
}
