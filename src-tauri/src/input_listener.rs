use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::application::multi_instance::{GameWindowPort, WindowMatch};
use crate::commands::account::{AccountManager, AccountMeta};
use crate::infrastructure::system;
use crate::state::SharedState;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicI32, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;

static APP_HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);
static KEYBOARD_HOOK: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static MOUSE_HOOK: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static BONGO_CAT_INPUT_ENABLED: AtomicBool = AtomicBool::new(false);
static BONGO_CAT_INPUT_VISIBLE: AtomicBool = AtomicBool::new(false);
static STATS_OVERLAY_MINI_INPUT_ENABLED: AtomicBool = AtomicBool::new(false);
static STATS_OVERLAY_MINI_LEFT: AtomicI32 = AtomicI32::new(0);
static STATS_OVERLAY_MINI_TOP: AtomicI32 = AtomicI32::new(0);
static STATS_OVERLAY_MINI_RIGHT: AtomicI32 = AtomicI32::new(0);
static STATS_OVERLAY_MINI_BOTTOM: AtomicI32 = AtomicI32::new(0);
static STATS_OVERLAY_LAST_CLICK_TIME: AtomicU32 = AtomicU32::new(0);
static STATS_OVERLAY_LAST_CLICK_X: AtomicI32 = AtomicI32::new(0);
static STATS_OVERLAY_LAST_CLICK_Y: AtomicI32 = AtomicI32::new(0);
static STATS_OVERLAY_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);
static INPUT_EVENT_TX: OnceLock<std::sync::mpsc::Sender<&'static str>> = OnceLock::new();
static CAPABILITY_SHORTCUTS: OnceLock<parking_lot::RwLock<CapabilityShortcutRegistry>> =
    OnceLock::new();
static SHORTCUT_ROUTING_TRANSACTION: OnceLock<parking_lot::Mutex<()>> = OnceLock::new();
static ACTIVE_HANDLED_SHORTCUT_KEYS: OnceLock<parking_lot::Mutex<HashSet<u32>>> = OnceLock::new();
static CAPABILITY_SHORTCUT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
enum CapabilityShortcutSender {
    Bounded(std::sync::mpsc::SyncSender<&'static str>),
    Unbounded(std::sync::mpsc::Sender<&'static str>),
}

struct CapabilityShortcutRoute {
    action: &'static str,
    sender: CapabilityShortcutSender,
}

#[derive(Default)]
struct CapabilityShortcutRegistry {
    core: HashMap<String, usize>,
    owners: HashMap<&'static str, (u64, HashMap<String, CapabilityShortcutRoute>)>,
}

/// RAII registration owned by an optional capability driver. Dropping the
/// guard removes every route, so a disabled module cannot keep consuming a
/// global shortcut.
pub(crate) struct CapabilityShortcutRegistration {
    owner_id: &'static str,
    generation: u64,
}

fn capability_shortcuts() -> &'static parking_lot::RwLock<CapabilityShortcutRegistry> {
    CAPABILITY_SHORTCUTS.get_or_init(|| parking_lot::RwLock::new(Default::default()))
}

fn active_handled_shortcut_keys() -> &'static parking_lot::Mutex<HashSet<u32>> {
    ACTIVE_HANDLED_SHORTCUT_KEYS.get_or_init(|| parking_lot::Mutex::new(HashSet::new()))
}

/// Serializes durable core-shortcut commits with optional route registration.
/// Callers may perform filesystem I/O while holding this lock, but must not
/// call capability lifecycle hooks synchronously from the transaction.
pub(crate) fn with_shortcut_routing_transaction<T>(operation: impl FnOnce() -> T) -> T {
    let transaction = SHORTCUT_ROUTING_TRANSACTION.get_or_init(|| parking_lot::Mutex::new(()));
    let _transaction = transaction.lock();
    operation()
}

/// Replaces the committed core reservation projection. The caller must hold
/// `with_shortcut_routing_transaction` whenever the values may differ from the
/// previous commit.
pub(crate) fn replace_core_shortcut_reservations(
    shortcuts: impl IntoIterator<Item = (String, usize)>,
) {
    capability_shortcuts().write().core = shortcuts
        .into_iter()
        .filter_map(|(shortcut, position)| {
            let shortcut = shortcut.trim().to_ascii_lowercase();
            (!shortcut.is_empty()).then_some((shortcut, position))
        })
        .collect();
}

