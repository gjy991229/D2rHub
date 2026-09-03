use crate::domain::account::{AccountMeta, WindowPositionPreset};
use crate::error::AppError;

use super::{AccountLeaseManager, AccountRepository};

/// Core use cases for an account's named window positions.
///
/// Domain methods own normalization and compatibility mirrors. This service
/// owns operation conflicts, persistence and secret redaction.
pub struct AccountPositionService<'a> {
    accounts: &'a dyn AccountRepository,
    leases: &'a AccountLeaseManager,
}

impl<'a> AccountPositionService<'a> {
    pub fn new(accounts: &'a dyn AccountRepository, leases: &'a AccountLeaseManager) -> Self {
        Self { accounts, leases }
    }

    pub fn set_window_position(
        &self,
        account_id: &str,
        window_x: Option<i32>,
        window_y: Option<i32>,
    ) -> Result<AccountMeta, AppError> {
        let _lease = self.leases.try_acquire(account_id)?;
        let mut account = self.accounts.load(account_id)?;
        account.set_legacy_window_position(window_x, window_y);
        self.accounts.save(&account)?;
        Ok(redact(account))
    }

    pub fn replace_positions(
        &self,
        account_id: &str,
        active_position_id: Option<String>,
        position_presets: Vec<WindowPositionPreset>,
    ) -> Result<AccountMeta, AppError> {
        let _lease = self.leases.try_acquire(account_id)?;
        let mut account = self.accounts.load(account_id)?;
        account
            .replace_position_presets(active_position_id, position_presets)
            .map_err(|error| AppError::ConfigWriteError(error.to_string()))?;
        self.accounts.save(&account)?;
        Ok(redact(account))
    }
}

fn redact(mut account: AccountMeta) -> AccountMeta {
    account.token = None;
    account
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::AccountPositionService;
    use crate::application::multi_instance::{AccountLeaseManager, AccountRepository};
    use crate::domain::account::{AccountMeta, WindowPositionPreset};
    use crate::error::AppError;

    struct FakeRepository {
        account: Mutex<AccountMeta>,
        saves: Mutex<usize>,
    }

    impl FakeRepository {
        fn new(account: AccountMeta) -> Self {
            Self {
                account: Mutex::new(account),
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

    fn account() -> AccountMeta {
        let mut account = AccountMeta::new("acount1");
        account.token = Some("secret".to_string());
        account
    }

    #[test]
    fn legacy_position_update_is_persisted_and_redacted_under_one_lease() {
        let repository = FakeRepository::new(account());
        let leases = AccountLeaseManager::default();
        let service = AccountPositionService::new(&repository, &leases);

        let result = service
            .set_window_position("acount1", Some(120), Some(240))
            .unwrap();

        assert_eq!((result.window_x, result.window_y), (Some(120), Some(240)));
        assert_eq!(result.position_presets.len(), 1);
        assert!(result.token.is_none());
        assert_eq!(*repository.saves.lock().unwrap(), 1);
        assert!(leases.is_empty());
    }

    #[test]
    fn invalid_position_set_never_reaches_persistence() {
        let repository = FakeRepository::new(account());
        let leases = AccountLeaseManager::default();
        let service = AccountPositionService::new(&repository, &leases);

        let error = service
            .replace_positions(
                "acount1",
                Some("missing".to_string()),
                vec![WindowPositionPreset {
                    id: "left".to_string(),
                    name: "Left".to_string(),
                    x: 0,
                    y: 0,
                }],
            )
            .unwrap_err();

        assert!(error.to_string().contains("所选位置不存在"));
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        assert!(leases.is_empty());
    }

    #[test]
    fn a_conflicting_account_operation_blocks_position_mutation() {
        let repository = FakeRepository::new(account());
        let leases = AccountLeaseManager::default();
        let blocker = leases.try_acquire("acount1").unwrap();
        let service = AccountPositionService::new(&repository, &leases);

        assert!(service
            .set_window_position("ACOUNT1", Some(1), Some(2))
            .is_err());
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        drop(blocker);
    }
}
