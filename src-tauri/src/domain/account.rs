use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

/// 账号级窗口位置胶囊。`window_x/window_y` 仍作为兼容旧版本的默认位置镜像保留。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPositionPreset {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AccountPositionError {
    #[error("位置名称和唯一标识不能为空")]
    EmptyIdentity,
    #[error("位置唯一标识重复: {0}")]
    DuplicateId(String),
    #[error("位置名称重复: {0}")]
    DuplicateName(String),
    #[error("所选位置不存在: {0}")]
    MissingSelection(String),
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

    /// Reconciles legacy `window_x/window_y` with the named position model.
    ///
    /// The legacy coordinates remain the compatibility authority while older
    /// D2RHub versions are supported. Loading the same metadata repeatedly is
    /// idempotent and never creates duplicate synthetic positions.
    pub fn normalize_legacy_window_position(&mut self) {
        let mut seen_ids = HashSet::new();
        self.position_presets.retain_mut(|preset| {
            preset.id = preset.id.trim().to_string();
            preset.name = preset.name.trim().to_string();
            !preset.id.is_empty() && !preset.name.is_empty() && seen_ids.insert(preset.id.clone())
        });

        let Some((x, y)) = self.window_x.zip(self.window_y) else {
            self.active_position_id = None;
            return;
        };

        if let Some(active_id) = self.active_position_id.as_deref() {
            if self
                .position_presets
                .iter()
                .any(|preset| preset.id == active_id && preset.x == x && preset.y == y)
            {
                return;
            }
        }

        if let Some(existing) = self
            .position_presets
            .iter()
            .find(|preset| preset.x == x && preset.y == y)
        {
            self.active_position_id = Some(existing.id.clone());
            return;
        }

        let id = next_legacy_position_id(&self.position_presets);
        let name = next_legacy_position_name(&self.position_presets);
        self.position_presets.push(WindowPositionPreset {
            id: id.clone(),
            name,
            x,
            y,
        });
        self.active_position_id = Some(id);
    }

    /// Updates the legacy coordinate pair and keeps the named position mirror
    /// compatible with historical callers.
    pub fn set_legacy_window_position(&mut self, window_x: Option<i32>, window_y: Option<i32>) {
        self.window_x = window_x;
        self.window_y = window_y;
        let Some((x, y)) = window_x.zip(window_y) else {
            self.active_position_id = None;
            return;
        };

        if let Some(active_id) = self.active_position_id.clone() {
            if let Some(active) = self
                .position_presets
                .iter_mut()
                .find(|preset| preset.id == active_id)
            {
                active.x = x;
                active.y = y;
            } else {
                self.active_position_id = None;
            }
        }

        if self.active_position_id.is_some() {
            return;
        }
        if let Some(existing) = self
            .position_presets
            .iter()
            .find(|preset| preset.x == x && preset.y == y)
        {
            self.active_position_id = Some(existing.id.clone());
            return;
        }

        let id = next_legacy_position_id(&self.position_presets);
        let name = next_legacy_position_name(&self.position_presets);
        self.position_presets.push(WindowPositionPreset {
            id: id.clone(),
            name,
            x,
            y,
        });
        self.active_position_id = Some(id);
    }

    /// Replaces the named position set atomically after complete validation and
    /// updates the legacy coordinate mirror from the selected position.
    pub fn replace_position_presets(
        &mut self,
        active_position_id: Option<String>,
        position_presets: Vec<WindowPositionPreset>,
    ) -> Result<(), AccountPositionError> {
        let mut normalized = Vec::with_capacity(position_presets.len());
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for mut preset in position_presets {
            preset.id = preset.id.trim().to_string();
            preset.name = preset.name.trim().to_string();
            if preset.id.is_empty() || preset.name.is_empty() {
                return Err(AccountPositionError::EmptyIdentity);
            }
            if !ids.insert(preset.id.clone()) {
                return Err(AccountPositionError::DuplicateId(preset.id));
            }
            if !names.insert(preset.name.to_lowercase()) {
                return Err(AccountPositionError::DuplicateName(preset.name));
            }
            normalized.push(preset);
        }

        let active_position_id = active_position_id
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        let selected = active_position_id
            .as_deref()
            .map(|id| {
                normalized
                    .iter()
                    .find(|preset| preset.id == id)
                    .ok_or_else(|| AccountPositionError::MissingSelection(id.to_string()))
            })
            .transpose()?;

        self.window_x = selected.map(|preset| preset.x);
        self.window_y = selected.map(|preset| preset.y);
        self.position_presets = normalized;
        self.active_position_id = active_position_id;
        Ok(())
    }
}

