use parking_lot::{Mutex, MutexGuard};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::AppError;

#[derive(Default)]
struct AccountLeaseState {
    active: Mutex<HashMap<String, AccountLeaseEntry>>,
    restart_reserved: AtomicBool,
}

struct AccountRestartReservation {
    state: Arc<AccountLeaseState>,
}

impl Drop for AccountRestartReservation {
    fn drop(&mut self) {
        self.state.restart_reserved.store(false, Ordering::Release);
    }
}

#[derive(Default)]
struct AccountLeaseEntry {
    readers: usize,
    writer: bool,
}

#[derive(Clone, Copy)]
enum AccountLeaseKind {
    Read,
    Write,
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
    kind: AccountLeaseKind,
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
    state: Mutex<bool>,
}

pub(crate) struct AccountCatalogLease<'a> {
    _guard: MutexGuard<'a, bool>,
}

struct CatalogRestartReservation<'a>(&'a AccountCatalogLeaseManager);

impl Drop for CatalogRestartReservation<'_> {
    fn drop(&mut self) { *self.0.state.lock() = false; }
}

fn account_key(account_id: &str) -> String {
    account_id.trim().to_ascii_lowercase()
}

impl AccountLeaseManager {
    /// Holds admission closed until restart; existing operations must finish
    /// first. Dropping the guard on preparation failure reopens admission.
    pub(crate) fn freeze_for_restart(&self) -> Result<impl Sized + '_, String> {
        let active = self.state.active.try_lock()
            .ok_or_else(|| "账号操作进行中，请完成后再切换模式".to_string())?;
        if !active.is_empty() {
            return Err("账号启动或保存进行中，请完成后再切换模式".to_string());
        }
        self.state.restart_reserved.store(true, Ordering::Release);
        Ok(AccountRestartReservation { state: Arc::clone(&self.state) })
    }

    pub fn try_acquire(&self, account_id: &str) -> Result<AccountOperationLease, AppError> {
        let operation_key = account_key(account_id);
        if operation_key.is_empty() {
            return Err(AppError::Unknown("账号 ID 不能为空".to_string()));
        }
        let mut active = self.state.active.lock();
        if self.state.restart_reserved.load(Ordering::Acquire) {
            return Err(AppError::Unknown("模式切换准备中，请稍候".to_string()));
        }
        if active
            .get(&operation_key)
            .is_some_and(|entry| entry.writer || entry.readers > 0)
        {
            return Err(AppError::Unknown(format!(
                "账号 {account_id} 正在执行另一项操作，请稍后重试"
            )));
        }
        active.insert(
            operation_key.clone(),
            AccountLeaseEntry {
                readers: 0,
                writer: true,
            },
        );
        drop(active);
        Ok(AccountOperationLease {
            state: Arc::clone(&self.state),
            account_id: operation_key,
            kind: AccountLeaseKind::Write,
        })
    }

    /// Acquires a shared account snapshot lease. Concurrent readers may proceed,
    /// while directory swaps and other account mutations remain exclusive.
    pub fn try_acquire_read(&self, account_id: &str) -> Result<AccountOperationLease, AppError> {
        let operation_key = account_key(account_id);
        if operation_key.is_empty() {
            return Err(AppError::Unknown("账号 ID 不能为空".to_string()));
        }
        let mut active = self.state.active.lock();
        if self.state.restart_reserved.load(Ordering::Acquire) {
            return Err(AppError::Unknown("模式切换准备中，请稍候".to_string()));
        }
        let entry = active.entry(operation_key.clone()).or_default();
        if entry.writer {
            return Err(AppError::Unknown(format!(
                "账号 {account_id} 正在执行另一项操作，请稍后重试"
            )));
        }
        entry.readers = entry
            .readers
            .checked_add(1)
            .ok_or_else(|| AppError::Unknown("账号读取租约计数溢出".to_string()))?;
        drop(active);
        Ok(AccountOperationLease {
            state: Arc::clone(&self.state),
            account_id: operation_key,
            kind: AccountLeaseKind::Read,
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
        if self.state.restart_reserved.load(Ordering::Acquire) {
            return Err(AppError::Unknown("模式切换准备中，请稍候".to_string()));
        }
        if let Some(account_id) = ids.iter().find(|account_id| {
            active
                .get(*account_id)
                .is_some_and(|entry| entry.writer || entry.readers > 0)
        }) {
            return Err(AppError::Unknown(format!(
                "账号 {account_id} 正在执行另一项操作，请稍后重试"
            )));
        }
        active.extend(ids.iter().cloned().map(|account_id| {
            (
                account_id,
                AccountLeaseEntry {
                    readers: 0,
                    writer: true,
                },
            )
        }));
        drop(active);

        let leases = ids
            .into_iter()
            .map(|account_id| AccountOperationLease {
                state: Arc::clone(&self.state),
                account_id,
                kind: AccountLeaseKind::Write,
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
        self.state
            .active
            .lock()
            .contains_key(&account_key(account_id))
    }
}

impl AccountCatalogLeaseManager {
    pub fn acquire(&self) -> Result<AccountCatalogLease<'_>, AppError> {
        let guard = self.state.lock();
        if *guard { return Err(AppError::Unknown("模式切换准备中，请稍候".to_string())); }
        Ok(AccountCatalogLease { _guard: guard })
    }

    pub(crate) fn freeze_for_restart(&self) -> Result<impl Sized + '_, String> {
        let mut guard = self.state.try_lock().ok_or("账号目录更新中，请完成后再切换模式")?;
        if *guard { return Err("模式切换已经开始".to_string()); }
        *guard = true;
        Ok(CatalogRestartReservation(self))
    }

    pub fn try_acquire(&self) -> Option<AccountCatalogLease<'_>> {
        self.state
            .try_lock()
            .filter(|guard| !**guard)
            .map(|guard| AccountCatalogLease { _guard: guard })
    }
}

impl Drop for AccountOperationLease {
    fn drop(&mut self) {
        let mut active = self.state.active.lock();
        let remove = if let Some(entry) = active.get_mut(&self.account_id) {
            match self.kind {
                AccountLeaseKind::Read => {
                    if entry.readers > 0 {
                        entry.readers -= 1;
                    }
                }
                AccountLeaseKind::Write => entry.writer = false,
            }
            !entry.writer && entry.readers == 0
        } else {
            false
        };
        if remove {
            active.remove(&self.account_id);
        }
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
