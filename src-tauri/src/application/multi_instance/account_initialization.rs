use crate::error::AppError;

use super::{AccountInitializationTransaction, AccountLeaseManager, CancellationTicket};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountInitializationKind {
    New,
    Reinitialize,
}

/// Core entry point shared by first-time Battle.net initialization and reset.
/// It reserves the account for the entire host transaction; the concrete
/// adapter owns Battle.net, registry, browser, and filesystem operations.
pub struct AccountInitializationService<'a> {
    transaction: &'a dyn AccountInitializationTransaction,
    account_leases: &'a AccountLeaseManager,
}

impl<'a> AccountInitializationService<'a> {
    pub fn new(
        transaction: &'a dyn AccountInitializationTransaction,
        account_leases: &'a AccountLeaseManager,
    ) -> Self {
        Self {
            transaction,
            account_leases,
        }
    }

    pub fn execute(
        &self,
        account_id: &str,
        kind: AccountInitializationKind,
        cancellation_ticket: CancellationTicket,
    ) -> Result<(), AppError> {
        let _account_lease = self.account_leases.try_acquire(account_id)?;
        self.transaction
            .execute(account_id, kind, cancellation_ticket)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{AccountInitializationKind, AccountInitializationService};
    use crate::application::multi_instance::{
        AccountInitializationTransaction, AccountLeaseManager, CancellationTicket,
        LaunchOrchestrator,
    };
    use crate::error::AppError;

    struct FakeTransaction<'a> {
        leases: &'a AccountLeaseManager,
        calls: Mutex<Vec<(String, AccountInitializationKind)>>,
        fail: bool,
    }

    impl AccountInitializationTransaction for FakeTransaction<'_> {
        fn execute(
            &self,
            account_id: &str,
            kind: AccountInitializationKind,
            _cancellation_ticket: CancellationTicket,
        ) -> Result<(), AppError> {
            assert!(self.leases.contains(account_id));
            self.calls
                .lock()
                .unwrap()
                .push((account_id.to_string(), kind));
            if self.fail {
                Err(AppError::Unknown("injected failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn both_initialization_kinds_run_under_the_shared_account_lease() {
        let leases = AccountLeaseManager::default();
        let transaction = FakeTransaction {
            leases: &leases,
            calls: Mutex::new(Vec::new()),
            fail: false,
        };
        let orchestrator = LaunchOrchestrator::default();
        let service = AccountInitializationService::new(&transaction, &leases);

        service
            .execute(
                "account-a",
                AccountInitializationKind::New,
                orchestrator.ticket(),
            )
            .unwrap();
        service
            .execute(
                "account-a",
                AccountInitializationKind::Reinitialize,
                orchestrator.ticket(),
            )
            .unwrap();

        assert_eq!(
            *transaction.calls.lock().unwrap(),
            [
                ("account-a".to_string(), AccountInitializationKind::New),
                (
                    "account-a".to_string(),
                    AccountInitializationKind::Reinitialize
                ),
            ]
        );
        assert!(leases.is_empty());
    }

    #[test]
    fn conflicts_and_transaction_failures_release_or_preserve_the_right_lease() {
        let leases = AccountLeaseManager::default();
        let transaction = FakeTransaction {
            leases: &leases,
            calls: Mutex::new(Vec::new()),
            fail: true,
        };
        let orchestrator = LaunchOrchestrator::default();
        let service = AccountInitializationService::new(&transaction, &leases);
        let blocker = leases.try_acquire("ACCOUNT-A").unwrap();

        assert!(service
            .execute(
                "account-a",
                AccountInitializationKind::New,
                orchestrator.ticket(),
            )
            .is_err());
        assert!(transaction.calls.lock().unwrap().is_empty());
        drop(blocker);

        assert!(service
            .execute(
                "account-a",
                AccountInitializationKind::New,
                orchestrator.ticket(),
            )
            .is_err());
        assert!(leases.is_empty());
    }
}
