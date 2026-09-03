use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error, Deserialize, Clone)]
pub enum AppError {
    #[error("需要管理员权限才能执行此操作，请以管理员身份重新运行")]
    NeedAdmin,

    #[error("战网客户端路径无效: {0}")]
    InvalidBnetPath(String),

    #[error("游戏路径无效: {0}")]
    InvalidGamePath(String),

    #[error("战网启动超时 ({0}秒)")]
    BnetLaunchTimeout(u64),

    #[error("游戏进程未能在 {0} 秒内启动")]
    GameLaunchTimeout(u64),

    #[error("未能在 {0} 秒内检测到战网登录")]
    LoginTimeout(u64),

    #[error("句柄清除失败，已重试 {0} 次")]
    MutexClearFailed(u32),

    #[error("游戏连接服务器超时")]
    ServerConnectTimeout,

    #[error("启动已被用户取消")]
    LaunchCancelled,

    #[error("账号不存在: {0}")]
    AccountNotFound(String),

    #[error("账号已存在: {0}")]
    AccountAlreadyExists(String),

    #[error("账号未完成初始化")]
    AccountNotInitialized(String),

    #[error("配置读取失败: {0}")]
    ConfigReadError(String),

    #[error("配置写入失败: {0}")]
    ConfigWriteError(String),

    #[error("文件操作失败: {0}")]
    FileError(String),

    #[error("注册表操作失败: {0}")]
    RegistryError(String),

    #[error("IO 错误: {0}")]
    IoError(String),

    #[error("序列化/反序列化失败: {0}")]
    SerdeError(String),

    #[error("未知错误: {0}")]
    Unknown(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::IoError(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::SerdeError(e.to_string())
    }
}

impl From<crate::domain::account::AccountConfigurationError> for AppError {
    fn from(error: crate::domain::account::AccountConfigurationError) -> Self {
        AppError::ConfigReadError(error.to_string())
    }
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
