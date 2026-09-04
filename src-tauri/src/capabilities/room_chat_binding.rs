//! Lifecycle-owned one-shot scanner for the room-automation F13 binding.
//!
//! This adapter deliberately owns no global configuration or Tauri state. A
//! capability driver supplies the save directories and the process probe, and
//! must retain this service for exactly as long as the capability is alive.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{Metadata, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::infrastructure::durable_fs;

const KEY_FILE_VERSION: u16 = 37;
const KEY_FILE_HEADER_SIZE: usize = 2;
const KEY_RECORD_SIZE: usize = 20;
const CHAT_ACTION_INDEX: usize = 5;
const CHAT_ACTION_ID: u32 = 5;
const CHAT_RECORD_OFFSET: usize = KEY_FILE_HEADER_SIZE + CHAT_ACTION_INDEX * KEY_RECORD_SIZE;
const SECONDARY_SLOT_OFFSET: usize = CHAT_RECORD_OFFSET + 10;
const VK_RETURN: u16 = 0x000D;
const VK_F13: u16 = 0x007C;
const BOUND_KEY_TYPE: u32 = 1;
const UNBOUND_KEY_TYPE: u32 = 0;
const BACKUP_SUFFIX: &str = ".d2rhub-chat-f13.bak";
const STAGING_SUFFIX: &str = ".d2rhub-chat-f13.stage";
const ROLLBACK_SUFFIX: &str = ".d2rhub-chat-f13.rollback";
const JOURNAL_SUFFIX: &str = ".d2rhub-chat-f13.journal";
const REPLACE_JOURNAL_VERSION: u8 = 1;
#[cfg(test)]
const UNBOUND_KEY: u16 = 0xFFFF;

/// Strong evidence that the caller read an affirmative, new-format user
/// choice. Legacy imports must never manufacture this token.
#[derive(Debug)]
pub(crate) struct ExplicitChatBindingConsent(());

impl ExplicitChatBindingConsent {
    pub(crate) fn from_persisted_user_consent(granted: bool) -> Result<Self, String> {
        if granted {
            Ok(Self(()))
        } else {
            Err("未获得用户对 F13 配置变更扫描的显式同意".to_string())
        }
    }
}

/// Filesystem readiness and durable scan consent are reported separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatF13BindingStatus {
    pub ready: bool,
    pub total_files: usize,
    pub installed_files: usize,
    pub eligible_files: usize,
    pub conflicted_files: usize,
    pub backup_files: usize,
    /// Backups whose original character key file no longer exists. They are
    /// retained as user-recoverable artifacts, but never block live files.
    pub orphan_backup_files: usize,
    pub transaction_artifacts: usize,
    pub d2r_running: bool,
    pub consent_granted: bool,
    pub watcher_running: bool,
    /// Compatibility projection for callers that previously exposed one
    /// boolean. It now reflects one-shot scan consent.
    pub auto_patch_enabled: bool,
    pub directories: Vec<String>,
    pub last_watcher_error: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingState {
    Eligible,
    Installed,
}

#[derive(Debug)]
struct InspectedFile {
    path: PathBuf,
    bytes: Vec<u8>,
    state: Result<BindingState, String>,
    backup: Option<Result<Vec<u8>, String>>,
}

impl InspectedFile {
    fn install_problem(&self) -> Option<String> {
        let state = match &self.state {
            Ok(state) => *state,
            Err(error) => return Some(error.clone()),
        };
        match (&self.backup, state) {
            // A manually installed F13 needs no D2RHub backup because install
            // will not touch it. Backup ownership matters only when an
            // eligible file is about to be changed.
            (_, BindingState::Installed) => None,
            (Some(Err(error)), BindingState::Eligible) => Some(error.clone()),
            (Some(Ok(bytes)), BindingState::Eligible)
                if inspect_key_bytes(bytes) != Ok(BindingState::Eligible) =>
            {
                Some("已有备份不是可恢复的原生 Chat 第二键位状态".to_string())
            }
            _ => None,
        }
    }

    fn has_valid_backup(&self) -> bool {
        self.backup.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|bytes| inspect_key_bytes(bytes) == Ok(BindingState::Eligible))
        })
    }
}

#[derive(Debug, Default)]
struct FilesystemSnapshot {
    files: Vec<InspectedFile>,
    orphan_backups: Vec<PathBuf>,
    transaction_artifacts: Vec<PathBuf>,
}

#[derive(Debug)]
struct RestoreEntry {
    path: PathBuf,
    current: Vec<u8>,
    restored: Vec<u8>,
}

impl FilesystemSnapshot {
    fn conflict_count(&self) -> usize {
        self.files.iter().filter(|file| file.state.is_err()).count()
    }

    fn ready(&self) -> bool {
        !self.files.is_empty()
            && self.transaction_artifacts.is_empty()
            && self
                .files
                .iter()
                .all(|file| file.state == Ok(BindingState::Installed))
    }
}

