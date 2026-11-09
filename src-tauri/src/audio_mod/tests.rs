//! Cross-adapter acceptance tests for Mod processing, trust and recovery.
use super::filesystem::{
    find_existing_mod_name, traverse_safe_directory_tree, SafeTreeNodeKind, TemporaryDirectory,
};
use super::replacement::{
    replace_audio_mod_directory, replace_journal_paths_are_valid, write_replace_journal,
    write_replace_journal_with_stage_sync,
};
use super::validation::layouts::{
    validate_auto_exit_on_death_layouts, validate_in_game_room_tool_layouts,
    NEXT_GAME_TOOLTIP_OFFSET_Y, ROOM_TOOL_BUTTON_SCALE, ROOM_TOOL_BUTTON_Y, ROOM_TOOL_CREATE_X,
    ROOM_TOOL_JOIN_X, ROOM_TOOL_LAYOUT_DIRECTORY, ROOM_TOOL_NEXT_X,
};
use super::validation::{
    compatibility, resolve_source_directory, validate_audio_mod, validate_audio_mod_credential,
    validate_generator_output, validate_recoverable_audio_mod_directory, LEGACY_MANIFEST_FILE_NAME,
    REQUIRED_AUDIO_MOD_RECIPE_VERSION,
};
use super::{
    active_mod_name, arguments_with_audio_mod, installed_mods, recover_audio_mod_replacements,
    require_verified_running_session, set_auto_exit_on_death_enabled,
};
use crate::domain::mod_arguments::{generated_audio_mod_name, has_txt_argument};
use crate::domain::mod_processing::{
    validate_preserved_feature_groups, GeneratorFeatureGroup, GeneratorReport,
    RequestedFeatureGroups, AUDIO_TELEMETRY_FEATURE_ID, AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
    AUTO_EXIT_ON_DEATH_FEATURE_ID, IN_GAME_ROOM_TOOLS_FEATURE_ID,
    IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
};
use crate::domain::mod_processing::{
    AUTO_EXIT_ON_DEATH_FINGERPRINT, AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT,
};
use crate::rune_audio::{
    catalog::AREA_CATALOG_FILE_NAME, item_catalog::ITEM_CATALOG_FILE_NAME,
    protocol::PROTOCOL_VERSION,
};

const TEST_TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

fn mod_use_fixture() -> (
    TemporaryDirectory,
    crate::state::SharedState,
    crate::domain::config::GlobalConfig,
) {
    let root = TemporaryDirectory::create(test_mods_directory("mod_use")).unwrap();
    let config = crate::domain::config::GlobalConfig {
        accounts_dir: root.path().join("accounts").to_string_lossy().into_owned(),
        cn_game_path: root.path().join("cn").to_string_lossy().into_owned(),
        global_game_path: root.path().join("global").to_string_lossy().into_owned(),
        ..Default::default()
    };
    std::fs::create_dir(root.path().join("accounts")).unwrap();
    let state = std::sync::Arc::new(crate::state::AppState::new());
    (root, state, config)
}

#[test]
fn external_mod_guard_uses_mutation_directory_instead_of_recognition_target() {
    let (_root, state, mut config) = mod_use_fixture();
    config.rune_audio_external_target = Some(crate::domain::config::ExternalAudioTarget {
        edition: "Global".into(),
        mod_name: "Audio".into(),
    });
    let cn = std::path::Path::new(&config.cn_game_path);
    let global = std::path::Path::new(&config.global_game_path);
    let external_use = |directory: &std::path::Path, name: &str| {
        assert_eq!(name, "Audio");
        if directory == global {
            Err("Global game is using Audio".to_string())
        } else {
            Ok(())
        }
    };
    super::ensure_audio_mod_not_in_use_with(&state, &config, cn, "Audio", external_use).unwrap();
    assert_eq!(
        super::ensure_audio_mod_not_in_use_with(&state, &config, global, "Audio", external_use),
        Err("Global game is using Audio".to_string())
    );
}

#[test]
fn external_mod_guard_does_not_depend_on_recognition_preferences() {
    let (_root, state, mut config) = mod_use_fixture();
    for target in [
        None,
        Some(crate::domain::config::ExternalAudioTarget {
            edition: "Global".into(),
            mod_name: "Other".into(),
        }),
    ] {
        config.rune_audio_external_target = target;
        let cn = std::path::Path::new(&config.cn_game_path);
        assert_eq!(
            super::ensure_audio_mod_not_in_use_with(
                &state,
                &config,
                cn,
                "Audio",
                |directory, name| {
                    assert_eq!(directory, cn);
                    assert_eq!(name, "Audio");
                    Err("CN game is using Audio".to_string())
                }
            ),
            Err("CN game is using Audio".to_string())
        );
    }
}

#[test]
fn registered_mod_use_is_scoped_to_installation_and_active_launch_arguments() {
    let (root, state, config) = mod_use_fixture();
    for (id, region, saved_mod, launched_mod, pid) in [
        ("acount1", "CN", "Other", "Audio", 1),
        ("acount2", "KR", "Audio", "Audio", 2),
        ("acount3", "CN", "Audio", "Other", 3),
    ] {
        let mut account = crate::domain::account::AccountMeta::new(id);
        account.display_name = id.into();
        account.region = Some(region.into());
        account.mod_args = format!("-mod {saved_mod} -txt");
        account.initialized = true;
        std::fs::create_dir(root.path().join("accounts").join(id)).unwrap();
        crate::commands::account::AccountManager::save_meta(&config.accounts_dir, &account)
            .unwrap();
        state.multi_instance().instances().record_launched(
            id,
            pid,
            &format!("-mod {launched_mod} -txt"),
        );
    }
    for (directory, account_id, pid) in [
        (&config.cn_game_path, "acount1", 1),
        (&config.global_game_path, "acount2", 2),
    ] {
        let directory = std::path::Path::new(directory);
        assert_eq!(
            super::running_accounts_using_mod(&state, &config, directory, "audio"),
            vec![(account_id.to_string(), pid)]
        );
        let error =
            super::ensure_audio_mod_not_in_use_with(&state, &config, directory, "audio", |_, _| {
                panic!("the registered active session already blocks this mutation")
            })
            .unwrap_err();
        assert!(error.contains(account_id));
        assert!(!error.contains("acount3"));
    }
}

fn write_test_audio_mod(
    mods_directory: &std::path::Path,
    mod_name: &str,
    manifest: serde_json::Value,
) {
    let mod_directory = mods_directory.join(mod_name);
    std::fs::create_dir_all(mod_directory.join(format!("{mod_name}.mpq"))).unwrap();
    std::fs::write(
        mod_directory.join("audio-telemetry-manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let catalog = serde_json::to_vec(&serde_json::json!({
        "protocol_version": PROTOCOL_VERSION
    }))
    .unwrap();
    std::fs::write(mod_directory.join(AREA_CATALOG_FILE_NAME), &catalog).unwrap();
    std::fs::write(mod_directory.join(ITEM_CATALOG_FILE_NAME), &catalog).unwrap();
}

fn test_mods_directory(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("d2rhub_audio_mod_{label}_{}", uuid::Uuid::new_v4()))
}

fn test_audio_fingerprint() -> String {
    format!(
        "audio-v{AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION};protocol={PROTOCOL_VERSION};areas=all_areas;track=charms,essences,gems,jewels,keys,organs,runes;gain_mdb=0"
    )
}

fn current_audio_manifest(mod_name: &str) -> serde_json::Value {
    serde_json::json!({
        "manifest_format": "d2r-audio-telemetry-mod",
        "producer": "d2r-audio-mod",
        "protocol_version": PROTOCOL_VERSION,
        "recipe_version": REQUIRED_AUDIO_MOD_RECIPE_VERSION,
        "build_mode": "minimal",
        "mod_name": mod_name,
        "feature_groups": [{
            "id": AUDIO_TELEMETRY_FEATURE_ID,
            "recipe_version": AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
            "fingerprint": test_audio_fingerprint()
        }]
    })
}

#[test]
fn installed_legacy_loot_credentials_remain_usable_with_audio() {
    let root = test_mods_directory("legacy_loot_credentials");
    for (recipe, fingerprint) in [
        (2, "party-loot-v2;native-party-row;jcy=1;launch=immediate"),
        (
            3,
            "party-loot-v3;native-party-row;portrait-position;launch=immediate",
        ),
    ] {
        let name = format!("legacy-loot-{recipe}");
        let mut manifest = current_audio_manifest(&name);
        manifest["feature_groups"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "party_loot", "recipe_version": recipe, "fingerprint": fingerprint
            }));
        write_test_audio_mod(&root, &name, manifest.clone());
        assert!(validate_audio_mod_credential(&root, &name).is_ok());
        manifest["feature_groups"][1]["fingerprint"] = serde_json::json!("forged");
        write_test_audio_mod(&root, &name, manifest);
        assert!(validate_audio_mod_credential(&root, &name).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}

