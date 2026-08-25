use serde::{Deserialize, Serialize};
use sysinfo::{ProcessesToUpdate, System};
use tauri::Manager;

use crate::commands::utils::{shared_system, silent_cmd};
use crate::error::AppError;
use crate::launch_context::{paths_have_same_identity, ContextPurpose, LaunchContext};

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

// ── 进程管理 ──

/// 检测当前有哪些 D2R 进程在运行（返回 PID 列表）
#[tauri::command]
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
#[tauri::command]
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
    crate::commands::utils::kill_processes_by_name(&["Battle.net.exe", "Agent.exe"]).is_ok()
}

/// 记录进程快照（用于后续对比判断新进程）

#[tauri::command]
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
#[tauri::command]
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

unsafe fn detect_event_type_index() -> Option<u16> {
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
        return None;
    }

    let mut type_index = None;
    if let Ok(handles) = get_system_handles() {
        let current_pid = std::process::id() as usize;
        for entry in handles {
            if entry.UniqueProcessId == current_pid && entry.HandleValue == event_handle as usize {
                type_index = Some(entry.ObjectTypeIndex);
                break;
            }
        }
    }

    CloseHandle(event_handle);
    type_index
}

unsafe fn get_object_name(handle: *mut c_void) -> Option<String> {
    let size = 1024;
    let mut buffer = vec![0u8; size];
    let mut return_length = 0u32;

    let status = NtQueryObject(
        handle,
        1, // ObjectNameInformation
        buffer.as_mut_ptr() as *mut c_void,
        size as u32,
        &mut return_length,
    );

    if status == STATUS_SUCCESS {
        let info = buffer.as_ptr() as *const OBJECT_NAME_INFORMATION;
        let len = (*info).Name.Length as usize / 2;
        if len > 0 && !(*info).Name.Buffer.is_null() {
            let slice = std::slice::from_raw_parts((*info).Name.Buffer, len);
            return Some(String::from_utf16_lossy(slice));
        }
    }
    None
}

/// 查询指定进程持有的 Event 句柄 (为兼容前端通信，保持函数名不变)
#[tauri::command]
pub fn find_mutex_handle(
    pid: u32,
    event_name: &str, // 实际传入 "DiabloII Check For Other Instances"
) -> Result<Option<String>, AppError> {
    unsafe {
        // 获取当前系统环境下 Event 类型的索引
        let event_index = match detect_event_type_index() {
            Some(idx) => idx,
            None => return Ok(None),
        };

        let target_process_handle = OpenProcess(0x0040, 0, pid); // PROCESS_DUP_HANDLE
        if target_process_handle.is_null() {
            return Ok(None);
        }

        let handles = match get_system_handles() {
            Ok(h) => h,
            Err(e) => {
                CloseHandle(target_process_handle);
                return Err(e);
            }
        };

        let current_process = -1isize as *mut c_void;
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
                    if let Some(name) = get_object_name(dup_handle) {
                        CloseHandle(dup_handle);
                        // 匹配 Event 名称
                        if name.contains(event_name) {
                            CloseHandle(target_process_handle);
                            return Ok(Some(entry.HandleValue.to_string()));
                        }
                    } else {
                        CloseHandle(dup_handle);
                    }
                }
            }
        }

        CloseHandle(target_process_handle);
    }
    Ok(None)
}

