use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::battle_net_config::update_mod_args;
use crate::commands::account::{
    copy_account_settings_to_system, recover_interrupted_replacement, remove_path_if_exists,
    replace_path_with_backup, resolve_account_runtime_snapshot, sibling_with_suffix,
    AccountManager, RegistrySnapshotPath,
};
use crate::commands::system::LaunchProgress;
use crate::commands::utils::silent_cmd;
use crate::error::AppError;
use crate::launch_context::{
    account_game_executable_identity, AuthMode, ContextPurpose, HostRuntimeLease, LaunchContext,
};
use crate::state::{AccountLifecycleLease, SharedState};
use crate::token_registry_trace::{WebTokenReadMonitor, WEB_TOKEN_VALUE_NAME};

/// 启动进度详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchResult {
    pub account_id: String,
    pub success: bool,
    pub d2r_pid: Option<u32>,
    pub error: Option<String>,
    pub mutex_killed: bool,
}

const MUTEX_NAME: &str = "DiabloII Check For Other Instances";

/// 2026年6月 暴雪更新后常规进程数为7，未来若卡在等待登录需修改此阈值
const BNET_LOGIN_PROCESS_COUNT_THRESHOLD: usize = 7;

fn battle_net_launch_argument(product_code: &str) -> String {
    format!(r#"--exec="launch {product_code}""#)
}

fn spawn_battle_net_launch_command(
    battle_net_path: &str,
    product_code: &str,
) -> std::io::Result<std::process::Child> {
    let launch_argument = battle_net_launch_argument(product_code);
    let mut command = Command::new(battle_net_path);

    #[cfg(windows)]
    {
        // Battle.net parses this switch from the raw Windows command line and expects the
        // value, rather than the complete argument, to be quoted: --exec="launch OSI".
        use std::os::windows::process::CommandExt;
        command.raw_arg(&launch_argument);
    }

    #[cfg(not(windows))]
    command.arg(&launch_argument);

    command.spawn()
}

fn token_and_mutex_are_ready(web_token_read_by_target_pid: bool, mutex_closed: bool) -> bool {
    web_token_read_by_target_pid && mutex_closed
}

fn launch_queue_can_continue(success: bool, mutex_closed: bool) -> bool {
    success && mutex_closed
}

#[derive(Default)]
struct MutexRemovalState {
    closed: std::sync::atomic::AtomicBool,
    found_once: std::sync::atomic::AtomicBool,
    last_error: std::sync::Mutex<Option<String>>,
}

impl MutexRemovalState {
    fn record_found(&self) {
        self.found_once.store(true, Ordering::SeqCst);
    }

    fn record_error(&self, error: impl Into<String>) {
        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = Some(error.into());
        }
    }

    fn confirm_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn diagnostics(&self) -> String {
        if self.is_closed() {
            return "已清除".to_string();
        }
        let last_error = self.last_error.lock().ok().and_then(|error| error.clone());
        if !self.found_once.load(Ordering::SeqCst) {
            return last_error
                .map(|error| format!("未检测到 (最近错误: {error})"))
                .unwrap_or_else(|| "未检测到".to_string());
        }
        last_error
            .map(|error| format!("已检测到但未能确认清除 ({error})"))
            .unwrap_or_else(|| "已检测到但未能确认清除".to_string())
    }
}

fn validate_launch_account_ids(account_ids: &[String]) -> Result<(), AppError> {
    let mut canonical_ids: Vec<String> = account_ids
        .iter()
        .map(|account_id| account_id.to_ascii_lowercase())
        .collect();
    canonical_ids.sort();
    canonical_ids.dedup();
    if canonical_ids.len() != account_ids.len() {
        return Err(AppError::ConfigReadError(
            "启动列表包含重复账号，已拒绝执行".to_string(),
        ));
    }
    for account_id in account_ids {
        AccountManager::validate_account_id(account_id)?;
    }
    Ok(())
}

fn persist_window_position(
    state: &SharedState,
    accounts_dir: &str,
    account_id: &str,
    position: (i32, i32),
) -> bool {
    let Ok(_lease) = AccountLifecycleLease::try_acquire(state, account_id) else {
        return false;
    };
    let Ok(mut meta) = AccountManager::load_meta(accounts_dir, account_id) else {
        return false;
    };
    meta.window_x = Some(position.0);
    meta.window_y = Some(position.1);
    AccountManager::save_meta(accounts_dir, &meta).is_ok()
}

// ── 取消启动 ──

/// 前端点「停止」时调用，后端在下一个检查点中止所有未完成的账号
#[tauri::command]
pub fn cancel_launch(state: tauri::State<'_, SharedState>) -> Result<(), AppError> {
    state.cancel_launch.store(true, Ordering::SeqCst);
    state.cancel_generation.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

/// 取消标志是否已置位
fn is_cancelled(state: &SharedState) -> bool {
    state.cancel_launch.load(Ordering::SeqCst)
}

fn account_path_error(account_id: &str, err: AppError) -> LaunchResult {
    LaunchResult {
        account_id: account_id.to_string(),
        success: false,
        d2r_pid: None,
        error: Some(err.to_string()),
        mutex_killed: false,
    }
}

fn account_window_title(meta: &crate::commands::account::AccountMeta) -> String {
    if meta.display_name.trim().is_empty() {
        meta.id.clone()
    } else {
        meta.display_name.clone()
    }
}

fn unique_account_window_executable(
    config: &crate::commands::global_config::GlobalConfig,
    meta: &crate::commands::account::AccountMeta,
) -> Option<PathBuf> {
    let title = account_window_title(meta);
    let expected = account_game_executable_identity(config, meta).ok()?;
    let matching_accounts = AccountManager::list_ids(&config.accounts_dir)
        .into_iter()
        .filter_map(|id| AccountManager::load_meta(&config.accounts_dir, &id).ok())
        .filter_map(|candidate| {
            let candidate_title = account_window_title(&candidate);
            let executable = account_game_executable_identity(config, &candidate).ok()?;
            Some((candidate_title, executable))
        })
        .filter(|(candidate_title, executable)| {
            candidate_title.eq_ignore_ascii_case(&title)
                && crate::commands::system::executable_paths_match(executable, &expected)
        })
        .count();
    (matching_accounts == 1).then_some(expected)
}

fn already_running_result(
    app: &tauri::AppHandle,
    state: &SharedState,
    account_id: &str,
    window_title: &str,
    pid: u32,
) -> LaunchResult {
    let message = format!("已检测到同名游戏窗口“{window_title}”(PID: {pid})，已跳过重复启动");
    crate::logger::log_msg(
        "WARN",
        "Launch",
        &format!("[Account {account_id}] {message}"),
    );
    state
        .active_games
        .write()
        .insert(account_id.to_string(), pid);
    let _ = app.emit(
        "launch-progress",
        LaunchProgress::new(account_id, "done", "warning", &message),
    );
    LaunchResult {
        account_id: account_id.to_string(),
        success: true,
        d2r_pid: Some(pid),
        // 保留在响应中，前端据此给出非阻断提示；success=true 表明不是启动失败。
        error: Some(message),
        // 已存在的实例不会进入后续队列安全判断，视为无需处理互斥句柄。
        mutex_killed: true,
    }
}

fn skip_existing_account_window(
    app: &tauri::AppHandle,
    state: &SharedState,
    config: &crate::commands::global_config::GlobalConfig,
    account_id: &str,
    meta: &crate::commands::account::AccountMeta,
) -> Option<LaunchResult> {
    let window_title = account_window_title(meta);
    let expected_executable = unique_account_window_executable(config, meta)?;
    let pid = crate::commands::system::find_unique_d2r_pid_by_window_identity(
        &window_title,
        &expected_executable,
    )?;
    if config.separate_game_taskbar_icons {
        let app_id = format!("D2RHub.Account.{account_id}");
        if let Err(error) = crate::commands::system::set_game_window_app_user_model_id(pid, &app_id)
        {
            crate::logger::log_msg("WARN", "Launch", &format!("[Account {account_id}] {error}"));
            let _ = app.emit(
                "launch-progress",
                LaunchProgress::new(account_id, "window", "warning", &error),
            );
        }
    }
    Some(already_running_result(
        app,
        state,
        account_id,
        &window_title,
        pid,
    ))
}

fn checked_account_dir(
    config: &crate::commands::global_config::GlobalConfig,
    account_id: &str,
) -> Result<std::path::PathBuf, LaunchResult> {
    AccountManager::account_dir_checked(&config.accounts_dir, account_id)
        .map_err(|e| account_path_error(account_id, e))
}

const UNIFIED_AUTH_SUBKEY: &str = r"Software\Blizzard Entertainment\Battle.net\UnifiedAuth";
const UNIFIED_AUTH_REG_SECTION: &str =
    r"HKEY_CURRENT_USER\Software\Blizzard Entertainment\Battle.net\UnifiedAuth";

fn validate_legacy_reg_sections(content: &str) -> Result<(), String> {
    let mut section_count = 0usize;
    for (line_index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('[') {
            continue;
        }
        if !trimmed.ends_with(']') {
            return Err(format!(
                "注册表文件第 {} 行包含未闭合的 section",
                line_index + 1
            ));
        }
        let section = trimmed[1..trimmed.len() - 1].trim();
        if !section.eq_ignore_ascii_case(UNIFIED_AUTH_REG_SECTION) {
            return Err(format!("注册表文件包含不允许的 section: [{section}]"));
        }
        section_count += 1;
    }
    if section_count == 0 {
        return Err("注册表文件不包含 UnifiedAuth section".to_string());
    }
    Ok(())
}

fn clear_unified_auth_registry_strict() -> Result<(), AppError> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.delete_subkey_all(UNIFIED_AUTH_SUBKEY) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::RegistryError(format!(
            "清空 UnifiedAuth 注册表失败: {error}"
        ))),
    }
}

fn replace_bnet_roaming_snapshot(source: &Path, target: &Path) -> Result<(), AppError> {
    if !source.is_dir() {
        return Err(AppError::FileError(format!(
            "Battle.net 快照目录不存在: {}",
            source.display()
        )));
    }
    let staged = sibling_with_suffix(target, ".tmp")?;
    let backup = sibling_with_suffix(target, ".bak")?;

    // 此处已持有 HostRuntimeLease；遗留 `.tmp` 必然来自中断事务，可安全丢弃。
    // 先恢复 `.bak`，保证后续复制失败时宿主仍保有上一份完整状态。
    remove_path_if_exists(&staged)?;
    recover_interrupted_replacement(target)?;
    crate::commands::utils::copy_dir_recursive(source, &staged).map_err(|error| {
        let _ = remove_path_if_exists(&staged);
        AppError::FileError(format!(
            "暂存 Battle.net 配置失败: {} -> {} ({error})",
            source.display(),
            staged.display()
        ))
    })?;
    replace_path_with_backup(&staged, target, &backup)
}

