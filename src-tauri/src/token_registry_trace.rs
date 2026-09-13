use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use ferrisetw::parser::Parser;
use ferrisetw::provider::Provider;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::{RealTimeTraceTrait, TraceTrait, UserTrace};
use ferrisetw::EventRecord;

const KERNEL_REGISTRY_PROVIDER_GUID: &str = "70eb4f03-c1de-4f73-a051-33d13d5413bd";
const QUERY_VALUE_EVENT_ID: u16 = 7;
pub(crate) const WEB_TOKEN_VALUE_NAME: &str = "WEB_TOKEN";

#[derive(Default)]
struct ObservationState {
    successful_read_pids: HashSet<u32>,
    matching_events: usize,
    parse_errors: usize,
    total_events: usize,
    query_events: usize,
    target_pid: Option<u32>,
    target_events: usize,
    target_queries: usize,
    last_event: Option<Instant>,
    parse_samples: Vec<String>,
    parse_stages: BTreeMap<&'static str, usize>,
    token_statuses: BTreeMap<(u32, u32), usize>,
    processor: String,
    stopping: bool,
}

impl ObservationState {
    fn parse_failure(&mut self, record: &EventRecord, field: &'static str, error: impl std::fmt::Debug) {
        self.parse_errors += 1;
        *self.parse_stages.entry(field).or_default() += 1;
        if self.parse_samples.len() < 3 {
            self.parse_samples.push(format!("PID={} ID={} version={} field={field}: {error:?}",
                record.process_id(), record.event_id(), record.version()));
        }
    }
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
    worker: Option<JoinHandle<()>>,
    started: Instant,
    session_name: String,
}

impl WebTokenReadMonitor {
    pub(crate) fn start() -> Result<Self, String> {
        let started = Instant::now();
        let state = Arc::new(Mutex::new(ObservationState {
            processor: "待启动".to_string(),
            ..Default::default()
        }));
        let callback_state = Arc::clone(&state);

        let provider = Provider::by_guid(KERNEL_REGISTRY_PROVIDER_GUID)
            .add_callback(
                move |record: &EventRecord, schema_locator: &SchemaLocator| {
                    if let Ok(mut state) = callback_state.lock() {
                        state.total_events += 1;
                        state.last_event = Some(Instant::now());
                        if record.event_id() == QUERY_VALUE_EVENT_ID { state.query_events += 1; }
                        if state.target_pid == Some(record.process_id()) {
                            state.target_events += 1;
                            if record.event_id() == QUERY_VALUE_EVENT_ID { state.target_queries += 1; }
                        }
                    }
                    if record.event_id() != QUERY_VALUE_EVENT_ID {
                        return;
                    }

                    let schema = match schema_locator.event_schema(record) {
                        Ok(schema) => schema,
                        Err(error) => {
                            if let Ok(mut state) = callback_state.lock() {
                                state.parse_failure(record, "schema", error);
                            }
                            return;
                        }
                    };
                    let parser = Parser::create(record, &schema);
                    let value_name = match parser.try_parse::<String>("ValueName") {
                        Ok(value_name) => value_name,
                        Err(error) => {
                            if let Ok(mut state) = callback_state.lock() {
                                state.parse_failure(record, "ValueName", error);
                            }
                            return;
                        }
                    };

                    if !value_name.eq_ignore_ascii_case(WEB_TOKEN_VALUE_NAME) {
                        return;
                    }

                    let status = match parser.try_parse::<u32>("Status") {
                        Ok(status) => status,
                        Err(error) => {
                            if let Ok(mut state) = callback_state.lock() {
                                state.parse_failure(record, "Status", error);
                            }
                            return;
                        }
                    };
                    if let Ok(mut state) = callback_state.lock() {
                        state.matching_events += 1;
                        let key = (record.process_id(), status);
                        if state.token_statuses.contains_key(&key) || state.token_statuses.len() < 32 {
                            *state.token_statuses.entry(key).or_default() += 1;
                        }
                        if is_successful_web_token_query(record.event_id(), status, &value_name) {
                            state.successful_read_pids.insert(record.process_id());
                        }
                    }
                },
            )
            .build();

        let (trace, handle) = UserTrace::new()
            .enable(provider)
            .start()
            .map_err(|error| {
                let message = format!("启动 WEB_TOKEN ETW 监听失败（StartTrace/EnableProvider/OpenTrace）: {error:?}");
                crate::logger::log_msg("ERROR", "TokenETW", &message);
                message
            })?;
        let session_name = trace.trace_name().to_string_lossy().into_owned();
        let worker_state = Arc::clone(&state);
        let worker_session = session_name.clone();
        let worker = std::thread::Builder::new().name("web-token-etw".to_string()).spawn(move || {
            if let Ok(mut state) = worker_state.lock() { state.processor = "运行中".to_string(); }
            let result = std::panic::catch_unwind(|| UserTrace::process_from_handle(handle));
            let outcome = match result {
                Ok(Ok(())) => "正常返回".to_string(),
                Ok(Err(error)) => format!("ProcessTrace 错误: {error:?}"),
                Err(_) => "ProcessTrace 线程 panic".to_string(),
            };
            let stopping = if let Ok(mut state) = worker_state.lock() {
                state.processor = outcome.clone();
                state.stopping
            } else { false };
            crate::logger::log_msg(if stopping { "INFO" } else { "ERROR" }, "TokenETW",
                &format!("session={worker_session} 事件处理结束；主动停止={stopping}；{outcome}"));
        }).map_err(|error| format!("创建 ETW 消费线程失败: {error}"))?;
        crate::logger::log_msg("INFO", "TokenETW", &format!(
            "session={session_name} 监听已启动；provider={KERNEL_REGISTRY_PROVIDER_GUID}；等待目标 PID；启动耗时={}ms", started.elapsed().as_millis()));

        Ok(Self {
            trace: Some(trace),
            state,
            worker: Some(worker),
            started,
            session_name,
        })
    }