fn write_test_room_tool_layouts(mods_directory: &std::path::Path, mod_name: &str) {
    let layouts = mods_directory
        .join(mod_name)
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    std::fs::create_dir_all(&layouts).unwrap();
    let pause_layout = || {
        serde_json::json!({
            "fields": {"defaultWidget": "D2RHubKeyboardGatewayHub"},
            "children": [
                {"type": "ButtonWidget", "name": "D2RHubKeyboardGatewayHub", "fields": {
                    "acceptsReturnKey": false,
                    "acceptsEscKeyEverywhere": false,
                    "navigation": {
                        "left": {"name": "D2RHubKeyboardCreateGateway"},
                        "right": {"name": "D2RHubKeyboardJoinGateway"}
                    }
                }},
                {"type": "ButtonWidget", "name": "ReturnToGame", "fields": {
                    "acceptsEscKeyEverywhere": true,
                    "onClickMessage": "PanelManager:OpenPanel:D2RHubPauseReturnToGame",
                    "navigation": {
                    "left": {"name": "D2RHubKeyboardCreateGateway"},
                    "right": {"name": "D2RHubKeyboardJoinGateway"}
                }}},
                {"type": "ButtonWidget", "name": "D2RHubKeyboardCreateGateway", "fields": {
                    "acceptsReturnKey": true,
                    "acceptsEscKeyEverywhere": false,
                    "navigation": {
                        "left": {"name": "D2RHubKeyboardCreateGateway"},
                        "right": {"name": "D2RHubKeyboardGatewayHub"}
                    },
                    "onClickMessage": "PanelManager:OpenPanel:D2RHubKeyboardOpenCreate"
                }},
                {"type": "ButtonWidget", "name": "D2RHubKeyboardJoinGateway", "fields": {
                    "acceptsReturnKey": true,
                    "acceptsEscKeyEverywhere": false,
                    "navigation": {
                        "left": {"name": "D2RHubKeyboardGatewayHub"},
                        "right": {"name": "D2RHubKeyboardJoinGateway"}
                    },
                    "onClickMessage": "PanelManager:OpenPanel:D2RHubKeyboardOpenJoin"
                }}
            ]
        })
    };
    for (name, document) in [
        (
            "HudWarningshd.json",
            serde_json::json!({"children": [{"fields": {"message": "PanelManager:ClosePanel:D2RHubRoomToolbar"}}]}),
        ),
        (
            "D2RHubPauseReturnToGamehd.json",
            serde_json::json!({"children": [
                {"fields": {"time": 0.001, "message": "PanelManager:ClosePanel:D2RHubQuickRecreateEscArm"}},
                {"fields": {"time": 0.005, "message": "PausePanelMessage:Close"}}
            ]}),
        ),
        (
            "D2RHubQuickRecreateEscArmhd.json",
            serde_json::json!({
                "type": "TooltipsPanel",
                "fields": {"priority": 9002},
                "children": [
                    {"name": "D2RHubEscNextGame", "fields": {
                        "acceptsEscKeyEverywhere": true,
                        "acceptsReturnKey": false,
                        "onClickMessage": "PanelManager:OpenPanel:D2RHubQuickRecreate"
                    }},
                    {"fields": {"time": 0.5, "message": "PanelManager:ClosePanel:D2RHubQuickRecreateEscArm"}}
                ]
            }),
        ),
        ("pauselayouthd.json", pause_layout()),
        ("pauselayoutgardenhd.json", pause_layout()),
        (
            "D2RHubRoomToolbarhd.json",
            serde_json::json!({"fields": {"rect": {"x": -9999, "y": -9999}}, "children": [
                {"name": "D2RHubNextGame", "fields": {
                    "rect": {"x": ROOM_TOOL_NEXT_X, "y": ROOM_TOOL_BUTTON_Y, "scale": ROOM_TOOL_BUTTON_SCALE},
                    "tooltipString": "左键双击进入下一局",
                    "tooltipOffset": {"y": NEXT_GAME_TOOLTIP_OFFSET_Y},
                    "onClickMessage": "PanelManager:OpenPanel:D2RHubQuickRecreateArm"
                }},
                {"name": "D2RHubCreateGame", "fields": {
                    "rect": {"x": ROOM_TOOL_CREATE_X, "y": ROOM_TOOL_BUTTON_Y, "scale": ROOM_TOOL_BUTTON_SCALE},
                    "onClickMessage": "PanelManager:OpenPanel:D2RHubOpenCreateGame"
                }},
                {"name": "D2RHubJoinGame", "fields": {
                    "rect": {"x": ROOM_TOOL_JOIN_X, "y": ROOM_TOOL_BUTTON_Y, "scale": ROOM_TOOL_BUTTON_SCALE},
                    "onClickMessage": "PanelManager:OpenPanel:D2RHubOpenJoinGame"
                }}
            ]}),
        ),
        (
            "D2RHubQuickRecreateArmhd.json",
            serde_json::json!({
                "type": "TooltipsPanel",
                "children": [
                    {"name": "D2RHubArmedNextGame", "fields": {
                        "rect": {"x": ROOM_TOOL_NEXT_X, "y": ROOM_TOOL_BUTTON_Y, "scale": ROOM_TOOL_BUTTON_SCALE},
                        "onClickMessage": "PanelManager:OpenPanel:D2RHubQuickRecreate"
                    }},
                    {"fields": {"time": 0.5, "message": "PanelManager:ClosePanel:D2RHubQuickRecreateArm"}}
                ]
            }),
        ),
        (
            "D2RHubQuickRecreatehd.json",
            serde_json::json!({"children": [
                {"fields": {"time": 0.01, "message": "PanelManager:OpenPanel:PauseLayoutGarden"}},
                {"fields": {"time": 0.05, "message": "PausePanelMessage:ExitGame"}},
                {"fields": {"time": 0.05, "message": "CharacterSelect:LoadCharacter:2"}},
                {"fields": {"time": 0.05, "message": "PanelManager:ClosePanel:D2RHubQuickRecreate"}}
            ]}),
        ),
        (
            "D2RHubCommitCreateGamehd.json",
            serde_json::json!({"children": [
                {"fields": {"time": 0.01, "message": "PanelManager:OpenPanel:PauseLayoutGarden"}},
                {"fields": {"time": 0.05, "message": "PausePanelMessage:ExitGame"}},
                {"fields": {"time": 0.05, "message": "CreateGame:CreateGame"}},
                {"fields": {"time": 0.05, "message": "PanelManager:ClosePanel:D2RHubCommitCreateGame"}}
            ]}),
        ),
        (
            "D2RHubCommitJoinGamehd.json",
            serde_json::json!({"children": [
                {"fields": {"time": 0.01, "message": "PanelManager:OpenPanel:PauseLayoutGarden"}},
                {"fields": {"time": 0.05, "message": "PausePanelMessage:ExitGame"}},
                {"fields": {"time": 0.05, "message": "JoinGame:JoinGame"}},
                {"fields": {"time": 0.05, "message": "PanelManager:ClosePanel:D2RHubCommitJoinGame"}}
            ]}),
        ),
        (
            "D2RHubOpenCreateGamehd.json",
            serde_json::json!({"children": [
                {"fields": {"time": 0.1, "message": "PanelManager:TogglePanel:D2RHubInGameCreateGame"}},
                {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubInGameJoinGame"}},
                {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubOpenCreateGame"}}
            ]}),
        ),
        (
            "D2RHubOpenJoinGamehd.json",
            serde_json::json!({"children": [
                {"fields": {"time": 0.1, "message": "PanelManager:TogglePanel:D2RHubInGameJoinGame"}},
                {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubInGameCreateGame"}},
                {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubOpenJoinGame"}}
            ]}),
        ),
        (
            "D2RHubKeyboardOpenCreatehd.json",
            serde_json::json!({"children": [
                {"fields": {"time": 0.005, "message": "PausePanelMessage:Close"}},
                {"fields": {"time": 0.1, "message": "PanelManager:TogglePanel:D2RHubInGameCreateGame"}},
                {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubInGameJoinGame"}},
                {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubKeyboardOpenCreate"}}
            ]}),
        ),
        (
            "D2RHubKeyboardOpenJoinhd.json",
            serde_json::json!({"children": [
                {"fields": {"time": 0.005, "message": "PausePanelMessage:Close"}},
                {"fields": {"time": 0.1, "message": "PanelManager:TogglePanel:D2RHubInGameJoinGame"}},
                {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubInGameCreateGame"}},
                {"fields": {"time": 0.1, "message": "PanelManager:ClosePanel:D2RHubKeyboardOpenJoin"}}
            ]}),
        ),
        (
            "D2RHubInGameCreateGamehd.json",
            serde_json::json!({
                "type": "CreateGamePanel", "name": "D2RHubInGameCreateGame",
                "fields": {"defaultWidget": "GameNameInput", "isDismissable": true, "acceptsEscKeyEverywhere": true},
                "children": [
                    {"name": "GameNameInput", "fields": {
                        "imeEnabled": true,
                        "onReturnInputMessage": "PanelManager:OpenPanel:D2RHubCommitCreateGame"
                    }},
                    {"name": "PasswordInput", "fields": {"imeEnabled": true}},
                    {"name": "DescriptionInput", "fields": {"imeEnabled": true}},
                    {"type": "TimerWidget", "name": "D2RHubDefaultHell", "fields": {
                        "time": 0.05, "message": "CreateGame:SetDifficulty:2"
                    }},
                    {"name": "D2RHubCloseRoomForm", "fields": {"onClickMessage": "PanelManager:ClosePanel:D2RHubInGameCreateGame"}}
                ]
            }),
        ),
        (
            "D2RHubInGameJoinGamehd.json",
            serde_json::json!({
                "type": "JoinGamePanel", "name": "D2RHubInGameJoinGame",
                "fields": {"defaultWidget": "NameInput", "isDismissable": true, "acceptsEscKeyEverywhere": true},
                "children": [
                    {"name": "NameInput", "fields": {
                        "imeEnabled": true,
                        "onReturnInputMessage": "JoinGame:JoinGame"
                    }},
                    {"name": "PasswordInput", "fields": {"imeEnabled": true}},
                    {"name": "D2RHubCloseRoomForm", "fields": {"onClickMessage": "PanelManager:ClosePanel:D2RHubInGameJoinGame"}}
                ]
            }),
        ),
    ] {
        std::fs::write(layouts.join(name), serde_json::to_vec(&document).unwrap()).unwrap();
    }
    // r26 keeps native lobby submissions separate from the in-game exit chain.
    for (source, target, native_panel, routed_submit, native_submit) in [
        (
            "D2RHubInGameCreateGamehd.json",
            "creategamepanelhd.json",
            "CreateGamePanel",
            "PanelManager:OpenPanel:D2RHubCommitCreateGame",
            "CreateGame:CreateGame",
        ),
        (
            "D2RHubInGameJoinGamehd.json",
            "joingamepanelhd.json",
            "JoinGamePanel",
            "PanelManager:OpenPanel:D2RHubCommitJoinGame",
            "JoinGame:JoinGame",
        ),
    ] {
        let mut lobby: serde_json::Value =
            serde_json::from_slice(&std::fs::read(layouts.join(source)).unwrap()).unwrap();
        lobby["name"] = serde_json::json!(native_panel);
        lobby["children"]
            .as_array_mut()
            .unwrap()
            .retain(|child| child["name"] != "D2RHubDefaultHell");
        lobby["children"][0]["fields"]["onReturnInputMessage"] = serde_json::json!(native_submit);
        let close = lobby["children"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap();
        close["fields"]["onClickMessage"] =
            serde_json::json!(format!("PanelManager:ClosePanel:{native_panel}"));
        let serialized = serde_json::to_vec(&lobby).unwrap();
        assert!(!String::from_utf8_lossy(&serialized).contains(routed_submit));
        std::fs::write(layouts.join(target), serialized).unwrap();
    }
    std::fs::write(
        layouts.join("lobbybackgroundpanelhd.json"),
        serde_json::to_vec(&serde_json::json!({"children": [{
            "type": "TextBoxWidget",
            "name": "D2RHubLobbyReturnHint",
            "fields": {
                "rect": {"x": -50, "y": 0},
                "text": "按 Esc 键返回",
                "style": {
                    "alignment": {"h": "center", "v": "center"},
                    "fontColor": "$FontColorDarkGold",
                    "pointSize": 120
                }
            }
        }]}))
        .unwrap(),
    )
    .unwrap();
}

fn write_test_auto_exit_on_death_layouts(
    mods_directory: &std::path::Path,
    mod_name: &str,
    enabled: bool,
) {
    let layouts = mods_directory
        .join(mod_name)
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    std::fs::create_dir_all(&layouts).unwrap();
    let death_children = if enabled {
        serde_json::json!([{
            "type": "TimerWidget",
            "name": "D2RHubAutoExitOnDeathLauncher",
            "fields": {
                "time": 0.01,
                "message": "PanelManager:OpenPanel:D2RHubAutoExitOnDeath"
            }
        }])
    } else {
        serde_json::json!([])
    };
    for (name, document) in [
        (
            "youdiedmodalhd.json",
            serde_json::json!({
                "type": "YouDiedModal",
                "name": "YouDiedModal",
                "children": death_children
            }),
        ),
        (
            "D2RHubAutoExitOnDeathhd.json",
            serde_json::json!({
                "type": "PausePanel",
                "name": "D2RHubAutoExitOnDeath",
                "children": [{
                    "type": "TimerWidget",
                    "name": "D2RHubAutoExitOnDeathCommit",
                    "fields": {
                        "time": 0.1,
                        "message": "PausePanelMessage:ExitGame"
                    }
                }]
            }),
        ),
        (
            "D2RHubAutoExitOnDeath.json",
            serde_json::json!({
                "type": "Panel",
                "name": "D2RHubAutoExitOnDeath"
            }),
        ),
    ] {
        std::fs::write(layouts.join(name), serde_json::to_vec(&document).unwrap()).unwrap();
    }
}

#[test]
fn rewrites_only_mod_and_txt_arguments() {
    let result = arguments_with_audio_mod(
        r#"-w -mod "Old Mod" -assettestmode 1 --label "hello world""#,
        "jcy-D2RHubAudio",
    )
    .unwrap();
    assert_eq!(
        active_mod_name(&result).unwrap().as_deref(),
        Some("jcy-D2RHubAudio")
    );
    assert!(has_txt_argument(&result).unwrap());
    assert!(result.contains("-w"));
    assert!(result.contains("-assettestmode 1"));
    assert!(result.contains("--label \"hello world\""));
    assert!(!result.contains("Old Mod"));
}

#[test]
fn adds_audio_arguments_to_an_original_profile() {
    let result = arguments_with_audio_mod("-w", "D2RHubAudio").unwrap();
    assert_eq!(result, "-w -mod D2RHubAudio -txt -assettestmode 1");
}

#[test]
fn room_tools_require_a_running_pid_with_a_matching_launch_snapshot() {
    assert!(
        require_verified_running_session("offline", ("-mod new".into(), None, false))
            .unwrap_err()
            .contains("没有由 D2RHub 确认的运行实例")
    );
    let discovered = require_verified_running_session(
        "discovered",
        ("-mod persisted-new".into(), Some(42), false),
    )
    .unwrap_err();
    assert!(discovered.contains("PID 42"));
    assert!(discovered.contains("可信启动快照"));
    let trusted = require_verified_running_session(
        "trusted",
        ("-mod actually-running -txt".into(), Some(43), true),
    )
    .unwrap();
    assert_eq!(trusted, ("-mod actually-running -txt".to_string(), 43));
}

#[test]
fn death_auto_exit_is_an_independent_verified_feature_group() {
    let requested = RequestedFeatureGroups::from_options(
        Some(false),
        Some(false),
        Some(false),
        None,
        None,
        None,
        Some(true),
    )
    .unwrap();
    assert_eq!(requested.generator_value(), "death-exit");

    let root = test_mods_directory("death_auto_exit");
    let mod_name = "DeathExit";
    write_test_audio_mod(
        &root,
        mod_name,
        serde_json::json!({
            "manifest_format": "d2r-audio-telemetry-mod",
            "producer": "d2r-audio-mod",
            "protocol_version": PROTOCOL_VERSION,
            "recipe_version": REQUIRED_AUDIO_MOD_RECIPE_VERSION,
            "build_mode": "minimal",
            "mod_name": mod_name,
            "feature_groups": [{
                "id": AUTO_EXIT_ON_DEATH_FEATURE_ID,
                "recipe_version": 1,
                "fingerprint": AUTO_EXIT_ON_DEATH_FINGERPRINT
            }]
        }),
    );
    write_test_auto_exit_on_death_layouts(&root, mod_name, true);
    let validated = validate_audio_mod(&root, mod_name).unwrap();
    assert!(!validated.has_audio_telemetry);
    assert!(validated.auto_exit_on_death_enabled);
    assert!(requested.all_present(&validated.feature_groups));
    validate_auto_exit_on_death_layouts(&root.join(mod_name), mod_name, true).unwrap();

    let enabled_group = validated.feature_groups[0].clone();
    let manifest_path = root.join(mod_name).join(LEGACY_MANIFEST_FILE_NAME);
    let manifest_before_toggle = std::fs::read(&manifest_path).unwrap();
    assert!(!set_auto_exit_on_death_enabled(&root, mod_name, false).unwrap());
    assert_eq!(
        std::fs::read(&manifest_path).unwrap(),
        manifest_before_toggle
    );
    let disabled = validate_audio_mod(&root, mod_name).unwrap();
    assert!(!disabled.auto_exit_on_death_enabled);
    let disabled_request = RequestedFeatureGroups::from_options(
        Some(false),
        Some(false),
        Some(false),
        None,
        None,
        None,
        Some(true),
    )
    .unwrap();
    assert!(disabled_request.all_present(&disabled.feature_groups));
    validate_preserved_feature_groups(&[enabled_group], &disabled.feature_groups).unwrap();
    let mut legacy_group = disabled.feature_groups[0].clone();
    legacy_group.fingerprint = AUTO_EXIT_ON_DEATH_LEGACY_DISABLED_FINGERPRINT.to_string();
    validate_preserved_feature_groups(&[legacy_group], &disabled.feature_groups).unwrap();
    validate_auto_exit_on_death_layouts(&root.join(mod_name), mod_name, false).unwrap();
    assert!(set_auto_exit_on_death_enabled(&root, mod_name, true).unwrap());
    validate_auto_exit_on_death_layouts(&root.join(mod_name), mod_name, true).unwrap();

    let death_layout = root
        .join(mod_name)
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY)
        .join("youdiedmodalhd.json");
    std::fs::write(
        death_layout,
        serde_json::to_vec(&serde_json::json!({
            "children": [{
                "type": "TimerWidget",
                "name": "legacy",
                "fields": {"time": 0.01, "message": "PanelManager:OpenPanel:exitgame"}
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(validate_audio_mod(&root, mod_name)
        .unwrap_err()
        .contains("exitgame"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn normalizes_existing_asset_test_mode_arguments() {
    let result =
        arguments_with_audio_mod("-assettestmode 0 -w -assettestmode=1 -txt", "FreshAudio")
            .unwrap();
    assert_eq!(result, "-w -mod FreshAudio -txt -assettestmode 1");
}

#[test]
fn validates_user_supplied_generated_mod_names() {
    assert_eq!(
        generated_audio_mod_name("  My-Audio_2  ").unwrap(),
        "My-Audio_2"
    );
    assert!(generated_audio_mod_name("").is_err());
    assert!(generated_audio_mod_name("My Audio").is_err());
    assert!(generated_audio_mod_name("我的Mod").is_err());
    assert!(generated_audio_mod_name("CON").is_err());
    assert!(generated_audio_mod_name("lpt9").is_err());
}

#[test]
fn detects_existing_mod_names_case_insensitively() {
    let root = std::env::temp_dir().join(format!(
        "d2rhub_audio_mod_collision_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(root.join("jcy")).unwrap();

    assert_eq!(
        find_existing_mod_name(&root, "jcy").unwrap().as_deref(),
        Some("jcy")
    );
    assert_eq!(
        find_existing_mod_name(&root, "JCY").unwrap().as_deref(),
        Some("jcy")
    );
    assert_eq!(find_existing_mod_name(&root, "fresh").unwrap(), None);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn replacement_journal_paths_cannot_alias_the_target_or_escape_the_transaction_layout() {
    assert!(replace_journal_paths_are_valid(
        "safe-mod",
        std::path::Path::new(".d2rhub-upgrade-stage-0123456789abcdef0123456789abcdef/safe-mod",),
        std::path::Path::new(".d2rhub-upgrade-backup-0123456789abcdef0123456789abcdef",),
    ));
    assert!(!replace_journal_paths_are_valid(
        "safe-mod",
        std::path::Path::new("safe-mod"),
        std::path::Path::new("safe-mod"),
    ));
    assert!(!replace_journal_paths_are_valid(
        "safe-mod",
        std::path::Path::new("../outside/safe-mod"),
        std::path::Path::new(".d2rhub-upgrade-backup-0123456789abcdef0123456789abcdef",),
    ));
    assert!(!replace_journal_paths_are_valid(
        "safe-mod",
        std::path::Path::new(".d2rhub-upgrade-stage-0123456789abcdef0123456789abcdef/safe-mod",),
        std::path::Path::new(".d2rhub-upgrade-backup-fedcba9876543210fedcba9876543210",),
    ));
}

#[test]
fn durability_traversal_visits_files_before_directories_bottom_up() {
    let root = test_mods_directory("durability_order");
    let mods = root.join("mods");
    let staged = mods.join("stage");
    let nested = staged.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("payload.bin"), b"payload").unwrap();
    let mut events = Vec::new();

    traverse_safe_directory_tree(&mods, &staged, |path, kind| {
        events.push((path.strip_prefix(&mods).unwrap().to_path_buf(), kind));
        Ok(())
    })
    .unwrap();

    let file_index = events
        .iter()
        .position(|(path, kind)| {
            path == std::path::Path::new("stage/nested/payload.bin")
                && *kind == SafeTreeNodeKind::File
        })
        .unwrap();
    let nested_index = events
        .iter()
        .position(|(path, kind)| {
            path == std::path::Path::new("stage/nested") && *kind == SafeTreeNodeKind::Directory
        })
        .unwrap();
    let root_index = events
        .iter()
        .position(|(path, kind)| {
            path == std::path::Path::new("stage") && *kind == SafeTreeNodeKind::Directory
        })
        .unwrap();
    assert!(file_index < nested_index && nested_index < root_index);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn staged_tree_is_synced_before_the_replace_journal_becomes_visible() {
    let root = test_mods_directory("durability_before_journal");
    let mods = root.join("mods");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("durable");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    let journal = mods.join(format!(
        "{}{}{}",
        super::replacement::REPLACE_JOURNAL_PREFIX,
        TEST_TRANSACTION_ID,
        super::replacement::REPLACE_JOURNAL_SUFFIX
    ));
    let temporary_journal = journal.with_extension("json.tmp");
    std::fs::create_dir_all(&mods).unwrap();
    write_test_audio_mod(&mods, "durable", current_audio_manifest("durable"));
    write_test_audio_mod(&stage_parent, "durable", current_audio_manifest("durable"));
    let sync_observed = std::cell::Cell::new(false);

    let actual_journal = write_replace_journal_with_stage_sync(
        &mods,
        "durable",
        &staged,
        &backup,
        &[],
        |mods_directory, staged_directory| {
            assert!(!journal.exists());
            assert!(!temporary_journal.exists());
            sync_observed.set(true);
            super::filesystem::sync_safe_directory_tree(mods_directory, staged_directory)
        },
    )
    .unwrap();

    assert!(sync_observed.get());
    assert_eq!(actual_journal, journal);
    assert!(journal.is_file());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn same_name_update_switches_only_after_staged_mod_is_ready() {
    let root = test_mods_directory("same_name_upgrade");
    let mods = root.join("mods");
    let staging = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(&staging).unwrap();
    write_test_audio_mod(
        &mods,
        "jcy-tz",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "build_mode": "minimal",
            "mod_name": "jcy-tz"
        }),
    );
    std::fs::write(mods.join("jcy-tz").join("old-marker.txt"), b"old").unwrap();
    write_test_audio_mod(&staging, "jcy-tz", current_audio_manifest("jcy-tz"));
    std::fs::write(staging.join("jcy-tz").join("new-marker.txt"), b"new").unwrap();
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));

    replace_audio_mod_directory(&mods, "jcy-tz", &staging.join("jcy-tz"), &backup, &[]).unwrap();

    assert!(mods.join("jcy-tz").join("new-marker.txt").is_file());
    assert!(!mods.join("jcy-tz").join("old-marker.txt").exists());
    assert!(!backup.exists());
    let result = compatibility(&mods, "-mod jcy-tz -txt");
    assert!(result.ready);
    assert!(!result.update_required);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn same_name_upgrade_cannot_drop_an_unknown_existing_feature_group() {
    let root = test_mods_directory("preserve_unknown_feature");
    let mods = root.join("mods");
    let staging = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    let unknown = GeneratorFeatureGroup {
        id: "future_feature".to_string(),
        recipe_version: 77,
        fingerprint: "future-v77;opaque=true".to_string(),
        reused_from_source: false,
    };
    let mut current_manifest = current_audio_manifest("preserve-me");
    current_manifest["feature_groups"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(&unknown).unwrap());
    write_test_audio_mod(&mods, "preserve-me", current_manifest);
    let existing = validate_audio_mod(&mods, "preserve-me")
        .unwrap()
        .feature_groups;

    let mut candidate_manifest = current_audio_manifest("preserve-me");
    candidate_manifest["feature_groups"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
            "recipe_version": IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
            "fingerprint": format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}")
        }));
    write_test_audio_mod(&staging, "preserve-me", candidate_manifest.clone());
    write_test_room_tool_layouts(&staging, "preserve-me");

    let error = replace_audio_mod_directory(
        &mods,
        "preserve-me",
        &staging.join("preserve-me"),
        &backup,
        &existing,
    )
    .unwrap_err();
    assert!(error.contains("未无损保留现有功能组“future_feature”"));
    assert!(validate_audio_mod(&mods, "preserve-me")
        .unwrap()
        .feature_groups
        .iter()
        .any(|group| group.id == "future_feature"));
    assert!(!backup.exists());

    let mut carried = unknown.clone();
    carried.reused_from_source = true;
    candidate_manifest["feature_groups"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(carried).unwrap());
    write_test_audio_mod(&staging, "preserve-me", candidate_manifest);
    replace_audio_mod_directory(
        &mods,
        "preserve-me",
        &staging.join("preserve-me"),
        &backup,
        &existing,
    )
    .unwrap();

    let installed = validate_audio_mod(&mods, "preserve-me").unwrap();
    validate_preserved_feature_groups(&existing, &installed.feature_groups).unwrap();
    assert!(installed
        .feature_groups
        .iter()
        .any(|group| group.id == "future_feature" && group.reused_from_source));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn crash_recovery_restores_backup_when_new_target_drops_an_unknown_group() {
    let root = test_mods_directory("recover_unknown_feature");
    let mods = root.join("mods");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("preserve-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    let unknown = GeneratorFeatureGroup {
        id: "future_feature".to_string(),
        recipe_version: 77,
        fingerprint: "future-v77;opaque=true".to_string(),
        reused_from_source: false,
    };
    let mut old_manifest = current_audio_manifest("preserve-me");
    old_manifest["feature_groups"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(&unknown).unwrap());
    write_test_audio_mod(&mods, "preserve-me", old_manifest);
    std::fs::write(mods.join("preserve-me").join("old-marker.txt"), b"old").unwrap();
    let required = validate_audio_mod(&mods, "preserve-me")
        .unwrap()
        .feature_groups;

    let mut incomplete_new = current_audio_manifest("preserve-me");
    incomplete_new["feature_groups"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
            "recipe_version": IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
            "fingerprint": format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}")
        }));
    write_test_audio_mod(&stage_parent, "preserve-me", incomplete_new);
    write_test_room_tool_layouts(&stage_parent, "preserve-me");
    std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();

    // Simulate a journal produced just before preservation metadata was enforced, then a crash
    // after both directory renames. Recovery must treat the missing opaque group as an invalid
    // new target and restore the strict r22 backup instead of deleting it.
    let journal = write_replace_journal(&mods, "preserve-me", &staged, &backup, &[]).unwrap();
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
    document["required_feature_groups"] = serde_json::to_value(&required).unwrap();
    std::fs::write(&journal, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
    std::fs::rename(mods.join("preserve-me"), &backup).unwrap();
    std::fs::rename(&staged, mods.join("preserve-me")).unwrap();

    recover_audio_mod_replacements(&mods).unwrap();

    let restored = validate_audio_mod(&mods, "preserve-me").unwrap();
    validate_preserved_feature_groups(&required, &restored.feature_groups).unwrap();
    assert!(mods.join("preserve-me").join("old-marker.txt").is_file());
    assert!(!mods.join("preserve-me").join("new-marker.txt").exists());
    assert!(!backup.exists());
    assert!(!journal.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_generated_mod_runs_but_is_not_an_additive_source() {
    let root = test_mods_directory("generated_source");
    write_test_audio_mod(
        &root,
        "old-audio",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "build_mode": "augment",
            "mod_name": "old-audio"
        }),
    );

    let error = resolve_source_directory(&root, "old-audio-updated", Some("old-audio".to_string()))
        .unwrap_err();

    let runtime = compatibility(&root, "-mod old-audio -txt");
    assert!(runtime.ready);
    assert!(runtime.update_required);
    assert!(error.contains("不能安全增量加工"));
    assert!(root.join("old-audio").is_dir());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn verified_current_feature_mod_is_an_additive_source() {
    let root = test_mods_directory("current_generated_source");
    write_test_audio_mod(&root, "audio-r22", current_audio_manifest("audio-r22"));

    let (source_name, source_directory) =
        resolve_source_directory(&root, "audio-plus-rooms", Some("audio-r22".to_string())).unwrap();

    let expected_source = root.join("audio-r22");
    assert_eq!(source_name.as_deref(), Some("audio-r22"));
    assert_eq!(source_directory.as_deref(), Some(expected_source.as_path()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn duplicate_or_unfingerprinted_groups_are_not_trusted_as_sources() {
    let root = test_mods_directory("invalid_feature_sources");
    for (name, groups) in [
        (
            "duplicate",
            serde_json::json!([
                {"id": AUDIO_TELEMETRY_FEATURE_ID, "recipe_version": 1, "fingerprint": "one"},
                {"id": AUDIO_TELEMETRY_FEATURE_ID, "recipe_version": 1, "fingerprint": "two"}
            ]),
        ),
        (
            "empty-fingerprint",
            serde_json::json!([
                {"id": AUDIO_TELEMETRY_FEATURE_ID, "recipe_version": 1, "fingerprint": ""}
            ]),
        ),
    ] {
        let mut manifest = current_audio_manifest(name);
        manifest["feature_groups"] = groups;
        write_test_audio_mod(&root, name, manifest);
        assert!(resolve_source_directory(&root, "next", Some(name.to_string())).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn generator_report_must_match_requested_and_persisted_groups() {
    let root = test_mods_directory("generator_groups");
    let mod_name = "generated";
    write_test_audio_mod(&root, mod_name, current_audio_manifest(mod_name));
    let audio_group = GeneratorFeatureGroup {
        id: AUDIO_TELEMETRY_FEATURE_ID.to_string(),
        recipe_version: AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
        fingerprint: test_audio_fingerprint(),
        reused_from_source: false,
    };
    let mut report = GeneratorReport {
        protocol_version: PROTOCOL_VERSION,
        recipe_version: REQUIRED_AUDIO_MOD_RECIPE_VERSION,
        mod_name: mod_name.to_string(),
        mod_directory: root.join(mod_name).to_string_lossy().into_owned(),
        feature_groups: vec![audio_group.clone()],
    };

    let actual = validate_generator_output(
        &root,
        mod_name,
        &report,
        RequestedFeatureGroups {
            audio_telemetry: true,
            room_tools: false,
            esc_next_game: false,
            scene_shortcuts: false,
            party_loot: false,
            hostile_auto_restart: false,
            auto_exit_on_death: false,
        },
        &[],
    )
    .unwrap();
    assert_eq!(actual.feature_groups, vec![audio_group]);

    let required_future = GeneratorFeatureGroup {
        id: "future_feature".to_string(),
        recipe_version: 77,
        fingerprint: "future-v77;opaque=true".to_string(),
        reused_from_source: false,
    };
    assert!(validate_generator_output(
        &root,
        mod_name,
        &report,
        RequestedFeatureGroups {
            audio_telemetry: true,
            room_tools: false,
            esc_next_game: false,
            scene_shortcuts: false,
            party_loot: false,
            hostile_auto_restart: false,
            auto_exit_on_death: false,
        },
        &[required_future],
    )
    .unwrap_err()
    .contains("未无损保留现有功能组“future_feature”"));

    assert!(validate_generator_output(
        &root,
        mod_name,
        &report,
        RequestedFeatureGroups {
            audio_telemetry: true,
            room_tools: true,
            esc_next_game: false,
            scene_shortcuts: false,
            party_loot: false,
            hostile_auto_restart: false,
            auto_exit_on_death: false,
        },
        &[],
    )
    .unwrap_err()
    .contains("局内房间工具"));

    report.feature_groups.push(GeneratorFeatureGroup {
        id: IN_GAME_ROOM_TOOLS_FEATURE_ID.to_string(),
        recipe_version: IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
        fingerprint: format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}"),
        reused_from_source: false,
    });
    assert!(validate_generator_output(
        &root,
        mod_name,
        &report,
        RequestedFeatureGroups {
            audio_telemetry: true,
            room_tools: true,
            esc_next_game: false,
            scene_shortcuts: false,
            party_loot: false,
            hostile_auto_restart: false,
            auto_exit_on_death: false,
        },
        &[],
    )
    .unwrap_err()
    .contains("落盘清单不一致"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn final_persisted_groups_control_audio_readiness() {
    let root = test_mods_directory("room_only");
    let name = "room-only";
    let mut manifest = current_audio_manifest(name);
    manifest["feature_groups"] = serde_json::json!([{
        "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
        "recipe_version": IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
        "fingerprint": format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}")
    }]);
    write_test_audio_mod(&root, name, manifest);
    write_test_room_tool_layouts(&root, name);
    std::fs::remove_file(root.join(name).join(AREA_CATALOG_FILE_NAME)).unwrap();
    std::fs::remove_file(root.join(name).join(ITEM_CATALOG_FILE_NAME)).unwrap();

    let state = compatibility(&root, &format!("-mod {name} -txt"));
    assert!(!state.ready);
    assert_eq!(state.reason_code, "missing_audio_feature");
    let listed = installed_mods(&root);
    assert_eq!(
        listed[0].feature_groups,
        vec![IN_GAME_ROOM_TOOLS_FEATURE_ID]
    );
    assert!(!listed[0].audio_ready);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn esc_layouts_reject_legacy_hud_cleanup_only_for_new_recipe() {
    use super::validation::layouts::validate_esc_next_game_layouts;
    let root = test_mods_directory("esc_hud_cleanup");
    let name = "esc-probe";
    write_test_room_tool_layouts(&root, name);
    let directory = root
        .join(name)
        .join(format!("{name}.mpq/data/global/ui/layouts"));
    for file in ["pauselayouthd.json", "pauselayoutgardenhd.json"] {
        let path = directory.join(file);
        let mut pause: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        pause["children"].as_array_mut().unwrap().extend([
            serde_json::json!({"fields":{"time":0.01,"message":"PanelManager:OpenPanel:D2RHubQuickRecreateEscArm"}}),
            serde_json::json!({"fields":{"time":0.5,"message":"PanelManager:ClosePanel:D2RHubQuickRecreateEscArm"}}),
        ]);
        std::fs::write(path, serde_json::to_vec(&pause).unwrap()).unwrap();
    }
    validate_esc_next_game_layouts(&root.join(name), name, 3).unwrap();
    let path = directory.join("HudWarningshd.json");
    let mut hud: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    hud["children"].as_array_mut().unwrap().push(serde_json::json!({"fields":{"time":0.001,"message":"PanelManager:ClosePanel:D2RHubQuickRecreateEscArm"}}));
    std::fs::write(path, serde_json::to_vec(&hud).unwrap()).unwrap();
    assert!(validate_esc_next_game_layouts(&root.join(name), name, 3)
        .unwrap_err()
        .contains("HUD"));
    validate_esc_next_game_layouts(&root.join(name), name, 2).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn room_tools_validate_ordered_submission_and_legacy_timing() {
    let root = test_mods_directory("submission_delays");
    let name = "room-tools";
    write_test_room_tool_layouts(&root, name);
    validate_in_game_room_tool_layouts(&root.join(name), name).unwrap();
    let directory = root
        .join(name)
        .join(format!("{name}.mpq/data/global/ui/layouts"));
    for file in ["D2RHubCommitCreateGamehd.json"] {
        let path = directory.join(file);
        let original: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        // Reject the delayed r30/r31 submission timings.
        for delay in [0.06, 0.10, 0.55] {
            let mut changed = original.clone();
            changed["children"][2]["fields"]["time"] = serde_json::json!(delay);
            std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
            assert!(
                validate_in_game_room_tool_layouts(&root.join(name), name).is_err(),
                "{file}: {delay}"
            );
        }
        let mut closing = original.clone();
        closing["children"][3]["fields"]["time"] = serde_json::json!(0.15);
        std::fs::write(&path, serde_json::to_vec(&closing).unwrap()).unwrap();
        assert!(validate_in_game_room_tool_layouts(&root.join(name), name).is_err());
        let mut reversed = original.clone();
        reversed["children"].as_array_mut().unwrap().swap(2, 3);
        std::fs::write(&path, serde_json::to_vec(&reversed).unwrap()).unwrap();
        assert!(validate_in_game_room_tool_layouts(&root.join(name), name).is_err());
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    }
    let join_path = directory.join("D2RHubInGameJoinGamehd.json");
    let join = std::fs::read_to_string(&join_path).unwrap().replace(
        "JoinGame:JoinGame",
        "PanelManager:OpenPanel:D2RHubCommitJoinGame",
    );
    std::fs::write(join_path, join).unwrap();
    // Previously generated r30 Mods remain readable as upgrade sources.
    for (file, submit, close) in [
        ("D2RHubCommitCreateGamehd.json", 0.10, 0.15),
        ("D2RHubCommitJoinGamehd.json", 0.55, 0.60),
    ] {
        let path = directory.join(file);
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        legacy["children"][2]["fields"]["time"] = serde_json::json!(submit);
        legacy["children"][3]["fields"]["time"] = serde_json::json!(close);
        std::fs::write(path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    }
    super::validation::layouts::validate_in_game_room_tool_layouts_for_version(
        &root.join(name),
        name,
        30,
    )
    .unwrap();
    for file in [
        "D2RHubCommitCreateGamehd.json",
        "D2RHubCommitJoinGamehd.json",
    ] {
        let path = directory.join(file);
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        legacy["children"][2]["fields"]["time"] = serde_json::json!(0.05);
        legacy["children"][3]["fields"]["time"] = serde_json::json!(0.05);
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    }
    super::validation::layouts::validate_in_game_room_tool_layouts_for_version(
        &root.join(name),
        name,
        29,
    )
    .unwrap();
    for file in [
        "D2RHubCommitCreateGamehd.json",
        "D2RHubCommitJoinGamehd.json",
    ] {
        let path = directory.join(file);
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        legacy["children"][2]["fields"]["time"] = serde_json::json!(0.06);
        legacy["children"].as_array_mut().unwrap().remove(3);
        std::fs::write(path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    }
    super::validation::layouts::validate_in_game_room_tool_layouts_for_version(
        &root.join(name),
        name,
        31,
    )
    .unwrap();
    assert!(validate_in_game_room_tool_layouts(&root.join(name), name).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn room_tools_validate_default_hell_initialization() {
    let root = test_mods_directory("default_hell");
    let name = "room-tools";
    write_test_room_tool_layouts(&root, name);
    validate_in_game_room_tool_layouts(&root.join(name), name).unwrap();
    let directory = root
        .join(name)
        .join(format!("{name}.mpq/data/global/ui/layouts"));
    let path = directory.join("D2RHubInGameCreateGamehd.json");
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let timer_index = original["children"]
        .as_array()
        .unwrap()
        .iter()
        .position(|child| child["name"] == "D2RHubDefaultHell")
        .unwrap();
    for mutation in ["missing", "duplicate", "normal", "late", "button"] {
        let mut form = original.clone();
        match mutation {
            "missing" => {
                form["children"].as_array_mut().unwrap().remove(timer_index);
            }
            "duplicate" => form["children"]
                .as_array_mut()
                .unwrap()
                .push(original["children"][timer_index].clone()),
            "normal" => {
                form["children"][timer_index]["fields"]["message"] =
                    serde_json::json!("CreateGame:SetDifficulty:0")
            }
            "late" => form["children"][timer_index]["fields"]["time"] = serde_json::json!(1.0),
            "button" => form["children"][timer_index]["type"] = serde_json::json!("ButtonWidget"),
            _ => unreachable!(),
        }
        std::fs::write(&path, serde_json::to_vec(&form).unwrap()).unwrap();
        assert!(
            validate_in_game_room_tool_layouts(&root.join(name), name)
                .unwrap_err()
                .contains("默认地狱"),
            "{mutation}"
        );
    }
    std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    for file in [
        "creategamepanelhd.json",
        "joingamepanelhd.json",
        "D2RHubInGameJoinGamehd.json",
    ] {
        let other_path = directory.join(file);
        let saved = std::fs::read(&other_path).unwrap();
        let mut other: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        other["children"]
            .as_array_mut()
            .unwrap()
            .push(original["children"][timer_index].clone());
        std::fs::write(&other_path, serde_json::to_vec(&other).unwrap()).unwrap();
        assert!(validate_in_game_room_tool_layouts(&root.join(name), name)
            .unwrap_err()
            .contains("默认地狱"));
        std::fs::write(&other_path, saved).unwrap();
    }
    let join_path = directory.join("D2RHubInGameJoinGamehd.json");
    let join = std::fs::read_to_string(&join_path).unwrap().replace(
        "JoinGame:JoinGame",
        "PanelManager:OpenPanel:D2RHubCommitJoinGame",
    );
    std::fs::write(join_path, join).unwrap();
    // A genuine r28 source has neither the initializer nor the r30 submit delays.
    let mut legacy = original.clone();
    legacy["children"]
        .as_array_mut()
        .unwrap()
        .remove(timer_index);
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    for file in [
        "D2RHubCommitCreateGamehd.json",
        "D2RHubCommitJoinGamehd.json",
    ] {
        let commit_path = directory.join(file);
        let mut commit: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&commit_path).unwrap()).unwrap();
        commit["children"][2]["fields"]["time"] = serde_json::json!(0.05);
        commit["children"][3]["fields"]["time"] = serde_json::json!(0.05);
        std::fs::write(commit_path, serde_json::to_vec(&commit).unwrap()).unwrap();
    }
    super::validation::layouts::validate_in_game_room_tool_layouts_for_version(
        &root.join(name),
        name,
        28,
    )
    .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn room_tools_processing_can_disable_and_restore_default_hell() {
    let root = TemporaryDirectory::create(test_mods_directory("room_options")).unwrap();
    let name = "Rooms";
    let mut manifest = current_audio_manifest(name);
    manifest["feature_groups"] = serde_json::json!([{
        "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
        "recipe_version": IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
        "fingerprint": format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}")
    }]);
    write_test_audio_mod(root.path(), name, manifest);
    write_test_room_tool_layouts(root.path(), name);
    let directory = root.path().join(name);
    let layouts = directory
        .join(format!("{name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let create = layouts.join("D2RHubInGameCreateGamehd.json");
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&create).unwrap()).unwrap();
    let join = std::fs::read(layouts.join("D2RHubInGameJoinGamehd.json")).unwrap();
    assert!(super::room_tool_options::default_hell(&directory).unwrap());
    for enabled in [false, false, true, true] {
        super::room_tool_options::configure(root.path(), name, enabled).unwrap();
        assert_eq!(
            super::room_tool_options::default_hell(&directory).unwrap(),
            enabled
        );
        validate_audio_mod(root.path(), name).unwrap();
        let form: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&create).unwrap()).unwrap();
        let mut expected = original.clone();
        expected["children"]
            .as_array_mut()
            .unwrap()
            .retain(|child| child["name"] != "D2RHubDefaultHell");
        let mut without_timer = form.clone();
        without_timer["children"]
            .as_array_mut()
            .unwrap()
            .retain(|child| child["name"] != "D2RHubDefaultHell");
        assert_eq!(without_timer, expected);
        assert_eq!(
            std::fs::read(layouts.join("D2RHubInGameJoinGamehd.json")).unwrap(),
            join
        );
        assert_eq!(
            installed_mods(root.path())[0].room_tools_default_hell,
            enabled
        );
    }
    super::room_tool_options::configure(root.path(), name, false).unwrap();
    std::fs::write(&create, serde_json::to_vec(&original).unwrap()).unwrap();
    assert!(validate_audio_mod(root.path(), name)
        .unwrap_err()
        .contains("默认地狱"));
    let path = directory.join("audio-telemetry-manifest.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    document["room_tools_default_hell"] = "false".into();
    std::fs::write(path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(validate_audio_mod(root.path(), name).is_err());
}

#[test]
fn claimed_room_tools_require_supported_metadata_and_complete_layouts() {
    let root = test_mods_directory("claimed_room_tools");
    let name = "room-tools";
    let room_manifest = |recipe_version, fingerprint: &str| {
        serde_json::json!({
            "manifest_format": "d2r-audio-telemetry-mod",
            "producer": "d2r-audio-mod",
            "protocol_version": PROTOCOL_VERSION,
            "recipe_version": REQUIRED_AUDIO_MOD_RECIPE_VERSION,
            "build_mode": "minimal",
            "mod_name": name,
            "feature_groups": [{
                "id": IN_GAME_ROOM_TOOLS_FEATURE_ID,
                "recipe_version": recipe_version,
                "fingerprint": fingerprint
            }]
        })
    };

    let room_fingerprint = format!("room-tools-v{IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION}");
    write_test_audio_mod(
        &root,
        name,
        room_manifest(IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION, &room_fingerprint),
    );
    let credential = validate_audio_mod_credential(&root, name).unwrap();
    assert_eq!(
        credential.feature_groups[0].id,
        IN_GAME_ROOM_TOOLS_FEATURE_ID
    );
    let incomplete = validate_audio_mod(&root, name).unwrap_err();
    assert!(incomplete.contains("缺少布局文件"));

    write_test_room_tool_layouts(&root, name);
    let validated = validate_audio_mod(&root, name).unwrap();
    validate_in_game_room_tool_layouts(&validated.directory, name).unwrap();

    let previous_recipe = IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION - 1;
    write_test_audio_mod(
        &root,
        name,
        room_manifest(previous_recipe, &format!("room-tools-v{previous_recipe}")),
    );
    assert!(validate_audio_mod(&root, name)
        .unwrap_err()
        .contains(&format!("配方 r{previous_recipe} 不受支持")));
    write_test_audio_mod(
        &root,
        name,
        room_manifest(
            IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION,
            "forged-room-tools",
        ),
    );
    assert!(validate_audio_mod(&root, name)
        .unwrap_err()
        .contains("指纹无效"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn known_audio_metadata_is_strict_but_unknown_groups_remain_forward_compatible() {
    let root = test_mods_directory("known_and_unknown_groups");
    let name = "feature-metadata";
    let mut manifest = current_audio_manifest(name);
    let unsupported_recipe = AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION + 1;
    manifest["feature_groups"][0]["recipe_version"] = serde_json::json!(unsupported_recipe);
    write_test_audio_mod(&root, name, manifest.clone());
    assert!(validate_audio_mod(&root, name)
        .unwrap_err()
        .contains(&format!("配方 r{unsupported_recipe} 不受支持")));

    manifest["feature_groups"][0]["recipe_version"] =
        serde_json::json!(AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION);
    manifest["feature_groups"][0]["fingerprint"] = serde_json::json!(format!(
        "audio-v{AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION};fixture=forged"
    ));
    write_test_audio_mod(&root, name, manifest.clone());
    assert!(validate_audio_mod(&root, name)
        .unwrap_err()
        .contains("指纹无效"));

    manifest["feature_groups"] = serde_json::json!([{
        "id": "future_feature",
        "recipe_version": 77,
        "fingerprint": "future-v77;opaque=true"
    }]);
    write_test_audio_mod(&root, name, manifest);
    let validated = validate_audio_mod(&root, name).unwrap();
    assert_eq!(validated.feature_groups[0].id, "future_feature");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_directory_switch_restores_the_old_mod() {
    let root = test_mods_directory("replace_recovery_rollback");
    let mods = root.join("mods");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    write_test_audio_mod(
        &mods,
        "recover-me",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION - 1,
            "build_mode": "minimal",
            "mod_name": "recover-me"
        }),
    );
    std::fs::write(mods.join("recover-me").join("old-marker.txt"), b"old").unwrap();
    write_test_audio_mod(
        &stage_parent,
        "recover-me",
        current_audio_manifest("recover-me"),
    );
    std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();
    let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
    std::fs::rename(mods.join("recover-me"), &backup).unwrap();

    recover_audio_mod_replacements(&mods).unwrap();

    assert!(mods.join("recover-me").join("old-marker.txt").is_file());
    assert!(!mods.join("recover-me").join("new-marker.txt").exists());
    assert!(!backup.exists());
    assert!(!journal.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_directory_switch_commits_the_valid_new_mod() {
    let root = test_mods_directory("replace_recovery_commit");
    let mods = root.join("mods");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    write_test_audio_mod(
        &mods,
        "recover-me",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "build_mode": "minimal",
            "mod_name": "recover-me"
        }),
    );
    write_test_audio_mod(
        &stage_parent,
        "recover-me",
        current_audio_manifest("recover-me"),
    );
    std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();
    let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
    std::fs::rename(mods.join("recover-me"), &backup).unwrap();
    std::fs::rename(&staged, mods.join("recover-me")).unwrap();

    recover_audio_mod_replacements(&mods).unwrap();

    assert!(mods.join("recover-me").join("new-marker.txt").is_file());
    assert!(!backup.exists());
    assert!(!journal.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_never_accepts_a_legacy_shaped_new_target_over_a_good_backup() {
    let root = test_mods_directory("replace_recovery_strict_new_target");
    let mods = root.join("mods");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    write_test_audio_mod(
        &mods,
        "recover-me",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "build_mode": "minimal",
            "mod_name": "recover-me"
        }),
    );
    std::fs::write(mods.join("recover-me").join("old-marker.txt"), b"old").unwrap();
    write_test_audio_mod(
        &stage_parent,
        "recover-me",
        current_audio_manifest("recover-me"),
    );
    std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();
    let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
    std::fs::rename(mods.join("recover-me"), &backup).unwrap();
    std::fs::rename(&staged, mods.join("recover-me")).unwrap();
    // It still passes the permissive published-release validator, but it is not a complete r22
    // target and therefore must never make the good backup disposable.
    write_test_audio_mod(
        &mods,
        "recover-me",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "build_mode": "minimal",
            "mod_name": "recover-me"
        }),
    );

    recover_audio_mod_replacements(&mods).unwrap();

    assert!(mods.join("recover-me").join("old-marker.txt").is_file());
    assert!(!mods.join("recover-me").join("new-marker.txt").exists());
    assert!(!backup.exists());
    assert!(!journal.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn target_without_transaction_artifacts_requires_strict_current_validation() {
    let root = test_mods_directory("replace_recovery_target_only_strict");
    let mods = root.join("mods");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    write_test_audio_mod(
        &mods,
        "recover-me",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "build_mode": "minimal",
            "mod_name": "recover-me"
        }),
    );
    std::fs::write(mods.join("recover-me").join("only-copy.txt"), b"evidence").unwrap();
    write_test_audio_mod(
        &stage_parent,
        "recover-me",
        current_audio_manifest("recover-me"),
    );
    let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
    std::fs::remove_dir_all(&stage_parent).unwrap();

    let error = recover_audio_mod_replacements(&mods).unwrap_err();

    assert!(error.contains("严格校验"));
    assert!(mods.join("recover-me").join("only-copy.txt").is_file());
    assert!(
        journal.is_file(),
        "unsafe recovery must preserve its journal"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn valid_staged_candidate_wins_only_after_a_missing_target_backup_is_rejected() {
    let root = test_mods_directory("replace_recovery_bad_backup");
    let mods = root.join("mods");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(mods.join("recover-me").join("recover-me.mpq")).unwrap();
    std::fs::write(mods.join("recover-me").join("bad-backup.txt"), b"bad").unwrap();
    write_test_audio_mod(
        &stage_parent,
        "recover-me",
        current_audio_manifest("recover-me"),
    );
    std::fs::write(staged.join("new-marker.txt"), b"new").unwrap();
    let journal = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap();
    std::fs::rename(mods.join("recover-me"), &backup).unwrap();

    recover_audio_mod_replacements(&mods).unwrap();

    assert!(mods.join("recover-me").join("new-marker.txt").is_file());
    assert!(!backup.exists());
    assert!(!journal.exists());
    assert!(std::fs::read_dir(&mods)
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".d2rhub-upgrade-failed-backup-")
        }));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn transaction_paths_reject_a_symlinked_stage_parent_outside_mods() {
    use std::os::unix::fs::symlink;

    let root = test_mods_directory("replace_symlink_escape");
    let mods = root.join("mods");
    let outside = root.join("outside");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(outside.join("recover-me")).unwrap();
    std::fs::write(outside.join("recover-me").join("evidence.txt"), b"outside").unwrap();
    write_test_audio_mod(&mods, "recover-me", current_audio_manifest("recover-me"));
    symlink(&outside, &stage_parent).unwrap();

    let error = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap_err();

    assert!(error.contains("符号链接"));
    assert_eq!(
        std::fs::read(outside.join("recover-me").join("evidence.txt")).unwrap(),
        b"outside"
    );
    let journal = mods.join(format!(
        "{}{}{}",
        super::replacement::REPLACE_JOURNAL_PREFIX,
        TEST_TRANSACTION_ID,
        super::replacement::REPLACE_JOURNAL_SUFFIX
    ));
    std::fs::write(
        &journal,
        serde_json::to_vec(&serde_json::json!({
            "format_version": 1,
            "mod_name": "recover-me",
            "staged_relative": format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}/recover-me"),
            "backup_relative": format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}")
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(recover_audio_mod_replacements(&mods)
        .unwrap_err()
        .contains("符号链接"));
    assert!(journal.is_file());
    assert_eq!(
        std::fs::read(outside.join("recover-me").join("evidence.txt")).unwrap(),
        b"outside"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn validation_and_journaling_reject_nested_staged_symlinks() {
    use std::os::unix::fs::symlink;

    let root = test_mods_directory("nested_stage_symlink");
    let mods = root.join("mods");
    let outside = root.join("outside");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("evidence.txt"), b"outside").unwrap();
    write_test_audio_mod(&mods, "recover-me", current_audio_manifest("recover-me"));
    write_test_audio_mod(
        &stage_parent,
        "recover-me",
        current_audio_manifest("recover-me"),
    );
    let nested_link = staged.join("recover-me.mpq").join("nested-link");
    symlink(&outside, &nested_link).unwrap();

    assert!(validate_audio_mod(&stage_parent, "recover-me")
        .unwrap_err()
        .contains("符号链接"));
    assert!(
        validate_recoverable_audio_mod_directory(&mods, "recover-me", &staged)
            .unwrap_err()
            .contains("符号链接")
    );
    assert!(
        write_replace_journal(&mods, "recover-me", &staged, &backup, &[])
            .unwrap_err()
            .contains("符号链接")
    );
    assert_eq!(
        std::fs::read(outside.join("evidence.txt")).unwrap(),
        b"outside"
    );
    assert!(!mods
        .join(format!(
            "{}{}{}",
            super::replacement::REPLACE_JOURNAL_PREFIX,
            TEST_TRANSACTION_ID,
            super::replacement::REPLACE_JOURNAL_SUFFIX
        ))
        .exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn transaction_paths_reject_windows_directory_reparse_points() {
    use std::os::windows::fs::symlink_dir;

    let root = test_mods_directory("replace_reparse_escape");
    let mods = root.join("mods");
    let outside = root.join("outside");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(outside.join("recover-me")).unwrap();
    write_test_audio_mod(&mods, "recover-me", current_audio_manifest("recover-me"));
    if symlink_dir(&outside, &stage_parent).is_err() {
        // Windows may deny symlink creation when Developer Mode is disabled. Production also
        // checks FILE_ATTRIBUTE_REPARSE_POINT, which covers directory junctions.
        std::fs::remove_dir_all(root).unwrap();
        return;
    }

    let error = write_replace_journal(&mods, "recover-me", &staged, &backup, &[]).unwrap_err();
    assert!(error.contains("重解析点") || error.contains("符号链接"));
    std::fs::remove_dir(&stage_parent).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn validation_and_journaling_reject_nested_windows_reparse_points() {
    use std::os::windows::fs::symlink_dir;

    let root = test_mods_directory("nested_stage_reparse");
    let mods = root.join("mods");
    let outside = root.join("outside");
    let stage_parent = mods.join(format!(".d2rhub-upgrade-stage-{TEST_TRANSACTION_ID}"));
    let staged = stage_parent.join("recover-me");
    let backup = mods.join(format!(".d2rhub-upgrade-backup-{TEST_TRANSACTION_ID}"));
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    write_test_audio_mod(&mods, "recover-me", current_audio_manifest("recover-me"));
    write_test_audio_mod(
        &stage_parent,
        "recover-me",
        current_audio_manifest("recover-me"),
    );
    let nested_link = staged.join("recover-me.mpq").join("nested-link");
    if symlink_dir(&outside, &nested_link).is_err() {
        std::fs::remove_dir_all(root).unwrap();
        return;
    }

    assert!(validate_audio_mod(&stage_parent, "recover-me")
        .unwrap_err()
        .contains("重解析点"));
    assert!(
        validate_recoverable_audio_mod_directory(&mods, "recover-me", &staged)
            .unwrap_err()
            .contains("重解析点")
    );
    assert!(
        write_replace_journal(&mods, "recover-me", &staged, &backup, &[])
            .unwrap_err()
            .contains("重解析点")
    );
    std::fs::remove_dir(&nested_link).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn published_legacy_manifests_remain_usable_but_request_an_update() {
    let root = test_mods_directory("legacy_recipe");
    let source_excel = root.join("jcy").join("jcy.mpq").join("data/global/excel");
    std::fs::create_dir_all(&source_excel).unwrap();
    write_test_audio_mod(
        &root,
        "early-official",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "build_mode": "minimal"
        }),
    );
    write_test_audio_mod(
        &root,
        "v013-official",
        serde_json::json!({
            "manifest_format": "d2r-audio-telemetry-mod",
            "producer": "d2r-audio-mod",
            "producer_version": "0.1.3",
            "protocol_version": PROTOCOL_VERSION,
            "build_mode": "augment",
            "mod_name": "v013-official",
            "source_excel_directory": source_excel
        }),
    );

    for name in ["early-official", "v013-official"] {
        let result = compatibility(&root, &format!("-mod {name} -txt"));
        assert!(result.ready, "legacy Mod {name} should stay usable");
        assert!(result.update_required);
        assert_eq!(result.reason_code, "update_available");
        assert_eq!(result.recipe_version, None);
    }
    let augmented = compatibility(&root, "-mod v013-official -txt");
    assert_eq!(augmented.source_mod_name.as_deref(), Some("jcy"));
    let listed = installed_mods(&root);
    let generated = listed
        .iter()
        .filter(|entry| entry.name != "jcy")
        .collect::<Vec<_>>();
    assert!(generated.iter().all(|entry| entry.audio_ready));
    assert!(generated.iter().all(|entry| entry.update_required));
    assert!(generated.iter().all(|entry| !entry.source_eligible));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn old_audio_group_is_usable_but_offers_update_even_at_current_overall_recipe() {
    let root = test_mods_directory("audio_group_update");
    write_test_audio_mod(
        &root,
        "old-audio",
        serde_json::json!({
            "manifest_format": "d2r-audio-telemetry-mod",
            "producer": "d2r-audio-mod", "protocol_version": PROTOCOL_VERSION,
            "recipe_version": REQUIRED_AUDIO_MOD_RECIPE_VERSION,
            "mod_name": "old-audio", "build_mode": "minimal",
            "feature_groups": [{"id": AUDIO_TELEMETRY_FEATURE_ID, "recipe_version": 4,
                "fingerprint": test_audio_fingerprint().replace("audio-v5;", "audio-v4;")}]
        }),
    );
    let compatible = compatibility(&root, "-mod old-audio -txt");
    assert!(compatible.ready);
    assert!(compatible.update_required);
    assert_eq!(compatible.reason_code, "update_available");
    let installed = installed_mods(&root);
    let old = installed
        .iter()
        .find(|item| item.name == "old-audio")
        .unwrap();
    assert!(old.update_required);
    assert!(!old.audio_reusable);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn current_and_future_recipes_do_not_trigger_false_updates() {
    let root = test_mods_directory("current_recipe");
    for version in [
        REQUIRED_AUDIO_MOD_RECIPE_VERSION,
        REQUIRED_AUDIO_MOD_RECIPE_VERSION + 1,
    ] {
        let name = format!("recipe-{version}");
        write_test_audio_mod(
            &root,
            &name,
            serde_json::json!({
                "manifest_format": "d2r-audio-telemetry-mod",
                "producer": "d2r-audio-mod",
                "protocol_version": PROTOCOL_VERSION,
                "recipe_version": version,
                "build_mode": "augment",
                "source_mod_name": "jcy",
                "mod_name": name,
                "feature_groups": [{
                    "id": AUDIO_TELEMETRY_FEATURE_ID,
                    "recipe_version": AUDIO_TELEMETRY_FEATURE_RECIPE_VERSION,
                    "fingerprint": test_audio_fingerprint()
                }]
            }),
        );
        let result = compatibility(&root, &format!("-mod {name} -txt"));
        assert!(result.ready);
        assert!(!result.update_required);
        assert_eq!(result.reason_code, "ready");
        assert_eq!(result.recipe_version, Some(version));
        assert_eq!(result.source_mod_name.as_deref(), Some("jcy"));
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_recipe_metadata_is_not_treated_as_a_legacy_release() {
    let root = test_mods_directory("invalid_recipe");
    write_test_audio_mod(
        &root,
        "broken",
        serde_json::json!({
            "manifest_format": "d2r-audio-telemetry-mod",
            "producer": "d2r-audio-mod",
            "protocol_version": PROTOCOL_VERSION,
            "recipe_version": "two",
            "mod_name": "broken"
        }),
    );
    let result = compatibility(&root, "-mod broken -txt");
    assert!(!result.ready);
    assert!(!result.update_required);
    assert_eq!(result.reason_code, "unsupported_mod");
    assert!(result.message.contains("配方版本无效"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn published_old_protocol_can_update_in_place_but_foreign_manifests_remain_blocked() {
    let root = test_mods_directory("incompatible");
    write_test_audio_mod(
        &root,
        "old-protocol",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION - 1,
            "mod_name": "old-protocol"
        }),
    );
    write_test_audio_mod(
        &root,
        "foreign",
        serde_json::json!({
            "manifest_format": "d2r-audio-telemetry-mod",
            "producer": "someone-else",
            "protocol_version": PROTOCOL_VERSION,
            "mod_name": "foreign"
        }),
    );
    let old = compatibility(&root, "-mod old-protocol -txt");
    assert!(!old.ready);
    assert!(old.update_required);
    assert_eq!(old.reason_code, "update_required");
    assert!(old.message.contains("保留原名称直接更新"));

    let foreign = compatibility(&root, "-mod foreign -txt");
    assert!(!foreign.ready);
    assert!(!foreign.update_required);
    assert_eq!(foreign.reason_code, "unsupported_mod");
    std::fs::remove_dir_all(root).unwrap();
}
