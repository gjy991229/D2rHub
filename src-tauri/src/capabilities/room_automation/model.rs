use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

pub const CURRENT_STRATEGY_VERSION: u8 = 17;
pub const MAX_ROOM_TEXT_LENGTH: usize = 15;

const DEFAULT_STANDARD_STEP_DELAY_MS: u64 = 100;
const DEFAULT_CHARACTER_DELAY_MS: u64 = 25;
const MAX_STEP_DELAY_MS: u64 = 2_000;
const MIN_CHARACTER_DELAY_MS: u64 = 10;
const MAX_CHARACTER_DELAY_MS: u64 = 250;
const MIN_AUTO_FOLLOWERS_DELAY_SECS: u64 = 2;
const MAX_AUTO_FOLLOWERS_DELAY_SECS: u64 = 60;
const MIN_SEQUENCE_WIDTH: u8 = 1;
const MAX_SEQUENCE_WIDTH: u8 = 6;

fn default_standard_step_delay_ms() -> u64 {
    DEFAULT_STANDARD_STEP_DELAY_MS
}

fn default_character_delay_ms() -> u64 {
    DEFAULT_CHARACTER_DELAY_MS
}

/// Keyboard pacing for one room-form workflow profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlowStrategy {
    #[serde(default = "default_standard_step_delay_ms")]
    pub step_delay_ms: u64,
    #[serde(default = "default_character_delay_ms")]
    pub character_delay_ms: u64,
}

impl FlowStrategy {
    pub fn standard() -> Self {
        Self {
            step_delay_ms: DEFAULT_STANDARD_STEP_DELAY_MS,
            character_delay_ms: DEFAULT_CHARACTER_DELAY_MS,
        }
    }

    fn normalize(&mut self) {
        self.step_delay_ms = self.step_delay_ms.min(MAX_STEP_DELAY_MS);
        self.character_delay_ms = self
            .character_delay_ms
            .clamp(MIN_CHARACTER_DELAY_MS, MAX_CHARACTER_DELAY_MS);
    }

    fn validate(&self, profile: &'static str) -> Result<(), RoomAutomationConfigError> {
        if self.step_delay_ms > MAX_STEP_DELAY_MS
            || !(MIN_CHARACTER_DELAY_MS..=MAX_CHARACTER_DELAY_MS).contains(&self.character_delay_ms)
        {
            return Err(RoomAutomationConfigError::InvalidFlowStrategy {
                profile,
                step_delay_ms: self.step_delay_ms,
                character_delay_ms: self.character_delay_ms,
            });
        }
        Ok(())
    }
}

impl Default for FlowStrategy {
    fn default() -> Self {
        Self::standard()
    }
}

fn default_auto_followers_delay_secs() -> u64 {
    5
}

fn default_primary_shortcut() -> String {
    "Ctrl+Alt+R".to_string()
}

fn default_followers_shortcut() -> String {
    "Ctrl+Alt+J".to_string()
}

fn default_name_prefix() -> String {
    "run-".to_string()
}

fn default_next_sequence() -> u32 {
    1
}

fn default_sequence_width() -> u8 {
    3
}

fn default_background_text_strategy() -> String {
    "post_keys".to_string()
}

fn default_standard_flow() -> FlowStrategy {
    FlowStrategy::standard()
}

/// Persisted configuration for room automation.
///
/// Legacy field aliases import the `room_rotation` object used through
/// strategy v16; v17 persists one unified keyboard flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomAutomationConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Explicit consent only. Legacy imports must never infer this value from
    /// `enabled`, because patching character key files is a separate mutation.
    #[serde(default)]
    pub chat_f13_auto_patch_enabled: bool,
    #[serde(default)]
    pub primary_account_id: String,
    #[serde(default)]
    pub follower_account_ids: Vec<String>,
    #[serde(default)]
    pub auto_followers_enabled: bool,
    #[serde(default = "default_auto_followers_delay_secs")]
    pub auto_followers_delay_secs: u64,
    #[serde(default = "default_primary_shortcut")]
    pub shortcut: String,
    #[serde(default = "default_followers_shortcut")]
    pub join_shortcut: String,
    #[serde(default = "default_name_prefix")]
    pub name_prefix: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_next_sequence")]
    pub next_sequence: u32,
    #[serde(default = "default_sequence_width")]
    pub sequence_width: u8,
    #[serde(default = "default_background_text_strategy")]
    pub background_text_strategy: String,
    /// Zero means the original unversioned representation.
    #[serde(default)]
    pub strategy_version: u8,
    /// One keyboard delivery path is used for every participant. The alias
    /// imports the former standard profile from v0-v16 sidecars; obsolete
    /// direct-lobby profiles and per-account bindings are intentionally ignored.
    #[serde(default = "default_standard_flow", alias = "standard_flow")]
    pub flow: FlowStrategy,
}

