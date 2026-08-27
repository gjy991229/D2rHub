use crate::commands::account::{update_account_mods_inner, AccountManager, AccountMeta};
use crate::commands::global_config::GlobalConfig;
use crate::commands::launch::parse_windows_command_line;
use crate::launch_context::{ContextPurpose, LaunchContext};
use crate::rune_audio::catalog::AREA_CATALOG_FILE_NAME;
use crate::rune_audio::item_catalog::ITEM_CATALOG_FILE_NAME;
use crate::rune_audio::protocol::PROTOCOL_VERSION;
use crate::state::SharedState;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::Emitter;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

const MANIFEST_FILE_NAME: &str = "audio-telemetry-manifest.json";
const MANIFEST_FORMAT: &str = "d2r-audio-telemetry-mod";
const PRODUCER_NAME: &str = "d2r-audio-mod";

#[derive(Debug, Clone, Serialize)]
pub struct InstalledMod {
    pub name: String,
    pub audio_ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModSetupState {
    pub account_id: String,
    pub account_name: String,
    pub current_mod_name: Option<String>,
    pub launch_arguments: String,
    pub has_txt: bool,
    pub ready: bool,
    pub reason_code: String,
    pub message: String,
    pub installed_mods: Vec<InstalledMod>,
    pub running_pid: Option<u32>,
    pub session_verified: bool,
    pub active_session_ready: Option<bool>,
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModPrepareProgress {
    pub account_id: String,
    pub phase: String,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModPrepareResult {
    pub account_id: String,
    pub mod_name: String,
    pub mod_directory: String,
    pub launch_arguments: String,
    pub source_mod_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioModRuntimeWarning {
    pub account_id: String,
    pub account_name: String,
    pub target_pid: u32,
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct GeneratorReport {
    protocol_version: u8,
    mod_name: String,
    mod_directory: String,
}

#[derive(Debug)]
struct Compatibility {
    mod_name: Option<String>,
    has_txt: bool,
    ready: bool,
    reason_code: String,
    message: String,
}

struct BuildLease(SharedState);

impl BuildLease {
    fn acquire(state: &SharedState) -> Result<Self, String> {
        state
            .audio_mod_build_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "已有一个识别 Mod 正在准备，请稍候".to_string())?;
        Ok(Self(state.clone()))
    }
}

impl Drop for BuildLease {
    fn drop(&mut self) {
        self.0.audio_mod_build_busy.store(false, Ordering::SeqCst);
    }
}

fn configured_account(
    state: &SharedState,
    account_id: &str,
) -> Result<(GlobalConfig, AccountMeta, LaunchContext), String> {
    let config = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| "尚未完成首次配置".to_string())?;
    let account = AccountManager::load_meta(&config.accounts_dir, account_id)
        .map_err(|error| error.to_string())?;
    if !account.initialized {
        return Err("请先初始化该账号".to_string());
    }
    let context = LaunchContext::for_account(&config, &account, ContextPurpose::LaunchGame)
        .map_err(|error| error.to_string())?;
    Ok((config, account, context))
}

fn plain_mod_name(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Mod 名称不能为空".to_string());
    }
    let path = Path::new(value);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err("Mod 名称不能包含目录路径".to_string());
    }
    Ok(value)
}

fn generated_audio_mod_name(value: &str) -> Result<&str, String> {
    let value = plain_mod_name(value)?;
    if value.len() > 128 {
        return Err("Mod 名称不能超过 128 个字符".to_string());
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Mod 名称仅可使用英文字母、数字、短横线和下划线".to_string());
    }
    let uppercase = value.to_ascii_uppercase();
    if matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || uppercase
            .strip_prefix("COM")
            .or_else(|| uppercase.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
    {
        return Err("该名称是 Windows 保留名称，请换一个".to_string());
    }
    Ok(value)
}

fn active_mod_name(mod_args: &str) -> Result<Option<String>, String> {
    let args = parse_windows_command_line(mod_args)
        .map_err(|error| format!("无法解析账号启动参数: {error}"))?;
    let mut index = 0usize;
    while index < args.len() {
        let argument = &args[index];
        if argument.eq_ignore_ascii_case("-mod") {
            let value = args
                .get(index + 1)
                .ok_or_else(|| "-mod 后缺少 Mod 名称".to_string())?;
            return Ok(Some(plain_mod_name(value)?.to_string()));
        }
        if argument
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("-mod="))
        {
            return Ok(Some(plain_mod_name(&argument[5..])?.to_string()));
        }
        index += 1;
    }
    Ok(None)
}

fn has_txt_argument(mod_args: &str) -> Result<bool, String> {
    Ok(parse_windows_command_line(mod_args)
        .map_err(|error| format!("无法解析账号启动参数: {error}"))?
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("-txt")))
}

