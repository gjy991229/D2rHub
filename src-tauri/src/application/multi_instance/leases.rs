use parking_lot::{Mutex, MutexGuard};
use std::collections::HashSet;
use std::sync::Arc;

use crate::error::AppError;

#[derive(Default)]
struct AccountLeaseState {
    active: Mutex<HashSet<String>>,
}

/// Core-owned lease manager for account lifecycle mutations.
///
/// Launch, delete, settings replacement and capability workflows all acquire
/// from this one source so a module cannot race an account transaction.
#[derive(Clone, Default)]
pub struct AccountLeaseManager {
    state: Arc<AccountLeaseState>,
}

pub struct AccountOperationLease {
    state: Arc<AccountLeaseState>,
    account_id: String,
}

/// A deterministically ordered all-or-nothing lease set.
/// Dropping it releases every account even after an early workflow failure.
pub struct AccountOperationLeases {
    _leases: Vec<AccountOperationLease>,
}

/// Serializes writes whose invariants span multiple account directories, such
/// as display-name uniqueness and import/create/delete catalog updates.
#[derive(Default)]
pub struct AccountCatalogLeaseManager {
    state: Mutex<()>,
}

pub(crate) struct AccountCatalogLease<'a> {
    _guard: MutexGuard<'a, ()>,
}

fn account_key(account_id: &str) -> String {
    account_id.trim().to_ascii_lowercase()
}

impl AccountLeaseManager {
    pub fn try_acquire(&self, account_id: &str) -> Result<AccountOperationLease, AppError> {
        let operation_key = account_key(account_id);
        if operation_key.is_empty() {
            return Err(AppError::Unknown("账号 ID 不能为空".to_string()));
        }
        let mut active = self.state.active.lock();
        if !active.insert(operation_key.clone()) {
            return Err(AppError::Unknown(format!(
                "账号 {account_id} 正在执行另一项操作，请稍后重试"
            )));
        }
        drop(active);
        Ok(AccountOperationLease {
            state: Arc::clone(&self.state),
            account_id: operation_key,
        })
    }

    pub fn try_acquire_many<I, S>(&self, account_ids: I) -> Result<AccountOperationLeases, AppError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut ids = account_ids
            .into_iter()
            .map(|account_id| account_key(account_id.as_ref()))
            .collect::<Vec<_>>();
        if ids.iter().any(String::is_empty) {
            return Err(AppError::Unknown("账号 ID 不能为空".to_string()));
        }
        ids.sort();
        ids.dedup();

        // Check and reserve the whole set while holding one lock. No observer
        // can see a partially acquired workflow lease set.
        let mut active = self.state.active.lock();
        if let Some(account_id) = ids.iter().find(|account_id| active.contains(*account_id)) {
            return Err(AppError::Unknown(format!(
                "账号 {account_id} 正在执行另一项操作，请稍后重试"
            )));
        }
        active.extend(ids.iter().cloned());
        drop(active);

        let leases = ids
            .into_iter()
            .map(|account_id| AccountOperationLease {
                state: Arc::clone(&self.state),
                account_id,
            })
            .collect();
        Ok(AccountOperationLeases { _leases: leases })
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.state.active.lock().is_empty()
    }

    #[cfg(test)]
    pub fn contains(&self, account_id: &str) -> bool {
        self.state.active.lock().contains(&account_key(account_id))
    }
}

impl AccountCatalogLeaseManager {
    pub fn acquire(&self) -> AccountCatalogLease<'_> {
        AccountCatalogLease {
            _guard: self.state.lock(),
        }
    }

    #[cfg(test)]
    pub fn try_acquire(&self) -> Option<AccountCatalogLease<'_>> {
        self.state
            .try_lock()
            .map(|guard| AccountCatalogLease { _guard: guard })
    }
}

impl Drop for AccountOperationLease {
    fn drop(&mut self) {
        self.state.active.lock().remove(&self.account_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountCatalogLeaseManager, AccountLeaseManager};

    #[test]
    fn account_aliases_share_one_core_lease() {
        let manager = AccountLeaseManager::default();
        let lease = manager.try_acquire("ACCOUNT-A").unwrap();
        assert!(manager.try_acquire("account-a").is_err());
        drop(lease);
        assert!(manager.try_acquire("account-a").is_ok());
    }

    #[test]
    fn lease_sets_are_atomic_and_release_partial_acquisitions() {
        let manager = AccountLeaseManager::default();
        let blocker = manager.try_acquire("account-b").unwrap();
        assert!(manager
            .try_acquire_many(["account-a", "account-b", "account-c"])
            .is_err());
        assert!(manager.try_acquire("account-a").is_ok());
        assert!(manager.try_acquire("account-c").is_ok());
        drop(blocker);
    }

    #[test]
    fn lease_sets_deduplicate_case_aliases() {
        let manager = AccountLeaseManager::default();
        let leases = manager
            .try_acquire_many(["ACCOUNT-A", "account-a", "account-b"])
            .unwrap();
        assert!(manager.try_acquire("account-a").is_err());
        assert!(manager.try_acquire("account-b").is_err());
        drop(leases);
        assert!(manager.try_acquire("account-a").is_ok());
    }

    #[test]
    fn empty_account_ids_are_rejected_without_reserving_anything() {
        let manager = AccountLeaseManager::default();
        assert!(manager.try_acquire("").is_err());
        assert!(manager.try_acquire_many(["account-a", "  "]).is_err());
        assert!(manager.try_acquire("account-a").is_ok());
    }

    #[test]
    fn catalog_lease_is_exclusive_and_released_on_drop() {
        let manager = AccountCatalogLeaseManager::default();
        let lease = manager.acquire();
        assert!(manager.try_acquire().is_none());
        drop(lease);
        assert!(manager.try_acquire().is_some());
    }
}