impl Default for RoomAutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chat_f13_auto_patch_enabled: false,
            primary_account_id: String::new(),
            follower_account_ids: Vec::new(),
            auto_followers_enabled: false,
            auto_followers_delay_secs: default_auto_followers_delay_secs(),
            shortcut: default_primary_shortcut(),
            join_shortcut: default_followers_shortcut(),
            name_prefix: default_name_prefix(),
            password: String::new(),
            next_sequence: default_next_sequence(),
            sequence_width: default_sequence_width(),
            background_text_strategy: default_background_text_strategy(),
            strategy_version: CURRENT_STRATEGY_VERSION,
            flow: default_standard_flow(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationReport {
    pub source_strategy_version: u8,
    pub target_strategy_version: u8,
    pub changed: bool,
    /// A v0-v12 enabled configuration used to be treated as implicit consent.
    /// The new adapter can use this flag to ask once instead of mutating files.
    pub requires_chat_binding_consent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShortcutValidationError {
    #[error("shortcut is empty")]
    Empty,
    #[error("shortcut contains an empty component")]
    EmptyComponent,
    #[error("shortcut repeats modifier {0}")]
    DuplicateModifier(String),
    #[error("shortcut contains more than one non-modifier key")]
    MultipleKeys,
    #[error("shortcut does not contain a non-modifier key")]
    MissingKey,
    #[error("shortcut uses unsupported key {0}")]
    UnsupportedKey(String),
    #[error("Win/Meta/Cmd shortcuts are not supported")]
    UnsupportedSystemModifier,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoomAutomationConfigError {
    #[error("room automation strategy v{found} is newer than supported v{supported}")]
    UnsupportedStrategyVersion { found: u8, supported: u8 },
    #[error("room name prefix is empty")]
    EmptyNamePrefix,
    #[error("room name prefix contains characters that cannot be delivered to D2R")]
    InvalidNamePrefix,
    #[error("room password exceeds D2R's {MAX_ROOM_TEXT_LENGTH}-character limit")]
    PasswordTooLong,
    #[error("room password contains characters that cannot be delivered to D2R")]
    InvalidPassword,
    #[error(
        "generated room name {room_name:?} exceeds D2R's {MAX_ROOM_TEXT_LENGTH}-character limit"
    )]
    RoomNameTooLong { room_name: String },
    #[error("sequence width must be between {MIN_SEQUENCE_WIDTH} and {MAX_SEQUENCE_WIDTH}")]
    InvalidSequenceWidth,
    #[error(
        "automatic follower delay must be between {MIN_AUTO_FOLLOWERS_DELAY_SECS} and {MAX_AUTO_FOLLOWERS_DELAY_SECS} seconds"
    )]
    InvalidAutoFollowersDelay,
    #[error("background text strategy {0:?} is unsupported")]
    InvalidBackgroundTextStrategy(String),
    #[error(
        "flow profile {profile} has invalid timing ({step_delay_ms} ms, {character_delay_ms} ms)"
    )]
    InvalidFlowStrategy {
        profile: &'static str,
        step_delay_ms: u64,
        character_delay_ms: u64,
    },
    #[error("primary account is not configured")]
    MissingPrimaryAccount,
    #[error("at least one follower account is required")]
    MissingFollowerAccount,
    #[error("account id {0:?} is empty or is not normalized")]
    InvalidAccountId(String),
    #[error("account {0:?} is selected more than once")]
    DuplicateAccount(String),
    #[error("primary account cannot also be a follower")]
    PrimaryIsFollower,
    #[error("{field} shortcut {value:?} is invalid: {reason}")]
    InvalidShortcut {
        field: &'static str,
        value: String,
        reason: ShortcutValidationError,
    },
    #[error("primary and follower shortcuts must be different")]
    DuplicateRoomShortcut,
    #[error("room automation shortcut {0:?} conflicts with an account shortcut")]
    AccountShortcutConflict(String),
}

