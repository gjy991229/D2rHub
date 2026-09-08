//! Read-only geometry for the installed room-tool recipe, never screen guesses.
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

pub(super) struct RoomLayout {
    toolbar: Value,
    form: Value,
    profile: Value,
    create: bool,
}

#[derive(Clone, Copy)]
struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
}

impl RoomLayout {
    pub(super) fn load(pid: u32, create: bool) -> Result<Self, String> {
        let directory = process_layout_directory(pid)?;
        let toolbar = read(&directory.join("D2RHubRoomToolbarhd.json"))?;
        let hud = read(&directory.join("HudWarningshd.json"))?;
        if !has_value(&hud, "PanelManager:OpenPanel:D2RHubRoomToolbar")
            || has_value(&hud, "PanelManager:ClosePanel:D2RHubRoomToolbar")
        {
            return Err(
                "此 Mod 的局内房间工具栏已隐藏，请在 Mod 管理中显示工具栏并重启游戏".to_string(),
            );
        }
        let (dedicated, legacy) = if create {
            ("D2RHubInGameCreateGamehd.json", "creategamepanelhd.json")
        } else {
            ("D2RHubInGameJoinGamehd.json", "joingamepanelhd.json")
        };
        // Older processed recipes use the shared form. Follow their opener,
        // rather than assuming the newest recipe or requiring regeneration.
        let opener = read(&directory.join(if create {
            "D2RHubOpenCreateGamehd.json"
        } else {
            "D2RHubOpenJoinGamehd.json"
        }))?;
        let dedicated_panel = if create {
            "D2RHubInGameCreateGame"
        } else {
            "D2RHubInGameJoinGame"
        };
        let legacy_panel = if create {
            "CreateGamePanel"
        } else {
            "JoinGamePanel"
        };
        let opens = |panel| {
            has_value(&opener, &format!("PanelManager:OpenPanel:{panel}"))
                || has_value(&opener, &format!("PanelManager:TogglePanel:{panel}"))
        };
        let form_file = if opens(dedicated_panel) {
            dedicated
        } else if opens(legacy_panel) {
            legacy
        } else {
            return Err("无法识别 Mod 的局内房间表单入口".to_string());
        };
        let form = read(&directory.join(form_file))?;
        let mut profile = json!({
            "LobbyAnchor": {"x": 0.5},
            "RightLobbySubPanelRect": {"x": 452, "y": 210}
        });
        let profile_path = directory.join("_profilehd.json");
        if profile_path.exists() {
            let overrides = read(&profile_path)?;
            let overrides = overrides.as_object().ok_or("Mod 高清布局配置不是对象")?;
            profile.as_object_mut().unwrap().extend(overrides.clone());
        }
        Ok(Self {
            toolbar,
            form,
            profile,
            create,
        })
    }

    pub(super) fn points(&self, width: i32, height: i32) -> Result<[(i32, i32); 4], String> {
        // Narrow/LV/controller and ultrawide letterbox profiles have different
        // transforms. Fail closed instead of clicking a different control.
        if height < 360 || (f64::from(width) / f64::from(height) - 16.0 / 9.0).abs() > 0.025 {
            return Err("键鼠接管需要 16:9 高清键鼠界面，请调整游戏窗口比例".to_string());
        }
        let root = Frame {
            x: 0.0,
            y: 0.0,
            width: f64::from(width),
            height: f64::from(height),
            scale: f64::from(height) / 2160.0,
        };
        let button = if self.create {
            "D2RHubCreateGame"
        } else {
            "D2RHubJoinGame"
        };
        let name = if self.create {
            "GameNameInput"
        } else {
            "NameInput"
        };
        let point = |document, name| -> Result<(i32, i32), String> {
            let (x, y) = locate(document, name, root, &self.profile)?
                .ok_or_else(|| format!("Mod 布局缺少控件：{name}"))?;
            if x < 0.0 || y < 0.0 || x >= root.width || y >= root.height {
                return Err(format!("Mod 控件 {name} 超出客户区，已停止点击"));
            }
            Ok((x.round() as i32, y.round() as i32))
        };
        let room_name = point(&self.form, name)?;
        Ok([
            point(&self.toolbar, button)?,
            room_name,
            point(&self.form, "PasswordInput")?,
            if self.create {
                point(&self.form, "DifficultyHell")?
            } else {
                room_name
            },
        ])
    }
}

fn resolve<'a>(value: &'a Value, profile: &'a Value) -> Result<&'a Value, String> {
    let mut current = value;
    for _ in 0..16 {
        match current.as_str().and_then(|s| s.strip_prefix('$')) {
            Some(key) => {
                current = profile
                    .get(key)
                    .ok_or_else(|| format!("无法解析布局变量：{key}"))?
            }
            None => return Ok(current),
        }
    }
    Err("布局变量循环引用".to_string())
}

fn number(value: &Value, key: &str, default: f64, profile: &Value) -> Result<f64, String> {
    match value.get(key) {
        None => Ok(default),
        Some(value) => resolve(value, profile)?
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("无法解析控件几何属性：{key}")),
    }
}

