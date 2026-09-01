use crate::domain::account::{is_valid_account_id, AccountMeta};
use crate::error::AppError;

use super::AccountLeaseManager;

/// Persistence boundary for the core account ordering use case.
///
/// The application layer owns validation, conflict control and rollback. The
/// filesystem adapter only loads and saves one account metadata record.
pub trait AccountOrderRepository: Send + Sync {
    fn load(&self, account_id: &str) -> Result<AccountMeta, AppError>;
    fn save(&self, account: &AccountMeta) -> Result<(), AppError>;
}

pub struct AccountOrderingService<'a> {
    accounts: &'a dyn AccountOrderRepository,
    leases: &'a AccountLeaseManager,
}

impl<'a> AccountOrderingService<'a> {
    pub fn new(accounts: &'a dyn AccountOrderRepository, leases: &'a AccountLeaseManager) -> Self {
        Self { accounts, leases }
    }

    pub fn reorder(&self, ordered_ids: &[String]) -> Result<(), AppError> {
        validate_ordered_ids(ordered_ids)?;

        // Reserve the complete set atomically. No concurrent operation can
        // observe a partially locked reorder request.
        let _leases = self.leases.try_acquire_many(ordered_ids)?;

        // Load the complete input before the first write so a missing or
        // damaged account cannot produce a partial reorder.
        let originals = ordered_ids
            .iter()
            .map(|account_id| self.accounts.load(account_id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut updated = originals.clone();
        for (index, account) in updated.iter_mut().enumerate() {
            account.order = index as u32;
        }

        for (index, account) in updated.iter().enumerate() {
            if let Err(write_error) = self.accounts.save(account) {
                let rollback_errors = originals[..index]
                    .iter()
                    .filter_map(|original| {
                        self.accounts
                            .save(original)
                            .err()
                            .map(|error| format!("{}: {error}", original.id))
                    })
                    .collect::<Vec<_>>();

                if rollback_errors.is_empty() {
                    return Err(write_error);
                }
                return Err(AppError::FileError(format!(
                    "账号排序写入失败且部分回滚失败。写入错误: {write_error}；回滚错误: {}",
                    rollback_errors.join("；")
                )));
            }
        }

        Ok(())
    }
}

fn validate_ordered_ids(ordered_ids: &[String]) -> Result<(), AppError> {
    let mut canonical_ids = Vec::with_capacity(ordered_ids.len());
    for account_id in ordered_ids {
        if !is_valid_account_id(account_id) {
            return Err(AppError::FileError(format!("账号 ID 非法: {account_id}")));
        }
        canonical_ids.push(account_id.to_ascii_lowercase());
    }
    canonical_ids.sort();
    canonical_ids.dedup();
    if canonical_ids.len() != ordered_ids.len() {
        return Err(AppError::ConfigWriteError(
            "账号排序列表包含重复账号，已拒绝写入".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    use super::{AccountOrderRepository, AccountOrderingService};
    use crate::application::multi_instance::AccountLeaseManager;
    use crate::domain::account::AccountMeta;
    use crate::error::AppError;

    struct FakeRepository {
        accounts: Mutex<HashMap<String, AccountMeta>>,
        saves: Mutex<Vec<(String, u32)>>,
        fail_on_orders: HashSet<u32>,
    }

    impl FakeRepository {
        fn new(accounts: impl IntoIterator<Item = AccountMeta>) -> Self {
            Self {
                accounts: Mutex::new(
                    accounts
                        .into_iter()
                        .map(|account| (account.id.to_ascii_lowercase(), account))
                        .collect(),
                ),
                saves: Mutex::new(Vec::new()),
                fail_on_orders: HashSet::new(),
            }
        }

        fn failing_on(mut self, orders: impl IntoIterator<Item = u32>) -> Self {
            self.fail_on_orders.extend(orders);
            self
        }

        fn order(&self, account_id: &str) -> u32 {
            self.accounts.lock().unwrap()[&account_id.to_ascii_lowercase()].order
        }
    }

    impl AccountOrderRepository for FakeRepository {
        fn load(&self, account_id: &str) -> Result<AccountMeta, AppError> {
            self.accounts
                .lock()
                .unwrap()
                .get(&account_id.to_ascii_lowercase())
                .cloned()
                .ok_or_else(|| AppError::AccountNotFound(account_id.to_string()))
        }

        fn save(&self, account: &AccountMeta) -> Result<(), AppError> {
            self.saves
                .lock()
                .unwrap()
                .push((account.id.clone(), account.order));
            if self.fail_on_orders.contains(&account.order) {
                return Err(AppError::FileError(format!(
                    "refused order {}",
                    account.order
                )));
            }
            self.accounts
                .lock()
                .unwrap()
                .insert(account.id.to_ascii_lowercase(), account.clone());
            Ok(())
        }
    }

    fn account(id: &str, order: u32) -> AccountMeta {
        let mut account = AccountMeta::new(id);
        account.order = order;
        account
    }

    #[test]
    fn reorder_validates_and_persists_the_requested_sequence() {
        let repository = FakeRepository::new([
            account("acount1", 10),
            account("acount2", 11),
            account("acount3", 12),
        ]);
        let leases = AccountLeaseManager::default();
        let service = AccountOrderingService::new(&repository, &leases);

        service
            .reorder(&[
                "acount3".to_string(),
                "acount1".to_string(),
                "acount2".to_string(),
            ])
            .unwrap();

        assert_eq!(repository.order("acount3"), 0);
        assert_eq!(repository.order("acount1"), 1);
        assert_eq!(repository.order("acount2"), 2);
        assert!(leases.is_empty());
    }

    #[test]
    fn aliases_are_rejected_before_any_account_is_loaded_or_saved() {
        let lowercase = "550e8400-e29b-41d4-a716-446655440000";
        let uppercase = "550E8400-E29B-41D4-A716-446655440000";
        let repository = FakeRepository::new([account(lowercase, 7)]);
        let leases = AccountLeaseManager::default();
        let service = AccountOrderingService::new(&repository, &leases);

        let error = service
            .reorder(&[lowercase.to_string(), uppercase.to_string()])
            .unwrap_err();

        assert!(error.to_string().contains("重复账号"));
        assert!(repository.saves.lock().unwrap().is_empty());
        assert!(leases.is_empty());
    }

    #[test]
    fn missing_input_fails_before_the_first_write() {
        let repository = FakeRepository::new([account("acount1", 7)]);
        let leases = AccountLeaseManager::default();
        let service = AccountOrderingService::new(&repository, &leases);

        assert!(service
            .reorder(&["acount1".to_string(), "acount2".to_string()])
            .is_err());
        assert!(repository.saves.lock().unwrap().is_empty());
        assert_eq!(repository.order("acount1"), 7);
        assert!(leases.is_empty());
    }

    #[test]
    fn a_failed_write_rolls_back_every_earlier_account() {
        let repository = FakeRepository::new([
            account("acount1", 10),
            account("acount2", 11),
            account("acount3", 12),
        ])
        .failing_on([1]);
        let leases = AccountLeaseManager::default();
        let service = AccountOrderingService::new(&repository, &leases);

        assert!(service
            .reorder(&[
                "acount1".to_string(),
                "acount2".to_string(),
                "acount3".to_string(),
            ])
            .is_err());
        assert_eq!(repository.order("acount1"), 10);
        assert_eq!(repository.order("acount2"), 11);
        assert_eq!(repository.order("acount3"), 12);
        assert!(leases.is_empty());
    }

    #[test]
    fn an_existing_account_operation_blocks_the_whole_reorder_without_writes() {
        let repository = FakeRepository::new([account("acount1", 10), account("acount2", 11)]);
        let leases = AccountLeaseManager::default();
        let blocker = leases.try_acquire("acount2").unwrap();
        let service = AccountOrderingService::new(&repository, &leases);

        assert!(service
            .reorder(&["acount1".to_string(), "acount2".to_string()])
            .is_err());
        assert!(repository.saves.lock().unwrap().is_empty());
        assert!(!leases.contains("acount1"));
        drop(blocker);
    }
}