/// 关闭指定进程的指定句柄
#[tauri::command]
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
struct MIB_TCPTABLE_OWNER_PID {
    dwNumEntries: u32,
    table: [MIB_TCPROW_OWNER_PID; 1],
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

/// 检查指定进程是否已建立 TCP 1119 端口连接（连接游戏大厅）
#[tauri::command]
pub fn check_game_connected(pid: u32) -> bool {
    unsafe {
        let mut size = 0u32;
        // 第一次调用获取所需缓冲区大小
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            2, // AF_INET
            5, // TCP_TABLE_OWNER_PID_ALL
            0,
        );

        let mut buffer = vec![0u8; size as usize];
        let res = GetExtendedTcpTable(
            buffer.as_mut_ptr() as *mut c_void,
            &mut size,
            0,
            2, // AF_INET
            5, // TCP_TABLE_OWNER_PID_ALL
            0,
        );

        if res == 0 {
            let table = buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID;
            let num_entries = (*table).dwNumEntries as usize;

            // 安全边界检查，防止句柄表解析越界
            let max_possible = (size as usize - std::mem::size_of::<u32>())
                / std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
            let actual_num = num_entries.min(max_possible);

            let table_ptr = (*table).table.as_ptr();
            for i in 0..actual_num {
                let row = &*table_ptr.add(i);
                if row.dwOwningPid == pid {
                    // dwState == 5 表示 MIB_TCP_STATE_ESTAB（已建立连接）
                    if row.dwState == 5 {
                        let port = u16::from_be((row.dwRemotePort & 0xFFFF) as u16);
                        if port == 1119 {
                            let remote_ip = row.dwRemoteAddr;
                            // 排除回环地址 (127.0.0.1 字节序对应关系)
                            if remote_ip != 0 && remote_ip != 0x0100007f && remote_ip != 0x7f000001
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
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
#[tauri::command]
pub fn bring_window_by_title_to_front(window_title: &str) -> bool {
    bring_window_by_title_to_front_logic(window_title)
}

/// 获取当前前台（焦点）窗口的标题
#[tauri::command]
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
#[tauri::command]
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
struct AccountGameIdentity {
    account_id: String,
    window_title: String,
    game_executable: std::path::PathBuf,
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

/// 扫描已运行的 D2R 窗口，匹配账号列表中的昵称，更新 active_games 状态
/// 解决"账号已在游戏但工具不识别为活动账号"的问题
/// 恢复匹配同时要求完整 exe 路径和精确窗口标题，且一个 PID 最多归属一个账号。
#[tauri::command]
pub fn refresh_account_running_state(
    state: tauri::State<'_, crate::state::SharedState>,
) -> Result<Vec<String>, String> {
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
            state.active_games.write().clear();
            return Ok(Vec::new());
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
        let config = state.config.read();
        let cfg = config
            .as_ref()
            .ok_or_else(|| "尚未完成首次配置".to_string())?;
        use crate::commands::account::AccountManager;
        let mut account_identities = Vec::new();
        for account_id in AccountManager::list_ids(&cfg.accounts_dir) {
            let Ok(meta) = AccountManager::load_meta(&cfg.accounts_dir, &account_id) else {
                continue;
            };
            let Ok(context) = LaunchContext::for_account(cfg, &meta, ContextPurpose::Settings)
            else {
                continue;
            };
            let window_title = if meta.display_name.is_empty() {
                account_id.clone()
            } else {
                meta.display_name
            };
            account_identities.push(AccountGameIdentity {
                account_id,
                window_title,
                game_executable: context.installation.game_executable,
            });
        }
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
        let mut active = state.active_games.write();
        let previous = std::mem::take(&mut *active);
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

        Ok(account_identities
            .iter()
            .filter(|identity| active.contains_key(&identity.account_id))
            .map(|identity| identity.account_id.clone())
            .collect())
    }
    #[cfg(not(target_os = "windows"))]
    {
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
#[tauri::command]
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
#[tauri::command]
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
#[tauri::command]
pub fn send_keys_to_window(pid: u32) -> Result<(), AppError> {
    // 确认进程存在
    let mut sys = shared_system().lock().unwrap_or_else(|e| e.into_inner());
    sys.refresh_processes(ProcessesToUpdate::All);
    let exists = sys
        .processes()
        .values()
        .any(|p| p.pid().as_u32() == pid && p.name().to_string_lossy() == "D2R.exe");

    if !exists {
        return Ok(());
    }

    // 寻找 D2R 窗口 HWND
    #[cfg(target_os = "windows")]
    {
        ENUM_PID.store(pid, Ordering::Relaxed);
        ENUM_HWND.store(0, Ordering::Relaxed);
        unsafe {
            EnumWindows(enum_window_callback, 0);
        }
        let hwnd = ENUM_HWND.load(Ordering::Relaxed);
        if hwnd != 0 {
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

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

#[cfg(target_os = "windows")]
extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
    fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
    fn IsWindowVisible(hWnd: isize) -> i32;
}

#[cfg(target_os = "windows")]
static ENUM_PID: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "windows")]
static ENUM_HWND: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_window_callback(hwnd: isize, _lparam: isize) -> i32 {
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == ENUM_PID.load(Ordering::Relaxed) && IsWindowVisible(hwnd) != 0 {
        ENUM_HWND.store(hwnd, Ordering::Relaxed);
        return 0; // stop enumeration
    }
    1 // continue
}

// ── 权限检查 ──

/// 检查是否以管理员权限运行
#[tauri::command]
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
#[tauri::command]
pub fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// 打开日志文件夹目录
#[tauri::command]
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
#[tauri::command]
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

#[tauri::command]
pub fn get_app_version() -> String {
    env!("APP_VERSION").to_string()
}

#[tauri::command]
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

#[tauri::command]
pub fn check_cloud_version() -> Result<CloudVersionInfo, String> {
    let url = "https://api.github.com/repos/gjy991229/D2RHub/releases/latest";
    let output = silent_cmd("curl")
        .args(["-sL", "-H", "User-Agent: D2RHub-Updater", url])
        .output();

    let stdout = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => {
            // Fallback to powershell
            let ps_res = silent_cmd("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(format!(
                    "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; (Invoke-WebRequest -UserAgent 'D2RHub-Updater' -Uri '{}' -UseBasicParsing).Content",
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

    // Match the current build flavor, then prefer the NSIS installer over MSI.
    let is_lite_build = !cfg!(feature = "ocr");
    let mut msi_url = None;
    let mut nsis_url = None;
    if let Some(assets) = json.get("assets").and_then(|a| a.as_array()) {
        for asset in assets {
            if let Some(name) = asset.get("name").and_then(|n| n.as_str()) {
                let lower = name.to_lowercase();
                let is_lite_asset = lower.contains("lite");
                if is_lite_build != is_lite_asset {
                    continue;
                }

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

#[tauri::command]
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
pub fn set_game_window_position(pid: u32, x: i32, y: i32) {
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

    if let Some(hwnd) = find_game_hwnd(pid) {
        unsafe {
            SetWindowPos(hwnd, 0, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
        }
    }
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
pub fn set_game_window_position(_pid: u32, _x: i32, _y: i32) {}

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
        fn GetWindowRect(hWnd: isize, lpRect: *mut RECT) -> i32;
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
        if GetWindowRect(hwnd, &mut rect) != 0 {
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
