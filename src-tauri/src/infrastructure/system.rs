use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use sysinfo::{ProcessesToUpdate, System};
use tauri::Manager;

use crate::application::multi_instance::{GameWindowIdentity, GameWindowPort, WindowPosition};
use crate::error::AppError;
use crate::infrastructure::process::{kill_processes_by_name, shared_system, silent_cmd};
use crate::launch_context::paths_have_same_identity;

/// 启动进度事件（通过 Tauri event 推送到前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchProgress {
    pub account_id: String,
    pub step: String,
    pub status: String, // "pending" | "running" | "ok" | "error"
    pub message: String,
}

impl LaunchProgress {
    pub fn new(account_id: &str, step: &str, status: &str, message: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            step: step.to_string(),
            status: status.to_string(),
            message: message.to_string(),
        }
    }
}

/// Transitional Windows adapter for the multi-instance application port.
/// Existing command helpers remain compatibility delegates until their callers migrate.
pub(crate) struct SystemGameWindowPort;

impl GameWindowPort for SystemGameWindowPort {
    fn find_unique_process(&self, identity: &GameWindowIdentity) -> Option<u32> {
        find_unique_d2r_pid_by_window_identity(&identity.title, &identity.executable)
    }

    fn rename(&self, pid: u32, title: &str) {
        rename_game_window(pid, title);
    }

    fn move_to(&self, pid: u32, position: WindowPosition) -> bool {
        find_game_hwnd(pid)
            .map(|hwnd| move_window_handle(hwnd, position.x, position.y))
            .unwrap_or(false)
    }

    fn move_by_title_compat(&self, title: &str, position: WindowPosition) -> bool {
        set_game_window_position_by_title(title, position.x, position.y)
    }

    fn position(&self, pid: u32) -> Option<WindowPosition> {
        find_game_hwnd(pid)
            .and_then(get_window_rect)
            .map(|(x, y)| WindowPosition { x, y })
    }

    fn set_taskbar_identity(&self, pid: u32, app_id: &str) -> Result<(), String> {
        set_game_window_app_user_model_id(pid, app_id)
    }

    fn focus_by_pid(&self, pid: u32) -> bool {
        let Some(hwnd) = find_game_hwnd(pid) else {
            return false;
        };
        bring_window_to_foreground_raw(hwnd);
        true
    }

    fn focus_by_title_compat(&self, title: &str) -> bool {
        let Some(hwnd) = find_game_hwnd_by_title(title) else {
            return false;
        };
        bring_window_to_foreground_raw(hwnd);
        true
    }
}

// ── 进程管理 ──

/// 检测当前有哪些 D2R 进程在运行（返回 PID 列表）
pub fn get_d2r_pids() -> Vec<u32> {
    let mut pids = vec![];
    let mut sys = shared_system().lock().unwrap_or_else(|e| e.into_inner());
    sys.refresh_processes(ProcessesToUpdate::All);
    for (pid, proc) in sys.processes() {
        if proc.name().to_string_lossy() == "D2R.exe" {
            pids.push(pid.as_u32());
        }
    }
    pids
}

