//! Shared shortcut routes and the desktop pet's passive input subscription.
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter, Manager};
use crate::application::multi_instance::{GameWindowPort, WindowMatch};
use crate::commands::account::{AccountManager, AccountMeta};
use crate::infrastructure::system;
use crate::state::{CoreShortcutAction, SharedState};

pub(crate) mod hotkeys;
mod runtime;

static APP_HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);
static BONGO_CAT_INPUT_ENABLED: AtomicBool = AtomicBool::new(false);
static BONGO_CAT_INPUT_VISIBLE: AtomicBool = AtomicBool::new(false);
static SHORTCUT_CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static OPTIONAL_SHORTCUTS_ALLOWED: AtomicBool = AtomicBool::new(false);
static CAPABILITY_SHORTCUTS: OnceLock<parking_lot::RwLock<CapabilityShortcutRegistry>> = OnceLock::new();
static SHORTCUT_ROUTING_TRANSACTION: OnceLock<parking_lot::Mutex<()>> = OnceLock::new();
static CAPABILITY_SHORTCUT_GENERATION: AtomicU64 = AtomicU64::new(1);

fn refresh_input_services() {
    hotkeys::refresh();
    runtime::request_refresh();
}
#[derive(Clone)]
enum CapabilityShortcutSender {
    #[cfg(test)]
    Bounded(std::sync::mpsc::SyncSender<&'static str>),
    Unbounded(std::sync::mpsc::Sender<&'static str>),
}

struct CapabilityShortcutRoute {
    action: &'static str,
    sender: CapabilityShortcutSender,
}

#[derive(Default)]
struct CapabilityShortcutRegistry {
    core: HashMap<String, CoreShortcutAction>,
    owners: HashMap<&'static str, (u64, HashMap<String, CapabilityShortcutRoute>)>,
    saved_owners: HashMap<&'static str, HashSet<String>>,
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
    replace_core_shortcut_routes(
        shortcuts
            .into_iter()
            .map(|(shortcut, position)| (shortcut, CoreShortcutAction::FocusAccount(position))),
    );
}

pub(crate) fn replace_core_shortcut_routes(
    shortcuts: impl IntoIterator<Item = (String, CoreShortcutAction)>,
) {
    let routes: HashMap<_, _> = shortcuts
        .into_iter()
        .filter_map(|(shortcut, action)| {
            let shortcut = shortcut.trim().to_ascii_lowercase();
            (!shortcut.is_empty()).then_some((shortcut, action))
        })
        .collect();
    capability_shortcuts().write().core = routes;
    refresh_input_services();
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
                "核心快捷键 {shortcut} 与已启用模块 {owner} 的快捷键冲突"
            ));
        }
    }
    Ok(())
}

/// Durable reservations survive dropping a capability's active routes.
/// Only installed modules reserve keys, so uninstalling releases the keys
/// without deleting that module's saved settings.
pub(crate) fn validate_core_shortcut_reservations_for_modules<'a>(
    shortcuts: impl IntoIterator<Item = &'a str>,
    installed_modules: &[String],
) -> Result<(), String> {
    let shortcuts: Vec<_> = shortcuts.into_iter().collect();
    validate_core_shortcut_reservations(shortcuts.iter().copied())?;
    let registry = capability_shortcuts().read();
    for shortcut in &shortcuts {
        let shortcut = shortcut.trim().to_ascii_lowercase();
        if !shortcut.is_empty() && registry.saved_owners.iter().any(|(owner, saved)| {
            installed_modules.iter().any(|module| module.as_str() == *owner) && saved.contains(&shortcut)
        }) {
            return Err(format!("快捷键 {shortcut} 已被原配置保留，请选择其他组合"));
        }
    }
    drop(registry);
    hotkeys::validate(shortcuts.into_iter().map(str::to_string).collect())
}

pub(crate) fn replace_saved_capability_shortcuts<'a>(
    owner: &'static str,
    shortcuts: impl IntoIterator<Item = &'a str>,
) {
    let saved = shortcuts.into_iter().filter_map(|shortcut| {
        crate::capabilities::room_automation::canonicalize_shortcut(shortcut).ok()
            .map(|shortcut| shortcut.to_ascii_lowercase())
    }).collect();
    capability_shortcuts().write().saved_owners.insert(owner, saved);
}