fn next_legacy_position_id(presets: &[WindowPositionPreset]) -> String {
    next_unique_label(presets, "legacy-window-position", "-", |preset| {
        preset.id.as_str()
    })
}

fn next_legacy_position_name(presets: &[WindowPositionPreset]) -> String {
    next_unique_label(presets, "原位置", " ", |preset| preset.name.as_str())
}

fn next_unique_label<'a>(
    presets: &'a [WindowPositionPreset],
    base: &str,
    separator: &str,
    value: impl Fn(&'a WindowPositionPreset) -> &'a str,
) -> String {
    if !presets.iter().any(|preset| value(preset) == base) {
        return base.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}{separator}{suffix}");
        if !presets.iter().any(|preset| value(preset) == candidate) {
            return candidate;
        }
        suffix += 1;
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
    use super::{is_valid_account_id, AccountMeta, AccountPositionError, WindowPositionPreset};

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

    #[test]
    fn legacy_coordinates_create_one_idempotent_named_position() {
        let mut account = AccountMeta::new("acount1");
        account.window_x = Some(120);
        account.window_y = Some(240);

        account.normalize_legacy_window_position();
        account.normalize_legacy_window_position();

        assert_eq!(account.position_presets.len(), 1);
        assert_eq!(account.position_presets[0].id, "legacy-window-position");
        assert_eq!(account.position_presets[0].name, "原位置");
        assert_eq!(
            account.active_position_id.as_deref(),
            Some("legacy-window-position")
        );
    }

    #[test]
    fn legacy_update_changes_the_active_named_position_in_place() {
        let mut account = AccountMeta::new("acount1");
        account.position_presets = vec![position("left", "Left", 0, 0)];
        account.active_position_id = Some("left".to_string());

        account.set_legacy_window_position(Some(40), Some(80));

        assert_eq!((account.window_x, account.window_y), (Some(40), Some(80)));
        assert_eq!(
            (account.position_presets[0].x, account.position_presets[0].y),
            (40, 80)
        );
        assert_eq!(account.active_position_id.as_deref(), Some("left"));
    }

    #[test]
    fn replacing_positions_trims_values_and_updates_the_legacy_mirror() {
        let mut account = AccountMeta::new("acount1");

        account
            .replace_position_presets(
                Some(" right ".to_string()),
                vec![position(" right ", " Right Side ", 900, 20)],
            )
            .unwrap();

        assert_eq!(account.active_position_id.as_deref(), Some("right"));
        assert_eq!(account.position_presets[0].id, "right");
        assert_eq!(account.position_presets[0].name, "Right Side");
        assert_eq!((account.window_x, account.window_y), (Some(900), Some(20)));
    }

    #[test]
    fn invalid_replacement_does_not_mutate_the_account() {
        let mut account = AccountMeta::new("acount1");
        account.position_presets = vec![position("left", "Left", 0, 0)];
        account.active_position_id = Some("left".to_string());
        account.window_x = Some(0);
        account.window_y = Some(0);
        let original = account.clone();

        let error = account
            .replace_position_presets(
                None,
                vec![position("one", "Same", 1, 1), position("two", "same", 2, 2)],
            )
            .unwrap_err();

        assert_eq!(
            error,
            AccountPositionError::DuplicateName("same".to_string())
        );
        assert_eq!(account.position_presets, original.position_presets);
        assert_eq!(account.active_position_id, original.active_position_id);
        assert_eq!((account.window_x, account.window_y), (Some(0), Some(0)));
    }

    fn position(id: &str, name: &str, x: i32, y: i32) -> WindowPositionPreset {
        WindowPositionPreset {
            id: id.to_string(),
            name: name.to_string(),
            x,
            y,
        }
    }
}