/// 杀死所有暗黑2进程 (D2R.exe，不分大小写)
pub fn kill_all_d2r_processes() -> Result<(), AppError> {
    let mut sys = shared_system().lock().unwrap_or_else(|e| e.into_inner());
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);

    // First round of kill using sysinfo
    for proc in sys.processes().values() {
        let name = proc.name().to_string_lossy();
        if name.eq_ignore_ascii_case("D2R.exe") && !proc.kill() {
            // If kill fails, try taskkill
            let _ = silent_cmd("taskkill")
                .args(["/F", "/PID", &proc.pid().to_string()])
                .output();
        }
    }

    // Wait a brief moment to let processes exit
    std::thread::sleep(std::time::Duration::from_millis(800));

    // Fallback: search again and use taskkill command for any remaining D2R.exe just in case
    let mut still_running = false;
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
    for proc in sys.processes().values() {
        let name = proc.name().to_string_lossy();
        if name.eq_ignore_ascii_case("D2R.exe") {
            still_running = true;
            let _ = silent_cmd("taskkill")
                .args(["/F", "/PID", &proc.pid().to_string()])
                .output();
        }
    }

    if still_running {
        std::thread::sleep(std::time::Duration::from_millis(500));
        // Verify again
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
        for proc in sys.processes().values() {
            let name = proc.name().to_string_lossy();
            if name.eq_ignore_ascii_case("D2R.exe") {
                return Err(AppError::FileError(
                    "部分暗黑2进程未能成功关闭，请尝试以管理员身份运行本工具。".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// 优雅关闭战网（战网默认会拦截关闭信号最小化到托盘，无法正常关闭）
/// 缓冲 1.5 秒让 Battle.net.exe 将认证状态持久化到注册表，然后强杀。
/// Agent.exe 不参与 token 管理——此处杀 Agent 仅为多开管理需要。
pub fn graceful_kill_bnet(_timeout_secs: u64) -> bool {
    // 缓冲 1.5 秒，让 Battle.net.exe 有足够时间将 token 写入注册表
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // 只有确认进程退出才报告成功；调用方可据此决定是否安全回写账号状态。
    kill_processes_by_name(&["Battle.net.exe", "Agent.exe"]).is_ok()
}

/// 记录进程快照（用于后续对比判断新进程）
pub fn snapshot_processes(process_name: String) -> Vec<u32> {
    let mut pids = vec![];
    let mut sys = shared_system().lock().unwrap_or_else(|e| e.into_inner());
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
    for (pid, proc) in sys.processes() {
        if proc.name().to_string_lossy() == process_name {
            pids.push(pid.as_u32());
        }
    }
    pids
}

/// 等待指定名称的新进程出现（与已知 PID 列表对比）
pub async fn wait_for_new_process(
    process_name: String,
    known_pids: Vec<u32>,
    timeout_secs: u64,
) -> Result<u32, AppError> {
    let start = std::time::Instant::now();
    loop {
        let current = snapshot_processes(process_name.clone());
        for pid in &current {
            if !known_pids.contains(pid) {
                return Ok(*pid);
            }
        }
        if start.elapsed().as_secs() > timeout_secs {
            return Err(AppError::GameLaunchTimeout(timeout_secs));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

// ── Native Windows 句柄操作 ──

use std::ffi::c_void;

// Keep the native Windows spelling so the declarations can be compared with the SDK.
#[allow(clippy::upper_case_acronyms)]
type NTSTATUS = i32;
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INFO_LENGTH_MISMATCH: NTSTATUS = 0xC0000004u32 as i32;

#[repr(C)]
#[allow(non_snake_case)]
struct UNICODE_STRING {
    Length: u16,
    MaximumLength: u16,
    Buffer: *mut u16,
}

#[repr(C)]
#[allow(non_snake_case)]
struct OBJECT_NAME_INFORMATION {
    Name: UNICODE_STRING,
}

#[repr(C)]
#[allow(non_snake_case)]
struct SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX {
    Object: *mut c_void,
    UniqueProcessId: usize,
    HandleValue: usize,
    GrantedAccess: u32,
    CreatorBackTraceIndex: u16,
    ObjectTypeIndex: u16,
    HandleAttributes: u32,
    Reserved: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct SYSTEM_HANDLE_INFORMATION_EX {
    NumberOfHandles: usize,
    Reserved: usize,
    Handles: [SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX; 1],
}

extern "system" {
    fn NtQuerySystemInformation(
        SystemInformationClass: u32,
        SystemInformation: *mut c_void,
        SystemInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> NTSTATUS;

    fn NtQueryObject(
        Handle: *mut c_void,
        ObjectInformationClass: u32,
        ObjectInformation: *mut c_void,
        ObjectInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> NTSTATUS;

    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> *mut c_void;

    fn DuplicateHandle(
        hSourceProcessHandle: *mut c_void,
        hSourceHandle: *mut c_void,
        hTargetProcessHandle: *mut c_void,
        lpTargetHandle: *mut *mut c_void,
        dwDesiredAccess: u32,
        bInheritHandle: i32,
        dwOptions: u32,
    ) -> i32;

    fn CloseHandle(hObject: *mut c_void) -> i32;
}

unsafe fn get_system_handles() -> Result<Vec<SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX>, AppError> {
    let mut size = 1024 * 1024 * 2; // 初始 2MB 缓冲区
    let mut buffer = vec![0u8; size];
    let mut return_length = 0u32;

    loop {
        let status = NtQuerySystemInformation(
            64, // SystemExtendedHandleInformation
            buffer.as_mut_ptr() as *mut c_void,
            size as u32,
            &mut return_length,
        );

        if status == STATUS_SUCCESS {
            break;
        } else if status == STATUS_INFO_LENGTH_MISMATCH || status == 0xC0000004u32 as i32 {
            size *= 2;
            buffer = vec![0u8; size];
        } else {
            return Err(AppError::FileError(format!(
                "NtQuerySystemInformation 失败: {:#x}",
                status
            )));
        }
    }

    let info = buffer.as_ptr() as *const SYSTEM_HANDLE_INFORMATION_EX;
    let num_handles = (*info).NumberOfHandles;

    // 安全检查，计算缓冲区最大容纳的项数，避免越界
    let max_possible = (size - std::mem::size_of::<usize>() * 2)
        / std::mem::size_of::<SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX>();
    let actual_num = num_handles.min(max_possible);

    let mut handles = Vec::with_capacity(actual_num);
    let handles_ptr = (*info).Handles.as_ptr();
    for i in 0..actual_num {
        handles.push(std::ptr::read(handles_ptr.add(i)));
    }

    Ok(handles)
}

unsafe fn detect_event_type_index() -> Result<u16, AppError> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        fn CreateEventW(
            lpEventAttributes: *mut c_void,
            bManualReset: i32,
            bInitialState: i32,
            lpName: *const u16,
        ) -> *mut c_void;
    }

    // 创建一个临时命名的事件对象以探测系统的 Event 类型索引
    let name: Vec<u16> = OsStr::new("D2RHubTempDetectEvent")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // 创建 Event
    let event_handle = CreateEventW(std::ptr::null_mut(), 0, 0, name.as_ptr());
    if event_handle.is_null() {
        return Err(AppError::FileError(
            "创建 Event 类型探测句柄失败".to_string(),
        ));
    }

    let result = get_system_handles().and_then(|handles| {
        let current_pid = std::process::id() as usize;
        for entry in handles {
            if entry.UniqueProcessId == current_pid && entry.HandleValue == event_handle as usize {
                return Ok(entry.ObjectTypeIndex);
            }
        }
        Err(AppError::FileError(
            "无法识别 Event 句柄类型索引".to_string(),
        ))
    });

    CloseHandle(event_handle);
    result
}

unsafe fn get_object_name(handle: *mut c_void) -> Result<Option<String>, AppError> {
    let mut size = 1024usize;
    for _ in 0..3 {
        let mut buffer = vec![0u8; size];
        let mut return_length = 0u32;
        let status = NtQueryObject(
            handle,
            1, // ObjectNameInformation
            buffer.as_mut_ptr() as *mut c_void,
            size as u32,
            &mut return_length,
        );

        if status == STATUS_INFO_LENGTH_MISMATCH {
            size = (return_length as usize).max(size.saturating_mul(2));
            continue;
        }
        if status != STATUS_SUCCESS {
            return Err(AppError::FileError(format!(
                "NtQueryObject 读取句柄名称失败: {status:#x}"
            )));
        }

        let info = buffer.as_ptr() as *const OBJECT_NAME_INFORMATION;
        let name_bytes = (*info).Name.Length as usize;
        if name_bytes == 0 || (*info).Name.Buffer.is_null() {
            return Ok(None);
        }
        let name_start = (*info).Name.Buffer as usize;
        let buffer_start = buffer.as_ptr() as usize;
        let buffer_end = buffer_start.saturating_add(buffer.len());
        let name_end = name_start
            .checked_add(name_bytes)
            .ok_or_else(|| AppError::FileError("句柄名称缓冲区长度溢出".to_string()))?;
        if name_start < buffer_start || name_end > buffer_end || !name_bytes.is_multiple_of(2) {
            return Err(AppError::FileError("句柄名称缓冲区范围无效".to_string()));
        }
        let slice = std::slice::from_raw_parts((*info).Name.Buffer, name_bytes / 2);
        return Ok(Some(String::from_utf16_lossy(slice)));
    }
    Err(AppError::FileError(
        "句柄名称缓冲区连续扩容后仍不足".to_string(),
    ))
}

/// 查询指定进程持有的 Event 句柄 (为兼容前端通信，保持函数名不变)
pub fn find_mutex_handle(
    pid: u32,
    event_name: &str, // 实际传入 "DiabloII Check For Other Instances"
) -> Result<Option<String>, AppError> {
    unsafe {
        // 获取当前系统环境下 Event 类型的索引
        let event_index = detect_event_type_index()?;

        let target_process_handle = OpenProcess(0x0040, 0, pid); // PROCESS_DUP_HANDLE
        if target_process_handle.is_null() {
            return Err(AppError::FileError(
                "无法打开目标进程以查询互斥句柄".to_string(),
            ));
        }

        let handles = match get_system_handles() {
            Ok(h) => h,
            Err(e) => {
                CloseHandle(target_process_handle);
                return Err(e);
            }
        };

        let current_process = -1isize as *mut c_void;
        let mut incomplete_reason = None;
        for entry in handles {
            // 严格过滤：PID 匹配且句柄类型必须是 Event
            if entry.UniqueProcessId == pid as usize && entry.ObjectTypeIndex == event_index {
                let mut dup_handle = std::ptr::null_mut();
                let success = DuplicateHandle(
                    target_process_handle,
                    entry.HandleValue as *mut c_void,
                    current_process,
                    &mut dup_handle,
                    0,
                    0,
                    2, // DUPLICATE_SAME_ACCESS
                );

                if success != 0 {
                    let name_result = get_object_name(dup_handle);
                    CloseHandle(dup_handle);
                    match name_result {
                        Ok(Some(name)) if name.contains(event_name) => {
                            CloseHandle(target_process_handle);
                            return Ok(Some(entry.HandleValue.to_string()));
                        }
                        Ok(_) => {}
                        Err(error) => {
                            incomplete_reason = Some(error.to_string());
                        }
                    }
                } else {
                    incomplete_reason = Some("复制目标进程 Event 句柄失败".to_string());
                }
            }
        }

        CloseHandle(target_process_handle);
        if let Some(reason) = incomplete_reason {
            return Err(AppError::FileError(format!("互斥句柄扫描不完整: {reason}")));
        }
    }
    Ok(None)
}

/// 关闭指定进程的指定句柄
pub fn close_handle(pid: u32, hid: &str) -> Result<(), AppError> {
    let source_handle_val = hid
        .parse::<usize>()
        .map_err(|e| AppError::FileError(format!("解析句柄值失败: {}", e)))?;

    unsafe {
        let target_process_handle = OpenProcess(0x0040, 0, pid); // PROCESS_DUP_HANDLE
        if target_process_handle.is_null() {
            return Err(AppError::FileError("无法打开目标进程".to_string()));
        }

        let success = DuplicateHandle(
            target_process_handle,
            source_handle_val as *mut c_void,
            std::ptr::null_mut(),
            &mut std::ptr::null_mut(),
            0,
            0,
            1, // DUPLICATE_CLOSE_SOURCE
        );

        CloseHandle(target_process_handle);

        if success == 0 {
            return Err(AppError::FileError("关闭远程句柄失败".to_string()));
        }
    }
    Ok(())
}

// ── 网络连接监测 ──

#[repr(C)]
#[allow(non_snake_case)]
struct MIB_TCPROW_OWNER_PID {
    dwState: u32,
    dwLocalAddr: u32,
    dwLocalPort: u32,
    dwRemoteAddr: u32,
    dwRemotePort: u32,
    dwOwningPid: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct MIB_TCP6ROW_OWNER_PID {
    ucLocalAddr: [u8; 16],
    dwLocalScopeId: u32,
    dwLocalPort: u32,
    ucRemoteAddr: [u8; 16],
    dwRemoteScopeId: u32,
    dwRemotePort: u32,
    dwState: u32,
    dwOwningPid: u32,
}

extern "system" {
    fn GetExtendedTcpTable(
        pTcpTable: *mut c_void,
        pdwSize: *mut u32,
        bOrder: i32,
        ulAf: u32,
        TableClass: u32,
        Reserved: u32,
    ) -> u32;
}

const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;
const TCP_TABLE_OWNER_PID_ALL: u32 = 5;
const MIB_TCP_STATE_ESTAB: u32 = 5;
const BATTLE_NET_LOBBY_PORT: u16 = 1119;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

fn tcp_connection_indicates_online_readiness(
    target_pid: u32,
    owning_pid: u32,
    state: u32,
    remote_port: u16,
) -> bool {
    owning_pid == target_pid && state == MIB_TCP_STATE_ESTAB && remote_port == BATTLE_NET_LOBBY_PORT
}

struct TcpTableSnapshot {
    words: Vec<u32>,
    byte_len: usize,
}

fn query_tcp_table(address_family: u32) -> Option<TcpTableSnapshot> {
    unsafe {
        let mut required_size = 0u32;
        let initial_result = GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut required_size,
            0,
            address_family,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
        if initial_result != 0 && initial_result != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }

        // TCP 表可能在两次 API 调用之间增长；按 API 返回的新尺寸有界重试。
        for _ in 0..3 {
            if required_size < std::mem::size_of::<u32>() as u32 {
                return None;
            }

            // Vec<u32> 为 C 结构体解析提供所需的 4 字节对齐。
            let word_count = (required_size as usize).div_ceil(std::mem::size_of::<u32>());
            let mut words = vec![0u32; word_count];
            let capacity_bytes = words.len() * std::mem::size_of::<u32>();
            let mut returned_size = capacity_bytes as u32;
            let result = GetExtendedTcpTable(
                words.as_mut_ptr() as *mut c_void,
                &mut returned_size,
                0,
                address_family,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if result == 0 {
                return Some(TcpTableSnapshot {
                    words,
                    byte_len: (returned_size as usize).min(capacity_bytes),
                });
            }
            if result != ERROR_INSUFFICIENT_BUFFER {
                return None;
            }
            required_size = returned_size;
        }
    }
    None
}

fn tcp_table_indicates_online_readiness<Row>(
    snapshot: &TcpTableSnapshot,
    target_pid: u32,
    row_fields: impl Fn(&Row) -> (u32, u32, u16),
) -> bool {
    let header_size = std::mem::size_of::<u32>();
    if snapshot.byte_len < header_size || std::mem::align_of::<Row>() > std::mem::align_of::<u32>()
    {
        return false;
    }

    let reported_entries = snapshot.words[0] as usize;
    let max_entries = (snapshot.byte_len - header_size) / std::mem::size_of::<Row>();
    let actual_entries = reported_entries.min(max_entries);
    let rows_ptr = unsafe { (snapshot.words.as_ptr() as *const u8).add(header_size) as *const Row };

    for index in 0..actual_entries {
        let row = unsafe { &*rows_ptr.add(index) };
        let (owning_pid, state, remote_port) = row_fields(row);
        if tcp_connection_indicates_online_readiness(target_pid, owning_pid, state, remote_port) {
            return true;
        }
    }
    false
}

fn tcp4_indicates_online_readiness(pid: u32) -> bool {
    query_tcp_table(AF_INET).is_some_and(|snapshot| {
        tcp_table_indicates_online_readiness(&snapshot, pid, |row: &MIB_TCPROW_OWNER_PID| {
            (
                row.dwOwningPid,
                row.dwState,
                u16::from_be((row.dwRemotePort & 0xFFFF) as u16),
            )
        })
    })
}

fn tcp6_indicates_online_readiness(pid: u32) -> bool {
    query_tcp_table(AF_INET6).is_some_and(|snapshot| {
        tcp_table_indicates_online_readiness(&snapshot, pid, |row: &MIB_TCP6ROW_OWNER_PID| {
            (
                row.dwOwningPid,
                row.dwState,
                u16::from_be((row.dwRemotePort & 0xFFFF) as u16),
            )
        })
    })
}

/// 检查目标进程是否已建立 TCP 1119 连接（Battle.net 游戏大厅）。
/// IPv4 和 IPv6 都会检查，但不再将目标进程的其他 TCP 连接视为大厅就绪。
pub fn check_game_connected(pid: u32) -> bool {
    tcp4_indicates_online_readiness(pid) || tcp6_indicates_online_readiness(pid)
}

#[cfg(test)]
mod network_readiness_tests {
    use super::tcp_connection_indicates_online_readiness;

    #[test]
    fn established_game_connection_requires_battle_net_lobby_port() {
        assert!(tcp_connection_indicates_online_readiness(42, 42, 5, 1119));
        assert!(!tcp_connection_indicates_online_readiness(42, 42, 5, 443));
        assert!(!tcp_connection_indicates_online_readiness(
            42, 42, 5, 52_123
        ));
    }

    #[test]
    fn readiness_rejects_other_processes_and_non_established_connections() {
        assert!(!tcp_connection_indicates_online_readiness(42, 7, 5, 1119));
        assert!(!tcp_connection_indicates_online_readiness(42, 42, 2, 1119));
    }
}

pub(crate) fn executable_paths_match(actual: &std::path::Path, expected: &std::path::Path) -> bool {
    paths_have_same_identity(actual, expected)
}

fn count_bnet_in_for_path(sys: &System, battle_net_path: &str) -> usize {
    let expected = std::path::Path::new(battle_net_path);
    sys.processes()
        .values()
        .filter(|process| {
            process
                .name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Battle.net.exe")
                && process
                    .exe()
                    .is_some_and(|actual| executable_paths_match(actual, expected))
        })
        .count()
}

pub fn count_bnet_processes_for_path(battle_net_path: &str) -> usize {
    let mut sys = shared_system().lock().unwrap_or_else(|e| e.into_inner());
    sys.refresh_processes(ProcessesToUpdate::All);
    count_bnet_in_for_path(&sys, battle_net_path)
}

// ── Windows API 按键模拟与窗口控制（纯 Rust，零外部进程）──

const VK_SPACE: usize = 0x20;
const VK_RETURN: usize = 0x0D;

/// 纯 Rust 将窗口前台激活并置顶
/// 使用 AttachThreadInput 绕过 Windows 前台窗口权限限制
#[cfg(target_os = "windows")]
pub fn bring_window_to_foreground_raw(hwnd: isize) {
    extern "system" {
        fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
        fn SetForegroundWindow(hWnd: isize) -> i32;
        fn SetActiveWindow(hWnd: isize) -> isize;
        fn BringWindowToTop(hWnd: isize) -> i32;
        fn IsIconic(hWnd: isize) -> i32;
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
        fn GetCurrentThreadId() -> u32;
        fn AttachThreadInput(idAttach: u32, idAttachTo: u32, fAttach: i32) -> i32;
        fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
    }
    const SW_SHOW: i32 = 5;
    const SW_RESTORE: i32 = 9;
    const HWND_TOP: isize = 0;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_SHOWWINDOW: u32 = 0x0040;

    unsafe {
        let current_thread_id = GetCurrentThreadId();
        let target_thread_id = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        let foreground_hwnd = GetForegroundWindow();
        let foreground_thread_id = if foreground_hwnd != 0 {
            GetWindowThreadProcessId(foreground_hwnd, std::ptr::null_mut())
        } else {
            0
        };

        // Attach 到前台线程和目标线程以获取 SetForegroundWindow 权限
        if foreground_thread_id != 0 && foreground_thread_id != current_thread_id {
            AttachThreadInput(current_thread_id, foreground_thread_id, 1);
        }
        if target_thread_id != current_thread_id && target_thread_id != foreground_thread_id {
            AttachThreadInput(current_thread_id, target_thread_id, 1);
        }

        // 最小化则恢复，否则显示
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        } else {
            ShowWindow(hwnd, SW_SHOW);
        }

        // 置顶 Z 序
        BringWindowToTop(hwnd);
        SetWindowPos(
            hwnd,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );

        // 激活前台
        SetForegroundWindow(hwnd);
        SetActiveWindow(hwnd);

        // 分离线程
        if foreground_thread_id != 0 && foreground_thread_id != current_thread_id {
            AttachThreadInput(current_thread_id, foreground_thread_id, 0);
        }
        if target_thread_id != current_thread_id && target_thread_id != foreground_thread_id {
            AttachThreadInput(current_thread_id, target_thread_id, 0);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn bring_window_to_foreground_raw(_hwnd: isize) {}

/// 根据窗口标题查找可见窗口并置顶（核心逻辑，可被其他模块调用）
/// 通过 rename_game_window 已将游戏窗口标题改为账号昵称
pub fn bring_window_by_title_to_front_logic(window_title: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        if window_title.is_empty() {
            return false;
        }

        extern "system" {
            fn EnumWindows(
                lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
                lparam: isize,
            ) -> i32;
            fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
            fn IsWindowVisible(hWnd: isize) -> i32;
        }

        struct Ctx {
            title: String,
            found_hwnd: isize,
            diablo_hwnd: isize,
        }

        unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
            let ctx = &mut *(lparam as *mut Ctx);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
            if len > 0 {
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                if title.to_lowercase().contains(&ctx.title.to_lowercase()) {
                    ctx.found_hwnd = hwnd;
                    return 0; // 精确/模糊匹配成功，停止
                }
                if title.contains("Diablo II: Resurrected") {
                    ctx.diablo_hwnd = hwnd; // 兜底记录
                }
            }
            1
        }

        let mut ctx = Ctx {
            title: window_title.to_string(),
            found_hwnd: 0,
            diablo_hwnd: 0,
        };

        unsafe {
            EnumWindows(callback, &mut ctx as *mut Ctx as isize);
        }

        let hwnd = if ctx.found_hwnd != 0 {
            ctx.found_hwnd
        } else if ctx.diablo_hwnd != 0 {
            ctx.diablo_hwnd
        } else {
            0
        };

        if hwnd != 0 {
            bring_window_to_foreground_raw(hwnd);
            return true;
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window_title;
    false
}

/// 根据窗口标题查找可见窗口并置顶（Tauri 命令封装）
pub fn bring_window_by_title_to_front(window_title: &str) -> bool {
    bring_window_by_title_to_front_logic(window_title)
}

/// 获取当前前台（焦点）窗口的标题
pub fn get_foreground_window_title() -> String {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn GetForegroundWindow() -> isize;
            fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
        }
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd != 0 {
                let mut buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
                if len > 0 {
                    return String::from_utf16_lossy(&buf[..len as usize]);
                }
            }
        }
    }
    String::new()
}

/// 枚举所有 D2R 可见窗口（通过进程名而非标题匹配），返回窗口标题列表
/// 用于启动时检测已运行的 D2R 窗口，更新悬浮窗状态
pub fn get_d2r_window_titles() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn EnumWindows(
                lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
                lparam: isize,
            ) -> i32;
            fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
            fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
            fn IsWindowVisible(hWnd: isize) -> i32;
        }

        let d2r_pids_vec = get_d2r_pids();
        if d2r_pids_vec.is_empty() {
            return Vec::new();
        }
        let d2r_pids: std::collections::HashSet<u32> = d2r_pids_vec.into_iter().collect();

        struct Ctx {
            titles: Vec<String>,
            d2r_pids: *const std::collections::HashSet<u32>,
        }

        unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
            let ctx = &mut *(lparam as *mut Ctx);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 || !(*ctx.d2r_pids).contains(&pid) {
                return 1;
            }
            let mut buf = [0u16; 260];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 260);
            if len > 0 {
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                ctx.titles.push(title);
            }
            1
        }

        let mut ctx = Ctx {
            titles: Vec::new(),
            d2r_pids: &d2r_pids as *const std::collections::HashSet<u32>,
        };
        unsafe {
            EnumWindows(callback, &mut ctx as *mut Ctx as isize);
        }
        ctx.titles
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AccountGameIdentity {
    account_id: String,
    window_title: String,
    game_executable: std::path::PathBuf,
}

impl AccountGameIdentity {
    pub(crate) fn new(
        account_id: String,
        window_title: String,
        game_executable: std::path::PathBuf,
    ) -> Self {
        Self {
            account_id,
            window_title,
            game_executable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunningGameWindow {
    pid: u32,
    window_title: String,
    game_executable: std::path::PathBuf,
}

fn recover_running_assignments(
    accounts: &[AccountGameIdentity],
    windows: &[RunningGameWindow],
    already_claimed: &std::collections::HashSet<u32>,
) -> Vec<(String, u32)> {
    let mut claimed = already_claimed.clone();
    let mut assignments = Vec::new();

    for account in accounts {
        let identity_count = accounts
            .iter()
            .filter(|candidate| {
                candidate
                    .window_title
                    .eq_ignore_ascii_case(&account.window_title)
                    && executable_paths_match(&candidate.game_executable, &account.game_executable)
            })
            .count();
        if identity_count != 1 {
            continue;
        }

        let candidates: std::collections::HashSet<u32> = windows
            .iter()
            .filter(|window| {
                !claimed.contains(&window.pid)
                    && window
                        .window_title
                        .eq_ignore_ascii_case(&account.window_title)
                    && executable_paths_match(&window.game_executable, &account.game_executable)
            })
            .map(|window| window.pid)
            .collect();
        if candidates.len() == 1 {
            let pid = *candidates.iter().next().expect("single candidate");
            claimed.insert(pid);
            assignments.push((account.account_id.clone(), pid));
        }
    }
    assignments
}

/// 扫描已运行的 D2R 窗口，匹配账号列表中的昵称，更新多开实例注册表。
/// 解决"账号已在游戏但工具不识别为活动账号"的问题
/// 恢复匹配同时要求完整 exe 路径和精确窗口标题，且一个 PID 最多归属一个账号。
pub fn refresh_account_running_state<F>(
    state: tauri::State<'_, crate::state::SharedState>,
    load_account_identities: F,
) -> Result<Vec<String>, String>
where
    F: FnOnce(&crate::domain::config::GlobalConfig) -> Vec<AccountGameIdentity>,
{
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn EnumWindows(
                lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
                lparam: isize,
            ) -> i32;
            fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
            fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
            fn IsWindowVisible(hWnd: isize) -> i32;
        }

        // 扫描可能跨越多个 Windows API 调用。先记住注册表代次，提交时
        // 若有新启动/清理已更新状态，则旧扫描必须让位。
        let registry = state.multi_instance().instances();
        let registry_snapshot = registry.snapshot();

        // 1. 只收集能够读取完整可执行文件路径的 D2R 进程。
        let d2r_processes: std::collections::HashMap<u32, std::path::PathBuf> = {
            let mut system = shared_system()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            system.refresh_processes(ProcessesToUpdate::All);
            system
                .processes()
                .iter()
                .filter_map(|(pid, process)| {
                    if process
                        .name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("D2R.exe")
                    {
                        process.exe().map(|path| (pid.as_u32(), path.to_path_buf()))
                    } else {
                        None
                    }
                })
                .collect()
        };
        if d2r_processes.is_empty() {
            let final_instances = registry
                .reconcile_if_unchanged(&registry_snapshot, std::iter::empty::<(String, u32)>())
                .unwrap_or_else(|| state.multi_instance().facade().running_instances());
            return Ok(final_instances
                .into_iter()
                .map(|instance| instance.account_id)
                .collect());
        }
        let d2r_pids: std::collections::HashSet<u32> = d2r_processes.keys().copied().collect();

        // 2. 枚举可见窗口，仅保留属于 D2R 进程的窗口 (标题, PID)
        struct WinInfo {
            title: String,
            pid: u32,
        }
        struct Ctx {
            wins: Vec<WinInfo>,
            d2r_pids: *const std::collections::HashSet<u32>,
        }

        unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
            let ctx = &mut *(lparam as *mut Ctx);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 || !(*ctx.d2r_pids).contains(&pid) {
                return 1;
            }
            let mut buf = [0u16; 260];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 260);
            if len > 0 {
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                ctx.wins.push(WinInfo { title, pid });
            }
            1
        }

        let mut ctx = Ctx {
            wins: Vec::new(),
            d2r_pids: &d2r_pids as *const std::collections::HashSet<u32>,
        };
        unsafe {
            EnumWindows(callback, &mut ctx as *mut Ctx as isize);
        }

        // 3. 为每个账号解析完整的 Edition-specific D2R.exe 身份。
        let config = state.configuration().snapshot();
        let cfg = config
            .as_ref()
            .ok_or_else(|| "尚未完成首次配置".to_string())?;
        let account_identities = load_account_identities(cfg);
        drop(config);

        let running_windows: Vec<RunningGameWindow> = ctx
            .wins
            .into_iter()
            .filter_map(|window| {
                d2r_processes
                    .get(&window.pid)
                    .cloned()
                    .map(|game_executable| RunningGameWindow {
                        pid: window.pid,
                        window_title: window.title,
                        game_executable,
                    })
            })
            .collect();

        // 4. 保留仍匹配精确窗口标题和 Edition 可执行文件的映射，并去除 PID 重复认领。
        let previous = registry_snapshot
            .instances()
            .iter()
            .map(|instance| (instance.account_id.clone(), instance.pid))
            .collect::<std::collections::HashMap<_, _>>();
        let mut active = std::collections::HashMap::new();
        let mut claimed = std::collections::HashSet::new();
        for identity in &account_identities {
            if let Some(pid) = previous.get(&identity.account_id) {
                let identity_still_running = running_windows.iter().any(|window| {
                    window.pid == *pid
                        && window
                            .window_title
                            .eq_ignore_ascii_case(&identity.window_title)
                        && executable_paths_match(
                            &window.game_executable,
                            &identity.game_executable,
                        )
                });
                if identity_still_running && claimed.insert(*pid) {
                    active.insert(identity.account_id.clone(), *pid);
                }
            }
        }

        // 5. 仅对未认领 PID 做无歧义恢复；子串、重复昵称和错误 Edition 均 fail closed。
        for (account_id, pid) in
            recover_running_assignments(&account_identities, &running_windows, &claimed)
        {
            if claimed.insert(pid) {
                active.insert(account_id, pid);
            }
        }

        let final_instances = registry
            .reconcile_if_unchanged(&registry_snapshot, active)
            .unwrap_or_else(|| state.multi_instance().facade().running_instances());
        let final_account_ids = final_instances
            .into_iter()
            .map(|instance| instance.account_id.to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>();
        let running_accounts = account_identities
            .iter()
            .filter(|identity| {
                final_account_ids.contains(&identity.account_id.to_ascii_lowercase())
            })
            .map(|identity| identity.account_id.clone())
            .collect::<Vec<_>>();
        Ok(running_accounts)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (state, load_account_identities);
        Ok(Vec::new())
    }
}

/// 收集所有可见的 Chrome/Edge 窗口句柄
#[cfg(target_os = "windows")]
pub fn collect_chrome_windows() -> std::collections::HashSet<isize> {
    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetClassNameW(hWnd: isize, lpClassName: *mut u16, nMaxCount: i32) -> i32;
        fn IsWindowVisible(hWnd: isize) -> i32;
    }

    thread_local! {
        static HWND_SET: std::cell::RefCell<std::collections::HashSet<isize>> = std::cell::RefCell::new(std::collections::HashSet::new());
    }

    HWND_SET.with(|set| set.borrow_mut().clear());

    unsafe extern "system" fn enum_callback(hwnd: isize, _lparam: isize) -> i32 {
        if IsWindowVisible(hwnd) != 0 {
            let mut class_name = [0u16; 256];
            let class_len = GetClassNameW(hwnd, class_name.as_mut_ptr(), 256);
            if class_len > 0 {
                let class_str = String::from_utf16_lossy(&class_name[..class_len as usize]);
                if class_str == "Chrome_WidgetWin_1" {
                    HWND_SET.with(|set| {
                        set.borrow_mut().insert(hwnd);
                    });
                }
            }
        }
        1
    }

    unsafe {
        EnumWindows(enum_callback, 0);
    }

    HWND_SET.with(|set| set.borrow().clone())
}

#[cfg(not(target_os = "windows"))]
pub fn collect_chrome_windows() -> std::collections::HashSet<isize> {
    std::collections::HashSet::new()
}

/// 监测新拉起的空白浏览器窗口，并将其置顶
#[cfg(target_os = "windows")]
pub fn bring_browser_login_to_foreground(before_hwnds: std::collections::HashSet<isize>) {
    std::thread::spawn(move || {
        for _ in 0..12 {
            // 最多等待 3 秒 (12 * 250ms)
            std::thread::sleep(std::time::Duration::from_millis(250));
            let current = collect_chrome_windows();
            for hwnd in current {
                if !before_hwnds.contains(&hwnd) {
                    bring_window_to_foreground_raw(hwnd);
                    return;
                }
            }
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub fn bring_browser_login_to_foreground(_before_hwnds: std::collections::HashSet<isize>) {}

/// 监测新拉起的战网窗口，并将其置顶（Tauri 暴露指令）
pub fn bring_bnet_to_foreground() {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(|| {
            for _ in 0..20 {
                // 最多等待 5 秒 (20 * 250ms)
                std::thread::sleep(std::time::Duration::from_millis(250));
                if let Some(hwnd) = find_bnet_window() {
                    bring_window_to_foreground_raw(hwnd);
                    break;
                }
            }
        });
    }
}

/// 将 D2RHub 主窗口提到前台
pub fn bring_self_to_foreground(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "windows")]
fn find_bnet_window() -> Option<isize> {
    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
        fn IsWindowVisible(hWnd: isize) -> i32;
    }

    use sysinfo::ProcessesToUpdate;
    let mut sys = shared_system().lock().unwrap_or_else(|e| e.into_inner());
    sys.refresh_processes(ProcessesToUpdate::All);
    let bnet_pids: std::collections::HashSet<u32> = sys
        .processes()
        .values()
        .filter(|p| p.name().to_string_lossy() == "Battle.net.exe")
        .map(|p| p.pid().as_u32())
        .collect();

    if bnet_pids.is_empty() {
        return None;
    }

    static MATCHED_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    MATCHED_HWND.store(0, std::sync::atomic::Ordering::Relaxed);

    thread_local! {
        static BNET_PIDS_TL: std::cell::RefCell<std::collections::HashSet<u32>> = std::cell::RefCell::new(std::collections::HashSet::new());
    }

    BNET_PIDS_TL.with(|tl| {
        *tl.borrow_mut() = bnet_pids;
    });

    unsafe extern "system" fn enum_callback(hwnd: isize, _lparam: isize) -> i32 {
        if IsWindowVisible(hwnd) != 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != 0 {
                let is_bnet = BNET_PIDS_TL.with(|tl| tl.borrow().contains(&pid));
                if is_bnet {
                    MATCHED_HWND.store(hwnd, std::sync::atomic::Ordering::Relaxed);
                    return 0; // stop enum
                }
            }
        }
        1
    }

    unsafe {
        EnumWindows(enum_callback, 0);
    }

    let hwnd = MATCHED_HWND.load(std::sync::atomic::Ordering::Relaxed);
    if hwnd != 0 {
        Some(hwnd)
    } else {
        None
    }
}

/// 纯 Rust 发送按键：空格 + 回车（静默后台投递，无 PowerShell，不抢占键盘焦点）
pub fn send_keys_to_window(pid: u32) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        // 该函数在启动阶段每 500ms 调用一次。窗口存在本身就足以证明目标
        // 进程仍可接收消息，无需每次都刷新完整系统进程表。
        if let Some(hwnd) = find_game_hwnd(pid) {
            extern "system" {
                fn PostMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> i32;
            }
            const WM_KEYDOWN: u32 = 0x0100;
            const WM_KEYUP: u32 = 0x0101;

            unsafe {
                // Post Space Key
                PostMessageW(hwnd, WM_KEYDOWN, VK_SPACE, 0);
                std::thread::sleep(std::time::Duration::from_millis(30));
                PostMessageW(hwnd, WM_KEYUP, VK_SPACE, 0xC0000001);

                std::thread::sleep(std::time::Duration::from_millis(100));

                // Post Enter Key
                PostMessageW(hwnd, WM_KEYDOWN, VK_RETURN, 0);
                std::thread::sleep(std::time::Duration::from_millis(30));
                PostMessageW(hwnd, WM_KEYUP, VK_RETURN, 0xC0000001);
            }
        }
    }

    Ok(())
}

// ── 窗口前台焦点 ──

// ── 权限检查 ──

/// 检查是否以管理员权限运行
pub fn is_admin() -> bool {
    #[cfg(target_os = "windows")]
    {
        // 尝试读取一个需要管理员权限的注册表路径
        let output = silent_cmd("net").args(["session"]).output();
        output.map(|o| o.status.success()).unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false // 非 Windows 平台暂不支持
    }
}

/// 退出程序
static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn exit_app(app: tauri::AppHandle) {
    if EXIT_REQUESTED.swap(true, Ordering::AcqRel) {
        return;
    }

    crate::logger::log_msg("INFO", "System", "已请求退出，后台回收运行服务");
    let shutdown_app = app.clone();
    match std::thread::Builder::new()
        .name("application-shutdown".to_string())
        .spawn(move || {
            // Capability shutdown may wait for workers that themselves marshal
            // window work back to Tauri's event loop. Never perform that wait
            // inside an IPC or tray event callback: keeping the event loop free
            // is what lets those workers finish.
            crate::capabilities::shutdown(&shutdown_app);
            shutdown_app.exit(0);
        }) {
        Ok(_) => {}
        Err(error) => {
            crate::logger::log_msg(
                "ERROR",
                "System",
                &format!("无法启动后台退出线程，将直接退出: {error}"),
            );
            app.exit(0);
        }
    }
}

/// 打开日志文件夹目录
pub fn open_logs_dir() -> Result<(), crate::error::AppError> {
    if let Some(dir) = crate::logger::get_logs_dir() {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg(&dir)
                .spawn()
                .map_err(|e| {
                    crate::error::AppError::FileError(format!("打开日志目录失败: {}", e))
                })?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::process::Command::new("open")
                .arg(&dir)
                .spawn()
                .map_err(|e| {
                    crate::error::AppError::FileError(format!("打开日志目录失败: {}", e))
                })?;
        }
        Ok(())
    } else {
        Err(crate::error::AppError::FileError(
            "日志目录尚未初始化".to_string(),
        ))
    }
}

/// 在默认浏览器中打开用户帮助文档 (docs/user-guide.html)
pub fn open_user_guide(app: tauri::AppHandle) -> Result<(), crate::error::AppError> {
    // 优先从资源目录查找（生产模式）
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| crate::error::AppError::FileError(format!("获取资源目录失败: {}", e)))?;
    let mut guide_path = resource_dir.join("docs").join("user-guide.html");

    // NSIS 安装包回退：资源可能被包裹在 _up_ 子目录中
    if !guide_path.exists() {
        let nsis_path = resource_dir
            .join("_up_")
            .join("docs")
            .join("user-guide.html");
        if nsis_path.exists() {
            guide_path = nsis_path;
        }
    }

    // 开发模式回退：从项目根目录 docs/ 查找
    if !guide_path.exists() {
        let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("docs")
            .join("user-guide.html");
        if dev_path.exists() {
            guide_path = dev_path;
        }
    }

    if !guide_path.exists() {
        return Err(crate::error::AppError::FileError(format!(
            "帮助文档不存在: {}",
            guide_path.display()
        )));
    }

    #[cfg(target_os = "windows")]
    {
        silent_cmd("cmd")
            .args(["/C", "start", "", &guide_path.to_string_lossy()])
            .spawn()
            .map_err(|e| crate::error::AppError::FileError(format!("打开帮助文档失败: {}", e)))?;
    }

    Ok(())
}

pub fn get_app_version() -> String {
    env!("APP_VERSION").to_string()
}

pub async fn install_update(_app: tauri::AppHandle, _url: String) -> Result<(), String> {
    Err(
        "应用内直接替换可执行文件的更新方式已禁用。请从 GitHub Releases 下载完整安装包后手动安装。"
            .to_string(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudVersionInfo {
    pub version: String,
    pub download_url: String,
}

pub fn check_cloud_version() -> Result<CloudVersionInfo, String> {
    let url = "https://api.github.com/repos/gjy991229/D2RHub/releases/latest";
    let output = silent_cmd("curl")
        .args([
            "-sL",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "-H",
            "User-Agent: D2RHub-Updater",
            url,
        ])
        .output();

    let stdout = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => {
            // Fallback to powershell
            let ps_res = silent_cmd("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(format!(
                    "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; (Invoke-WebRequest -UserAgent 'D2RHub-Updater' -Uri '{}' -UseBasicParsing -TimeoutSec 15).Content",
                    url
                ))
                .output();
            match ps_res {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
                Ok(out) => {
                    return Err(format!(
                        "获取失败 (PowerShell): {}",
                        String::from_utf8_lossy(&out.stderr)
                    ))
                }
                Err(e) => return Err(format!("获取失败 (curl & PowerShell): {}", e)),
            }
        }
    };

    // Parse JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("解析 JSON 失败: {}, 响应内容: {}", e, stdout))?;

    let tag_name = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "未找到 tag_name 字段".to_string())?;

    // D2RHub only ships one complete build; prefer the NSIS installer over MSI.
    let mut msi_url = None;
    let mut nsis_url = None;
    if let Some(assets) = json.get("assets").and_then(|a| a.as_array()) {
        for asset in assets {
            if let Some(name) = asset.get("name").and_then(|n| n.as_str()) {
                let lower = name.to_lowercase();
                let url = asset.get("browser_download_url").and_then(|u| u.as_str());
                if lower.ends_with(".msi") && msi_url.is_none() {
                    msi_url = url.map(|u| u.to_string());
                }
                if lower.ends_with(".exe")
                    && (lower.contains("setup")
                        || lower.contains("installer")
                        || lower.contains("nsis"))
                    && nsis_url.is_none()
                {
                    nsis_url = url.map(|u| u.to_string());
                }
            }
        }
    }

    let download_url = nsis_url.or(msi_url).ok_or_else(|| {
        "未在 Release 中找到安装包资产，预期 NSIS .exe 或 .msi 安装器".to_string()
    })?;

    Ok(CloudVersionInfo {
        version: tag_name.trim_start_matches('v').to_string(),
        download_url,
    })
}

pub fn check_path_exists(path: String, is_file: bool) -> bool {
    if path.is_empty() {
        return false;
    }
    let p = std::path::Path::new(&path);
    if is_file {
        p.is_file()
    } else {
        p.is_dir()
    }
}

/// 根据 PID 查找窗口并将其标题改为指定名称（用于区分多开账号）
#[cfg(target_os = "windows")]
pub fn rename_game_window(pid: u32, new_title: &str) {
    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
        fn IsWindowVisible(hWnd: isize) -> i32;
        fn SetWindowTextW(hWnd: isize, lpString: *const u16) -> i32;
        fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
    }

    struct Ctx {
        target_pid: u32,
        title_wide: Vec<u16>,
    }

    unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
        let ctx = &mut *(lparam as *mut Ctx);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != ctx.target_pid || IsWindowVisible(hwnd) == 0 {
            return 1; // 继续枚举
        }
        // 只修改主游戏窗口（类名以 "Diablo" 开头或标题包含 "Diablo"）
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
        if len > 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            if title.to_lowercase().contains("diablo") {
                SetWindowTextW(hwnd, ctx.title_wide.as_ptr());
                return 0; // 找到并改名后停止
            }
        }
        1
    }

    let title_wide: Vec<u16> = format!("{}\0", new_title).encode_utf16().collect();
    let mut ctx = Ctx {
        target_pid: pid,
        title_wide,
    };
    unsafe {
        EnumWindows(callback, &mut ctx as *mut Ctx as isize);
    }
}

