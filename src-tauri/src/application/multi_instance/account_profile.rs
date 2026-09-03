use crate::domain::account::{AccountMeta, AuthMode, ClientEdition, GameRegion};
use crate::error::AppError;

use super::{AccountLeaseManager, AccountRepository};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountProfilePatch {
    pub auth_mode: Option<String>,
    pub token: Option<String>,
    pub region: Option<String>,
    pub language: Option<String>,
    pub voice_language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAccountProfile {
    pub auth_mode: AuthMode,
    pub game_region: GameRegion,
    pub client_edition: ClientEdition,
    pub default_locale: &'static str,
}

/// Validates a candidate account against configured client installations.
pub trait AccountProfilePolicy: Send + Sync {
    fn resolve(&self, account: &AccountMeta) -> Result<ResolvedAccountProfile, AppError>;
}

/// Protects a plaintext credential for current-user persistence.
pub trait TokenProtector: Send + Sync {
    fn protect(&self, plaintext: &str) -> Result<String, AppError>;
}

pub struct AccountProfileService<'a> {
    accounts: &'a dyn AccountRepository,
    leases: &'a AccountLeaseManager,
    policy: &'a dyn AccountProfilePolicy,
    tokens: &'a dyn TokenProtector,
}

impl<'a> AccountProfileService<'a> {
    pub fn new(
        accounts: &'a dyn AccountRepository,
        leases: &'a AccountLeaseManager,
        policy: &'a dyn AccountProfilePolicy,
        tokens: &'a dyn TokenProtector,
    ) -> Self {
        Self {
            accounts,
            leases,
            policy,
            tokens,
        }
    }

    pub fn update(&self, account_id: &str, patch: AccountProfilePatch) -> Result<(), AppError> {
        let _lease = self.leases.try_acquire(account_id)?;
        let mut account = self.accounts.load(account_id)?;
        let previous_auth_mode = AuthMode::parse(account.auth_mode.as_deref())?;
        let old_edition = account
            .region
            .as_deref()
            .and_then(|value| GameRegion::parse(value).ok())
            .map(GameRegion::edition);
        let region_updated = patch.region.is_some();
        let auth_mode_updated = patch.auth_mode.is_some();

        if let Some(value) = patch.auth_mode {
            account.auth_mode = Some(value);
        }
        if let Some(value) = patch.region {
            account.region = Some(value);
        }

        // Complete policy validation happens before token protection or save.
        let resolved = self.policy.resolve(&account)?;
        account.auth_mode = Some(resolved.auth_mode.canonical().to_string());
        account.region = Some(resolved.game_region.canonical().to_string());

        if resolved.auth_mode == AuthMode::Token
            && account.token.is_none()
            && patch
                .token
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(AppError::ConfigReadError(
                "迁移为 Token 认证时必须提供 Token".to_string(),
            ));
        }

        if region_updated {
            let edition_changed = old_edition
                .map(|edition| edition != resolved.client_edition)
                .unwrap_or(true);
            if edition_changed {
                account.has_customized_settings = false;
                account.snapshot_edition = None;
                if resolved.auth_mode == AuthMode::BattleNet {
                    account.initialized = false;
                }
            }
            if resolved.auth_mode == AuthMode::Token {
                if account.language.is_none() {
                    account.language = Some(resolved.default_locale.to_string());
                }
                if account.voicelanguage.is_none() {
                    account.voicelanguage = Some(resolved.default_locale.to_string());
                }
            }
        }

        if resolved.auth_mode == AuthMode::Token {
            account.initialized = true;
            if auth_mode_updated && previous_auth_mode != AuthMode::Token {
                account.snapshot_edition = None;
            }
            if let Some(value) = patch.language {
                account.language = Some(value);
            }
            if let Some(value) = patch.voice_language {
                account.voicelanguage = Some(value);
            }
        }
        if let Some(plaintext) = patch.token {
            account.token = Some(self.tokens.protect(&plaintext)?);
        }

