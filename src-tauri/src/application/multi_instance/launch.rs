use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::account::is_valid_account_id;
use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancellationTicket(u64);

#[derive(Default)]
pub struct LaunchOrchestrator {
    cancellation_generation: AtomicU64,
}

impl LaunchOrchestrator {
    pub fn validate_account_ids(account_ids: &[String]) -> Result<(), AppError> {
        let mut canonical_ids = account_ids
            .iter()
            .map(|account_id| account_id.to_ascii_lowercase())
            .collect::<Vec<_>>();
        canonical_ids.sort();
        canonical_ids.dedup();
        if canonical_ids.len() != account_ids.len() {
            return Err(AppError::ConfigReadError(
                "启动列表包含重复账号，已拒绝执行".to_string(),
            ));
        }
        for account_id in account_ids {
            if !is_valid_account_id(account_id) {
                return Err(AppError::FileError(format!("账号 ID 非法: {account_id}")));
            }
        }
        Ok(())
    }

    pub fn ticket(&self) -> CancellationTicket {
        CancellationTicket(self.cancellation_generation.load(Ordering::SeqCst))
    }

    pub fn cancel_current_operation(&self) {
        self.cancellation_generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self, ticket: CancellationTicket) -> bool {
        self.cancellation_generation.load(Ordering::SeqCst) != ticket.0
    }
}

#[cfg(test)]
mod tests {
    use super::LaunchOrchestrator;

    #[test]
    fn cancellation_invalidates_only_tickets_that_already_exist() {
        let orchestrator = LaunchOrchestrator::default();
        let old = orchestrator.ticket();

        orchestrator.cancel_current_operation();
        let new = orchestrator.ticket();

        assert!(orchestrator.is_cancelled(old));
        assert!(!orchestrator.is_cancelled(new));
    }

    #[test]
    fn repeated_cancellation_remains_monotonic() {
        let orchestrator = LaunchOrchestrator::default();
        let before_first = orchestrator.ticket();
        orchestrator.cancel_current_operation();
        let before_second = orchestrator.ticket();
        orchestrator.cancel_current_operation();
        let after_second = orchestrator.ticket();

        assert!(orchestrator.is_cancelled(before_first));
        assert!(orchestrator.is_cancelled(before_second));
        assert!(!orchestrator.is_cancelled(after_second));
    }

    #[test]
    fn launch_validation_rejects_case_aliases_before_side_effects() {
        let account = "550e8400-e29b-41d4-a716-446655440000";
        let aliases = vec![account.to_string(), account.to_ascii_uppercase()];

        assert!(LaunchOrchestrator::validate_account_ids(&aliases).is_err());
    }

    #[test]
    fn launch_validation_keeps_historical_account_ids_compatible() {
        LaunchOrchestrator::validate_account_ids(&["acount1".to_string()]).unwrap();
    }
}
