use crate::error::AppError;

use super::{AccountLeaseManager, AccountSettingsPreferenceRepository};

/// Controls whether an account uses its private game-settings snapshot.
///
/// Enabling fails closed unless the adapter confirms that a real, non-empty
/// snapshot exists. Disabling does not delete the snapshot, preserving the
/// user's ability to turn it back on later.
pub struct AccountSettingsPreferenceService<'a> {
    accounts: &'a dyn AccountSettingsPreferenceRepository,
    leases: &'a AccountLeaseManager,
}

impl<'a> AccountSettingsPreferenceService<'a> {
    pub fn new(
        accounts: &'a dyn AccountSettingsPreferenceRepository,
        leases: &'a AccountLeaseManager,
    ) -> Self {
        Self { accounts, leases }
    }

    pub fn set_customized(&self, account_id: &str, customized: bool) -> Result<(), AppError> {
        let _lease = self.leases.try_acquire(account_id)?;
        let mut account = self.accounts.load(account_id)?;
        if customized {
            self.accounts.ensure_complete_snapshot(account_id)?;
        }
        account.has_customized_settings = customized;
        self.accounts.save(&account)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::AccountSettingsPreferenceService;
    use crate::application::multi_instance::{
        AccountLeaseManager, AccountRepository, AccountSettingsPreferenceRepository,
    };
    use crate::domain::account::AccountMeta;
    use crate::error::AppError;

    struct FakeRepository {
        account: Mutex<AccountMeta>,
        snapshot_result: Result<(), AppError>,
        snapshot_checks: Mutex<usize>,
        saves: Mutex<usize>,
    }

    impl FakeRepository {
        fn new(snapshot_result: Result<(), AppError>) -> Self {
            Self {
                account: Mutex::new(AccountMeta::new("acount1")),
                snapshot_result,
                snapshot_checks: Mutex::new(0),
                saves: Mutex::new(0),
            }
        }
    }

    impl AccountRepository for FakeRepository {
        fn load(&self, account_id: &str) -> Result<AccountMeta, AppError> {
            let account = self.account.lock().unwrap();
            if account.id.eq_ignore_ascii_case(account_id) {
                Ok(account.clone())
            } else {
                Err(AppError::AccountNotFound(account_id.to_string()))
            }
        }

        fn save(&self, account: &AccountMeta) -> Result<(), AppError> {
            *self.saves.lock().unwrap() += 1;
            *self.account.lock().unwrap() = account.clone();
            Ok(())
        }
    }

    impl AccountSettingsPreferenceRepository for FakeRepository {
        fn ensure_complete_snapshot(&self, _account_id: &str) -> Result<(), AppError> {
            *self.snapshot_checks.lock().unwrap() += 1;
            self.snapshot_result.clone()
        }
    }

    #[test]
    fn enabling_requires_a_complete_snapshot_before_saving() {
        let repository = FakeRepository::new(Err(AppError::ConfigReadError(
            "snapshot is empty".to_string(),
        )));
        let leases = AccountLeaseManager::default();
        let service = AccountSettingsPreferenceService::new(&repository, &leases);

        assert!(service.set_customized("acount1", true).is_err());
        assert_eq!(*repository.snapshot_checks.lock().unwrap(), 1);
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        assert!(!repository.account.lock().unwrap().has_customized_settings);
        assert!(leases.is_empty());
    }

    #[test]
    fn disabling_preserves_the_snapshot_and_only_changes_metadata() {
        let repository = FakeRepository::new(Ok(()));
        repository.account.lock().unwrap().has_customized_settings = true;
        let leases = AccountLeaseManager::default();
        let service = AccountSettingsPreferenceService::new(&repository, &leases);

        service.set_customized("acount1", false).unwrap();

        assert_eq!(*repository.snapshot_checks.lock().unwrap(), 0);
        assert_eq!(*repository.saves.lock().unwrap(), 1);
        assert!(!repository.account.lock().unwrap().has_customized_settings);
        assert!(leases.is_empty());
    }

    #[test]
    fn a_conflicting_operation_prevents_snapshot_checks_and_writes() {
        let repository = FakeRepository::new(Ok(()));
        let leases = AccountLeaseManager::default();
        let blocker = leases.try_acquire("acount1").unwrap();
        let service = AccountSettingsPreferenceService::new(&repository, &leases);

        assert!(service.set_customized("ACOUNT1", true).is_err());
        assert_eq!(*repository.snapshot_checks.lock().unwrap(), 0);
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        drop(blocker);
    }
}
