use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use ferrisetw::parser::Parser;
use ferrisetw::provider::Provider;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::UserTrace;
use ferrisetw::EventRecord;

const KERNEL_REGISTRY_PROVIDER_GUID: &str = "70eb4f03-c1de-4f73-a051-33d13d5413bd";
const QUERY_VALUE_EVENT_ID: u16 = 7;
const WEB_TOKEN_VALUE_NAME: &str = "WEB_TOKEN";

#[derive(Default)]
struct ObservationState {
    successful_read_pids: HashSet<u32>,
    matching_events: usize,
    parse_errors: usize,
}

fn is_successful_web_token_query(event_id: u16, status: u32, value_name: &str) -> bool {
    event_id == QUERY_VALUE_EVENT_ID
        && status == 0
        && value_name.eq_ignore_ascii_case(WEB_TOKEN_VALUE_NAME)
}

/// Short-lived ETW monitor that records successful WEB_TOKEN reads by process ID.
///
/// The trace starts before D2R is spawned so an early registry read cannot race PID discovery.
/// Observations are retained by PID and matched after the child process is known.
pub(crate) struct WebTokenReadMonitor {
    trace: Option<UserTrace>,
    state: Arc<Mutex<ObservationState>>,
}

impl WebTokenReadMonitor {
    pub(crate) fn start() -> Result<Self, String> {
        let state = Arc::new(Mutex::new(ObservationState::default()));
        let callback_state = Arc::clone(&state);

        let provider = Provider::by_guid(KERNEL_REGISTRY_PROVIDER_GUID)
            .add_callback(
                move |record: &EventRecord, schema_locator: &SchemaLocator| {
                    if record.event_id() != QUERY_VALUE_EVENT_ID {
                        return;
                    }

                    let schema = match schema_locator.event_schema(record) {
                        Ok(schema) => schema,
                        Err(_) => {
                            if let Ok(mut state) = callback_state.lock() {
                                state.parse_errors += 1;
                            }
                            return;
                        }
                    };
                    let parser = Parser::create(record, &schema);
                    let value_name = match parser.try_parse::<String>("ValueName") {
                        Ok(value_name) => value_name,
                        Err(_) => {
                            if let Ok(mut state) = callback_state.lock() {
                                state.parse_errors += 1;
                            }
                            return;
                        }
                    };

                    if !value_name.eq_ignore_ascii_case(WEB_TOKEN_VALUE_NAME) {
                        return;
                    }

                    let status = match parser.try_parse::<u32>("Status") {
                        Ok(status) => status,
                        Err(_) => {
                            if let Ok(mut state) = callback_state.lock() {
                                state.parse_errors += 1;
                            }
                            return;
                        }
                    };
                    if let Ok(mut state) = callback_state.lock() {
                        state.matching_events += 1;
                        if is_successful_web_token_query(record.event_id(), status, &value_name) {
                            state.successful_read_pids.insert(record.process_id());
                        }
                    }
                },
            )
            .build();

        let trace = UserTrace::new()
            .enable(provider)
            .start_and_process()
            .map_err(|error| format!("启动 WEB_TOKEN ETW 监听失败: {error:?}"))?;

        Ok(Self {
            trace: Some(trace),
            state,
        })
    }

    pub(crate) fn was_read_by(&self, pid: u32) -> bool {
        self.state
            .lock()
            .map(|state| state.successful_read_pids.contains(&pid))
            .unwrap_or(false)
    }

    pub(crate) fn diagnostics(&self) -> String {
        self.state
            .lock()
            .map(|state| {
                let mut pids: Vec<u32> = state.successful_read_pids.iter().copied().collect();
                pids.sort_unstable();
                format!(
                    "匹配事件 {}，成功读取 PID {:?}，解析错误 {}",
                    state.matching_events, pids, state.parse_errors
                )
            })
            .unwrap_or_else(|_| "ETW 观察状态锁异常".to_string())
    }

    pub(crate) fn stop(mut self) -> Result<(), String> {
        if let Some(trace) = self.trace.take() {
            trace
                .stop()
                .map_err(|error| format!("停止 WEB_TOKEN ETW 监听失败: {error:?}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{is_successful_web_token_query, WebTokenReadMonitor};
    use std::time::{Duration, Instant};
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    struct TestRegistryKey {
        path: String,
    }

    impl Drop for TestRegistryKey {
        fn drop(&mut self) {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let _ = hkcu.delete_subkey_all(&self.path);
        }
    }

    #[test]
    fn only_a_successful_web_token_query_is_consumption_evidence() {
        assert!(is_successful_web_token_query(7, 0, "WEB_TOKEN"));
        assert!(is_successful_web_token_query(7, 0, "web_token"));
        assert!(!is_successful_web_token_query(5, 0, "WEB_TOKEN"));
        assert!(!is_successful_web_token_query(7, 2, "WEB_TOKEN"));
        assert!(!is_successful_web_token_query(7, 0, "REGION"));
    }

    #[test]
    #[ignore = "requires an elevated process to start Microsoft-Windows-Kernel-Registry"]
    fn captures_web_token_query_from_the_process_that_read_it() {
        let test_path = format!(r"Software\D2RHub\Tests\Etw\{}", uuid::Uuid::new_v4());
        let cleanup = TestRegistryKey {
            path: test_path.clone(),
        };
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(&test_path).unwrap();
        key.set_value("WEB_TOKEN", &"test-token").unwrap();
        drop(key);

        let monitor = WebTokenReadMonitor::start().unwrap();
        let script = format!(
            "$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('{}'); Start-Sleep -Milliseconds 300; [void]$key.GetValue('WEB_TOKEN'); $key.Dispose()",
            test_path
        );
        let mut child = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .spawn()
            .unwrap();
        let child_pid = child.id();
        assert!(child.wait().unwrap().success());

        let deadline = Instant::now() + Duration::from_secs(3);
        while !monitor.was_read_by(child_pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }

        assert!(
            monitor.was_read_by(child_pid),
            "the ETW trace did not observe the controlled registry read: {}",
            monitor.diagnostics()
        );
        monitor.stop().unwrap();
        drop(cleanup);
    }
}