fn locate(
    node: &Value,
    name: &str,
    parent: Frame,
    profile: &Value,
) -> Result<Option<(f64, f64)>, String> {
    // Only traverse ancestors of the requested control. Unrelated artwork can
    // use profile variables that are irrelevant to these hit targets.
    if !contains_node(node, name) {
        return Ok(None);
    }
    let rect = resolve(&node["fields"]["rect"], profile)?;
    let anchor = resolve(&node["fields"]["anchor"], profile)?;
    let scale = parent.scale * number(rect, "scale", 1.0, profile)?;
    if scale <= 0.0 {
        return Err("控件缩放无效".to_string());
    }
    let frame = Frame {
        x: parent.x
            + number(anchor, "x", 0.0, profile)? * parent.width
            + number(rect, "x", 0.0, profile)? * parent.scale,
        y: parent.y
            + number(anchor, "y", 0.0, profile)? * parent.height
            + number(rect, "y", 0.0, profile)? * parent.scale,
        width: number(rect, "width", 0.0, profile)? * scale,
        height: number(rect, "height", 0.0, profile)? * scale,
        scale,
    };
    if node["name"].as_str() == Some(name) {
        let (dx, dy) = if frame.width > 0.0 && frame.height > 0.0 {
            (frame.width / 2.0, frame.height / 2.0)
        } else {
            // These native sprites omit rect dimensions. Use an interior hit
            // point only for the exact artwork used by the processed recipe.
            match node["fields"]["filename"].as_str() {
                Some("FrontEnd\\HD\\Final\\FrontEnd_ButtonLarge") => (400.0 * scale, 80.0 * scale),
                Some("Lobby\\CreateGame\\CreateGame_DifficultyBTN") => {
                    (100.0 * scale, 24.0 * scale)
                }
                _ => return Err(format!("控件 {name} 没有可识别的点击区域")),
            }
        };
        return Ok(Some((frame.x + dx, frame.y + dy)));
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            if let Some(point) = locate(child, name, frame, profile)? {
                return Ok(Some(point));
            }
        }
    }
    Ok(None)
}

fn contains_node(node: &Value, name: &str) -> bool {
    node["name"].as_str() == Some(name)
        || node["children"]
            .as_array()
            .is_some_and(|children| children.iter().any(|child| contains_node(child, name)))
}

fn has_value(node: &Value, expected: &str) -> bool {
    match node {
        Value::String(s) => s == expected,
        Value::Array(a) => a.iter().any(|v| has_value(v, expected)),
        Value::Object(o) => o.values().any(|v| has_value(v, expected)),
        _ => false,
    }
}

fn process_layout_directory(pid: u32) -> Result<PathBuf, String> {
    let mut system = crate::infrastructure::process::shared_system()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let pid = Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        ProcessRefreshKind::new()
            .with_cmd(UpdateKind::Always)
            .with_exe(UpdateKind::Always),
    );
    let process = system.process(pid).ok_or("目标游戏进程已退出")?;
    let args = process.cmd();
    let names = args
        .windows(2)
        .filter(|pair| pair[0].to_string_lossy().eq_ignore_ascii_case("-mod"))
        .map(|pair| pair[1].to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if names.len() != 1
        || !args
            .iter()
            .any(|arg| arg.to_string_lossy().eq_ignore_ascii_case("-txt"))
    {
        return Err("无法从目标进程确认 -mod / -txt，请通过 D2RHub 启动加工 Mod".to_string());
    }
    let name = &names[0];
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || name.contains(':')
    {
        return Err("目标进程 Mod 名称无效".to_string());
    }
    let game = process
        .exe()
        .and_then(Path::parent)
        .ok_or("无法读取目标进程游戏目录")?;
    Ok(game
        .join("mods")
        .join(name)
        .join(format!("{name}.mpq"))
        .join("data/global/ui/layouts"))
}

fn read(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|_| {
        format!(
            "缺少房间工具布局：{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })?;
    // Blizzard profiles allow comments and trailing commas. Preserve quoted
    // text (including escaped quotes and // in strings) byte-for-byte.
    let mut clean = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF])
        .unwrap_or(&bytes)
        .to_vec();
    let (mut i, mut quoted) = (0, false);
    while i < clean.len() {
        if quoted {
            if clean[i] == b'\\' {
                i += 2;
                continue;
            }
            if clean[i] == b'"' {
                quoted = false;
            }
        } else if clean[i] == b'"' {
            quoted = true;
        } else if clean.get(i..i + 2) == Some(b"//") {
            while i < clean.len() && clean[i] != b'\n' {
                clean[i] = b' ';
                i += 1;
            }
            continue;
        } else if clean.get(i..i + 2) == Some(b"/*") {
            clean[i..i + 2].fill(b' ');
            i += 2;
            while i + 1 < clean.len() && &clean[i..i + 2] != b"*/" {
                clean[i] = b' ';
                i += 1;
            }
            if i + 1 >= clean.len() {
                return Err("布局注释未闭合".to_string());
            }
            clean[i..i + 2].fill(b' ');
            i += 2;
            continue;
        }
        i += 1;
    }
    i = 0;
    quoted = false;
    while i < clean.len() {
        if quoted {
            if clean[i] == b'\\' {
                i += 2;
                continue;
            }
            if clean[i] == b'"' {
                quoted = false;
            }
        } else if clean[i] == b'"' {
            quoted = true;
        } else if clean[i] == b','
            && clean[i + 1..]
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| matches!(b, b'}' | b']'))
        {
            clean[i] = b' ';
        }
        i += 1;
    }
    serde_json::from_slice(&clean).map_err(|_| {
        format!(
            "无法解析布局：{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}
