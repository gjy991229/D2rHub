use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::account::{AccountManager, AccountMeta};
use crate::commands::system;
use crate::state::SharedState;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;

static APP_HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);
static KEYBOARD_HOOK: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static MOUSE_HOOK: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static BONGO_CAT_INPUT_ENABLED: AtomicBool = AtomicBool::new(false);
static BONGO_CAT_INPUT_VISIBLE: AtomicBool = AtomicBool::new(false);
static INPUT_EVENT_TX: OnceLock<std::sync::mpsc::Sender<&'static str>> = OnceLock::new();

pub fn set_bongo_cat_input_enabled(enabled: bool) {
    BONGO_CAT_INPUT_ENABLED.store(enabled, Ordering::Relaxed);
}

#[tauri::command]
pub fn set_bongo_cat_input_visible(visible: bool) {
    BONGO_CAT_INPUT_VISIBLE.store(visible, Ordering::Relaxed);
}

/// RAII guard：Drop 时自动调用 UnhookWindowsHookEx 并清空对应的全局钩子指针，
/// 确保线程 panic 或提前退出时释放钩子且不留悬空指针。
struct HookGuard {
    hook: *mut std::ffi::c_void,
    slot: &'static AtomicPtr<std::ffi::c_void>,
}

impl HookGuard {
    unsafe fn new(hook: *mut std::ffi::c_void, slot: &'static AtomicPtr<std::ffi::c_void>) -> Self {
        Self { hook, slot }
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        if !self.hook.is_null() {
            unsafe {
                UnhookWindowsHookEx(self.hook);
            }
        }
        self.slot.store(std::ptr::null_mut(), Ordering::SeqCst);
    }
}

// Low-level Windows Hook types and constants
// These aliases mirror Win32 SDK names; preserving them makes FFI review less error-prone.
#[allow(clippy::upper_case_acronyms)]
type LRESULT = isize;
#[allow(clippy::upper_case_acronyms)]
type WPARAM = usize;
#[allow(clippy::upper_case_acronyms)]
type LPARAM = isize;
#[allow(clippy::upper_case_acronyms)]
type HOOKPROC = Option<
    unsafe extern "system" fn(code: std::os::raw::c_int, wparam: WPARAM, lparam: LPARAM) -> LRESULT,
>;

const WH_KEYBOARD_LL: std::os::raw::c_int = 13;
const WH_MOUSE_LL: std::os::raw::c_int = 14;

const WM_KEYDOWN: usize = 0x0100;
const WM_SYSKEYDOWN: usize = 0x0104;
const WM_LBUTTONDOWN: usize = 0x0201;
const WM_RBUTTONDOWN: usize = 0x0204;

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(clippy::upper_case_acronyms)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(clippy::upper_case_acronyms)]
struct MSG {
    hwnd: *mut std::ffi::c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: POINT,
    l_private: u32,
}

/// KBDLLHOOKSTRUCT — 键盘低级钩子数据结构
#[repr(C)]
#[allow(clippy::upper_case_acronyms)]
struct KBDLLHOOKSTRUCT {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    dw_extra_info: usize,
}

extern "system" {
    fn SetWindowsHookExW(
        idHook: std::os::raw::c_int,
        lpfn: HOOKPROC,
        hmod: *mut std::ffi::c_void,
        dwThreadId: u32,
    ) -> *mut std::ffi::c_void;

    fn UnhookWindowsHookEx(hhk: *mut std::ffi::c_void) -> std::os::raw::c_int;

    fn CallNextHookEx(
        hhk: *mut std::ffi::c_void,
        nCode: std::os::raw::c_int,
        wParam: WPARAM,
        lParam: LPARAM,
    ) -> LRESULT;

    fn GetMessageW(
        lpMsg: *mut std::ffi::c_void,
        hWnd: *mut std::ffi::c_void,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32,
    ) -> std::os::raw::c_int;

    fn TranslateMessage(lpMsg: *const std::ffi::c_void) -> std::os::raw::c_int;
    fn DispatchMessageW(lpMsg: *const std::ffi::c_void) -> LRESULT;

    fn GetKeyState(nVirtKey: i32) -> i16;

}