/// 根据 PID 查找游戏窗口并移动位置（用于多开窗口排列）
#[cfg(target_os = "windows")]
#[allow(dead_code)] // compatibility delegate used by unfinished feature branches
pub fn set_game_window_position(pid: u32, x: i32, y: i32) {
    if let Some(hwnd) = find_game_hwnd(pid) {
        move_window_handle(hwnd, x, y);
    }
}

#[cfg(target_os = "windows")]
fn move_window_handle(hwnd: isize, x: i32, y: i32) -> bool {
    extern "system" {
        fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
    }
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;

    unsafe { SetWindowPos(hwnd, 0, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER) != 0 }
}

/// 根据窗口标题查找并移动位置（用于多选或 PID 缺失时的降级查找）
#[cfg(target_os = "windows")]
pub fn set_game_window_position_by_title(title: &str, x: i32, y: i32) -> bool {
    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hWnd: isize) -> i32;
        fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
        fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
    }
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;

    struct FindCtx {
        title: String,
        found_hwnd: Option<isize>,
    }

    unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
        let ctx = &mut *(lparam as *mut FindCtx);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
        if len > 0 {
            let t = String::from_utf16_lossy(&buf[..len as usize]);
            if t == ctx.title {
                ctx.found_hwnd = Some(hwnd);
                return 0; // 停止
            }
        }
        1
    }

    let mut ctx = FindCtx {
        title: title.to_string(),
        found_hwnd: None,
    };
    unsafe {
        EnumWindows(callback, &mut ctx as *mut FindCtx as isize);
    }

    if let Some(hwnd) = ctx.found_hwnd {
        unsafe {
            SetWindowPos(hwnd, 0, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
        }
        true
    } else {
        false
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_game_window_position_by_title(_title: &str, _x: i32, _y: i32) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)] // compatibility delegate used by unfinished feature branches
pub fn set_game_window_position(_pid: u32, _x: i32, _y: i32) {}