fn build_restore_plan(snapshot: &FilesystemSnapshot) -> Result<Vec<RestoreEntry>, String> {
    snapshot
        .files
        .iter()
        .filter_map(|file| {
            let backup = file.backup.as_ref()?;
            Some((|| {
                let backup = backup
                    .as_ref()
                    .map_err(|error| format!("无法使用 {} 的备份：{error}", file.path.display()))?;
                let restored = restore_chat_secondary_slot(&file.bytes, backup)
                    .map_err(|error| format!("无法恢复 {}：{error}", file.path.display()))?;
                Ok(RestoreEntry {
                    path: file.path.clone(),
                    current: file.bytes.clone(),
                    restored,
                })
            })())
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceJournal {
    version: u8,
    target_file_name: String,
    original_checksum: u64,
    replacement_checksum: u64,
}

struct ServiceInner {
    directories: Vec<PathBuf>,
    d2r_running: Arc<dyn Fn() -> bool + Send + Sync + 'static>,
    operation: Mutex<()>,
    consent_granted: AtomicBool,
}

/// Capability-owned F13 binding service.
///
/// A newly constructed service has no consent and starts no threads. A
/// successful [`Self::install`] grants consent and performs one bounded scan.
/// Later scans are triggered by room-automation configuration commits or by an
/// explicit user action; no persistent filesystem watcher is required.
pub(crate) struct ChatF13BindingService {
    inner: Arc<ServiceInner>,
}

impl ChatF13BindingService {
    pub(crate) fn new<F>(directories: Vec<PathBuf>, d2r_running: F) -> Result<Self, String>
    where
        F: Fn() -> bool + Send + Sync + 'static,
    {
        let directories = validate_and_canonicalize_directories(directories)?;
        Ok(Self {
            inner: Arc::new(ServiceInner {
                directories,
                d2r_running: Arc::new(d2r_running),
                operation: Mutex::new(()),
                consent_granted: AtomicBool::new(false),
            }),
        })
    }

    /// Preflights every key file, creates durable non-overwriting backups,
    /// atomically patches eligible files, and grants consent.
    pub(crate) fn install(&self) -> Result<ChatF13BindingStatus, String> {
        self.scan_once()
    }

    fn scan_once(&self) -> Result<ChatF13BindingStatus, String> {
        {
            let _operation = lock(&self.inner.operation);
            let snapshot = inspect_filesystem(&self.inner.directories)?;
            if snapshot.ready() {
                self.inner.consent_granted.store(true, Ordering::Release);
                return Ok(build_status(&self.inner, &snapshot));
            }
            if snapshot.transaction_artifacts.is_empty() {
                preflight_install(&snapshot)?;
            }
            ensure_d2r_closed(&self.inner)?;
        }

        let verified = (|| {
            let _operation = lock(&self.inner.operation);
            ensure_d2r_closed(&self.inner)?;
            recover_interrupted_transactions(&self.inner.directories)?;
            let snapshot = inspect_filesystem(&self.inner.directories)?;
            preflight_install(&snapshot)?;

            // Finish every backup before changing the first live key file.
            for file in &snapshot.files {
                if file.state == Ok(BindingState::Eligible) && file.backup.is_none() {
                    create_backup(&file.path, &file.bytes)?;
                }
            }

            let mut changed: Vec<(&Path, Vec<u8>)> = Vec::new();
            for file in &snapshot.files {
                if file.state == Ok(BindingState::Installed) {
                    continue;
                }
                let patched = patch_f13(&file.bytes)?;
                if let Err(error) = atomic_replace_bytes(&file.path, &file.bytes, &patched) {
                    let rollback_errors = rollback_changed_files(&changed);
                    let suffix = if rollback_errors.is_empty() {
                        "；本轮已修改文件已回滚".to_string()
                    } else {
                        format!("；回滚失败：{}", rollback_errors.join("；"))
                    };
                    return Err(format!("{error}{suffix}"));
                }
                changed.push((&file.path, file.bytes.clone()));
            }

            let verified = inspect_filesystem(&self.inner.directories)?;
            require_scan_ready(&verified)?;
            Ok::<FilesystemSnapshot, String>(verified)
        })()?;

        self.inner.consent_granted.store(true, Ordering::Release);
        Ok(build_status(&self.inner, &verified))
    }

    /// Compatibility entry point for persisted consent. It now performs one
    /// bounded scan instead of starting a persistent watcher.
    pub(crate) fn start_watcher_with_consent(
        &self,
        _consent: ExplicitChatBindingConsent,
    ) -> Result<ChatF13BindingStatus, String> {
        self.scan_once()
    }

    /// Releases runtime ownership without changing files or scan consent.
    pub(crate) fn stop(&self) -> Result<ChatF13BindingStatus, String> {
        self.status()
    }

    /// There is no background worker to join. This hook keeps lifecycle callers
    /// independent from the one-shot implementation.
    pub(crate) fn shutdown(&self) -> Result<(), String> {
        Ok(())
    }

    /// Verifies that a restore has a safe, complete plan without changing
    /// consent, backups or live key files.
    pub(crate) fn preflight_restore(&self) -> Result<(), String> {
        let _operation = lock(&self.inner.operation);
        let snapshot = inspect_filesystem(&self.inner.directories)?;
        if snapshot.transaction_artifacts.is_empty() {
            if !build_restore_plan(&snapshot)?.is_empty() {
                ensure_d2r_closed(&self.inner)?;
            }
        } else {
            ensure_d2r_closed(&self.inner)?;
        }
        Ok(())
    }

    /// Restores only the Chat secondary slot from each D2RHub backup. Unrelated
    /// edits made since installation survive.
    pub(crate) fn restore(&self) -> Result<ChatF13BindingStatus, String> {
        // Invalid or absent backups must not revoke valid scan consent.
        self.preflight_restore()?;

        let restore_result =
            (|| {
                let _operation = lock(&self.inner.operation);
                let before_recovery = inspect_filesystem(&self.inner.directories)?;
                if !before_recovery.transaction_artifacts.is_empty() {
                    ensure_d2r_closed(&self.inner)?;
                    recover_interrupted_transactions(&self.inner.directories)?;
                }
                let snapshot = inspect_filesystem(&self.inner.directories)?;
                let restore_plan = build_restore_plan(&snapshot)?;
                if !restore_plan.is_empty() {
                    ensure_d2r_closed(&self.inner)?;
                }
                let restored_any = !restore_plan.is_empty();

                let mut changed: Vec<(&Path, Vec<u8>)> = Vec::new();
                for entry in &restore_plan {
                    let path = entry.path.as_path();
                    let current = &entry.current;
                    let restored = &entry.restored;
                    if current != restored {
                        if let Err(error) = atomic_replace_bytes(path, current, restored) {
                            let rollback_errors = rollback_changed_files(&changed);
                            let suffix = if rollback_errors.is_empty() {
                                "；本轮已恢复文件已回滚".to_string()
                            } else {
                                format!("；回滚失败：{}", rollback_errors.join("；"))
                            };
                            return Err(format!("{error}{suffix}"));
                        }
                        changed.push((path, current.clone()));
                    }
                }

                self.inner.consent_granted.store(false, Ordering::Release);
                let mut cleanup_warnings = Vec::new();
                for entry in restore_plan {
                    let backup = backup_path(&entry.path)?;
                    if let Err(error) = remove_regular_if_exists(&backup).and_then(|_| {
                        sync_directory(entry.path.parent().ok_or_else(|| {
                            format!("键位文件缺少父目录：{}", entry.path.display())
                        })?)
                    }) {
                        cleanup_warnings.push(format!("{}：{error}", backup.display()));
                    }
                }
                Ok::<(Vec<String>, bool), String>((cleanup_warnings, restored_any))
            })();
        let (cleanup_warnings, restored_any) = restore_result?;
        let mut status = self.status()?;
        if !restored_any {
            status
                .message
                .push_str("；没有 D2RHub 管理的备份，已仅关闭配置变更扫描，未改动手动 F13 键位");
        }
        if !cleanup_warnings.is_empty() {
            status.message.push_str(&format!(
                "；键位已恢复，但保留了部分无法删除的安全备份：{}",
                cleanup_warnings.join("；")
            ));
        }
        Ok(status)
    }

    pub(crate) fn status(&self) -> Result<ChatF13BindingStatus, String> {
        let _operation = lock(&self.inner.operation);
        validate_configured_directories(&self.inner.directories)?;
        let snapshot = inspect_filesystem(&self.inner.directories)?;
        Ok(build_status(&self.inner, &snapshot))
    }
}

fn preflight_install(snapshot: &FilesystemSnapshot) -> Result<(), String> {
    if snapshot.files.is_empty() {
        return Err("存档目录中没有找到 .key/.keyo 键位文件".to_string());
    }
    if !snapshot.transaction_artifacts.is_empty() {
        return Err(format!(
            "发现未恢复的键位写入事务：{}",
            display_paths(&snapshot.transaction_artifacts)
        ));
    }
    let problems: Vec<String> = snapshot
        .files
        .iter()
        .filter_map(|file| {
            file.install_problem()
                .map(|error| format!("{}：{error}", file.path.display()))
        })
        .collect();
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "F13 安装预检失败，未修改任何键位文件：{}",
            problems.join("；")
        ))
    }
}

fn require_scan_ready(snapshot: &FilesystemSnapshot) -> Result<(), String> {
    if snapshot.ready() {
        return Ok(());
    }
    let problems: Vec<String> = snapshot
        .files
        .iter()
        .filter_map(|file| {
            file.state
                .as_ref()
                .err()
                .map(|error| format!("{}：{error}", file.path.display()))
        })
        .collect();
    if snapshot.files.is_empty() {
        Err("没有可验证的 .key/.keyo 键位文件，F13 扫描未完成".to_string())
    } else if !snapshot.transaction_artifacts.is_empty() {
        Err(format!(
            "仍有未完成的键位写入事务，F13 扫描未完成：{}",
            display_paths(&snapshot.transaction_artifacts)
        ))
    } else {
        Err(format!(
            "F13 键位或备份未通过扫描校验：{}",
            if problems.is_empty() {
                "并非所有键位文件都已安装且可恢复".to_string()
            } else {
                problems.join("；")
            }
        ))
    }
}

