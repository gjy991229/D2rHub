use crate::domain::account::AccountMeta;
use crate::domain::config::GlobalConfig;
use crate::error::AppError;

use super::{AccountCatalog, AccountRuntimePort};

/// Joins persisted account metadata with verified live instance state.
///
/// The catalog remains the source of persisted account fields. Runtime flags are always replaced
/// from the instance registry and process verifier so stale values in historical `account.json`
/// files can never be presented as a live process.
pub struct AccountQueryService<'a> {
    accounts: &'a dyn AccountCatalog,
    runtime: &'a dyn AccountRuntimePort,
}

impl<'a> AccountQueryService<'a> {
    pub fn new(accounts: &'a dyn AccountCatalog, runtime: &'a dyn AccountRuntimePort) -> Self {
        Self { accounts, runtime }
    }

    pub fn list(&self, config: &GlobalConfig) -> Result<Vec<AccountMeta>, AppError> {
        let mut accounts = self.accounts.list()?;
        for account in &mut accounts {
            let candidate_pid = self.runtime.registered_pid(&account.id);
            let is_running = candidate_pid
                .is_some_and(|pid| self.runtime.is_expected_game_process(config, account, pid));

            if !is_running {
                if let Some(pid) = candidate_pid {
                    // Conditional removal prevents a process-check result for an old PID from
                    // deleting a newer instance registered concurrently for the same account.
                    self.runtime.remove_if_pid(&account.id, pid);
                }
            }

            account.is_running = is_running;
            account.running_pid = candidate_pid.filter(|_| is_running);
            account.token = None;
        }
        Ok(accounts)
    }

    pub fn get(&self, account_id: &str) -> Result<AccountMeta, AppError> {
        let mut account = self.accounts.get(account_id)?;
        account.token = None;
        Ok(account)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    use super::AccountQueryService;
    use crate::application::multi_instance::{AccountCatalog, AccountRuntimePort};
    use crate::domain::account::AccountMeta;
    use crate::domain::config::GlobalConfig;
    use crate::error::AppError;

    struct FakeAccountCatalog {
        accounts: Vec<AccountMeta>,
    }

    impl AccountCatalog for FakeAccountCatalog {
        fn list_account_ids(&self) -> Result<Vec<String>, AppError> {
            Ok(self
                .accounts
                .iter()
                .map(|account| account.id.clone())
                .collect())
        }

        fn list(&self) -> Result<Vec<AccountMeta>, AppError> {
            Ok(self.accounts.clone())
        }

        fn get(&self, account_id: &str) -> Result<AccountMeta, AppError> {
            self.accounts
                .iter()
                .find(|account| account.id.eq_ignore_ascii_case(account_id))
                .cloned()
                .ok_or_else(|| AppError::AccountNotFound(account_id.to_string()))
        }
    }

    #[derive(Default)]
    struct FakeAccountRuntime {
        pids: HashMap<String, u32>,
        matching_pids: HashSet<u32>,
        removals: Mutex<Vec<(String, u32)>>,
    }

    impl AccountRuntimePort for FakeAccountRuntime {
        fn registered_pid(&self, account_id: &str) -> Option<u32> {
            self.pids.get(&account_id.to_ascii_lowercase()).copied()
        }

        fn is_expected_game_process(
            &self,
            _config: &GlobalConfig,
            _account: &AccountMeta,
            pid: u32,
        ) -> bool {
            self.matching_pids.contains(&pid)
        }

        fn remove_if_pid(&self, account_id: &str, pid: u32) -> bool {
            self.removals
                .lock()
                .expect("removal log lock")
                .push((account_id.to_string(), pid));
            true
        }
    }

    fn account(id: &str, token: &str) -> AccountMeta {
        let mut account = AccountMeta::new(id);
        account.display_name = format!("Player {id}");
        account.token = Some(token.to_string());
        // Historical files may contain stale runtime fields. A list query must replace them.
        account.is_running = true;
        account.running_pid = Some(999);
        account
    }

    #[test]
    fn list_replaces_stale_runtime_fields_and_redacts_tokens() {
        let catalog = FakeAccountCatalog {
            accounts: vec![
                account("acount1", "secret-1"),
                account("acount2", "secret-2"),
            ],
        };
        let runtime = FakeAccountRuntime {
            pids: HashMap::from([("acount1".to_string(), 41), ("acount2".to_string(), 42)]),
            matching_pids: HashSet::from([41]),
            ..FakeAccountRuntime::default()
        };
        let service = AccountQueryService::new(&catalog, &runtime);

        let accounts = service.list(&GlobalConfig::default()).unwrap();

        assert!(accounts[0].is_running);
        assert_eq!(accounts[0].running_pid, Some(41));
        assert!(!accounts[1].is_running);
        assert_eq!(accounts[1].running_pid, None);
        assert!(accounts.iter().all(|account| account.token.is_none()));
        assert_eq!(
            runtime.removals.into_inner().unwrap(),
            [("acount2".to_string(), 42)]
        );
    }

    #[test]
    fn stale_candidate_pid_is_forwarded_to_conditional_removal() {
        let catalog = FakeAccountCatalog {
            accounts: vec![account("acount1", "secret")],
        };
        let runtime = FakeAccountRuntime {
            pids: HashMap::from([("acount1".to_string(), 77)]),
            ..FakeAccountRuntime::default()
        };

        AccountQueryService::new(&catalog, &runtime)
            .list(&GlobalConfig::default())
            .unwrap();

        assert_eq!(
            runtime.removals.into_inner().unwrap(),
            [("acount1".to_string(), 77)]
        );
    }

    #[test]
    fn single_account_query_keeps_persisted_shape_but_removes_the_secret() {
        let catalog = FakeAccountCatalog {
            accounts: vec![account("acount1", "secret")],
        };
        let runtime = FakeAccountRuntime::default();
        let account = AccountQueryService::new(&catalog, &runtime)
            .get("ACOUNT1")
            .unwrap();
        let serialized = serde_json::to_value(account).unwrap();

        assert_eq!(serialized["id"], "acount1");
        assert!(serialized.get("token").is_none());
        // `get_account` historically returns the persisted runtime compatibility fields rather
        // than performing a process scan; keep that behavior until its IPC contract is revised.
        assert_eq!(serialized["is_running"], true);
        assert_eq!(serialized["running_pid"], 999);
    }
}
