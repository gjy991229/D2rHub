//! A two-phase Windows process handoff. The replacement acknowledges that it
//! can wait for this exact process before the mode is committed. It never
//! enters Tauri, takes the single-instance mutex, or reads configuration until
//! the old process has exited. Dropping an uncommitted handoff kills the child.
use crate::state::SharedState;
use std::sync::atomic::Ordering;

pub(crate) struct RuntimeWriteReservation {
    state: SharedState,
}

impl RuntimeWriteReservation {
    pub(crate) fn acquire(state: &SharedState) -> Result<Self, String> {
        state.host_runtime_busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "游戏启动或账号初始化进行中，请完成后再切换模式".to_string())?;
        if state.audio_mod_build_busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            state.host_runtime_busy.store(false, Ordering::Release);
            return Err("Mod 加工进行中，请完成后再切换模式".to_string());
        }
        Ok(Self { state: state.clone() })
    }
}

impl Drop for RuntimeWriteReservation {
    fn drop(&mut self) {
        self.state.audio_mod_build_busy.store(false, Ordering::Release);
        self.state.host_runtime_busy.store(false, Ordering::Release);
    }
}

pub(crate) struct WindowWriteReservation(SharedState);

impl WindowWriteReservation {
    pub(crate) fn acquire(state: &SharedState) -> Result<Self, String> {
        let _io = state.window_placement_io.try_lock()
            .ok_or("窗口位置正在保存，请稍后切换模式")?;
        state.window_writes_suspended.store(true, Ordering::Release);
        Ok(Self(state.clone()))
    }
}