fn build_status(inner: &ServiceInner, snapshot: &FilesystemSnapshot) -> ChatF13BindingStatus {
    let total_files = snapshot.files.len();
    let installed_files = snapshot
        .files
        .iter()
        .filter(|file| file.state == Ok(BindingState::Installed))
        .count();
    let eligible_files = snapshot
        .files
        .iter()
        .filter(|file| file.state == Ok(BindingState::Eligible))
        .count();
    let backup_files = snapshot
        .files
        .iter()
        .filter(|file| file.has_valid_backup())
        .count();
    let orphan_backup_files = snapshot.orphan_backups.len();
    let conflicted_files = snapshot.conflict_count();
    let transaction_artifacts = snapshot.transaction_artifacts.len();
    let ready = snapshot.ready();
    let consent_granted = inner.consent_granted.load(Ordering::Acquire);
    let watcher_running = false;
    let d2r_running = probe_d2r_running(inner);
    let mut message = if ready {
        format!("F13 原生 Chat 备用键已验证：{installed_files}/{total_files} 个键位文件")
    } else if total_files == 0 {
        "存档目录中没有找到 .key/.keyo 键位文件".to_string()
    } else if transaction_artifacts > 0 {
        format!("发现 {transaction_artifacts} 个待恢复的安全写入事务")
    } else if conflicted_files > 0 {
        format!("发现 {conflicted_files} 个格式、键位、备份或路径冲突；未自动覆盖")
    } else {
        format!("尚有 {eligible_files}/{total_files} 个键位文件可以安全安装 F13")
    };
    if d2r_running {
        message.push_str("；D2R 正在运行，扫描补装或恢复前必须全部关闭");
    }
    if consent_granted {
        message.push_str("；自动跟房配置变更时会扫描一次，新建角色后请手动扫描");
    }
    let managed_installed_files = snapshot
        .files
        .iter()
        .filter(|file| file.state == Ok(BindingState::Installed) && file.has_valid_backup())
        .count();
    let externally_managed = installed_files.saturating_sub(managed_installed_files);
    if externally_managed > 0 {
        message.push_str(&format!(
            "；其中 {externally_managed} 个 F13 键位由用户或其他工具管理，不会由 D2RHub 恢复"
        ));
    }
    if orphan_backup_files > 0 {
        message.push_str(&format!(
            "；保留 {orphan_backup_files} 个已删除角色的历史备份，不影响当前角色"
        ));
    }

    ChatF13BindingStatus {
        ready,
        total_files,
        installed_files,
        eligible_files,
        conflicted_files,
        backup_files,
        orphan_backup_files,
        transaction_artifacts,
        d2r_running,
        consent_granted,
        watcher_running,
        auto_patch_enabled: consent_granted,
        directories: inner
            .directories
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        last_watcher_error: None,
        message,
    }
}

fn inspect_filesystem(directories: &[PathBuf]) -> Result<FilesystemSnapshot, String> {
    validate_configured_directories(directories)?;
    let mut snapshot = FilesystemSnapshot::default();
    let mut key_files = Vec::new();
    let mut backup_targets = HashMap::<PathBuf, PathBuf>::new();

    for directory in directories {
        for entry in std::fs::read_dir(directory)
            .map_err(|error| format!("无法读取存档目录 {}：{error}", directory.display()))?
        {
            let entry = entry
                .map_err(|error| format!("读取存档目录项失败 {}：{error}", directory.display()))?;
            let path = entry.path();
            let Some(kind) = relevant_path_kind(&path) else {
                continue;
            };
            match kind {
                RelevantPathKind::Key => {
                    validate_regular_file(&path)?;
                    key_files.push(path);
                }
                RelevantPathKind::Backup => {
                    let target = target_from_suffixed_path(&path, BACKUP_SUFFIX)?;
                    if !is_key_file(&target) {
                        snapshot.orphan_backups.push(path);
                        continue;
                    }
                    backup_targets.insert(target, path);
                }
                RelevantPathKind::Transaction => {
                    validate_regular_file(&path)?;
                    snapshot.transaction_artifacts.push(path);
                }
            }
        }
    }
    key_files.sort();
    key_files.dedup();
    snapshot.transaction_artifacts.sort();
    snapshot.transaction_artifacts.dedup();

    let live: HashSet<PathBuf> = key_files.iter().cloned().collect();
    for (target, backup) in &backup_targets {
        if !live.contains(target) {
            snapshot.orphan_backups.push(backup.clone());
        }
    }
    snapshot.orphan_backups.sort();

    for path in key_files {
        let bytes = read_regular_file(&path)?;
        let state = inspect_key_bytes(&bytes);
        let backup = backup_targets
            .remove(&path)
            .map(|backup| read_regular_file(&backup));
        snapshot.files.push(InspectedFile {
            path,
            bytes,
            state,
            backup,
        });
    }
    Ok(snapshot)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelevantPathKind {
    Key,
    Backup,
    Transaction,
}

fn relevant_path_kind(path: &Path) -> Option<RelevantPathKind> {
    if is_key_file(path) {
        return Some(RelevantPathKind::Key);
    }
    let name = path.file_name()?.to_str()?;
    if name.ends_with(BACKUP_SUFFIX) {
        Some(RelevantPathKind::Backup)
    } else if [STAGING_SUFFIX, ROLLBACK_SUFFIX, JOURNAL_SUFFIX]
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        Some(RelevantPathKind::Transaction)
    } else {
        None
    }
}

