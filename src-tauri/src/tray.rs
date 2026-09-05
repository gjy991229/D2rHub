use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

const TRAY_ID: &str = "main-tray";

#[derive(Default)]
struct TrayMenuState(parking_lot::Mutex<bool>);

fn build_menu(app: &AppHandle, optional_visible: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let show_i = MenuItem::with_id(app, "show", "显示面板", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出 D2RHub", true, None::<&str>)?;
    if optional_visible {
        let recover_i = MenuItem::with_id(app, "recover-overlays", "找回所有悬浮窗", true, None::<&str>)?;
        Menu::with_items(app, &[&show_i, &recover_i, &quit_i])
    } else {
        Menu::with_items(app, &[&show_i, &quit_i])
    }
}

/// Configuration observers hold their transaction lock. Read the latest
/// snapshot on a worker, never inline or on the UI thread from that observer.
pub(crate) fn schedule_menu_update(app: &AppHandle) {
    let app = app.clone();
    if let Err(error) = std::thread::Builder::new().name("tray-menu-update".to_string()).spawn(move || {
        let Some(menu_state) = app.try_state::<TrayMenuState>() else { return; };
        // Serialize updates and read *after* acquiring the lock. An old update
        // cannot restore an obsolete menu after a newer mode commit.
        let mut previous = menu_state.0.lock();
        let state = app.state::<crate::state::SharedState>();
        let visible = state.optional_runtime_ready() && state.configuration().snapshot()
            .is_some_and(|config| config.optional_features_runtime_allowed());
        if visible == *previous { return; }
        let Some(tray) = app.tray_by_id(TRAY_ID) else { return; };
        match build_menu(&app, visible).and_then(|menu| tray.set_menu(Some(menu))) {
            Ok(()) => *previous = visible,
            Err(error) => crate::logger::log_msg("WARN", "Tray", &format!("更新托盘菜单失败：{error}")),
        }
    }) {
        crate::logger::log_msg("WARN", "Tray", &format!("启动托盘菜单更新失败：{error}"));
    }
}

pub fn create_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(TrayMenuState::default());
    // Startup is gated by mode selection and disclosure, including old configs.
    let menu = build_menu(app, false)?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "应用托盘图标未配置"))?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                crate::window_placement::show_main_window_safely(app);
            }
            "recover-overlays" => {
                let recover_app = app.clone();
                if let Err(error) = std::thread::Builder::new()
                    .name("recover-overlay-windows".to_string())
                    .spawn(move || {
                        if let Err(error) =
                            crate::window_placement::recover_auxiliary_windows_for_app(
                                &recover_app,
                                "cursor",
                            )
                        {
                            crate::logger::log_msg(
                                "WARN",
                                "WindowPlacement",
                                &format!("从托盘找回悬浮窗失败: {error}"),
                            );
                        }
                    })
                {
                    crate::logger::log_msg(
                        "WARN",
                        "WindowPlacement",
                        &format!("无法启动悬浮窗找回任务: {error}"),
                    );
                }
            }
            "quit" => {
                crate::infrastructure::system::exit_app(app.clone());
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(main_win) = app.get_webview_window("main") {
                    let is_visible = main_win.is_visible().unwrap_or(false);
                    if is_visible && !main_win.is_minimized().unwrap_or(false) {
                        crate::window_placement::hide_main_window_to_tray(app);
                    } else {
                        crate::window_placement::show_main_window_safely(app);
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