/// Caller holds the routing transaction until the module sidecar is committed.
pub(crate) fn validate_saved_capability_shortcuts<'a>(
    owner: &'static str,
    shortcuts: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let registry = capability_shortcuts().read();
    let mut normalized = Vec::new();
    for shortcut in shortcuts {
        let Ok(shortcut) = crate::capabilities::room_automation::canonicalize_shortcut(shortcut) else {
            continue; // Disabled module drafts may contain incomplete shortcuts.
        };
        let shortcut = shortcut.to_ascii_lowercase();
        if normalized.contains(&shortcut) {
            return Err(format!("快捷键 {shortcut} 同时用于主号建房和跟随入房，请为两个动作设置不同组合"));
        }
        if let Some(action) = registry.core.get(&shortcut) {
            let label = match action {
                CoreShortcutAction::FocusAccount(position) => format!("账号位置 #{position}"),
                CoreShortcutAction::ToggleMainWindow => "切换主面板".to_string(),
            };
            return Err(format!("快捷键 {shortcut} 已用于{label}，配置未保存"));
        }
        if registry.owners.iter().any(|(other, (_, routes))| {
                *other != owner && routes.contains_key(&shortcut)
            })
        {
            return Err(format!("快捷键 {shortcut} 已被其他动作使用，配置未保存"));
        }
        normalized.push(shortcut);
    }
    drop(registry);
    hotkeys::validate(normalized)
}