fn rollback_changed_files(changed: &[(&Path, Vec<u8>)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (path, original) in changed.iter().rev() {
        match read_regular_file(path)
            .and_then(|current| atomic_replace_bytes(path, &current, original))
        {
            Ok(()) => {}
            Err(error) => errors.push(format!("{}：{error}", path.display())),
        }
    }
    errors
}

fn recover_interrupted_transactions(directories: &[PathBuf]) -> Result<(), String> {
    validate_configured_directories(directories)?;
    let mut targets = HashSet::new();
    for directory in directories {
        for entry in std::fs::read_dir(directory)
            .map_err(|error| format!("无法读取存档目录 {}：{error}", directory.display()))?
        {
            let entry = entry
                .map_err(|error| format!("读取存档目录项失败 {}：{error}", directory.display()))?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let suffix = [STAGING_SUFFIX, ROLLBACK_SUFFIX, JOURNAL_SUFFIX]
                .into_iter()
                .find(|suffix| name.ends_with(suffix));
            if let Some(suffix) = suffix {
                validate_regular_file(&path)?;
                let target = target_from_suffixed_path(&path, suffix)?;
                if !is_key_file(&target) {
                    return Err(format!("F13 安全写入事务文件名无效：{}", path.display()));
                }
                targets.insert(target);
            }
        }
    }
    let mut targets: Vec<PathBuf> = targets.into_iter().collect();
    targets.sort();
    for target in targets {
        recover_interrupted_transaction(&target)?;
    }
    Ok(())
}

fn recover_interrupted_transaction(target: &Path) -> Result<(), String> {
    let stage = sibling_with_suffix(target, STAGING_SUFFIX)?;
    let rollback = sibling_with_suffix(target, ROLLBACK_SUFFIX)?;
    let journal_path = sibling_with_suffix(target, JOURNAL_SUFFIX)?;
    let target_exists = path_exists_no_follow(target)?;
    let stage_exists = path_exists_no_follow(&stage)?;
    let rollback_exists = path_exists_no_follow(&rollback)?;
    let journal_exists = path_exists_no_follow(&journal_path)?;

    if !journal_exists {
        if rollback_exists {
            return Err(format!(
                "发现无事务日志的回滚文件，拒绝猜测恢复：{}",
                rollback.display()
            ));
        }
        if stage_exists {
            if !target_exists {
                return Err(format!(
                    "原键位文件缺失且只有未提交 staging，拒绝提升：{}",
                    stage.display()
                ));
            }
            remove_regular_if_exists(&stage)?;
            sync_parent(target)?;
        }
        return Ok(());
    }

    let journal_bytes = read_regular_file(&journal_path)?;
    let journal: ReplaceJournal = match serde_json::from_slice(&journal_bytes) {
        Ok(journal) => journal,
        Err(_error) if target_exists && !rollback_exists => {
            // The target is renamed only after the complete journal has been
            // synced. A partial journal with the original still present is
            // therefore safely before the commit point.
            if stage_exists {
                remove_regular_if_exists(&stage)?;
            }
            remove_regular_if_exists(&journal_path)?;
            sync_parent(target)?;
            return Ok(());
        }
        Err(error) => {
            return Err(format!(
                "F13 安全写入事务日志损坏且无法安全判定提交点 {}：{error}",
                journal_path.display()
            ));
        }
    };
    validate_replace_journal(target, &journal)?;

    if target_exists {
        let current = read_regular_file(target)?;
        let current_checksum = checksum(&current);
        if current_checksum != journal.original_checksum
            && current_checksum != journal.replacement_checksum
        {
            return Err(format!(
                "键位文件在未完成事务外又发生变化，拒绝覆盖：{}",
                target.display()
            ));
        }
        if rollback_exists {
            let rollback_bytes = read_regular_file(&rollback)?;
            if checksum(&rollback_bytes) != journal.original_checksum {
                return Err(format!("回滚文件校验失败：{}", rollback.display()));
            }
            remove_regular_if_exists(&rollback)?;
        }
        if stage_exists {
            let stage_bytes = read_regular_file(&stage)?;
            if checksum(&stage_bytes) != journal.replacement_checksum {
                crate::logger::log_msg(
                    "WARN",
                    "RoomChatBinding",
                    &format!(
                        "已提交键位文件旁的 staging 校验失败，将其作为不可提交事务残留清理：{}",
                        stage.display()
                    ),
                );
            }
            remove_regular_if_exists(&stage)?;
        }
        remove_regular_if_exists(&journal_path)?;
        sync_parent(target)?;
        return Ok(());
    }

    if !rollback_exists {
        return Err(format!(
            "原键位文件和回滚文件均缺失，拒绝把 staging 当作原文件：{}",
            target.display()
        ));
    }
    let rollback_bytes = read_regular_file(&rollback)?;
    if checksum(&rollback_bytes) != journal.original_checksum {
        return Err(format!("回滚文件校验失败：{}", rollback.display()));
    }
    // With the target missing, rollback is the only authoritative copy. A
    // staging file is an uncommitted candidate and must never prevent restoring
    // the checksum-verified original, even if that candidate was torn by a
    // crash. Validate it as a regular owned artifact, then discard it first so
    // another interruption still leaves an unambiguous rollback state.
    if stage_exists {
        let stage_bytes = read_regular_file(&stage)?;
        if checksum(&stage_bytes) != journal.replacement_checksum {
            crate::logger::log_msg(
                "WARN",
                "RoomChatBinding",
                &format!(
                    "未提交的 F13 staging 校验失败，正在恢复原键位并清理残留：{}",
                    stage.display()
                ),
            );
        }
        remove_regular_if_exists(&stage)?;
        sync_parent(target)?;
    }
    durable_fs::durable_sibling_rename(&rollback, target).map_err(|error| {
        format!(
            "无法恢复中断写入的原键位 {} -> {}：{error}",
            rollback.display(),
            target.display()
        )
    })?;
    remove_regular_if_exists(&journal_path)?;
    sync_parent(target)
}

fn atomic_replace_bytes(
    target: &Path,
    expected_original: &[u8],
    replacement: &[u8],
) -> Result<(), String> {
    validate_regular_file(target)?;
    recover_interrupted_transaction(target)?;
    let current = read_regular_file(target)?;
    if current != expected_original {
        return Err(format!(
            "键位文件在预检后发生变化，拒绝覆盖：{}",
            target.display()
        ));
    }
    if replacement == expected_original {
        return Ok(());
    }

    let stage = sibling_with_suffix(target, STAGING_SUFFIX)?;
    let rollback = sibling_with_suffix(target, ROLLBACK_SUFFIX)?;
    let journal_path = sibling_with_suffix(target, JOURNAL_SUFFIX)?;
    if path_exists_no_follow(&stage)?
        || path_exists_no_follow(&rollback)?
        || path_exists_no_follow(&journal_path)?
    {
        return Err(format!("键位文件仍有未清理事务：{}", target.display()));
    }

    create_synced_new_file(&stage, replacement)?;
    let journal = ReplaceJournal {
        version: REPLACE_JOURNAL_VERSION,
        target_file_name: target
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("键位文件名不是有效 Unicode：{}", target.display()))?
            .to_string(),
        original_checksum: checksum(expected_original),
        replacement_checksum: checksum(replacement),
    };
    let journal_bytes = serde_json::to_vec(&journal)
        .map_err(|error| format!("无法序列化 F13 安全写入事务：{error}"))?;
    if let Err(error) = create_synced_new_file(&journal_path, &journal_bytes) {
        let _ = remove_regular_if_exists(&stage);
        return Err(error);
    }
    sync_parent(target)?;

    if let Err(error) = durable_fs::durable_sibling_rename(target, &rollback) {
        let _ = remove_regular_if_exists(&journal_path);
        let _ = remove_regular_if_exists(&stage);
        return Err(format!(
            "无法把原键位移入可恢复位置 {}：{error}",
            rollback.display()
        ));
    }
    sync_parent(target)?;
    if let Err(install_error) = durable_fs::durable_sibling_rename(&stage, target) {
        return match durable_fs::durable_sibling_rename(&rollback, target) {
            Ok(()) => {
                let _ = remove_regular_if_exists(&journal_path);
                let _ = sync_parent(target);
                Err(format!("安装 staged 键位失败，已恢复原文件：{install_error}"))
            }
            Err(rollback_error) => Err(format!(
                "安装 staged 键位失败且自动恢复失败；原文件仍在 {}。安装错误：{install_error}；恢复错误：{rollback_error}",
                rollback.display()
            )),
        };
    }
    sync_parent(target)?;

    let verified = read_regular_file(target)?;
    if verified != replacement {
        return Err(format!(
            "键位 staged 替换后的内容复核失败，回滚数据保留在 {}",
            rollback.display()
        ));
    }
    remove_regular_if_exists(&rollback)?;
    remove_regular_if_exists(&journal_path)?;
    sync_parent(target)
}

fn validate_replace_journal(target: &Path, journal: &ReplaceJournal) -> Result<(), String> {
    if journal.version != REPLACE_JOURNAL_VERSION {
        return Err(format!(
            "不支持的 F13 安全写入事务版本：{}",
            journal.version
        ));
    }
    let expected = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("键位文件名不是有效 Unicode：{}", target.display()))?;
    if journal.target_file_name != expected {
        return Err(format!(
            "F13 安全写入事务目标不匹配：期望 {expected}，实际 {}",
            journal.target_file_name
        ));
    }
    Ok(())
}