impl RoomAutomationConfig {
    /// Normalizes any legacy strategy from the unversioned shape through v16.
    /// Unknown obsolete mouse/profile fields are ignored by Serde and disappear
    /// on the next serialization.
    pub fn normalize_legacy(&mut self) -> Result<NormalizationReport, RoomAutomationConfigError> {
        let source_strategy_version = self.strategy_version;
        if source_strategy_version > CURRENT_STRATEGY_VERSION {
            return Err(RoomAutomationConfigError::UnsupportedStrategyVersion {
                found: source_strategy_version,
                supported: CURRENT_STRATEGY_VERSION,
            });
        }

        let original = self.clone();
        self.primary_account_id = self.primary_account_id.trim().to_string();
        self.name_prefix = self.name_prefix.trim().to_string();
        self.password = self.password.trim().to_string();
        self.sequence_width = self
            .sequence_width
            .clamp(MIN_SEQUENCE_WIDTH, MAX_SEQUENCE_WIDTH);
        self.auto_followers_delay_secs = self
            .auto_followers_delay_secs
            .clamp(MIN_AUTO_FOLLOWERS_DELAY_SECS, MAX_AUTO_FOLLOWERS_DELAY_SECS);

        self.shortcut = normalize_shortcut_or_trim(&self.shortcut);
        self.join_shortcut = normalize_shortcut_or_trim(&self.join_shortcut);

        self.background_text_strategy = match self
            .background_text_strategy
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "post_keys" | "post_keys_chat" | "post_paste" => "post_keys".to_string(),
            "send_keys" | "send_keys_chat" | "send_paste" => "send_keys".to_string(),
            _ => default_background_text_strategy(),
        };

        self.flow.normalize();

        let primary_account_id = self.primary_account_id.clone();
        let primary_identity = account_identity(&primary_account_id);
        let mut followers_seen = BTreeSet::new();
        self.follower_account_ids = self
            .follower_account_ids
            .iter()
            .map(|account_id| account_id.trim())
            .filter(|account_id| !account_id.is_empty())
            .filter(|account_id| account_identity(account_id) != primary_identity)
            .filter(|account_id| followers_seen.insert(account_identity(account_id)))
            .map(str::to_string)
            .collect();

        self.strategy_version = CURRENT_STRATEGY_VERSION;

