use crate::domain::account::{validate_account_display_name, AccountMeta};
use crate::error::AppError;

use super::{AccountCatalogLeaseManager, AccountLeaseManager, AccountNameRepository};

pub struct AccountNamingService<'a> {
    accounts: &'a dyn AccountNameRepository,
    catalog_leases: &'a AccountCatalogLeaseManager,
    account_leases: &'a AccountLeaseManager,
}

impl<'a> AccountNamingService<'a> {
    pub fn new(
        accounts: &'a dyn AccountNameRepository,
        catalog_leases: &'a AccountCatalogLeaseManager,
        account_leases: &'a AccountLeaseManager,
    ) -> Self {
        Self {
            accounts,
            catalog_leases,
            account_leases,
        }
    }

    pub fn rename(&self, account_id: &str, new_name: &str) -> Result<AccountMeta, AppError> {
        let new_name = validate_account_display_name(new_name)?;
        let _catalog_lease = self.catalog_leases.acquire();
        let _account_lease = self.account_leases.try_acquire(account_id)?;
        self.accounts
            .ensure_display_name_available(&new_name, Some(account_id))?;
        let mut account = self.accounts.load(account_id)?;
        account.display_name = new_name;
        self.accounts.save(&account)?;
        account.token = None;
        Ok(account)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::AccountNamingService;
    use crate::application::multi_instance::{
        AccountCatalogLeaseManager, AccountLeaseManager, AccountNameRepository, AccountRepository,
    };
    use crate::domain::account::{normalize_account_display_name, AccountMeta};
    use crate::error::AppError;

    struct FakeRepository {
        account: Mutex<AccountMeta>,
        occupied_name: Option<String>,
        saves: Mutex<usize>,
    }

    impl FakeRepository {
        fn new(occupied_name: Option<&str>) -> Self {
            let mut account = AccountMeta::new("acount1");
            account.display_name = "Old".to_string();
            account.token = Some("secret".to_string());
            Self {
                account: Mutex::new(account),
                occupied_name: occupied_name.map(normalize_account_display_name),
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

    impl AccountNameRepository for FakeRepository {
        fn ensure_display_name_available(
            &self,
            requested_name: &str,
            _excluded_account_id: Option<&str>,
        ) -> Result<(), AppError> {
            if self.occupied_name.as_deref()
                == Some(normalize_account_display_name(requested_name).as_str())
            {
                Err(AppError::AccountAlreadyExists(requested_name.to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn rename_trims_persists_and_redacts() {
        let repository = FakeRepository::new(None);
        let catalog_leases = AccountCatalogLeaseManager::default();
        let account_leases = AccountLeaseManager::default();
        let service = AccountNamingService::new(&repository, &catalog_leases, &account_leases);

        let result = service.rename("acount1", "  New Name  ").unwrap();

        assert_eq!(result.display_name, "New Name");
        assert!(result.token.is_none());
        assert_eq!(*repository.saves.lock().unwrap(), 1);
        assert!(account_leases.is_empty());
    }

    #[test]
    fn empty_duplicate_and_conflicting_renames_never_save() {
        let repository = FakeRepository::new(Some("Existing"));
        let catalog_leases = AccountCatalogLeaseManager::default();
        let account_leases = AccountLeaseManager::default();
        let service = AccountNamingService::new(&repository, &catalog_leases, &account_leases);

        assert!(service.rename("acount1", " ").is_err());
        assert!(service.rename("acount1", "existing").is_err());
        let blocker = account_leases.try_acquire("acount1").unwrap();
        assert!(service.rename("ACOUNT1", "Available").is_err());
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        drop(blocker);
    }
}
