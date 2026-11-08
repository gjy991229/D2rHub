//! Content contracts for the generated in-game layouts.
//! Reading a layout never installs it or starts a runtime capability.
#[cfg(test)]
use crate::domain::mod_processing::IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION;
use std::path::Path;

pub(in crate::audio_mod) const ROOM_TOOL_LAYOUT_DIRECTORY: &str = "data/global/ui/layouts";

pub(in crate::audio_mod) fn validate_party_loot(
    mod_directory: &Path,
    mod_name: &str,
    recipe: u32,
) -> Result<(), String> {
    let layouts = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let (file, name) = if layouts.join("mymodsShowListLayouthd.json").is_file() {
        ("mymodsShowListLayouthd.json", "mymodsShowListLayout")
    } else if recipe >= 2 && layouts.join("jcypartyextrahd.json").is_file() {
        ("jcypartyextrahd.json", "JcyPartyExtra")
    } else {
        ("D2RHubPartyLoothd.json", "D2RHubPartyLoot")
    };
    let document = read_room_tool_layout(&layouts, file)?;
    let invalid = || "左侧玩家拾取按钮布局无效，请重新加工".to_string();
    if document["type"] != "PartyPanel" || document["name"] != name {
        return Err(invalid());
    }
    let list = find_layout_node(&document, "ListContainer").ok_or_else(invalid)?;
    let row = find_layout_node(list, "FriendsListTemplate").ok_or_else(invalid)?;
    let button = find_layout_node(row, "LootBodyButton").ok_or_else(invalid)?;
    let fields = &button["fields"];
    if button["type"] != "ToggleButtonWidget"
        || fields["onClickMessage"] != "PartyPanelMessage:ToggleLootBody"
        || fields["filename"] != "PANEL\\Party\\LootBody"
        || fields["untoggledFrame"] != 0
        || fields["toggledFrame"] != 4
        || fields["visible"] == false
    {
        return Err(invalid());
    }
    let x = fields["rect"]["x"].as_f64().ok_or_else(invalid)?;
    let y = fields["rect"]["y"].as_f64().ok_or_else(invalid)?;
    if recipe >= 3 {
        let portraits = read_room_tool_layout(&layouts, "hireablespanelhd.json")?;
        let pf = &portraits["fields"];
        let number = |v: &serde_json::Value| v.as_f64().ok_or_else(invalid);
        let expected_x = number(&pf["rect"]["x"])?;
        let expected_y = number(&pf["rect"]["y"])?;
        let pitch =
            number(&pf["nextIconOffset"]["y"])? * pf["rect"]["scale"].as_f64().unwrap_or(1.0);
        if x != 0.0
            || y != 0.0
            || (number(&list["fields"]["rect"]["x"])? - expected_x).abs() > 0.01
            || (number(&row["fields"]["rect"]["y"])? - expected_y).abs() > 0.01
            || (number(&row["fields"]["rect"]["height"])? - pitch).abs() > 0.01
            || fields["rect"].get("width").is_some()
            || fields["rect"].get("height").is_some()
        {
            return Err(invalid());
        }
    } else if name == "JcyPartyExtra" {
        let row_x = row["fields"]["rect"]["x"].as_f64().ok_or_else(invalid)?;
        let row_y = row["fields"]["rect"]["y"].as_f64().ok_or_else(invalid)?;
        if (x + row_x - 161.0).abs() > 0.01
            || (y + row_y - 280.0).abs() > 0.01
            || row["fields"]["rect"]["width"].as_f64().unwrap_or(0.0) < 300.0
            || document["fields"]["priority"] != -5
        {
            return Err(invalid());
        }
    } else if x != 80.0 || y != 20.0 {
        return Err(invalid());
    }
    let hud = read_room_tool_layout(&layouts, "HudWarningshd.json")?;
    if !layout_has_child_message(&hud, "message", &format!("PanelManager:OpenPanel:{name}")) {
        return Err(invalid());
    }
    if recipe >= 2 && !has_immediate_launcher(&hud, &format!("PanelManager:OpenPanel:{name}")) {
        return Err(invalid());
    }
    Ok(())
}

// Multiple modules may share the immediate startup prefix; none owns index 0.
fn has_immediate_launcher(hud: &serde_json::Value, message: &str) -> bool {
    let Some(children) = hud["children"].as_array() else {
        return false;
    };
    for child in children {
        if child["type"] != "TimerWidget" {
            continue;
        }
        let fields = &child["fields"];
        if fields["time"].as_f64() != Some(-1.0) {
            return false;
        }
        if fields["message"] == message {
            return true;
        }
        if fields["message"].as_str().is_some_and(|m| {
            m.eq_ignore_ascii_case("PanelManager:ClosePanel:HUDWarnings")
                || m.eq_ignore_ascii_case("PanelManager:UnloadPanel:HUDWarnings")
        }) {
            return false;
        }
    }
    false
}

#[cfg(test)]
pub(in crate::audio_mod) fn validate_scene_shortcuts(
    mod_directory: &Path,
    mod_name: &str,
) -> Result<(), String> {
    validate_scene_shortcuts_version(mod_directory, mod_name, 7)
}

