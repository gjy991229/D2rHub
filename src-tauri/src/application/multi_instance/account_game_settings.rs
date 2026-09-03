use crate::error::AppError;

use super::{AccountGameSettingsRepository, AccountLeaseManager, GameSettings};

pub struct AccountGameSettingsService<'a> {
    accounts: &'a dyn AccountGameSettingsRepository,
    leases: &'a AccountLeaseManager,
}

impl<'a> AccountGameSettingsService<'a> {
    pub fn new(
        accounts: &'a dyn AccountGameSettingsRepository,
        leases: &'a AccountLeaseManager,
    ) -> Self {
        Self { accounts, leases }
    }

    pub fn get(&self, account_id: &str) -> Result<GameSettings, AppError> {
        // 共享读取允许多个设置面板并发加载，同时与账号目录切换事务互斥。
        let _lease = self.leases.try_acquire_read(account_id)?;
        let account = self.accounts.load(account_id)?;
        if account.has_customized_settings {
            self.accounts.read_account_settings(account_id)
        } else {
            self.accounts.read_system_settings_required(&account)
        }
    }

    pub fn save(&self, account_id: &str, settings: GameSettings) -> Result<(), AppError> {
        ensure_nonempty(&settings, "待保存的 Settings.json")?;
        let _lease = self.leases.try_acquire(account_id)?;
        let account = self.accounts.load(account_id)?;
        self.accounts.save_account_settings(&account, &settings)
    }

    pub fn snapshot_system(&self, account_id: &str) -> Result<GameSettings, AppError> {
        let _lease = self.leases.try_acquire(account_id)?;
        let account = self.accounts.load(account_id)?;
        self.accounts.snapshot_system_settings(&account)
    }

    pub fn get_system_optional(&self, account_id: &str) -> Result<GameSettings, AppError> {
        let _lease = self.leases.try_acquire_read(account_id)?;
        let account = self.accounts.load(account_id)?;
        self.accounts.read_system_settings_optional(&account)
    }
}

fn ensure_nonempty(settings: &GameSettings, source: &str) -> Result<(), AppError> {
    if settings.is_empty() {
        return Err(AppError::ConfigReadError(format!(
            "{source} 为空，无法创建完整的账号画质配置"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::AccountGameSettingsService;
    use crate::application::multi_instance::{
        AccountGameSettingsRepository, AccountLeaseManager, AccountRepository, GameSettings,
    };
    use crate::domain::account::AccountMeta;
    use crate::error::AppError;

    struct FakeRepository {
        account: AccountMeta,
        private: GameSettings,
        system: GameSettings,
        saves: Mutex<Vec<GameSettings>>,
    }

    impl FakeRepository {
        fn new(customized: bool) -> Self {
            let mut account = AccountMeta::new("acount1");
            account.has_customized_settings = customized;
            Self {
                account,
                private: GameSettings::from([("source".to_string(), json!("private"))]),
                system: GameSettings::from([("source".to_string(), json!("system"))]),
                saves: Mutex::new(Vec::new()),
            }
        }
    }

    impl AccountRepository for FakeRepository {
        fn load(&self, account_id: &str) -> Result<AccountMeta, AppError> {
            if self.account.id.eq_ignore_ascii_case(account_id) {
                Ok(self.account.clone())
            } else {
                Err(AppError::AccountNotFound(account_id.to_string()))
            }
        }

        fn save(&self, _account: &AccountMeta) -> Result<(), AppError> {
            unreachable!()
        }
    }

    impl AccountGameSettingsRepository for FakeRepository {
        fn read_account_settings(&self, _account_id: &str) -> Result<GameSettings, AppError> {
            Ok(self.private.clone())
        }

        fn read_system_settings_required(
            &self,
            _account: &AccountMeta,
        ) -> Result<GameSettings, AppError> {
            Ok(self.system.clone())
        }

        fn read_system_settings_optional(
            &self,
            _account: &AccountMeta,
        ) -> Result<GameSettings, AppError> {
            Ok(self.system.clone())
        }

        fn save_account_settings(
            &self,
            _account: &AccountMeta,
            settings: &GameSettings,
        ) -> Result<(), AppError> {
            self.saves.lock().unwrap().push(settings.clone());
            Ok(())
        }

        fn snapshot_system_settings(
            &self,
            _account: &AccountMeta,
        ) -> Result<GameSettings, AppError> {
            self.saves.lock().unwrap().push(self.system.clone());
            Ok(self.system.clone())
        }
    }

    #[test]
    fn get_selects_private_or_system_from_persisted_account_intent() {
        let leases = AccountLeaseManager::default();
        let private = FakeRepository::new(true);
        let system = FakeRepository::new(false);

        assert_eq!(
            AccountGameSettingsService::new(&private, &leases)
                .get("acount1")
                .unwrap()["source"],
            "private"
        );
        assert_eq!(
            AccountGameSettingsService::new(&system, &leases)
                .get("acount1")
                .unwrap()["source"],
            "system"
        );
        assert!(leases.is_empty());
    }

    #[test]
    fn empty_save_fails_before_lock_or_persistence() {
        let repository = FakeRepository::new(false);
        let leases = AccountLeaseManager::default();
        let service = AccountGameSettingsService::new(&repository, &leases);

        assert!(service.save("acount1", GameSettings::new()).is_err());
        assert!(repository.saves.lock().unwrap().is_empty());
        assert!(leases.is_empty());
    }

    #[test]
    fn snapshot_reads_system_then_commits_the_same_complete_map() {
        let repository = FakeRepository::new(false);
        let leases = AccountLeaseManager::default();
        let service = AccountGameSettingsService::new(&repository, &leases);

        let snapshot = service.snapshot_system("acount1").unwrap();

        assert_eq!(snapshot["source"], "system");
        assert_eq!(repository.saves.lock().unwrap().as_slice(), [snapshot]);
        assert!(leases.is_empty());
    }
}
