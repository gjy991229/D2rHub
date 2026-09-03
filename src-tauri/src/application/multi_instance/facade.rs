use super::{
    CancellationTicket, GameWindowPort, InstanceStatusPort, LaunchOrchestrator, RunningInstance,
    WindowPosition,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMatch {
    ProcessId,
    CompatibilityTitle,
}

pub struct MultiInstanceFacade<'a> {
    instances: &'a dyn InstanceStatusPort,
    launches: &'a LaunchOrchestrator,
}

impl<'a> MultiInstanceFacade<'a> {
    pub(super) fn new(
        instances: &'a dyn InstanceStatusPort,
        launches: &'a LaunchOrchestrator,
    ) -> Self {
        Self {
            instances,
            launches,
        }
    }

    pub fn instance(&self, account_id: &str) -> Option<RunningInstance> {
        self.instances.find(account_id)
    }

    pub fn running_instances(&self) -> Vec<RunningInstance> {
        self.instances.list()
    }

    pub fn cancellation_ticket(&self) -> CancellationTicket {
        self.launches.ticket()
    }

    pub fn cancel_current_operation(&self) {
        self.launches.cancel_current_operation();
    }

    pub fn cancel(&self, ticket: CancellationTicket) {
        self.launches.cancel(ticket);
    }

    pub fn complete(&self, ticket: CancellationTicket) {
        self.launches.complete(ticket);
    }

    pub fn is_cancelled(&self, ticket: CancellationTicket) -> bool {
        self.launches.is_cancelled(ticket)
    }

    pub fn focus_account_window<W: GameWindowPort + ?Sized>(
        &self,
        windows: &W,
        account_id: &str,
        title: &str,
    ) -> Option<WindowMatch> {
        if let Some(pid) = self.instances.find(account_id).map(|instance| instance.pid) {
            if windows.focus_by_pid(pid) {
                return Some(WindowMatch::ProcessId);
            }
        }
        windows
            .focus_by_title_compat(title)
            .then_some(WindowMatch::CompatibilityTitle)
    }

    pub fn move_account_window<W: GameWindowPort + ?Sized>(
        &self,
        windows: &W,
        account_id: &str,
        title: &str,
        position: WindowPosition,
    ) -> Option<WindowMatch> {
        if let Some(pid) = self.instances.find(account_id).map(|instance| instance.pid) {
            // Compatibility: the historical command never redirected a move
            // to a title match when a registered PID existed but had no window.
            return windows
                .move_to(pid, position)
                .then_some(WindowMatch::ProcessId);
        }
        windows
            .move_by_title_compat(title, position)
            .then_some(WindowMatch::CompatibilityTitle)
    }
}

#[cfg(test)]
mod tests {
    use super::WindowMatch;
    use crate::application::multi_instance::{
        GameWindowIdentity, GameWindowPort, MultiInstanceRuntime, WindowPosition,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeWindows {
        focused: Mutex<Vec<String>>,
        moved: Mutex<Vec<String>>,
        pid_available: bool,
    }

    impl GameWindowPort for FakeWindows {
        fn find_unique_process(&self, _identity: &GameWindowIdentity) -> Option<u32> {
            None
        }

        fn rename(&self, _pid: u32, _title: &str) {}

        fn move_to(&self, _pid: u32, _position: WindowPosition) -> bool {
            self.moved.lock().unwrap().push("pid".to_string());
            false
        }

        fn move_by_title_compat(&self, _title: &str, _position: WindowPosition) -> bool {
            self.moved.lock().unwrap().push("title".to_string());
            true
        }

        fn position(&self, _pid: u32) -> Option<WindowPosition> {
            None
        }

        fn set_taskbar_identity(&self, _pid: u32, _app_id: &str) -> Result<(), String> {
            Ok(())
        }

        fn focus_by_pid(&self, pid: u32) -> bool {
            self.focused.lock().unwrap().push(format!("pid:{pid}"));
            self.pid_available
        }

        fn focus_by_title_compat(&self, title: &str) -> bool {
            self.focused.lock().unwrap().push(format!("title:{title}"));
            true
        }
    }

    #[test]
    fn focus_prefers_the_registered_pid_and_keeps_the_legacy_title_fallback() {
        let runtime = MultiInstanceRuntime::default();
        runtime.instances().record_discovered("one", 42);
        let windows = FakeWindows {
            pid_available: false,
            ..FakeWindows::default()
        };
        let facade = runtime.facade();

        assert_eq!(
            facade.focus_account_window(&windows, "one", "Player One"),
            Some(WindowMatch::CompatibilityTitle)
        );
        assert_eq!(
            windows.focused.into_inner().unwrap(),
            ["pid:42", "title:Player One"]
        );
    }

    #[test]
    fn instance_registry_is_the_runtime_source_of_truth() {
        let runtime = MultiInstanceRuntime::default();
        runtime.instances().record_launched("one", 42, "-mod x");
        let facade = runtime.facade();

        assert_eq!(facade.instance("ONE").unwrap().pid, 42);
    }

    #[test]
    fn move_does_not_redirect_a_stale_registered_pid_to_a_title_match() {
        let runtime = MultiInstanceRuntime::default();
        runtime.instances().record_discovered("one", 42);
        let windows = FakeWindows::default();

        assert_eq!(
            runtime.facade().move_account_window(
                &windows,
                "one",
                "Player One",
                WindowPosition { x: 10, y: 20 }
            ),
            None
        );
        assert_eq!(windows.moved.into_inner().unwrap(), ["pid"]);
    }
}
