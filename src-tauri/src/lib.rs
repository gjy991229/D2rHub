mod application;
mod audio_mod;
mod battle_net_config;
mod capabilities;
mod commands;
mod domain;
mod error;
mod input_listener;
mod launch_context;
pub mod logger;
mod rune_audio;
mod rune_data;
mod state;
mod stats;
mod stats_page;
#[cfg(target_os = "windows")]
mod token_registry_trace;
mod tray;
mod window_placement;

use crate::domain::config::GlobalConfig;
use state::AppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── 单实例检查：不允许同时运行多个 D2RHub ──
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn CreateMutexW(
                lpMutexAttributes: *const std::ffi::c_void,
                bInitialOwner: i32,
                lpName: *const u16,
            ) -> isize;
            fn GetLastError() -> u32;
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
            fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
            fn SetForegroundWindow(hWnd: isize) -> i32;
            fn IsIconic(hWnd: isize) -> i32;
        }
        const ERROR_ALREADY_EXISTS: u32 = 183;
        const SW_RESTORE: i32 = 9;

        let name: Vec<u16> = "D2RHub_SingleInstance_Mutex\0".encode_utf16().collect();
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if GetLastError() == ERROR_ALREADY_EXISTS {
                // 已有实例在运行 — 激活其窗口后退出
                let title: Vec<u16> = "D2RHub\0".encode_utf16().collect();
                let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
                if hwnd != 0 {
                    if IsIconic(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    }
                    SetForegroundWindow(hwnd);
                }
                std::process::exit(0);
            }
            // 泄漏句柄以保持互斥体在进程生命周期内有效
            let _ = Box::leak(Box::new(h));
        }
    }

    let _ = logger::init_logger();
    logger::log_msg("INFO", "System", "D2RHub starting up...");

    // Clean up any leftover old updater executables on startup (using a retry loop for Windows file locks/antivirus scan delays)
    if let Ok(current_exe) = std::env::current_exe() {
        let old_exe = current_exe.with_extension("exe.old");
        if old_exe.exists() {
            std::thread::spawn(move || {
                for _ in 0..15 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    if std::fs::remove_file(&old_exe).is_ok() {
                        logger::log_msg(
                            "INFO",
                            "System",
                            "Successfully cleaned up old executable.",
                        );
                        break;
                    }
                }
            });
        }
    }

    let app_state = Arc::new(AppState::new());

    // Load global config through the application transaction runtime so startup,
    // commands and background consumers all observe the same committed snapshot.
    match commands::global_config::load_global_config_into_state(&app_state) {
        Ok(_) => {}
        Err(error) => logger::log_msg(
            "ERROR",
            "Config",
            &format!("全局配置加载失败，为防止覆盖已停止自动初始化: {error}"),
        ),
    }

    // 从磁盘加载窗口几何并应用到初始窗口
    let geo = GlobalConfig::load_geometry(&app_state.app_data_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(app_state.clone())
        .setup(move |app| {
            let mut apply_default = true;
            if let Some(g) = &geo {
                if g.x > -32000 && g.y > -32000 && g.width > 100 && g.height > 100 {
                    apply_default = false;
                    if let Some(win) = app.get_webview_window("main") {
                        use tauri::LogicalPosition;
                        use tauri::LogicalSize;
                        let _ = win.set_position(LogicalPosition::new(g.x as f64, g.y as f64));
                        let _ = win.set_size(LogicalSize::new(g.width as f64, g.height as f64));
                    }
                }
            }
            if apply_default {
                if let Some(win) = app.get_webview_window("main") {
                    if let Ok(Some(monitor)) = win.current_monitor() {
                        let scale_factor = monitor.scale_factor();
                        let size = monitor.size();
                        let logical_width = (size.width as f64) / scale_factor;
                        let default_width = logical_width * 0.625;
                        let default_height = default_width * 0.656;
                        use tauri::LogicalSize;
                        let _ = win.set_size(LogicalSize::new(default_width, default_height));
                        let _ = win.center();
                    }
                }
            }
            window_placement::ensure_main_window_visible(app.handle());

            capabilities::install(app);

            // 初始化托盘
            let _ = tray::create_tray(app.handle());

            // 启动全局输入监听
            input_listener::start_input_listener(app.handle().clone());

            capabilities::start(app);

            Ok(())
        })
        // ── 全局配置 ──
        .invoke_handler(tauri::generate_handler![
            commands::global_config::get_global_config,
            commands::global_config::save_global_config,
            commands::global_config::patch_global_config,
            commands::global_config::save_window_geometry,
            commands::global_config::load_window_geometry,
            commands::global_config::save_overlay_geometry,
            commands::global_config::load_overlay_geometry,
            commands::global_config::save_stats_overlay_geometry,
            commands::global_config::load_stats_overlay_geometry,
            window_placement::restore_window_placement,
            window_placement::save_window_placement,
            window_placement::set_auxiliary_window_visible,
            window_placement::recover_auxiliary_windows,
            commands::global_config::save_theme,
            commands::global_config::detection::detect_saved_games_path,
            commands::global_config::detection::detect_global_saved_games_path,
            commands::global_config::check_saved_games_settings,
            commands::global_config::detection::detect_program_data_agent_path,
            commands::global_config::detection::detect_app_data_roaming_bnet_path,
            commands::global_config::detection::detect_browser_path,
            commands::global_config::detection::detect_browser_path_by_type,
            // ── 账号管理 ──
            commands::account::list_accounts,
            commands::account::get_account,
            commands::account::create_account,
            commands::account::update_account_meta,
            commands::account::update_account_region,
            commands::account::delete_account,
            commands::account::rename_account,
            commands::account::add_account_mod,
            commands::account::update_account_mods,
            commands::account::mark_settings_customized,
            commands::account::set_settings_customized,
            commands::account::set_account_window_position,
            commands::account::update_account_positions,
            commands::account::initialize_bnet_account,
            commands::account::reinitialize_account,
            commands::account::reorder_accounts,
            commands::account::get_account_dir_path,
            commands::account::open_account_dir,
            commands::account::move_game_window,
            commands::account_transfer::export_accounts,
            commands::account_transfer::import_accounts,
            // ── 启动引擎 ──
            commands::launch::launch_accounts,
            commands::launch::launch_battle_net_only,
            commands::launch::cancel_launch,
            // ── Settings 编辑器 ──
            commands::settings::get_account_settings,
            commands::settings::save_account_settings,
            commands::settings::get_game_settings,
            commands::settings::snapshot_system_settings_to_account,
            // ── 浏览器 ──
            commands::browser::launch_browser_for_account,
            commands::browser::open_url_in_browser,
            commands::browser::check_browser_running,
            commands::browser::kill_browser_processes,
            // ── 系统工具 ──
            commands::system::is_admin,
            commands::system::get_d2r_pids,
            commands::system::kill_all_d2r_processes,
            commands::system::bring_bnet_to_foreground,
            commands::system::bring_self_to_foreground,
            commands::system::bring_window_by_title_to_front,
            commands::system::get_foreground_window_title,
            commands::system::get_d2r_window_titles,
            commands::system::refresh_account_running_state,
            commands::system::check_game_connected,
            commands::system::send_keys_to_window,
            commands::system::snapshot_processes,
            commands::system::wait_for_new_process,
            commands::system::exit_app,
            commands::system::open_logs_dir,
            commands::system::open_user_guide,
            commands::terror_zone::get_terror_zone_snapshot,
            commands::terror_zone::get_next_terror_zone,
            // ── 声纹 Mod 一键准备 ──
            audio_mod::get_audio_mod_setup_state,
            audio_mod::prepare_audio_mod,
            audio_mod::upgrade_audio_mod,
            audio_mod::apply_audio_mod_to_account,
            // ── 符文音频声纹 ──
            rune_audio::monitor::start_rune_audio_monitor,
            rune_audio::monitor::restart_rune_audio_monitor,
            rune_audio::monitor::stop_rune_audio_monitor,
            rune_audio::monitor::get_rune_audio_status,
            rune_audio::monitor::start_rune_audio_diagnostic_recording,
            rune_audio::monitor::stop_rune_audio_diagnostic_recording,
            // ── 数据统计 ──
            stats::save_scene_record,
            stats::get_stats_data,
            stats::get_stats_json,
            stats::get_stats_page_preferences,
            stats::save_stats_page_preferences,
            stats::get_scene_avg_time,
            stats::get_scene_stats,
            stats::delete_scene_record,
            stats::open_stats_page,
            commands::system::get_app_version,
            commands::system::install_update,
            commands::system::check_cloud_version,
            commands::system::check_path_exists,
            logger::write_log,
            input_listener::set_bongo_cat_input_visible,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            logger::log_msg("ERROR", "System", &format!("Tauri run failed: {}", e));
            panic!("启动 D2RHub 时发生错误: {}", e);
        });
}