fn create_backup(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if inspect_key_bytes(bytes)? != BindingState::Eligible {
        return Err(format!(
            "拒绝备份不可替换的 Chat 第二键位状态：{}",
            path.display()
        ));
    }
    let backup = backup_path(path)?;
    if path_exists_no_follow(&backup)? {
        let existing = read_regular_file(&backup)?;
        if inspect_key_bytes(&existing)? != BindingState::Eligible {
            return Err(format!("已有备份不可安全恢复：{}", backup.display()));
        }
        return Ok(());
    }
    create_synced_new_file(&backup, bytes)?;
    sync_parent(path)
}

fn create_synced_new_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    validate_safe_parent(path)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("无法创建安全写入文件 {}：{error}", path.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        return Err(format!("无法写入并同步 {}：{error}", path.display()));
    }
    drop(file);
    validate_regular_file(path)
}

fn inspect_key_bytes(bytes: &[u8]) -> Result<BindingState, String> {
    if read_u16(bytes, 0)? != KEY_FILE_VERSION {
        return Err("不是已验证的 D2R v37 键位格式".to_string());
    }
    if bytes.len() <= CHAT_RECORD_OFFSET
        || !(bytes.len() - KEY_FILE_HEADER_SIZE).is_multiple_of(KEY_RECORD_SIZE)
    {
        return Err("键位文件结构或长度不符合 v37 记录布局".to_string());
    }

    let primary_id = read_u32(bytes, CHAT_RECORD_OFFSET)?;
    let primary_key = read_u16(bytes, CHAT_RECORD_OFFSET + 4)?;
    let primary_type = read_u32(bytes, CHAT_RECORD_OFFSET + 6)?;
    let secondary_id = read_u32(bytes, SECONDARY_SLOT_OFFSET)?;
    let secondary_key = read_u16(bytes, SECONDARY_SLOT_OFFSET + 4)?;
    let secondary_type = read_u32(bytes, SECONDARY_SLOT_OFFSET + 6)?;
    if primary_id != CHAT_ACTION_ID
        || primary_key != VK_RETURN
        || primary_type != BOUND_KEY_TYPE
        || secondary_id != CHAT_ACTION_ID
    {
        return Err("动作 5 不是“Enter 主键 + 同动作次键”的原生 Chat 记录".to_string());
    }

    let record_count = (bytes.len() - KEY_FILE_HEADER_SIZE) / KEY_RECORD_SIZE;
    for record in 0..record_count {
        let record_offset = KEY_FILE_HEADER_SIZE + record * KEY_RECORD_SIZE;
        for slot in 0..2 {
            if record == CHAT_ACTION_INDEX && slot == 1 {
                continue;
            }
            let slot_offset = record_offset + slot * 10;
            let key = read_u16(bytes, slot_offset + 4)?;
            let key_type = read_u32(bytes, slot_offset + 6)?;
            if key == VK_F13 && key_type != UNBOUND_KEY_TYPE {
                return Err(format!("F13 已被动作 {record} 的第 {} 键位占用", slot + 1));
            }
        }
    }

    match (secondary_key, secondary_type) {
        (VK_F13, BOUND_KEY_TYPE) => Ok(BindingState::Installed),
        // Cloud-created v37 files sometimes keep a stale raw key value while
        // type 0 still means unbound. The full slot is retained in the backup.
        (_, UNBOUND_KEY_TYPE) => Ok(BindingState::Eligible),
        // A user-defined secondary Chat key is also replaceable. Installation
        // backs up the complete slot before assigning F13, so restore can put
        // the user's original key back exactly.
        (_, BOUND_KEY_TYPE) => Ok(BindingState::Eligible),
        _ => Err(format!(
            "无法安全识别原生 Chat 第二键位（VK=0x{secondary_key:04X}，类型={secondary_type}）"
        )),
    }
}

fn patch_f13(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if inspect_key_bytes(bytes)? != BindingState::Eligible {
        return Err("该键位文件不是可安装状态".to_string());
    }
    let mut patched = bytes.to_vec();
    write_u32(&mut patched, SECONDARY_SLOT_OFFSET, CHAT_ACTION_ID);
    write_u16(&mut patched, SECONDARY_SLOT_OFFSET + 4, VK_F13);
    write_u32(&mut patched, SECONDARY_SLOT_OFFSET + 6, BOUND_KEY_TYPE);
    if inspect_key_bytes(&patched)? != BindingState::Installed {
        return Err("F13 键位写入后的结构校验失败".to_string());
    }
    Ok(patched)
}

fn restore_chat_secondary_slot(current: &[u8], backup: &[u8]) -> Result<Vec<u8>, String> {
    if !matches!(
        inspect_key_bytes(current)?,
        BindingState::Installed | BindingState::Eligible
    ) {
        return Err("当前键位文件不是可恢复状态".to_string());
    }
    if inspect_key_bytes(backup)? != BindingState::Eligible {
        return Err("备份中的原生 Chat 第二键位不可安全恢复".to_string());
    }
    let mut restored = current.to_vec();
    restored[SECONDARY_SLOT_OFFSET..SECONDARY_SLOT_OFFSET + 10]
        .copy_from_slice(&backup[SECONDARY_SLOT_OFFSET..SECONDARY_SLOT_OFFSET + 10]);
    if inspect_key_bytes(&restored)? != BindingState::Eligible {
        return Err("恢复后的键位结构校验失败".to_string());
    }
    Ok(restored)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "键位文件长度不足".to_string())?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "键位文件长度不足".to_string())?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn backup_path(path: &Path) -> Result<PathBuf, String> {
    sibling_with_suffix(path, BACKUP_SUFFIX)
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("文件名不是有效 Unicode：{}", path.display()))?;
    Ok(path.with_file_name(format!("{name}{suffix}")))
}

fn target_from_suffixed_path(path: &Path, suffix: &str) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("事务文件名不是有效 Unicode：{}", path.display()))?;
    let target_name = name
        .strip_suffix(suffix)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("事务文件后缀不匹配：{}", path.display()))?;
    Ok(path.with_file_name(target_name))
}

fn is_key_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("key") || value.eq_ignore_ascii_case("keyo")
        })
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, String> {
    validate_regular_file(path)?;
    let bytes = std::fs::read(path)
        .map_err(|error| format!("无法读取普通文件 {}：{error}", path.display()))?;
    validate_regular_file(path)?;
    Ok(bytes)
}

fn validate_regular_file(path: &Path) -> Result<(), String> {
    validate_safe_parent(path)?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("无法读取文件元数据 {}：{error}", path.display()))?;
    reject_link_or_reparse(path, &metadata)?;
    if !metadata.file_type().is_file() {
        return Err(format!("路径不是普通文件，拒绝访问：{}", path.display()));
    }
    Ok(())
}

fn validate_safe_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("路径缺少父目录：{}", path.display()))?;
    let canonical_parent = validate_directory_path(parent)?;
    if !same_path(parent, &canonical_parent) {
        return Err(format!(
            "文件父目录不是已解析的真实目录，拒绝访问：{}",
            parent.display()
        ));
    }
    Ok(())
}