        Ok(NormalizationReport {
            source_strategy_version,
            target_strategy_version: CURRENT_STRATEGY_VERSION,
            changed: original != *self,
            requires_chat_binding_consent: source_strategy_version < 13
                && self.enabled
                && !self.chat_f13_auto_patch_enabled,
        })
    }

    /// Compatibility validation: disabled legacy configurations may remain
    /// incomplete, while a future strategy version always fails closed.
    pub fn validate<'a, I>(&self, account_shortcuts: I) -> Result<(), RoomAutomationConfigError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        self.validate_supported_version()?;
        if !self.enabled {
            return Ok(());
        }
        self.validate_for_activation(account_shortcuts)
    }

    /// Validates the complete configuration required to enable the capability.
    pub fn validate_for_activation<'a, I>(
        &self,
        account_shortcuts: I,
    ) -> Result<(), RoomAutomationConfigError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        self.validate_supported_version()?;
        self.validate_room_text()?;

        if !(MIN_SEQUENCE_WIDTH..=MAX_SEQUENCE_WIDTH).contains(&self.sequence_width) {
            return Err(RoomAutomationConfigError::InvalidSequenceWidth);
        }
        if !(MIN_AUTO_FOLLOWERS_DELAY_SECS..=MAX_AUTO_FOLLOWERS_DELAY_SECS)
            .contains(&self.auto_followers_delay_secs)
        {
            return Err(RoomAutomationConfigError::InvalidAutoFollowersDelay);
        }
        if !matches!(
            self.background_text_strategy.as_str(),
            "post_keys" | "send_keys"
        ) {
            return Err(RoomAutomationConfigError::InvalidBackgroundTextStrategy(
                self.background_text_strategy.clone(),
            ));
        }
        self.flow.validate("unified")?;

        let primary = self.primary_account_id.as_str();
        if primary.is_empty() {
            return Err(RoomAutomationConfigError::MissingPrimaryAccount);
        }
        validate_normalized_account_id(primary)?;
        if self.follower_account_ids.is_empty() {
            return Err(RoomAutomationConfigError::MissingFollowerAccount);
        }

        let primary_identity = account_identity(primary);
        let mut selected_accounts = BTreeSet::from([primary_identity.clone()]);
        for follower in &self.follower_account_ids {
            validate_normalized_account_id(follower)?;
            let follower_identity = account_identity(follower);
            if follower_identity == primary_identity {
                return Err(RoomAutomationConfigError::PrimaryIsFollower);
            }
            if !selected_accounts.insert(follower_identity) {
                return Err(RoomAutomationConfigError::DuplicateAccount(
                    follower.clone(),
                ));
            }
        }

        let primary_shortcut = canonicalize_shortcut(&self.shortcut).map_err(|reason| {
            RoomAutomationConfigError::InvalidShortcut {
                field: "primary",
                value: self.shortcut.clone(),
                reason,
            }
        })?;
        let followers_shortcut = canonicalize_shortcut(&self.join_shortcut).map_err(|reason| {
            RoomAutomationConfigError::InvalidShortcut {
                field: "followers",
                value: self.join_shortcut.clone(),
                reason,
            }
        })?;
        if primary_shortcut.eq_ignore_ascii_case(&followers_shortcut) {
            return Err(RoomAutomationConfigError::DuplicateRoomShortcut);
        }

        for account_shortcut in account_shortcuts {
            let Ok(account_shortcut) = canonicalize_shortcut(account_shortcut) else {
                continue;
            };
            if account_shortcut.eq_ignore_ascii_case(&primary_shortcut)
                || account_shortcut.eq_ignore_ascii_case(&followers_shortcut)
            {
                return Err(RoomAutomationConfigError::AccountShortcutConflict(
                    account_shortcut,
                ));
            }
        }
        Ok(())
    }

    pub fn generate_room_name(&self, sequence: u32) -> Result<String, RoomAutomationConfigError> {
        validate_ascii_room_value(&self.name_prefix, false).map_err(|error| match error {
            RoomValueError::Empty => RoomAutomationConfigError::EmptyNamePrefix,
            RoomValueError::InvalidCharacter => RoomAutomationConfigError::InvalidNamePrefix,
            RoomValueError::TooLong => RoomAutomationConfigError::RoomNameTooLong {
                room_name: self.name_prefix.clone(),
            },
        })?;
        if !(MIN_SEQUENCE_WIDTH..=MAX_SEQUENCE_WIDTH).contains(&self.sequence_width) {
            return Err(RoomAutomationConfigError::InvalidSequenceWidth);
        }

        let room_name = format!(
            "{}{:0width$}",
            self.name_prefix,
            sequence,
            width = usize::from(self.sequence_width)
        );
        if room_name.len() > MAX_ROOM_TEXT_LENGTH {
            return Err(RoomAutomationConfigError::RoomNameTooLong { room_name });
        }
        Ok(room_name)
    }

    pub fn flow(&self) -> &FlowStrategy {
        &self.flow
    }

    fn validate_supported_version(&self) -> Result<(), RoomAutomationConfigError> {
        if self.strategy_version > CURRENT_STRATEGY_VERSION {
            return Err(RoomAutomationConfigError::UnsupportedStrategyVersion {
                found: self.strategy_version,
                supported: CURRENT_STRATEGY_VERSION,
            });
        }
        Ok(())
    }

    fn validate_room_text(&self) -> Result<(), RoomAutomationConfigError> {
        self.generate_room_name(self.next_sequence)?;
        validate_ascii_room_value(&self.password, true).map_err(|error| match error {
            RoomValueError::Empty => unreachable!("empty passwords are allowed"),
            RoomValueError::InvalidCharacter => RoomAutomationConfigError::InvalidPassword,
            RoomValueError::TooLong => RoomAutomationConfigError::PasswordTooLong,
        })?;
        Ok(())
    }
}