pub(in crate::audio_mod) fn validate_scene_shortcuts_version(
    mod_directory: &Path,
    mod_name: &str,
    version: u32,
) -> Result<(), String> {
    if !(1..=7).contains(&version) {
        return Err(format!("不支持的场景重开配方 r{version}"));
    }
    let root = mod_directory.join(format!("{mod_name}.mpq"));
    let layouts = root.join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let invalid = || format!("场景重开 r{version} 布局缺失、时序变化或帮助页冲突，请重新加工");
    if version >= 6 {
        validate_exact_scene(&root, &layouts)?;
        if version >= 7 {
            validate_scene_actinfo(&root)?;
        }
        return Ok(());
    }
    for (file, name) in [
        ("D2RHubSceneRestarthd.json", "D2RHubSceneRestart"),
        ("helppanelhd.json", "helppanel"),
    ] {
        let value = read_room_tool_layout(&layouts, file)?;
        if value["type"] != "PausePanel" || value["name"] != name {
            return Err(invalid());
        }
        let expected = [
            (
                -1.0,
                "PanelManager:OpenPanel:WaypointsPanelExpansion".to_string(),
            ),
            (0.1, "PanelManager:OpenPanel:D2RHubSceneWaypoint".into()),
            (0.2, "PanelManager:OpenPanel:D2RHubSceneRecreate".into()),
            (0.3, format!("PanelManager:UnloadPanel:{name}")),
        ];
        let children = value["children"].as_array().ok_or_else(invalid)?;
        if children.len() != expected.len() {
            return Err(invalid());
        }
        for (child, (time, message)) in children.iter().zip(expected) {
            if child["type"] != "TimerWidget"
                || child["fields"]["time"].as_f64() != Some(time)
                || child["fields"]["message"] != message
            {
                return Err(invalid());
            }
        }
    }
    let wp = read_room_tool_layout(&layouts, "D2RHubSceneWaypointhd.json")?;
    let tabs = find_layout_node(&wp, "Tabs").ok_or_else(invalid)?;
    if wp["type"] != "WaypointsPanel"
        || tabs["fields"]["defaultTab"] != 2
        || tabs["fields"]["tabCount"] != if version == 2 { 3 } else { 4 }
        || tabs["fields"]["onSwitchTabMessage"] != "Waypoints:SelectWaypoint"
    {
        return Err(invalid());
    }
    for panel in [
        "D2RHubSceneWaypoint",
        "WaypointsPanelExpansion",
        if version <= 2 {
            "D2RHubSceneRestart"
        } else {
            "D2RHubSceneArrival"
        },
    ] {
        if !layout_has_timed_child_message(&wp, &format!("PanelManager:UnloadPanel:{panel}"), 0.0) {
            return Err(invalid());
        }
    }
    if version >= 3 {
        let arrival_wp = read_room_tool_layout(&layouts, "D2RHubSceneArrivalWaypointhd.json")?;
        let arrival_tabs = find_layout_node(&arrival_wp, "Tabs").ok_or_else(invalid)?;
        if arrival_wp["type"] != "WaypointsPanel"
            || arrival_tabs["fields"]["tabCount"] != 3
            || arrival_tabs["fields"]["defaultTab"] != 2
            || arrival_tabs["fields"]["onSwitchTabMessage"] != "Waypoints:SelectWaypoint"
        {
            return Err(invalid());
        }
        for panel in [
            "D2RHubSceneArrivalWaypoint",
            "WaypointsPanelExpansion",
            "D2RHubSceneArrival",
        ] {
            if !layout_has_timed_child_message(
                &arrival_wp,
                &format!("PanelManager:UnloadPanel:{panel}"),
                0.0,
            ) {
                return Err(invalid());
            }
        }
        for (file, kind, expected) in [
            (
                "D2RHubSceneEntryhd.json",
                "Panel",
                vec![
                    (1.0, "PanelManager:OpenPanel:D2RHubSceneArrival"),
                    (1.4, "PanelManager:UnloadPanel:D2RHubSceneEntry"),
                ],
            ),
            (
                "D2RHubSceneArrivalhd.json",
                "PausePanel",
                vec![
                    (-1.0, "PanelManager:OpenPanel:WaypointsPanelExpansion"),
                    (0.1, "PanelManager:OpenPanel:D2RHubSceneArrivalWaypoint"),
                    (0.3, "PanelManager:UnloadPanel:D2RHubSceneArrival"),
                ],
            ),
        ] {
            if version >= 5 && file == "D2RHubSceneEntryhd.json" {
                continue;
            }
            let doc = read_room_tool_layout(&layouts, file)?;
            let nodes = doc["children"].as_array().ok_or_else(invalid)?;
            if doc["type"] != kind || nodes.len() != expected.len() {
                return Err(invalid());
            }
            for (node, (time, message)) in nodes.iter().zip(expected) {
                if node["type"] != "TimerWidget"
                    || node["fields"]["time"].as_f64() != Some(time)
                    || node["fields"]["message"] != message
                {
                    return Err(invalid());
                }
            }
        }
    }
    let restart = read_room_tool_layout(&layouts, "D2RHubSceneRecreatehd.json")?;
    let children = restart["children"].as_array().ok_or_else(invalid)?;
    if restart["type"] != "PausePanel" || children.len() != 4 {
        return Err(invalid());
    }
    for (child, message) in children.iter().zip([
        "PanelManager:OpenPanel:D2RHubSceneExit",
        "PanelManager:OpenPanel:D2RHubSceneLoadHell",
        "PanelManager:UnloadPanel:D2RHubSceneRecreate",
        "PanelManager:UnloadPanel:helppanel",
    ]) {
        if child["fields"]["time"].as_f64() != Some(-1.0) || child["fields"]["message"] != message {
            return Err(invalid());
        }
    }
    let exit = read_room_tool_layout(&layouts, "D2RHubSceneExithd.json")?;
    let row = find_layout_node(&exit, "Row Save and Exit Game").ok_or_else(invalid)?;
    if exit["type"] != "PausePanel"
        || !layout_has_timed_child_message(row, "PausePanelMessage:ExitGame", -1.0)
    {
        return Err(invalid());
    }
    let load = read_room_tool_layout(&layouts, "D2RHubSceneLoadHellhd.json")?;
    if load["type"] != "MainMenuHDPanel"
        || !layout_has_timed_child_message(&load, "CharacterSelect:LoadCharacter:2", -1.0)
    {
        return Err(invalid());
    }
    let hud = read_room_tool_layout(&layouts, "HudWarningshd.json")?;
    if version == 5 {
        let nodes = hud["children"].as_array().ok_or_else(invalid)?;
        let index = nodes
            .iter()
            .position(|n| n["name"] == "D2RHubSceneArrivalLauncher")
            .ok_or_else(invalid)?;
        if nodes[index]["type"] != "TimerWidget"
            || nodes[index]["fields"]["time"].as_f64() != Some(1.0)
            || nodes[index]["fields"]["message"] != "PanelManager:OpenPanel:D2RHubSceneArrival"
            || find_layout_node(&hud, "D2RHubSceneEntryLauncher").is_some()
            || find_layout_node(&hud, "D2RHubSceneToolbarLauncher").is_some()
            || layouts.join("D2RHubSceneEntryhd.json").exists()
            || layouts.join("D2RHubSceneToolbarhd.json").exists()
        {
            return Err(invalid());
        }
        for (i, node) in nodes.iter().enumerate() {
            if node["fields"]["message"].as_str().is_some_and(|m| {
                m.eq_ignore_ascii_case("PanelManager:ClosePanel:HUDWarnings")
                    || m.eq_ignore_ascii_case("PanelManager:UnloadPanel:HUDWarnings")
            }) {
                let time = node["fields"]["time"].as_f64().ok_or_else(invalid)?;
                if time < 1.0 || (time == 1.0 && i < index) {
                    return Err(invalid());
                }
            }
        }
    } else if version == 4 {
        if !has_immediate_launcher(&hud, "PanelManager:OpenPanel:D2RHubSceneEntry")
            || !layout_has_timed_child_message(
                &hud,
                "PanelManager:OpenPanel:D2RHubSceneEntry",
                -1.0,
            )
            || find_layout_node(&hud, "D2RHubSceneToolbarLauncher").is_some()
            || layouts.join("D2RHubSceneToolbarhd.json").exists()
        {
            return Err(invalid());
        }
    } else {
        let toolbar = read_room_tool_layout(&layouts, "D2RHubSceneToolbarhd.json")?;
        let button = find_layout_node(&toolbar, "D2RHubSceneRestartButton").ok_or_else(invalid)?;
        if !layout_has_timed_child_message(
            &hud,
            "PanelManager:OpenPanel:D2RHubSceneToolbar",
            if version == 1 { 0.01 } else { -1.0 },
        ) || button["fields"]["onClickMessage"] != "PanelManager:OpenPanel:D2RHubSceneRestart"
            || (version >= 2
                && !has_immediate_launcher(&hud, "PanelManager:OpenPanel:D2RHubSceneToolbar"))
            || (version == 3
                && (!has_immediate_launcher(&hud, "PanelManager:OpenPanel:D2RHubSceneEntry")
                    || !layout_has_timed_child_message(
                        &hud,
                        "PanelManager:OpenPanel:D2RHubSceneEntry",
                        -1.0,
                    )))
        {
            return Err(invalid());
        }
    }
    let backup = read_room_tool_layout(&root, "d2rhub-scene-shortcuts-original-help.json")?;
    if !backup.is_object() {
        return Err(invalid());
    }
    Ok(())
}
fn validate_scene_actinfo(root: &Path) -> Result<(), String> {
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../resources/scene-shortcuts-v7-actinfo.json"
    ))
    .map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(root.join("data/global/excel/actinfo.txt"))
        .map_err(|e| format!("场景重开缺少传送点映射 actinfo.txt：{e}"))?;
    let rows: Vec<Vec<&str>> = text
        .lines()
        .map(|l| l.trim_end_matches('\r').split('\t').collect())
        .collect();
    let header = rows.first().ok_or("actinfo.txt 为空")?;
    let column = |name: &str| -> Result<usize, String> {
        let indices: Vec<_> = header
            .iter()
            .enumerate()
            .filter(|(_, h)| h.trim_start_matches('\u{feff}') == name)
            .map(|(i, _)| i)
            .collect();
        if indices.len() != 1 {
            return Err(format!("actinfo.txt 列缺失或重复：{name}"));
        }
        Ok(indices[0])
    };
    let act_index = column("act")?;
    for item in expected.as_array().ok_or("小站参考映射无效")? {
        let act = item["act"].as_str().ok_or("小站参考幕编号无效")?;
        let matches: Vec<_> = rows
            .iter()
            .skip(1)
            .filter(|r| r.get(act_index) == Some(&act))
            .collect();
        if matches.len() != 1 {
            return Err(format!("actinfo.txt 幕编号缺失或重复：{act}"));
        }
        for (field, value) in item["waypoints"].as_object().ok_or("小站映射无效")? {
            if matches[0].get(column(field)?).copied() != value.as_str() {
                return Err(format!(
                    "场景重开传送点映射缺失：act={act}, {field}，需按MYMOD同步actinfo.txt"
                ));
            }
        }
    }
    Ok(())
}

