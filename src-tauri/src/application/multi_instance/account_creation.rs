use crate::domain::account::{validate_account_display_name, AccountMeta, AuthMode};
use crate::error::AppError;

use super::{
    AccountCatalogLeaseManager, AccountCreationRepository, AccountLeaseManager,
    AccountProfilePolicy, TokenProtector,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateAccountRequest {
    pub display_name: String,
    pub auth_mode: Option<String>,
    pub token: Option<String>,
    pub region: Option<String>,
    pub language: Option<String>,
    pub voice_language: Option<String>,
}

pub trait TimestampProvider: Send + Sync {
    fn now_rfc3339(&self) -> String;
}

pub struct AccountCreationService<'a> {
    accounts: &'a dyn AccountCreationRepository,
    catalog_leases: &'a AccountCatalogLeaseManager,
    account_leases: &'a AccountLeaseManager,
    policy: &'a dyn AccountProfilePolicy,
    tokens: &'a dyn TokenProtector,
    clock: &'a dyn TimestampProvider,
}

impl<'a> AccountCreationService<'a> {
    pub fn new(
        accounts: &'a dyn AccountCreationRepository,
        catalog_leases: &'a AccountCatalogLeaseManager,
        account_leases: &'a AccountLeaseManager,
        policy: &'a dyn AccountProfilePolicy,
        tokens: &'a dyn TokenProtector,
        clock: &'a dyn TimestampProvider,
    ) -> Self {
        Self {
            accounts,
            catalog_leases,
            account_leases,
            policy,
            tokens,
            clock,
        }
    }