fn account_identity(account_id: &str) -> String {
    account_id.trim().to_ascii_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoomValueError {
    Empty,
    InvalidCharacter,
    TooLong,
}

fn validate_ascii_room_value(value: &str, allow_empty: bool) -> Result<(), RoomValueError> {
    if value.is_empty() {
        return allow_empty.then_some(()).ok_or(RoomValueError::Empty);
    }
    if value.len() > MAX_ROOM_TEXT_LENGTH {
        return Err(RoomValueError::TooLong);
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(RoomValueError::InvalidCharacter);
    }
    Ok(())
}

fn validate_normalized_account_id(account_id: &str) -> Result<(), RoomAutomationConfigError> {
    if account_id.is_empty() || account_id.trim() != account_id {
        return Err(RoomAutomationConfigError::InvalidAccountId(
            account_id.to_string(),
        ));
    }
    Ok(())
}

fn normalize_shortcut_or_trim(value: &str) -> String {
    canonicalize_shortcut(value).unwrap_or_else(|_| value.trim().to_string())
}

/// Produces the same stable modifier order used by the input listener.
pub fn canonicalize_shortcut(value: &str) -> Result<String, ShortcutValidationError> {
    if value.trim().is_empty() {
        return Err(ShortcutValidationError::Empty);
    }

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut key = None;
    let mut components = value.split('+').map(str::trim).collect::<Vec<_>>();
    // `+` is both the separator and the numpad-add key suffix. Fold the one
    // unambiguous trailing form before validating the remaining grammar.
    if components.last() == Some(&"")
        && components
            .get(components.len().saturating_sub(2))
            .is_some_and(|component| component.eq_ignore_ascii_case("num"))
    {
        components.pop();
        if let Some(last) = components.last_mut() {
            *last = "Num+";
        }
    }
    for component in components {
        if component.is_empty() {
            return Err(ShortcutValidationError::EmptyComponent);
        }
        match component.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => set_modifier(&mut ctrl, "Ctrl")?,
            "alt" => set_modifier(&mut alt, "Alt")?,
            "shift" => set_modifier(&mut shift, "Shift")?,
            "win" | "meta" | "cmd" | "command" => {
                return Err(ShortcutValidationError::UnsupportedSystemModifier)
            }
            _ => {
                if key.is_some() {
                    return Err(ShortcutValidationError::MultipleKeys);
                }
                key = Some(canonicalize_key(component)?);
            }
        }
    }

    let key = key.ok_or(ShortcutValidationError::MissingKey)?;
    let mut parts = Vec::with_capacity(4);
    if ctrl {
        parts.push("Ctrl".to_string());
    }
    if alt {
        parts.push("Alt".to_string());
    }
    if shift {
        parts.push("Shift".to_string());
    }
    parts.push(key);
    Ok(parts.join("+"))
}

fn set_modifier(value: &mut bool, name: &str) -> Result<(), ShortcutValidationError> {
    if *value {
        return Err(ShortcutValidationError::DuplicateModifier(name.to_string()));
    }
    *value = true;
    Ok(())
}

fn canonicalize_key(value: &str) -> Result<String, ShortcutValidationError> {
    if value.len() == 1 {
        let byte = value.as_bytes()[0];
        if byte.is_ascii_graphic() && byte != b'+' {
            return Ok(char::from(byte).to_ascii_uppercase().to_string());
        }
    }

    let lower = value.to_ascii_lowercase();
    let named = match lower.as_str() {
        "space" => Some("Space"),
        "enter" => Some("Enter"),
        "tab" => Some("Tab"),
        "escape" | "esc" => Some("Escape"),
        "backspace" => Some("Backspace"),
        "delete" | "del" => Some("Delete"),
        "insert" | "ins" => Some("Insert"),
        "home" => Some("Home"),
        "end" => Some("End"),
        "pageup" => Some("PageUp"),
        "pagedown" => Some("PageDown"),
        "up" | "arrowup" => Some("Up"),
        "down" | "arrowdown" => Some("Down"),
        "left" | "arrowleft" => Some("Left"),
        "right" | "arrowright" => Some("Right"),
        "printscreen" => Some("PrintScreen"),
        "scrolllock" => Some("ScrollLock"),
        "pause" => Some("Pause"),
        "numlock" => Some("NumLock"),
        "num*" => Some("Num*"),
        "num+" => Some("Num+"),
        "num-" => Some("Num-"),
        "num." => Some("Num."),
        "num/" => Some("Num/"),
        _ => None,
    };
    if let Some(named) = named {
        return Ok(named.to_string());
    }

    if let Some(number) = lower.strip_prefix('f').and_then(parse_decimal) {
        if (1..=24).contains(&number) {
            return Ok(format!("F{number}"));
        }
    }
    if let Some(number) = lower.strip_prefix("num").and_then(parse_decimal) {
        if number <= 9 {
            return Ok(format!("Num{number}"));
        }
    }
    if let Some(hex) = lower.strip_prefix("vk") {
        if !hex.is_empty() && hex.len() <= 4 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(format!("VK{}", hex.to_ascii_uppercase()));
        }
    }

    Err(ShortcutValidationError::UnsupportedKey(value.to_string()))
}