fn validate_bnet_snapshot(
    config: &crate::commands::global_config::GlobalConfig,
    meta: &crate::commands::account::AccountMeta,
    expected_edition: crate::launch_context::ClientEdition,
) -> Result<(), AppError> {
    let account_dir = AccountManager::account_dir_checked(&config.accounts_dir, &meta.id)?;
    let snapshot = resolve_account_runtime_snapshot(&account_dir, meta, expected_edition)?;
    if let RegistrySnapshotPath::LegacyReg(reg_path) = snapshot.registry {
        let bytes = std::fs::read(&reg_path)?;
        if bytes.is_empty() {
            return Err(AppError::ConfigReadError(format!(
                "账号 {} 的注册表快照为空",
                meta.id
            )));
        }
        let content = decode_reg_file(&bytes).ok_or_else(|| {
            AppError::ConfigReadError(format!("账号 {} 的注册表快照编码无法识别", meta.id))
        })?;
        validate_legacy_reg_sections(&content).map_err(|error| {
            AppError::ConfigReadError(format!("账号 {} 的注册表快照无效: {error}", meta.id))
        })?;
    }
    Ok(())
}

fn preflight_accounts(
    config: &crate::commands::global_config::GlobalConfig,
    account_ids: &[String],
    purpose: ContextPurpose,
) -> Result<(), AppError> {
    for account_id in account_ids {
        AccountManager::validate_account_id(account_id)?;
        let meta = AccountManager::load_meta(&config.accounts_dir, account_id)?;
        let auth_mode = AuthMode::parse(meta.auth_mode.as_deref())?;
        if purpose == ContextPurpose::BattleNetOnly && auth_mode == AuthMode::Token {
            return Err(AppError::ConfigReadError(format!(
                "Token 认证账号不支持仅启动 Battle.net: {account_id}"
            )));
        }
        if !meta.initialized {
            return Err(AppError::ConfigReadError(format!(
                "账号 {account_id} 尚未初始化"
            )));
        }

        let context = LaunchContext::for_account(config, &meta, purpose)?;
        // Settings.json 是可选能力。缺失、损坏或存档目录不可用时，具体启动流程
        // 会跳过画质覆盖并发出 warning，不能在这里阻断核心认证和 D2R.exe 启动。
        match auth_mode {
            AuthMode::Token => {
                let token = meta
                    .token
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        AppError::ConfigReadError(format!("账号 {account_id} 缺少 Token"))
                    })?;
                let bytes = crate::commands::crypto::hex_decode(token).map_err(|error| {
                    AppError::ConfigReadError(format!(
                        "账号 {account_id} 的 Token 数据损坏: {error}"
                    ))
                })?;
                if bytes.is_empty() {
                    return Err(AppError::ConfigReadError(format!(
                        "账号 {account_id} 的 Token 数据为空"
                    )));
                }
                if purpose == ContextPurpose::LaunchGame && !meta.mod_args.trim().is_empty() {
                    parse_windows_command_line(&meta.mod_args).map_err(|error| {
                        AppError::ConfigReadError(format!(
                            "账号 {account_id} 的 Mod 启动参数无效: {error}"
                        ))
                    })?;
                }
            }
            AuthMode::BattleNet => {
                validate_bnet_snapshot(config, &meta, context.installation.edition)?;
                if purpose == ContextPurpose::LaunchGame
                    && crate::commands::account::is_token_expired(&meta.last_reset_at)
                {
                    return Err(AppError::ConfigReadError(format!(
                        "账号 {account_id} 的认证已超过 30 天，请重新初始化账号"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Terminate only Battle.net processes owned by this installation profile.
/// `/T` includes descendants of the matched process without claiming unrelated Agent.exe trees.
fn kill_battle_net_for_context(context: &LaunchContext, flush_before_kill: bool) {
    let Ok(expected_path) = context.battle_net_executable() else {
        return;
    };
    if flush_before_kill {
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }

    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);
    let process_ids: Vec<u32> = system
        .processes()
        .values()
        .filter(|process| {
            process
                .name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Battle.net.exe")
                && process.exe().is_some_and(|actual| {
                    crate::commands::system::executable_paths_match(actual, expected_path)
                })
        })
        .map(|process| process.pid().as_u32())
        .collect();
    for process_id in process_ids {
        let _ = silent_cmd("taskkill")
            .args(["/F", "/T", "/PID", &process_id.to_string()])
            .output();
    }
}

/// 取消前检查战网状态：
/// - 已登录（进程≥7）：先优雅关闭战网让其 flush 注册表，再回写备份
/// - 运行中但未登录：直接强杀，不回写（注册表中是刚恢复的旧数据，无保存价值）
async fn cancel_with_cleanup(
    config: &crate::commands::global_config::GlobalConfig,
    context: &LaunchContext,
    account_id: &str,
) -> LaunchResult {
    // 取消清理只能认领当前 Launch Context 对应的 Battle.net，绝不回退到其他版本。
    let bnet_count = context
        .battle_net_executable()
        .ok()
        .map(|path| crate::commands::system::count_bnet_processes_for_path(&path.to_string_lossy()))
        .unwrap_or(0);
    let bnet_logged_in = bnet_count >= BNET_LOGIN_PROCESS_COUNT_THRESHOLD;

    if bnet_logged_in {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!(
                "[Account {}] 取消时检测到战网已登录 ({}进程)，先关闭战网再回写认证状态...",
                account_id, bnet_count
            ),
        );

        // 给当前版本的 BNet 时间 flush，再只终止其进程树。
        kill_battle_net_for_context(context, true);

        let account_dir = match checked_account_dir(config, account_id) {
            Ok(dir) => dir,
            Err(res) => return res,
        };
        let config_clone = config.clone();
        let account_dir_clone = account_dir.clone();
        let sync_res = tokio::task::spawn_blocking(move || {
            crate::commands::account::sync_back_to_account(&account_dir_clone, &config_clone)
        })
        .await;

        match sync_res {
            Ok(Ok(())) => {
                crate::logger::log_msg(
                    "INFO",
                    "Launch",
                    &format!("[Account {}] 取消完成，认证状态已回写", account_id),
                );
            }
            Ok(Err(e)) => {
                crate::logger::log_msg(
                    "WARN",
                    "Launch",
                    &format!(
                        "[Account {}] 取消完成，但认证状态回写失败: {}",
                        account_id, e
                    ),
                );
            }
            Err(e) => {
                crate::logger::log_msg(
                    "WARN",
                    "Launch",
                    &format!(
                        "[Account {}] 取消完成，但认证状态回写线程异常: {:?}",
                        account_id, e
                    ),
                );
            }
        }
    } else if bnet_count > 0 {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!(
                "[Account {}] 取消时战网未登录 ({}进程)，直接关闭不保存",
                account_id, bnet_count
            ),
        );
        kill_battle_net_for_context(context, false);
    }

    LaunchResult {
        account_id: account_id.to_string(),
        success: false,
        d2r_pid: None,
        error: Some("启动已被用户取消".to_string()),
        mutex_killed: false,
    }
}

// ── 一键启动 ──

/// 只启动战网（不走游戏、互斥、连接等后续步骤）
#[tauri::command]
pub async fn launch_battle_net_only(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_ids: Vec<String>,
) -> Result<Vec<LaunchResult>, AppError> {
    if account_ids.is_empty() {
        return Ok(Vec::new());
    }
    validate_launch_account_ids(&account_ids)?;
    let config = {
        let cfg = state.config.read();
        cfg.clone()
            .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?
    };
    let mut results = Vec::new();
    let total = account_ids.len();
    let mut host_runtime_lease: Option<HostRuntimeLease> = None;

    for (i, account_id) in account_ids.iter().enumerate() {
        // 宿主租约已建立后，取消标记才属于当前批次。首次有效账号取得租约时会清除
        // 上一批遗留的标记，避免新请求一进入循环就被旧状态误取消。
        if host_runtime_lease.is_some() && is_cancelled(&state) {
            emit_cancelled(&app, account_id);
            results.push(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("启动已被用户取消".to_string()),
                mutex_killed: false,
            });
            for remaining in &account_ids[i + 1..] {
                emit_cancelled(&app, remaining);
                results.push(LaunchResult {
                    account_id: remaining.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some("启动已被用户取消".to_string()),
                    mutex_killed: false,
                });
            }
            return Ok(results);
        }

        let _account_lease = match AccountLifecycleLease::try_acquire(state.inner(), account_id) {
            Ok(lease) => lease,
            Err(error) => {
                let message = error.to_string();
                let _ = app.emit(
                    "launch-progress",
                    LaunchProgress::new(account_id, "done", "error", &message),
                );
                results.push(account_path_error(account_id, error));
                continue;
            }
        };
        if let Err(error) = preflight_accounts(
            &config,
            std::slice::from_ref(account_id),
            ContextPurpose::BattleNetOnly,
        ) {
            let message = error.to_string();
            let _ = app.emit(
                "launch-progress",
                LaunchProgress::new(account_id, "done", "error", &message),
            );
            results.push(account_path_error(account_id, error));
            continue;
        }

        if host_runtime_lease.is_none() {
            match HostRuntimeLease::try_acquire(state.inner().as_ref()) {
                Ok(lease) => {
                    host_runtime_lease = Some(lease);
                    state.cancel_launch.store(false, Ordering::SeqCst);
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = app.emit(
                        "launch-progress",
                        LaunchProgress::new(account_id, "done", "error", &message),
                    );
                    results.push(account_path_error(account_id, error));
                    continue;
                }
            }
        }

        if is_cancelled(&state) {
            emit_cancelled(&app, account_id);
            results.push(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("启动已被用户取消".to_string()),
                mutex_killed: false,
            });
            for remaining in &account_ids[i + 1..] {
                emit_cancelled(&app, remaining);
                results.push(LaunchResult {
                    account_id: remaining.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some("启动已被用户取消".to_string()),
                    mutex_killed: false,
                });
            }
            return Ok(results);
        }

        let msg = format!("[{}/{}] 仅启动战网", i + 1, total);
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [queue] [running]: {}", account_id, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, "queue", "running", &msg),
        );

        let result = launch_single_bnet_only(&app, &config, &state, account_id).await;
        crate::logger::log_msg(
            if result.success { "INFO" } else { "ERROR" },
            "Launch",
            &format!(
                "[Account {}] 仅启动战网结果: success={}, error={:?}",
                account_id, result.success, result.error
            ),
        );
        results.push(result);

        if i + 1 < total && !is_cancelled(&state) {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    Ok(results)
}

async fn prepare_bnet_environment(
    app: &tauri::AppHandle,
    config: &crate::commands::global_config::GlobalConfig,
    state: &SharedState,
    account_id: &str,
    wait_login: bool,
) -> Result<LaunchContext, LaunchResult> {
    // Resolve the complete account context before any process, file, or registry mutation.
    let meta = AccountManager::load_meta(&config.accounts_dir, account_id)
        .map_err(|error| account_path_error(account_id, error))?;
    let auth_mode = AuthMode::parse(meta.auth_mode.as_deref())
        .map_err(|error| account_path_error(account_id, error))?;
    if auth_mode != AuthMode::BattleNet {
        return Err(account_path_error(
            account_id,
            AppError::ConfigReadError("Token 认证账号不能进入 Battle.net 启动流程".to_string()),
        ));
    }
    let purpose = if wait_login {
        ContextPurpose::BattleNetOnly
    } else {
        ContextPurpose::LaunchGame
    };
    let context = LaunchContext::for_account(config, &meta, purpose)
        .map_err(|error| account_path_error(account_id, error))?;
    let battle_net_path = context
        .battle_net_executable()
        .map_err(|error| account_path_error(account_id, error))?
        .to_string_lossy()
        .to_string();
    let saved_games_path = context.installation.saved_games_directory.clone();
    let battle_net_config_game_key = context.edition.battle_net_config_game_key;
    let has_customized_settings = meta.has_customized_settings;
    let mod_args = meta.mod_args.clone();

    let emit = |step: &str, status: &str, msg: &str| {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [{}] [{}]: {}", account_id, step, status, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, step, status, msg),
        );
    };

    let cancelled = || -> LaunchResult {
        LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some("启动已被用户取消".to_string()),
            mutex_killed: false,
        }
    };

    // ── Step 1: 环境清理 ──
    emit("clean", "running", "正在清理战网和 Agent 进程...");
    let clean_res = tokio::task::spawn_blocking(|| {
        crate::commands::utils::kill_processes_by_name(&["Battle.net.exe", "Agent.exe"])
    })
    .await;

    match clean_res {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let message = format!("清理共享进程失败: {error}");
            emit("clean", "error", &message);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(message),
                mutex_killed: false,
            });
        }
        Err(error) => {
            let message = format!("清理环境线程异常: {error}");
            emit("clean", "error", &message);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(message),
                mutex_killed: false,
            });
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    emit("clean", "ok", "环境已清理");

    if is_cancelled(state) {
        emit("done", "error", "已取消");
        return Err(cancelled());
    }

    // ── Step 2: 配置覆盖 ──
    emit("copy", "running", "正在覆盖配置文件...");

    let accounts_dir = config.accounts_dir.clone();
    let app_data_roaming_bnet_path = config.app_data_roaming_bnet_path.clone();
    let account_id_str = account_id.to_string();
    let snapshot_meta = meta.clone();
    let snapshot_edition = context.installation.edition;

    let copy_res = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        let account_dir = AccountManager::account_dir_checked(&accounts_dir, &account_id_str)
            .map_err(|e| e.to_string())?;
        let mut optional_warning = None;

        let snapshot =
            resolve_account_runtime_snapshot(&account_dir, &snapshot_meta, snapshot_edition)
                .map_err(|error| format!("解析账号运行时快照失败: {error}"))?;

        // 2.1 精确替换 Battle.net Roaming 配置
        let bnet_dst = Path::new(&app_data_roaming_bnet_path);
        replace_bnet_roaming_snapshot(&snapshot.bnet_directory, bnet_dst)
            .map_err(|error| error.to_string())?;

        // 2.2 导入注册表（已初始化账号必须成功，否则战网无法自动登录）
        match snapshot.registry {
            RegistrySnapshotPath::Json(json_path) => {
                // JSON 恢复函数在验证完整快照后自行清空并写入，调用方不得重复预清空。
                crate::commands::account::restore_registry_from_json(&json_path)
                    .map_err(|e| format!("恢复注册表失败: {e}"))?;
            }
            RegistrySnapshotPath::LegacyReg(reg_path) => {
                let reg_bytes =
                    std::fs::read(&reg_path).map_err(|e| format!("读取注册表文件失败: {e}"))?;
                if reg_bytes.is_empty() {
                    return Err("注册表文件为空，导入被拒绝".to_string());
                }
                let reg_content = decode_reg_file(&reg_bytes).ok_or_else(|| {
                    "注册表文件编码无法识别（需 UTF-8 或 UTF-16LE），导入被拒绝".to_string()
                })?;
                validate_legacy_reg_sections(&reg_content)?;
                clear_unified_auth_registry_strict().map_err(|error| error.to_string())?;
                let output = silent_cmd("reg")
                    .args(["import", &reg_path.to_string_lossy()])
                    .output()
                    .map_err(|e| format!("执行 reg import 失败: {e}"))?;
                if !output.status.success() {
                    let import_error = format!(
                        "reg import 返回非零退出码: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    return match clear_unified_auth_registry_strict() {
                        Ok(()) => Err(import_error),
                        Err(cleanup_error) => Err(format!(
                            "{import_error}；清理部分导入也失败: {cleanup_error}"
                        )),
                    };
                }
            }
        }

        // 2.3 覆盖 Settings.json
        if has_customized_settings {
            match saved_games_path.as_deref() {
                Some(saved_games_path) => {
                    if let Err(error) =
                        copy_account_settings_to_system(&account_dir, saved_games_path)
                    {
                        optional_warning = Some(format!(
                            "独立 Settings.json 覆盖失败，已继续使用系统配置: {error}"
                        ));
                    }
                }
                None => {
                    optional_warning =
                        Some("未配置可用的存档目录，已跳过独立 Settings.json 覆盖".to_string());
                }
            }
        } else {
            crate::logger::log_msg(
                "INFO",
                "Launch",
                &format!(
                    "[Account {}] 使用系统 Settings.json，跳过账号画质配置覆盖",
                    account_id_str
                ),
            );
        }

        let bnet_config_path = bnet_dst.join("Battle.net.config");

        // 2.3.5 注入 Mod 参数
        if bnet_config_path.exists() {
            update_mod_args(&bnet_config_path, battle_net_config_game_key, &mod_args)
                .map_err(|error| format!("注入 Mod 参数失败: {error}"))?;
        }

        // 2.4 强制确保 SingleInstance
        if let Err(e) = crate::commands::account::enforce_single_instance(&bnet_config_path) {
            crate::logger::log_msg(
                "WARN",
                "Launch",
                &format!(
                    "[Account {}] SingleInstance 校验失败: {}",
                    account_id_str, e
                ),
            );
        }

        Ok(optional_warning)
    })
    .await;

    match copy_res {
        Ok(Ok(optional_warning)) => {
            if let Some(warning) = optional_warning {
                emit("copy", "warning", &warning);
            }
            emit("copy", "ok", "配置文件覆盖完成");
        }
        Ok(Err(e)) => {
            emit("copy", "error", &e);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(e),
                mutex_killed: false,
            });
        }
        Err(_) => {
            let msg = "执行配置覆盖线程异常".to_string();
            emit("copy", "error", &msg);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(msg),
                mutex_killed: false,
            });
        }
    }

    if is_cancelled(state) {
        emit("done", "error", "已取消");
        return Err(cancelled());
    }

    // ── Step 3: 启动战网并等待登录 ──
    emit("launch", "running", "正在启动战网客户端...");

    // 基础安全校验：确保路径指向预期的可执行文件
    if !battle_net_path.to_lowercase().ends_with("battle.net.exe") {
        let msg = format!("战网路径异常，预期 Battle.net.exe: {}", battle_net_path);
        emit("launch", "error", &msg);
        return Err(LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some(msg),
            mutex_killed: false,
        });
    }
    let battle_net_spawn_path = battle_net_path.clone();
    let spawn_res =
        tokio::task::spawn_blocking(move || Command::new(&battle_net_spawn_path).spawn()).await;

    match spawn_res {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            let msg = format!("启动战网失败: {}", e);
            emit("launch", "error", &msg);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(msg),
                mutex_killed: false,
            });
        }
        Err(_) => {
            let msg = "启动战网线程异常".to_string();
            emit("launch", "error", &msg);
            return Err(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(msg),
                mutex_killed: false,
            });
        }
    }

    if !wait_login {
        // 游戏新进程启动前由 launch_single 主循环重复检查并强杀 Agent.exe
        emit("launch", "ok", "战网客户端已启动，进入进程与 Agent 监控...");
    } else {
        let mut bnet_ready = false;
        for i in 1..=60 {
            if is_cancelled(state) {
                emit("done", "error", "已取消，正在保存状态...");
                return Err(cancel_with_cleanup(config, &context, account_id).await);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let count = crate::commands::system::count_bnet_processes_for_path(&battle_net_path);
            if count >= BNET_LOGIN_PROCESS_COUNT_THRESHOLD {
                bnet_ready = true;
                emit(
                    "launch",
                    "ok",
                    &format!("战网已登录 ({}s, {}进程)", i, count),
                );
                break;
            }
            if i % 5 == 0 {
                emit(
                    "launch",
                    "running",
                    &format!("等待战网登录... ({}s, {}进程)", i, count),
                );
            }
        }
        if !bnet_ready {
            emit("launch", "warning", "未检测到战网登录，但战网已启动");
        } else {
            // 登录成功，立即回写 token 快照作为检查点
            // 后续无论游戏崩溃、用户取消、还是进程被杀，至少保留一份刚登录时的有效 token
            emit("checkpoint", "running", "登录成功，保存认证检查点...");
            let account_dir = checked_account_dir(config, account_id)?;
            let config_clone = config.clone();
            let account_dir_clone = account_dir.clone();
            let sync_res = tokio::task::spawn_blocking(move || {
                crate::commands::account::sync_back_to_account(&account_dir_clone, &config_clone)
            })
            .await;
            match sync_res {
                Ok(Ok(())) => {
                    emit("checkpoint", "ok", "认证检查点已保存");
                }
                Ok(Err(e)) => {
                    emit(
                        "checkpoint",
                        "warning",
                        &format!("保存认证检查点失败: {}", e),
                    );
                }
                Err(_) => {
                    emit("checkpoint", "warning", "保存认证检查点线程异常");
                }
            }
        }
    }

    if is_cancelled(state) {
        emit("done", "error", "已取消，正在保存状态...");
        return Err(cancel_with_cleanup(config, &context, account_id).await);
    }

    Ok(context)
}

