use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};

use super::ports::InstanceStatusPort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSnapshot {
    pub pid: u32,
    pub mod_args: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningInstance {
    pub account_id: String,
    pub pid: u32,
    pub launch: Option<LaunchSnapshot>,
}

#[derive(Debug, Clone)]
struct InstanceEntry {
    account_id: String,
    active_pid: Option<u32>,
    launch: Option<LaunchSnapshot>,
}

#[derive(Default)]
struct RegistryState {
    revision: u64,
    entries: HashMap<String, InstanceEntry>,
}

/// An opaque, point-in-time view used to commit a process scan only when no
/// newer launch or cleanup has changed the registry in the meantime.
#[derive(Debug, Clone)]
pub struct InstanceRegistrySnapshot {
    revision: u64,
    instances: Vec<RunningInstance>,
}

impl InstanceRegistrySnapshot {
    pub fn instances(&self) -> &[RunningInstance] {
        &self.instances
    }
}

#[derive(Default)]
pub struct InstanceRegistry {
    state: RwLock<RegistryState>,
}

fn account_key(account_id: &str) -> String {
    account_id.to_ascii_lowercase()
}

impl InstanceRegistry {
    pub fn get(&self, account_id: &str) -> Option<RunningInstance> {
        let state = self.state.read();
        let entry = state.entries.get(&account_key(account_id))?;
        let pid = entry.active_pid?;
        Some(RunningInstance {
            account_id: entry.account_id.clone(),
            pid,
            launch: entry.launch.clone().filter(|launch| launch.pid == pid),
        })
    }

    pub fn list(&self) -> Vec<RunningInstance> {
        let state = self.state.read();
        running_instances(&state.entries)
    }

    pub fn snapshot(&self) -> InstanceRegistrySnapshot {
        let state = self.state.read();
        InstanceRegistrySnapshot {
            revision: state.revision,
            instances: running_instances(&state.entries),
        }
    }

    pub fn pid_for(&self, account_id: &str) -> Option<u32> {
        self.state
            .read()
            .entries
            .get(&account_key(account_id))
            .and_then(|entry| entry.active_pid)
    }

    pub fn record_launched(&self, account_id: &str, pid: u32, mod_args: &str) {
        if pid == 0 {
            return;
        }
        let key = account_key(account_id);
        let mut state = self.state.write();
        state
            .entries
            .retain(|existing_key, entry| existing_key == &key || entry.active_pid != Some(pid));
        state.entries.insert(
            key,
            InstanceEntry {
                account_id: account_id.to_string(),
                active_pid: Some(pid),
                launch: Some(LaunchSnapshot {
                    pid,
                    mod_args: mod_args.to_string(),
                }),
            },
        );
        state.revision = state.revision.wrapping_add(1);
    }

    pub fn record_discovered(&self, account_id: &str, pid: u32) {
        if pid == 0 {
            return;
        }
        let key = account_key(account_id);
        let mut state = self.state.write();
        state
            .entries
            .retain(|existing_key, entry| existing_key == &key || entry.active_pid != Some(pid));
        let launch = state
            .entries
            .get(&key)
            .and_then(|entry| entry.launch.clone())
            .filter(|launch| launch.pid == pid);
        state.entries.insert(
            key,
            InstanceEntry {
                account_id: account_id.to_string(),
                active_pid: Some(pid),
                launch,
            },
        );
        state.revision = state.revision.wrapping_add(1);
    }

    pub fn record_launch_snapshot(&self, account_id: &str, pid: u32, mod_args: &str) {
        if pid == 0 {
            return;
        }
        let key = account_key(account_id);
        let mut state = self.state.write();
        let entry = state.entries.entry(key).or_insert_with(|| InstanceEntry {
            account_id: account_id.to_string(),
            active_pid: None,
            launch: None,
        });
        entry.account_id = account_id.to_string();
        entry.launch = Some(LaunchSnapshot {
            pid,
            mod_args: mod_args.to_string(),
        });
        state.revision = state.revision.wrapping_add(1);
    }

    pub fn remove(&self, account_id: &str) {
        let mut state = self.state.write();
        if state.entries.remove(&account_key(account_id)).is_some() {
            state.revision = state.revision.wrapping_add(1);
        }
    }

    pub fn remove_if_pid(&self, account_id: &str, pid: u32) -> bool {
        let key = account_key(account_id);
        let mut state = self.state.write();
        if state.entries.get(&key).and_then(|entry| entry.active_pid) != Some(pid) {
            return false;
        }
        state.entries.remove(&key);
        state.revision = state.revision.wrapping_add(1);
        true
    }

    /// Replaces a process-scan result only if the registry still matches the
    /// snapshot taken before that scan. `None` means a newer mutation won and
    /// the caller must use the current registry rather than stale observations.
    pub fn reconcile_if_unchanged<I>(
        &self,
        snapshot: &InstanceRegistrySnapshot,
        detected: I,
    ) -> Option<Vec<RunningInstance>>
    where
        I: IntoIterator<Item = (String, u32)>,
    {
        let mut state = self.state.write();
        if state.revision != snapshot.revision {
            return None;
        }

        let previous = std::mem::take(&mut state.entries);
        let mut claimed_pids = HashSet::new();

        for (account_id, pid) in detected {
            if pid == 0 || !claimed_pids.insert(pid) {
                continue;
            }
            let key = account_key(&account_id);
            if state.entries.contains_key(&key) {
                continue;
            }
            let launch = previous
                .get(&key)
                .and_then(|entry| entry.launch.clone())
                .filter(|launch| launch.pid == pid);
            state.entries.insert(
                key,
                InstanceEntry {
                    account_id,
                    active_pid: Some(pid),
                    launch,
                },
            );
        }
        state.revision = state.revision.wrapping_add(1);
        Some(running_instances(&state.entries))
    }
}

impl InstanceStatusPort for InstanceRegistry {
    fn find(&self, account_id: &str) -> Option<RunningInstance> {
        self.get(account_id)
    }

    fn list(&self) -> Vec<RunningInstance> {
        InstanceRegistry::list(self)
    }
}

fn running_instances(entries: &HashMap<String, InstanceEntry>) -> Vec<RunningInstance> {
    let mut instances = entries
        .values()
        .filter_map(|entry| {
            let pid = entry.active_pid?;
            Some(RunningInstance {
                account_id: entry.account_id.clone(),
                pid,
                launch: entry.launch.clone().filter(|launch| launch.pid == pid),
            })
        })
        .collect::<Vec<_>>();
    instances.sort_by(|left, right| {
        left.account_id
            .to_ascii_lowercase()
            .cmp(&right.account_id.to_ascii_lowercase())
    });
    instances
}

#[cfg(test)]
mod tests {
    use super::InstanceRegistry;

    #[test]
    fn launched_instance_records_pid_and_arguments_atomically() {
        let registry = InstanceRegistry::default();
        registry.record_launched("Account-A", 42, "-mod telemetry -txt");

        let instance = registry.get("account-a").unwrap();
        assert_eq!(instance.pid, 42);
        assert_eq!(instance.launch.unwrap().mod_args, "-mod telemetry -txt");
    }

    #[test]
    fn discovered_pid_change_discards_an_old_launch_snapshot() {
        let registry = InstanceRegistry::default();
        registry.record_launched("account", 42, "-mod old");
        registry.record_discovered("account", 43);

        let instance = registry.get("account").unwrap();
        assert_eq!(instance.pid, 43);
        assert!(instance.launch.is_none());
    }

    #[test]
    fn conditional_remove_does_not_delete_a_newer_instance() {
        let registry = InstanceRegistry::default();
        registry.record_discovered("account", 42);
        registry.record_discovered("account", 43);

        assert!(!registry.remove_if_pid("account", 42));
        assert_eq!(registry.pid_for("account"), Some(43));
    }

    #[test]
    fn reconciliation_preserves_only_matching_trusted_snapshots() {
        let registry = InstanceRegistry::default();
        registry.record_launched("one", 10, "-mod one");
        registry.record_launched("two", 20, "-mod two");

        let snapshot = registry.snapshot();
        registry.reconcile_if_unchanged(
            &snapshot,
            [
                ("ONE".to_string(), 10),
                ("two".to_string(), 21),
                ("duplicate-pid".to_string(), 21),
            ],
        );

        assert_eq!(
            registry.get("one").unwrap().launch.unwrap().mod_args,
            "-mod one"
        );
        assert!(registry.get("two").unwrap().launch.is_none());
        assert!(registry.get("duplicate-pid").is_none());
    }

    #[test]
    fn one_pid_cannot_belong_to_two_accounts() {
        let registry = InstanceRegistry::default();
        registry.record_discovered("one", 42);
        registry.record_discovered("two", 42);

        assert!(registry.get("one").is_none());
        assert_eq!(registry.pid_for("two"), Some(42));
    }

    #[test]
    fn stale_scan_cannot_overwrite_a_new_launch() {
        let registry = InstanceRegistry::default();
        registry.record_discovered("old", 10);
        let stale_scan = registry.snapshot();

        registry.record_launched("new", 20, "-mod new");

        assert!(registry
            .reconcile_if_unchanged(&stale_scan, [("old".to_string(), 10)])
            .is_none());
        assert_eq!(registry.pid_for("new"), Some(20));
        assert_eq!(
            registry.get("new").unwrap().launch.unwrap().mod_args,
            "-mod new"
        );
    }
}