impl Drop for WindowWriteReservation {
    fn drop(&mut self) {
        let _io = self.0.window_placement_io.lock();
        self.0.window_writes_suspended.store(false, Ordering::Release);
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::io::{Read, Write};
    use std::process::{Child, Command, Stdio};
    use std::os::windows::process::CommandExt;
    use std::os::windows::io::AsRawHandle;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use std::ffi::c_void;
    use tauri::Manager;
    use windows::Win32::Foundation::FILETIME;

    const ARGUMENT: &str = "--d2rhub-restart";
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const EVENT_MODIFY_STATE: u32 = 0x0002;
    const WAIT_OBJECT_0: u32 = 0;
    const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;
    static INHERITED_INSTANCES: Mutex<Option<Vec<RestartInstance>>> = Mutex::new(None);

    #[derive(serde::Serialize, serde::Deserialize)]
    struct RestartInstance {
        account_id: String,
        pid: u32,
        creation_time: u64,
        mod_args: Option<String>,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateEventW(attributes: *mut c_void, manual: i32, initial: i32, name: *const u16) -> *mut c_void;
        fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
        fn SetEvent(event: *mut c_void) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, millis: u32) -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetProcessTimes(
            process: *mut c_void,
            created: *mut FILETIME,
            exited: *mut FILETIME,
            kernel: *mut FILETIME,
            user: *mut FILETIME,
        ) -> i32;
        fn CancelSynchronousIo(thread: *mut c_void) -> i32;
    }

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) { unsafe { CloseHandle(self.0); } }
    }

    fn wide(value: &str) -> Vec<u16> { value.encode_utf16().chain(Some(0)).collect() }

    fn process_creation_time(pid: u32) -> Option<u64> {
        let process = Handle(unsafe { OpenProcess(SYNCHRONIZE | 0x1000, 0, pid) });
        if process.0.is_null() || unsafe { WaitForSingleObject(process.0, 0) } != 258 { return None; }
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        (unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) } != 0)
            .then_some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }

    pub(crate) fn restore_instances(state: &crate::state::SharedState) {
        let inherited = INHERITED_INSTANCES.lock().unwrap_or_else(|error| error.into_inner()).take();
        if let Some(instances) = inherited {
            for instance in instances {
                if process_creation_time(instance.pid) != Some(instance.creation_time) { continue; }
                if let Some(arguments) = &instance.mod_args {
                    state.multi_instance().instances().record_launched(&instance.account_id, instance.pid, arguments);
                } else {
                    state.multi_instance().instances().record_discovered(&instance.account_id, instance.pid);
                }
            }
        }
    }

    #[derive(Default)]
    struct PendingChild {
        child: Mutex<Option<Child>>,
        cancelled: AtomicBool,
    }

    impl PendingChild {
        fn cancel(&self) {
            self.cancelled.store(true, Ordering::Release);
            if let Some(mut child) = self.child.lock().unwrap_or_else(|error| error.into_inner()).take() {
                // Killing the reader releases a blocked pipe write. Do not
                // add an unbounded wait to the caller's timeout path.
                let _ = child.kill();
            }
        }

        fn install(&self, mut child: Child) -> Result<(), String> {
            let mut slot = self.child.lock().unwrap_or_else(|error| error.into_inner());
            if self.cancelled.load(Ordering::Acquire) {
                let _ = child.kill();
                return Err("重启准备已取消".to_string());
            }
            *slot = Some(child);
            Ok(())
        }
    }

    pub(crate) struct PreparedRestart {
        child: Arc<PendingChild>,
        exit_request: Option<mpsc::SyncSender<()>>,
    }

    impl PreparedRestart {
        pub(crate) fn prepare(app: &tauri::AppHandle) -> Result<Self, String> {
            let deadline = Instant::now() + Duration::from_secs(10);
            let child = Arc::new(PendingChild::default());
            let mut prepared = Self { child: child.clone(), exit_request: None };
            let state = app.state::<crate::state::SharedState>().inner().clone();
            let (reply, completion) = mpsc::sync_channel(1);
            let worker = std::thread::Builder::new().name("restart-handoff".to_string())
                .spawn(move || {
                    let result = prepare_child(&state, &child, deadline);
                    if reply.send(result).is_err() { child.cancel(); }
                }).map_err(|error| format!("无法准备重启交接: {error}"))?;
            match completion.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(result) => result?,
                Err(error) => {
                    prepared.child.cancel();
                    unsafe { CancelSynchronousIo(worker.as_raw_handle()); }
                    return Err(format!("重启准备未能按时完成，原模式已保留: {error}"));
                }
            }
            // Allocate the exit worker before the durable mode commit.
            let (sender, receiver) = mpsc::sync_channel(1);
            let app = app.clone();
            std::thread::Builder::new().name("mode-restart".to_string()).spawn(move || {
                if receiver.recv().is_ok() {
                    std::thread::sleep(Duration::from_millis(250));
                    app.exit(0);
                }
            }).map_err(|error| format!("无法准备重启退出流程: {error}"))?;
            prepared.exit_request = Some(sender);
            Ok(prepared)
        }

        pub(crate) fn commit(mut self) {
            self.child.child.lock().unwrap_or_else(|error| error.into_inner()).take();
            if let Some(sender) = self.exit_request.take() {
                if sender.send(()).is_err() {
                    std::process::exit(0);
                }
            }
        }
    }

    fn prepare_child(
        state: &crate::state::SharedState,
        pending: &PendingChild,
        deadline: Instant,
    ) -> Result<(), String> {
        let instances: Vec<_> = state.multi_instance().instances().list().into_iter()
            .filter_map(|instance| Some(RestartInstance {
                creation_time: process_creation_time(instance.pid)?,
                account_id: instance.account_id,
                pid: instance.pid,
                mod_args: instance.launch.map(|launch| launch.mod_args),
            })).collect();
        let payload = serde_json::to_vec(&instances)
            .map_err(|error| format!("无法准备游戏运行状态: {error}"))?;
        if payload.len() > MAX_SNAPSHOT_BYTES {
            return Err("游戏运行状态超过重启交接容量，原模式已保留".to_string());
        }
        if pending.cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err("重启准备已超时".to_string());
        }
        let event_name = format!("Local\\D2RHub_Restart_{}", uuid::Uuid::new_v4());
        let event = Handle(unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, wide(&event_name).as_ptr()) });
        if event.0.is_null() { return Err(format!("无法准备重启交接: {}", std::io::Error::last_os_error())); }
        let executable = std::env::current_exe().map_err(|error| format!("无法定位 D2RHub: {error}"))?;
        let child = Command::new(executable)
            .arg(ARGUMENT).arg(std::process::id().to_string()).arg(&event_name)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW; the app shows its own WebView later.
            .spawn().map_err(|error| format!("无法启动新进程，原模式已保留: {error}"))?;
        pending.install(child)?;
        let mut input = pending.child.lock().unwrap_or_else(|error| error.into_inner())
            .as_mut().and_then(|child| child.stdin.take())
            .ok_or_else(|| "无法传递运行中的游戏状态".to_string())?;
        // No child-state lock is held during the blocking write. The
        // caller can terminate the reader and cancel this thread's I/O.
        input.write_all(&payload)
            .map_err(|error| format!("无法传递运行中的游戏状态: {error}"))?;
        drop(input);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if pending.cancelled.load(Ordering::Acquire) || remaining.is_zero()
            || unsafe { WaitForSingleObject(event.0, remaining.as_millis().min(u32::MAX as u128) as u32) } != WAIT_OBJECT_0
        {
            return Err("新进程未能完成重启准备，原模式已保留".to_string());
        }
        let mut child = pending.child.lock().unwrap_or_else(|error| error.into_inner());
        if child.as_mut().ok_or("重启准备已取消")?.try_wait()
            .map_err(|error| format!("无法确认新进程状态: {error}"))?.is_some()
        {
            return Err("新进程提前退出，原模式已保留".to_string());
        }
        Ok(())
    }

    impl Drop for PreparedRestart {
        fn drop(&mut self) {
            self.child.cancel();
        }
    }

    pub(crate) fn wait_for_predecessor() -> Result<(), String> {
        let mut args = std::env::args_os().skip(1);
        if args.next().as_deref() != Some(std::ffi::OsStr::new(ARGUMENT)) { return Ok(()); }
        let pid = args.next().and_then(|value| value.to_str().and_then(|value| value.parse::<u32>().ok()))
            .filter(|pid| *pid != 0 && *pid != std::process::id())
            .ok_or_else(|| "重启交接进程无效".to_string())?;
        let name = args.next().and_then(|value| value.into_string().ok())
            .filter(|value| value.starts_with("Local\\D2RHub_Restart_"))
            .ok_or_else(|| "重启交接标识无效".to_string())?;
        let parent = Handle(unsafe { OpenProcess(SYNCHRONIZE, 0, pid) });
        if parent.0.is_null() { return Err("无法等待原进程退出".to_string()); }
        let mut payload = Vec::new();
        std::io::stdin().take(MAX_SNAPSHOT_BYTES as u64 + 1).read_to_end(&mut payload)
            .map_err(|error| format!("无法接收游戏运行状态: {error}"))?;
        if payload.len() > MAX_SNAPSHOT_BYTES { return Err("游戏运行状态超过重启交接容量".to_string()); }
        let instances: Vec<RestartInstance> = serde_json::from_slice(&payload)
            .map_err(|error| format!("游戏运行状态无效: {error}"))?;
        *INHERITED_INSTANCES.lock().unwrap_or_else(|error| error.into_inner()) = Some(instances);
        let ready = Handle(unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wide(&name).as_ptr()) });
        if ready.0.is_null() || unsafe { SetEvent(ready.0) } == 0 {
            return Err("无法确认重启准备完成".to_string());
        }
        if unsafe { WaitForSingleObject(parent.0, u32::MAX) } != WAIT_OBJECT_0 {
            return Err("等待原进程退出失败".to_string());
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub(crate) use windows::{restore_instances, wait_for_predecessor, PreparedRestart};

#[cfg(not(target_os = "windows"))]
pub(crate) fn wait_for_predecessor() -> Result<(), String> { Ok(()) }

#[cfg(not(target_os = "windows"))]
pub(crate) fn restore_instances(_state: &SharedState) {}

#[cfg(not(target_os = "windows"))]
pub(crate) struct PreparedRestart;
#[cfg(not(target_os = "windows"))]
impl PreparedRestart {
    pub(crate) fn prepare(_app: &tauri::AppHandle) -> Result<Self, String> {
        Err("模式重启交接仅支持 Windows".to_string())
    }
    pub(crate) fn commit(self) {}
}
