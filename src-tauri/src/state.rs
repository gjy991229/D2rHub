use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::application::capability::CapabilityRegistry;
use crate::application::configuration::ConfigurationRuntime;
use crate::application::multi_instance::{AccountOperationLease, MultiInstanceRuntime};
use crate::application::task_runtime::TaskRuntime;
use crate::error::AppError;

/// 应用全局运行时状态
pub struct AppState {
    /// 全局配置事务运行时。所有读取都取得不可变快照，写入只能经仓储与策略端口提交。
    configuration: ConfigurationRuntime,
    /// 可选模块生命周期的纯应用层控制面。
    capabilities: Arc<CapabilityRegistry>,
    /// 应用数据目录路径
    pub app_data_dir: String,
    /// 多开核心运行时。账号实例与操作取消只能通过其公开接口访问，避免模块直接操作锁。
    multi_instance: MultiInstanceRuntime,
    /// Unified status, cancellation and timeline registry for long-running work.
    tasks: TaskRuntime,
    /// Battle.net 目录、注册表和 Agent 都是主机级共享状态，同一时刻只能由一个流程修改。
    /// 该租约必须在产生任何进程、文件或注册表副作用之前取得。
    pub host_runtime_busy: AtomicBool,
    /// 首次披露确认后的运行服务启动锁，确保并发 IPC 也只会激活一次。
    pub(crate) runtime_activation_lock: Mutex<()>,
    /// 高风险运行服务是否已经完成激活。
    pub(crate) runtime_activated: AtomicBool,
    /// 本进程内已经逻辑删除的稳定账号 ID。配置策略用它阻止排队中的陈旧
    /// 全量保存重新引入已删除账号；不扫描目录，避免与账号目录替换窗口竞争。
    retired_account_ids: RwLock<HashSet<String>>,
    /// 同一时间只允许一个 Mod 加工任务，避免两个生成器写入同一个 mods 目录。
    pub audio_mod_build_busy: AtomicBool,
    /// 快捷键内存映射缓存：lowercase_shortcut -> account_position (1-based)
    pub shortcut_map: RwLock<HashMap<String, usize>>,
    /// 串行化窗口位置文件的迁移和写入，避免多个 WebView 同时读改写导致配置丢失。
    pub window_placement_io: Mutex<()>,
}

impl AppState {
    pub fn new() -> Self {
        #[cfg(not(test))]
        let app_data = resolve_app_data_dir();
        // 单元测试不得触碰真实用户的 AppData，也不能触发旧版目录迁移。
        #[cfg(test)]
        let app_data =
            std::env::temp_dir().join(format!("d2rhub_state_test_{}", std::process::id()));

        Self::with_app_data_dir(app_data)
    }

    fn with_app_data_dir(app_data: PathBuf) -> Self {
        Self {
            configuration: ConfigurationRuntime::new(),
            capabilities: Arc::new(CapabilityRegistry::new()),
            app_data_dir: app_data.to_string_lossy().to_string(),
            multi_instance: MultiInstanceRuntime::default(),
            tasks: TaskRuntime::default(),
            host_runtime_busy: AtomicBool::new(false),
            runtime_activation_lock: Mutex::new(()),
            runtime_activated: AtomicBool::new(false),
            retired_account_ids: RwLock::new(HashSet::new()),
            audio_mod_build_busy: AtomicBool::new(false),
            shortcut_map: RwLock::new(HashMap::new()),
            window_placement_io: Mutex::new(()),
        }
    }

    pub fn configuration(&self) -> &ConfigurationRuntime {
        &self.configuration
    }

    pub fn capabilities(&self) -> &Arc<CapabilityRegistry> {
        &self.capabilities
    }

    pub fn multi_instance(&self) -> &MultiInstanceRuntime {
        &self.multi_instance
    }

    pub fn tasks(&self) -> &TaskRuntime {
        &self.tasks
    }

    pub fn retire_account_id(&self, account_id: &str) {
        self.retired_account_ids
            .write()
            .insert(account_id.to_ascii_lowercase());
    }

    pub fn retired_account_ids_snapshot(&self) -> Vec<String> {
        self.retired_account_ids.read().iter().cloned().collect()
    }
}

/// D2RHub 的固定用户配置目录。
/// Windows 下 `dirs::config_dir()` 对应 `%APPDATA%`，符合普通桌面软件的配置存放习惯。
#[cfg(not(test))]
fn system_app_data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .join("D2RHub")
}

#[cfg(not(test))]
fn legacy_portable_config_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("config")))
        .unwrap_or_else(|| PathBuf::from("./config"))
}