    pub(crate) fn was_read_by(&self, pid: u32) -> bool {
        self.state
            .lock()
            .map(|mut state| {
                if state.target_pid != Some(pid) {
                    state.target_pid = Some(pid);
                    state.target_events = 0;
                    state.target_queries = 0;
                    crate::logger::log_msg("INFO", "TokenETW", &format!(
                        "session={} 绑定目标 PID={pid}；监听已运行={}ms；早期读取已缓存={}",
                        self.session_name, self.started.elapsed().as_millis(), state.successful_read_pids.contains(&pid)));
                }
                state.successful_read_pids.contains(&pid)
            })
            .unwrap_or(false)
    }

    pub(crate) fn diagnostics(&self) -> String {
        let session_stats = self.session_stats();
        self.state
            .lock()
            .map(|state| {
                let mut pids: Vec<u32> = state.successful_read_pids.iter().copied().collect();
                pids.sort_unstable();
                let observation = if state.target_pid.is_some_and(|pid| state.successful_read_pids.contains(&pid)) {
                    "目标读取已命中"
                } else if self.worker.as_ref().is_some_and(|worker| worker.is_finished()) {
                    "消费线程已结束，查看线程返回错误与主动停止标志"
                } else if state.total_events == 0 {
                    "未收到任何该 Provider 事件；需结合会话查询/丢失统计，不能断定游戏未读 Token"
                } else if state.query_events == 0 {
                    "Provider 有事件，但未收到 ID=7 的 QueryValue"
                } else if state.parse_errors != 0 {
                    "存在解析失败，不能排除目标事件未解析；错误计数为全局 Provider 范围"
                } else if !pids.is_empty() {
                    "仅观察到其他 PID 成功读取 WEB_TOKEN，目标 PID 未命中"
                } else {
                    "监听有事件但未确认目标读取；查看读取状态，未命中不等于监听故障"
                };
                format!(
                    "session={}，目标PID={:?}，监听={}ms，处理线程={}，线程已结束={}，总事件={}，QueryValue={}，目标事件/查询（绑定后）={}/{}，最近事件距今ms={:?}，匹配事件={}，成功读取PID={:?}，WEB_TOKEN读取状态(PID,NTSTATUS十六进制,次数，最多32组)={:?}，解析错误={}，解析样例={:?}，{}，解析阶段={:?}，观察结论={}",
                    self.session_name, state.target_pid, self.started.elapsed().as_millis(),
                    state.processor, self.worker.as_ref().is_some_and(|worker| worker.is_finished()),
                    state.total_events, state.query_events, state.target_events, state.target_queries,
                    state.last_event.map(|at| at.elapsed().as_millis()), state.matching_events, pids,
                    state.token_statuses.iter().map(|((pid, status), count)| (*pid, format!("0x{status:08X}"), *count)).collect::<Vec<_>>(),
                    state.parse_errors, state.parse_samples, session_stats, state.parse_stages, observation
                )
            })
            .unwrap_or_else(|_| "ETW 观察状态锁异常".to_string())
    }

    pub(crate) fn stop(mut self) -> Result<(), String> {
        if let Ok(mut state) = self.state.lock() { state.stopping = true; }
        if let Some(trace) = self.trace.take() {
            trace
                .stop()
                .map_err(|error| format!("停止 WEB_TOKEN ETW 监听失败: {error:?}"))?;
        }
        Ok(())
    }

    fn session_stats(&self) -> String {
        use windows::core::PCWSTR;
        use windows::Win32::System::Diagnostics::Etw::{ControlTraceW, CONTROLTRACE_HANDLE,
            EVENT_TRACE_CONTROL_QUERY, EVENT_TRACE_PROPERTIES};
        #[repr(C)]
        struct QueryBuffer {
            properties: EVENT_TRACE_PROPERTIES,
            names: [u16; 2048],
        }
        let mut buffer = QueryBuffer { properties: EVENT_TRACE_PROPERTIES::default(), names: [0; 2048] };
        buffer.properties.Wnode.BufferSize = std::mem::size_of::<QueryBuffer>() as u32;
        buffer.properties.LoggerNameOffset = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() as u32;
        buffer.properties.LogFileNameOffset = buffer.properties.LoggerNameOffset + 2048;
        let name = self.session_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let result = unsafe { ControlTraceW(CONTROLTRACE_HANDLE { Value: 0 }, PCWSTR(name.as_ptr()),
            &mut buffer.properties, EVENT_TRACE_CONTROL_QUERY) };
        if result.0 != 0 { return format!("ETW会话统计查询失败 Win32={}（不能据此认定无丢失）", result.0); }
        format!("EventsLost={}，RealTimeBuffersLost={}，LogBuffersLost={}，BuffersWritten={}",
            buffer.properties.EventsLost, buffer.properties.RealTimeBuffersLost,
            buffer.properties.LogBuffersLost, buffer.properties.BuffersWritten)
    }
}

impl Drop for WebTokenReadMonitor {
    fn drop(&mut self) {
        if self.trace.is_some() {
            crate::logger::log_msg("INFO", "TokenETW", &format!(
                "作用域提前结束，释放监听；{}", self.diagnostics()));
        }
        if let Ok(mut state) = self.state.lock() { state.stopping = true; }
        // CloseTrace releases the consumer before joining, including early returns.
        drop(self.trace.take());
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
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
