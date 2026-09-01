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

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AccountConfigurationError {
    #[error("不支持的账号游戏区服: {0}")]
    UnsupportedRegion(String),
    #[error("不支持的认证方式: {0}")]
    UnsupportedAuthMode(String),
    #[error("只有 Token 直启账号可以切换国际服服务器")]
    RegionSwitchRequiresToken,
    #[error("账号缺少区服，无法切换服务器")]
    MissingRegion,
    #[error("只有国际服账号可以在亚服、美服和欧服之间切换")]
    RegionSwitchRequiresGlobalEdition,
    #[error("不支持的国际服服务器: {0}")]
    UnsupportedInternationalRegion(String),
    #[error("账号名称不能为空")]
    EmptyDisplayName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameRegion {
    Cn,
    Asia,
    Americas,
    Europe,
}

impl GameRegion {
    pub fn parse(raw: &str) -> Result<Self, AccountConfigurationError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "CN" => Ok(Self::Cn),
            "KR" | "GLOBAL" | "ASIA" => Ok(Self::Asia),
            "NA" | "US" | "AMERICAS" => Ok(Self::Americas),
            "EU" | "EUROPE" => Ok(Self::Europe),
            _ => Err(AccountConfigurationError::UnsupportedRegion(
                raw.to_string(),
            )),
        }
    }

    pub fn canonical(self) -> &'static str {
        match self {
            Self::Cn => "CN",
            Self::Asia => "KR",
            Self::Americas => "NA",
            Self::Europe => "EU",
        }
    }

    pub fn edition(self) -> ClientEdition {
        match self {
            Self::Cn => ClientEdition::Cn,
            Self::Asia | Self::Americas | Self::Europe => ClientEdition::Global,
        }
    }

    pub fn registry_region(self) -> &'static str {
        match self {
            Self::Cn => "CN",
            Self::Asia => "KR",
            Self::Americas => "US",
            Self::Europe => "EU",
        }
    }

    pub fn default_locale(self) -> &'static str {
        match self {
            Self::Cn => "zhCN",
            Self::Asia => "zhTW",
            Self::Americas | Self::Europe => "enUS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientEdition {
    Cn,
    Global,
}

impl ClientEdition {
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Cn => "CN",
            Self::Global => "Global",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Cn => "国服",
            Self::Global => "国际服",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    BattleNet,
    Token,
}

impl AuthMode {
    pub fn parse(raw: Option<&str>) -> Result<Self, AccountConfigurationError> {
        match raw.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("bnet") => Ok(Self::BattleNet),
            Some("token") => Ok(Self::Token),
            Some(mode) => Err(AccountConfigurationError::UnsupportedAuthMode(
                mode.to_string(),
            )),
        }
    }

    pub fn canonical(self) -> &'static str {
        match self {
            Self::BattleNet => "bnet",
            Self::Token => "token",
        }
    }
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

    /// Switches between international regions without disturbing the account's
    /// token, locale, settings, runtime snapshot, or initialized state.
    pub fn switch_international_region(
        &mut self,
        requested_region: &str,
    ) -> Result<(), AccountConfigurationError> {
        if AuthMode::parse(self.auth_mode.as_deref())? != AuthMode::Token {
            return Err(AccountConfigurationError::RegionSwitchRequiresToken);
        }
        let current_region = self
            .region
            .as_deref()
            .ok_or(AccountConfigurationError::MissingRegion)?;
        if GameRegion::parse(current_region)?.edition() != ClientEdition::Global {
            return Err(AccountConfigurationError::RegionSwitchRequiresGlobalEdition);
        }

        let requested_region = requested_region.trim().to_ascii_uppercase();
        if !matches!(requested_region.as_str(), "KR" | "NA" | "EU") {
            return Err(AccountConfigurationError::UnsupportedInternationalRegion(
                requested_region,
            ));
        }
        self.region = Some(
            GameRegion::parse(&requested_region)?
                .canonical()
                .to_string(),
        );
        Ok(())
    }

    /// Replaces the complete Mod configuration set while keeping first-seen
    /// order and guaranteeing that the active configuration is represented.
    pub fn replace_mod_configurations(&mut self, active_mod: String, mod_list: Vec<String>) {
        let active_mod = active_mod.trim().to_string();
        let mut normalized =
            Vec::with_capacity(mod_list.len() + usize::from(!active_mod.is_empty()));
        for configuration in mod_list {
            append_unique_mod_configuration(&mut normalized, &configuration);
        }
        if !active_mod.is_empty() {
            append_unique_mod_configuration(&mut normalized, &active_mod);
        }
        self.mod_args = active_mod;
        self.mod_list = normalized;
    }
}

