use crate::error::AppError;

use super::{
    AccountCatalogLeaseManager, AccountDeletionCleanupPort, AccountDeletionTransaction,
    AccountLeaseManager,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDeletionWarning {
    pub component: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDeletionOutcome {
    pub account_id: String,
    pub warnings: Vec<AccountDeletionWarning>,
}

/// Coordinates the complete account lifecycle without depending on Tauri,
/// Windows, browser-profile layout, or a concrete capability registry.
///
/// The transaction adapter owns the recoverable filesystem/configuration
/// commit. This service owns the global lock order and guarantees that optional
/// capability callbacks run only after the account lease is released.
pub struct AccountDeletionService<'a> {
    transaction: &'a dyn AccountDeletionTransaction,
    cleanup: &'a dyn AccountDeletionCleanupPort,
    catalog_leases: &'a AccountCatalogLeaseManager,
    account_leases: &'a AccountLeaseManager,
}

impl<'a> AccountDeletionService<'a> {
    pub fn new(
        transaction: &'a dyn AccountDeletionTransaction,
        cleanup: &'a dyn AccountDeletionCleanupPort,
        catalog_leases: &'a AccountCatalogLeaseManager,
        account_leases: &'a AccountLeaseManager,
    ) -> Self {
        Self {
            transaction,
            cleanup,
            catalog_leases,
            account_leases,
        }
    }

    pub fn delete(&self, requested_account_id: &str) -> Result<AccountDeletionOutcome, AppError> {
        // Cross-account/catalog operations use one documented order everywhere:
        // catalog -> account -> configuration/disk transaction.
        let catalog_lease = self.catalog_leases.acquire();
        let account_lease = self.account_leases.try_acquire(requested_account_id)?;
        let deleted_account_id = self.transaction.delete(requested_account_id)?;

        // Browser cleanup no longer needs the catalog lock, but it still runs
        // before the account lease is released so no same-account operation can
        // recreate the profile while deletion is being finalized.
        drop(catalog_lease);
        let mut warnings = Vec::new();
        if let Err(message) = self.cleanup.remove_browser_profiles(&deleted_account_id) {
            warnings.push(AccountDeletionWarning {
                component: "browser-profiles".to_string(),
                message,
            });
        }
        self.cleanup.remove_runtime_instance(&deleted_account_id);

        // Optional modules may acquire their own account/configuration resources.
        // Releasing the core account lease first prevents dependency inversion.
        drop(account_lease);
        warnings.extend(
            self.cleanup
                .notify_account_removed(&deleted_account_id)
                .into_iter()
                .map(|(component, message)| AccountDeletionWarning { component, message }),
        );

        Ok(AccountDeletionOutcome {
            account_id: deleted_account_id,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::AccountDeletionService;
    use crate::application::multi_instance::{
        AccountCatalogLeaseManager, AccountDeletionCleanupPort, AccountDeletionTransaction,
        AccountLeaseManager,
    };
    use crate::error::AppError;

    struct FakeTransaction<'a> {
        account_leases: &'a AccountLeaseManager,
        catalog_leases: &'a AccountCatalogLeaseManager,
        fail: bool,
    }

    impl AccountDeletionTransaction for FakeTransaction<'_> {
        fn delete(&self, requested_account_id: &str) -> Result<String, AppError> {
            assert!(self.account_leases.contains(requested_account_id));
            assert!(self.catalog_leases.try_acquire().is_none());
            if self.fail {
                Err(AppError::FileError(
                    "injected transaction failure".to_string(),
                ))
            } else {
                Ok(requested_account_id.to_ascii_lowercase())
            }
        }
    }

    #[derive(Default)]
    struct FakeCleanup<'a> {
        account_leases: Option<&'a AccountLeaseManager>,
        calls: Mutex<Vec<String>>,
        browser_failure: bool,
    }

    impl AccountDeletionCleanupPort for FakeCleanup<'_> {
        fn remove_browser_profiles(&self, account_id: &str) -> Result<(), String> {
            assert!(self.account_leases.unwrap().contains(account_id));
            self.calls.lock().unwrap().push("browser".to_string());
            self.browser_failure
                .then(|| "profile busy".to_string())
                .map_or(Ok(()), Err)
        }

        fn remove_runtime_instance(&self, account_id: &str) {
            assert!(self.account_leases.unwrap().contains(account_id));
            self.calls.lock().unwrap().push("runtime".to_string());
        }

        fn notify_account_removed(&self, account_id: &str) -> Vec<(String, String)> {
            assert!(!self.account_leases.unwrap().contains(account_id));
            self.calls.lock().unwrap().push("capabilities".to_string());
            vec![("optional-module".to_string(), "cleanup failed".to_string())]
        }
    }

    #[test]
    fn deletion_uses_the_shared_lock_order_and_releases_before_capabilities() {
        let catalog_leases = AccountCatalogLeaseManager::default();
        let account_leases = AccountLeaseManager::default();
        let transaction = FakeTransaction {
            account_leases: &account_leases,
            catalog_leases: &catalog_leases,
            fail: false,
        };
        let cleanup = FakeCleanup {
            account_leases: Some(&account_leases),
            browser_failure: true,
            ..FakeCleanup::default()
        };

        let outcome =
            AccountDeletionService::new(&transaction, &cleanup, &catalog_leases, &account_leases)
                .delete("ACCOUNT-A")
                .unwrap();

        assert_eq!(outcome.account_id, "account-a");
        assert_eq!(
            *cleanup.calls.lock().unwrap(),
            ["browser", "runtime", "capabilities"]
        );
        assert_eq!(outcome.warnings.len(), 2);
        assert!(account_leases.is_empty());
        assert!(catalog_leases.try_acquire().is_some());
    }

    #[test]
    fn a_failed_transaction_runs_no_cleanup_and_releases_both_leases() {
        let catalog_leases = AccountCatalogLeaseManager::default();
        let account_leases = AccountLeaseManager::default();
        let transaction = FakeTransaction {
            account_leases: &account_leases,
            catalog_leases: &catalog_leases,
            fail: true,
        };
        let cleanup = FakeCleanup {
            account_leases: Some(&account_leases),
            ..FakeCleanup::default()
        };

        assert!(AccountDeletionService::new(
            &transaction,
            &cleanup,
            &catalog_leases,
            &account_leases,
        )
        .delete("account-a")
        .is_err());

        assert!(cleanup.calls.lock().unwrap().is_empty());
        assert!(account_leases.is_empty());
        assert!(catalog_leases.try_acquire().is_some());
    }
}
