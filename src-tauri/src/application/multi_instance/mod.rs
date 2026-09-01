mod account_ordering;
mod account_positions;
mod account_query;
mod facade;
mod instances;
mod launch;
mod leases;
mod ports;

pub use account_ordering::AccountOrderingService;
pub use account_positions::AccountPositionService;
pub use account_query::AccountQueryService;
pub use facade::{MultiInstanceFacade, WindowMatch};
pub use instances::{InstanceRegistry, RunningInstance};
pub use launch::{CancellationTicket, LaunchOrchestrator};
pub use leases::{AccountLeaseManager, AccountOperationLease, AccountOperationLeases};
pub use ports::{
    AccountCatalog, AccountRepository, AccountRuntimePort, GameWindowIdentity, GameWindowPort,
    InstanceStatusPort, WindowPosition,
};

#[derive(Default)]
pub struct MultiInstanceRuntime {
    instances: InstanceRegistry,
    launches: LaunchOrchestrator,
    account_leases: AccountLeaseManager,
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
}