fn read_protocol_version(path: &Path) -> Result<u8, String> {
    let document: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|error| format!("读取 {} 失败: {error}", path.display()))?,
    )
    .map_err(|error| format!("解析 {} 失败: {error}", path.display()))?;
    document
        .get("protocol_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| format!("{} 缺少协议版本", path.display()))
}

fn validate_audio_mod(mods_directory: &Path, mod_name: &str) -> Result<PathBuf, String> {
    let mod_name = plain_mod_name(mod_name)?;
    let mod_directory = mods_directory.join(mod_name);
    if !mod_directory.is_dir() {
        return Err(format!("未找到 Mod：{mod_name}"));
    }
    if !mod_directory.join(format!("{mod_name}.mpq")).is_dir() {
        return Err("Mod 目录结构不完整".to_string());
    }

    let manifest_path = mod_directory.join(MANIFEST_FILE_NAME);
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|_| "这个 Mod 未经过 D2RHub 声纹加工".to_string())?,
    )
    .map_err(|_| "识别 Mod 清单已损坏，请重新准备".to_string())?;
    let protocol = manifest
        .get("protocol_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "识别 Mod 清单缺少协议版本".to_string())?;
    if protocol != u64::from(PROTOCOL_VERSION) {
        return Err(format!(
            "识别 Mod 协议版本不匹配（需要 v{PROTOCOL_VERSION}）"
        ));
    }
    if let Some(format) = manifest
        .get("manifest_format")
        .and_then(serde_json::Value::as_str)
    {
        if format != MANIFEST_FORMAT {
            return Err("识别 Mod 清单类型不受支持".to_string());
        }
    }
    if let Some(producer) = manifest.get("producer").and_then(serde_json::Value::as_str) {
        if producer != PRODUCER_NAME {
            return Err("识别 Mod 生成器不受支持".to_string());
        }
    }
    if let Some(recorded_name) = manifest.get("mod_name").and_then(serde_json::Value::as_str) {
        if recorded_name != mod_name {
            return Err("识别 Mod 已被改名，请重新准备".to_string());
        }
    }

    for catalog in [AREA_CATALOG_FILE_NAME, ITEM_CATALOG_FILE_NAME] {
        let version = read_protocol_version(&mod_directory.join(catalog))?;
        if version != PROTOCOL_VERSION {
            return Err(format!("{catalog} 协议版本不匹配"));
        }
    }
    Ok(mod_directory)
}

fn compatibility(mods_directory: &Path, launch_arguments: &str) -> Compatibility {
    let has_txt = has_txt_argument(launch_arguments).unwrap_or(false);
    let mod_name = match active_mod_name(launch_arguments) {
        Ok(value) => value,
        Err(error) => {
            return Compatibility {
                mod_name: None,
                has_txt,
                ready: false,
                reason_code: "invalid_arguments".to_string(),
                message: error,
            }
        }
    };
    let Some(name) = mod_name.clone() else {
        return Compatibility {
            mod_name,
            has_txt,
            ready: false,
            reason_code: "missing_mod".to_string(),
            message: "当前账号还没有使用识别 Mod".to_string(),
        };
    };
    if !has_txt {
        return Compatibility {
            mod_name,
            has_txt,
            ready: false,
            reason_code: "missing_txt".to_string(),
            message: "启动参数缺少 -txt，声纹资源不会生效".to_string(),
        };
    }
    match validate_audio_mod(mods_directory, &name) {
        Ok(_) => Compatibility {
            mod_name,
            has_txt,
            ready: true,
            reason_code: "ready".to_string(),
            message: "识别 Mod 已准备好".to_string(),
        },
        Err(error) => Compatibility {
            mod_name,
            has_txt,
            ready: false,
            reason_code: "unsupported_mod".to_string(),
            message: error,
        },
    }
}