async fn launch_single_bnet_only(
    app: &tauri::AppHandle,
    config: &crate::commands::global_config::GlobalConfig,
    state: &SharedState,
    account_id: &str,
) -> LaunchResult {
    let emit = |step: &str, status: &str, msg: &str| {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [{}] [{}]: {}", account_id, step, status, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, step, status, msg),
        );
    };

    if let Err(res) = prepare_bnet_environment(app, config, state, account_id, true).await {
        return res;
    }

    // Step 4: 回写最新认证状态（战网登录后 token 可能已刷新）
    emit("cleanup", "running", "正在回写认证状态...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let account_dir = match checked_account_dir(config, account_id) {
        Ok(dir) => dir,
        Err(res) => return res,
    };

    let config_clone = config.clone();
    let account_dir_clone = account_dir.clone();
    let sync_res = tokio::task::spawn_blocking(move || {
        crate::commands::account::sync_back_to_account(&account_dir_clone, &config_clone)
    })
    .await;

    match sync_res {
        Ok(Ok(())) => {
            emit("cleanup", "ok", "认证状态已同步");
        }
        Ok(Err(e)) => {
            emit("cleanup", "warning", &format!("回写状态失败: {}", e));
        }
        Err(_) => {
            emit("cleanup", "warning", "回写状态线程异常");
        }
    }

    emit("done", "ok", "战网启动完成（仅启动战网）");
    LaunchResult {
        account_id: account_id.to_string(),
        success: true,
        d2r_pid: None,
        error: None,
        mutex_killed: false,
    }
}