pub(crate) fn validate_and_canonicalize_directories(
    directories: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, String> {
    if directories.is_empty() {
        return Err("没有配置 D2R 存档目录".to_string());
    }
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    for directory in directories {
        if !directory.is_absolute() {
            return Err(format!(
                "D2R 存档目录必须是绝对路径：{}",
                directory.display()
            ));
        }
        let canonical = validate_directory_path(&directory)?;
        let identity = path_identity(&canonical);
        if seen.insert(identity) {
            resolved.push(canonical);
        }
    }
    if resolved.is_empty() {
        return Err("没有可用的 D2R 存档目录".to_string());
    }
    resolved.sort();
    Ok(resolved)
}

fn validate_configured_directories(directories: &[PathBuf]) -> Result<(), String> {
    if directories.is_empty() {
        return Err("没有配置 D2R 存档目录".to_string());
    }
    for directory in directories {
        let current = validate_directory_path(directory)?;
        if !same_path(directory, &current) {
            return Err(format!(
                "D2R 存档目录解析结果发生变化，拒绝继续：{}",
                directory.display()
            ));
        }
    }
    Ok(())
}

fn validate_directory_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("目录必须是绝对路径：{}", path.display()));
    }
    reject_link_ancestors(path)?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("无法读取目录元数据 {}：{error}", path.display()))?;
    reject_link_or_reparse(path, &metadata)?;
    if !metadata.file_type().is_dir() {
        return Err(format!("路径不是目录：{}", path.display()));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("无法解析目录 {}：{error}", path.display()))?;
    reject_link_ancestors(&canonical)?;
    Ok(canonical)
}

fn reject_link_ancestors(path: &Path) -> Result<(), String> {
    for ancestor in path
        .ancestors()
        .filter(|value| !value.as_os_str().is_empty())
    {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) => reject_link_or_reparse(ancestor, &metadata)?,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!("无法验证路径层级 {}：{error}", ancestor.display()));
            }
        }
    }
    Ok(())
}

fn reject_link_or_reparse(path: &Path, metadata: &Metadata) -> Result<(), String> {
    if metadata.file_type().is_symlink() || is_reparse_point(metadata) {
        Err(format!(
            "拒绝访问符号链接或 Windows reparse 路径：{}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &Metadata) -> bool {
    false
}

fn path_exists_no_follow(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            reject_link_or_reparse(path, &metadata)?;
            if !metadata.file_type().is_file() {
                return Err(format!("事务路径不是普通文件：{}", path.display()));
            }
            validate_safe_parent(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("无法检查路径 {}：{error}", path.display())),
    }
}

fn remove_regular_if_exists(path: &Path) -> Result<(), String> {
    if !path_exists_no_follow(path)? {
        return Ok(());
    }
    std::fs::remove_file(path).map_err(|error| format!("无法删除文件 {}：{error}", path.display()))
}

fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("路径缺少父目录：{}", path.display()))?;
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    durable_fs::sync_directory(path)
        .map_err(|error| format!("无法同步目录元数据 {}：{error}", path.display()))
}

fn ensure_d2r_closed(inner: &ServiceInner) -> Result<(), String> {
    if probe_d2r_running(inner) {
        Err("请先关闭全部 D2R 窗口；游戏运行时会缓存并覆盖 .key/.keyo 键位文件".to_string())
    } else {
        Ok(())
    }
}

fn probe_d2r_running(inner: &ServiceInner) -> bool {
    // A broken platform probe must fail closed instead of unwinding through a
    // capability lifecycle call and accidentally allowing writes.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (inner.d2r_running)())).unwrap_or(true)
}

fn checksum(bytes: &[u8]) -> u64 {
    // FNV-1a is not an authenticity primitive; it is a compact crash journal
    // discriminator that prevents guessing which side of a rename committed.
    let mut value = 0xcbf29ce484222325u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    value
}