fn parse_decimal(value: &str) -> Option<u8> {
    (!value.is_empty())
        .then(|| value.parse::<u8>().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unversioned_legacy_object_uses_safe_disabled_field_defaults() {
        let config: RoomAutomationConfig = serde_json::from_value(json!({})).unwrap();

        assert!(!config.enabled);
        assert!(!config.auto_followers_enabled);
        assert_eq!(config.auto_followers_delay_secs, 5);
        assert_eq!(config.shortcut, "Ctrl+Alt+R");
        assert_eq!(config.strategy_version, 0);
    }

    #[test]
    fn every_legacy_strategy_from_v0_through_v17_normalizes() {
        for version in 0..=CURRENT_STRATEGY_VERSION {
            let mut config = RoomAutomationConfig {
                strategy_version: version,
                ..RoomAutomationConfig::default()
            };

            let report = config.normalize_legacy().unwrap();

            assert_eq!(report.source_strategy_version, version);
            assert_eq!(config.strategy_version, CURRENT_STRATEGY_VERSION);
        }
    }

    #[test]
    fn v15_drops_obsolete_mouse_fields_and_preserves_keyboard_timing() {
        let value = json!({
            "enabled": true,
            "strategy_version": 15,
            "auto_followers_enabled": true,
            "auto_followers_delay_secs": 1,
            "background_click_strategy": "send_child",
            "ui_profile": {"create_tab": {"x": 730, "y": 20}},
            "frontend_timeout_ms": 12_000,
            "standard_flow": {
                "step_delay_ms": 200,
                "character_delay_ms": 50,
                "ui_profile": {"join_tab": {"x": 820, "y": 20}}
            }
        });
        let mut config: RoomAutomationConfig = serde_json::from_value(value).unwrap();

        config.normalize_legacy().unwrap();

        assert_eq!(config.strategy_version, 17);
        assert!(config.auto_followers_enabled);
        assert_eq!(config.auto_followers_delay_secs, 2);
        assert_eq!(config.flow.step_delay_ms, 200);
        assert_eq!(config.flow.character_delay_ms, 50);
        let saved = serde_json::to_value(config).unwrap();
        for obsolete in [
            "background_click_strategy",
            "ui_profile",
            "frontend_timeout_ms",
        ] {
            assert!(saved.get(obsolete).is_none(), "serialized {obsolete}");
        }
        assert!(saved["flow"].get("ui_profile").is_none());
        assert!(saved.get("standard_flow").is_none());
        assert!(saved.get("direct_lobby_flow").is_none());
        assert!(saved.get("account_flow_bindings").is_none());
    }

    #[test]
    fn legacy_enabled_config_requires_explicit_f13_consent() {
        let mut config = RoomAutomationConfig {
            enabled: true,
            strategy_version: 12,
            chat_f13_auto_patch_enabled: false,
            ..RoomAutomationConfig::default()
        };

        let report = config.normalize_legacy().unwrap();

        assert!(report.requires_chat_binding_consent);
        assert!(!config.chat_f13_auto_patch_enabled);
    }

    #[test]
    fn future_strategy_fails_closed() {
        let mut config = RoomAutomationConfig {
            strategy_version: 18,
            ..RoomAutomationConfig::default()
        };

        assert_eq!(
            config.normalize_legacy(),
            Err(RoomAutomationConfigError::UnsupportedStrategyVersion {
                found: 18,
                supported: 17,
            })
        );
    }

    #[test]
    fn normalization_deduplicates_accounts_and_canonicalizes_shortcuts() {
        let mut config = RoomAutomationConfig {
            primary_account_id: " main ".to_string(),
            follower_account_ids: vec![
                "one".to_string(),
                " ONE ".to_string(),
                "MAIN".to_string(),
                "".to_string(),
                "two".to_string(),
            ],
            shortcut: " alt + ctrl + r ".to_string(),
            join_shortcut: "CTRL+alt+j".to_string(),
            ..RoomAutomationConfig::default()
        };

        config.normalize_legacy().unwrap();

        assert_eq!(config.primary_account_id, "main");
        assert_eq!(config.follower_account_ids, ["one", "two"]);
        assert_eq!(config.shortcut, "Ctrl+Alt+R");
        assert_eq!(config.join_shortcut, "Ctrl+Alt+J");
    }

    #[test]
    fn generated_room_names_preserve_zero_padding() {
        let config = RoomAutomationConfig {
            name_prefix: "chaos-".to_string(),
            sequence_width: 3,
            ..RoomAutomationConfig::default()
        };

        assert_eq!(config.generate_room_name(7).unwrap(), "chaos-007");
        assert_eq!(config.generate_room_name(1234).unwrap(), "chaos-1234");
    }

    #[test]
    fn room_name_and_password_reject_unicode_before_runtime_delivery() {
        let mut config = enabled_config();
        config.name_prefix = "巴尔-".to_string();
        assert_eq!(
            config.validate_for_activation(std::iter::empty()),
            Err(RoomAutomationConfigError::InvalidNamePrefix)
        );

        config.name_prefix = "run-".to_string();
        config.password = "密码".to_string();
        assert_eq!(
            config.validate_for_activation(std::iter::empty()),
            Err(RoomAutomationConfigError::InvalidPassword)
        );
    }

    #[test]
    fn validation_rejects_duplicate_accounts_and_shortcut_conflicts() {
        let mut config = enabled_config();
        config.follower_account_ids.push("FOLLOWER".to_string());
        assert_eq!(
            config.validate_for_activation(std::iter::empty()),
            Err(RoomAutomationConfigError::DuplicateAccount(
                "FOLLOWER".to_string()
            ))
        );

        config.follower_account_ids.pop();
        assert_eq!(
            config.validate_for_activation([" alt+CTRL+r "]),
            Err(RoomAutomationConfigError::AccountShortcutConflict(
                "Ctrl+Alt+R".to_string()
            ))
        );

        config.join_shortcut = "ctrl+ALT+r".to_string();
        assert_eq!(
            config.validate_for_activation(std::iter::empty()),
            Err(RoomAutomationConfigError::DuplicateRoomShortcut)
        );
    }

    #[test]
    fn disabled_legacy_config_can_remain_incomplete_until_activation() {
        let config = RoomAutomationConfig {
            name_prefix: String::new(),
            ..RoomAutomationConfig::default()
        };

        assert!(config.validate(std::iter::empty()).is_ok());
        assert_eq!(
            config.validate_for_activation(std::iter::empty()),
            Err(RoomAutomationConfigError::EmptyNamePrefix)
        );
    }

    #[test]
    fn canonical_shortcuts_have_stable_modifier_order() {
        assert_eq!(
            canonicalize_shortcut("shift + alt + control + f12").unwrap(),
            "Ctrl+Alt+Shift+F12"
        );
        assert_eq!(
            canonicalize_shortcut("Meta+R"),
            Err(ShortcutValidationError::UnsupportedSystemModifier)
        );
        assert_eq!(canonicalize_shortcut(" ctrl + num+ ").unwrap(), "Ctrl+Num+");
    }

    fn enabled_config() -> RoomAutomationConfig {
        RoomAutomationConfig {
            enabled: true,
            primary_account_id: "main".to_string(),
            follower_account_ids: vec!["follower".to_string()],
            ..RoomAutomationConfig::default()
        }
    }
}