fn append_unique_mod_configuration(mod_list: &mut Vec<String>, candidate: &str) -> bool {
    let candidate = candidate.trim();
    if candidate.is_empty() || mod_list.iter().any(|existing| existing.trim() == candidate) {
        return false;
    }
    mod_list.push(candidate.to_string());
    true
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

pub fn normalize_account_display_name(name: &str) -> String {
    name.trim().to_lowercase()
}

pub fn validate_account_display_name(name: &str) -> Result<String, AccountConfigurationError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AccountConfigurationError::EmptyDisplayName);
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        is_valid_account_id, normalize_account_display_name, validate_account_display_name,
        AccountMeta, AccountPositionError, AuthMode, ClientEdition, GameRegion,
        WindowPositionPreset,
    };

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
    fn account_display_names_trim_for_storage_and_fold_for_identity() {
        assert_eq!(
            validate_account_display_name("  Primary  ").unwrap(),
            "Primary"
        );
        assert_eq!(normalize_account_display_name("  Primary  "), "primary");
        assert!(validate_account_display_name("   ").is_err());
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

    #[test]
    fn account_regions_and_authentication_have_stable_canonical_values() {
        assert_eq!(AuthMode::parse(None).unwrap(), AuthMode::BattleNet);
        assert_eq!(AuthMode::parse(Some("token")).unwrap(), AuthMode::Token);
        assert_eq!(GameRegion::parse("Global").unwrap(), GameRegion::Asia);
        assert_eq!(GameRegion::parse("US").unwrap().canonical(), "NA");
        assert_eq!(
            GameRegion::parse("EU").unwrap().edition(),
            ClientEdition::Global
        );
        assert_eq!(GameRegion::Americas.registry_region(), "US");
        assert_eq!(GameRegion::Asia.default_locale(), "zhTW");
    }

    #[test]
    fn international_switch_preserves_non_region_account_configuration() {
        let mut account = AccountMeta::new("account1");
        account.auth_mode = Some("token".to_string());
        account.region = Some("Global".to_string());
        account.token = Some("encrypted-token".to_string());
        account.language = Some("zhTW".to_string());
        account.voicelanguage = Some("enUS".to_string());
        account.has_customized_settings = true;
        account.snapshot_edition = Some("Global".to_string());
        account.initialized = true;

        account.switch_international_region("EU").unwrap();

        assert_eq!(account.region.as_deref(), Some("EU"));
        assert_eq!(account.token.as_deref(), Some("encrypted-token"));
        assert_eq!(account.language.as_deref(), Some("zhTW"));
        assert_eq!(account.voicelanguage.as_deref(), Some("enUS"));
        assert!(account.has_customized_settings);
        assert_eq!(account.snapshot_edition.as_deref(), Some("Global"));
        assert!(account.initialized);
    }

    #[test]
    fn international_switch_rejects_cn_battle_net_and_cn_targets() {
        let mut cn = AccountMeta::new("cn");
        cn.auth_mode = Some("token".to_string());
        cn.region = Some("CN".to_string());
        assert!(cn.switch_international_region("EU").is_err());

        let mut battle_net = AccountMeta::new("bnet");
        battle_net.auth_mode = Some("bnet".to_string());
        battle_net.region = Some("EU".to_string());
        assert!(battle_net.switch_international_region("NA").is_err());

        let mut invalid_target = AccountMeta::new("invalid");
        invalid_target.auth_mode = Some("token".to_string());
        invalid_target.region = Some("KR".to_string());
        assert!(invalid_target.switch_international_region("CN").is_err());
    }

    #[test]
    fn mod_configurations_are_trimmed_deduplicated_and_keep_the_active_value() {
        let mut account = AccountMeta::new("acount1");
        account.replace_mod_configurations(
            " -mod highres -txt ".to_string(),
            vec![
                "-mod highres -txt".to_string(),
                " -mod highres -txt ".to_string(),
                "-direct -txt".to_string(),
            ],
        );

        assert_eq!(account.mod_args, "-mod highres -txt");
        assert_eq!(account.mod_list, ["-mod highres -txt", "-direct -txt"]);
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