/// 一键启动选中的账号列表
/// 逐个串行启动：一个账号完整走完（清理→覆盖→启动战网→游戏→清互斥→连接→关战网）
/// 再开始下一个。两个账号之间留 2 秒缓冲。
#[tauri::command]
pub async fn launch_accounts(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    account_ids: Vec<String>,
) -> Result<Vec<LaunchResult>, AppError> {
    if account_ids.is_empty() {
        return Ok(Vec::new());
    }
    // 此处只校验批次，不提前占用账号租约。每个账号会在同名窗口检查之后单独
    // 获取租约；提前持有再逐项获取会让启动流程被自己的租约误判为并发操作。
    validate_launch_account_ids(&account_ids)?;
    let config = {
        let cfg = state.config.read();
        cfg.clone()
            .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?
    };
    let mut results = Vec::new();
    let total = account_ids.len();
    // 同名窗口检查不修改共享状态，因此不应被宿主运行时租约阻断。直到确实有账号
    // 要启动时才取得租约，并一直持有到本批次结束。
    let mut host_runtime_lease: Option<HostRuntimeLease> = None;

    for (i, account_id) in account_ids.iter().enumerate() {
        // 先按精确窗口标题检查。命中后不执行注册表、Settings、Battle.net 或 D2R
        // 的任何启动步骤，且不会阻断同一批次中的其他账号。
        let meta = match AccountManager::load_meta(&config.accounts_dir, account_id) {
            Ok(meta) => meta,
            Err(error) => {
                let result = account_path_error(account_id, error);
                let message = result
                    .error
                    .clone()
                    .unwrap_or_else(|| "账号配置无效".to_string());
                let _ = app.emit(
                    "launch-progress",
                    LaunchProgress::new(account_id, "done", "error", &message),
                );
                results.push(result);
                continue;
            }
        };
        if let Some(result) =
            skip_existing_account_window(&app, state.inner(), &config, account_id, &meta)
        {
            results.push(result);
            continue;
        }

        // 同名窗口检查是只读的，优先执行；只有确实需要启动时才取得该账号的生命周期
        // 租约。账号编辑/导入占用只影响当前账号，不阻断同批次的其他账号。
        let _account_lease = match AccountLifecycleLease::try_acquire(state.inner(), account_id) {
            Ok(lease) => lease,
            Err(error) => {
                let message = error.to_string();
                let _ = app.emit(
                    "launch-progress",
                    LaunchProgress::new(account_id, "done", "error", &message),
                );
                results.push(account_path_error(account_id, error));
                continue;
            }
        };
        // 关闭首次只读检查与取得租约之间的竞态窗口，租约内重新读取一次元数据。
        let meta = match AccountManager::load_meta(&config.accounts_dir, account_id) {
            Ok(meta) => meta,
            Err(error) => {
                results.push(account_path_error(account_id, error));
                continue;
            }
        };
        if let Some(result) =
            skip_existing_account_window(&app, state.inner(), &config, account_id, &meta)
        {
            results.push(result);
            continue;
        }

        // 单个账号的只读预检失败只影响该账号；未触碰共享宿主状态，可以继续下一项。
        if let Err(error) = preflight_accounts(
            &config,
            std::slice::from_ref(account_id),
            ContextPurpose::LaunchGame,
        ) {
            let message = error.to_string();
            let _ = app.emit(
                "launch-progress",
                LaunchProgress::new(account_id, "done", "error", &message),
            );
            results.push(account_path_error(account_id, error));
            continue;
        }

        if host_runtime_lease.is_none() {
            // Token 启动同样会覆盖机器级 Launch Options\OSI 与 Settings.json；只在
            // 真正启动前取得宿主租约，避免并发流程串号或串配置。
            match HostRuntimeLease::try_acquire(state.inner().as_ref()) {
                Ok(lease) => {
                    host_runtime_lease = Some(lease);
                    // 只有成功持有租约的请求才能清除上一批次遗留的取消标记。
                    state.cancel_launch.store(false, Ordering::SeqCst);
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = app.emit(
                        "launch-progress",
                        LaunchProgress::new(account_id, "done", "error", &message),
                    );
                    results.push(account_path_error(account_id, error));
                    continue;
                }
            }
        }

        // 每个真正需要启动的账号开始前检查取消标志；已存在的同名窗口仍按上方
        // 逻辑直接跳过并提示，不会被取消状态误报为失败。
        if is_cancelled(&state) {
            emit_cancelled(&app, account_id);
            results.push(LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("启动已被用户取消".to_string()),
                mutex_killed: false,
            });
            // 剩余未启动的账号也标记为取消
            for remaining in &account_ids[i + 1..] {
                emit_cancelled(&app, remaining);
                results.push(LaunchResult {
                    account_id: remaining.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some("启动已被用户取消".to_string()),
                    mutex_killed: false,
                });
            }
            return Ok(results);
        }

        let msg = format!("[{}/{}] 开始启动账号", i + 1, total);
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [queue] [running]: {}", account_id, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, "queue", "running", &msg),
        );

        // 当前账号的生命周期租约保证本次启动期间元数据稳定。
        let result = launch_single(&app, &config, &state, account_id).await;
        let killed = result.mutex_killed;
        let success = result.success;
        let pid = result.d2r_pid;
        let err = result.error.clone();
        crate::logger::log_msg(
            if success { "INFO" } else { "ERROR" },
            "Launch",
            &format!(
                "[Account {}] 启动结果: success={}, pid={:?}, error={:?}, mutex_killed={}",
                account_id, success, pid, err, killed
            ),
        );
        results.push(result);

        // 两种认证模式都必须确认目标 D2R 已消费 WEB_TOKEN，并成功清除互斥句柄，
        // 才允许覆盖共享 Token 启动下一账号。
        if !launch_queue_can_continue(success, killed) && i + 1 < total {
            let (queue_message, remaining_error) = if success {
                ("互斥句柄未清除，后续账号暂停启动", "互斥句柄未清除，已暂停")
            } else {
                (
                    "当前账号启动失败，后续账号暂停启动",
                    "前序账号启动失败，已暂停",
                )
            };
            let _ = app.emit(
                "launch-progress",
                LaunchProgress::new(account_id, "queue", "warning", queue_message),
            );
            for remaining in &account_ids[i + 1..] {
                emit_cancelled(&app, remaining);
                results.push(LaunchResult {
                    account_id: remaining.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some(remaining_error.to_string()),
                    mutex_killed: false,
                });
            }
            return Ok(results);
        }

        // 如果还有下一个账号，等 2 秒让系统稳定
        if i + 1 < total && !is_cancelled(&state) {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    Ok(results)
}

fn emit_cancelled(app: &tauri::AppHandle, account_id: &str) {
    crate::logger::log_msg(
        "INFO",
        "Launch",
        &format!("[Account {}] 启动已被用户取消", account_id),
    );
    let _ = app.emit(
        "launch-progress",
        LaunchProgress::new(account_id, "done", "error", "已取消"),
    );
}

