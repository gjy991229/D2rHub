mod account_query;
mod facade;
mod instances;
mod launch;
mod ports;

pub use account_query::AccountQueryService;
pub use facade::{MultiInstanceFacade, WindowMatch};
pub use instances::{InstanceRegistry, RunningInstance};
pub use launch::{CancellationTicket, LaunchOrchestrator};
pub use ports::{
    AccountCatalog, AccountRuntimePort, GameWindowIdentity, GameWindowPort, InstanceStatusPort,
    WindowPosition,
};

#[derive(Default)]
pub struct MultiInstanceRuntime {
    instances: InstanceRegistry,
    launches: LaunchOrchestrator,
}

impl MultiInstanceRuntime {
    pub fn instances(&self) -> &InstanceRegistry {
        &self.instances
    }

    pub fn facade(&self) -> MultiInstanceFacade<'_> {
        MultiInstanceFacade::new(&self.instances, &self.launches)
    }
}