#[cfg(not(target_os = "windows"))]
fn move_window_handle(_hwnd: isize, _x: i32, _y: i32) -> bool {
    false
}

/// 根据 PID 查找 D2R 游戏窗口句柄（公用基础设施）
#[cfg(target_os = "windows")]
pub fn find_game_hwnd(pid: u32) -> Option<isize> {
    if pid == 0 {
        return None;
    }
    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
        fn IsWindowVisible(hWnd: isize) -> i32;
    }

    struct Ctx {
        target_pid: u32,
        found_hwnd: isize,
    }

    unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
        let ctx = &mut *(lparam as *mut Ctx);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }

        let mut p = 0u32;
        GetWindowThreadProcessId(hwnd, &mut p);
        if p == ctx.target_pid {
            ctx.found_hwnd = hwnd;
            return 0; // D2R进程只有一个可见主窗口，直接返回
        }
        1
    }

    let mut ctx = Ctx {
        target_pid: pid,
        found_hwnd: 0,
    };
    unsafe {
        EnumWindows(callback, &mut ctx as *mut Ctx as isize);
    }
    if ctx.found_hwnd != 0 {
        Some(ctx.found_hwnd)
    } else {
        None
    }
}

/// 无 PID 时按窗口标题精确匹配查找（用于 D2RHub 重启后 PID 丢失的场景）
#[cfg(target_os = "windows")]
pub fn find_game_hwnd_by_title(title: &str) -> Option<isize> {
    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hWnd: isize) -> i32;
        fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
    }

    struct Ctx {
        target_title: String,
        found_hwnd: isize,
        found_diablo_hwnd: isize,
    }

    unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
        let ctx = &mut *(lparam as *mut Ctx);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
        if len > 0 {
            let wt = String::from_utf16_lossy(&buf[..len as usize]);
            if wt == ctx.target_title {
                ctx.found_hwnd = hwnd;
                return 0; // 精确匹配到账号昵称，直接返回
            }
            if wt.contains("Diablo II: Resurrected") {
                ctx.found_diablo_hwnd = hwnd; // 兜底：记录游戏原名窗口
            }
        }
        1
    }

    let mut ctx = Ctx {
        target_title: title.to_string(),
        found_hwnd: 0,
        found_diablo_hwnd: 0,
    };
    unsafe {
        EnumWindows(callback, &mut ctx as *mut Ctx as isize);
    }

    if ctx.found_hwnd != 0 {
        Some(ctx.found_hwnd)
    } else if ctx.found_diablo_hwnd != 0 {
        Some(ctx.found_diablo_hwnd)
    } else {
        None
    }
}