async fn launch_single(
    app: &tauri::AppHandle,
    config: &crate::commands::global_config::GlobalConfig,
    state: &SharedState,
    account_id: &str,
) -> LaunchResult {
    let emit = |step: &str, status: &str, msg: &str| {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [{}] [{}]: {}", account_id, step, status, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, step, status, msg),
        );
    };

    let meta = match AccountManager::load_meta(&config.accounts_dir, account_id) {
        Ok(meta) => meta,
        Err(error) => return account_path_error(account_id, error),
    };
    let preflight_context =
        match LaunchContext::for_account(config, &meta, ContextPurpose::LaunchGame) {
            Ok(context) => context,
            Err(error) => return account_path_error(account_id, error),
        };

    // ── Token 过期检查（仅战网模式）──
    if preflight_context.auth_mode == AuthMode::BattleNet
        && crate::commands::account::is_token_expired(&meta.last_reset_at)
    {
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some("Token 已过期（超过30天），请重新初始化账号".to_string()),
            mutex_killed: false,
        };
    }

    if preflight_context.auth_mode == AuthMode::Token {
        return launch_single_token(app, config, state, account_id, &meta, &preflight_context)
            .await;
    }

    let context = match prepare_bnet_environment(app, config, state, account_id, false).await {
        Ok(context) => context,
        Err(result) => return result,
    };
    let product_code = context.edition.battle_net_launch_product;
    let battle_net_path = match context.battle_net_executable() {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(error) => return account_path_error(account_id, error),
    };
    let expected_game_path = context.installation.game_executable.clone();

    // Battle.net 最终同样通过注册表 WEB_TOKEN 启动 D2R。监听必须在发送游戏
    // 启动指令前开始，避免游戏在发现 PID 之前就完成 Token 读取。
    emit("connect", "running", "正在启动 WEB_TOKEN ETW 读取监听...");
    let token_read_monitor = match WebTokenReadMonitor::start() {
        Ok(monitor) => monitor,
        Err(error) => {
            emit("connect", "error", &error);
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(error),
                mutex_killed: false,
            };
        }
    };

    // ── Step 4: 记录当前 D2R 进程快照 ──
    let before_pids = crate::commands::system::snapshot_processes("D2R.exe".to_string());

    // ── Step 5 & 6: 发送游戏启动指令并等待新 D2R 进程 ──
    emit("game", "running", "正在启动游戏进程...");

    let mut locked_agent_pid: Option<u32> = None;
    let mut first_agent_killed = false;
    let mut agent_locked_at: Option<std::time::Instant> = None;
    let mut last_launch_sent: Option<std::time::Instant> = None;
    let mut last_launch_error: Option<String> = None;
    let mut d2r_pid_opt: Option<u32> = None;
    let mut sys = sysinfo::System::new(); // 优化：复用 System 实例以提高效率
                                          // 跟踪已尝试 kill 的 Agent PID，避免重复日志洪水
    let mut killed_agent_pids: std::collections::HashSet<u32> = std::collections::HashSet::new();

    let wait_start = std::time::Instant::now();
    let timeout_secs = 60;

    while wait_start.elapsed().as_secs() < timeout_secs {
        if is_cancelled(state) {
            emit("done", "error", "已取消，正在保存状态...");
            return cancel_with_cleanup(config, &context, account_id).await;
        }

        struct SysStatus {
            agent_pids: Vec<u32>,
            bnet_count: usize,
            d2r_pids: Vec<u32>,
        }

        let monitored_bnet_path = battle_net_path.clone();
        let monitored_game_path = expected_game_path.clone();
        let (status, sys_ret) = tokio::task::spawn_blocking(move || {
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All);

            let mut agent_pids = Vec::new();
            let mut bnet_count = 0;
            let mut d2r_pids = Vec::new();

            for (pid, proc) in sys.processes() {
                let name = proc.name().to_string_lossy();
                if name.eq_ignore_ascii_case("Agent.exe") {
                    agent_pids.push(pid.as_u32());
                } else if name.eq_ignore_ascii_case("Battle.net.exe") {
                    if proc.exe().is_some_and(|actual| {
                        crate::commands::system::executable_paths_match(
                            actual,
                            Path::new(&monitored_bnet_path),
                        )
                    }) {
                        bnet_count += 1;
                    }
                } else if name.eq_ignore_ascii_case("D2R.exe")
                    && proc.exe().is_some_and(|actual| {
                        crate::commands::system::executable_paths_match(
                            actual,
                            &monitored_game_path,
                        )
                    })
                {
                    d2r_pids.push(pid.as_u32());
                }
            }

            (
                SysStatus {
                    agent_pids,
                    bnet_count,
                    d2r_pids,
                },
                sys,
            )
        })
        .await
        .unwrap_or((
            SysStatus {
                agent_pids: Vec::new(),
                bnet_count: 0,
                d2r_pids: Vec::new(),
            },
            sysinfo::System::new(),
        ));
        sys = sys_ret;

        // 1. 检查是否有新进程
        let mut found_new = false;
        for pid in &status.d2r_pids {
            if !before_pids.contains(pid) {
                d2r_pid_opt = Some(*pid);
                found_new = true;
                break;
            }
        }
        if found_new {
            break;
        }

        // 2. 检测 Agent.exe 进程并锁定 (模式3跳过)
        if config.agent_mode != 3 && !first_agent_killed {
            if let Some(pid) = locked_agent_pid {
                if !status.agent_pids.contains(&pid) {
                    locked_agent_pid = None;
                    agent_locked_at = None;
                }
            }
            if locked_agent_pid.is_none() {
                if let Some(&first_pid) = status.agent_pids.first() {
                    locked_agent_pid = Some(first_pid);
                    agent_locked_at = Some(std::time::Instant::now());
                    emit(
                        "game",
                        "running",
                        &format!("已锁定战网 Agent 进程 (PID: {})", first_pid),
                    );
                }
            }
        }

        // 3. Agent 杀一次逻辑：根据多开模式决定何时杀 (模式3跳过)
        if config.agent_mode != 3 {
            if !first_agent_killed {
                if let Some(agent_pid) = locked_agent_pid {
                    let should_kill = match config.agent_mode {
                        2 => status.bnet_count >= config.agent_threshold as usize,
                        _ => {
                            // 模式1 (默认): 从 Agent 被锁定起等待 agent_delay_secs 秒
                            agent_locked_at
                                .map(|t| t.elapsed().as_secs_f64() >= config.agent_delay_secs)
                                .unwrap_or(false)
                        }
                    };
                    if should_kill {
                        let agent_pid_copy = agent_pid;
                        let _ = tokio::task::spawn_blocking(move || {
                            let _ = silent_cmd("taskkill")
                                .args(["/F", "/PID", &agent_pid_copy.to_string()])
                                .output();
                        })
                        .await;
                        first_agent_killed = true;
                        locked_agent_pid = None; // Reset
                        if config.agent_mode == 1 {
                            emit(
                                "game",
                                "running",
                                &format!(
                                    "Agent 存活 {}s 后已终止 (PID: {})",
                                    config.agent_delay_secs, agent_pid
                                ),
                            );
                        } else {
                            emit(
                                "game",
                                "running",
                                &format!(
                                    "战网进程数达到 {} (≥{})，已终止 Agent (PID: {})",
                                    status.bnet_count, config.agent_threshold, agent_pid
                                ),
                            );
                        }
                    }
                }
            } else {
                // 后续追着杀：查到就秒杀（每次都执行 taskkill，仅首次 emit 日志）
                for &pid in &status.agent_pids {
                    let first_seen = killed_agent_pids.insert(pid);
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = silent_cmd("taskkill")
                            .args(["/F", "/PID", &pid.to_string()])
                            .output();
                    })
                    .await;
                    if first_seen {
                        emit(
                            "game",
                            "running",
                            &format!("检测到新生 Agent 进程，已立即秒杀 (PID: {})", pid),
                        );
                    }
                }
            }
        }

        // 4. 判断 Battle.net.exe 数量是否大于 5，只有大于 5 才发送游戏启动指令
        if status.bnet_count > 5 {
            let should_send = match last_launch_sent {
                None => true,
                Some(last) => last.elapsed() >= std::time::Duration::from_secs(5),
            };

            if should_send {
                let battle_net_path = battle_net_path.clone();
                emit(
                    "game",
                    "running",
                    &format!(
                        "战网进程数达到 {} (>5)，正在发送游戏启动指令 ({})...",
                        status.bnet_count, product_code
                    ),
                );
                let launch_result = tokio::task::spawn_blocking(move || {
                    spawn_battle_net_launch_command(&battle_net_path, product_code)
                })
                .await;
                match launch_result {
                    Ok(Ok(_child)) => {
                        last_launch_error = None;
                        emit(
                            "game",
                            "running",
                            &format!("已向战网提交游戏启动指令 ({})", product_code),
                        );
                    }
                    Ok(Err(error)) => {
                        let message = format!("发送游戏启动指令失败: {error}");
                        last_launch_error = Some(message.clone());
                        emit("game", "warning", &message);
                    }
                    Err(error) => {
                        let message = format!("发送游戏启动指令线程异常: {error}");
                        last_launch_error = Some(message.clone());
                        emit("game", "warning", &message);
                    }
                }
                last_launch_sent = Some(std::time::Instant::now());
            }
        } else {
            if last_launch_sent.is_none() {
                emit(
                    "game",
                    "running",
                    &format!(
                        "等待战网客户端加载... (当前战网进程数: {})",
                        status.bnet_count
                    ),
                );
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let d2r_pid = match d2r_pid_opt {
        Some(pid) => {
            emit("game", "ok", &format!("游戏进程已启动 (PID: {})", pid));
            state.record_active_game(account_id, pid, &meta.mod_args);
            crate::audio_mod::emit_runtime_compatibility_warning(
                app,
                state,
                config,
                &meta,
                pid,
                &meta.mod_args,
            );
            // 将游戏窗口标题改为账号昵称，并调整窗口位置（如已配置）
            if let Ok(meta) = AccountManager::load_meta(&config.accounts_dir, account_id) {
                let win_title = if meta.display_name.is_empty() {
                    account_id.to_string()
                } else {
                    meta.display_name.clone()
                };
                let win_x = meta.window_x;
                let win_y = meta.window_y;
                // 延迟重试 + 位置持续轮询
                let pid_copy = pid;
                let title_copy = win_title.clone();
                let accounts_dir = config.accounts_dir.clone();
                let account_id_owned = account_id.to_string();
                let state_for_position = state.clone();
                let separate_taskbar_icon = config.separate_game_taskbar_icons;
                let app_user_model_id = format!("D2RHub.Account.{account_id}");
                let app_for_window = app.clone();
                let account_id_for_window = account_id.to_string();
                tokio::task::spawn_blocking(move || {
                    // Phase 1: 10 次重试重命名 + 初始定位
                    let mut taskbar_configured = !separate_taskbar_icon;
                    let mut taskbar_error = None;
                    for _ in 0..10 {
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        crate::commands::system::rename_game_window(pid_copy, &title_copy);
                        if !taskbar_configured {
                            match crate::commands::system::set_game_window_app_user_model_id(
                                pid_copy,
                                &app_user_model_id,
                            ) {
                                Ok(()) => taskbar_configured = true,
                                Err(error) => taskbar_error = Some(error),
                            }
                        }
                        if let (Some(x), Some(y)) = (win_x, win_y) {
                            crate::commands::system::set_game_window_position(pid_copy, x, y);
                        }
                    }
                    if !taskbar_configured {
                        let message = taskbar_error
                            .unwrap_or_else(|| "游戏窗口任务栏独立分组设置失败".to_string());
                        crate::logger::log_msg(
                            "WARN",
                            "Launch",
                            &format!("[Account {account_id_for_window}] {message}"),
                        );
                        let _ = app_for_window.emit(
                            "launch-progress",
                            LaunchProgress::new(
                                &account_id_for_window,
                                "window",
                                "warning",
                                &message,
                            ),
                        );
                    }
                    // Phase 2: 窗口位置轮询，拖动停止后反向写入账号配置
                    let mut sys = sysinfo::System::new();
                    let sys_pid = sysinfo::Pid::from(pid_copy as usize);
                    let mut last_pos: Option<(i32, i32)> = None;
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sys_pid]));
                        let alive = sys
                            .process(sys_pid)
                            .map(|p| p.name().to_string_lossy().eq_ignore_ascii_case("D2R.exe"))
                            .unwrap_or(false);
                        if !alive {
                            break;
                        }
                        if let Some(hwnd) = crate::commands::system::find_game_hwnd(pid_copy) {
                            if let Some(pos) = crate::commands::system::get_window_rect(hwnd) {
                                // 过滤最小化时的异常坐标（Windows 对最小化窗口返回 ~-32000）
                                if pos.0 > -10000 && pos.1 > -10000 && last_pos != Some(pos) {
                                    // 只有真正落盘后才更新 last_pos；租约忙时保留旧值，
                                    // 下一轮会在启动事务释放账号租约后自动重试。
                                    if persist_window_position(
                                        &state_for_position,
                                        &accounts_dir,
                                        &account_id_owned,
                                        pos,
                                    ) {
                                        last_pos = Some(pos);
                                    }
                                }
                            }
                        }
                    }
                });
            }
            pid
        }
        None => {
            let error = last_launch_error.unwrap_or_else(|| "等待游戏进程启动超时".to_string());
            emit("game", "error", &error);
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(error),
                mutex_killed: false,
            };
        }
    };

    if is_cancelled(state) {
        emit("done", "error", "已取消，正在保存状态...");
        return cancel_with_cleanup(config, &context, account_id).await;
    }

    // ── Step 7: 互斥句柄清除 (后台任务，与 Step 8 并发) ──
    let mutex_killed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mutex_found_once = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mutex_task = {
        let killed = mutex_killed.clone();
        let found = mutex_found_once.clone();
        let state_clone = state.clone();
        tokio::spawn(async move {
            loop {
                if is_cancelled(&state_clone) {
                    break;
                }
                if let Ok(Some(hid)) =
                    crate::commands::system::find_mutex_handle(d2r_pid, MUTEX_NAME)
                {
                    found.store(true, std::sync::atomic::Ordering::SeqCst);
                    let _ = crate::commands::system::close_handle(d2r_pid, &hid);
                    if let Ok(None) =
                        crate::commands::system::find_mutex_handle(d2r_pid, MUTEX_NAME)
                    {
                        killed.store(true, std::sync::atomic::Ordering::SeqCst);
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        })
    };

    // ── Step 8: 持续跳过动画，直到目标 D2R 消费 WEB_TOKEN ──
    // 保留原来的 2 秒窗口初始化等待；如果 ETW 更早命中，任务会在首次按键前终止。
    let intro_skip_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        loop {
            let _ = crate::commands::system::send_keys_to_window(d2r_pid);
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    });

    emit("connect", "running", "正在等待 D2R 读取 WEB_TOKEN...");
    emit("mutex", "running", "后台监控互斥句柄中...");

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(60);
    let mut web_token_read;
    let mut mutex_closed;
    let mut launch_ready;
    let mut token_read_logged = false;
    let mut mutex_closed_logged = false;

    loop {
        if is_cancelled(state) {
            emit("done", "error", "已取消，正在保存状态...");
            mutex_task.abort();
            intro_skip_task.abort();
            return cancel_with_cleanup(config, &context, account_id).await;
        }

        web_token_read = token_read_monitor.was_read_by(d2r_pid);
        mutex_closed = mutex_killed.load(std::sync::atomic::Ordering::SeqCst);
        if web_token_read && !token_read_logged {
            intro_skip_task.abort();
            emit(
                "connect",
                "ok",
                "检测到 D2R 已读取 WEB_TOKEN，停止发送跳过按键",
            );
            token_read_logged = true;
        }
        if mutex_closed && !mutex_closed_logged {
            emit("mutex", "ok", "互斥句柄已清除");
            mutex_closed_logged = true;
        }
        launch_ready = token_and_mutex_are_ready(web_token_read, mutex_closed);
        if launch_ready || start.elapsed() >= timeout {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    if !launch_ready {
        let mutex_diagnostics = if mutex_closed {
            "已清除"
        } else if mutex_found_once.load(std::sync::atomic::Ordering::SeqCst) {
            "曾检测到但未能确认清除"
        } else {
            "未检测到"
        };
        let error = format!(
            "等待 Token 消费与互斥句柄清除超时：WEB_TOKEN {}，互斥句柄 {}，{}",
            if web_token_read {
                "已读取"
            } else {
                "未读取"
            },
            mutex_diagnostics,
            token_read_monitor.diagnostics()
        );
        emit("connect", "error", &error);
        emit("mutex", "error", &error);
        mutex_task.abort();
        intro_skip_task.abort();
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some(error),
            mutex_killed: mutex_closed,
        };
    }

    let _ = mutex_task.await;
    intro_skip_task.abort();
    if let Err(error) = token_read_monitor.stop() {
        crate::logger::log_msg("WARN", "Launch", &format!("[Account {account_id}] {error}"));
    }

    // ── Step 9: 优雅关闭战网 → 等待退出 → 回写最新状态 ──
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let graceful = crate::commands::system::graceful_kill_bnet(30);
    if !graceful {
        emit("cleanup", "warning", "战网未能优雅关闭，已回退强制关闭");
    }
    emit("cleanup", "running", "正在回写最新认证状态...");

    let account_dir = match checked_account_dir(config, account_id) {
        Ok(dir) => dir,
        Err(res) => return res,
    };
    let config_clone = config.clone();
    let account_dir_clone = account_dir.clone();
    let sync_res = tokio::task::spawn_blocking(move || {
        crate::commands::account::sync_back_to_account(&account_dir_clone, &config_clone)
    })
    .await;

    match sync_res {
        Ok(Ok(())) => {
            emit("cleanup", "ok", "战网已关闭，状态已同步");
        }
        Ok(Err(e)) => {
            emit("cleanup", "warning", &format!("回写状态失败: {}", e));
        }
        Err(_) => {
            emit("cleanup", "warning", "回写状态线程异常");
        }
    }

    // 更新最后启动时间
    let accounts_dir_clone = config.accounts_dir.clone();
    let account_id_clone = account_id.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(mut meta) = AccountManager::load_meta(&accounts_dir_clone, &account_id_clone) {
            meta.last_launched_at = Some(chrono::Utc::now().to_rfc3339());
            let _ = AccountManager::save_meta(&accounts_dir_clone, &meta);
        }
    })
    .await;

    emit("done", "ok", "启动完成");
    LaunchResult {
        account_id: account_id.to_string(),
        success: true,
        d2r_pid: Some(d2r_pid),
        error: None,
        mutex_killed: mutex_killed.load(std::sync::atomic::Ordering::SeqCst),
    }
}

