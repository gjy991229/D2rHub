use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub fn create_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItem::with_id(app, "show", "显示面板", true, None::<&str>)?;
    let recover_i = MenuItem::with_id(
        app,
        "recover-overlays",
        "找回所有悬浮窗",
        true,
        None::<&str>,
    )?;
    let quit_i = MenuItem::with_id(app, "quit", "退出 D2RHub", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &recover_i, &quit_i])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "应用托盘图标未配置"))?;

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                crate::window_placement::show_main_window_safely(app);
            }
            "recover-overlays" => {
                if let Err(error) =
                    crate::window_placement::recover_auxiliary_windows_for_app(app, "cursor")
                {
                    crate::logger::log_msg(
                        "WARN",
                        "WindowPlacement",
                        &format!("从托盘找回悬浮窗失败: {error}"),
                    );
                }
            }
            "quit" => {
                crate::capabilities::shutdown(app);
                app.exit(0);
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
                    if is_visible {
                        let _ = main_win.hide();
                    } else {
                        crate::window_placement::show_main_window_safely(app);
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