/// 将虚拟键码转换为可读键名
fn vk_to_key_string(vk: u32) -> String {
    match vk {
        // 数字 0-9
        0x30..=0x39 => format!("{}", (vk - 0x30)),
        // 字母 A-Z
        0x41..=0x5A => format!("{}", (vk as u8) as char),
        // F1-F24
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        // 功能键
        0x20 => "Space".to_string(),
        0x0D => "Enter".to_string(),
        0x09 => "Tab".to_string(),
        0x1B => "Escape".to_string(),
        0x08 => "Backspace".to_string(),
        0x2E => "Delete".to_string(),
        0x2D => "Insert".to_string(),
        0x24 => "Home".to_string(),
        0x23 => "End".to_string(),
        0x21 => "PageUp".to_string(),
        0x22 => "PageDown".to_string(),
        0x26 => "Up".to_string(),
        0x28 => "Down".to_string(),
        0x25 => "Left".to_string(),
        0x27 => "Right".to_string(),
        0x2C => "PrintScreen".to_string(),
        0x91 => "ScrollLock".to_string(),
        0x13 => "Pause".to_string(),
        0x90 => "NumLock".to_string(),
        0x6A => "Num*".to_string(),
        0x6B => "Num+".to_string(),
        0x6D => "Num-".to_string(),
        0x6E => "Num.".to_string(),
        0x6F => "Num/".to_string(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0xBA => ";".to_string(),
        0xBB => "=".to_string(),
        0xBC => ",".to_string(),
        0xBD => "-".to_string(),
        0xBE => ".".to_string(),
        0xBF => "/".to_string(),
        0xC0 => "`".to_string(),
        0xDB => "[".to_string(),
        0xDC => "\\".to_string(),
        0xDD => "]".to_string(),
        0xDE => "'".to_string(),
        _ => format!("VK{:X}", vk),
    }
}

/// 根据当前修饰键和主键构造快捷键字符串，如 "Ctrl+1"、"Alt+F"、"F5"
fn build_shortcut_string(ctrl: bool, alt: bool, shift: bool, key: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if alt {
        parts.push("Alt");
    }
    if shift {
        parts.push("Shift");
    }
    parts.push(key);
    parts.join("+")
}

/// 检查当前按键组合是否匹配某个已配置的快捷键，若匹配则聚焦对应账号窗口
/// 返回 true 表示已处理（应吞掉该按键事件）
unsafe fn try_handle_shortcut(kbd: &KBDLLHOOKSTRUCT) -> bool {
    // 忽略修饰键本身的按下
    let vk = kbd.vk_code;
    if vk == 0x10 || vk == 0x11 || vk == 0x12 {
        return false;
    }

    // 使用 GetKeyState（与消息队列同步）而非 GetAsyncKeyState（异步物理状态），
    // 避免快速按键序列中修饰键状态与钩子消息不匹配的竞态
    let ctrl = GetKeyState(0x11) < 0;
    let alt = GetKeyState(0x12) < 0;
    let shift = GetKeyState(0x10) < 0;
    let key_name = vk_to_key_string(vk);
    let combo = build_shortcut_string(ctrl, alt, shift, &key_name);

    // Read from cached shortcut memory map (non-blocking: skip if lock held)
    if let Ok(guard) = APP_HANDLE.try_lock() {
        if let Some(app) = &*guard {
            if let Some(state) = app.try_state::<SharedState>() {
                let combo_lower = combo.to_lowercase();
                let shortcut_map = state.shortcut_map.read();
                if let Some(&pos) = shortcut_map.get(&combo_lower) {
                    let config = state.config.read();
                    if let Some(cfg) = config.as_ref() {
                        let accounts_dir = cfg.accounts_dir.clone();
                        let app_clone = app.clone();
                        let combo_clone = combo.clone();
                        std::thread::spawn(move || {
                            focus_account_at_position(&app_clone, &accounts_dir, pos, &combo_clone);
                        });
                        return true; // 已处理，吞掉按键
                    }
                }
            }
        }
    }
    false
}

/// 加载账号列表，找到指定位置的账号，聚焦其游戏窗口
/// 优先通过 PID 查找（active_games），降级使用窗口标题精确匹配
fn focus_account_at_position(app: &AppHandle, accounts_dir: &str, position: usize, _combo: &str) {
    let ids = AccountManager::list_ids(accounts_dir);
    let mut accounts: Vec<AccountMeta> = Vec::new();
    for id in &ids {
        if let Ok(meta) = AccountManager::load_meta(accounts_dir, id) {
            accounts.push(meta);
        }
    }
    // 按 order 排序
    accounts.sort_by_key(|a| a.order);

    let index = position - 1; // 1-based → 0-based
    if let Some(account) = accounts.get(index) {
        let title = if account.display_name.is_empty() {
            &account.id
        } else {
            &account.display_name
        };

        // 1) 优先按 PID 查找（active_games 中有记录）
        if let Some(state) = app.try_state::<SharedState>() {
            let active = state.active_games.read();
            if let Some(&pid) = active.get(&account.id) {
                if let Some(hwnd) = system::find_game_hwnd(pid) {
                    crate::logger::log_msg(
                        "INFO",
                        "Shortcut",
                        &format!(
                            "快捷键触发(pid): 位置{} → 账号「{}」, pid={}",
                            position, title, pid
                        ),
                    );
                    system::bring_window_to_foreground_raw(hwnd);
                    return;
                }
            }
        }

        // 2) 降级：按窗口标题精确匹配
        if let Some(hwnd) = system::find_game_hwnd_by_title(title) {
            crate::logger::log_msg(
                "INFO",
                "Shortcut",
                &format!("快捷键触发(title): 位置{} → 账号「{}」", position, title),
            );
            system::bring_window_to_foreground_raw(hwnd);
        }
    }
}

unsafe extern "system" fn keyboard_hook_proc(
    code: std::os::raw::c_int,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 && (wparam == WM_KEYDOWN || wparam == WM_SYSKEYDOWN) {
        // ── 快捷键检测 ──
        let kbd = &*(lparam as *const KBDLLHOOKSTRUCT);
        // 仅处理按下事件（非抬起），flags bit 7 (LLKHF_UP) = 0 表示按下
        if (kbd.flags & 0x80) == 0 && try_handle_shortcut(kbd) {
            // 快捷键已处理，吞掉该按键，不传递给其他应用
            return 1;
        }

        if BONGO_CAT_INPUT_ENABLED.load(Ordering::Relaxed)
            && BONGO_CAT_INPUT_VISIBLE.load(Ordering::Relaxed)
        {
            if let Some(tx) = INPUT_EVENT_TX.get() {
                let _ = tx.send("Keyboard");
            }
        }
    }
    CallNextHookEx(KEYBOARD_HOOK.load(Ordering::SeqCst), code, wparam, lparam)
}

unsafe extern "system" fn mouse_hook_proc(
    code: std::os::raw::c_int,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0
        && (wparam == WM_LBUTTONDOWN || wparam == WM_RBUTTONDOWN)
        && BONGO_CAT_INPUT_ENABLED.load(Ordering::Relaxed)
        && BONGO_CAT_INPUT_VISIBLE.load(Ordering::Relaxed)
    {
        if let Some(tx) = INPUT_EVENT_TX.get() {
            let event_type = if wparam == WM_LBUTTONDOWN {
                "MouseLeft"
            } else {
                "MouseRight"
            };
            let _ = tx.send(event_type);
        }
    }
    CallNextHookEx(MOUSE_HOOK.load(Ordering::SeqCst), code, wparam, lparam)
}

pub fn start_input_listener(app_handle: AppHandle) {
    if let Some(state) = app_handle.try_state::<SharedState>() {
        let enabled = state
            .config
            .read()
            .as_ref()
            .map(|c| c.enable_bongo_cat)
            .unwrap_or(false);
        set_bongo_cat_input_enabled(enabled);
    }

    if let Ok(mut guard) = APP_HANDLE.lock() {
        *guard = Some(app_handle.clone());
    }

    let (tx, rx) = std::sync::mpsc::channel::<&'static str>();
    let _ = INPUT_EVENT_TX.set(tx);
    std::thread::spawn(move || {
        while let Ok(event_type) = rx.recv() {
            let _ = app_handle.emit("global-input-event", event_type);
        }
    });

    std::thread::spawn(|| {
        unsafe {
            let k_hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook_proc),
                std::ptr::null_mut(),
                0,
            );
            KEYBOARD_HOOK.store(k_hook, Ordering::SeqCst);

            let m_hook =
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), std::ptr::null_mut(), 0);
            MOUSE_HOOK.store(m_hook, Ordering::SeqCst);

            if KEYBOARD_HOOK.load(Ordering::SeqCst).is_null()
                || MOUSE_HOOK.load(Ordering::SeqCst).is_null()
            {
                crate::logger::log_msg("ERROR", "System", "Failed to install global input hooks.");
                return;
            }

            crate::logger::log_msg(
                "INFO",
                "System",
                "Global input hooks installed successfully.",
            );

            // Standard Win32 Message Loop to keep the hooks alive
            // HookGuard 确保即使线程 panic，钩子也会被释放
            let _k_guard = HookGuard::new(k_hook, &KEYBOARD_HOOK);
            let _m_guard = HookGuard::new(m_hook, &MOUSE_HOOK);

            let mut msg = std::mem::zeroed::<MSG>();
            while GetMessageW(
                &mut msg as *mut MSG as *mut std::ffi::c_void,
                std::ptr::null_mut(),
                0,
                0,
            ) > 0
            {
                TranslateMessage(&msg as *const MSG as *const std::ffi::c_void);
                DispatchMessageW(&msg as *const MSG as *const std::ffi::c_void);
            }
        }
    });
}
