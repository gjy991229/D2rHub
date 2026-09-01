use std::path::PathBuf;

use crate::domain::account::AccountMeta;
use crate::domain::config::GlobalConfig;
use crate::error::AppError;

use super::instances::RunningInstance;

pub trait AccountCatalog: Send + Sync {
    fn list_account_ids(&self) -> Result<Vec<String>, AppError>;

    fn list(&self) -> Result<Vec<AccountMeta>, AppError> {
        self.list_account_ids()?
            .into_iter()
            .map(|account_id| self.get(&account_id))
            .collect()
    }

    fn get(&self, account_id: &str) -> Result<AccountMeta, AppError> {
        Err(AppError::AccountNotFound(account_id.to_string()))
    }
}

/// Live runtime facts needed by account queries.
///
/// The port deliberately owns process verification as well as registry access: the application
/// service must not depend on `sysinfo`, Tauri state, or the concrete in-memory registry.
pub trait AccountRuntimePort: Send + Sync {
    fn registered_pid(&self, account_id: &str) -> Option<u32>;

    fn is_expected_game_process(
        &self,
        config: &GlobalConfig,
        account: &AccountMeta,
        pid: u32,
    ) -> bool;

    fn remove_if_pid(&self, account_id: &str, pid: u32) -> bool;
}

/// Read-only view of live multi-instance state exposed to capability modules.
/// Registry mutation remains an adapter concern owned by core launch/recovery use cases.
pub trait InstanceStatusPort: Send + Sync {
    fn find(&self, account_id: &str) -> Option<RunningInstance>;
    fn list(&self) -> Vec<RunningInstance>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameWindowIdentity {
    pub title: String,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
}

pub trait GameWindowPort: Send + Sync {
    fn find_unique_process(&self, identity: &GameWindowIdentity) -> Option<u32>;
    fn rename(&self, pid: u32, title: &str);
    fn move_to(&self, pid: u32, position: WindowPosition) -> bool;
    fn move_by_title_compat(&self, title: &str, position: WindowPosition) -> bool;
    fn position(&self, pid: u32) -> Option<WindowPosition>;
    fn set_taskbar_identity(&self, pid: u32, app_id: &str) -> Result<(), String>;
    fn focus_by_pid(&self, pid: u32) -> bool;
    fn focus_by_title_compat(&self, title: &str) -> bool;
}
