const APP_COMMANDS: &[&str] = &[
    "get_global_config",
    "get_capability_statuses",
    "get_capability_descriptors",
    "get_tasks",
    "get_task",
    "get_task_timeline",
    "get_task_retry_descriptor",
    "cancel_task",
    "retry_task",
    "export_diagnostic_bundle",
    "room_automation_get_config",
    "room_automation_save_config",
    "room_automation_get_status",
    "room_automation_start_primary",
    "room_automation_start_followers",
    "room_automation_retry",
    "room_automation_cancel",
    "room_automation_get_chat_binding",
    "room_automation_install_chat_binding",
    "room_automation_restore_chat_binding",
    "save_global_config",
    "patch_global_config",
    "patch_desktop_pet_settings",
    "save_window_geometry",
    "load_window_geometry",
    "save_overlay_geometry",
    "load_overlay_geometry",
    "save_stats_overlay_geometry",
    "load_stats_overlay_geometry",
    "restore_window_placement",
    "save_window_placement",
    "set_auxiliary_window_visible",
    "recover_auxiliary_windows",
    "save_theme",
    "detect_saved_games_path",
    "detect_global_saved_games_path",
    "check_saved_games_settings",
    "detect_program_data_agent_path",
    "detect_app_data_roaming_bnet_path",
    "detect_browser_path",
    "detect_browser_path_by_type",
    "list_accounts",
    "get_account",
    "create_account",
    "update_account_meta",
    "update_account_region",
    "delete_account",
    "rename_account",
    "mark_settings_customized",
    "set_settings_customized",
    "set_account_window_position",
    "update_account_positions",
    "initialize_bnet_account",
    "reinitialize_account",
    "reorder_accounts",
    "get_account_dir_path",
    "open_account_dir",
    "move_game_window",
    "export_accounts",
    "import_accounts",
    "launch_accounts",
    "launch_battle_net_only",
    "cancel_launch",
    "get_account_settings",
    "save_account_settings",
    "get_game_settings",
    "snapshot_system_settings_to_account",
    "launch_browser_for_account",
    "open_token_login_url",
    "open_url_in_browser",
    "check_browser_running",
    "kill_browser_processes",
    "is_admin",
    "get_d2r_pids",
    "kill_all_d2r_processes",
    "bring_bnet_to_foreground",
    "bring_self_to_foreground",
    "bring_window_by_title_to_front",
    "get_foreground_window_title",
    "get_d2r_window_titles",
    "refresh_account_running_state",
    "check_game_connected",
    "send_keys_to_window",
    "snapshot_processes",
    "wait_for_new_process",
    "exit_app",
    "open_logs_dir",
    "open_user_guide",
    "activate_application_runtime",
    "get_terror_zone_snapshot",
    "get_next_terror_zone",
    "get_audio_mod_setup_state",
    "get_mod_capsule_pool",
    "scan_mod_capsule_pool",
    "open_mods_directory",
    "set_mod_auto_exit_on_death_enabled",
    "add_mod_capsule",
    "update_mod_capsule",
    "delete_mod_capsule",
    "assign_mod_capsule_to_account",
    "prepare_audio_mod",
    "upgrade_audio_mod",
    "apply_audio_mod_to_account",
    "start_rune_audio_monitor",
    "restart_rune_audio_monitor",
    "stop_rune_audio_monitor",
    "get_rune_audio_status",
    "start_rune_audio_diagnostic_recording",
    "stop_rune_audio_diagnostic_recording",
    "save_scene_record",
    "get_stats_data",
    "get_stats_json",
    "get_stats_page_preferences",
    "save_stats_page_preferences",
    "get_scene_avg_time",
    "get_scene_stats",
    "delete_scene_record",
    "open_stats_page",
    "get_app_version",
    "install_update",
    "check_cloud_version",
    "check_path_exists",
    "write_log",
    "set_bongo_cat_input_visible",
    "set_stats_overlay_mini_input_region",
];

