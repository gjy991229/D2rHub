use serde::{Deserialize, Serialize};

/// 账号级窗口位置胶囊。`window_x/window_y` 仍作为兼容旧版本的默认位置镜像保留。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPositionPreset {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
}

/// Stable account metadata persisted in `accounts/{id}/account.json`.
///
/// Runtime fields remain in the serialized shape for backward compatibility. The account query
/// application service overwrites them from the live instance registry before returning a list to
/// the frontend, and all frontend projections remove `token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountMeta {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub mod_args: String,
    #[serde(default)]
    pub mod_list: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_launched_at: Option<String>,
    #[serde(default)]
    pub initialized: bool,
    /// 最后一次初始化/重置的时间（用于 token 有效期计算）
    #[serde(default)]
    pub last_reset_at: Option<String>,
    #[serde(default)]
    pub order: u32,
    #[serde(default)]
    pub is_running: bool,
    /// 当前运行中的 D2R 进程 PID（None = 未运行）
    #[serde(default)]
    pub running_pid: Option<u32>,
    /// 游戏窗口目标 X 坐标（None = 不调整位置）
    #[serde(default)]
    pub window_x: Option<i32>,
    /// 游戏窗口目标 Y 坐标（None = 不调整位置）
    #[serde(default)]
    pub window_y: Option<i32>,
    /// 账号级窗口位置胶囊库。旧配置缺少该字段时，会由 window_x/window_y 补全。
    #[serde(default)]
    pub position_presets: Vec<WindowPositionPreset>,
    /// 主界面默认选择的位置胶囊；None 表示不指定窗口位置。
    #[serde(default)]
    pub active_position_id: Option<String>,
    /// 认证模式 ("bnet" 或 "token")
    #[serde(default)]
    pub auth_mode: Option<String>,
    /// Token 认证的区服 ("CN" 或 "Global")
    #[serde(default)]
    pub region: Option<String>,
    /// DPAPI 加密后的 Token 密文十六进制；任何前端 IPC 响应都会移除此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// 最近一次 Battle.net/UnifiedAuth 快照所属客户端版本（CN / Global）。
    #[serde(default)]
    pub snapshot_edition: Option<String>,
    /// 是否已自定义过设置。
    #[serde(default)]
    pub has_customized_settings: bool,
    /// 界面语言 ("zhCN" / "zhTW" / "enUS"，默认取决于区服)
    #[serde(default)]
    pub language: Option<String>,
    /// 配音语言 ("zhCN" / "zhTW" / "enUS"，默认取决于区服)
    #[serde(default)]
    pub voicelanguage: Option<String>,
}

impl AccountMeta {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            display_name: String::new(),
            mod_args: String::new(),
            mod_list: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            last_launched_at: None,
            initialized: false,
            last_reset_at: None,
            order: 0,
            is_running: false,
            running_pid: None,
            window_x: None,
            window_y: None,
            position_presets: Vec::new(),
            active_position_id: None,
            auth_mode: None,
            region: None,
            token: None,
            has_customized_settings: false,
            snapshot_edition: None,
            language: None,
            voicelanguage: None,
        }
    }
}

/// Historical account identifiers include the misspelled `acountN` form and UUIDs.
/// Both remain valid so existing account directories continue to load without migration.
pub fn is_valid_account_id(id: &str) -> bool {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('\\')
        || id.contains('/')
        || id.contains(':')
    {
        return false;
    }

    if let Some(rest) = id.strip_prefix("acount") {
        return !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit());
    }

    let parts = id.split('-').collect::<Vec<_>>();
    if parts.len() == 5 {
        let expected = [8, 4, 4, 4, 12];
        return parts.iter().zip(expected).all(|(part, length)| {
            part.len() == length && part.chars().all(|character| character.is_ascii_hexdigit())
        });
    }

    false
}

#[cfg(test)]
mod tests {
    use super::is_valid_account_id;

    #[test]
    fn historical_and_uuid_identifiers_remain_supported() {
        assert!(is_valid_account_id("acount1"));
        assert!(is_valid_account_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(is_valid_account_id("550E8400-E29B-41D4-A716-446655440000"));
    }

    #[test]
    fn path_like_and_partial_identifiers_are_rejected() {
        for invalid in [
            "",
            ".",
            "..",
            "acount",
            "acountx",
            "../acount1",
            "C:account",
        ] {
            assert!(
                !is_valid_account_id(invalid),
                "unexpectedly accepted {invalid}"
            );
        }
    }
}