        self.accounts.save(&account)
    }

    pub fn switch_international_region(
        &self,
        account_id: &str,
        requested_region: &str,
    ) -> Result<(), AppError> {
        let _lease = self.leases.try_acquire(account_id)?;
        let mut account = self.accounts.load(account_id)?;
        account.switch_international_region(requested_region)?;
        self.accounts.save(&account)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{
        AccountProfilePatch, AccountProfilePolicy, AccountProfileService, ResolvedAccountProfile,
        TokenProtector,
    };
    use crate::application::multi_instance::{AccountLeaseManager, AccountRepository};
    use crate::domain::account::{AccountMeta, AuthMode, ClientEdition, GameRegion};
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

    struct FakePolicy;

    impl AccountProfilePolicy for FakePolicy {
        fn resolve(&self, account: &AccountMeta) -> Result<ResolvedAccountProfile, AppError> {
            let auth_mode = AuthMode::parse(account.auth_mode.as_deref())?;
            let game_region = GameRegion::parse(
                account
                    .region
                    .as_deref()
                    .ok_or_else(|| AppError::ConfigReadError("missing region".to_string()))?,
            )?;
            if auth_mode == AuthMode::BattleNet && game_region.edition() == ClientEdition::Global {
                return Err(AppError::ConfigReadError(
                    "global Battle.net is unsupported".to_string(),
                ));
            }
            Ok(ResolvedAccountProfile {
                auth_mode,
                game_region,
                client_edition: game_region.edition(),
                default_locale: game_region.default_locale(),
            })
        }
    }

    struct FakeTokens;

    impl TokenProtector for FakeTokens {
        fn protect(&self, plaintext: &str) -> Result<String, AppError> {
            Ok(format!("protected:{plaintext}"))
        }
    }

    fn service<'a>(
        repository: &'a FakeRepository,
        leases: &'a AccountLeaseManager,
    ) -> AccountProfileService<'a> {
        AccountProfileService::new(repository, leases, &FakePolicy, &FakeTokens)
    }

    #[test]
    fn token_migration_requires_a_credential_before_save() {
        let mut account = AccountMeta::new("acount1");
        account.auth_mode = Some("bnet".to_string());
        account.region = Some("CN".to_string());
        let repository = FakeRepository::new(account);
        let leases = AccountLeaseManager::default();

        let error = service(&repository, &leases)
            .update(
                "acount1",
                AccountProfilePatch {
                    auth_mode: Some("token".to_string()),
                    ..AccountProfilePatch::default()
                },
            )
            .unwrap_err();

        assert!(error.to_string().contains("必须提供 Token"));
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        assert!(leases.is_empty());
    }

    #[test]
    fn changing_edition_resets_edition_owned_state_and_protects_token() {
        let mut account = AccountMeta::new("acount1");
        account.auth_mode = Some("bnet".to_string());
        account.region = Some("CN".to_string());
        account.initialized = true;
        account.has_customized_settings = true;
        account.snapshot_edition = Some("CN".to_string());
        let repository = FakeRepository::new(account);
        let leases = AccountLeaseManager::default();

        service(&repository, &leases)
            .update(
                "acount1",
                AccountProfilePatch {
                    auth_mode: Some("token".to_string()),
                    token: Some("credential".to_string()),
                    region: Some("EU".to_string()),
                    ..AccountProfilePatch::default()
                },
            )
            .unwrap();

        let account = repository.account.lock().unwrap();
        assert_eq!(account.auth_mode.as_deref(), Some("token"));
        assert_eq!(account.region.as_deref(), Some("EU"));
        assert_eq!(account.token.as_deref(), Some("protected:credential"));
        assert!(account.initialized);
        assert!(!account.has_customized_settings);
        assert!(account.snapshot_edition.is_none());
        assert_eq!(account.language.as_deref(), Some("enUS"));
        assert_eq!(account.voicelanguage.as_deref(), Some("enUS"));
    }

    #[test]
    fn same_edition_region_switch_preserves_account_state() {
        let mut account = AccountMeta::new("acount1");
        account.auth_mode = Some("token".to_string());
        account.region = Some("KR".to_string());
        account.token = Some("protected".to_string());
        account.initialized = true;
        account.has_customized_settings = true;
        account.snapshot_edition = Some("Global".to_string());
        let repository = FakeRepository::new(account);
        let leases = AccountLeaseManager::default();

        service(&repository, &leases)
            .switch_international_region("acount1", "NA")
            .unwrap();

        let account = repository.account.lock().unwrap();
        assert_eq!(account.region.as_deref(), Some("NA"));
        assert_eq!(account.token.as_deref(), Some("protected"));
        assert!(account.initialized);
        assert!(account.has_customized_settings);
        assert_eq!(account.snapshot_edition.as_deref(), Some("Global"));
    }

    #[test]
    fn policy_failure_and_account_conflict_never_save() {
        let mut account = AccountMeta::new("acount1");
        account.auth_mode = Some("bnet".to_string());
        account.region = Some("CN".to_string());
        let repository = FakeRepository::new(account);
        let leases = AccountLeaseManager::default();

        assert!(service(&repository, &leases)
            .update(
                "acount1",
                AccountProfilePatch {
                    region: Some("EU".to_string()),
                    ..AccountProfilePatch::default()
                },
            )
            .is_err());
        let blocker = leases.try_acquire("acount1").unwrap();
        assert!(service(&repository, &leases)
            .switch_international_region("ACOUNT1", "EU")
            .is_err());
        assert_eq!(*repository.saves.lock().unwrap(), 0);
        drop(blocker);
    }
}