/// Rejects a core multi-instance shortcut that would be shadowed by an
/// already-running optional capability. This closes the reverse registration
/// order: capability startup already rejects core collisions, while global
/// configuration saves must reject a later core assignment as well.
pub(crate) fn validate_core_shortcut_reservations<'a>(
    shortcuts: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let registry = capability_shortcuts().read();
    for shortcut in shortcuts {
        let shortcut = shortcut.trim().to_ascii_lowercase();
        if shortcut.is_empty() {
            continue;
        }
        if let Some(owner) = registry
            .owners
            .iter()
            .find_map(|(owner, (_, routes))| routes.contains_key(&shortcut).then_some(*owner))
        {
            return Err(format!(
                "账号快捷键 {shortcut} 与已启用模块 {owner} 的快捷键冲突"
            ));
        }
    }
    Ok(())
}

pub(crate) fn register_capability_shortcuts(
    owner_id: &'static str,
    routes: impl IntoIterator<Item = (String, &'static str)>,
    sender: std::sync::mpsc::SyncSender<&'static str>,
) -> Result<CapabilityShortcutRegistration, String> {
    install_capability_shortcuts(
        owner_id,
        routes,
        CapabilityShortcutSender::Bounded(sender),
        false,
    )
}

pub(crate) fn register_unbounded_capability_shortcuts(
    owner_id: &'static str,
    routes: impl IntoIterator<Item = (String, &'static str)>,
    sender: std::sync::mpsc::Sender<&'static str>,
) -> Result<CapabilityShortcutRegistration, String> {
    install_capability_shortcuts(
        owner_id,
        routes,
        CapabilityShortcutSender::Unbounded(sender),
        false,
    )
}

/// Atomically replaces one capability's routes while preserving every other
/// owner's conflict checks. The previous guard becomes inert through its
/// generation token, so dropping it cannot remove the replacement.
pub(crate) fn replace_capability_shortcuts(
    owner_id: &'static str,
    routes: impl IntoIterator<Item = (String, &'static str)>,
    sender: std::sync::mpsc::SyncSender<&'static str>,
) -> Result<CapabilityShortcutRegistration, String> {
    install_capability_shortcuts(
        owner_id,
        routes,
        CapabilityShortcutSender::Bounded(sender),
        true,
    )
}

pub(crate) fn replace_unbounded_capability_shortcuts(
    owner_id: &'static str,
    routes: impl IntoIterator<Item = (String, &'static str)>,
    sender: std::sync::mpsc::Sender<&'static str>,
) -> Result<CapabilityShortcutRegistration, String> {
    install_capability_shortcuts(
        owner_id,
        routes,
        CapabilityShortcutSender::Unbounded(sender),
        true,
    )
}

fn install_capability_shortcuts(
    owner_id: &'static str,
    routes: impl IntoIterator<Item = (String, &'static str)>,
    sender: CapabilityShortcutSender,
    replace_owner: bool,
) -> Result<CapabilityShortcutRegistration, String> {
    with_shortcut_routing_transaction(|| {
        install_capability_shortcuts_in_transaction(owner_id, routes, sender, replace_owner)
    })
}

fn install_capability_shortcuts_in_transaction(
    owner_id: &'static str,
    routes: impl IntoIterator<Item = (String, &'static str)>,
    sender: CapabilityShortcutSender,
    replace_owner: bool,
) -> Result<CapabilityShortcutRegistration, String> {
    let mut normalized = HashMap::new();
    for (shortcut, action) in routes {
        let shortcut = shortcut.trim().to_ascii_lowercase();
        if shortcut.is_empty() {
            return Err(format!("capability {owner_id} 注册了空快捷键"));
        }
        if normalized
            .insert(
                shortcut.clone(),
                CapabilityShortcutRoute {
                    action,
                    sender: sender.clone(),
                },
            )
            .is_some()
        {
            return Err(format!("capability {owner_id} 的快捷键重复: {shortcut}"));
        }
    }
    if normalized.is_empty() {
        return Err(format!("capability {owner_id} 没有可注册的快捷键"));
    }

    let mut registry = capability_shortcuts().write();
    if !replace_owner && registry.owners.contains_key(owner_id) {
        return Err(format!("capability {owner_id} 的快捷键已注册"));
    }
    for shortcut in normalized.keys() {
        if let Some(position) = registry.core.get(shortcut) {
            return Err(format!(
                "快捷键 {shortcut} 已由多开核心账号位置 {position} 使用"
            ));
        }
        if let Some(conflicting_owner) = registry
            .owners
            .iter()
            .filter(|(owner, _)| **owner != owner_id)
            .find_map(|(owner, (_, routes))| routes.contains_key(shortcut).then_some(*owner))
        {
            return Err(format!(
                "快捷键 {shortcut} 已由 capability {conflicting_owner} 注册"
            ));
        }
    }
    let generation = CAPABILITY_SHORTCUT_GENERATION.fetch_add(1, Ordering::Relaxed);
    registry.owners.insert(owner_id, (generation, normalized));
    Ok(CapabilityShortcutRegistration {
        owner_id,
        generation,
    })
}

fn dispatch_capability_shortcut(shortcut: &str) -> bool {
    let delivery = capability_shortcuts()
        .read()
        .owners
        .values()
        .find_map(|(_, routes)| {
            routes
                .get(shortcut)
                .map(|route| (route.sender.clone(), route.action))
        });
    let Some((sender, action)) = delivery else {
        return false;
    };
    match sender {
        CapabilityShortcutSender::Bounded(sender) => match sender.try_send(action) {
            Ok(()) | Err(std::sync::mpsc::TrySendError::Full(_)) => true,
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => false,
        },
        CapabilityShortcutSender::Unbounded(sender) => sender.send(action).is_ok(),
    }
}

impl Drop for CapabilityShortcutRegistration {
    fn drop(&mut self) {
        let mut registry = capability_shortcuts().write();
        if registry
            .owners
            .get(self.owner_id)
            .is_some_and(|(generation, _)| *generation == self.generation)
        {
            registry.owners.remove(self.owner_id);
        }
    }
}

pub fn set_bongo_cat_input_enabled(enabled: bool) {
    BONGO_CAT_INPUT_ENABLED.store(enabled, Ordering::Relaxed);
}

pub(crate) fn set_bongo_cat_input_visible_state(visible: bool) {
    BONGO_CAT_INPUT_VISIBLE.store(visible, Ordering::Relaxed);
}

#[tauri::command]
pub fn set_bongo_cat_input_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    if visible {
        let installed = app
            .state::<SharedState>()
            .configuration()
            .snapshot()
            .is_some_and(|config| {
                config.optional_module_installed(crate::domain::config::OPTIONAL_MODULE_PET)
            });
        if !installed {
            return Err("桌宠模块尚未安装".to_string());
        }
    }
    set_bongo_cat_input_visible_state(visible);
    Ok(())
}

pub(crate) fn set_stats_overlay_mini_input_region_state(
    enabled: bool,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) {
    STATS_OVERLAY_MINI_LEFT.store(x, Ordering::Relaxed);
    STATS_OVERLAY_MINI_TOP.store(y, Ordering::Relaxed);
    STATS_OVERLAY_MINI_RIGHT.store(x.saturating_add_unsigned(width), Ordering::Relaxed);
    STATS_OVERLAY_MINI_BOTTOM.store(y.saturating_add_unsigned(height), Ordering::Relaxed);
    STATS_OVERLAY_LAST_CLICK_TIME.store(0, Ordering::Relaxed);
    STATS_OVERLAY_POINTER_INSIDE.store(false, Ordering::Relaxed);
    STATS_OVERLAY_MINI_INPUT_ENABLED.store(enabled, Ordering::Release);
}

#[tauri::command]
pub fn set_stats_overlay_mini_input_region(
    app: AppHandle,
    enabled: bool,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    if enabled {
        let installed = app
            .state::<SharedState>()
            .configuration()
            .snapshot()
            .is_some_and(|config| {
                config.optional_module_installed(
                    crate::domain::config::OPTIONAL_MODULE_OVERLAYS,
                ) && config.optional_module_installed(
                    crate::domain::config::OPTIONAL_MODULE_AUTOMATION,
                )
            });
        if !installed {
            return Err("识别与统计模块尚未安装".to_string());
        }
    }
    set_stats_overlay_mini_input_region_state(enabled, x, y, width, height);
    Ok(())
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
const WM_KEYUP: usize = 0x0101;
const WM_SYSKEYDOWN: usize = 0x0104;
const WM_SYSKEYUP: usize = 0x0105;
const WM_MOUSEMOVE: usize = 0x0200;
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

#[repr(C)]
#[allow(clippy::upper_case_acronyms)]
struct MSLLHOOKSTRUCT {
    pt: POINT,
    mouse_data: u32,
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

    fn GetDoubleClickTime() -> u32;
    fn GetSystemMetrics(nIndex: i32) -> i32;

}

unsafe fn handle_stats_overlay_mini_double_click(mouse: &MSLLHOOKSTRUCT) -> bool {
    if !STATS_OVERLAY_MINI_INPUT_ENABLED.load(Ordering::Acquire) {
        return false;
    }

    if !is_inside_stats_overlay_mini_region(mouse.pt.x, mouse.pt.y) {
        STATS_OVERLAY_LAST_CLICK_TIME.store(0, Ordering::Relaxed);
        return false;
    }

    let previous_time = STATS_OVERLAY_LAST_CLICK_TIME.swap(mouse.time, Ordering::Relaxed);
    let previous_x = STATS_OVERLAY_LAST_CLICK_X.swap(mouse.pt.x, Ordering::Relaxed);
    let previous_y = STATS_OVERLAY_LAST_CLICK_Y.swap(mouse.pt.y, Ordering::Relaxed);
    const SM_CXDOUBLECLK: i32 = 36;
    const SM_CYDOUBLECLK: i32 = 37;
    let max_delta_x = GetSystemMetrics(SM_CXDOUBLECLK).max(1) / 2;
    let max_delta_y = GetSystemMetrics(SM_CYDOUBLECLK).max(1) / 2;
    if !is_stats_overlay_double_click(
        previous_time,
        mouse.time,
        previous_x,
        previous_y,
        mouse.pt.x,
        mouse.pt.y,
        GetDoubleClickTime(),
        max_delta_x,
        max_delta_y,
    ) {
        return false;
    }

    STATS_OVERLAY_LAST_CLICK_TIME.store(0, Ordering::Relaxed);
    if let Some(tx) = INPUT_EVENT_TX.get() {
        let _ = tx.send("StatsOverlayMiniToggle");
    }
    true
}

fn handle_stats_overlay_mini_pointer_move(mouse: &MSLLHOOKSTRUCT) {
    if !STATS_OVERLAY_MINI_INPUT_ENABLED.load(Ordering::Acquire) {
        return;
    }

    let inside = is_inside_stats_overlay_mini_region(mouse.pt.x, mouse.pt.y);
    if STATS_OVERLAY_POINTER_INSIDE.swap(inside, Ordering::Relaxed) == inside {
        return;
    }
    if let Some(tx) = INPUT_EVENT_TX.get() {
        let _ = tx.send(if inside {
            "StatsOverlayMiniHoverEnter"
        } else {
            "StatsOverlayMiniHoverLeave"
        });
    }
}

fn is_inside_stats_overlay_mini_region(x: i32, y: i32) -> bool {
    let left = STATS_OVERLAY_MINI_LEFT.load(Ordering::Relaxed);
    let top = STATS_OVERLAY_MINI_TOP.load(Ordering::Relaxed);
    let right = STATS_OVERLAY_MINI_RIGHT.load(Ordering::Relaxed);
    let bottom = STATS_OVERLAY_MINI_BOTTOM.load(Ordering::Relaxed);
    x >= left && x < right && y >= top && y < bottom
}

#[allow(clippy::too_many_arguments)]
fn is_stats_overlay_double_click(
    previous_time: u32,
    current_time: u32,
    previous_x: i32,
    previous_y: i32,
    current_x: i32,
    current_y: i32,
    max_delay: u32,
    max_delta_x: i32,
    max_delta_y: i32,
) -> bool {
    previous_time != 0
        && current_time.wrapping_sub(previous_time) <= max_delay
        && (current_x - previous_x).abs() <= max_delta_x
        && (current_y - previous_y).abs() <= max_delta_y
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
                // Do not hold the shortcut-map lock while reading the
                // configuration snapshot. Configuration updates rebuild the
                // shortcut map, so overlapping both locks would invert that
                // writer's lock order.
                let position = {
                    let shortcut_map = state.shortcut_map.read();
                    shortcut_map.get(&combo_lower).copied()
                };
                if let Some(pos) = position {
                    if let Some(cfg) = state.configuration().snapshot() {
                        let accounts_dir = cfg.accounts_dir.clone();
                        let app_clone = app.clone();
                        let combo_clone = combo.clone();
                        std::thread::spawn(move || {
                            focus_account_at_position(&app_clone, &accounts_dir, pos, &combo_clone);
                        });
                        return true; // 已处理，吞掉按键
                    }
                }
                // Multi-instance account focus is a core action and therefore
                // always wins if a legacy or concurrently edited optional
                // module happens to claim the same key. Module configuration
                // validation still prevents new conflicts at rest.
                if dispatch_capability_shortcut(&combo_lower) {
                    return true;
                }
            }
        }
    }
    false
}

/// 加载账号列表，找到指定位置的账号，聚焦其游戏窗口
/// 优先通过实例注册表中的 PID 查找，降级使用兼容窗口标题匹配。
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

        let windows = system::SystemGameWindowPort;
        // 1) 优先按实例注册表中的 PID 查找；保留旧标题降级行为。
        if let Some(state) = app.try_state::<SharedState>() {
            let facade = state.multi_instance().facade();
            if let Some(matched_by) = facade.focus_account_window(&windows, &account.id, title) {
                match matched_by {
                    WindowMatch::ProcessId => {
                        let pid = facade.instance(&account.id).map(|instance| instance.pid);
                        crate::logger::log_msg(
                            "INFO",
                            "Shortcut",
                            &format!(
                                "快捷键触发(pid): 位置{} → 账号「{}」, pid={}",
                                position,
                                title,
                                pid.map(|value| value.to_string())
                                    .unwrap_or_else(|| "unknown".to_string())
                            ),
                        );
                    }
                    WindowMatch::CompatibilityTitle => {
                        crate::logger::log_msg(
                            "INFO",
                            "Shortcut",
                            &format!("快捷键触发(title): 位置{} → 账号「{}」", position, title),
                        );
                    }
                }
            }
        } else if windows.focus_by_title_compat(title) {
            crate::logger::log_msg(
                "INFO",
                "Shortcut",
                &format!("快捷键触发(title): 位置{} → 账号「{}」", position, title),
            );
        }
    }
}

unsafe extern "system" fn keyboard_hook_proc(
    code: std::os::raw::c_int,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 && (wparam == WM_KEYUP || wparam == WM_SYSKEYUP) {
        let kbd = &*(lparam as *const KBDLLHOOKSTRUCT);
        if active_handled_shortcut_keys().lock().remove(&kbd.vk_code) {
            // The matching key-down was a global shortcut and was swallowed.
            // Swallow its key-up as well so D2R never receives an orphan event.
            return 1;
        }
    } else if code >= 0 && (wparam == WM_KEYDOWN || wparam == WM_SYSKEYDOWN) {
        // ── 快捷键检测 ──
        let kbd = &*(lparam as *const KBDLLHOOKSTRUCT);
        if active_handled_shortcut_keys()
            .lock()
            .contains(&kbd.vk_code)
        {
            // Windows emits repeated key-down messages while a key is held.
            // The first event already dispatched this shortcut; consume repeats
            // without enqueueing duplicate room workflows.
            return 1;
        }
        // 仅处理按下事件（非抬起），flags bit 7 (LLKHF_UP) = 0 表示按下
        if (kbd.flags & 0x80) == 0 && try_handle_shortcut(kbd) {
            active_handled_shortcut_keys().lock().insert(kbd.vk_code);
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
    if code >= 0 && (wparam == WM_MOUSEMOVE || wparam == WM_LBUTTONDOWN) {
        let mouse = &*(lparam as *const MSLLHOOKSTRUCT);
        if wparam == WM_MOUSEMOVE {
            handle_stats_overlay_mini_pointer_move(mouse);
        } else if handle_stats_overlay_mini_double_click(mouse) {
            // The first click remains click-through. Swallow the confirming
            // second press so the foreground game cannot also interpret the
            // gesture as a double-click action.
            return 1;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{
        dispatch_capability_shortcut, is_stats_overlay_double_click, register_capability_shortcuts,
        replace_capability_shortcuts, replace_core_shortcut_reservations,
        validate_core_shortcut_reservations, with_shortcut_routing_transaction,
    };

    static SHORTCUT_TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn stats_overlay_click_through_double_click_keeps_time_and_position_limits() {
        assert!(is_stats_overlay_double_click(
            1_000, 1_240, 300, 200, 302, 201, 500, 2, 2,
        ));
        assert!(!is_stats_overlay_double_click(
            1_000, 1_501, 300, 200, 302, 201, 500, 2, 2,
        ));
        assert!(!is_stats_overlay_double_click(
            1_000, 1_240, 300, 200, 303, 201, 500, 2, 2,
        ));
    }

    #[test]
    fn capability_shortcuts_are_bounded_unique_and_owned_by_a_guard() {
        let _serial = SHORTCUT_TEST_SERIAL.lock().unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let registration = register_capability_shortcuts(
            "shortcut-router-test",
            [(" Ctrl+Alt+R ".to_string(), "start-primary")],
            sender.clone(),
        )
        .unwrap();

        assert!(register_capability_shortcuts(
            "shortcut-router-test",
            [("Ctrl+Alt+J".to_string(), "start-followers")],
            sender.clone(),
        )
        .is_err());
        assert!(register_capability_shortcuts(
            "shortcut-router-conflict-test",
            [("ctrl+alt+r".to_string(), "conflict")],
            sender,
        )
        .is_err());

        assert!(dispatch_capability_shortcut("ctrl+alt+r"));
        assert!(dispatch_capability_shortcut("ctrl+alt+r"));
        assert_eq!(receiver.try_recv().unwrap(), "start-primary");
        assert!(receiver.try_recv().is_err());

        drop(registration);
        assert!(!dispatch_capability_shortcut("ctrl+alt+r"));
    }

    #[test]
    fn capability_shortcut_replacement_is_atomic_and_old_guard_is_inert() {
        let _serial = SHORTCUT_TEST_SERIAL.lock().unwrap();
        let (old_sender, old_receiver) = std::sync::mpsc::sync_channel(1);
        let old = register_capability_shortcuts(
            "shortcut-replace-test",
            [("Ctrl+Alt+R".to_string(), "old")],
            old_sender,
        )
        .unwrap();
        let (new_sender, new_receiver) = std::sync::mpsc::sync_channel(1);
        let replacement = replace_capability_shortcuts(
            "shortcut-replace-test",
            [("Ctrl+Alt+J".to_string(), "new")],
            new_sender,
        )
        .unwrap();

        drop(old);
        assert!(!dispatch_capability_shortcut("ctrl+alt+r"));
        assert!(dispatch_capability_shortcut("ctrl+alt+j"));
        assert_eq!(new_receiver.recv().unwrap(), "new");
        assert!(old_receiver.recv().is_err());

        drop(replacement);
        assert!(!dispatch_capability_shortcut("ctrl+alt+j"));
    }

    #[test]
    fn active_capability_reservation_rejects_a_later_core_shortcut() {
        let _serial = SHORTCUT_TEST_SERIAL.lock().unwrap();
        let (sender, _receiver) = std::sync::mpsc::sync_channel(1);
        let registration = register_capability_shortcuts(
            "shortcut-core-collision-test",
            [("Ctrl+Alt+R".to_string(), "start-primary")],
            sender,
        )
        .unwrap();

        let error = validate_core_shortcut_reservations([" ctrl+ALT+r "]).unwrap_err();
        assert!(error.contains("shortcut-core-collision-test"));
        assert!(validate_core_shortcut_reservations(["Ctrl+1"]).is_ok());

        drop(registration);
        assert!(validate_core_shortcut_reservations(["Ctrl+Alt+R"]).is_ok());
    }

    #[test]
    fn committed_core_shortcut_rejects_a_later_capability_route() {
        let _serial = SHORTCUT_TEST_SERIAL.lock().unwrap();
        with_shortcut_routing_transaction(|| {
            replace_core_shortcut_reservations([("Ctrl+F23".to_string(), 2)]);
        });
        let (sender, _receiver) = std::sync::mpsc::sync_channel(1);

        let error = register_capability_shortcuts(
            "shortcut-after-core-test",
            [("ctrl+f23".to_string(), "optional")],
            sender,
        )
        .err()
        .expect("core collision must reject the optional route");
        assert!(error.contains("多开核心账号位置 2"));

        with_shortcut_routing_transaction(|| {
            replace_core_shortcut_reservations(std::iter::empty());
        });
    }
}
