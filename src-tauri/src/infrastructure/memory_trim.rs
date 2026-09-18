//! Batch working-set trimming for successfully launched game windows.

use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;

use windows::core::PWSTR;
use windows::Win32::Foundation::{HANDLE, WAIT_TIMEOUT};
use windows::Win32::System::ProcessStatus::{
    EmptyWorkingSet, GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_SYNCHRONIZE,
};

use crate::application::multi_instance::CancellationTicket;
use crate::domain::config::GlobalConfig;
use crate::state::SharedState;

static TRIM_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub(crate) struct PendingMemoryTrim {
    // Hold the original process object throughout login and cleanup. Never reopen by PID.
    process: OwnedHandle,
    account_id: String,
    pid: u32,
}

impl PendingMemoryTrim {
    pub(crate) fn prepare(
        config: &GlobalConfig,
        executable: &Path,
        account_id: &str,
        pid: u32,
    ) -> Option<Self> {
        if !config.launch_trim_memory {
            return None;
        }

        let result = (|| -> Result<OwnedHandle, String> {
            let handle = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_SYNCHRONIZE,
                    false,
                    pid,
                )
            }
            .map_err(|error| error.to_string())?;
            // OpenProcess returned a uniquely owned, non-null handle.
            let process = unsafe { OwnedHandle::from_raw_handle(handle.0) };
            let mut image = vec![0u16; 32_768];
            let mut length = image.len() as u32;
            unsafe {
                QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    PWSTR(image.as_mut_ptr()),
                    &mut length,
                )
            }
            .map_err(|error| error.to_string())?;
            let actual =
                std::path::PathBuf::from(std::ffi::OsString::from_wide(&image[..length as usize]));
            if !super::system::executable_paths_match(&actual, executable) {
                return Err("目标进程路径与本次启动不匹配".to_string());
            }
            Ok(process)
        })();

        match result {
            Ok(process) => Some(Self {
                process,
                account_id: account_id.to_string(),
                pid,
            }),
            Err(error) => {
                crate::logger::log_msg(
                    "WARN",
                    "MemoryTrim",
                    &format!("[Account {account_id}] PID {pid}: 跳过内存整理：{error}"),
                );
                None
            }
        }
    }

    fn trim(&self) {
        let handle = HANDLE(self.process.as_raw_handle());
        if unsafe { WaitForSingleObject(handle, 0) } != WAIT_TIMEOUT {
            self.log("INFO", "原游戏进程已退出或不可用，跳过内存整理");
            return;
        }
        let before = working_set_mib(handle);
        match unsafe { EmptyWorkingSet(handle) } {
            Ok(()) => {
                let after = working_set_mib(handle);
                self.log(
                    "INFO",
                    &format!(
                        "工作集整理完成：{before} → {after} MiB（即时工作集，不代表整机释放量）"
                    ),
                );
            }
            Err(error) => self.log("WARN", &format!("内存整理失败，不影响游戏：{error}")),
        }
    }

    fn log(&self, level: &str, message: &str) {
        crate::logger::log_msg(
            level,
            "MemoryTrim",
            &format!("[Account {}] PID {}: {message}", self.account_id, self.pid),
        );
    }
}

pub(crate) struct BatchMemoryTrim {
    processes: Vec<std::sync::Arc<PendingMemoryTrim>>,
    successful: usize,
    halfway: usize,
    total: usize,
}

impl BatchMemoryTrim {
    pub(crate) fn new(total: usize) -> Self {
        Self {
            processes: Vec::new(),
            successful: 0,
            halfway: total.div_ceil(2),
            total,
        }
    }

    pub(crate) fn confirm(&mut self, process: Option<PendingMemoryTrim>) {
        self.successful += 1;
        if let Some(process) = process {
            self.processes.push(std::sync::Arc::new(process));
        }
    }

    pub(crate) async fn trim_halfway(&self, state: &SharedState, cancellation: CancellationTicket) {
        // For a single window, halfway and completion are the same checkpoint.
        // The denominator is the requested batch size, so accounts skipped by
        // preflight can leave the halfway checkpoint unreached; the batch
        // always finishes with one more `trim` call.
        if self.total > 1 && self.successful == self.halfway {
            self.trim(state, cancellation).await;
        }
    }

    pub(crate) async fn trim(&self, state: &SharedState, cancellation: CancellationTicket) {
        if self.processes.is_empty() {
            return;
        }
        let processes = self.processes.clone();
        let state = state.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _guard = TRIM_LOCK.lock();
            for process in processes {
                // A cancelled batch must not wait for every queued window to be
                // trimmed; the remaining games keep running normally.
                if state.multi_instance().facade().is_cancelled(cancellation) {
                    break;
                }
                if !state
                    .configuration()
                    .snapshot()
                    .is_some_and(|config| config.launch_trim_memory)
                {
                    break;
                }
                process.trim();
            }
        })
        .await;
        if let Err(error) = result {
            crate::logger::log_msg(
                "WARN",
                "MemoryTrim",
                &format!("批次内存整理任务异常：{error}"),
            );
        }
    }
}

fn working_set_mib(handle: HANDLE) -> String {
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: size,
        ..Default::default()
    };
    if unsafe { GetProcessMemoryInfo(handle, &mut counters, size) }.is_ok() {
        format!("{:.1}", counters.WorkingSetSize as f64 / 1_048_576.0)
    } else {
        "未知".to_string()
    }
}