fn installed_mods(mods_directory: &Path) -> Vec<InstalledMod> {
    let mut mods = std::fs::read_dir(mods_directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !entry.path().join(format!("{name}.mpq")).is_dir() {
                return None;
            }
            let audio_ready = validate_audio_mod(mods_directory, &name).is_ok();
            Some(InstalledMod { name, audio_ready })
        })
        .collect::<Vec<_>>();
    mods.sort_by_key(|entry| entry.name.to_lowercase());
    mods
}

fn session_arguments(state: &SharedState, account: &AccountMeta) -> (String, Option<u32>, bool) {
    let running_pid = state
        .active_games
        .read()
        .get(&account.id)
        .copied()
        .or(account.running_pid);
    if let Some(pid) = running_pid {
        if let Some(snapshot) = state.active_game_launches.read().get(&account.id) {
            if snapshot.pid == pid {
                return (snapshot.mod_args.clone(), Some(pid), true);
            }
        }
        return (account.mod_args.clone(), Some(pid), false);
    }
    (account.mod_args.clone(), None, false)
}

fn setup_state(state: &SharedState, account_id: &str) -> Result<AudioModSetupState, String> {
    let (_config, account, context) = configured_account(state, account_id)?;
    let mods_directory = context.installation.game_directory.join("mods");
    let configured = compatibility(&mods_directory, &account.mod_args);
    let (session_arguments, running_pid, session_verified) = session_arguments(state, &account);
    let active_session_ready = running_pid.and_then(|_| {
        session_verified.then(|| compatibility(&mods_directory, &session_arguments).ready)
    });
    Ok(AudioModSetupState {
        account_id: account.id.clone(),
        account_name: if account.display_name.trim().is_empty() {
            account.id.clone()
        } else {
            account.display_name.clone()
        },
        current_mod_name: configured.mod_name,
        launch_arguments: account.mod_args,
        has_txt: configured.has_txt,
        ready: configured.ready,
        reason_code: configured.reason_code,
        message: configured.message,
        installed_mods: installed_mods(&mods_directory),
        running_pid,
        session_verified,
        active_session_ready,
        restart_required: running_pid.is_some() && active_session_ready != Some(true),
    })
}

#[tauri::command]
pub fn get_audio_mod_setup_state(
    state: tauri::State<'_, SharedState>,
    account_id: String,
) -> Result<AudioModSetupState, String> {
    setup_state(state.inner(), &account_id)
}

fn emit_prepare_progress(
    app: &tauri::AppHandle,
    account_id: &str,
    phase: &str,
    percent: u8,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "audio-mod-prepare-progress",
        AudioModPrepareProgress {
            account_id: account_id.to_string(),
            phase: phase.to_string(),
            percent: percent.min(100),
            message: message.into(),
        },
    );
}