fn validate_exact_scene(root: &Path, layouts: &Path) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../resources/scene-shortcuts-v6.json"
    ))
    .map_err(|e| e.to_string())?;
    for entry in manifest.as_array().ok_or("场景模板清单无效")? {
        let file = entry["path"].as_str().ok_or("场景模板路径缺失")?;
        let bytes = std::fs::read(layouts.join(file))
            .map_err(|e| format!("场景重开 r6 文件缺失：{file}：{e}"))?;
        if format!("{:x}", Sha256::digest(bytes)) != entry["sha256"].as_str().unwrap_or("") {
            return Err(format!("场景重开 r6 文件与参考不一致：{file}"));
        }
    }
    let hud = read_room_tool_layout(layouts, "HudWarningshd.json")?;
    let children = hud["children"].as_array().ok_or("场景重开 HUD 缺少子项")?;
    if children.len() < 2 {
        return Err("场景重开 HUD 缺少启动项".into());
    }
    for (node, message) in children[children.len() - 2..].iter().zip([
        "PanelManager:UnloadPanel:HUDWarnings",
        "PanelManager:OpenPanel:CALL/OnekeyTravincal",
    ]) {
        if node["type"] != "TimerWidget"
            || node["fields"]["time"].as_f64() != Some(1.0)
            || node["fields"]["message"] != message
        {
            return Err("场景重开 r6 HUD 收尾时序不一致".into());
        }
    }
    for node in &children[..children.len() - 2] {
        let message = node["fields"]["message"].as_str().unwrap_or("");
        if message.contains("D2RHubScene")
            || message == "PanelManager:OpenPanel:CALL/OnekeyTravincal"
            || message.eq_ignore_ascii_case("PanelManager:ClosePanel:HUDWarnings")
            || message.eq_ignore_ascii_case("PanelManager:UnloadPanel:HUDWarnings")
            || (message == "PanelManager:OpenPanel:JcyHudWarnings"
                && node["fields"]["time"].as_f64() != Some(1.0))
        {
            return Err("场景重开 r6 存在冲突的 HUD 启动项".into());
        }
    }
    for name in [
        "Restart",
        "Waypoint",
        "Recreate",
        "Exit",
        "LoadHell",
        "Toolbar",
        "Arrival",
        "ArrivalWaypoint",
        "Entry",
    ] {
        if layouts.join(format!("D2RHubScene{name}hd.json")).exists() {
            return Err(format!("场景重开遗留旧控制器：{name}"));
        }
    }
    let backup = read_room_tool_layout(root, "d2rhub-scene-shortcuts-original-help.json")?;
    if !backup.is_object() {
        return Err("场景重开帮助页备份无效".into());
    }
    Ok(())
}

const ROOM_TOOLBAR_OPEN_MESSAGE: &str = "PanelManager:OpenPanel:D2RHubRoomToolbar";
const ROOM_TOOLBAR_CLOSE_MESSAGE: &str = "PanelManager:ClosePanel:D2RHubRoomToolbar";
const ROOM_TOOL_GATEWAY_HUB: &str = "D2RHubKeyboardGatewayHub";
const ROOM_TOOL_CREATE_GATEWAY: &str = "D2RHubKeyboardCreateGateway";
const ROOM_TOOL_JOIN_GATEWAY: &str = "D2RHubKeyboardJoinGateway";
const AUTO_EXIT_ON_DEATH_PANEL: &str = "D2RHubAutoExitOnDeath";
pub(in crate::audio_mod) const NEXT_GAME_TOOLTIP_OFFSET_Y: i64 = 267;
pub(in crate::audio_mod) const ROOM_TOOL_BUTTON_SCALE: f64 = 0.30;
pub(in crate::audio_mod) const ROOM_TOOL_BUTTON_Y: i64 = 12;
pub(in crate::audio_mod) const ROOM_TOOL_NEXT_X: i64 = -1_040;
pub(in crate::audio_mod) const ROOM_TOOL_CREATE_X: i64 = -760;
pub(in crate::audio_mod) const ROOM_TOOL_JOIN_X: i64 = -480;
const QUICK_RECREATE_DOUBLE_CLICK_WINDOW_SECONDS: f64 = 0.5;
const ROOM_TRANSITION_OPEN_PAUSE_DELAY_SECONDS: f64 = 0.01;
const ROOM_TRANSITION_EXIT_DELAY_SECONDS: f64 = 0.05;
const ROOM_TRANSITION_COMMIT_DELAY_SECONDS: f64 = ROOM_TRANSITION_EXIT_DELAY_SECONDS;
const ROOM_TRANSITION_CLOSE_DELAY_SECONDS: f64 = ROOM_TRANSITION_EXIT_DELAY_SECONDS;
fn read_room_tool_layout(layout_directory: &Path, name: &str) -> Result<serde_json::Value, String> {
    let path = layout_directory.join(name);
    let bytes = std::fs::read(&path).map_err(|_| format!("局内房间工具缺少布局文件：{name}"))?;
    serde_json::from_slice(&bytes).map_err(|_| format!("局内房间工具布局已损坏：{name}"))
}

fn layout_has_child_message(document: &serde_json::Value, field: &str, expected: &str) -> bool {
    document
        .get("children")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|children| {
            children.iter().any(|child| {
                child
                    .get("fields")
                    .and_then(|fields| fields.get(field))
                    .and_then(serde_json::Value::as_str)
                    == Some(expected)
            })
        })
}

fn room_toolbar_visibility(hud: &serde_json::Value) -> Result<bool, String> {
    match (
        layout_has_child_message(hud, "message", ROOM_TOOLBAR_OPEN_MESSAGE),
        layout_has_child_message(hud, "message", ROOM_TOOLBAR_CLOSE_MESSAGE),
    ) {
        (true, false) => Ok(true),
        (false, true) => Ok(false),
        _ => Err("局内按钮的 HUD 显示配置缺失或冲突，请重新加工".to_string()),
    }
}