fn copy_directory_recursive(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory_recursive(&source_path, &target_path)?;
        } else {
            std::fs::copy(source_path, target_path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyConfigMigrationOutcome {
    NotNeeded,
    Migrated,
    /// 两边都含用户配置，不能静默覆盖或合并。
    Conflict,
}

fn directory_has_entries(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_some()
}

fn directory_has_account_data(path: &Path) -> bool {
    let accounts = path.join("accounts");
    std::fs::read_dir(accounts).ok().is_some_and(|entries| {
        entries.flatten().any(|entry| {
            entry.file_type().ok().is_some_and(|kind| kind.is_dir())
                && entry.path().join("account.json").is_file()
        })
    })
}

const LEGACY_MIGRATION_MARKER: &str = ".legacy-portable-config-migrated";

fn normalized_path_identity(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

fn migration_marker_matches_source(source: &Path, target: &Path) -> bool {
    std::fs::read_to_string(target.join(LEGACY_MIGRATION_MARKER))
        .ok()
        .is_some_and(|recorded| recorded.trim() == normalized_path_identity(source))
}

fn write_migration_marker(source_identity: &str, target: &Path) -> Result<(), String> {
    std::fs::write(target.join(LEGACY_MIGRATION_MARKER), source_identity)
        .map_err(|error| format!("写入旧版配置迁移标记失败: {error}"))
}

fn config_file_has_user_data(path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str(&content) else {
        return false;
    };
    // 与 GlobalConfig::load 的完成条件保持一致：没有游戏目录的配置即使错误地
    // 标记 first_run_complete=true，也仍是未完成配置，不能挡住便携版数据迁移。
    ["game_path", "cn_game_path", "global_game_path"]
        .iter()
        .any(|key| {
            object
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        })
}

fn directory_global_config_has_user_data(path: &Path) -> bool {
    [
        "global_config.json",
        "global_config.json.bak",
        "global_config.json.tmp",
    ]
    .iter()
    .any(|name| config_file_has_user_data(&path.join(name)))
}

fn directory_has_statistics_data(path: &Path) -> bool {
    // `data.db` is the pre-stateData layout and remains a supported migration
    // source. Any entry below stateData can include the database, screenshots,
    // or exported statistics and must therefore be treated as user data.
    path.join("data.db").is_file() || directory_has_entries(&path.join("stateData"))
}

fn directory_has_user_config(path: &Path) -> bool {
    directory_global_config_has_user_data(path)
        || directory_has_account_data(path)
        || directory_has_statistics_data(path)
}

fn unique_migration_backup_path(target: &Path) -> Result<PathBuf, String> {
    let parent = target
        .parent()
        .ok_or_else(|| format!("系统配置目录缺少父目录: {}", target.display()))?;
    let stem = format!(".D2RHub.pre-migration-{}", std::process::id());
    for suffix in 0..1000 {
        let name = if suffix == 0 {
            stem.clone()
        } else {
            format!("{stem}-{suffix}")
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("无法为现有系统配置分配迁移备份目录".to_string())
}

fn install_legacy_config_dir(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!("旧版配置目录不存在: {}", source.display()));
    }

    let parent = target
        .parent()
        .ok_or_else(|| format!("系统配置目录缺少父目录: {}", target.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("创建系统配置父目录 {} 失败: {error}", parent.display()))?;

    match std::fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            // exe 与 AppData 可能位于不同磁盘；跨卷时先完整复制到同卷暂存目录，再原子安装。
            let staging = parent.join(format!(".D2RHub.migration-{}.tmp", std::process::id()));
            if staging.exists() {
                std::fs::remove_dir_all(&staging).map_err(|error| {
                    format!("清理配置迁移暂存目录 {} 失败: {error}", staging.display())
                })?;
            }
            copy_directory_recursive(source, &staging).map_err(|error| {
                let _ = std::fs::remove_dir_all(&staging);
                format!(
                    "复制旧配置 {} 到系统目录失败（原始移动错误: {rename_error}）: {error}",
                    source.display()
                )
            })?;
            std::fs::rename(&staging, target).map_err(|error| {
                let _ = std::fs::remove_dir_all(&staging);
                format!(
                    "安装迁移后的系统配置目录 {} 失败: {error}",
                    target.display()
                )
            })?;

            // 目标已完整安装后再删除旧目录；删除失败不影响后续始终读取系统目录。
            if let Err(error) = std::fs::remove_dir_all(source) {
                crate::logger::log_msg(
                    "WARN",
                    "Config",
                    &format!(
                        "配置已迁移到 {}，但旧目录 {} 未能删除: {error}",
                        target.display(),
                        source.display()
                    ),
                );
            }
            Ok(())
        }
    }
}

fn replace_target_with_legacy_config(
    source: &Path,
    target: &Path,
    source_identity: &str,
) -> Result<(), String> {
    let target_was_empty = !directory_has_entries(target);
    let backup = unique_migration_backup_path(target)?;
    std::fs::rename(target, &backup)
        .map_err(|error| format!("备份现有系统配置目录 {} 失败: {error}", target.display()))?;

    if let Err(error) = install_legacy_config_dir(source, target) {
        if target.exists() {
            let _ = std::fs::remove_dir_all(target);
        }
        std::fs::rename(&backup, target).map_err(|restore_error| {
            format!(
                "{error}；恢复原系统配置目录 {} 也失败: {restore_error}",
                target.display()
            )
        })?;
        return Err(error);
    }

    if let Err(error) = write_migration_marker(source_identity, target) {
        crate::logger::log_msg("WARN", "Config", &error);
    }

    if target_was_empty {
        let _ = std::fs::remove_dir(&backup);
    } else {
        crate::logger::log_msg(
            "WARN",
            "Config",
            &format!("原系统配置已备份到 {}，旧版配置已接管", backup.display()),
        );
    }
    Ok(())
}

/// 将旧版 exe 同目录 `config` 搬到系统配置目录。
///
/// 空目录或无法识别的半成品目标不能遮蔽完整旧配置；替换前会先留下可恢复备份。
/// 两边都含用户配置时保留两边并报告冲突，绝不静默合并。
fn migrate_legacy_config_dir(
    source: &Path,
    target: &Path,
) -> Result<LegacyConfigMigrationOutcome, String> {
    if target.exists() && !target.is_dir() {
        return Err(format!(
            "系统配置路径已存在但不是目录: {}",
            target.display()
        ));
    }

    if !source.is_dir() || !directory_has_entries(source) {
        if !target.exists() {
            std::fs::create_dir_all(target)
                .map_err(|error| format!("创建系统配置目录 {} 失败: {error}", target.display()))?;
        }
        return Ok(LegacyConfigMigrationOutcome::NotNeeded);
    }

    let source_identity = normalized_path_identity(source);
    if target.is_dir() && migration_marker_matches_source(source, target) {
        return Ok(LegacyConfigMigrationOutcome::NotNeeded);
    }

    if !target.exists() {
        install_legacy_config_dir(source, target)?;
        if let Err(error) = write_migration_marker(&source_identity, target) {
            crate::logger::log_msg("WARN", "Config", &error);
        }
        return Ok(LegacyConfigMigrationOutcome::Migrated);
    }

    let source_has_accounts = directory_has_account_data(source);
    let target_has_accounts = directory_has_account_data(target);

    // 修复已经进入混乱状态的安装：系统目录只有新建的全局配置、没有任何账号，
    // 而便携目录仍保有账号时，应让便携目录接管。原系统目录会完整备份；迁移标记
    // 可防止用户日后主动删除全部账号后，残留旧目录再次把账号复活。
    if source_has_accounts && !target_has_accounts {
        replace_target_with_legacy_config(source, target, &source_identity)?;
        return Ok(LegacyConfigMigrationOutcome::Migrated);
    }

    if directory_has_user_config(target) {
        return Ok(if directory_has_user_config(source) {
            LegacyConfigMigrationOutcome::Conflict
        } else {
            LegacyConfigMigrationOutcome::NotNeeded
        });
    }

    replace_target_with_legacy_config(source, target, &source_identity)?;
    Ok(LegacyConfigMigrationOutcome::Migrated)
}

#[cfg(not(test))]
fn resolve_app_data_dir() -> PathBuf {
    let target = system_app_data_dir();
    let source = legacy_portable_config_dir();
    match migrate_legacy_config_dir(&source, &target) {
        Ok(LegacyConfigMigrationOutcome::Migrated) => crate::logger::log_msg(
            "INFO",
            "Config",
            &format!(
                "旧版配置已从 {} 搬迁到 {}",
                source.display(),
                target.display()
            ),
        ),
        Ok(LegacyConfigMigrationOutcome::Conflict) => crate::logger::log_msg(
            "WARN",
            "Config",
            &format!(
                "系统配置 {} 与旧版配置 {} 均包含用户数据；继续使用系统配置并保留旧目录，请勿手动覆盖",
                target.display(),
                source.display()
            ),
        ),
        Ok(LegacyConfigMigrationOutcome::NotNeeded) => {}
        Err(error) => {
            crate::logger::log_msg(
                "ERROR",
                "Config",
                &format!("{error}；本次继续使用旧配置目录并在下次启动重试迁移"),
            );
            // 迁移失败时绝不能用空的系统目录遮蔽旧数据。保留旧目录作为本次兼容
            // 回退，下一次启动仍会再次尝试搬迁。
            if source.is_dir() {
                return source;
            }
        }
    }
    target
}

pub struct AccountLifecycleLease {
    _lease: AccountOperationLease,
}

impl AccountLifecycleLease {
    pub fn try_acquire(state: &Arc<AppState>, account_id: &str) -> Result<Self, AppError> {
        state
            .multi_instance()
            .account_leases()
            .try_acquire(account_id)
            .map(|lease| Self { _lease: lease })
    }
}

pub type SharedState = Arc<AppState>;

#[cfg(test)]
mod tests {
    use super::{
        migrate_legacy_config_dir, AccountLifecycleLease, AppState, LegacyConfigMigrationOutcome,
    };

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "d2rhub_state_{name}_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn account_lifecycle_lease_is_scoped_per_account_and_released_on_drop() {
        let state = std::sync::Arc::new(AppState::new());
        let first = AccountLifecycleLease::try_acquire(&state, "acount1").unwrap();
        assert!(AccountLifecycleLease::try_acquire(&state, "acount1").is_err());
        let second = AccountLifecycleLease::try_acquire(&state, "acount2").unwrap();
        drop(second);
        drop(first);
        assert!(AccountLifecycleLease::try_acquire(&state, "acount1").is_ok());
    }

    #[test]
    fn account_lifecycle_lease_normalizes_uuid_case_aliases() {
        let state = std::sync::Arc::new(AppState::new());
        let uppercase = "ABCDEF01-2345-6789-ABCD-EF0123456789";
        let lowercase = "abcdef01-2345-6789-abcd-ef0123456789";

        let lease = AccountLifecycleLease::try_acquire(&state, uppercase).unwrap();
        assert!(AccountLifecycleLease::try_acquire(&state, lowercase).is_err());
        drop(lease);
        assert!(AccountLifecycleLease::try_acquire(&state, lowercase).is_ok());
    }

    #[test]
    fn legacy_config_is_moved_when_system_directory_is_absent() {
        let root = temp_dir("migration");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(source.join("accounts").join("acount1")).unwrap();
        std::fs::write(source.join("global_config.json"), "{}").unwrap();
        std::fs::write(
            source.join("accounts").join("acount1").join("account.json"),
            "{}",
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Migrated
        );
        assert!(!source.exists());
        assert!(target.join("global_config.json").is_file());
        assert!(target
            .join("accounts")
            .join("acount1")
            .join("account.json")
            .is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn two_populated_config_directories_report_a_conflict_without_overwriting() {
        let root = temp_dir("system_wins");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            source.join("global_config.json"),
            r#"{"version":1,"first_run_complete":true,"game_path":"D:\\OldD2R"}"#,
        )
        .unwrap();
        std::fs::write(
            target.join("global_config.json"),
            r#"{"version":6,"first_run_complete":true,"cn_game_path":"C:\\CurrentD2R"}"#,
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Conflict
        );
        assert_eq!(
            std::fs::read_to_string(target.join("global_config.json")).unwrap(),
            r#"{"version":6,"first_run_complete":true,"cn_game_path":"C:\\CurrentD2R"}"#
        );
        assert!(source.join("global_config.json").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn portable_statistics_are_user_data_when_system_config_already_exists() {
        let root = temp_dir("portable_statistics_conflict");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(source.join("stateData")).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(source.join("stateData").join("data.db"), "portable stats").unwrap();
        std::fs::write(
            target.join("global_config.json"),
            r#"{"version":9,"first_run_complete":true,"cn_game_path":"C:\\CurrentD2R"}"#,
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Conflict
        );
        assert_eq!(
            std::fs::read_to_string(source.join("stateData").join("data.db")).unwrap(),
            "portable stats"
        );
        assert!(target.join("global_config.json").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn empty_system_directory_does_not_hide_legacy_config() {
        let root = temp_dir("empty_system_directory");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            source.join("global_config.json"),
            r#"{"version":1,"first_run_complete":true}"#,
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Migrated
        );
        assert!(!source.exists());
        assert_eq!(
            std::fs::read_to_string(target.join("global_config.json")).unwrap(),
            r#"{"version":1,"first_run_complete":true}"#
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_system_directory_is_backed_up_before_legacy_config_takes_over() {
        let root = temp_dir("incomplete_system_directory");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            source.join("global_config.json"),
            r#"{"version":1,"first_run_complete":true}"#,
        )
        .unwrap();
        std::fs::write(
            target.join("global_config.json"),
            r#"{"version":6,"first_run_complete":false}"#,
        )
        .unwrap();
        std::fs::write(target.join("interrupted.tmp"), "recoverable").unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Migrated
        );
        let parent = target.parent().unwrap();
        let backup = std::fs::read_dir(parent)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".D2RHub.pre-migration-"))
            })
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(backup.join("interrupted.tmp")).unwrap(),
            "recoverable"
        );
        assert!(target.join("global_config.json").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_completed_marker_without_game_paths_does_not_block_portable_config() {
        let root = temp_dir("invalid_completed_marker");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            source.join("global_config.json"),
            r#"{"version":3,"first_run_complete":true,"global_game_path":"D:\\D2R"}"#,
        )
        .unwrap();
        std::fs::write(
            target.join("global_config.json"),
            r#"{"version":6,"first_run_complete":true,"cn_game_path":"","global_game_path":""}"#,
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Migrated
        );
        assert!(target.join("global_config.json").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn browser_only_initial_config_does_not_block_portable_config() {
        let root = temp_dir("browser_only_initial_config");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            source.join("global_config.json"),
            r#"{"version":3,"first_run_complete":true,"global_game_path":"D:\\D2R"}"#,
        )
        .unwrap();
        std::fs::write(
            target.join("global_config.json"),
            r#"{"version":6,"first_run_complete":false,"browser_path":"C:\\Browser\\browser.exe"}"#,
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Migrated
        );
        assert!(target.join("global_config.json").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn portable_accounts_recover_an_already_configured_but_accountless_system_dir() {
        let root = temp_dir("recover_accountless_system_dir");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(source.join("accounts").join("acount1")).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            source.join("global_config.json"),
            r#"{"version":6,"first_run_complete":true,"cn_game_path":"D:\\PortableD2R"}"#,
        )
        .unwrap();
        std::fs::write(
            source.join("accounts").join("acount1").join("account.json"),
            r#"{"id":"acount1","display_name":"portable"}"#,
        )
        .unwrap();
        std::fs::write(
            target.join("global_config.json"),
            r#"{"version":6,"first_run_complete":true,"cn_game_path":"C:\\NewD2R"}"#,
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Migrated
        );
        assert!(target
            .join("accounts")
            .join("acount1")
            .join("account.json")
            .is_file());
        assert!(target.join(super::LEGACY_MIGRATION_MARKER).is_file());
        assert!(target
            .parent()
            .unwrap()
            .read_dir()
            .unwrap()
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".D2RHub.pre-migration-")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn migration_marker_prevents_deleted_accounts_from_being_resurrected() {
        let root = temp_dir("marker_prevents_resurrection");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(source.join("accounts").join("acount1")).unwrap();
        std::fs::write(source.join("global_config.json"), "{}").unwrap();
        std::fs::write(
            source.join("accounts").join("acount1").join("account.json"),
            r#"{"id":"acount1"}"#,
        )
        .unwrap();
        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Migrated
        );

        std::fs::remove_dir_all(target.join("accounts")).unwrap();
        std::fs::create_dir_all(source.join("accounts").join("acount1")).unwrap();
        std::fs::write(source.join("global_config.json"), "{}").unwrap();
        std::fs::write(
            source.join("accounts").join("acount1").join("account.json"),
            r#"{"id":"acount1"}"#,
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::NotNeeded
        );
        assert!(!target.join("accounts").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn valid_system_backup_prevents_an_older_portable_config_from_taking_over() {
        let root = temp_dir("system_backup_is_user_data");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            source.join("global_config.json"),
            r#"{"version":1,"first_run_complete":true,"game_path":"D:\\Old"}"#,
        )
        .unwrap();
        std::fs::write(target.join("global_config.json"), "{broken").unwrap();
        std::fs::write(
            target.join("global_config.json.bak"),
            r#"{"version":6,"first_run_complete":true,"cn_game_path":"C:\\Current"}"#,
        )
        .unwrap();

        assert_eq!(
            migrate_legacy_config_dir(&source, &target).unwrap(),
            LegacyConfigMigrationOutcome::Conflict
        );
        assert_eq!(
            std::fs::read_to_string(target.join("global_config.json")).unwrap(),
            "{broken"
        );
        assert!(target.join("global_config.json.bak").is_file());
        assert!(source.join("global_config.json").is_file());
        let _ = std::fs::remove_dir_all(root);
    }
}