#[tauri::command]
pub async fn prepare_audio_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_id: String,
    mod_name: String,
    source_mod_name: Option<String>,
) -> Result<AudioModPrepareResult, String> {
    let shared_state = state.inner().clone();
    let _lease = BuildLease::acquire(&shared_state)?;
    let (_config, _account, context) = configured_account(&shared_state, &account_id)?;
    let game_directory = context.installation.game_directory;
    let mods_directory = game_directory.join("mods");
    std::fs::create_dir_all(&mods_directory)
        .map_err(|error| format!("创建 mods 目录失败: {error}"))?;

    let mod_name = generated_audio_mod_name(&mod_name)?.to_string();
    if mods_directory.join(&mod_name).exists() {
        return Err(format!("Mod 名称“{mod_name}”已存在，请换一个名称"));
    }

    let source_mod_name = source_mod_name
        .map(|name| plain_mod_name(&name).map(str::to_string))
        .transpose()?;
    if source_mod_name
        .as_deref()
        .is_some_and(|source| source.eq_ignore_ascii_case(&mod_name))
    {
        return Err("新 Mod 名称不能与源 Mod 相同".to_string());
    }
    let source_directory = source_mod_name
        .as_deref()
        .map(|name| mods_directory.join(name));
    if let Some(source) = source_directory.as_ref() {
        if !source.is_dir() {
            return Err(format!("未找到源 Mod：{}", source.display()));
        }
        if validate_audio_mod(
            &mods_directory,
            source_mod_name.as_deref().unwrap_or_default(),
        )
        .is_ok()
        {
            return Err("请选择原始 Mod，不要再次加工已经生成的识别 Mod".to_string());
        }
    }

    emit_prepare_progress(&app, &account_id, "starting", 1, "正在开始准备…");
    let command_name = if source_directory.is_some() {
        "augment"
    } else {
        "minimal"
    };
    let mut arguments = vec![
        command_name.to_string(),
        "--game".to_string(),
        game_directory.to_string_lossy().into_owned(),
        "--output".to_string(),
        mods_directory.to_string_lossy().into_owned(),
        "--name".to_string(),
        mod_name.clone(),
        "--areas".to_string(),
        "all".to_string(),
        "--track".to_string(),
        "all".to_string(),
        "--events".to_string(),
    ];
    if let Some(source) = source_directory.as_ref() {
        arguments.push("--source".to_string());
        arguments.push(source.to_string_lossy().into_owned());
    }

    let (mut receiver, _child) = app
        .shell()
        .sidecar("d2r-audio-mod")
        .map_err(|error| format!("识别 Mod 生成器不可用: {error}"))?
        .args(arguments)
        .spawn()
        .map_err(|error| format!("无法启动识别 Mod 生成器: {error}"))?;

    let mut report: Option<GeneratorReport> = None;
    let mut stderr = String::new();
    let mut exit_code = None;
    while let Some(event) = receiver.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
                    continue;
                };
                match value.get("type").and_then(serde_json::Value::as_str) {
                    Some("progress") => {
                        emit_prepare_progress(
                            &app,
                            &account_id,
                            value
                                .get("phase")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("working"),
                            value
                                .get("percent")
                                .and_then(serde_json::Value::as_u64)
                                .and_then(|percent| u8::try_from(percent).ok())
                                .unwrap_or(0),
                            value
                                .get("message")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("正在准备…"),
                        );
                    }
                    Some("completed") => {
                        report = Some(
                            serde_json::from_value(
                                value.get("report").cloned().unwrap_or_default(),
                            )
                            .map_err(|error| format!("生成器返回了无效结果: {error}"))?,
                        );
                    }
                    Some("error") => {
                        if let Some(message) =
                            value.get("message").and_then(serde_json::Value::as_str)
                        {
                            stderr = message.to_string();
                        }
                    }
                    _ => {}
                }
            }
            CommandEvent::Stderr(bytes) => {
                if stderr.len() < 8_000 {
                    stderr.push_str(&String::from_utf8_lossy(&bytes));
                    stderr.push('\n');
                }
            }
            CommandEvent::Error(error) => stderr.push_str(&error),
            CommandEvent::Terminated(payload) => exit_code = payload.code,
            _ => {}
        }
    }

    if exit_code != Some(0) {
        return Err(if stderr.trim().is_empty() {
            format!("识别 Mod 生成失败（退出码 {:?}）", exit_code)
        } else {
            stderr.trim().to_string()
        });
    }
    let report = report.ok_or_else(|| "生成器没有返回完成结果".to_string())?;
    if report.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "生成器协议版本不匹配：收到 v{}，需要 v{PROTOCOL_VERSION}",
            report.protocol_version
        ));
    }
    if report.mod_name != mod_name {
        return Err("生成器返回的 Mod 名称与用户指定名称不一致".to_string());
    }
    let validated_directory = validate_audio_mod(&mods_directory, &report.mod_name)?;
    let reported_directory = std::fs::canonicalize(&report.mod_directory)
        .map_err(|error| format!("无法校验生成目录: {error}"))?;
    let validated_directory = std::fs::canonicalize(validated_directory)
        .map_err(|error| format!("无法校验识别 Mod: {error}"))?;
    if reported_directory != validated_directory {
        return Err("生成器返回的目录与实际输出不一致".to_string());
    }
    emit_prepare_progress(&app, &account_id, "complete", 100, "识别 Mod 已准备完成");
    Ok(AudioModPrepareResult {
        account_id,
        mod_name: report.mod_name,
        mod_directory: report.mod_directory,
        launch_arguments: arguments_with_audio_mod("", &mod_name)?,
        source_mod_name,
    })
}

fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty()
        && !argument
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return argument.to_string();
    }
    let mut output = String::from("\"");
    let mut slashes = 0usize;
    for character in argument.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        if character == '"' {
            output.push_str(&"\\".repeat(slashes * 2 + 1));
            output.push('"');
        } else {
            output.push_str(&"\\".repeat(slashes));
            output.push(character);
        }
        slashes = 0;
    }
    output.push_str(&"\\".repeat(slashes * 2));
    output.push('"');
    output
}