fn layout_has_timed_child_message(
    document: &serde_json::Value,
    expected: &str,
    expected_time: f64,
) -> bool {
    document
        .get("children")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|children| {
            children.iter().any(|child| {
                let fields = child.get("fields");
                fields
                    .and_then(|value| value.get("message"))
                    .and_then(serde_json::Value::as_str)
                    == Some(expected)
                    && fields
                        .and_then(|value| value.get("time"))
                        .and_then(serde_json::Value::as_f64)
                        .is_some_and(|time| (time - expected_time).abs() < f64::EPSILON)
            })
        })
}

fn layout_field_value_count(document: &serde_json::Value, expected: &str) -> usize {
    let own = document
        .get("fields")
        .and_then(serde_json::Value::as_object)
        .map_or(0, |fields| {
            fields
                .values()
                .filter(|value| value.as_str() == Some(expected))
                .count()
        });
    own + document
        .get("children")
        .and_then(serde_json::Value::as_array)
        .map_or(0, |children| {
            children
                .iter()
                .map(|child| layout_field_value_count(child, expected))
                .sum()
        })
}

fn find_layout_node<'a>(
    document: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    if document.get("name").and_then(serde_json::Value::as_str) == Some(name) {
        return Some(document);
    }
    document
        .get("children")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find_map(|child| find_layout_node(child, name))
}

fn validate_routed_pause_buttons(
    node: &serde_json::Value,
    pause_name: &str,
) -> Result<usize, String> {
    let mut routed = 0;
    if node.get("type").and_then(serde_json::Value::as_str) == Some("ButtonWidget") {
        let name = node.get("name").and_then(serde_json::Value::as_str);
        let is_gateway = name.is_some_and(|name| {
            [
                ROOM_TOOL_GATEWAY_HUB,
                ROOM_TOOL_CREATE_GATEWAY,
                ROOM_TOOL_JOIN_GATEWAY,
            ]
            .contains(&name)
        });
        if !is_gateway {
            if node
                .pointer("/fields/navigation/left/name")
                .and_then(serde_json::Value::as_str)
                != Some(ROOM_TOOL_CREATE_GATEWAY)
                || node
                    .pointer("/fields/navigation/right/name")
                    .and_then(serde_json::Value::as_str)
                    != Some(ROOM_TOOL_JOIN_GATEWAY)
            {
                return Err(format!(
                    "暂停布局按钮未连接安全键盘入口：{pause_name}/{}",
                    name.unwrap_or("<unnamed>")
                ));
            }
            routed += 1;
        }
    }

    if let Some(children) = node.get("children") {
        let children = children
            .as_array()
            .ok_or_else(|| format!("暂停布局 children 不是数组：{pause_name}"))?;
        for child in children {
            routed += validate_routed_pause_buttons(child, pause_name)?;
        }
    }
    Ok(routed)
}

fn validate_pause_esc_bindings(
    node: &serde_json::Value,
    pause_name: &str,
    room_recipe_version: u32,
) -> Result<(), String> {
    let is_button = node.get("type").and_then(serde_json::Value::as_str) == Some("ButtonWidget");
    let name = node.get("name").and_then(serde_json::Value::as_str);
    let returns_to_game = is_button && name == Some("ReturnToGame");
    let accepts_esc = node.pointer("/fields/acceptsEscKeyEverywhere");
    if (is_button || accepts_esc.is_some())
        && accepts_esc.and_then(serde_json::Value::as_bool) != Some(returns_to_game)
    {
        return Err(format!(
            "暂停布局仍包含其他 Mod 的 Esc 绑定或缺少统一绑定，请重新加工：{pause_name}/{}",
            name.unwrap_or("<unnamed>")
        ));
    }
    if returns_to_game
        && node
            .pointer("/fields/onClickMessage")
            .and_then(serde_json::Value::as_str)
            != Some(if room_recipe_version >= 27 {
                "PanelManager:OpenPanel:D2RHubPauseReturnToGame"
            } else {
                "PausePanelMessage:Close"
            })
    {
        return Err(format!(
            "暂停布局 Esc 未绑定返回游戏，请重新加工：{pause_name}"
        ));
    }
    if let Some(children) = node.get("children") {
        let children = children
            .as_array()
            .ok_or_else(|| format!("暂停布局 children 不是数组：{pause_name}"))?;
        for child in children {
            validate_pause_esc_bindings(child, pause_name, room_recipe_version)?;
        }
    }
    Ok(())
}

fn read_auto_exit_on_death_layout(
    layout_directory: &Path,
    name: &str,
) -> Result<serde_json::Value, String> {
    let path = layout_directory.join(name);
    let bytes = std::fs::read(&path).map_err(|_| format!("死亡后自动退出缺少布局文件：{name}"))?;
    serde_json::from_slice(&bytes).map_err(|_| format!("死亡后自动退出布局已损坏：{name}"))
}

pub(in crate::audio_mod) fn auto_exit_on_death_layout_enabled(
    mod_directory: &Path,
    mod_name: &str,
) -> Result<bool, String> {
    let layout_directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let death_modal = read_auto_exit_on_death_layout(&layout_directory, "youdiedmodalhd.json")?;
    let launcher = find_layout_node(&death_modal, "D2RHubAutoExitOnDeathLauncher");
    let launcher_is_valid = launcher.is_some_and(|launcher| {
        launcher.get("type").and_then(serde_json::Value::as_str) == Some("TimerWidget")
            && launcher
                .pointer("/fields/message")
                .and_then(serde_json::Value::as_str)
                == Some("PanelManager:OpenPanel:D2RHubAutoExitOnDeath")
            && launcher
                .pointer("/fields/time")
                .and_then(serde_json::Value::as_f64)
                .is_some_and(|time| (time - 0.01).abs() < f64::EPSILON)
    });
    if launcher.is_some() && !launcher_is_valid {
        return Err("死亡界面的自动退出入口无效".to_string());
    }
    let has_legacy_exit_timer = death_modal
        .get("children")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|children| {
            children.iter().any(|child| {
                child.get("type").and_then(serde_json::Value::as_str) == Some("TimerWidget")
                    && child
                        .pointer("/fields/message")
                        .and_then(serde_json::Value::as_str)
                        == Some("PanelManager:OpenPanel:exitgame")
            })
        });
    if has_legacy_exit_timer {
        return Err("死亡界面仍包含未规范化的原生 exitgame 定时入口".to_string());
    }
    Ok(launcher_is_valid)
}

pub(in crate::audio_mod) fn validate_auto_exit_on_death_layouts(
    mod_directory: &Path,
    mod_name: &str,
    enabled: bool,
) -> Result<(), String> {
    let layout_directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    if auto_exit_on_death_layout_enabled(mod_directory, mod_name)? != enabled {
        return Err("死亡界面的自动退出启用状态与配置不一致".to_string());
    }

    let panel_name = format!("{AUTO_EXIT_ON_DEATH_PANEL}hd.json");
    let exit_panel = read_auto_exit_on_death_layout(&layout_directory, &panel_name)?;
    if exit_panel.get("type").and_then(serde_json::Value::as_str) != Some("PausePanel")
        || exit_panel.get("name").and_then(serde_json::Value::as_str)
            != Some(AUTO_EXIT_ON_DEATH_PANEL)
        || !layout_has_timed_child_message(&exit_panel, "PausePanelMessage:ExitGame", 0.1)
    {
        return Err("死亡后自动退出面板的退出消息链无效".to_string());
    }

    let stub_name = format!("{AUTO_EXIT_ON_DEATH_PANEL}.json");
    let stub = read_auto_exit_on_death_layout(&layout_directory, &stub_name)?;
    if stub.get("type").and_then(serde_json::Value::as_str) != Some("Panel")
        || stub.get("name").and_then(serde_json::Value::as_str) != Some(AUTO_EXIT_ON_DEATH_PANEL)
    {
        return Err("死亡后自动退出面板入口无效".to_string());
    }
    Ok(())
}

