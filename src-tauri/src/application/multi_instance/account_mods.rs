use crate::domain::account::AccountMeta;
use crate::error::AppError;

use super::{AccountLeaseManager, AccountModRepository};

pub struct AccountModService<'a> {
    accounts: &'a dyn AccountModRepository,
    leases: &'a AccountLeaseManager,
}

impl<'a> AccountModService<'a> {
    pub fn new(accounts: &'a dyn AccountModRepository, leases: &'a AccountLeaseManager) -> Self {
        Self { accounts, leases }
    }

    pub fn add(&self, account_id: &str, configuration: &str) -> Result<bool, AppError> {
        let _lease = self.leases.try_acquire(account_id)?;
        let mut account = self.accounts.load(account_id)?;
        if !account.add_mod_configuration(configuration) {
            return Ok(false);
        }
        self.accounts.save_mod_configuration(account)?;
        Ok(true)
    }

    pub fn replace(
        &self,
        account_id: &str,
        active_mod: String,
        mod_list: Vec<String>,
    ) -> Result<AccountMeta, AppError> {
        let _lease = self.leases.try_acquire(account_id)?;
        let mut account = self.accounts.load(account_id)?;
        account.replace_mod_configurations(active_mod, mod_list);
        let mut account = self.accounts.save_mod_configuration(account)?;
        account.token = None;
        Ok(account)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::AccountModService;
    use crate::application::multi_instance::{
        AccountLeaseManager, AccountModRepository, AccountRepository,
    };
    use crate::domain::account::AccountMeta;
    use crate::error::AppError;

    struct FakeRepository {
        account: Mutex<AccountMeta>,
        saves: Mutex<usize>,
    }

    impl FakeRepository {
        fn new() -> Self {
            let mut account = AccountMeta::new("acount1");
            account.token = Some("secret".to_string());
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
            *self.account.lock().unwrap() = account.clone();
            Ok(())
        }
    }

    impl AccountModRepository for FakeRepository {
        fn save_mod_configuration(&self, account: AccountMeta) -> Result<AccountMeta, AppError> {
            *self.saves.lock().unwrap() += 1;
            *self.account.lock().unwrap() = account.clone();
            Ok(account)
        }
    }

    #[test]
    fn duplicate_add_is_a_noop_and_new_add_is_persisted_once() {
        let repository = FakeRepository::new();
        repository.account.lock().unwrap().mod_list = vec!["-mod one".to_string()];
        let leases = AccountLeaseManager::default();
        let service = AccountModService::new(&repository, &leases);

        assert!(!service.add("acount1", " -mod one ").unwrap());
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        assert!(service.add("acount1", " -mod two ").unwrap());
        assert_eq!(*repository.saves.lock().unwrap(), 1);
        assert_eq!(repository.account.lock().unwrap().mod_args, "-mod two");
        assert!(leases.is_empty());
    }

    #[test]
    fn replacement_normalizes_and_redacts_the_response() {
        let repository = FakeRepository::new();
        let leases = AccountLeaseManager::default();
        let service = AccountModService::new(&repository, &leases);

        let result = service
            .replace(
                "acount1",
                " -mod one ".to_string(),
                vec!["-mod one".to_string(), " -mod one ".to_string()],
            )
            .unwrap();

        assert_eq!(result.mod_args, "-mod one");
        assert_eq!(result.mod_list, ["-mod one"]);
        assert!(result.token.is_none());
        assert_eq!(*repository.saves.lock().unwrap(), 1);
    }

    #[test]
    fn account_conflict_prevents_reads_and_writes() {
        let repository = FakeRepository::new();
        let leases = AccountLeaseManager::default();
        let blocker = leases.try_acquire("acount1").unwrap();
        let service = AccountModService::new(&repository, &leases);

        assert!(service.add("ACOUNT1", "-mod one").is_err());
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        drop(blocker);
    }
}