pub(crate) fn arguments_with_audio_mod(
    existing_arguments: &str,
    mod_name: &str,
) -> Result<String, String> {
    let mod_name = plain_mod_name(mod_name)?;
    let arguments = parse_windows_command_line(existing_arguments)
        .map_err(|error| format!("无法解析原启动参数: {error}"))?;
    let mut preserved = Vec::new();
    let mut index = 0usize;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument.eq_ignore_ascii_case("-mod") {
            index += 2;
            continue;
        }
        if argument
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("-mod="))
            || argument.eq_ignore_ascii_case("-txt")
        {
            index += 1;
            continue;
        }
        if argument.eq_ignore_ascii_case("-assettestmode") {
            index += 1;
            if arguments
                .get(index)
                .is_some_and(|value| !value.starts_with('-'))
            {
                index += 1;
            }
            continue;
        }
        if argument
            .get(..15)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("-assettestmode="))
        {
            index += 1;
            continue;
        }
        preserved.push(argument.clone());
        index += 1;
    }
    preserved.push("-mod".to_string());
    preserved.push(mod_name.to_string());
    preserved.push("-txt".to_string());
    preserved.push("-assettestmode".to_string());
    preserved.push("1".to_string());
    Ok(preserved
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" "))
}

#[tauri::command]
pub fn apply_audio_mod_to_account(
    state: tauri::State<'_, SharedState>,
    account_id: String,
    mod_name: String,
) -> Result<AudioModSetupState, String> {
    let (_config, account, context) = configured_account(state.inner(), &account_id)?;
    validate_audio_mod(&context.installation.game_directory.join("mods"), &mod_name)?;
    let next_arguments = arguments_with_audio_mod(&account.mod_args, &mod_name)?;
    let mut mod_list = account.mod_list.clone();
    if !mod_list.iter().any(|entry| entry == &next_arguments) {
        mod_list.push(next_arguments.clone());
    }
    update_account_mods_inner(state.inner(), account_id.clone(), next_arguments, mod_list)
        .map_err(|error| error.to_string())?;
    setup_state(state.inner(), &account_id)
}

pub(crate) fn validate_runtime_audio_mod(
    config: &GlobalConfig,
    account: &AccountMeta,
    launch_arguments: &str,
) -> Result<PathBuf, String> {
    let context = LaunchContext::for_account(config, account, ContextPurpose::Settings)
        .map_err(|error| error.to_string())?;
    let mods_directory = context.installation.game_directory.join("mods");
    let result = compatibility(&mods_directory, launch_arguments);
    if !result.ready {
        return Err(result.message);
    }
    validate_audio_mod(
        &mods_directory,
        result.mod_name.as_deref().unwrap_or_default(),
    )
}

pub(crate) fn emit_runtime_compatibility_warning(
    app: &tauri::AppHandle,
    state: &SharedState,
    config: &GlobalConfig,
    account: &AccountMeta,
    pid: u32,
    launch_arguments: &str,
) {
    if !config.rune_audio_enabled || config.rune_audio_target_account != account.id {
        return;
    }
    let context = match LaunchContext::for_account(config, account, ContextPurpose::Settings) {
        Ok(context) => context,
        Err(_) => return,
    };
    let result = compatibility(
        &context.installation.game_directory.join("mods"),
        launch_arguments,
    );
    if result.ready {
        return;
    }
    let account_name = if account.display_name.trim().is_empty() {
        account.id.clone()
    } else {
        account.display_name.clone()
    };
    let warning = AudioModRuntimeWarning {
        account_id: account.id.clone(),
        account_name: account_name.clone(),
        target_pid: pid,
        reason_code: result.reason_code,
        message: format!(
            "“{account_name}”当前使用的 Mod 不支持声纹识别，请检查。游戏可以继续运行，但本次识别与统计不会生效。"
        ),
    };
    let _ = app.emit("audio-mod-compatibility-warning", warning);
    state.active_game_launches.write().insert(
        account.id.clone(),
        crate::state::ActiveGameLaunch {
            pid,
            mod_args: launch_arguments.to_string(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{
        active_mod_name, arguments_with_audio_mod, generated_audio_mod_name, has_txt_argument,
    };

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
}