    pub fn create(&self, request: CreateAccountRequest) -> Result<String, AppError> {
        let display_name = validate_account_display_name(&request.display_name)?;
        let _catalog_lease = self.catalog_leases.acquire();
        self.accounts
            .ensure_display_name_available(&display_name, None)?;
        let account_id = self.accounts.next_account_id();
        let _account_lease = self.account_leases.try_acquire(&account_id)?;

        let mut account = AccountMeta::new(&account_id);
        account.display_name = display_name;
        account.auth_mode = request.auth_mode;
        account.region = request.region;
        let resolved = self.policy.resolve(&account)?;
        account.auth_mode = Some(resolved.auth_mode.canonical().to_string());
        account.region = Some(resolved.game_region.canonical().to_string());

        match resolved.auth_mode {
            AuthMode::Token => {
                let plaintext = request
                    .token
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        AppError::ConfigReadError("Token 认证账号必须提供 Token".to_string())
                    })?;
                account.language = request
                    .language
                    .or_else(|| Some(resolved.default_locale.to_string()));
                account.voicelanguage = request
                    .voice_language
                    .or_else(|| Some(resolved.default_locale.to_string()));
                account.initialized = true;
                account.token = Some(self.tokens.protect(plaintext)?);
            }
            AuthMode::BattleNet => {
                if request
                    .token
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    return Err(AppError::ConfigReadError(
                        "Battle.net 认证账号不能保存 Token".to_string(),
                    ));
                }
                account.token = None;
            }
        }
        account.last_reset_at = Some(self.clock.now_rfc3339());

        self.accounts.create(&account)?;
        Ok(account_id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{AccountCreationService, CreateAccountRequest, TimestampProvider};
    use crate::application::multi_instance::{
        AccountCatalogLeaseManager, AccountCreationRepository, AccountLeaseManager,
        AccountNameRepository, AccountProfilePolicy, AccountRepository, ResolvedAccountProfile,
        TokenProtector,
    };
    use crate::domain::account::{
        normalize_account_display_name, AccountMeta, AuthMode, GameRegion,
    };
    use crate::error::AppError;

    struct FakeRepository {
        created: Mutex<Vec<AccountMeta>>,
        occupied: Option<String>,
    }

    impl FakeRepository {
        fn new(occupied: Option<&str>) -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                occupied: occupied.map(normalize_account_display_name),
            }
        }
    }

    impl AccountRepository for FakeRepository {
        fn load(&self, account_id: &str) -> Result<AccountMeta, AppError> {
            self.created
                .lock()
                .unwrap()
                .iter()
                .find(|account| account.id.eq_ignore_ascii_case(account_id))
                .cloned()
                .ok_or_else(|| AppError::AccountNotFound(account_id.to_string()))
        }

        fn save(&self, _account: &AccountMeta) -> Result<(), AppError> {
            unreachable!("creation tests never update an existing account")
        }
    }

    impl AccountNameRepository for FakeRepository {
        fn ensure_display_name_available(
            &self,
            requested_name: &str,
            _excluded_account_id: Option<&str>,
        ) -> Result<(), AppError> {
            if self.occupied.as_deref()
                == Some(normalize_account_display_name(requested_name).as_str())
            {
                Err(AppError::AccountAlreadyExists(requested_name.to_string()))
            } else {
                Ok(())
            }
        }
    }

    impl AccountCreationRepository for FakeRepository {
        fn next_account_id(&self) -> String {
            "550e8400-e29b-41d4-a716-446655440000".to_string()
        }

        fn create(&self, account: &AccountMeta) -> Result<(), AppError> {
            self.created.lock().unwrap().push(account.clone());
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

    struct FakeClock;

    impl TimestampProvider for FakeClock {
        fn now_rfc3339(&self) -> String {
            "2026-09-01T00:00:00Z".to_string()
        }
    }

    fn service<'a>(
        repository: &'a FakeRepository,
        catalog_leases: &'a AccountCatalogLeaseManager,
        account_leases: &'a AccountLeaseManager,
    ) -> AccountCreationService<'a> {
        AccountCreationService::new(
            repository,
            catalog_leases,
            account_leases,
            &FakePolicy,
            &FakeTokens,
            &FakeClock,
        )
    }

    #[test]
    fn token_account_is_canonical_protected_initialized_and_timestamped() {
        let repository = FakeRepository::new(None);
        let catalog_leases = AccountCatalogLeaseManager::default();
        let account_leases = AccountLeaseManager::default();

        let id = service(&repository, &catalog_leases, &account_leases)
            .create(CreateAccountRequest {
                display_name: "  Primary  ".to_string(),
                auth_mode: Some("token".to_string()),
                token: Some("credential".to_string()),
                region: Some("US".to_string()),
                ..CreateAccountRequest::default()
            })
            .unwrap();

        let created = repository.created.lock().unwrap();
        let account = &created[0];
        assert_eq!(id, account.id);
        assert_eq!(account.display_name, "Primary");
        assert_eq!(account.region.as_deref(), Some("NA"));
        assert_eq!(account.token.as_deref(), Some("protected:credential"));
        assert_eq!(account.language.as_deref(), Some("enUS"));
        assert!(account.initialized);
        assert_eq!(
            account.last_reset_at.as_deref(),
            Some("2026-09-01T00:00:00Z")
        );
        assert!(account_leases.is_empty());
    }

    #[test]
    fn invalid_duplicate_and_credential_mismatches_create_nothing() {
        let repository = FakeRepository::new(Some("Existing"));
        let catalog_leases = AccountCatalogLeaseManager::default();
        let account_leases = AccountLeaseManager::default();
        let service = service(&repository, &catalog_leases, &account_leases);

        assert!(service
            .create(CreateAccountRequest {
                display_name: "existing".to_string(),
                auth_mode: Some("token".to_string()),
                token: Some("credential".to_string()),
                region: Some("CN".to_string()),
                ..CreateAccountRequest::default()
            })
            .is_err());
        assert!(service
            .create(CreateAccountRequest {
                display_name: "Token Missing".to_string(),
                auth_mode: Some("token".to_string()),
                region: Some("CN".to_string()),
                ..CreateAccountRequest::default()
            })
            .is_err());
        assert!(service
            .create(CreateAccountRequest {
                display_name: "Unexpected Token".to_string(),
                auth_mode: Some("bnet".to_string()),
                token: Some("credential".to_string()),
                region: Some("CN".to_string()),
                ..CreateAccountRequest::default()
            })
            .is_err());
        assert!(repository.created.lock().unwrap().is_empty());
        assert!(account_leases.is_empty());
    }

    #[test]
    fn battle_net_account_starts_uninitialized_without_a_token() {
        let repository = FakeRepository::new(None);
        let catalog_leases = AccountCatalogLeaseManager::default();
        let account_leases = AccountLeaseManager::default();

        service(&repository, &catalog_leases, &account_leases)
            .create(CreateAccountRequest {
                display_name: "Battle Net".to_string(),
                auth_mode: Some("bnet".to_string()),
                region: Some("CN".to_string()),
                ..CreateAccountRequest::default()
            })
            .unwrap();

        let created = repository.created.lock().unwrap();
        assert!(!created[0].initialized);
        assert!(created[0].token.is_none());
    }
}