pub(in crate::audio_mod) fn validate_lobby_return_hint(
    mod_directory: &Path,
    mod_name: &str,
) -> Result<(), String> {
    let layout_directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let lobby = read_room_tool_layout(&layout_directory, "lobbybackgroundpanelhd.json")?;
    let hint = find_layout_node(&lobby, "D2RHubLobbyReturnHint")
        .ok_or_else(|| "局内房间工具缺少大厅 Esc 返回提示，请重新加工".to_string())?;
    let expected = serde_json::json!({
        "type": "TextBoxWidget",
        "name": "D2RHubLobbyReturnHint",
        "fields": {
            "rect": { "x": -50, "y": 0 },
            "text": "按 Esc 键返回",
            "style": {
                "alignment": { "h": "center", "v": "center" },
                "fontColor": "$FontColorDarkGold",
                "pointSize": 120
            }
        }
    });
    if hint != &expected {
        return Err("大厅 Esc 返回提示的文案或格式无效，请重新加工".to_string());
    }
    Ok(())
}

pub(in crate::audio_mod) fn validate_esc_next_game_layouts(
    mod_directory: &Path,
    mod_name: &str,
    recipe_version: u32,
) -> Result<(), String> {
    let directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    if recipe_version == 3 {
        let hud = read_room_tool_layout(&directory, "HudWarningshd.json")?;
        if layout_field_value_count(&hud, "PanelManager:ClosePanel:D2RHubQuickRecreateEscArm") != 0
        {
            return Err("HUD 仍包含会取消双击 Esc 的旧清理动作，请重新加工".to_string());
        }
    }
    if recipe_version >= 4 {
        let hud = read_room_tool_layout(&directory, "HudWarningshd.json")?;
        if layout_field_value_count(&hud, "PanelManager:ClosePanel:D2RHubQuickRecreateEscArm") != 0
        {
            return Err("双击 Esc 模块的 HUD 不应关闭 Esc 接收器，请重新加工".into());
        }
    }
    let arm = read_room_tool_layout(&directory, "D2RHubQuickRecreateEscArmhd.json")?;
    let receiver = find_layout_node(&arm, "D2RHubEscNextGame")
        .ok_or_else(|| "缺少双击 Esc 下一局入口".to_string())?;
    if arm.get("type").and_then(serde_json::Value::as_str) != Some("TooltipsPanel")
        || receiver
            .pointer("/fields/acceptsEscKeyEverywhere")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || receiver
            .pointer("/fields/onClickMessage")
            .and_then(serde_json::Value::as_str)
            != Some("PanelManager:OpenPanel:D2RHubQuickRecreate")
        || !layout_has_timed_child_message(
            &arm,
            "PanelManager:ClosePanel:D2RHubQuickRecreateEscArm",
            0.5,
        )
    {
        return Err("双击 Esc 下一局接收器无效".to_string());
    }
    for name in ["pauselayouthd.json", "pauselayoutgardenhd.json"] {
        let pause = read_room_tool_layout(&directory, name)?;
        if !layout_has_timed_child_message(
            &pause,
            "PanelManager:OpenPanel:D2RHubQuickRecreateEscArm",
            0.01,
        ) {
            return Err(format!("暂停布局缺少双击 Esc 入口：{name}"));
        }
        if !layout_has_timed_child_message(
            &pause,
            "PanelManager:ClosePanel:D2RHubQuickRecreateEscArm",
            QUICK_RECREATE_DOUBLE_CLICK_WINDOW_SECONDS,
        ) {
            return Err(format!("暂停布局缺少双击 Esc 超时关闭：{name}，请重新加工"));
        }
    }
    let controller = read_room_tool_layout(&directory, "D2RHubQuickRecreatehd.json")?;
    let messages = controller
        .get("children")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "下一局控制器不完整".to_string())?;
    let exit = messages.iter().position(|child| {
        child
            .pointer("/fields/message")
            .and_then(serde_json::Value::as_str)
            == Some("PausePanelMessage:ExitGame")
    });
    let load = messages.iter().position(|child| {
        child
            .pointer("/fields/message")
            .and_then(serde_json::Value::as_str)
            == Some("CharacterSelect:LoadCharacter:2")
    });
    if !matches!((exit, load), (Some(exit), Some(load)) if exit < load)
        || !layout_has_timed_child_message(&controller, "PausePanelMessage:ExitGame", 0.05)
        || !layout_has_timed_child_message(&controller, "CharacterSelect:LoadCharacter:2", 0.05)
    {
        return Err("下一局地狱必须先正常退出再加载角色".to_string());
    }
    Ok(())
}

#[cfg(test)]
pub(in crate::audio_mod) fn validate_in_game_room_tool_layouts_for_recipe(
    mod_directory: &Path,
    mod_name: &str,
    require_room_submission_transition: bool,
) -> Result<(), String> {
    validate_in_game_room_tool_layouts_for_version(
        mod_directory,
        mod_name,
        if require_room_submission_transition {
            IN_GAME_ROOM_TOOLS_FEATURE_RECIPE_VERSION
        } else {
            21
        },
    )
}

