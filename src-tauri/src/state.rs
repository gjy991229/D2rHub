use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

use crate::commands::global_config::GlobalConfig;
use crate::error::AppError;

/// 应用全局运行时状态
pub struct AppState {
    /// 全局配置（线程安全读写）
    pub config: RwLock<Option<GlobalConfig>>,
    /// 应用数据目录路径
    pub app_data_dir: String,
    /// 启动取消标志（前端点停止时置 true，启动循环检测到后中止）
    pub cancel_launch: AtomicBool,
    /// 每次取消递增；长事务通过启动代次识别属于自己的取消请求。
    pub cancel_generation: AtomicU64,
    /// Battle.net 目录、注册表和 Agent 都是主机级共享状态，同一时刻只能由一个流程修改。
    /// 该租约必须在产生任何进程、文件或注册表副作用之前取得。
    pub host_runtime_busy: AtomicBool,
    /// 账号目录及 account.json 的生命周期写操作；同一账号同一时刻只能有一个事务。
    pub account_operations: Mutex<HashSet<String>>,
    /// 正在运行的账号游戏 PID 映射：account_id -> d2r_pid
    pub active_games: RwLock<HashMap<String, u32>>,
    /// 快捷键内存映射缓存：lowercase_shortcut -> account_position (1-based)
    pub shortcut_map: RwLock<HashMap<String, usize>>,
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
            config: RwLock::new(None),
            app_data_dir: app_data.to_string_lossy().to_string(),
            cancel_launch: AtomicBool::new(false),
            cancel_generation: AtomicU64::new(0),
            host_runtime_busy: AtomicBool::new(false),
            account_operations: Mutex::new(HashSet::new()),
            active_games: RwLock::new(HashMap::new()),
            shortcut_map: RwLock::new(HashMap::new()),
        }
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

/// 仅在系统目录尚不存在、旧版 exe 同目录 `config` 存在时执行一次搬迁。
/// 系统目录一旦存在便始终优先，避免升级时把新配置与旧配置静默合并。
fn migrate_legacy_config_dir(source: &Path, target: &Path) -> Result<bool, String> {
    if target.exists() {
        return if target.is_dir() {
            Ok(false)
        } else {
            Err(format!(
                "系统配置路径已存在但不是目录: {}",
                target.display()
            ))
        };
    }
    if !source.is_dir() {
        std::fs::create_dir_all(target)
            .map_err(|error| format!("创建系统配置目录 {} 失败: {error}", target.display()))?;
        return Ok(false);
    }

    let parent = target
        .parent()
        .ok_or_else(|| format!("系统配置目录缺少父目录: {}", target.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("创建系统配置父目录 {} 失败: {error}", parent.display()))?;

    match std::fs::rename(source, target) {
        Ok(()) => return Ok(true),
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
            Ok(true)
        }
    }
}

#[cfg(not(test))]
fn resolve_app_data_dir() -> PathBuf {
    let target = system_app_data_dir();
    let source = legacy_portable_config_dir();
    match migrate_legacy_config_dir(&source, &target) {
        Ok(true) => crate::logger::log_msg(
            "INFO",
            "Config",
            &format!(
                "旧版配置已从 {} 搬迁到 {}",
                source.display(),
                target.display()
            ),
        ),
        Ok(false) => {}
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
    state: Arc<AppState>,
    account_id: String,
}

impl AccountLifecycleLease {
    pub fn try_acquire(state: &Arc<AppState>, account_id: &str) -> Result<Self, AppError> {
        // Windows 文件系统通常大小写不敏感；UUID 大小写别名必须映射到同一把租约。
        let operation_key = account_id.to_ascii_lowercase();
        let mut active = state.account_operations.lock();
        if !active.insert(operation_key.clone()) {
            return Err(AppError::Unknown(format!(
                "账号 {account_id} 正在执行另一项操作，请稍后重试"
            )));
        }
        drop(active);
        Ok(Self {
            state: Arc::clone(state),
            account_id: operation_key,
        })
    }
}

impl Drop for AccountLifecycleLease {
    fn drop(&mut self) {
        self.state
            .account_operations
            .lock()
            .remove(&self.account_id);
    }
}

pub type SharedState = Arc<AppState>;

#[cfg(test)]
mod tests {
    use super::{migrate_legacy_config_dir, AccountLifecycleLease, AppState};

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

        assert!(migrate_legacy_config_dir(&source, &target).unwrap());
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
    fn existing_system_config_always_wins_over_legacy_directory() {
        let root = temp_dir("system_wins");
        let source = root.join("portable").join("config");
        let target = root.join("AppData").join("D2RHub");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(source.join("global_config.json"), "legacy").unwrap();
        std::fs::write(target.join("global_config.json"), "system").unwrap();

        assert!(!migrate_legacy_config_dir(&source, &target).unwrap());
        assert_eq!(
            std::fs::read_to_string(target.join("global_config.json")).unwrap(),
            "system"
        );
        assert!(source.join("global_config.json").is_file());
        let _ = std::fs::remove_dir_all(root);
    }
}