fn main() {
    validate_command_surfaces();

    // Read version from config file
    let version = get_version_from_config();
    println!("cargo:rustc-env=APP_VERSION={}", version);

    let mut windows = tauri_build::WindowsAttributes::new();

    // Define the manifest with requireAdministrator
    let manifest = r#"
        <assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
            <dependency>
                <dependentAssembly>
                    <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
                </dependentAssembly>
            </dependency>
            <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
                <security>
                    <requestedPrivileges>
                        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
                    </requestedPrivileges>
                </security>
            </trustInfo>
        </assembly>
    "#;

    windows = windows.app_manifest(manifest);

    let app_manifest = tauri_build::AppManifest::new().commands(APP_COMMANDS);
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(windows)
            .app_manifest(app_manifest),
    )
    .expect("failed to run build script");
}

fn validate_command_surfaces() {
    use std::collections::BTreeSet;

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=permissions/window-commands.toml");
    println!("cargo:rerun-if-changed=../src/platform/tauri/contracts.ts");

    let declared = APP_COMMANDS
        .iter()
        .map(|command| (*command).to_string())
        .collect::<BTreeSet<_>>();

    let lib_source = std::fs::read_to_string("src/lib.rs")
        .expect("failed to read src/lib.rs while validating Tauri commands");
    let handler_block = delimited_block(
        &lib_source,
        "tauri::generate_handler![",
        "])",
        "tauri::generate_handler!",
    );
    let registered = handler_block
        .lines()
        .filter_map(|line| {
            let candidate = line.trim().trim_end_matches(',');
            if candidate.is_empty() || candidate.starts_with("//") {
                return None;
            }
            let command = candidate.rsplit("::").next()?;
            command
                .chars()
                .all(|character| character == '_' || character.is_ascii_alphanumeric())
                .then(|| command.to_string())
        })
        .collect::<BTreeSet<_>>();
    assert_same_commands("invoke handler and APP_COMMANDS", &registered, &declared);

    let permission_source = std::fs::read_to_string("permissions/window-commands.toml")
        .expect("failed to read permissions/window-commands.toml while validating Tauri commands");
    let main_permission = permission_source
        .split_once("identifier = \"main-window-commands\"")
        .map(|(_, section)| section)
        .expect("main-window-commands permission is missing");
    let main_acl = quoted_commands(delimited_block(
        main_permission,
        "commands.allow = [",
        "]",
        "main-window-commands allow list",
    ));
    assert_same_commands(
        "APP_COMMANDS and main-window-commands ACL",
        &declared,
        &main_acl,
    );

    let contract_source = std::fs::read_to_string("../src/platform/tauri/contracts.ts")
        .expect("failed to read frontend Tauri command contract");
    let frontend_commands = quoted_commands(delimited_block(
        &contract_source,
        "export const TAURI_COMMANDS = [",
        "] as const",
        "frontend TAURI_COMMANDS",
    ));
    let unavailable = frontend_commands
        .difference(&declared)
        .cloned()
        .collect::<Vec<_>>();
    if !unavailable.is_empty() {
        panic!(
            "frontend TAURI_COMMANDS contains unavailable commands: {}",
            unavailable.join(", ")
        );
    }
}

fn delimited_block<'a>(source: &'a str, start: &str, end: &str, label: &str) -> &'a str {
    let (_, remainder) = source
        .split_once(start)
        .unwrap_or_else(|| panic!("{label} start marker is missing"));
    remainder
        .split_once(end)
        .map(|(block, _)| block)
        .unwrap_or_else(|| panic!("{label} end marker is missing"))
}

fn quoted_commands(source: &str) -> std::collections::BTreeSet<String> {
    source
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn assert_same_commands(
    label: &str,
    expected: &std::collections::BTreeSet<String>,
    actual: &std::collections::BTreeSet<String>,
) {
    let missing = expected.difference(actual).cloned().collect::<Vec<_>>();
    let extra = actual.difference(expected).cloned().collect::<Vec<_>>();
    if !missing.is_empty() || !extra.is_empty() {
        panic!(
            "{label} differ; missing: [{}]; extra: [{}]",
            missing.join(", "),
            extra.join(", ")
        );
    }
}

fn get_version_from_config() -> String {
    let content = std::fs::read_to_string("tauri.conf.json").unwrap_or_default();
    if let Some(pos) = content.find("\"version\"") {
        let after_key = &content[pos + 9..];
        if let Some(colon_pos) = after_key.find(':') {
            let after_colon = &after_key[colon_pos + 1..];
            if let Some(quote_start) = after_colon.find('"') {
                let after_quote = &after_colon[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    return after_quote[..quote_end].trim().to_string();
                }
            }
        }
    }
    "0.1.0".to_string()
}