async fn launch_single_token(
    app: &tauri::AppHandle,
    config: &crate::commands::global_config::GlobalConfig,
    state: &SharedState,
    account_id: &str,
    meta: &crate::commands::account::AccountMeta,
    context: &LaunchContext,
) -> LaunchResult {
    let emit = |step: &str, status: &str, msg: &str| {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!("[Account {}] [{}] [{}]: {}", account_id, step, status, msg),
        );
        let _ = app.emit(
            "launch-progress",
            LaunchProgress::new(account_id, step, status, msg),
        );
    };

    let cancelled = || -> LaunchResult {
        LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some("启动已被用户取消".to_string()),
            mutex_killed: false,
        }
    };

    emit("clean", "running", "正在清理环境变量...");
    if is_cancelled(state) {
        emit("done", "error", "已取消");
        return cancelled();
    }

    emit("copy", "running", "正在准备注册表与 Settings.json...");
    let account_dir = match checked_account_dir(config, account_id) {
        Ok(dir) => dir,
        Err(res) => return res,
    };

    // 1. 覆盖 Settings.json
    if meta.has_customized_settings {
        match context.installation.saved_games_directory.as_deref() {
            Some(saved_games_directory) => {
                if let Err(error) =
                    copy_account_settings_to_system(&account_dir, saved_games_directory)
                {
                    emit(
                        "copy",
                        "warning",
                        &format!("独立 Settings.json 覆盖失败，已继续使用系统配置: {error}"),
                    );
                }
            }
            None => emit(
                "copy",
                "warning",
                "未配置可用的存档目录，已跳过独立 Settings.json 覆盖",
            ),
        }
    } else {
        crate::logger::log_msg(
            "INFO",
            "Launch",
            &format!(
                "[Account {}] Token 启动使用系统 Settings.json，跳过账号画质配置覆盖",
                account_id
            ),
        );
    }

    // 2. 写入 Token 到注册表
    let protected_bytes = match &meta.token {
        Some(t) => {
            // Token 在 account.json 中以 hex(DPAPI加密结果) 形式存储，
            // 直接解码后写入注册表即可，D2R 会自行调用 CryptUnprotectData 解密
            match crate::commands::crypto::hex_decode(t) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return LaunchResult {
                        account_id: account_id.to_string(),
                        success: false,
                        d2r_pid: None,
                        error: Some(format!("Token 解码失败: {}", e)),
                        mutex_killed: false,
                    };
                }
            }
        }
        None => {
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("账号缺少 Token".to_string()),
                mutex_killed: false,
            };
        }
    };

    let token_registry_path = context.token_registry_path();
    let registry_result = (|| -> Result<(), AppError> {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(&token_registry_path).map_err(|error| {
            AppError::RegistryError(format!("打开 Token 注册表路径失败: {error}"))
        })?;
        key.set_value("REGION", &context.region.registry_region)
            .map_err(|error| AppError::RegistryError(format!("写入 REGION 失败: {error}")))?;
        let locale = meta
            .language
            .as_deref()
            .unwrap_or(context.region.default_locale);
        let audio = meta
            .voicelanguage
            .as_deref()
            .unwrap_or(context.region.default_locale);
        key.set_value("LOCALE", &locale)
            .map_err(|error| AppError::RegistryError(format!("写入 LOCALE 失败: {error}")))?;
        key.set_value("LOCALE_AUDIO", &audio)
            .map_err(|error| AppError::RegistryError(format!("写入 LOCALE_AUDIO 失败: {error}")))?;

        let val = winreg::RegValue {
            bytes: protected_bytes.clone(),
            vtype: RegType::REG_BINARY,
        };
        key.set_raw_value(WEB_TOKEN_VALUE_NAME, &val)
            .map_err(|error| {
                AppError::RegistryError(format!("写入 {WEB_TOKEN_VALUE_NAME} 失败: {error}"))
            })?;
        Ok(())
    })();
    if let Err(error) = registry_result {
        return account_path_error(account_id, error);
    }
    emit("copy", "ok", "配置覆盖完成");
    emit("connect", "running", "正在启动 WEB_TOKEN ETW 读取监听...");
    let token_read_monitor = match WebTokenReadMonitor::start() {
        Ok(monitor) => monitor,
        Err(error) => {
            emit("connect", "error", &error);
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some(error),
                mutex_killed: false,
            };
        }
    };

    // 3. 记录之前存在的 D2R 进程
    let before_pids = crate::commands::system::snapshot_processes("D2R.exe".to_string());

    emit("game", "running", "正在直接启动 D2R.exe...");
    // 4. 启动 D2R.exe
    let game_path = context.installation.game_executable.clone();
    let expected_game_path = game_path.clone();
    // The executable UID is edition-specific; the shared token registry key is not.
    let uid_arg = context.edition.token_auth_app;

    let mut cmd = Command::new(&game_path);
    cmd.current_dir(&context.installation.game_directory);
    cmd.arg("-uid").arg(uid_arg);

    if !meta.mod_args.is_empty() {
        match parse_windows_command_line(&meta.mod_args) {
            Ok(args) => {
                cmd.args(args);
            }
            Err(error) => {
                return account_path_error(account_id, AppError::ConfigReadError(error));
            }
        }
    }

    let spawn_res = tokio::task::spawn_blocking(move || cmd.spawn()).await;
    match spawn_res {
        Ok(Ok(_)) => {}
        _ => {
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("启动 D2R.exe 失败".to_string()),
                mutex_killed: false,
            };
        }
    }

    // 5. 等待进程
    let mut d2r_pid_opt: Option<u32> = None;
    let mut sys = sysinfo::System::new();
    let wait_start = std::time::Instant::now();
    let timeout_secs = 60;

    while wait_start.elapsed().as_secs() < timeout_secs {
        if is_cancelled(state) {
            return cancelled();
        }

        let monitored_game_path = expected_game_path.clone();
        let process_refresh = tokio::task::spawn_blocking(move || {
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
            let mut pids = Vec::new();
            for (pid, proc) in sys.processes() {
                if proc
                    .name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("D2R.exe")
                    && proc.exe().is_some_and(|actual| {
                        crate::commands::system::executable_paths_match(
                            actual,
                            &monitored_game_path,
                        )
                    })
                {
                    pids.push(pid.as_u32());
                }
            }
            (pids, sys)
        })
        .await;
        let (d2r_pids, sys_ret) = match process_refresh {
            Ok(result) => result,
            Err(error) => {
                return LaunchResult {
                    account_id: account_id.to_string(),
                    success: false,
                    d2r_pid: None,
                    error: Some(format!("刷新游戏进程列表失败: {error}")),
                    mutex_killed: false,
                };
            }
        };
        sys = sys_ret;

        for pid in &d2r_pids {
            if !before_pids.contains(pid) {
                d2r_pid_opt = Some(*pid);
                break;
            }
        }
        if d2r_pid_opt.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let d2r_pid = match d2r_pid_opt {
        Some(pid) => {
            emit("game", "ok", &format!("游戏进程已启动 (PID: {})", pid));
            state.record_active_game(account_id, pid, &meta.mod_args);
            crate::audio_mod::emit_runtime_compatibility_warning(
                app,
                state,
                config,
                meta,
                pid,
                &meta.mod_args,
            );

            let win_title = if meta.display_name.is_empty() {
                account_id.to_string()
            } else {
                meta.display_name.clone()
            };
            let win_x = meta.window_x;
            let win_y = meta.window_y;
            let pid_copy = pid;
            let separate_taskbar_icon = config.separate_game_taskbar_icons;
            let app_user_model_id = format!("D2RHub.Account.{account_id}");
            let app_for_window = app.clone();
            let account_id_for_window = account_id.to_string();
            tokio::task::spawn_blocking(move || {
                let mut taskbar_configured = !separate_taskbar_icon;
                let mut taskbar_error = None;
                for _ in 0..10 {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    crate::commands::system::rename_game_window(pid_copy, &win_title);
                    if !taskbar_configured {
                        match crate::commands::system::set_game_window_app_user_model_id(
                            pid_copy,
                            &app_user_model_id,
                        ) {
                            Ok(()) => taskbar_configured = true,
                            Err(error) => taskbar_error = Some(error),
                        }
                    }
                    if let (Some(x), Some(y)) = (win_x, win_y) {
                        crate::commands::system::set_game_window_position(pid_copy, x, y);
                    }
                }
                if !taskbar_configured {
                    let message = taskbar_error
                        .unwrap_or_else(|| "游戏窗口任务栏独立分组设置失败".to_string());
                    crate::logger::log_msg(
                        "WARN",
                        "Launch",
                        &format!("[Account {account_id_for_window}] {message}"),
                    );
                    let _ = app_for_window.emit(
                        "launch-progress",
                        LaunchProgress::new(&account_id_for_window, "window", "warning", &message),
                    );
                }
            });
            pid
        }
        None => {
            emit("game", "error", "等待游戏进程启动超时");
            return LaunchResult {
                account_id: account_id.to_string(),
                success: false,
                d2r_pid: None,
                error: Some("等待游戏进程启动超时".to_string()),
                mutex_killed: false,
            };
        }
    };

    // ── 杀 Mutex ──
    let mutex_state = std::sync::Arc::new(MutexRemovalState::default());
    let mutex_task = {
        let mutex_state = mutex_state.clone();
        tokio::spawn(async move {
            let mut closed_at_least_once = false;
            for _ in 0..120 {
                match crate::commands::system::find_mutex_handle(d2r_pid, MUTEX_NAME) {
                    Ok(Some(hid)) => {
                        mutex_state.record_found();
                        match crate::commands::system::close_handle(d2r_pid, &hid) {
                            Ok(()) => {
                                closed_at_least_once = true;
                                match crate::commands::system::find_mutex_handle(
                                    d2r_pid, MUTEX_NAME,
                                ) {
                                    Ok(None) => {
                                        mutex_state.confirm_closed();
                                        break;
                                    }
                                    Ok(Some(_)) => {
                                        mutex_state.record_error("关闭后仍检测到互斥句柄");
                                    }
                                    Err(error) => {
                                        mutex_state
                                            .record_error(format!("确认互斥句柄清除失败: {error}"));
                                    }
                                }
                            }
                            Err(error) => {
                                mutex_state.record_error(error.to_string());
                            }
                        }
                    }
                    Ok(None) if closed_at_least_once => {
                        mutex_state.confirm_closed();
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        mutex_state.record_error(error.to_string());
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        })
    };

    // 在 ETW 确认目标 D2R 已消费 WEB_TOKEN 前持续跳过动画。停止条件由实际 Token
    // 消费事件驱动，避免用固定次数或固定时长猜测游戏初始化进度。
    let intro_skip_task = tokio::spawn(async move {
        loop {
            let _ = crate::commands::system::send_keys_to_window(d2r_pid);
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    });

    // ── ETW WEB_TOKEN 读取检测 ──
    emit("connect", "running", "正在等待 D2R 读取 WEB_TOKEN...");
    emit("mutex", "running", "后台监控互斥句柄中...");

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(60);
    let mut web_token_read;
    let mut mutex_closed;
    let mut launch_ready;
    let mut token_read_logged = false;
    let mut mutex_closed_logged = false;

    loop {
        if is_cancelled(state) {
            mutex_task.abort();
            intro_skip_task.abort();
            return cancelled();
        }

        web_token_read = token_read_monitor.was_read_by(d2r_pid);
        mutex_closed = mutex_state.is_closed();
        if web_token_read && !token_read_logged {
            intro_skip_task.abort();
            emit("connect", "ok", "检测到 D2R 已读取 WEB_TOKEN");
            token_read_logged = true;
        }
        if mutex_closed && !mutex_closed_logged {
            emit("mutex", "ok", "互斥句柄已清除");
            mutex_closed_logged = true;
        }
        launch_ready = token_and_mutex_are_ready(web_token_read, mutex_closed);
        if launch_ready || start.elapsed() >= timeout {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    if !launch_ready {
        let error = format!(
            "等待 Token 消费与互斥句柄清除超时：WEB_TOKEN {}，互斥句柄 {}，{}",
            if web_token_read {
                "已读取"
            } else {
                "未读取"
            },
            mutex_state.diagnostics(),
            token_read_monitor.diagnostics()
        );
        emit("connect", "error", &error);
        emit("mutex", "error", &error);
        mutex_task.abort();
        intro_skip_task.abort();
        return LaunchResult {
            account_id: account_id.to_string(),
            success: false,
            d2r_pid: None,
            error: Some(error),
            mutex_killed: false,
        };
    }
    let _ = mutex_task.await;
    if let Err(error) = token_read_monitor.stop() {
        crate::logger::log_msg("WARN", "Launch", &format!("[Account {account_id}] {error}"));
    }

    // 更新最后启动时间
    let accounts_dir_clone = config.accounts_dir.clone();
    let account_id_clone = account_id.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(mut meta) = AccountManager::load_meta(&accounts_dir_clone, &account_id_clone) {
            meta.last_launched_at = Some(chrono::Utc::now().to_rfc3339());
            let _ = AccountManager::save_meta(&accounts_dir_clone, &meta);
        }
    })
    .await;

    emit("done", "ok", "启动完成");
    LaunchResult {
        account_id: account_id.to_string(),
        success: true,
        d2r_pid: Some(d2r_pid),
        error: None,
        mutex_killed: mutex_state.is_closed(),
    }
}

// ── 工具函数 ──

/// Parse a Windows command-line fragment into arguments without losing quoted spaces.
/// Implements the backslash-before-quote rules used by the Microsoft C runtime.
pub(crate) fn parse_windows_command_line(input: &str) -> Result<Vec<String>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut args = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index == chars.len() {
            break;
        }

        let mut argument = String::new();
        let mut in_quotes = false;
        let mut started = false;
        while index < chars.len() {
            let current = chars[index];
            if current.is_whitespace() && !in_quotes {
                break;
            }
            if current == '\\' {
                let slash_start = index;
                while index < chars.len() && chars[index] == '\\' {
                    index += 1;
                }
                let slash_count = index - slash_start;
                if index < chars.len() && chars[index] == '"' {
                    argument.extend(std::iter::repeat_n('\\', slash_count / 2));
                    if slash_count % 2 == 0 {
                        in_quotes = !in_quotes;
                    } else {
                        argument.push('"');
                    }
                    started = true;
                    index += 1;
                } else {
                    argument.extend(std::iter::repeat_n('\\', slash_count));
                    started = true;
                }
                continue;
            }
            if current == '"' {
                in_quotes = !in_quotes;
                started = true;
                index += 1;
                continue;
            }
            argument.push(current);
            started = true;
            index += 1;
        }

        if in_quotes {
            return Err("Mod 启动参数包含未闭合的双引号".to_string());
        }
        if started {
            args.push(argument);
        }
    }

    Ok(args)
}

/// 解码 .reg 注册表文件内容为 String。
/// Windows regedit 导出默认 UTF-16LE（BOM 0xFF 0xFE），也兼容 UTF-8（含或不含 BOM）。
/// 返回 None 表示文件编码无法识别或解码失败——调用方应拒绝导入（Fail-Safe）。
fn decode_reg_file(raw: &[u8]) -> Option<String> {
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        // UTF-16LE with BOM
        let u16_bytes = &raw[2..];
        if !u16_bytes.len().is_multiple_of(2) {
            return None;
        }
        let u16_words: Vec<u16> = u16_bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&u16_words).ok()
    } else if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        // UTF-8 with BOM
        String::from_utf8(raw[3..].to_vec()).ok()
    } else {
        // Assume UTF-8 without BOM (or plain ASCII)
        String::from_utf8(raw.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        battle_net_launch_argument, launch_queue_can_continue, parse_windows_command_line,
        persist_window_position, preflight_accounts, replace_bnet_roaming_snapshot,
        token_and_mutex_are_ready, unique_account_window_executable, validate_launch_account_ids,
        validate_legacy_reg_sections,
    };
    use crate::commands::account::{AccountManager, AccountMeta};
    use crate::commands::global_config::GlobalConfig;
    use crate::launch_context::ContextPurpose;
    use crate::state::{AccountLifecycleLease, AppState};

    #[test]
    fn token_read_and_closed_mutex_are_both_required_for_every_launch_mode() {
        assert!(!token_and_mutex_are_ready(false, false));
        assert!(!token_and_mutex_are_ready(false, true));
        assert!(!token_and_mutex_are_ready(true, false));
        assert!(token_and_mutex_are_ready(true, true));
    }

    #[test]
    fn failed_or_incomplete_launch_stops_the_account_queue() {
        assert!(!launch_queue_can_continue(false, false));
        assert!(!launch_queue_can_continue(false, true));
        assert!(!launch_queue_can_continue(true, false));
        assert!(launch_queue_can_continue(true, true));
    }

    #[test]
    fn battle_net_launch_argument_quotes_only_the_exec_value() {
        assert_eq!(battle_net_launch_argument("OSI"), r#"--exec="launch OSI""#);
    }

    #[test]
    fn launch_batch_rejects_uuid_case_aliases() {
        let state = std::sync::Arc::new(AppState::new());
        let account_ids = vec![
            "ABCDEF01-2345-6789-ABCD-EF0123456789".to_string(),
            "abcdef01-2345-6789-abcd-ef0123456789".to_string(),
        ];

        assert!(validate_launch_account_ids(&account_ids).is_err());
        assert!(state.account_operations.lock().is_empty());
    }

    #[test]
    fn launch_batch_validation_does_not_reserve_account_lifecycle() {
        let state = std::sync::Arc::new(AppState::new());
        let account_ids = vec!["acount1".to_string(), "acount2".to_string()];

        validate_launch_account_ids(&account_ids).unwrap();
        assert!(state.account_operations.lock().is_empty());

        for account_id in account_ids {
            let lease = AccountLifecycleLease::try_acquire(&state, &account_id).unwrap();
            assert!(state
                .account_operations
                .lock()
                .contains(&account_id.to_ascii_lowercase()));
            drop(lease);
        }
        assert!(state.account_operations.lock().is_empty());
    }

    #[test]
    fn window_position_retries_after_account_lease_is_released() {
        let root = temp_dir("window_position_retry");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let state = std::sync::Arc::new(AppState::new());
        let blocking_lease = AccountLifecycleLease::try_acquire(&state, "acount1").unwrap();

        assert!(!persist_window_position(
            &state,
            &config.accounts_dir,
            "acount1",
            (120, 240),
        ));
        drop(blocking_lease);
        assert!(persist_window_position(
            &state,
            &config.accounts_dir,
            "acount1",
            (120, 240),
        ));

        let meta = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        assert_eq!((meta.window_x, meta.window_y), (Some(120), Some(240)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_window_identities_are_not_assigned_to_an_account() {
        let root = temp_dir("duplicate_window_identity");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let first = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        assert!(unique_account_window_executable(&config, &first).is_some());

        save_account(&config, "acount2", "token", Some("00"));
        for id in ["acount1", "acount2"] {
            let mut meta = AccountManager::load_meta(&config.accounts_dir, id).unwrap();
            meta.display_name = "同名账号".to_string();
            AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();
        }
        let first = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        assert!(unique_account_window_executable(&config, &first).is_none());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn windows_mod_arguments_preserve_quoted_spaces() {
        assert_eq!(
            parse_windows_command_line(r#"-mod "My Mod" -txt"#).unwrap(),
            vec!["-mod", "My Mod", "-txt"]
        );
    }

    #[test]
    fn windows_mod_arguments_preserve_escaped_quotes_and_backslashes() {
        assert_eq!(
            parse_windows_command_line(r#"--label "say \"hello\"" C:\Mods\D2RMM"#).unwrap(),
            vec!["--label", "say \"hello\"", r"C:\Mods\D2RMM"]
        );
    }

    #[test]
    fn windows_mod_arguments_reject_unclosed_quotes() {
        assert!(parse_windows_command_line(r#"-mod "unfinished"#).is_err());
    }

    #[test]
    fn legacy_reg_accepts_only_the_unified_auth_section() {
        let content = concat!(
            "Windows Registry Editor Version 5.00\n\n",
            "[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Battle.net\\UnifiedAuth]\n",
            "\"US\"=hex:01,02\n",
            "[hkey_current_user\\software\\blizzard entertainment\\battle.NET\\unifiedauth]\n",
            "\"EU\"=hex:03,04\n"
        );
        assert!(validate_legacy_reg_sections(content).is_ok());
    }

    #[test]
    fn legacy_reg_rejects_any_additional_or_nested_section() {
        let additional = concat!(
            "[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Battle.net\\UnifiedAuth]\n",
            "[HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run]\n"
        );
        let nested =
            "[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Battle.net\\UnifiedAuth\\Child]\n";
        assert!(validate_legacy_reg_sections(additional).is_err());
        assert!(validate_legacy_reg_sections(nested).is_err());
        assert!(validate_legacy_reg_sections("Windows Registry Editor Version 5.00").is_err());
    }

    #[test]
    fn replacing_bnet_snapshot_removes_stale_target_files() {
        let root = temp_dir("replace_bnet");
        let source = root.join("source");
        let target = root.join("target");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(source.join("new.json"), b"new").unwrap();
        std::fs::write(target.join("stale.json"), b"stale").unwrap();

        replace_bnet_roaming_snapshot(&source, &target).unwrap();

        assert!(target.join("new.json").is_file());
        assert!(!target.join("stale.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replacing_bnet_snapshot_repairs_a_non_directory_target() {
        let root = temp_dir("replace_bnet_repairs_target");
        let source = root.join("source");
        let target = root.join("target");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("new.json"), b"new").unwrap();
        std::fs::write(&target, b"not a directory").unwrap();

        replace_bnet_roaming_snapshot(&source, &target).unwrap();

        assert!(target.is_dir());
        assert_eq!(std::fs::read(target.join("new.json")).unwrap(), b"new");
        assert!(!root.join("target.tmp").exists());
        assert!(!root.join("target.bak").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replacing_bnet_snapshot_recovers_an_interrupted_host_swap() {
        let root = temp_dir("replace_bnet_recovers_host");
        let source = root.join("source");
        let target = root.join("target");
        let staged = root.join("target.tmp");
        let backup = root.join("target.bak");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(source.join("new.json"), b"new").unwrap();
        std::fs::write(staged.join("partial.json"), b"partial").unwrap();
        std::fs::write(backup.join("old.json"), b"old").unwrap();

        replace_bnet_roaming_snapshot(&source, &target).unwrap();

        assert_eq!(std::fs::read(target.join("new.json")).unwrap(), b"new");
        assert!(!target.join("old.json").exists());
        assert!(!staged.exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "d2rhub_launch_preflight_{name}_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn configure_global_install(root: &std::path::Path, with_bnet: bool) -> GlobalConfig {
        let game = root.join("game");
        let saves = root.join("saves");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(game.join("D2R.exe"), b"test").unwrap();

        let cn_battle_net_path = if with_bnet {
            let bnet = root.join("Battle.net.exe");
            std::fs::write(&bnet, b"test").unwrap();
            bnet.to_string_lossy().to_string()
        } else {
            String::new()
        };
        GlobalConfig {
            accounts_dir: root.join("accounts").to_string_lossy().to_string(),
            cn_battle_net_path,
            cn_game_path: if with_bnet {
                game.to_string_lossy().to_string()
            } else {
                String::new()
            },
            cn_saved_games_path: if with_bnet {
                saves.to_string_lossy().to_string()
            } else {
                String::new()
            },
            global_game_path: if with_bnet {
                String::new()
            } else {
                game.to_string_lossy().to_string()
            },
            global_saved_games_path: if with_bnet {
                String::new()
            } else {
                saves.to_string_lossy().to_string()
            },
            ..GlobalConfig::default()
        }
    }

    fn save_account(config: &GlobalConfig, id: &str, auth_mode: &str, token: Option<&str>) {
        let mut meta = AccountMeta::new(id);
        meta.region = Some(if auth_mode == "bnet" { "CN" } else { "NA" }.to_string());
        meta.auth_mode = Some(auth_mode.to_string());
        meta.token = token.map(str::to_string);
        meta.initialized = true;
        meta.last_reset_at = Some(chrono::Utc::now().to_rfc3339());
        let account_dir = AccountManager::account_dir_checked(&config.accounts_dir, id).unwrap();
        std::fs::create_dir_all(account_dir).unwrap();
        AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();
    }

    #[test]
    fn batch_preflight_rejects_a_later_account_before_launch_side_effects() {
        let root = temp_dir("token_batch");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        save_account(&config, "acount2", "token", None);

        let result = preflight_accounts(
            &config,
            &["acount1".to_string(), "acount2".to_string()],
            ContextPurpose::LaunchGame,
        );
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn batch_preflight_allows_missing_customized_settings() {
        let root = temp_dir("missing_custom_settings");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let mut meta = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        meta.has_customized_settings = true;
        AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();

        let result = preflight_accounts(
            &config,
            &["acount1".to_string()],
            ContextPurpose::LaunchGame,
        );

        assert!(result.is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn batch_preflight_rejects_invalid_token_mod_arguments() {
        let root = temp_dir("invalid_mod_args");
        let config = configure_global_install(&root, false);
        save_account(&config, "acount1", "token", Some("00"));
        let mut meta = AccountManager::load_meta(&config.accounts_dir, "acount1").unwrap();
        meta.mod_args = r#"-mod "unterminated"#.to_string();
        AccountManager::save_meta(&config.accounts_dir, &meta).unwrap();

        let error = preflight_accounts(
            &config,
            &["acount1".to_string()],
            ContextPurpose::LaunchGame,
        )
        .unwrap_err();

        assert!(error.to_string().contains("Mod 启动参数无效"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bnet_preflight_rejects_an_empty_auth_snapshot() {
        let root = temp_dir("bnet_snapshot");
        let config = configure_global_install(&root, true);
        save_account(&config, "acount3", "bnet", None);
        let account_dir =
            AccountManager::account_dir_checked(&config.accounts_dir, "acount3").unwrap();
        let bnet_dir = account_dir.join("Battle.net");
        std::fs::create_dir_all(&bnet_dir).unwrap();
        std::fs::write(bnet_dir.join("Battle.net.config"), b"{}").unwrap();
        std::fs::write(account_dir.join("unified_auth.json"), b"[]").unwrap();
        std::fs::write(
            account_dir.join("unified_auth.reg"),
            "[HKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Battle.net\\UnifiedAuth]\n\"US\"=hex:01",
        )
        .unwrap();

        assert!(preflight_accounts(
            &config,
            &["acount3".to_string()],
            ContextPurpose::LaunchGame,
        )
        .is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