/// 按“精确窗口标题 + 完整可执行文件路径”查找唯一的 D2R 进程。
/// 与 `find_game_hwnd_by_title` 不同，此函数没有“任意 Diablo 原名窗口”兜底；
/// 出现多个候选时也不会猜测归属，避免把其他账号或另一客户端版本错误标记为当前账号。
#[cfg(target_os = "windows")]
pub fn find_unique_d2r_pid_by_window_identity(
    title: &str,
    expected_executable: &std::path::Path,
) -> Option<u32> {
    if title.trim().is_empty() {
        return None;
    }

    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hWnd: isize) -> i32;
        fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
    }

    struct Ctx {
        target_title: String,
        candidate_pids: Vec<u32>,
    }

    unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
        let ctx = &mut *(lparam as *mut Ctx);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut buffer = [0u16; 512];
        let length = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if length <= 0 {
            return 1;
        }
        let window_title = String::from_utf16_lossy(&buffer[..length as usize]);
        if !window_title.eq_ignore_ascii_case(&ctx.target_title) {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 {
            ctx.candidate_pids.push(pid);
        }
        1
    }

    let mut ctx = Ctx {
        target_title: title.to_string(),
        candidate_pids: Vec::new(),
    };
    unsafe {
        EnumWindows(callback, &mut ctx as *mut Ctx as isize);
    }

    let mut system = shared_system()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    system.refresh_processes(ProcessesToUpdate::All);
    let mut candidates = ctx
        .candidate_pids
        .into_iter()
        .filter(|pid| {
            system
                .process(sysinfo::Pid::from(*pid as usize))
                .is_some_and(|process| {
                    process
                        .name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("D2R.exe")
                        && process.exe().is_some_and(|actual| {
                            executable_paths_match(actual, expected_executable)
                        })
                })
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.dedup();
    (candidates.len() == 1).then(|| candidates[0])
}

#[cfg(not(target_os = "windows"))]
pub fn find_unique_d2r_pid_by_window_identity(
    _title: &str,
    _expected_executable: &std::path::Path,
) -> Option<u32> {
    None
}

/// Compatibility fallback for a D2RHub restart: returns a PID only when one
/// visible window has the exact account title. The normal runtime path
/// continues to use its trusted launch snapshot and PID.
#[cfg(target_os = "windows")]
pub fn find_unique_d2r_pid_by_exact_title(title: &str) -> Option<u32> {
    if title.trim().is_empty() {
        return None;
    }

    extern "system" {
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(hwnd: isize, lparam: isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hWnd: isize) -> i32;
        fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
        fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
    }

    struct Ctx {
        target_title: String,
        candidate_pids: Vec<u32>,
    }

    unsafe extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
        let ctx = &mut *(lparam as *mut Ctx);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut buffer = [0u16; 512];
        let length = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if length <= 0 {
            return 1;
        }
        let window_title = String::from_utf16_lossy(&buffer[..length as usize]);
        if !window_title.eq_ignore_ascii_case(&ctx.target_title) {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 {
            ctx.candidate_pids.push(pid);
        }
        1
    }

    let mut ctx = Ctx {
        target_title: title.to_string(),
        candidate_pids: Vec::new(),
    };
    unsafe {
        EnumWindows(callback, &mut ctx as *mut Ctx as isize);
    }

    let mut candidates = ctx.candidate_pids.into_iter().collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.dedup();
    (candidates.len() == 1).then(|| candidates[0])
}

#[cfg(not(target_os = "windows"))]
pub fn find_unique_d2r_pid_by_exact_title(_title: &str) -> Option<u32> {
    None
}

/// 为一个已出现的游戏窗口设置账号级 AppUserModelID，使其获得独立任务栏分组。
#[cfg(target_os = "windows")]
pub fn set_game_window_app_user_model_id(pid: u32, app_id: &str) -> Result<(), String> {
    use windows::core::{GUID, PROPVARIANT};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreForWindow, PROPERTYKEY,
    };

    if app_id.trim().is_empty() || app_id.len() > 128 {
        return Err("AppUserModelID 为空或超过 128 个字符".to_string());
    }
    let hwnd = find_game_hwnd(pid).ok_or_else(|| format!("尚未找到游戏窗口 (PID: {pid})"))?;
    const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };

    unsafe {
        // Tauri/Tokio 线程的 COM 状态不固定；成功初始化时与本次调用成对释放。
        // RPC_E_CHANGED_MODE 表示线程已用另一 apartment 模式初始化，COM 本身仍可用。
        let init_status = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let initialized_here = if init_status.is_ok() {
            true
        } else if init_status.0 == 0x80010106u32 as i32 {
            false
        } else {
            return Err(format!("初始化窗口任务栏 COM 环境失败: {init_status:?}"));
        };
        let result = (|| -> windows::core::Result<()> {
            let store: IPropertyStore =
                SHGetPropertyStoreForWindow(HWND(hwnd as *mut std::ffi::c_void))?;
            let value = PROPVARIANT::from(app_id);
            // SHGetPropertyStoreForWindow 返回的窗口属性存储不要求也不支持 Commit。
            store.SetValue(&PKEY_APP_USER_MODEL_ID, &value)
        })();
        if initialized_here {
            CoUninitialize();
        }
        result.map_err(|error| format!("设置游戏窗口任务栏标识失败: {error}"))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_game_window_app_user_model_id(_pid: u32, _app_id: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn find_game_hwnd(_pid: u32) -> Option<isize> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn find_game_hwnd_by_title(_title: &str) -> Option<isize> {
    None
}

/// 获取窗口位置 (left, top, right, bottom)
#[cfg(target_os = "windows")]
pub fn get_window_rect(hwnd: isize) -> Option<(i32, i32)> {
    extern "system" {
        fn GetWindowRect(hWnd: isize, lpRect: *mut std::ffi::c_void) -> i32;
    }
    // Keep the native Windows spelling so the declaration matches Win32 documentation.
    #[allow(clippy::upper_case_acronyms)]
    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        if GetWindowRect(hwnd, (&mut rect as *mut RECT).cast()) != 0 {
            Some((rect.left, rect.top))
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_window_rect(_hwnd: isize) -> Option<(i32, i32)> {
    None
}

#[cfg(test)]
mod region_process_tests {
    use super::{
        executable_paths_match, recover_running_assignments, AccountGameIdentity, RunningGameWindow,
    };
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    #[test]
    fn battle_net_processes_are_matched_to_the_selected_installation() {
        assert!(executable_paths_match(
            Path::new(r"C:\Battle.net-CN\Battle.net.exe"),
            Path::new(r"c:\battle.net-cn\BATTLE.NET.EXE"),
        ));
        assert!(!executable_paths_match(
            Path::new(r"C:\Battle.net-CN\Battle.net.exe"),
            Path::new(r"D:\Battle.net-Global\Battle.net.exe"),
        ));
    }

    fn account(id: &str, title: &str, executable: &str) -> AccountGameIdentity {
        AccountGameIdentity {
            account_id: id.to_string(),
            window_title: title.to_string(),
            game_executable: PathBuf::from(executable),
        }
    }

    fn window(pid: u32, title: &str, executable: &str) -> RunningGameWindow {
        RunningGameWindow {
            pid,
            window_title: title.to_string(),
            game_executable: PathBuf::from(executable),
        }
    }

    #[test]
    fn running_recovery_requires_exact_title_and_executable_path() {
        let accounts = [account("acount1", "Main", r"C:\CN\D2R.exe")];
        let windows = [
            window(10, "Main extra", r"C:\CN\D2R.exe"),
            window(11, "Main", r"D:\Global\D2R.exe"),
        ];

        assert!(recover_running_assignments(&accounts, &windows, &HashSet::new()).is_empty());

        let exact = [window(12, "main", r"c:\cn\D2R.EXE")];
        assert_eq!(
            recover_running_assignments(&accounts, &exact, &HashSet::new()),
            vec![("acount1".to_string(), 12)]
        );
    }

    #[test]
    fn duplicate_account_identity_is_not_recovered_ambiguously() {
        let accounts = [
            account("acount1", "Same", r"C:\CN\D2R.exe"),
            account("acount2", "Same", r"C:\CN\D2R.exe"),
        ];
        let windows = [window(10, "Same", r"C:\CN\D2R.exe")];

        assert!(recover_running_assignments(&accounts, &windows, &HashSet::new()).is_empty());
    }

    #[test]
    fn same_title_in_different_editions_is_resolved_by_path() {
        let accounts = [
            account("acount1", "Same", r"C:\CN\D2R.exe"),
            account("acount2", "Same", r"D:\Global\D2R.exe"),
        ];
        let windows = [
            window(10, "Same", r"C:\CN\D2R.exe"),
            window(20, "Same", r"D:\Global\D2R.exe"),
        ];

        assert_eq!(
            recover_running_assignments(&accounts, &windows, &HashSet::new()),
            vec![("acount1".to_string(), 10), ("acount2".to_string(), 20)]
        );
    }
}