#[cfg(windows)]
fn path_identity(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

#[cfg(not(windows))]
fn path_identity(path: &Path) -> String {
    path.to_string_lossy().trim_end_matches('/').to_string()
}

fn same_path(left: &Path, right: &Path) -> bool {
    path_identity(left) == path_identity(right)
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("；")
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "d2rhub_room_chat_binding_{label}_{}",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn key(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn service(directory: &TestDirectory) -> ChatF13BindingService {
        ChatF13BindingService::new(vec![directory.path().to_path_buf()], || false).unwrap()
    }

    #[test]
    fn service_is_send_and_sync_for_capability_ownership() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ChatF13BindingService>();
    }

    fn sample_key_file() -> Vec<u8> {
        let mut bytes = vec![0u8; KEY_FILE_HEADER_SIZE + 61 * KEY_RECORD_SIZE];
        write_u16(&mut bytes, 0, KEY_FILE_VERSION);
        for record in 0..61usize {
            let offset = KEY_FILE_HEADER_SIZE + record * KEY_RECORD_SIZE;
            write_u32(&mut bytes, offset, record as u32);
            write_u16(&mut bytes, offset + 4, UNBOUND_KEY);
            write_u32(&mut bytes, offset + 6, UNBOUND_KEY_TYPE);
            write_u32(&mut bytes, offset + 10, record as u32);
            write_u16(&mut bytes, offset + 14, UNBOUND_KEY);
            write_u32(&mut bytes, offset + 16, UNBOUND_KEY_TYPE);
        }
        write_u16(&mut bytes, CHAT_RECORD_OFFSET + 4, VK_RETURN);
        write_u32(&mut bytes, CHAT_RECORD_OFFSET + 6, BOUND_KEY_TYPE);
        bytes
    }

    #[test]
    fn patches_only_native_chat_secondary_slot() {
        let original = sample_key_file();
        let patched = patch_f13(&original).unwrap();
        assert_eq!(inspect_key_bytes(&patched), Ok(BindingState::Installed));
        let changed: Vec<usize> = original
            .iter()
            .zip(&patched)
            .enumerate()
            .filter_map(|(index, (before, after))| (before != after).then_some(index))
            .collect();
        assert_eq!(
            changed,
            vec![
                SECONDARY_SLOT_OFFSET + 4,
                SECONDARY_SLOT_OFFSET + 5,
                SECONDARY_SLOT_OFFSET + 6,
            ]
        );
    }

    #[test]
    fn rejects_wrong_version_and_an_existing_f13_collision() {
        let mut wrong_version = sample_key_file();
        write_u16(&mut wrong_version, 0, 36);
        assert!(inspect_key_bytes(&wrong_version)
            .unwrap_err()
            .contains("v37"));

        let mut collision = sample_key_file();
        let other_slot = KEY_FILE_HEADER_SIZE + 9 * KEY_RECORD_SIZE;
        write_u16(&mut collision, other_slot + 4, VK_F13);
        write_u32(&mut collision, other_slot + 6, BOUND_KEY_TYPE);
        assert!(inspect_key_bytes(&collision)
            .unwrap_err()
            .contains("动作 9"));
    }

    #[test]
    fn stale_unbound_raw_value_round_trips_through_restore() {
        let mut original = sample_key_file();
        write_u16(&mut original, SECONDARY_SLOT_OFFSET + 4, 0x1070);
        assert_eq!(inspect_key_bytes(&original), Ok(BindingState::Eligible));
        let restored =
            restore_chat_secondary_slot(&patch_f13(&original).unwrap(), &original).unwrap();
        assert_eq!(
            read_u16(&restored, SECONDARY_SLOT_OFFSET + 4).unwrap(),
            0x1070
        );
        assert_eq!(
            read_u32(&restored, SECONDARY_SLOT_OFFSET + 6).unwrap(),
            UNBOUND_KEY_TYPE
        );
    }

    #[test]
    fn a_new_service_has_no_consent_and_no_watcher() {
        let directory = TestDirectory::new("default_off");
        directory.key("hero.keyo", &sample_key_file());
        let service = service(&directory);
        let status = service.status().unwrap();
        assert!(!status.consent_granted);
        assert!(!status.watcher_running);
        assert!(!status.auto_patch_enabled);
        assert!(!status.ready);
    }

    #[test]
    fn install_is_explicit_and_backs_up_without_starting_a_watcher() {
        let directory = TestDirectory::new("install");
        let path = directory.key("hero.keyo", &sample_key_file());
        let service = service(&directory);
        let status = service.install().unwrap();
        assert!(status.ready);
        assert!(status.consent_granted);
        assert!(!status.watcher_running);
        assert_eq!(status.backup_files, 1);
        assert_eq!(
            inspect_key_bytes(&std::fs::read(path).unwrap()),
            Ok(BindingState::Installed)
        );
        let stopped = service.stop().unwrap();
        assert!(!stopped.watcher_running);
        assert!(stopped.consent_granted);
    }

    #[test]
    fn install_while_d2r_runs_changes_nothing() {
        let directory = TestDirectory::new("running");
        let original = sample_key_file();
        let path = directory.key("hero.key", &original);
        let service =
            ChatF13BindingService::new(vec![directory.path().to_path_buf()], || true).unwrap();
        assert!(service.install().unwrap_err().contains("关闭全部 D2R"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!backup_path(&path).unwrap().exists());
        assert!(!service.status().unwrap().consent_granted);
    }

    #[test]
    fn a_panicking_process_probe_fails_closed() {
        let directory = TestDirectory::new("probe_panic");
        let original = sample_key_file();
        let path = directory.key("hero.key", &original);
        let service =
            ChatF13BindingService::new(vec![directory.path().to_path_buf()], || -> bool {
                panic!("probe failed")
            })
            .unwrap();
        assert!(service.install().unwrap_err().contains("关闭全部 D2R"));
        assert_eq!(std::fs::read(path).unwrap(), original);
    }

    #[test]
    fn install_preflight_is_all_or_nothing_for_conflicts() {
        let directory = TestDirectory::new("preflight");
        let original = sample_key_file();
        let first = directory.key("first.key", &original);
        let mut collision = sample_key_file();
        let other_slot = KEY_FILE_HEADER_SIZE + 9 * KEY_RECORD_SIZE;
        write_u16(&mut collision, other_slot + 4, VK_F13);
        write_u32(&mut collision, other_slot + 6, BOUND_KEY_TYPE);
        directory.key("second.keyo", &collision);
        let service = service(&directory);
        assert!(service.install().unwrap_err().contains("预检失败"));
        assert_eq!(std::fs::read(&first).unwrap(), original);
        assert!(!backup_path(&first).unwrap().exists());
    }

    #[test]
    fn an_existing_invalid_backup_is_never_overwritten() {
        let directory = TestDirectory::new("backup_no_overwrite");
        let path = directory.key("hero.key", &sample_key_file());
        let backup = backup_path(&path).unwrap();
        let sentinel = b"do-not-overwrite".to_vec();
        std::fs::write(&backup, &sentinel).unwrap();
        let service = service(&directory);
        assert!(service.install().is_err());
        assert_eq!(std::fs::read(backup).unwrap(), sentinel);
        assert_eq!(
            inspect_key_bytes(&std::fs::read(path).unwrap()),
            Ok(BindingState::Eligible)
        );
    }

    #[test]
    fn explicit_rescan_patches_new_files_and_stop_leaves_later_files_unchanged() {
        let directory = TestDirectory::new("watcher_stop");
        directory.key("existing.key", &sample_key_file());
        let service = service(&directory);
        service.install().unwrap();

        let new_file = directory.key("new.keyo", &sample_key_file());
        assert_eq!(
            inspect_key_bytes(&std::fs::read(&new_file).unwrap()),
            Ok(BindingState::Eligible)
        );
        let rescanned = service.install().unwrap();
        assert!(rescanned.ready);
        assert_eq!(
            inspect_key_bytes(&std::fs::read(&new_file).unwrap()),
            Ok(BindingState::Installed)
        );
        service.stop().unwrap();

        let after_stop = directory.key("after-stop.keyo", &sample_key_file());
        assert_eq!(
            inspect_key_bytes(&std::fs::read(after_stop).unwrap()),
            Ok(BindingState::Eligible)
        );
    }

    #[test]
    fn restore_stops_watcher_and_preserves_unrelated_edits() {
        let directory = TestDirectory::new("restore_merge");
        let path = directory.key("hero.key", &sample_key_file());
        let service = service(&directory);
        service.install().unwrap();

        let mut current = std::fs::read(&path).unwrap();
        let unrelated = KEY_FILE_HEADER_SIZE + 12 * KEY_RECORD_SIZE;
        write_u16(&mut current, unrelated + 4, 0x41);
        write_u32(&mut current, unrelated + 6, BOUND_KEY_TYPE);
        std::fs::write(&path, &current).unwrap();

        let status = service.restore().unwrap();
        let restored = std::fs::read(&path).unwrap();
        assert_eq!(inspect_key_bytes(&restored), Ok(BindingState::Eligible));
        assert_eq!(read_u16(&restored, unrelated + 4).unwrap(), 0x41);
        assert!(!status.watcher_running);
        assert!(!status.consent_granted);
        assert!(!backup_path(&path).unwrap().exists());
    }

    #[test]
    fn explicit_persisted_consent_accepts_a_manually_installed_f13_without_a_watcher() {
        let directory = TestDirectory::new("persisted_requires_backup");
        let patched = patch_f13(&sample_key_file()).unwrap();
        directory.key("hero.key", &patched);
        let service = service(&directory);
        let consent = ExplicitChatBindingConsent::from_persisted_user_consent(true).unwrap();
        let status = service.start_watcher_with_consent(consent).unwrap();
        assert!(status.ready);
        assert_eq!(status.backup_files, 0);
        assert!(status.consent_granted);
        assert!(!status.watcher_running);
        assert!(status.message.contains("其他工具管理"));
        service.stop().unwrap();
    }

    #[test]
    fn orphan_backup_is_reported_but_never_blocks_live_files() {
        let directory = TestDirectory::new("orphan_warning");
        let live = directory.key("hero.key", &sample_key_file());
        let orphan = backup_path(&directory.path().join("deleted.keyo")).unwrap();
        std::fs::write(&orphan, b"opaque orphan retained verbatim").unwrap();
        let service = service(&directory);

        let installed = service.install().unwrap();
        assert!(installed.ready);
        assert_eq!(installed.orphan_backup_files, 1);
        assert_eq!(installed.conflicted_files, 0);
        assert!(orphan.exists());

        let restored = service.restore().unwrap();
        assert_eq!(
            inspect_key_bytes(&std::fs::read(live).unwrap()),
            Ok(BindingState::Eligible)
        );
        assert_eq!(restored.orphan_backup_files, 1);
        assert!(orphan.exists());
    }

    #[test]
    fn repeat_install_is_a_safe_scan_and_rejected_restore_preserves_consent() {
        let directory = TestDirectory::new("failed_repeat_keeps_watcher");
        directory.key("hero.key", &sample_key_file());
        let d2r_running = Arc::new(AtomicBool::new(false));
        let probe = Arc::clone(&d2r_running);
        let service = ChatF13BindingService::new(vec![directory.path().to_path_buf()], move || {
            probe.load(Ordering::Acquire)
        })
        .unwrap();
        service.install().unwrap();

        d2r_running.store(true, Ordering::Release);
        let after_install = service.install().unwrap();
        assert!(after_install.consent_granted);
        assert!(!after_install.watcher_running);

        assert!(service.restore().unwrap_err().contains("关闭全部 D2R"));
        let after_restore = service.status().unwrap();
        assert!(after_restore.consent_granted);
        assert!(!after_restore.watcher_running);

        d2r_running.store(false, Ordering::Release);
        service.stop().unwrap();
    }

    #[test]
    fn restore_without_owned_backup_only_disables_watcher() {
        let directory = TestDirectory::new("manual_restore");
        let path = directory.key("hero.key", &patch_f13(&sample_key_file()).unwrap());
        let service = service(&directory);
        let consent = ExplicitChatBindingConsent::from_persisted_user_consent(true).unwrap();
        service.start_watcher_with_consent(consent).unwrap();

        let status = service.restore().unwrap();
        assert!(status.ready);
        assert!(!status.consent_granted);
        assert!(!status.watcher_running);
        assert!(status.message.contains("未改动手动 F13"));
        assert_eq!(
            inspect_key_bytes(&std::fs::read(path).unwrap()),
            Ok(BindingState::Installed)
        );
    }

    #[test]
    fn persisted_explicit_consent_resumes_only_after_full_validation_without_a_watcher() {
        let directory = TestDirectory::new("persisted_resume");
        directory.key("hero.key", &sample_key_file());
        {
            let service = service(&directory);
            service.install().unwrap();
            service.stop().unwrap();
        }
        let restarted = service(&directory);
        let consent = ExplicitChatBindingConsent::from_persisted_user_consent(true).unwrap();
        let status = restarted.start_watcher_with_consent(consent).unwrap();
        assert!(status.ready);
        assert!(status.consent_granted);
        assert!(!status.watcher_running);
    }

    #[test]
    fn false_persisted_consent_cannot_create_a_token() {
        assert!(ExplicitChatBindingConsent::from_persisted_user_consent(false).is_err());
    }

    #[test]
    fn interrupted_replace_with_missing_target_restores_original_not_staging() {
        let directory = TestDirectory::new("recover_original");
        let original = sample_key_file();
        let replacement = patch_f13(&original).unwrap();
        let target = directory.key("hero.key", &original);
        let stage = sibling_with_suffix(&target, STAGING_SUFFIX).unwrap();
        let rollback = sibling_with_suffix(&target, ROLLBACK_SUFFIX).unwrap();
        let journal_path = sibling_with_suffix(&target, JOURNAL_SUFFIX).unwrap();
        create_synced_new_file(&stage, &replacement).unwrap();
        let journal = ReplaceJournal {
            version: REPLACE_JOURNAL_VERSION,
            target_file_name: "hero.key".to_string(),
            original_checksum: checksum(&original),
            replacement_checksum: checksum(&replacement),
        };
        create_synced_new_file(&journal_path, &serde_json::to_vec(&journal).unwrap()).unwrap();
        std::fs::rename(&target, &rollback).unwrap();

        recover_interrupted_transaction(&target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), original);
        assert!(!stage.exists());
        assert!(!rollback.exists());
        assert!(!journal_path.exists());
    }

    #[test]
    fn torn_uncommitted_stage_cannot_block_original_key_recovery() {
        let directory = TestDirectory::new("recover_original_torn_stage");
        let original = sample_key_file();
        let replacement = patch_f13(&original).unwrap();
        let target = directory.key("hero.key", &original);
        let stage = sibling_with_suffix(&target, STAGING_SUFFIX).unwrap();
        let rollback = sibling_with_suffix(&target, ROLLBACK_SUFFIX).unwrap();
        let journal_path = sibling_with_suffix(&target, JOURNAL_SUFFIX).unwrap();
        create_synced_new_file(&stage, b"torn-stage").unwrap();
        let journal = ReplaceJournal {
            version: REPLACE_JOURNAL_VERSION,
            target_file_name: "hero.key".to_string(),
            original_checksum: checksum(&original),
            replacement_checksum: checksum(&replacement),
        };
        create_synced_new_file(&journal_path, &serde_json::to_vec(&journal).unwrap()).unwrap();
        std::fs::rename(&target, &rollback).unwrap();

        recover_interrupted_transaction(&target).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), original);
        assert!(!stage.exists());
        assert!(!rollback.exists());
        assert!(!journal_path.exists());
        recover_interrupted_transaction(&target).unwrap();
    }

    #[test]
    fn interrupted_replace_with_new_target_commits_and_cleans_rollback() {
        let directory = TestDirectory::new("recover_commit");
        let original = sample_key_file();
        let replacement = patch_f13(&original).unwrap();
        let target = directory.key("hero.key", &replacement);
        let rollback = sibling_with_suffix(&target, ROLLBACK_SUFFIX).unwrap();
        let journal_path = sibling_with_suffix(&target, JOURNAL_SUFFIX).unwrap();
        create_synced_new_file(&rollback, &original).unwrap();
        let journal = ReplaceJournal {
            version: REPLACE_JOURNAL_VERSION,
            target_file_name: "hero.key".to_string(),
            original_checksum: checksum(&original),
            replacement_checksum: checksum(&replacement),
        };
        create_synced_new_file(&journal_path, &serde_json::to_vec(&journal).unwrap()).unwrap();

        recover_interrupted_transaction(&target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), replacement);
        assert!(!rollback.exists());
        assert!(!journal_path.exists());
    }

    #[test]
    fn malformed_journal_before_commit_keeps_original() {
        let directory = TestDirectory::new("bad_journal_before_commit");
        let original = sample_key_file();
        let target = directory.key("hero.key", &original);
        let stage = sibling_with_suffix(&target, STAGING_SUFFIX).unwrap();
        let journal_path = sibling_with_suffix(&target, JOURNAL_SUFFIX).unwrap();
        create_synced_new_file(&stage, &patch_f13(&original).unwrap()).unwrap();
        create_synced_new_file(&journal_path, b"{partial").unwrap();

        recover_interrupted_transaction(&target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), original);
        assert!(!stage.exists());
        assert!(!journal_path.exists());
    }

    #[test]
    fn operation_mutex_serializes_concurrent_installs() {
        let directory = TestDirectory::new("serialized");
        directory.key("hero.key", &sample_key_file());
        let service = Arc::new(service(&directory));
        let successes = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let service = Arc::clone(&service);
            let successes = Arc::clone(&successes);
            workers.push(std::thread::spawn(move || {
                if service.install().is_ok() {
                    successes.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(successes.load(Ordering::SeqCst), 2);
        assert!(service.status().unwrap().ready);
        service.stop().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_key_file_is_rejected_without_touching_target() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new("symlink_file");
        let outside = TestDirectory::new("symlink_outside");
        let original = sample_key_file();
        let outside_key = outside.key("outside.key", &original);
        symlink(&outside_key, directory.path().join("hero.key")).unwrap();
        let service = service(&directory);
        assert!(service.install().unwrap_err().contains("符号链接"));
        assert_eq!(std::fs::read(outside_key).unwrap(), original);
    }

    #[cfg(windows)]
    #[test]
    fn symlink_key_file_is_rejected_without_touching_target() {
        use std::os::windows::fs::symlink_file;

        let directory = TestDirectory::new("symlink_file");
        let outside = TestDirectory::new("symlink_outside");
        let original = sample_key_file();
        let outside_key = outside.key("outside.key", &original);
        let link = directory.path().join("hero.key");
        if symlink_file(&outside_key, &link).is_err() {
            return;
        }
        let service = service(&directory);
        assert!(service.install().unwrap_err().contains("reparse"));
        assert_eq!(std::fs::read(outside_key).unwrap(), original);
    }
}
