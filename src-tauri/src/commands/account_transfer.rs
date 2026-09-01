use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use crate::commands::account::{
    normalized_account_display_name, remove_path_if_exists, replace_path_with_backup,
    resolve_account_runtime_snapshot, sibling_with_suffix, AccountManager, AccountMeta,
};
use crate::error::AppError;
use crate::launch_context::{AuthMode, ContextPurpose, GameRegion, LaunchContext};
use crate::state::{AccountLifecycleLease, SharedState};

const EXPORT_FORMAT: &str = "D2RHub.AccountExport";
const EXPORT_SCHEMA_VERSION: u32 = 2;
const CREDENTIAL_FORMAT: &str = "plaintext";
const MAX_ACCOUNT_COUNT: usize = 100;
const MAX_FILE_COUNT: usize = 20_000;
const MAX_DECODED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_JSON_BYTES: u64 = 600 * 1024 * 1024;
const MAX_PLAINTEXT_TOKEN_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct AccountExportBundle {
    format: String,
    schema_version: u32,
    credential_format: String,
    exported_at: String,
    accounts: Vec<ExportedAccount>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportedAccount {
    account: AccountMeta,
    /// 可跨设备迁移的明文 Token。导出文件本身不提供任何加密保护。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plaintext_token: Option<String>,
    files: Vec<ExportedFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportedFile {
    path: String,
    content_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ExportAccountsSummary {
    pub path: String,
    pub account_count: usize,
    pub plaintext_token_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ImportedAccountSummary {
    pub id: String,
    pub display_name: String,
    pub initialized: bool,
}

#[derive(Debug, Serialize)]
pub struct ImportAccountsSummary {
    pub imported: Vec<ImportedAccountSummary>,
    pub warnings: Vec<String>,
    pub reencrypted_token_count: usize,
}

fn resolved_path_identity(path: &Path) -> Result<String, AppError> {
    let resolved = std::fs::canonicalize(path).map_err(|error| {
        AppError::FileError(format!("无法解析路径 {}: {error}", path.display()))
    })?;
    Ok(resolved
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('/', r"\")
        .trim_end_matches('\\')
        .to_lowercase())
}

fn destination_is_inside_managed_root(
    destination: &Path,
    managed_root: &Path,
) -> Result<bool, AppError> {
    let parent = destination
        .parent()
        .ok_or_else(|| AppError::FileError("导出位置缺少父目录".to_string()))?;
    let parent_identity = resolved_path_identity(parent)?;
    let root_identity = resolved_path_identity(managed_root)?;
    Ok(parent_identity == root_identity
        || parent_identity
            .strip_prefix(&root_identity)
            .is_some_and(|suffix| suffix.starts_with('\\')))
}

fn normalized_export_destination(
    destination: &str,
    managed_root: &Path,
) -> Result<PathBuf, AppError> {
    let mut path = PathBuf::from(destination.trim());
    if !path.is_absolute() {
        return Err(AppError::FileError("导出位置必须是绝对路径".to_string()));
    }
    if path.extension().is_none() {
        path.set_extension("json");
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err(AppError::FileError(
            "账号导出文件必须使用 .json 扩展名".to_string(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::FileError("导出位置缺少父目录".to_string()))?;
    if !parent.is_dir() {
        return Err(AppError::FileError(format!(
            "导出目录不存在: {}",
            parent.display()
        )));
    }
    if destination_is_inside_managed_root(&path, managed_root)? {
        return Err(AppError::FileError(format!(
            "账号导出文件不能保存到 D2RHub 配置目录内，以免覆盖或递归导出软件数据: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn validate_import_source(source: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(source.trim());
    if !path.is_absolute() || !path.is_file() {
        return Err(AppError::FileError(format!(
            "账号导入文件不存在或不是绝对路径: {}",
            path.display()
        )));
    }
    let size = std::fs::metadata(&path)?.len();
    if size > MAX_JSON_BYTES {
        return Err(AppError::FileError(format!(
            "账号导入文件过大（最大 {} MB）",
            MAX_JSON_BYTES / 1024 / 1024
        )));
    }
    Ok(path)
}

fn portable_path(path: &Path) -> Result<String, AppError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            _ => {
                return Err(AppError::FileError(format!(
                    "账号导出路径包含不安全组件: {}",
                    path.display()
                )))
            }
        }
    }
    Ok(parts.join("/"))
}

fn safe_import_relative_path(raw: &str) -> Result<PathBuf, AppError> {
    if raw.is_empty() || raw.contains('\\') || raw.contains(':') {
        return Err(AppError::FileError(format!(
            "账号导入包包含非法文件路径: {raw}"
        )));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError::FileError(format!(
            "账号导入包包含越界文件路径: {raw}"
        )));
    }
    let first = path
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .unwrap_or_default();
    let allowed = matches!(
        first,
        "Settings.json" | "unified_auth.json" | "unified_auth.reg" | "runtime" | "Battle.net"
    );
    if !allowed || first.eq_ignore_ascii_case("account.json") {
        return Err(AppError::FileError(format!(
            "账号导入包包含不受支持的文件: {raw}"
        )));
    }
    Ok(path.to_path_buf())
}

fn collect_files_recursive(
    account_dir: &Path,
    current: &Path,
    files: &mut Vec<ExportedFile>,
    file_count: &mut usize,
    decoded_bytes: &mut u64,
) -> Result<(), AppError> {
    let mut entries = std::fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(AppError::FileError(format!(
                "账号目录包含不支持导出的链接: {}",
                entry.path().display()
            )));
        }
        if file_type.is_dir() {
            collect_files_recursive(account_dir, &entry.path(), files, file_count, decoded_bytes)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".tmp") || name.ends_with(".bak") {
            continue;
        }
        collect_exported_file(account_dir, &entry.path(), files, file_count, decoded_bytes)?;
    }
    Ok(())
}

fn collect_exported_file(
    account_dir: &Path,
    path: &Path,
    files: &mut Vec<ExportedFile>,
    file_count: &mut usize,
    decoded_bytes: &mut u64,
) -> Result<(), AppError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::FileError(format!(
            "账号导出只支持普通文件: {}",
            path.display()
        )));
    }
    if *file_count >= MAX_FILE_COUNT {
        return Err(AppError::FileError(format!(
            "账号导出文件数超过上限 {MAX_FILE_COUNT}"
        )));
    }
    let file_size = metadata.len();
    if decoded_bytes.saturating_add(file_size) > MAX_DECODED_BYTES {
        return Err(AppError::FileError(format!(
            "账号导出内容超过上限 {} MB",
            MAX_DECODED_BYTES / 1024 / 1024
        )));
    }
    let bytes = std::fs::read(path)?;
    *decoded_bytes = decoded_bytes.saturating_add(bytes.len() as u64);
    if *decoded_bytes > MAX_DECODED_BYTES {
        return Err(AppError::FileError(format!(
            "账号导出内容超过上限 {} MB",
            MAX_DECODED_BYTES / 1024 / 1024
        )));
    }
    *file_count += 1;
    let relative = path
        .strip_prefix(account_dir)
        .map_err(|_| AppError::FileError("账号导出路径越界".to_string()))?;
    files.push(ExportedFile {
        path: portable_path(relative)?,
        content_hex: crate::commands::crypto::hex_encode(&bytes),
    });
    Ok(())
}

fn collect_exported_account_files(
    account_dir: &Path,
    file_count: &mut usize,
    decoded_bytes: &mut u64,
) -> Result<Vec<ExportedFile>, AppError> {
    let metadata = std::fs::symlink_metadata(account_dir)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::FileError(format!(
            "账号目录不是可导出的普通目录: {}",
            account_dir.display()
        )));
    }
    let mut files = Vec::new();
    for file_name in ["Settings.json", "unified_auth.json", "unified_auth.reg"] {
        let path = account_dir.join(file_name);
        if path.is_file() {
            collect_exported_file(account_dir, &path, &mut files, file_count, decoded_bytes)?;
        }
    }
    for directory_name in ["runtime", "Battle.net"] {
        let path = account_dir.join(directory_name);
        if path.is_dir() {
            collect_files_recursive(account_dir, &path, &mut files, file_count, decoded_bytes)?;
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn write_bundle(destination: &Path, bundle: &AccountExportBundle) -> Result<(), AppError> {
    let staged = sibling_with_suffix(destination, ".tmp")?;
    let backup = sibling_with_suffix(destination, ".bak")?;
    remove_path_if_exists(&staged)?;
    let mut content = serde_json::to_vec_pretty(bundle)?;
    if content.len() as u64 > MAX_JSON_BYTES {
        content.fill(0);
        return Err(AppError::FileError(format!(
            "账号导出 JSON 超过上限 {} MB",
            MAX_JSON_BYTES / 1024 / 1024
        )));
    }
    let write_result = std::fs::write(&staged, &content);
    content.fill(0);
    write_result?;
    replace_path_with_backup(&staged, destination, &backup).inspect_err(|_| {
        let _ = remove_path_if_exists(&staged);
    })
}

fn existing_display_names(accounts_dir: &str) -> HashSet<String> {
    AccountManager::list_ids(accounts_dir)
        .into_iter()
        .filter_map(|id| AccountManager::load_meta(accounts_dir, &id).ok())
        .map(|meta| {
            let name = if meta.display_name.trim().is_empty() {
                meta.id
            } else {
                meta.display_name
            };
            normalized_account_display_name(&name)
        })
        .collect()
}

fn unique_import_name(requested: &str, used: &mut HashSet<String>) -> String {
    let base = if requested.trim().is_empty() {
        "导入账号".to_string()
    } else {
        requested.trim().to_string()
    };
    if used.insert(normalized_account_display_name(&base)) {
        return base;
    }
    for index in 2..=10_000 {
        let candidate = format!("{base}（导入 {index}）");
        if used.insert(normalized_account_display_name(&candidate)) {
            return candidate;
        }
    }
    format!("{base}（{}）", uuid::Uuid::new_v4())
}

fn write_imported_files(
    target: &Path,
    files: &[ExportedFile],
    file_count: &mut usize,
    decoded_bytes: &mut u64,
) -> Result<(), AppError> {
    let mut seen = HashSet::new();
    for file in files {
        if *file_count >= MAX_FILE_COUNT {
            return Err(AppError::FileError(format!(
                "账号导入文件数超过上限 {MAX_FILE_COUNT}"
            )));
        }
        let relative = safe_import_relative_path(&file.path)?;
        let identity = file.path.to_ascii_lowercase();
        if !seen.insert(identity) {
            return Err(AppError::FileError(format!(
                "账号导入包包含重复文件路径: {}",
                file.path
            )));
        }
        let estimated_bytes = u64::try_from(file.content_hex.len() / 2).unwrap_or(u64::MAX);
        if file.content_hex.len() % 2 != 0
            || decoded_bytes.saturating_add(estimated_bytes) > MAX_DECODED_BYTES
        {
            return Err(AppError::FileError(format!(
                "账号导入内容超过上限 {} MB 或包含无效十六进制数据",
                MAX_DECODED_BYTES / 1024 / 1024
            )));
        }
        let bytes = crate::commands::crypto::hex_decode(&file.content_hex).map_err(|error| {
            AppError::FileError(format!("导入文件 {} 解码失败: {error}", file.path))
        })?;
        *decoded_bytes = decoded_bytes.saturating_add(bytes.len() as u64);
        *file_count += 1;
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(destination, bytes)?;
    }
    Ok(())
}

fn write_account_meta(target: &Path, meta: &AccountMeta) -> Result<(), AppError> {
    std::fs::write(
        target.join("account.json"),
        serde_json::to_vec_pretty(meta)?,
    )?;
    Ok(())
}

fn clear_sensitive_string(value: &mut String) {
    // String 的 UTF-8 缓冲区在本函数内保持有效；清零后再释放，缩短明文驻留时间。
    unsafe {
        value.as_mut_vec().fill(0);
    }
    value.clear();
}

fn export_plaintext_token(account: &mut AccountMeta) -> Result<Option<String>, AppError> {
    let protected_hex = account.token.take();
    match AuthMode::parse(account.auth_mode.as_deref()) {
        Ok(AuthMode::Token) => {
            let Some(protected_hex) = protected_hex else {
                if account.initialized {
                    return Err(AppError::ConfigReadError(format!(
                        "账号“{}”已初始化但缺少 Token，无法生成完整导出文件",
                        account.display_name
                    )));
                }
                return Ok(None);
            };
            let protected =
                crate::commands::crypto::hex_decode(&protected_hex).map_err(|error| {
                    AppError::ConfigReadError(format!(
                        "账号“{}”的 Token 密文损坏，无法导出: {error}",
                        account.display_name
                    ))
                })?;
            let mut plaintext =
                crate::commands::crypto::unprotect_token(&protected).map_err(|error| {
                    AppError::ConfigReadError(format!(
                        "账号“{}”的 Token 无法在当前 Windows 用户下解密，无法导出: {error}",
                        account.display_name
                    ))
                })?;
            if plaintext.len() > MAX_PLAINTEXT_TOKEN_BYTES {
                clear_sensitive_string(&mut plaintext);
                return Err(AppError::ConfigReadError(format!(
                    "账号“{}”的明文 Token 超过 1 MB 安全上限",
                    account.display_name
                )));
            }
            Ok(Some(plaintext))
        }
        Ok(AuthMode::BattleNet) => {
            if protected_hex.is_some() {
                return Err(AppError::ConfigReadError(format!(
                    "账号“{}”为 Battle.net 认证但包含异常 Token，已拒绝导出",
                    account.display_name
                )));
            }
            Ok(None)
        }
        Err(error) => {
            if protected_hex.is_some() {
                return Err(AppError::ConfigReadError(format!(
                    "账号“{}”的认证模式无效且包含 Token，无法安全导出: {error}",
                    account.display_name
                )));
            }
            Ok(None)
        }
    }
}

fn prepare_imported_credentials(
    staging: &Path,
    meta: &mut AccountMeta,
    plaintext_token: Option<String>,
    warnings: &mut Vec<String>,
) -> Result<bool, AppError> {
    match AuthMode::parse(meta.auth_mode.as_deref()) {
        Ok(AuthMode::Token) => {
            let Some(mut plaintext) = plaintext_token else {
                meta.token = None;
                if meta.initialized {
                    meta.initialized = false;
                    meta.last_reset_at = None;
                    warnings.push(format!(
                        "“{}”的导入数据缺少明文 Token，已导入为待重新认证账号",
                        meta.display_name
                    ));
                }
                return Ok(false);
            };
            if plaintext.trim().is_empty() {
                clear_sensitive_string(&mut plaintext);
                return Err(AppError::ConfigReadError(format!(
                    "账号“{}”的明文 Token 为空",
                    meta.display_name
                )));
            }
            if plaintext.len() > MAX_PLAINTEXT_TOKEN_BYTES {
                clear_sensitive_string(&mut plaintext);
                return Err(AppError::ConfigReadError(format!(
                    "账号“{}”的明文 Token 超过 1 MB 安全上限",
                    meta.display_name
                )));
            }
            let protected_result = crate::commands::crypto::protect_token(&plaintext);
            clear_sensitive_string(&mut plaintext);
            let protected = protected_result.map_err(|error| {
                AppError::ConfigWriteError(format!(
                    "账号“{}”的 Token 无法使用当前设备 DPAPI 加密: {error}",
                    meta.display_name
                ))
            })?;
            meta.token = Some(crate::commands::crypto::hex_encode(&protected));
            Ok(true)
        }
        Ok(AuthMode::BattleNet) => {
            if let Some(mut plaintext) = plaintext_token {
                clear_sensitive_string(&mut plaintext);
                return Err(AppError::ConfigReadError(format!(
                    "账号“{}”为 Battle.net 认证，但导入包包含不应存在的明文 Token",
                    meta.display_name
                )));
            }
            meta.token = None;
            if !meta.initialized {
                return Ok(false);
            }
            let validation = meta
                .region
                .as_deref()
                .ok_or_else(|| AppError::ConfigReadError("缺少游戏区服".to_string()))
                .and_then(GameRegion::parse)
                .and_then(|region| {
                    resolve_account_runtime_snapshot(staging, meta, region.edition()).map(|_| ())
                });
            if let Err(error) = validation {
                meta.initialized = false;
                meta.snapshot_edition = None;
                warnings.push(format!(
                    "“{}”的 Battle.net 认证快照不完整，已导入为待重新初始化账号: {error}",
                    meta.display_name
                ));
            }
            Ok(false)
        }
        Err(error) => {
            if let Some(mut plaintext) = plaintext_token {
                clear_sensitive_string(&mut plaintext);
                return Err(AppError::ConfigReadError(format!(
                    "账号“{}”的认证模式无效，但导入包包含明文 Token: {error}",
                    meta.display_name
                )));
            }
            meta.token = None;
            meta.initialized = false;
            warnings.push(format!(
                "“{}”的认证模式无效，已导入为待重新认证账号: {error}",
                meta.display_name
            ));
            Ok(false)
        }
    }
}

#[tauri::command]
pub fn export_accounts(
    state: tauri::State<'_, SharedState>,
    account_ids: Vec<String>,
    destination: String,
    acknowledge_plaintext_risk: bool,
) -> Result<ExportAccountsSummary, AppError> {
    if !acknowledge_plaintext_risk {
        return Err(AppError::ConfigReadError(
            "导出文件包含可直接使用的明文 Token，请先确认已理解账号安全风险".to_string(),
        ));
    }
    if account_ids.is_empty() || account_ids.len() > MAX_ACCOUNT_COUNT {
        return Err(AppError::ConfigReadError(format!(
            "请选择 1 至 {MAX_ACCOUNT_COUNT} 个账号导出"
        )));
    }
    let mut canonical = HashSet::new();
    for account_id in &account_ids {
        AccountManager::validate_account_id(account_id)?;
        if !canonical.insert(account_id.to_ascii_lowercase()) {
            return Err(AppError::ConfigReadError(
                "导出列表包含重复账号".to_string(),
            ));
        }
    }

    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    let mut lease_ids = account_ids.clone();
    lease_ids.sort_by_key(|id| id.to_ascii_lowercase());
    let _leases: Vec<AccountLifecycleLease> = lease_ids
        .iter()
        .map(|id| AccountLifecycleLease::try_acquire(state.inner(), id))
        .collect::<Result<_, _>>()?;

    let destination = normalized_export_destination(&destination, Path::new(&state.app_data_dir))?;
    let mut accounts = Vec::with_capacity(account_ids.len());
    let mut file_count = 0usize;
    let mut decoded_bytes = 0u64;
    let mut plaintext_token_count = 0usize;
    for account_id in &account_ids {
        let mut account = AccountManager::load_meta(&cfg.accounts_dir, account_id)?;
        account.is_running = false;
        account.running_pid = None;
        let plaintext_token = export_plaintext_token(&mut account)?;
        plaintext_token_count += usize::from(plaintext_token.is_some());
        let account_dir = AccountManager::account_dir_checked(&cfg.accounts_dir, account_id)?;
        accounts.push(ExportedAccount {
            account,
            plaintext_token,
            files: collect_exported_account_files(
                &account_dir,
                &mut file_count,
                &mut decoded_bytes,
            )?,
        });
    }

    let mut bundle = AccountExportBundle {
        format: EXPORT_FORMAT.to_string(),
        schema_version: EXPORT_SCHEMA_VERSION,
        credential_format: CREDENTIAL_FORMAT.to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        accounts,
    };
    let write_result = write_bundle(&destination, &bundle);
    for exported in &mut bundle.accounts {
        if let Some(plaintext) = exported.plaintext_token.as_mut() {
            clear_sensitive_string(plaintext);
        }
        exported.plaintext_token = None;
    }
    write_result?;
    Ok(ExportAccountsSummary {
        path: destination.to_string_lossy().to_string(),
        account_count: account_ids.len(),
        plaintext_token_count,
    })
}

#[tauri::command]
pub fn import_accounts(
    state: tauri::State<'_, SharedState>,
    source: String,
) -> Result<ImportAccountsSummary, AppError> {
    let source = validate_import_source(&source)?;
    let mut source_bytes = std::fs::read(&source)?;
    let parsed = serde_json::from_slice(&source_bytes);
    source_bytes.fill(0);
    let bundle: AccountExportBundle = parsed?;
    if bundle.format != EXPORT_FORMAT
        || bundle.schema_version != EXPORT_SCHEMA_VERSION
        || bundle.credential_format != CREDENTIAL_FORMAT
    {
        return Err(AppError::ConfigReadError(format!(
            "不支持的账号导入格式、版本或凭据类型: {} / {} / {}",
            bundle.format, bundle.schema_version, bundle.credential_format
        )));
    }
    if bundle.accounts.is_empty() || bundle.accounts.len() > MAX_ACCOUNT_COUNT {
        return Err(AppError::ConfigReadError(format!(
            "导入包必须包含 1 至 {MAX_ACCOUNT_COUNT} 个账号"
        )));
    }
    let mut source_ids = HashSet::new();
    for exported in &bundle.accounts {
        AccountManager::validate_account_id(&exported.account.id)?;
        if exported.account.token.is_some() {
            return Err(AppError::ConfigReadError(format!(
                "新版账号导入包不能在 account 中夹带设备绑定的 DPAPI Token: {}",
                exported.account.id
            )));
        }
        if !source_ids.insert(exported.account.id.to_ascii_lowercase()) {
            return Err(AppError::ConfigReadError(
                "导入包包含重复账号 ID".to_string(),
            ));
        }
    }

    let cfg = state
        .configuration()
        .snapshot()
        .ok_or_else(|| AppError::ConfigReadError("尚未完成首次配置".to_string()))?;
    // 与新建、重命名共用账号清单锁，确保导入生成名称期间唯一性不被并发绕过。
    let _catalog_guard = state.account_catalog_write_lock.lock();
    std::fs::create_dir_all(&cfg.accounts_dir)?;
    let mut used_names = existing_display_names(&cfg.accounts_dir);
    let next_order = AccountManager::list_ids(&cfg.accounts_dir)
        .into_iter()
        .filter_map(|id| AccountManager::load_meta(&cfg.accounts_dir, &id).ok())
        .map(|meta| meta.order)
        .max()
        .map_or(0, |order| order.saturating_add(1));

    let mut warnings = vec![
        "导入文件包含可直接使用的明文 Token 或认证快照；请确认导入成功后立即安全删除该 JSON，且不要发送给任何人。".to_string(),
        "导入不包含隔离浏览器缓存；Battle.net 认证快照跨设备时仍可能需要重新初始化。".to_string(),
    ];
    let mut staged_accounts: Vec<(PathBuf, PathBuf, AccountMeta)> = Vec::new();
    let mut file_count = 0usize;
    let mut decoded_bytes = 0u64;
    let mut reencrypted_token_count = 0usize;
    let build_result = (|| -> Result<(), AppError> {
        for (index, exported) in bundle.accounts.into_iter().enumerate() {
            let ExportedAccount {
                account: mut meta,
                plaintext_token,
                files,
            } = exported;
            meta.id = AccountManager::next_id(&cfg.accounts_dir);
            meta.display_name = unique_import_name(&meta.display_name, &mut used_names);
            meta.order = next_order.saturating_add(index as u32);
            meta.is_running = false;
            meta.running_pid = None;
            meta.last_launched_at = None;

            let target = AccountManager::account_dir_checked(&cfg.accounts_dir, &meta.id)?;
            if target.exists() {
                return Err(AppError::FileError(format!(
                    "导入生成的账号目录已存在，请重试: {}",
                    target.display()
                )));
            }
            let staging = sibling_with_suffix(&target, ".tmp")?;
            remove_path_if_exists(&staging)?;
            let stage_result = (|| -> Result<(), AppError> {
                std::fs::create_dir_all(&staging)?;
                write_imported_files(&staging, &files, &mut file_count, &mut decoded_bytes)?;
                if prepare_imported_credentials(
                    &staging,
                    &mut meta,
                    plaintext_token,
                    &mut warnings,
                )? {
                    reencrypted_token_count += 1;
                }
                write_account_meta(&staging, &meta)?;

                if let Err(error) =
                    LaunchContext::for_account(&cfg, &meta, ContextPurpose::LaunchGame)
                {
                    warnings.push(format!(
                        "“{}”已导入，但当前安装配置暂不能启动该账号: {error}",
                        meta.display_name
                    ));
                }
                Ok(())
            })();
            if let Err(error) = stage_result {
                let _ = remove_path_if_exists(&staging);
                return Err(error);
            }
            staged_accounts.push((staging, target, meta));
        }
        Ok(())
    })();
    if let Err(error) = build_result {
        for (staging, _, _) in &staged_accounts {
            let _ = remove_path_if_exists(staging);
        }
        return Err(error);
    }

    let new_ids: Vec<String> = staged_accounts
        .iter()
        .map(|(_, _, meta)| meta.id.clone())
        .collect();
    let leases: Result<Vec<AccountLifecycleLease>, AppError> = new_ids
        .iter()
        .map(|id| AccountLifecycleLease::try_acquire(state.inner(), id))
        .collect();
    let _leases = match leases {
        Ok(leases) => leases,
        Err(error) => {
            for (staging, _, _) in &staged_accounts {
                let _ = remove_path_if_exists(staging);
            }
            return Err(error);
        }
    };
    let mut installed_targets: Vec<PathBuf> = Vec::new();
    for (staging, target, _) in &staged_accounts {
        if target.exists() {
            for installed in installed_targets {
                let _ = remove_path_if_exists(&installed);
            }
            for (pending, _, _) in &staged_accounts {
                let _ = remove_path_if_exists(pending);
            }
            return Err(AppError::FileError(format!(
                "导入目标账号目录在提交前已出现，已取消导入: {}",
                target.display()
            )));
        }
        let backup = sibling_with_suffix(target, ".bak")?;
        if let Err(error) = replace_path_with_backup(staging, target, &backup) {
            for installed in installed_targets {
                let _ = remove_path_if_exists(&installed);
            }
            for (pending, _, _) in &staged_accounts {
                let _ = remove_path_if_exists(pending);
            }
            return Err(error);
        }
        installed_targets.push(target.to_path_buf());
    }

    Ok(ImportAccountsSummary {
        imported: staged_accounts
            .into_iter()
            .map(|(_, _, meta)| ImportedAccountSummary {
                id: meta.id,
                display_name: meta.display_name,
                initialized: meta.initialized,
            })
            .collect(),
        warnings,
        reencrypted_token_count,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        export_plaintext_token, normalized_export_destination, prepare_imported_credentials,
        safe_import_relative_path, unique_import_name, ExportedAccount,
    };
    use crate::commands::account::AccountMeta;
    use std::collections::HashSet;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "d2rhub_account_transfer_{name}_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn import_paths_cannot_escape_the_account_directory() {
        assert!(safe_import_relative_path("runtime/snapshot.json").is_ok());
        assert!(safe_import_relative_path("Settings.json").is_ok());
        assert!(safe_import_relative_path("../account.json").is_err());
        assert!(safe_import_relative_path("BrowserProfile/Default/Cookies").is_err());
        assert!(safe_import_relative_path("C:/Windows/system.ini").is_err());
    }

    #[test]
    fn export_destination_cannot_overwrite_managed_configuration() {
        let root = temp_dir("protected_destination");
        let managed = root.join("D2RHub");
        let external = root.join("exports");
        std::fs::create_dir_all(managed.join("accounts").join("acount1")).unwrap();
        std::fs::create_dir_all(&external).unwrap();

        assert!(normalized_export_destination(
            managed
                .join("accounts")
                .join("acount1")
                .join("account.json")
                .to_str()
                .unwrap(),
            &managed,
        )
        .is_err());
        assert!(normalized_export_destination(
            external.join("accounts.json").to_str().unwrap(),
            &managed,
        )
        .is_ok());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn imported_duplicate_names_receive_a_stable_suffix() {
        let mut used = HashSet::from(["主号".to_string(), "主号（导入 2）".to_string()]);
        assert_eq!(unique_import_name("主号", &mut used), "主号（导入 3）");
        assert_eq!(unique_import_name("副号", &mut used), "副号");

        let mut case_insensitive = HashSet::from(["primary account".to_string()]);
        assert_eq!(
            unique_import_name("  PRIMARY ACCOUNT  ", &mut case_insensitive),
            "PRIMARY ACCOUNT（导入 2）"
        );
    }

    #[test]
    fn exported_plaintext_token_is_reencrypted_for_the_importing_user() {
        let token = "portable-test-token";
        let protected = crate::commands::crypto::protect_token(token).unwrap();
        let mut meta = AccountMeta::new("acount1");
        meta.display_name = "测试账号".to_string();
        meta.auth_mode = Some("token".to_string());
        meta.region = Some("NA".to_string());
        meta.initialized = true;
        meta.token = Some(crate::commands::crypto::hex_encode(&protected));

        let plaintext = export_plaintext_token(&mut meta).unwrap();
        assert_eq!(plaintext.as_deref(), Some(token));
        assert!(meta.token.is_none());
        let serialized = serde_json::to_value(ExportedAccount {
            account: meta.clone(),
            plaintext_token: plaintext.clone(),
            files: Vec::new(),
        })
        .unwrap();
        assert_eq!(serialized["plaintext_token"], token);
        assert!(serialized["account"].get("token").is_none());

        let mut warnings = Vec::new();
        assert!(prepare_imported_credentials(
            std::path::Path::new("."),
            &mut meta,
            plaintext,
            &mut warnings,
        )
        .unwrap());
        let imported_protected = crate::commands::crypto::hex_decode(
            meta.token
                .as_deref()
                .expect("import should store DPAPI token"),
        )
        .unwrap();
        assert_eq!(
            crate::commands::crypto::unprotect_token(&imported_protected).unwrap(),
            token
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn battle_net_account_rejects_an_unexpected_plaintext_token() {
        let mut meta = AccountMeta::new("acount1");
        meta.display_name = "战网账号".to_string();
        meta.auth_mode = Some("bnet".to_string());
        let mut warnings = Vec::new();

        assert!(prepare_imported_credentials(
            std::path::Path::new("."),
            &mut meta,
            Some("unexpected-token".to_string()),
            &mut warnings,
        )
        .is_err());
    }
}