#[cfg(test)]
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
#[cfg(test)]
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
        if let Some(action) = registry.core.get(shortcut) {
            let owner = match action {
                CoreShortcutAction::FocusAccount(position) => {
                    format!("多开核心账号位置 {position}")
                }
                CoreShortcutAction::ToggleMainWindow => "D2RHub 主面板切换动作".to_string(),
            };
            return Err(format!(
                "快捷键 {shortcut} 已由{owner}使用"
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
    hotkeys::validate(normalized.keys().cloned().collect())?;
    let generation = CAPABILITY_SHORTCUT_GENERATION.fetch_add(1, Ordering::Relaxed);
    registry.owners.insert(owner_id, (generation, normalized));
    drop(registry);
    refresh_input_services();
    Ok(CapabilityShortcutRegistration {
        owner_id,
        generation,
    })
}

fn dispatch_capability_shortcut(shortcut: &str) -> bool {
    let Some(registry) = capability_shortcuts().try_read() else {
        return false;
    };
    let delivery = registry
        .owners
        .values()
        .find_map(|(_, routes)| {
            routes
                .get(shortcut)
                .map(|route| (route.sender.clone(), route.action))
        });
    drop(registry);
    let Some((sender, action)) = delivery else {
        return false;
    };
    match sender {
        #[cfg(test)]
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
        drop(registry);
        refresh_input_services();
    }
}

pub fn set_bongo_cat_input_enabled(enabled: bool) {
    BONGO_CAT_INPUT_ENABLED.store(enabled, Ordering::Relaxed);
    refresh_input_services();
}

pub(crate) fn set_bongo_cat_input_visible_state(visible: bool) {
    BONGO_CAT_INPUT_VISIBLE.store(visible, Ordering::Relaxed);
    refresh_input_services();
}

#[tauri::command]
pub fn set_bongo_cat_input_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let state = app.state::<SharedState>();
    let _operation = state.optional_window_operations.try_lock()
        .ok_or_else(|| "辅助窗口操作进行中，请稍后重试".to_string())?;
    if visible {
        let installed = state.optional_runtime_ready() && state
            .configuration()
            .snapshot()
            .is_some_and(|config| {
                config.optional_module_runtime_allowed(crate::domain::config::OPTIONAL_MODULE_PET)
            });
        if !installed {
            return Err("桌宠模块尚未安装".to_string());
        }
    }
    set_bongo_cat_input_visible_state(visible);
    Ok(())
}


#[allow(dead_code)]
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

fn registered_shortcuts() -> Vec<String> {
    let registry = capability_shortcuts().read();
    let mut keys: Vec<_> = registry.core.keys().cloned().collect();
    if OPTIONAL_SHORTCUTS_ALLOWED.load(Ordering::Acquire) {
        keys.extend(registry.owners.values().flat_map(|(_, routes)| routes.keys().cloned()));
    }
    keys.sort();
    keys.dedup();
    keys
}

fn dispatch_registered_shortcut(app: &AppHandle, shortcut: &str) {
    if SHORTCUT_CAPTURE_ACTIVE.load(Ordering::Acquire) || hotkeys::is_suspended() { return; }
    let action = capability_shortcuts().read().core.get(shortcut).copied();
    match action {
        Some(CoreShortcutAction::ToggleMainWindow) => {
            crate::logger::log_msg("INFO", "Shortcut", &format!("系统热键触发：{shortcut} → 切换主面板"));
            crate::window_placement::toggle_main_window(app);
        }
        Some(CoreShortcutAction::FocusAccount(position)) => {
            if let Some(state) = app.try_state::<SharedState>() {
                if let Some(config) = state.configuration().snapshot() {
                    focus_account_at_position(app, &config.accounts_dir, position, shortcut);
                }
            }
        }
        None if OPTIONAL_SHORTCUTS_ALLOWED.load(Ordering::Acquire) => {
            dispatch_capability_shortcut(shortcut);
        }
        None => {}
    }
}

pub fn start_input_listener(app: AppHandle) {
    if let Ok(mut handle) = APP_HANDLE.lock() { *handle = Some(app.clone()); }
    if let Err(error) = hotkeys::initialize(app) {
        crate::logger::log_msg("ERROR", "Shortcut", &error);
    }
    runtime::initialize();
    refresh_input_services();
}

#[tauri::command(async)]
pub fn validate_shortcut_availability(
    app: AppHandle,
    shortcut: String,
    check_module_conflicts: Option<bool>,
) -> Result<(), String> {
    if shortcut.trim().is_empty() {
        return Err("请先输入快捷键".into());
    }
    hotkeys::initialize(app.clone())?;
    if check_module_conflicts.unwrap_or(false) {
        let state = app.state::<SharedState>();
        let config = state.configuration().snapshot()
            .ok_or_else(|| "配置尚未加载，请稍后重新录入快捷键".to_string())?;
        let normalized = crate::capabilities::room_automation::canonicalize_shortcut(&shortcut)
            .map_err(|error| format!("快捷键 {shortcut} 无效：{error}"))?;
        if config.optional_module_installed("room-automation") {
            // Read the saved projection even when Pure mode has no module runtime.
            let reserved = crate::capabilities::room_automation_config::persisted_shortcuts(
                &state.app_data_dir, config.preserved_unknown_fields.get("room_rotation"),
            ).map_err(|error| error.to_string())?;
            if reserved.iter().any(|key| key.eq_ignore_ascii_case(&normalized)) {
                return Err(format!("快捷键 {normalized} 已用于自动跟房模块，原设置保持不变"));
            }
        }
        return validate_core_shortcut_reservations_for_modules(
            [normalized.as_str()], &config.installed_optional_modules,
        );
    }
    hotkeys::validate(vec![shortcut])
}

#[tauri::command(async)]
pub fn set_shortcut_capture_active(active: bool) -> Result<(), String> {
    SHORTCUT_CAPTURE_ACTIVE.store(active, Ordering::Release);
    hotkeys::refresh_sync()
}

pub(crate) fn cancel_shortcut_capture() {
    SHORTCUT_CAPTURE_ACTIVE.store(false, Ordering::Release);
    hotkeys::refresh();
}

pub(crate) fn set_optional_shortcuts_allowed(allowed: bool) {
    OPTIONAL_SHORTCUTS_ALLOWED.store(allowed, Ordering::Release);
    refresh_input_services();
}

pub(crate) fn shutdown() {
    BONGO_CAT_INPUT_ENABLED.store(false, Ordering::Release);
    runtime::shutdown();
    hotkeys::shutdown();
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
