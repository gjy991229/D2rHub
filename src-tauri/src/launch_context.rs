use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::commands::account::AccountMeta;
use crate::domain::config::GlobalConfig;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GameRegion {
    Cn,
    Asia,
    Americas,
    Europe,
}

impl GameRegion {
    pub(crate) fn parse(raw: &str) -> Result<Self, AppError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "CN" => Ok(Self::Cn),
            "KR" | "GLOBAL" | "ASIA" => Ok(Self::Asia),
            "NA" | "US" | "AMERICAS" => Ok(Self::Americas),
            "EU" | "EUROPE" => Ok(Self::Europe),
            _ => Err(AppError::ConfigReadError(format!(
                "不支持的账号游戏区服: {raw}"
            ))),
        }
    }

    pub(crate) fn canonical(self) -> &'static str {
        match self {
            Self::Cn => "CN",
            Self::Asia => "KR",
            Self::Americas => "NA",
            Self::Europe => "EU",
        }
    }

    pub(crate) fn edition(self) -> ClientEdition {
        match self {
            Self::Cn => ClientEdition::Cn,
            Self::Asia | Self::Americas | Self::Europe => ClientEdition::Global,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientEdition {
    Cn,
    Global,
}

impl ClientEdition {
    pub(crate) fn canonical(self) -> &'static str {
        match self {
            Self::Cn => "CN",
            Self::Global => "Global",
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Cn => "国服",
            Self::Global => "国际服",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthMode {
    BattleNet,
    Token,
}

impl AuthMode {
    pub(crate) fn parse(raw: Option<&str>) -> Result<Self, AppError> {
        match raw.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("bnet") => Ok(Self::BattleNet),
            Some("token") => Ok(Self::Token),
            Some(mode) => Err(AppError::ConfigReadError(format!(
                "不支持的认证方式: {mode}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextPurpose {
    LaunchGame,
    BattleNetOnly,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditionConventions {
    /// Product accepted by Battle.net's `--exec="launch ..."` command.
    pub battle_net_launch_product: &'static str,
    /// Object key under `Games` in Battle.net.config.
    pub battle_net_config_game_key: &'static str,
    /// Application UID passed to D2R.exe with `-uid` for Token authentication.
    pub token_auth_app: &'static str,
    /// Subkey under Battle.net's `Launch Options` registry key.
    pub token_registry_game_key: &'static str,
}

impl EditionConventions {
    pub(crate) fn for_edition(edition: ClientEdition) -> Self {
        match edition {
            ClientEdition::Cn => Self {
                // The CN client uses `osic` for its config key and direct-launch UID,
                // but Battle.net's command-line launcher still identifies D2R as `OSI`.
                battle_net_launch_product: "OSI",
                battle_net_config_game_key: "osic",
                token_auth_app: "osic",
                token_registry_game_key: "OSI",
            },
            ClientEdition::Global => Self {
                battle_net_launch_product: "OSI",
                battle_net_config_game_key: "osi",
                token_auth_app: "OSI",
                token_registry_game_key: "OSI",
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RegionConventions {
    pub registry_region: &'static str,
    pub default_locale: &'static str,
}

impl RegionConventions {
    fn for_region(region: GameRegion) -> Self {
        match region {
            GameRegion::Cn => Self {
                registry_region: "CN",
                default_locale: "zhCN",
            },
            GameRegion::Asia => Self {
                registry_region: "KR",
                default_locale: "zhTW",
            },
            GameRegion::Americas => Self {
                registry_region: "US",
                default_locale: "enUS",
            },
            GameRegion::Europe => Self {
                registry_region: "EU",
                default_locale: "enUS",
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InstallationProfile {
    pub edition: ClientEdition,
    pub battle_net_executable: Option<PathBuf>,
    pub game_directory: PathBuf,
    pub game_executable: PathBuf,
    /// 可选的存档目录。核心启动只需要游戏可执行文件；仅画质读取/覆盖需要此目录。
    pub saved_games_directory: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct LaunchContext {
    pub game_region: GameRegion,
    pub auth_mode: AuthMode,
    pub installation: InstallationProfile,
    pub edition: EditionConventions,
    pub region: RegionConventions,
}

impl LaunchContext {
    pub(crate) fn for_account(
        config: &GlobalConfig,
        account: &AccountMeta,
        purpose: ContextPurpose,
    ) -> Result<Self, AppError> {
        Self::resolve(
            config,
            account.region.as_deref(),
            account.auth_mode.as_deref(),
            purpose,
        )
    }

    pub(crate) fn for_draft(
        config: &GlobalConfig,
        region: Option<&str>,
        auth_mode: Option<&str>,
        purpose: ContextPurpose,
    ) -> Result<Self, AppError> {
        Self::resolve(config, region, auth_mode, purpose)
    }

    fn resolve(
        config: &GlobalConfig,
        raw_region: Option<&str>,
        raw_auth_mode: Option<&str>,
        purpose: ContextPurpose,
    ) -> Result<Self, AppError> {
        let auth_mode = AuthMode::parse(raw_auth_mode)?;
        if purpose == ContextPurpose::BattleNetOnly && auth_mode == AuthMode::Token {
            return Err(AppError::ConfigReadError(
                "Token 认证不支持 Battle.net 专用操作".to_string(),
            ));
        }
        let game_region = match raw_region.map(str::trim).filter(|value| !value.is_empty()) {
            Some(region) => GameRegion::parse(region)?,
            None => infer_legacy_region(config)?,
        };
        let client_edition = game_region.edition();
        let edition_name = client_edition.display_name();
        if client_edition == ClientEdition::Global && auth_mode == AuthMode::BattleNet {
            return Err(AppError::ConfigReadError(
                "国际服仅支持 Token 直启；请将该账号迁移为 Token 认证".to_string(),
            ));
        }
        let (battle_net_path, game_path, saved_games_path) = match client_edition {
            ClientEdition::Cn => (
                config.cn_battle_net_path.trim(),
                config.cn_game_path.trim(),
                config.cn_saved_games_path.trim(),
            ),
            ClientEdition::Global => (
                "",
                config.global_game_path.trim(),
                config.global_saved_games_path.trim(),
            ),
        };

        let saved_games_directory = if purpose == ContextPurpose::Settings {
            if saved_games_path.is_empty() {
                return Err(AppError::ConfigReadError(format!(
                    "账号属于{edition_name}，画质配置需要先设置该版本的存档目录"
                )));
            }
            Some(validated_directory(
                saved_games_path,
                edition_name,
                "存档目录",
            )?)
        } else if saved_games_path.is_empty() {
            None
        } else {
            // 启动路径中存档目录仅用于可选的 Settings.json 覆盖；无效时由启动流程
            // 降级并告警，不能影响 D2R.exe、认证与互斥句柄这些核心步骤。
            let path = PathBuf::from(saved_games_path);
            path.is_dir().then_some(path)
        };

        let requires_game_installation = purpose != ContextPurpose::Settings;
        let (game_directory, game_executable) = if requires_game_installation {
            if game_path.is_empty() {
                return Err(AppError::ConfigReadError(format!(
                    "账号属于{edition_name}，请先配置该版本的游戏安装目录"
                )));
            }
            let directory = validated_directory(game_path, edition_name, "游戏安装目录")?;
            let executable = directory.join("D2R.exe");
            if !executable.is_file() {
                return Err(AppError::InvalidGamePath(format!(
                    "{}（未找到 D2R.exe）",
                    directory.display()
                )));
            }
            (directory, executable)
        } else {
            // Settings 操作只依赖存档目录。保留声明路径供只读身份比较使用，但不要求
            // 游戏当前在线，也不因 D2R.exe 暂时不可用而阻止设置读写。
            let directory = PathBuf::from(game_path);
            let executable = directory.join("D2R.exe");
            (directory, executable)
        };

        let requires_battle_net = purpose == ContextPurpose::BattleNetOnly
            || (purpose == ContextPurpose::LaunchGame && auth_mode == AuthMode::BattleNet);
        let battle_net_executable = if purpose == ContextPurpose::Settings {
            None
        } else if battle_net_path.is_empty() {
            if requires_battle_net {
                return Err(AppError::ConfigReadError(format!(
                    "账号属于{edition_name}，当前操作需要先配置该版本的 Battle.net.exe"
                )));
            }
            None
        } else {
            let path = validated_file(battle_net_path, edition_name, "Battle.net.exe")?;
            let valid_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("Battle.net.exe"));
            if !valid_name {
                return Err(AppError::InvalidBnetPath(path.display().to_string()));
            }
            Some(path)
        };

        Ok(Self {
            game_region,
            auth_mode,
            installation: InstallationProfile {
                edition: client_edition,
                battle_net_executable,
                game_directory,
                game_executable,
                saved_games_directory,
            },
            edition: EditionConventions::for_edition(client_edition),
            region: RegionConventions::for_region(game_region),
        })
    }

    pub(crate) fn battle_net_executable(&self) -> Result<&Path, AppError> {
        self.installation
            .battle_net_executable
            .as_deref()
            .ok_or_else(|| {
                AppError::ConfigReadError(format!(
                    "{}未配置 Battle.net.exe",
                    self.installation.edition.display_name()
                ))
            })
    }

    pub(crate) fn token_registry_path(&self) -> String {
        format!(
            r"Software\Blizzard Entertainment\Battle.net\Launch Options\{}",
            self.edition.token_registry_game_key
        )
    }

    pub(crate) fn required_saved_games_directory(&self) -> Result<&Path, AppError> {
        self.installation
            .saved_games_directory
            .as_deref()
            .ok_or_else(|| {
                AppError::ConfigReadError(format!(
                    "{}未配置可用的存档目录",
                    self.installation.edition.display_name()
                ))
            })
    }
}

fn validated_directory(raw: &str, edition: &str, role: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(AppError::ConfigReadError(format!(
            "{edition}{role}必须是绝对路径: {raw}"
        )));
    }
    if !path.is_dir() {
        return Err(AppError::ConfigReadError(format!(
            "{edition}{role}不存在或不是目录: {raw}"
        )));
    }
    Ok(path)
}

fn validated_file(raw: &str, edition: &str, role: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(AppError::ConfigReadError(format!(
            "{edition}{role}必须是绝对路径: {raw}"
        )));
    }
    if !path.is_file() {
        return Err(AppError::ConfigReadError(format!(
            "{edition}{role}不存在或不是文件: {raw}"
        )));
    }
    Ok(path)
}

fn profile_declared(config: &GlobalConfig, edition: ClientEdition) -> bool {
    let game = match edition {
        ClientEdition::Cn => &config.cn_game_path,
        ClientEdition::Global => &config.global_game_path,
    };
    !game.trim().is_empty()
}

fn infer_legacy_region(config: &GlobalConfig) -> Result<GameRegion, AppError> {
    match (
        profile_declared(config, ClientEdition::Cn),
        profile_declared(config, ClientEdition::Global),
    ) {
        (true, false) => Ok(GameRegion::Cn),
        (false, true) => Ok(GameRegion::Asia),
        (true, true) => Err(AppError::ConfigReadError(
            "旧账号缺少游戏区服，而国服和国际服均已配置；请编辑账号并明确选择区服".to_string(),
        )),
        (false, false) => Err(AppError::ConfigReadError(
            "旧账号缺少游戏区服，且没有可用于安全推断的客户端版本".to_string(),
        )),
    }
}

/// Resolve only the configured D2R executable identity for an account.
/// This intentionally avoids validating authentication, Battle.net, saved-games, or file
/// availability so an already-running window can still be recognized when an optional path is
/// temporarily unavailable.
pub(crate) fn account_game_executable_identity(
    config: &GlobalConfig,
    account: &AccountMeta,
) -> Result<PathBuf, AppError> {
    let region = match account.region.as_deref() {
        Some(raw) if !raw.trim().is_empty() => GameRegion::parse(raw)?,
        _ => infer_legacy_region(config)?,
    };
    let game_directory = match region.edition() {
        ClientEdition::Cn => config.cn_game_path.trim(),
        ClientEdition::Global => config.global_game_path.trim(),
    };
    if game_directory.is_empty() {
        return Err(AppError::ConfigReadError(format!(
            "{}未配置游戏安装目录",
            region.edition().display_name()
        )));
    }
    Ok(PathBuf::from(game_directory).join("D2R.exe"))
}

pub(crate) fn normalized_path_identity(path: &Path) -> Option<String> {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(trimmed));
    let mut identity = canonical.to_string_lossy().replace('/', "\\");
    let lower = identity.to_ascii_lowercase();
    if lower.starts_with(r"\\?\unc\") {
        identity = format!(r"\\{}", &identity[8..]);
    } else if lower.starts_with(r"\\?\") {
        identity = identity[4..].to_string();
    }

    Some(identity.trim_end_matches('\\').to_ascii_lowercase())
}

pub(crate) fn paths_have_same_identity(actual: &Path, expected: &Path) -> bool {
    match (
        normalized_path_identity(actual),
        normalized_path_identity(expected),
    ) {
        (Some(actual), Some(expected)) => actual == expected,
        _ => false,
    }
}

#[cfg(test)]
pub(crate) fn validate_distinct_installation_profiles(
    config: &GlobalConfig,
) -> Result<(), AppError> {
    if !profile_declared(config, ClientEdition::Cn)
        || !profile_declared(config, ClientEdition::Global)
    {
        return Ok(());
    }
    let pairs = [
        (
            "游戏安装目录",
            &config.cn_game_path,
            &config.global_game_path,
        ),
        (
            "存档目录",
            &config.cn_saved_games_path,
            &config.global_saved_games_path,
        ),
    ];
    for (role, cn, global) in pairs {
        if let (Some(cn), Some(global)) = (
            normalized_path_identity(Path::new(cn)),
            normalized_path_identity(Path::new(global)),
        ) {
            if cn == global {
                return Err(AppError::ConfigWriteError(format!(
                    "国服与国际服不能共用同一个{role}；请分别选择对应客户端版本的路径"
                )));
            }
        }
    }
    Ok(())
}

pub(crate) struct HostRuntimeLease<'a> {
    busy: &'a std::sync::atomic::AtomicBool,
}

impl<'a> HostRuntimeLease<'a> {
    pub(crate) fn try_acquire(state: &'a AppState) -> Result<Self, AppError> {
        state
            .host_runtime_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| {
                AppError::Unknown(
                    "已有账号启动或认证流程正在修改共享 Battle.net 环境，请稍后重试".to_string(),
                )
            })?;
        Ok(Self {
            busy: &state.host_runtime_busy,
        })
    }
}

impl Drop for HostRuntimeLease<'_> {
    fn drop(&mut self) {
        self.busy.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "d2rhub_launch_context_{}_{}_{}",
            name,
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn install(root: &Path, name: &str) -> (String, String, String) {
        let bnet_dir = root.join(format!("{name}-bnet"));
        let game_dir = root.join(format!("{name}-game"));
        let saves_dir = root.join(format!("{name}-saves"));
        std::fs::create_dir_all(&bnet_dir).unwrap();
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::create_dir_all(&saves_dir).unwrap();
        let bnet = bnet_dir.join("Battle.net.exe");
        std::fs::write(&bnet, b"test").unwrap();
        std::fs::write(game_dir.join("D2R.exe"), b"test").unwrap();
        (
            bnet.to_string_lossy().to_string(),
            game_dir.to_string_lossy().to_string(),
            saves_dir.to_string_lossy().to_string(),
        )
    }

    #[test]
    fn path_identity_normalizes_windows_verbatim_prefixes_and_separators() {
        let verbatim = Path::new(r"\\?\C:\Games\D2R\D2R.exe");
        let regular = Path::new("c:/games/d2r/D2R.exe");
        assert!(paths_have_same_identity(verbatim, regular));
    }

    #[test]
    fn edition_identifiers_follow_their_independent_external_contracts() {
        let cn = EditionConventions::for_edition(ClientEdition::Cn);
        assert_eq!(cn.battle_net_launch_product, "OSI");
        assert_eq!(cn.battle_net_config_game_key, "osic");
        assert_eq!(cn.token_auth_app, "osic");
        assert_eq!(cn.token_registry_game_key, "OSI");

        let global = EditionConventions::for_edition(ClientEdition::Global);
        assert_eq!(global.battle_net_launch_product, "OSI");
        assert_eq!(global.battle_net_config_game_key, "osi");
        assert_eq!(global.token_auth_app, "OSI");
        assert_eq!(global.token_registry_game_key, "OSI");
    }

    #[test]
    fn account_regions_map_to_registry_regions_and_default_locales_explicitly() {
        let cases = [
            (GameRegion::Cn, "CN", "zhCN"),
            (GameRegion::Asia, "KR", "zhTW"),
            (GameRegion::Americas, "US", "enUS"),
            (GameRegion::Europe, "EU", "enUS"),
        ];

        for (region, expected_registry_region, expected_locale) in cases {
            let conventions = RegionConventions::for_region(region);
            assert_eq!(conventions.registry_region, expected_registry_region);
            assert_eq!(conventions.default_locale, expected_locale);
        }
    }

    #[test]
    fn window_identity_only_needs_the_accounts_configured_game_directory() {
        let config = GlobalConfig {
            cn_game_path: r"C:\Games\D2R-CN".to_string(),
            ..GlobalConfig::default()
        };
        let mut account = AccountMeta::new("account-1");
        account.region = Some("CN".to_string());
        account.auth_mode = Some("bnet".to_string());

        assert_eq!(
            account_game_executable_identity(&config, &account).unwrap(),
            PathBuf::from(r"C:\Games\D2R-CN").join("D2R.exe")
        );
    }

    #[test]
    fn dual_installations_resolve_cn_battle_net_and_global_token_independently() {
        let root = temp_dir("dual");
        let (cn_bnet, cn_game, cn_saves) = install(&root, "cn");
        let (_, global_game, global_saves) = install(&root, "global");
        let config = GlobalConfig {
            cn_battle_net_path: cn_bnet.clone(),
            cn_game_path: cn_game.clone(),
            cn_saved_games_path: cn_saves,
            global_game_path: global_game.clone(),
            global_saved_games_path: global_saves,
            ..GlobalConfig::default()
        };

        let cn = LaunchContext::for_draft(
            &config,
            Some("CN"),
            Some("bnet"),
            ContextPurpose::LaunchGame,
        )
        .unwrap();
        let eu = LaunchContext::for_draft(
            &config,
            Some("EU"),
            Some("token"),
            ContextPurpose::LaunchGame,
        )
        .unwrap();
        assert_eq!(cn.installation.edition, ClientEdition::Cn);
        assert_eq!(cn.installation.game_directory, PathBuf::from(cn_game));
        assert_eq!(cn.battle_net_executable().unwrap(), Path::new(&cn_bnet));
        assert_eq!(cn.edition.battle_net_launch_product, "OSI");
        assert_eq!(cn.edition.battle_net_config_game_key, "osic");
        assert_eq!(cn.edition.token_auth_app, "osic");
        assert_eq!(cn.edition.token_registry_game_key, "OSI");
        assert_eq!(eu.installation.edition, ClientEdition::Global);
        assert_eq!(eu.installation.game_directory, PathBuf::from(global_game));
        assert!(eu.installation.battle_net_executable.is_none());
        assert_eq!(eu.region.registry_region, "EU");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_region_fails_closed() {
        let result = LaunchContext::for_draft(
            &GlobalConfig::default(),
            Some("unknown"),
            Some("token"),
            ContextPurpose::LaunchGame,
        );
        assert!(result.is_err());
    }

    #[test]
    fn missing_legacy_region_is_ambiguous_with_both_editions() {
        let config = GlobalConfig {
            cn_game_path: r"C:\Games\D2R-CN".to_string(),
            cn_saved_games_path: r"C:\Saves\D2R-CN".to_string(),
            global_game_path: r"D:\Games\D2R".to_string(),
            global_saved_games_path: r"D:\Saves\D2R".to_string(),
            ..GlobalConfig::default()
        };
        assert!(
            LaunchContext::for_draft(&config, None, Some("token"), ContextPurpose::LaunchGame)
                .is_err()
        );
    }

    #[test]
    fn token_context_does_not_require_battle_net() {
        let root = temp_dir("token");
        let (_, game, saves) = install(&root, "global");
        let config = GlobalConfig {
            global_game_path: game,
            global_saved_games_path: saves,
            ..GlobalConfig::default()
        };
        assert!(LaunchContext::for_draft(
            &config,
            Some("NA"),
            Some("token"),
            ContextPurpose::LaunchGame
        )
        .is_ok());
        assert!(LaunchContext::for_draft(
            &config,
            Some("NA"),
            Some("bnet"),
            ContextPurpose::LaunchGame
        )
        .is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn battle_net_only_context_rejects_token_auth() {
        let root = temp_dir("token_bnet_only");
        let (_, game, saves) = install(&root, "global");
        let config = GlobalConfig {
            global_game_path: game,
            global_saved_games_path: saves,
            ..GlobalConfig::default()
        };

        let result = LaunchContext::for_draft(
            &config,
            Some("NA"),
            Some("token"),
            ContextPurpose::BattleNetOnly,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Token 认证不支持 Battle.net"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn settings_context_only_requires_the_saved_games_directory() {
        let root = temp_dir("settings_only");
        let saves = root.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        let config = GlobalConfig {
            global_game_path: String::new(),
            global_saved_games_path: saves.to_string_lossy().to_string(),
            ..GlobalConfig::default()
        };

        let context =
            LaunchContext::for_draft(&config, Some("NA"), Some("token"), ContextPurpose::Settings)
                .unwrap();

        assert_eq!(context.installation.saved_games_directory, Some(saves));
        assert!(context.installation.battle_net_executable.is_none());
        assert!(LaunchContext::for_draft(
            &config,
            Some("NA"),
            Some("token"),
            ContextPurpose::LaunchGame,
        )
        .is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn global_battle_net_auth_is_rejected_before_touching_client_paths() {
        let error = LaunchContext::for_draft(
            &GlobalConfig::default(),
            Some("EU"),
            Some("bnet"),
            ContextPurpose::LaunchGame,
        )
        .unwrap_err();

        assert!(error.to_string().contains("国际服仅支持 Token 直启"));
    }

    #[test]
    fn duplicate_installation_paths_are_rejected() {
        let config = GlobalConfig {
            cn_game_path: r"C:\Games\D2R".to_string(),
            cn_saved_games_path: r"C:\Saves\D2R-CN".to_string(),
            global_game_path: r"c:/games/d2r/".to_string(),
            global_saved_games_path: r"C:\Saves\D2R-Global".to_string(),
            ..GlobalConfig::default()
        };
        assert!(validate_distinct_installation_profiles(&config).is_err());
    }

    #[test]
    fn duplicate_game_paths_are_rejected_even_when_save_paths_are_optional() {
        let config = GlobalConfig {
            cn_game_path: r"C:\Games\D2R".to_string(),
            cn_saved_games_path: r"C:\Saves\D2R-CN".to_string(),
            global_game_path: r"c:/games/d2r/".to_string(),
            ..GlobalConfig::default()
        };
        assert!(validate_distinct_installation_profiles(&config).is_err());
    }

    #[test]
    fn launch_context_allows_missing_optional_saved_games_directory() {
        let root = temp_dir("launch_without_saves");
        let (_, game, _) = install(&root, "global");
        let config = GlobalConfig {
            global_game_path: game,
            global_saved_games_path: String::new(),
            ..GlobalConfig::default()
        };

        let context = LaunchContext::for_draft(
            &config,
            Some("NA"),
            Some("token"),
            ContextPurpose::LaunchGame,
        )
        .unwrap();

        assert!(context.installation.saved_games_directory.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn host_runtime_lease_is_exclusive_and_released_on_drop() {
        let state = AppState::new();
        let first = HostRuntimeLease::try_acquire(&state).unwrap();
        assert!(HostRuntimeLease::try_acquire(&state).is_err());

        drop(first);

        assert!(HostRuntimeLease::try_acquire(&state).is_ok());
    }
}
