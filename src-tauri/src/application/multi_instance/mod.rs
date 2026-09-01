mod account_creation;
mod account_mods;
mod account_naming;
mod account_ordering;
mod account_positions;
mod account_profile;
mod account_query;
mod account_settings;
mod facade;
mod instances;
mod launch;
mod leases;
mod ports;

pub use account_creation::{AccountCreationService, CreateAccountRequest, TimestampProvider};
pub use account_mods::AccountModService;
pub use account_naming::AccountNamingService;
pub use account_ordering::AccountOrderingService;
pub use account_positions::AccountPositionService;
pub use account_profile::{
    AccountProfilePatch, AccountProfilePolicy, AccountProfileService, ResolvedAccountProfile,
    TokenProtector,
};
pub use account_query::AccountQueryService;
pub use account_settings::AccountSettingsPreferenceService;
pub use facade::{MultiInstanceFacade, WindowMatch};
pub use instances::{InstanceRegistry, RunningInstance};
pub use launch::{CancellationTicket, LaunchOrchestrator};
pub use leases::{
    AccountCatalogLeaseManager, AccountLeaseManager, AccountOperationLease, AccountOperationLeases,
};
pub use ports::{
    AccountCatalog, AccountCreationRepository, AccountModRepository, AccountNameRepository,
    AccountRepository, AccountRuntimePort, AccountSettingsRepository, GameWindowIdentity,
    GameWindowPort, InstanceStatusPort, WindowPosition,
};

#[derive(Default)]
pub struct MultiInstanceRuntime {
    instances: InstanceRegistry,
    launches: LaunchOrchestrator,
    account_leases: AccountLeaseManager,
    catalog_leases: AccountCatalogLeaseManager,
}

impl MultiInstanceRuntime {
    pub fn instances(&self) -> &InstanceRegistry {
        &self.instances
    }

    pub fn facade(&self) -> MultiInstanceFacade<'_> {
        MultiInstanceFacade::new(&self.instances, &self.launches)
    }

    pub fn account_leases(&self) -> &AccountLeaseManager {
        &self.account_leases
    }

    pub fn catalog_leases(&self) -> &AccountCatalogLeaseManager {
        &self.catalog_leases
    }
}