pub(in crate::audio_mod) fn validate_in_game_room_tool_layouts_for_version(
    mod_directory: &Path,
    mod_name: &str,
    room_recipe_version: u32,
) -> Result<(), String> {
    let default_hell = super::super::room_tool_options::default_hell(mod_directory)?;
    // Old recipes remain readable as upgrade sources. r26 separates the lobby
    // and in-game forms, and queues exit + submit at the same time like JCY.
    let requires_input_safety = room_recipe_version >= 24;
    let separate_in_game_forms = room_recipe_version >= 26;
    let commit_delay = if separate_in_game_forms {
        ROOM_TRANSITION_COMMIT_DELAY_SECONDS
    } else if requires_input_safety {
        0.20
    } else {
        0.05
    };
    let close_delay = if separate_in_game_forms {
        ROOM_TRANSITION_CLOSE_DELAY_SECONDS
    } else {
        0.25
    };
    let create_form_panel = if separate_in_game_forms {
        "D2RHubInGameCreateGame"
    } else {
        "CreateGamePanel"
    };
    let join_form_panel = if separate_in_game_forms {
        "D2RHubInGameJoinGame"
    } else {
        "JoinGamePanel"
    };
    let layout_directory = mod_directory
        .join(format!("{mod_name}.mpq"))
        .join(ROOM_TOOL_LAYOUT_DIRECTORY);
    let hud = read_room_tool_layout(&layout_directory, "HudWarningshd.json")?;
    // Legacy visibility is accepted only when reading an upgrade source.
    let toolbar_visible = room_toolbar_visibility(&hud)?;
    if room_recipe_version >= 27 && toolbar_visible {
        return Err("局内按钮应始终隐藏，请重新加工".to_string());
    }

    let toolbar = read_room_tool_layout(&layout_directory, "D2RHubRoomToolbarhd.json")?;
    if room_recipe_version >= 27 {
        if toolbar
            .pointer("/fields/rect/x")
            .and_then(serde_json::Value::as_i64)
            != Some(-9999)
            || toolbar
                .pointer("/fields/rect/y")
                .and_then(serde_json::Value::as_i64)
                != Some(-9999)
        {
            return Err("局内工具栏没有固定隐藏，请重新加工".to_string());
        }
        let return_helper =
            read_room_tool_layout(&layout_directory, "D2RHubPauseReturnToGamehd.json")?;
        if !layout_has_timed_child_message(
            &return_helper,
            "PanelManager:ClosePanel:D2RHubQuickRecreateEscArm",
            0.001,
        ) || !layout_has_timed_child_message(&return_helper, "PausePanelMessage:Close", 0.005)
        {
            return Err("暂停菜单返回入口无效，请重新加工".to_string());
        }
        let esc_arm = read_room_tool_layout(&layout_directory, "D2RHubQuickRecreateEscArmhd.json")?;
        let receiver = find_layout_node(&esc_arm, "D2RHubEscNextGame")
            .ok_or_else(|| "缺少双击 Esc 下一局入口，请重新加工".to_string())?;
        if esc_arm.get("type").and_then(serde_json::Value::as_str) != Some("TooltipsPanel")
            || esc_arm
                .pointer("/fields/priority")
                .and_then(serde_json::Value::as_i64)
                != Some(9002)
            || receiver
                .pointer("/fields/acceptsEscKeyEverywhere")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
            || receiver
                .pointer("/fields/acceptsReturnKey")
                .and_then(serde_json::Value::as_bool)
                != Some(false)
            || receiver
                .pointer("/fields/onClickMessage")
                .and_then(serde_json::Value::as_str)
                != Some("PanelManager:OpenPanel:D2RHubQuickRecreate")
            || !layout_has_timed_child_message(
                &esc_arm,
                "PanelManager:ClosePanel:D2RHubQuickRecreateEscArm",
                QUICK_RECREATE_DOUBLE_CLICK_WINDOW_SECONDS,
            )
        {
            return Err("双击 Esc 下一局入口或双击时限无效，请重新加工".to_string());
        }
    }
    for action in [
        "PanelManager:OpenPanel:D2RHubQuickRecreateArm",
        "PanelManager:OpenPanel:D2RHubOpenCreateGame",
        "PanelManager:OpenPanel:D2RHubOpenJoinGame",
    ] {
        if !layout_has_child_message(&toolbar, "onClickMessage", action) {
            return Err("局内房间工具栏按钮不完整".to_string());
        }
    }
    for (name, expected_x) in [
        ("D2RHubNextGame", ROOM_TOOL_NEXT_X),
        ("D2RHubCreateGame", ROOM_TOOL_CREATE_X),
        ("D2RHubJoinGame", ROOM_TOOL_JOIN_X),
    ] {
        let button = find_layout_node(&toolbar, name)
            .ok_or_else(|| format!("局内房间工具栏缺少按钮 {name}"))?;
        let scale = button
            .pointer("/fields/rect/scale")
            .and_then(serde_json::Value::as_f64);
        if button
            .pointer("/fields/rect/x")
            .and_then(serde_json::Value::as_i64)
            != Some(expected_x)
            || button
                .pointer("/fields/rect/y")
                .and_then(serde_json::Value::as_i64)
                != Some(ROOM_TOOL_BUTTON_Y)
            || scale.is_none_or(|value| (value - ROOM_TOOL_BUTTON_SCALE).abs() > f64::EPSILON)
        {
            return Err(format!("局内房间工具栏按钮尺寸或位置无效：{name}"));
        }
    }
    let next_game = find_layout_node(&toolbar, "D2RHubNextGame")
        .ok_or_else(|| "局内“下一局”按钮不完整".to_string())?;
    if next_game
        .pointer("/fields/tooltipString")
        .and_then(serde_json::Value::as_str)
        .is_none_or(|tooltip| tooltip.is_empty() || tooltip.starts_with('@'))
        || next_game
            .pointer("/fields/tooltipOffset/y")
            .and_then(serde_json::Value::as_i64)
            != Some(NEXT_GAME_TOOLTIP_OFFSET_Y)
    {
        return Err("局内“下一局”第一层提示位置或文字无效".to_string());
    }

    let arm = read_room_tool_layout(&layout_directory, "D2RHubQuickRecreateArmhd.json")?;
    let armed_next = find_layout_node(&arm, "D2RHubArmedNextGame")
        .ok_or_else(|| "局内“下一局”双击接收按钮不完整".to_string())?;
    if arm.get("type").and_then(serde_json::Value::as_str) != Some("TooltipsPanel")
        || !layout_has_child_message(
            &arm,
            "onClickMessage",
            "PanelManager:OpenPanel:D2RHubQuickRecreate",
        )
        || !layout_has_timed_child_message(
            &arm,
            "PanelManager:ClosePanel:D2RHubQuickRecreateArm",
            QUICK_RECREATE_DOUBLE_CLICK_WINDOW_SECONDS,
        )
        || armed_next
            .pointer("/fields/rect/x")
            .and_then(serde_json::Value::as_i64)
            != Some(ROOM_TOOL_NEXT_X)
        || armed_next
            .pointer("/fields/rect/y")
            .and_then(serde_json::Value::as_i64)
            != Some(ROOM_TOOL_BUTTON_Y)
        || armed_next
            .pointer("/fields/rect/scale")
            .and_then(serde_json::Value::as_f64)
            .is_none_or(|value| (value - ROOM_TOOL_BUTTON_SCALE).abs() > f64::EPSILON)
    {
        return Err("局内“下一局”双击窗口不完整".to_string());
    }

    let quick_recreate = read_room_tool_layout(&layout_directory, "D2RHubQuickRecreatehd.json")?;
    if !layout_has_timed_child_message(
        &quick_recreate,
        "PanelManager:OpenPanel:PauseLayoutGarden",
        ROOM_TRANSITION_OPEN_PAUSE_DELAY_SECONDS,
    ) || !layout_has_timed_child_message(
        &quick_recreate,
        "PausePanelMessage:ExitGame",
        ROOM_TRANSITION_EXIT_DELAY_SECONDS,
    ) || !layout_has_timed_child_message(
        &quick_recreate,
        "CharacterSelect:LoadCharacter:2",
        commit_delay,
    ) {
        return Err("局内“下一局”动作无效".to_string());
    }
    if requires_input_safety
        && !layout_has_timed_child_message(
            &quick_recreate,
            "PanelManager:ClosePanel:D2RHubQuickRecreate",
            close_delay,
        )
    {
        return Err("局内“下一局”控制器未延后关闭，请重新加工".to_string());
    }
    let quick_messages = quick_recreate
        .get("children")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "局内“下一局”消息链无效".to_string())?;
    let exit_index = quick_messages
        .iter()
        .position(|child| {
            child
                .pointer("/fields/message")
                .and_then(serde_json::Value::as_str)
                == Some("PausePanelMessage:ExitGame")
        })
        .ok_or_else(|| "局内“下一局”缺少正常退出消息".to_string())?;
    let load_index = quick_messages
        .iter()
        .position(|child| {
            child
                .pointer("/fields/message")
                .and_then(serde_json::Value::as_str)
                == Some("CharacterSelect:LoadCharacter:2")
        })
        .ok_or_else(|| "局内“下一局”缺少地狱加载消息".to_string())?;
    if exit_index >= load_index {
        return Err("局内“下一局”必须先正常退出再加载角色".to_string());
    }
    if [
        "D2RHubQuickRecreateConfirmhd.json",
        "D2RHubQuickRecreateConfirm.json",
    ]
    .iter()
    .any(|name| layout_directory.join(name).exists())
    {
        return Err("局内“下一局”仍包含旧版二级确认布局".to_string());
    }
    if room_recipe_version >= 22 {
        for (name, native_message) in [
            ("D2RHubCommitCreateGamehd.json", "CreateGame:CreateGame"),
            ("D2RHubCommitJoinGamehd.json", "JoinGame:JoinGame"),
        ] {
            if room_recipe_version >= 33 && native_message == "JoinGame:JoinGame" {
                continue;
            }
            // r32 restores same-deadline exit, submit and close in child order.
            // Keep r30/r31 contracts when inspecting upgrade sources.
            let (commit_delay, close_delay) = if room_recipe_version >= 32 {
                (0.05, Some(0.05))
            } else if room_recipe_version >= 31 {
                (0.06, None)
            } else if room_recipe_version >= 30 {
                if native_message == "CreateGame:CreateGame" {
                    (0.10, Some(0.15))
                } else {
                    (0.55, Some(0.60))
                }
            } else {
                (commit_delay, Some(close_delay))
            };
            let commit = read_room_tool_layout(&layout_directory, name)?;
            if !layout_has_timed_child_message(
                &commit,
                "PanelManager:OpenPanel:PauseLayoutGarden",
                ROOM_TRANSITION_OPEN_PAUSE_DELAY_SECONDS,
            ) || !layout_has_timed_child_message(
                &commit,
                "PausePanelMessage:ExitGame",
                ROOM_TRANSITION_EXIT_DELAY_SECONDS,
            ) || !layout_has_timed_child_message(&commit, native_message, commit_delay)
            {
                return Err(format!("局内房间提交控制器无效：{name}"));
            }
            let close_message = format!(
                "PanelManager:ClosePanel:{}",
                name.trim_end_matches("hd.json")
            );
            if let Some(close_delay) = close_delay {
                if requires_input_safety
                    && !layout_has_timed_child_message(&commit, &close_message, close_delay)
                {
                    return Err(format!("局内房间提交控制器未延后关闭，请重新加工：{name}"));
                }
            } else if layout_field_value_count(&commit, &close_message) != 0 {
                return Err(format!(
                    "局内房间提交控制器仍包含主动关闭动作，请重新加工：{name}"
                ));
            }
            let messages = commit
                .get("children")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("局内房间提交控制器消息链无效：{name}"))?;
            let exit_index = messages
                .iter()
                .position(|child| {
                    child
                        .pointer("/fields/message")
                        .and_then(serde_json::Value::as_str)
                        == Some("PausePanelMessage:ExitGame")
                })
                .ok_or_else(|| format!("局内房间提交控制器缺少正常退出消息：{name}"))?;
            let submit_index = messages
                .iter()
                .position(|child| {
                    child
                        .pointer("/fields/message")
                        .and_then(serde_json::Value::as_str)
                        == Some(native_message)
                })
                .ok_or_else(|| format!("局内房间提交控制器缺少原生提交消息：{name}"))?;
            if exit_index >= submit_index {
                return Err(format!("局内房间提交控制器必须先退出再提交：{name}"));
            }
            if room_recipe_version >= 32 {
                let close_index = messages.iter().position(|child| {
                    child
                        .pointer("/fields/message")
                        .and_then(serde_json::Value::as_str)
                        == Some(close_message.as_str())
                });
                if close_index.is_none_or(|index| submit_index >= index) {
                    return Err(format!("局内房间提交控制器必须在提交之后关闭：{name}"));
                }
            }
        }
    }
    for (name, opener_name, native_target, opposite_target) in [
        (
            "D2RHubOpenCreateGamehd.json",
            "D2RHubOpenCreateGame",
            create_form_panel,
            join_form_panel,
        ),
        (
            "D2RHubOpenJoinGamehd.json",
            "D2RHubOpenJoinGame",
            join_form_panel,
            create_form_panel,
        ),
    ] {
        let opener = read_room_tool_layout(&layout_directory, name)?;
        if opener.get("fields").is_some()
            || !layout_has_timed_child_message(
                &opener,
                &format!("PanelManager:TogglePanel:{native_target}"),
                0.1,
            )
            || !layout_has_timed_child_message(
                &opener,
                &format!("PanelManager:ClosePanel:{opposite_target}"),
                0.1,
            )
            || !layout_has_timed_child_message(
                &opener,
                &format!("PanelManager:ClosePanel:{opener_name}"),
                0.1,
            )
        {
            return Err(format!("局内房间工具未按 MDK 时序打开 {native_target}"));
        }
    }

    for pause_name in ["pauselayouthd.json", "pauselayoutgardenhd.json"] {
        let pause = read_room_tool_layout(&layout_directory, pause_name)?;
        if room_recipe_version == 27
            && !layout_has_timed_child_message(
                &pause,
                "PanelManager:OpenPanel:D2RHubQuickRecreateEscArm",
                0.01,
            )
        {
            return Err(format!("暂停布局缺少双击 Esc 入口：{pause_name}"));
        }
        if requires_input_safety {
            if find_layout_node(&pause, "ReturnToGame")
                .and_then(|node| node.get("type"))
                .and_then(serde_json::Value::as_str)
                != Some("ButtonWidget")
            {
                return Err(format!("暂停布局缺少返回游戏按钮：{pause_name}"));
            }
            validate_pause_esc_bindings(&pause, pause_name, room_recipe_version)?;
        }
        let safe_hub = find_layout_node(&pause, ROOM_TOOL_GATEWAY_HUB)
            .ok_or_else(|| format!("暂停布局缺少安全键盘焦点：{pause_name}"))?;
        if pause
            .pointer("/fields/defaultWidget")
            .and_then(serde_json::Value::as_str)
            != Some(ROOM_TOOL_GATEWAY_HUB)
            || safe_hub
                .pointer("/fields/acceptsReturnKey")
                .and_then(serde_json::Value::as_bool)
                != Some(false)
            || safe_hub
                .pointer("/fields/navigation/left/name")
                .and_then(serde_json::Value::as_str)
                != Some(ROOM_TOOL_CREATE_GATEWAY)
            || safe_hub
                .pointer("/fields/navigation/right/name")
                .and_then(serde_json::Value::as_str)
                != Some(ROOM_TOOL_JOIN_GATEWAY)
        {
            return Err(format!("暂停布局安全键盘焦点无效：{pause_name}"));
        }
        if find_layout_node(&pause, "ReturnToGame").is_none() {
            return Err(format!("暂停布局缺少 ReturnToGame：{pause_name}"));
        }
        if validate_routed_pause_buttons(&pause, pause_name)? == 0 {
            return Err(format!("暂停布局没有可验证的真实按钮：{pause_name}"));
        }
        for (gateway, action, select_direction, back_direction) in [
            (
                ROOM_TOOL_CREATE_GATEWAY,
                "PanelManager:OpenPanel:D2RHubKeyboardOpenCreate",
                "left",
                "right",
            ),
            (
                ROOM_TOOL_JOIN_GATEWAY,
                "PanelManager:OpenPanel:D2RHubKeyboardOpenJoin",
                "right",
                "left",
            ),
        ] {
            let node = find_layout_node(&pause, gateway)
                .ok_or_else(|| format!("暂停布局缺少键盘入口 {gateway}：{pause_name}"))?;
            if node
                .pointer("/fields/onClickMessage")
                .and_then(serde_json::Value::as_str)
                != Some(action)
                || node
                    .pointer("/fields/acceptsReturnKey")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                || node
                    .pointer(&format!("/fields/navigation/{select_direction}/name"))
                    .and_then(serde_json::Value::as_str)
                    != Some(gateway)
                || node
                    .pointer(&format!("/fields/navigation/{back_direction}/name"))
                    .and_then(serde_json::Value::as_str)
                    != Some(ROOM_TOOL_GATEWAY_HUB)
            {
                return Err(format!("暂停布局键盘入口无效 {gateway}：{pause_name}"));
            }
        }
    }
    for (helper_name, helper_panel, native_target, opposite_target) in [
        (
            "D2RHubKeyboardOpenCreatehd.json",
            "D2RHubKeyboardOpenCreate",
            create_form_panel,
            join_form_panel,
        ),
        (
            "D2RHubKeyboardOpenJoinhd.json",
            "D2RHubKeyboardOpenJoin",
            join_form_panel,
            create_form_panel,
        ),
    ] {
        let helper = read_room_tool_layout(&layout_directory, helper_name)?;
        if !layout_has_timed_child_message(&helper, "PausePanelMessage:Close", 0.005)
            || !layout_has_timed_child_message(
                &helper,
                &format!("PanelManager:TogglePanel:{native_target}"),
                0.1,
            )
            || !layout_has_timed_child_message(
                &helper,
                &format!("PanelManager:ClosePanel:{opposite_target}"),
                0.1,
            )
            || !layout_has_timed_child_message(
                &helper,
                &format!("PanelManager:ClosePanel:{helper_panel}"),
                0.1,
            )
        {
            return Err(format!("暂停菜单房间入口未复用 MDK 时序：{helper_name}"));
        }
    }

    let mut form_specs: Vec<(&str, &str, &[&str], &str, &str)> = vec![
        (
            "creategamepanelhd.json",
            "GameNameInput",
            &["GameNameInput", "PasswordInput", "DescriptionInput"],
            "CreateGame:CreateGame",
            "PanelManager:OpenPanel:D2RHubCommitCreateGame",
        ),
        (
            "joingamepanelhd.json",
            "NameInput",
            &["NameInput", "PasswordInput"],
            "JoinGame:JoinGame",
            "PanelManager:OpenPanel:D2RHubCommitJoinGame",
        ),
    ];
    if separate_in_game_forms {
        form_specs.extend([
            (
                "D2RHubInGameCreateGamehd.json",
                "GameNameInput",
                &["GameNameInput", "PasswordInput", "DescriptionInput"][..],
                "CreateGame:CreateGame",
                "PanelManager:OpenPanel:D2RHubCommitCreateGame",
            ),
            (
                "D2RHubInGameJoinGamehd.json",
                "NameInput",
                &["NameInput", "PasswordInput"][..],
                "JoinGame:JoinGame",
                "PanelManager:OpenPanel:D2RHubCommitJoinGame",
            ),
        ]);
    }
    for (name, primary_input, input_names, native_submit, routed_submit) in form_specs {
        let form = read_room_tool_layout(&layout_directory, name)?;
        if room_recipe_version >= 29 {
            let timers: Vec<_> = form
                .get("children")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter(|child| {
                    child.get("name").and_then(serde_json::Value::as_str)
                        == Some("D2RHubDefaultHell")
                })
                .collect();
            if name == "D2RHubInGameCreateGamehd.json" && default_hell {
                let expected = serde_json::json!({
                    "type": "TimerWidget",
                    "name": "D2RHubDefaultHell",
                    "fields": {"time": 0.05, "message": "CreateGame:SetDifficulty:2"}
                });
                if timers.len() != 1 || timers[0] != &expected {
                    return Err(format!(
                        "局内创建表单缺少有效的默认地狱初始化，请重新加工：{name}"
                    ));
                }
            } else if !timers.is_empty() {
                return Err(format!("默认地狱初始化已停用或不适用于此表单：{name}"));
            }
        }
        let is_lobby_form = separate_in_game_forms && !name.starts_with("D2RHubInGame");
        let native_panel = if primary_input == "NameInput" {
            "JoinGamePanel"
        } else {
            "CreateGamePanel"
        };
        let expected_panel = if separate_in_game_forms && !is_lobby_form {
            name.trim_end_matches("hd.json")
        } else {
            native_panel
        };
        if separate_in_game_forms
            && (form.get("name").and_then(serde_json::Value::as_str) != Some(expected_panel)
                || form.get("type").and_then(serde_json::Value::as_str) != Some(native_panel))
        {
            return Err(format!("大厅与局内房间表单身份无效：{name}"));
        }
        if form
            .pointer("/fields/defaultWidget")
            .and_then(serde_json::Value::as_str)
            != Some(primary_input)
            || form
                .pointer("/fields/isDismissable")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
            || form
                .pointer("/fields/acceptsEscKeyEverywhere")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
        {
            return Err(format!("局内房间表单未正确加工：{name}"));
        }
        if input_names.iter().any(|input_name| {
            let Some(node) = find_layout_node(&form, input_name) else {
                return true;
            };
            node.pointer("/fields/imeEnabled")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
                || node.pointer("/fields/alwaysAcceptsKeyInput").is_some()
        }) {
            return Err(format!("局内房间表单无法完整捕获键盘输入：{name}"));
        }
        if is_lobby_form || (room_recipe_version >= 33 && native_submit == "JoinGame:JoinGame") {
            if layout_field_value_count(&form, native_submit) == 0
                || layout_field_value_count(&form, routed_submit) != 0
            {
                return Err(format!("大厅表单仍包含局内退出提交入口：{name}"));
            }
        } else if room_recipe_version >= 22
            && (layout_field_value_count(&form, native_submit) != 0
                || layout_field_value_count(&form, routed_submit) == 0)
        {
            return Err(format!("局内房间表单没有完整接入主动退出提交链：{name}"));
        }
        let close_action = format!("PanelManager:ClosePanel:{expected_panel}");
        if find_layout_node(&form, "D2RHubCloseRoomForm")
            .and_then(|node| node.pointer("/fields/onClickMessage"))
            .and_then(serde_json::Value::as_str)
            != Some(close_action.as_str())
        {
            return Err(format!("局内房间表单缺少关闭按钮：{name}"));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(in crate::audio_mod) fn validate_in_game_room_tool_layouts(
    mod_directory: &Path,
    mod_name: &str,
) -> Result<(), String> {
    validate_in_game_room_tool_layouts_for_recipe(mod_directory, mod_name, true)
}

#[cfg(test)]
mod startup_prefix_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn independent_modules_share_startup_prefix_but_delayed_or_closed_launchers_fail() {
        let scene = "PanelManager:OpenPanel:D2RHubSceneEntry";
        let party = "PanelManager:OpenPanel:JcyPartyExtra";
        let timer = |message: &str, time: f64| json!({"type":"TimerWidget","fields":{"message":message,"time":time}});
        for messages in [[scene, party], [party, scene]] {
            let hud = json!({"children":[timer(messages[0],-1.0),timer(messages[1],-1.0),timer("PanelManager:ClosePanel:HUDWarnings",0.008)]});
            assert!(has_immediate_launcher(&hud, scene));
            assert!(has_immediate_launcher(&hud, party));
        }
        assert!(!has_immediate_launcher(
            &json!({"children":[timer(scene,0.01)]}),
            scene
        ));
        assert!(!has_immediate_launcher(
            &json!({"children":[timer("PanelManager:ClosePanel:HUDWarnings",-1.0),timer(scene,-1.0)]}),
            scene
        ));
    }
}
