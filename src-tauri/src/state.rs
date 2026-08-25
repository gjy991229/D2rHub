use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
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
        // 使用 exe 同目录下的 config 文件夹，而非系统 AppData
        let app_data = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|parent| parent.join("config")))
            .unwrap_or_else(|| std::path::PathBuf::from("./config"));

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
    use super::{AccountLifecycleLease, AppState};

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
}
