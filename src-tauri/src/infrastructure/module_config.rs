//! Versioned, crash-recoverable JSON storage for optional capability modules.
//!
//! A module owns one sidecar at
//! `<app-data>/modules/<module-id>/config.json`. The store deliberately keeps
//! commit-point recovery separate from module-specific migrations: callers
//! choose the supported schema range and deserialize their own payload type.

use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::durable_fs;

/// Files written by this implementation use the first sidecar envelope
/// format. A format change alters recovery or envelope semantics and therefore
/// must fail closed in older binaries.
pub const MODULE_CONFIG_FORMAT_VERSION: u32 = 1;

/// Durable envelope around a module-owned, typed payload.
///
/// Unknown top-level fields from the same envelope format are retained across
/// writes. Module payload compatibility remains the module's responsibility.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModuleConfigEnvelope<T> {
    pub format_version: u32,
    pub module_id: String,
    pub schema_version: u32,
    pub generation: u64,
    #[serde(default)]
    pub legacy_import: Option<Value>,
    pub payload: T,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleConfigError {
    #[error("invalid module ID `{0}`; use lowercase ASCII letters, digits and single hyphens")]
    InvalidModuleId(String),
    #[error(
        "invalid schema range {minimum}..={current}; schema versions must start at one and be ordered"
    )]
    InvalidSchemaRange { minimum: u32, current: u32 },
    #[error("module config I/O failed while {operation} at {path:?}: {message}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        message: String,
    },
    #[error("module config serialization failed while {operation}: {message}")]
    Serialization {
        operation: &'static str,
        message: String,
    },
    #[error("module config artifact {path:?} is not a regular file")]
    UnsafeArtifactType { path: PathBuf },
    #[error("module config directory {path:?} is unsafe: {reason}")]
    UnsafeDirectory { path: PathBuf, reason: String },
    #[error("module config artifact {path:?} is corrupt: {reason}")]
    CorruptArtifact { path: PathBuf, reason: String },
    #[error(
        "module config format {found} in {path:?} is unsupported; this binary supports format {supported}"
    )]
    UnsupportedFormat {
        path: PathBuf,
        found: u64,
        supported: u32,
    },
    #[error(
        "module config schema {found} in {path:?} is unsupported; supported range is {minimum}..={current}"
    )]
    UnsupportedSchema {
        path: PathBuf,
        found: u64,
        minimum: u32,
        current: u32,
    },
    #[error("module config {path:?} belongs to `{found}`, expected `{expected}`")]
    ModuleIdMismatch {
        path: PathBuf,
        expected: String,
        found: String,
    },
    #[error("module config recovery stopped without a safe candidate: {reason}")]
    UnsafeRecovery { reason: String },
    #[error("module config generation conflict: expected {expected}, current is {actual}")]
    GenerationConflict { expected: u64, actual: u64 },
    #[error("module config generation overflow at {0}")]
    GenerationOverflow(u64),
}

/// Cloneable store intended to be shared by a capability's commands and
/// worker. Clones serialize load/recover/write transactions through one lock.
#[derive(Clone, Debug)]
pub struct ModuleConfigStore {
    module_id: String,
    minimum_schema_version: u32,
    current_schema_version: u32,
    app_data_dir: PathBuf,
    module_dir: PathBuf,
    io: Arc<Mutex<()>>,
}

impl ModuleConfigStore {
    /// Creates a store that accepts and writes exactly `current_schema_version`.
    pub fn new(
        app_data_dir: impl AsRef<Path>,
        module_id: impl Into<String>,
        current_schema_version: u32,
    ) -> Result<Self, ModuleConfigError> {
        Self::with_supported_schema_range(
            app_data_dir,
            module_id,
            current_schema_version,
            current_schema_version,
        )
    }

    /// Creates a store that can load `minimum_schema_version..=current_schema_version`
    /// and always writes the current version. Payload migrations are performed
    /// by the module before it saves the next generation.
    pub fn with_supported_schema_range(
        app_data_dir: impl AsRef<Path>,
        module_id: impl Into<String>,
        minimum_schema_version: u32,
        current_schema_version: u32,
    ) -> Result<Self, ModuleConfigError> {
        let module_id = module_id.into();
        validate_module_id(&module_id)?;
        if minimum_schema_version == 0
            || current_schema_version == 0
            || minimum_schema_version > current_schema_version
        {
            return Err(ModuleConfigError::InvalidSchemaRange {
                minimum: minimum_schema_version,
                current: current_schema_version,
            });
        }

        let app_data_dir = app_data_dir.as_ref().to_path_buf();
        Ok(Self {
            module_dir: app_data_dir.join("modules").join(&module_id),
            app_data_dir,
            module_id,
            minimum_schema_version,
            current_schema_version,
            io: Arc::new(Mutex::new(())),
        })
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn module_dir(&self) -> &Path {
        &self.module_dir
    }

    pub fn config_path(&self) -> PathBuf {
        self.module_dir.join("config.json")
    }

    pub fn backup_path(&self) -> PathBuf {
        self.module_dir.join("config.json.bak")
    }

    pub fn staging_path(&self) -> PathBuf {
        self.module_dir.join("config.json.tmp")
    }

    /// Loads and, when necessary, idempotently installs the safe recovery
    /// candidate. `Ok(None)` is returned only when no transaction artifacts
    /// exist at all.
    pub fn load<T>(&self) -> Result<Option<ModuleConfigEnvelope<T>>, ModuleConfigError>
    where
        T: DeserializeOwned,
    {
        let _io = self.io.lock();
        self.load_locked()
    }

    /// Commits a payload only if `expected_generation` still matches disk.
    /// Generation zero denotes a sidecar that does not exist yet.
    pub fn save_if_generation<T>(
        &self,
        expected_generation: u64,
        payload: T,
    ) -> Result<ModuleConfigEnvelope<T>, ModuleConfigError>
    where
        T: DeserializeOwned + Serialize,
    {
        self.save_locked(expected_generation, payload, None)
    }

    /// Performs the same CAS write and records legacy-import metadata only for
    /// the first generation. Once present, that marker is immutable and is
    /// preserved by every later write.
    pub fn save_with_legacy_import_if_generation<T>(
        &self,
        expected_generation: u64,
        payload: T,
        legacy_import: Value,
    ) -> Result<ModuleConfigEnvelope<T>, ModuleConfigError>
    where
        T: DeserializeOwned + Serialize,
    {
        self.save_locked(expected_generation, payload, Some(legacy_import))
    }

    fn save_locked<T>(
        &self,
        expected_generation: u64,
        payload: T,
        initial_legacy_import: Option<Value>,
    ) -> Result<ModuleConfigEnvelope<T>, ModuleConfigError>
    where
        T: DeserializeOwned + Serialize,
    {
        let _io = self.io.lock();
        let current = self.load_locked::<T>()?;
        let actual_generation = current.as_ref().map_or(0, |envelope| envelope.generation);
        if actual_generation != expected_generation {
            return Err(ModuleConfigError::GenerationConflict {
                expected: expected_generation,
                actual: actual_generation,
            });
        }

        let generation = actual_generation
            .checked_add(1)
            .ok_or(ModuleConfigError::GenerationOverflow(actual_generation))?;
        let (legacy_import, extensions) = current.map_or_else(
            || (initial_legacy_import, BTreeMap::new()),
            |envelope| (envelope.legacy_import, envelope.extensions),
        );
        let envelope = ModuleConfigEnvelope {
            format_version: MODULE_CONFIG_FORMAT_VERSION,
            module_id: self.module_id.clone(),
            schema_version: self.current_schema_version,
            generation,
            legacy_import,
            payload,
            extensions,
        };
        let mut content = serde_json::to_vec_pretty(&envelope).map_err(|error| {
            ModuleConfigError::Serialization {
                operation: "encoding a payload",
                message: error.to_string(),
            }
        })?;
        content.push(b'\n');
        self.commit_bytes(&content)?;
        Ok(envelope)
    }

    fn load_locked<T>(&self) -> Result<Option<ModuleConfigEnvelope<T>>, ModuleConfigError>
    where
        T: DeserializeOwned,
    {
        if !self.validate_directory_tree(false)? {
            return Ok(None);
        }

        let primary = self.config_path();
        let backup = self.backup_path();
        let staging = self.staging_path();

        if artifact_exists(&primary)? {
            return match self.read_candidate(&primary) {
                Ok(envelope) => Ok(Some(envelope)),
                Err(ModuleConfigError::CorruptArtifact { .. }) => {
                    if !artifact_exists(&backup)? {
                        return Err(ModuleConfigError::UnsafeRecovery {
                            reason: format!(
                                "primary {} is corrupt and no backup exists; staging is deliberately ignored before the commit point",
                                primary.display()
                            ),
                        });
                    }
                    let envelope = self.read_candidate(&backup)?;
                    self.replace_corrupt_with(&primary, &backup)?;
                    Ok(Some(envelope))
                }
                Err(error) => Err(error),
            };
        }

        if artifact_exists(&staging)? {
            return match self.read_candidate(&staging) {
                Ok(envelope) => {
                    self.promote_missing_primary(&staging, &primary)?;
                    Ok(Some(envelope))
                }
                Err(ModuleConfigError::CorruptArtifact { .. }) => {
                    if !artifact_exists(&backup)? {
                        return Err(ModuleConfigError::UnsafeRecovery {
                            reason: format!(
                                "primary is missing and staging {} is corrupt with no backup available",
                                staging.display()
                            ),
                        });
                    }
                    let envelope = self.read_candidate(&backup)?;
                    self.archive_corrupt(&staging)?;
                    self.promote_missing_primary(&backup, &primary)?;
                    Ok(Some(envelope))
                }
                Err(error) => Err(error),
            };
        }

        if artifact_exists(&backup)? {
            let envelope = self.read_candidate(&backup)?;
            self.promote_missing_primary(&backup, &primary)?;
            return Ok(Some(envelope));
        }

        Ok(None)
    }

    fn read_candidate<T>(&self, path: &Path) -> Result<ModuleConfigEnvelope<T>, ModuleConfigError>
    where
        T: DeserializeOwned,
    {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| io_error("reading artifact metadata", path, error))?;
        if !metadata.file_type().is_file() {
            return Err(ModuleConfigError::UnsafeArtifactType {
                path: path.to_path_buf(),
            });
        }

        let content =
            fs::read(path).map_err(|error| io_error("reading an artifact", path, error))?;
        let value: Value = serde_json::from_slice(&content).map_err(|error| {
            ModuleConfigError::CorruptArtifact {
                path: path.to_path_buf(),
                reason: error.to_string(),
            }
        })?;
        let object = value
            .as_object()
            .ok_or_else(|| ModuleConfigError::CorruptArtifact {
                path: path.to_path_buf(),
                reason: "envelope must be a JSON object".to_string(),
            })?;

        let format_version = required_u64(object, "format_version", path)?;
        if format_version != u64::from(MODULE_CONFIG_FORMAT_VERSION) {
            return Err(ModuleConfigError::UnsupportedFormat {
                path: path.to_path_buf(),
                found: format_version,
                supported: MODULE_CONFIG_FORMAT_VERSION,
            });
        }
        let found_module_id = required_string(object, "module_id", path)?;
        if found_module_id != self.module_id {
            return Err(ModuleConfigError::ModuleIdMismatch {
                path: path.to_path_buf(),
                expected: self.module_id.clone(),
                found: found_module_id.to_string(),
            });
        }
        let schema_version = required_u64(object, "schema_version", path)?;
        if schema_version < u64::from(self.minimum_schema_version)
            || schema_version > u64::from(self.current_schema_version)
        {
            return Err(ModuleConfigError::UnsupportedSchema {
                path: path.to_path_buf(),
                found: schema_version,
                minimum: self.minimum_schema_version,
                current: self.current_schema_version,
            });
        }
        let generation = required_u64(object, "generation", path)?;
        if generation == 0 {
            return Err(ModuleConfigError::CorruptArtifact {
                path: path.to_path_buf(),
                reason: "generation must be greater than zero".to_string(),
            });
        }

        serde_json::from_value(value).map_err(|error| ModuleConfigError::CorruptArtifact {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })
    }

    fn commit_bytes(&self, content: &[u8]) -> Result<(), ModuleConfigError> {
        self.validate_directory_tree(true)?;
        let primary = self.config_path();
        let backup = self.backup_path();
        let staging = self.staging_path();

        remove_stale_staging(&staging)?;
        let mut staged = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)
            .map_err(|error| io_error("creating the staging file", &staging, error))?;
        if let Err(error) = staged.write_all(content).and_then(|_| staged.sync_all()) {
            drop(staged);
            let _ = fs::remove_file(&staging);
            return Err(io_error(
                "writing and syncing the staging file",
                &staging,
                error,
            ));
        }
        drop(staged);

        let had_primary = artifact_exists(&primary)?;
        if had_primary {
            if artifact_exists(&backup)? {
                fs::remove_file(&backup)
                    .map_err(|error| io_error("removing the old backup", &backup, error))?;
            }
            durable_fs::durable_sibling_rename(&primary, &backup)
                .map_err(|error| io_error("rotating the primary into backup", &primary, error))?;
            sync_directory(&self.module_dir)?;
        }

        if let Err(install_error) = durable_fs::durable_sibling_rename(&staging, &primary) {
            if had_primary {
                if let Err(restore_error) = durable_fs::durable_sibling_rename(&backup, &primary) {
                    return Err(ModuleConfigError::UnsafeRecovery {
                        reason: format!(
                            "installing staged config failed ({install_error}); restoring backup also failed ({restore_error})"
                        ),
                    });
                }
            }
            return Err(io_error(
                "installing the staged config",
                &primary,
                install_error,
            ));
        }
        sync_directory(&self.module_dir)?;
        Ok(())
    }

    fn replace_corrupt_with(
        &self,
        corrupt_primary: &Path,
        safe_backup: &Path,
    ) -> Result<(), ModuleConfigError> {
        self.require_existing_directory_tree()?;

        // Once a corrupt primary has selected its backup, any staging file is
        // definitively before the commit point. Archive it first so a crash in
        // the small primary-replacement window cannot make a later startup
        // mistake that staging file for the committed candidate.
        let staging = self.staging_path();
        if artifact_exists(&staging)? {
            let abandoned = self.unique_archive_path("staging-abandoned");
            durable_fs::durable_sibling_rename(&staging, &abandoned)
                .map_err(|error| io_error("archiving abandoned staging", &staging, error))?;
            sync_directory(&self.module_dir)?;
        }

        let archived = self.unique_corrupt_path();
        durable_fs::durable_sibling_rename(corrupt_primary, &archived)
            .map_err(|error| io_error("archiving the corrupt primary", corrupt_primary, error))?;
        if let Err(install_error) = durable_fs::durable_sibling_rename(safe_backup, corrupt_primary)
        {
            if let Err(restore_error) =
                durable_fs::durable_sibling_rename(&archived, corrupt_primary)
            {
                return Err(ModuleConfigError::UnsafeRecovery {
                    reason: format!(
                        "installing backup failed ({install_error}); restoring corrupt primary also failed ({restore_error})"
                    ),
                });
            }
            return Err(io_error(
                "installing the recovery backup",
                corrupt_primary,
                install_error,
            ));
        }
        sync_directory(&self.module_dir)
    }

    fn archive_corrupt(&self, path: &Path) -> Result<(), ModuleConfigError> {
        self.require_existing_directory_tree()?;
        let archived = self.unique_corrupt_path();
        durable_fs::durable_sibling_rename(path, &archived)
            .map_err(|error| io_error("archiving a corrupt artifact", path, error))?;
        sync_directory(&self.module_dir)
    }

    fn promote_missing_primary(
        &self,
        candidate: &Path,
        primary: &Path,
    ) -> Result<(), ModuleConfigError> {
        self.require_existing_directory_tree()?;
        durable_fs::durable_sibling_rename(candidate, primary)
            .map_err(|error| io_error("promoting a recovery candidate", candidate, error))?;
        sync_directory(&self.module_dir)
    }

    fn unique_corrupt_path(&self) -> PathBuf {
        self.unique_archive_path("corrupt")
    }

    fn unique_archive_path(&self, label: &str) -> PathBuf {
        self.module_dir.join(format!(
            "config.{label}-{}.json",
            uuid::Uuid::new_v4().simple()
        ))
    }

    /// Validates the complete directory chain owned by the store before any
    /// artifact is inspected or mutated. Reads deliberately leave missing
    /// directories absent; writes create each owned layer separately so
    /// `create_dir_all` can never traverse a pre-existing link.
    fn validate_directory_tree(&self, create_missing: bool) -> Result<bool, ModuleConfigError> {
        if !ensure_real_directory(&self.app_data_dir, create_missing)? {
            return Ok(false);
        }
        let canonical_app_data = canonical_real_directory(&self.app_data_dir)?;

        let modules_dir = self.app_data_dir.join("modules");
        if !ensure_real_directory(&modules_dir, create_missing)? {
            return Ok(false);
        }
        let canonical_modules = canonical_real_directory(&modules_dir)?;
        ensure_canonical_child(&canonical_app_data, &canonical_modules, &modules_dir)?;

        if !ensure_real_directory(&self.module_dir, create_missing)? {
            return Ok(false);
        }
        let canonical_module = canonical_real_directory(&self.module_dir)?;
        ensure_canonical_child(&canonical_modules, &canonical_module, &self.module_dir)?;
        Ok(true)
    }

    fn require_existing_directory_tree(&self) -> Result<(), ModuleConfigError> {
        if self.validate_directory_tree(false)? {
            Ok(())
        } else {
            Err(unsafe_directory(
                &self.module_dir,
                "directory chain disappeared before recovery could be committed",
            ))
        }
    }
}

fn ensure_real_directory(path: &Path, create_missing: bool) -> Result<bool, ModuleConfigError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_directory_metadata(path, &metadata)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !create_missing => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => {}
                // Another participant may have created the path between the
                // metadata check and create. It is trusted only after the same
                // non-following metadata validation as every existing layer.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(io_error("creating a module config directory", path, error));
                }
            }
            let metadata = fs::symlink_metadata(path).map_err(|error| {
                io_error("validating a created module config directory", path, error)
            })?;
            validate_directory_metadata(path, &metadata)?;
            if let Some(parent) = path.parent() {
                durable_fs::sync_directory(parent).map_err(|error| {
                    io_error("syncing a newly created directory entry", parent, error)
                })?;
            }
            Ok(true)
        }
        Err(error) => Err(io_error("checking a module config directory", path, error)),
    }
}

fn validate_directory_metadata(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), ModuleConfigError> {
    if metadata.file_type().is_symlink() {
        return Err(unsafe_directory(path, "symbolic links are not allowed"));
    }
    if metadata_is_reparse_point(metadata) {
        return Err(unsafe_directory(
            path,
            "filesystem reparse points are not allowed",
        ));
    }
    if !metadata.is_dir() {
        return Err(unsafe_directory(path, "path is not a directory"));
    }
    Ok(())
}

fn canonical_real_directory(path: &Path) -> Result<PathBuf, ModuleConfigError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| io_error("canonicalizing a module config directory", path, error))?;

    // Re-check the configured path after canonicalization. Besides narrowing
    // the metadata/canonicalize race, this ensures canonical containment is
    // never used to legitimize a link that appeared between the two calls.
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        io_error(
            "revalidating a canonical module config directory",
            path,
            error,
        )
    })?;
    validate_directory_metadata(path, &metadata)?;
    Ok(canonical)
}

fn ensure_canonical_child(
    canonical_parent: &Path,
    canonical_child: &Path,
    configured_child: &Path,
) -> Result<(), ModuleConfigError> {
    let relative = canonical_child
        .strip_prefix(canonical_parent)
        .map_err(|_| unsafe_directory(configured_child, "canonical path escapes its owner"))?;
    if relative.components().count() != 1 {
        return Err(unsafe_directory(
            configured_child,
            "canonical path is not an immediate child of its owner",
        ));
    }
    Ok(())
}

fn unsafe_directory(path: &Path, reason: impl Into<String>) -> ModuleConfigError {
    ModuleConfigError::UnsafeDirectory {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

#[cfg(windows)]
fn file_attributes_include_reparse_point(attributes: u32) -> bool {
    attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    file_attributes_include_reparse_point(metadata.file_attributes())
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn validate_module_id(module_id: &str) -> Result<(), ModuleConfigError> {
    let bytes = module_id.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= 64
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !module_id.contains("--");
    if valid {
        Ok(())
    } else {
        Err(ModuleConfigError::InvalidModuleId(module_id.to_string()))
    }
}

fn artifact_exists(path: &Path) -> Result<bool, ModuleConfigError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("checking an artifact", path, error)),
    }
}

fn remove_stale_staging(path: &Path) -> Result<(), ModuleConfigError> {
    if artifact_exists(path)? {
        fs::remove_file(path).map_err(|error| io_error("removing stale staging", path, error))?;
    }
    Ok(())
}

fn required_u64(
    object: &serde_json::Map<String, Value>,
    field: &str,
    path: &Path,
) -> Result<u64, ModuleConfigError> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| ModuleConfigError::CorruptArtifact {
            path: path.to_path_buf(),
            reason: format!("field `{field}` must be an unsigned integer"),
        })
}

fn required_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &str,
    path: &Path,
) -> Result<&'a str, ModuleConfigError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| ModuleConfigError::CorruptArtifact {
            path: path.to_path_buf(),
            reason: format!("field `{field}` must be a string"),
        })
}

fn io_error(operation: &'static str, path: &Path, error: std::io::Error) -> ModuleConfigError {
    ModuleConfigError::Io {
        operation,
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

fn sync_directory(path: &Path) -> Result<(), ModuleConfigError> {
    durable_fs::sync_directory(path)
        .map_err(|error| io_error("syncing directory metadata", path, error))
}

#[cfg(test)]
mod tests {
    use super::{
        ModuleConfigEnvelope, ModuleConfigError, ModuleConfigStore, MODULE_CONFIG_FORMAT_VERSION,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Barrier};

    #[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
    struct Payload {
        sequence: u64,
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "d2rhub_module_config_{label}_{}",
                uuid::Uuid::new_v4().simple()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn store(root: &TestDirectory) -> ModuleConfigStore {
        ModuleConfigStore::new(root.path(), "room-automation", 1).unwrap()
    }

    fn config_value(
        format_version: u64,
        module_id: &str,
        schema_version: u64,
        generation: u64,
        sequence: Value,
    ) -> Value {
        json!({
            "format_version": format_version,
            "module_id": module_id,
            "schema_version": schema_version,
            "generation": generation,
            "legacy_import": null,
            "payload": { "sequence": sequence }
        })
    }

    fn write_json(path: &Path, value: &Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    fn write_config(path: &Path, generation: u64, sequence: u64) {
        write_json(
            path,
            &config_value(
                u64::from(MODULE_CONFIG_FORMAT_VERSION),
                "room-automation",
                1,
                generation,
                json!(sequence),
            ),
        );
    }

    fn read_value(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn path_layout_and_identifiers_are_safe() {
        let root = TestDirectory::new("paths");
        let store = store(&root);

        assert_eq!(
            store.config_path(),
            root.path()
                .join("modules")
                .join("room-automation")
                .join("config.json")
        );
        for invalid in [
            "",
            "RoomAutomation",
            "room_automation",
            "room--automation",
            "-room",
            "room-",
            "../room",
        ] {
            assert!(matches!(
                ModuleConfigStore::new(root.path(), invalid, 1),
                Err(ModuleConfigError::InvalidModuleId(_))
            ));
        }
        assert!(matches!(
            ModuleConfigStore::with_supported_schema_range(root.path(), "room-automation", 2, 1),
            Err(ModuleConfigError::InvalidSchemaRange { .. })
        ));
    }

    #[test]
    fn no_artifacts_is_the_only_missing_state() {
        let root = TestDirectory::new("missing");
        let store = store(&root);

        assert!(store.load::<Payload>().unwrap().is_none());
        assert!(!store.module_dir().exists());
    }

    #[test]
    fn save_creates_each_missing_owned_directory() {
        let root = TestDirectory::new("create_owned_layers");
        let app_data = root.path().join("new-app-data");
        let store = ModuleConfigStore::new(&app_data, "room-automation", 1).unwrap();

        store
            .save_if_generation(0, Payload { sequence: 1 })
            .unwrap();

        assert!(app_data.is_dir());
        assert!(app_data.join("modules").is_dir());
        assert!(store.module_dir().is_dir());
        assert!(store.config_path().is_file());
    }

    #[test]
    fn writes_versioned_generations_and_rotates_the_last_good_backup() {
        let root = TestDirectory::new("round_trip");
        let store = store(&root);
        let marker = json!({ "source": "global-v9.room_rotation", "digest": "abc" });

        let first = store
            .save_with_legacy_import_if_generation(0, Payload { sequence: 10 }, marker.clone())
            .unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(first.legacy_import, Some(marker.clone()));
        assert!(!store.backup_path().exists());

        let second = store
            .save_if_generation(1, Payload { sequence: 11 })
            .unwrap();
        assert_eq!(second.generation, 2);
        assert_eq!(second.legacy_import, Some(marker.clone()));
        assert!(!store.staging_path().exists());

        let loaded = store.load::<Payload>().unwrap().unwrap();
        assert_eq!(loaded, second);
        let backup: ModuleConfigEnvelope<Payload> =
            serde_json::from_slice(&fs::read(store.backup_path()).unwrap()).unwrap();
        assert_eq!(backup.generation, 1);
        assert_eq!(backup.payload.sequence, 10);

        let third = store
            .save_with_legacy_import_if_generation(
                2,
                Payload { sequence: 12 },
                json!({ "source": "must-not-replace" }),
            )
            .unwrap();
        assert_eq!(third.legacy_import, Some(marker));
    }

    #[test]
    fn stale_generation_is_rejected_without_touching_disk() {
        let root = TestDirectory::new("conflict");
        let store = store(&root);
        store
            .save_if_generation(0, Payload { sequence: 1 })
            .unwrap();
        let before = fs::read(store.config_path()).unwrap();

        let error = store
            .save_if_generation(0, Payload { sequence: 99 })
            .unwrap_err();
        assert!(matches!(
            error,
            ModuleConfigError::GenerationConflict {
                expected: 0,
                actual: 1
            }
        ));
        assert_eq!(fs::read(store.config_path()).unwrap(), before);
        assert!(!store.backup_path().exists());
    }

    #[test]
    fn cloned_stores_serialize_competing_compare_and_swap_writes() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ModuleConfigStore>();

        let root = TestDirectory::new("clone_concurrency");
        let first = store(&root);
        let second = first.clone();
        let start = Arc::new(Barrier::new(3));
        let writers = [first, second]
            .into_iter()
            .enumerate()
            .map(|(index, store)| {
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    store.save_if_generation(
                        0,
                        Payload {
                            sequence: index as u64,
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        start.wait();

        let results = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(
                    result,
                    Err(ModuleConfigError::GenerationConflict {
                        expected: 0,
                        actual: 1
                    })
                ))
                .count(),
            1
        );
    }

    #[test]
    fn supported_old_schema_and_unknown_envelope_fields_survive_the_next_write() {
        let root = TestDirectory::new("schema_upgrade");
        let store =
            ModuleConfigStore::with_supported_schema_range(root.path(), "room-automation", 1, 2)
                .unwrap();
        let mut value = config_value(1, "room-automation", 1, 7, json!(40));
        value
            .as_object_mut()
            .unwrap()
            .insert("future_same_format".to_string(), json!({ "keep": [1, 2] }));
        write_json(&store.config_path(), &value);

        let loaded = store.load::<Payload>().unwrap().unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(
            loaded.extensions["future_same_format"],
            json!({ "keep": [1, 2] })
        );
        let saved = store
            .save_if_generation(7, Payload { sequence: 41 })
            .unwrap();

        assert_eq!(saved.schema_version, 2);
        assert_eq!(saved.generation, 8);
        assert_eq!(
            read_value(&store.config_path())["future_same_format"],
            json!({ "keep": [1, 2] })
        );
    }

    #[test]
    fn a_valid_primary_wins_over_backup_and_staging_regardless_of_generation() {
        let root = TestDirectory::new("primary_priority");
        let store = store(&root);
        write_config(&store.config_path(), 2, 20);
        fs::write(store.backup_path(), "{broken-backup").unwrap();
        write_json(
            &store.staging_path(),
            &config_value(2, "room-automation", 1, 90, json!(90)),
        );

        let loaded = store.load::<Payload>().unwrap().unwrap();

        assert_eq!(loaded.generation, 2);
        assert_eq!(loaded.payload.sequence, 20);
        assert!(store.backup_path().exists());
        assert!(store.staging_path().exists());
    }

    #[test]
    fn an_unsupported_primary_never_downgrades_to_a_valid_backup() {
        let root = TestDirectory::new("future_primary");
        let store = store(&root);
        write_json(
            &store.config_path(),
            &config_value(2, "room-automation", 1, 2, json!(20)),
        );
        write_config(&store.backup_path(), 1, 10);

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::UnsupportedFormat { found: 2, .. })
        ));
        assert_eq!(read_value(&store.config_path())["format_version"], 2);
        assert!(store.backup_path().exists());
    }

    #[test]
    fn a_future_schema_primary_never_downgrades_to_a_valid_backup() {
        let root = TestDirectory::new("future_schema");
        let store = store(&root);
        write_json(
            &store.config_path(),
            &config_value(1, "room-automation", 2, 2, json!(20)),
        );
        write_config(&store.backup_path(), 1, 10);

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::UnsupportedSchema { found: 2, .. })
        ));
        assert_eq!(read_value(&store.config_path())["schema_version"], 2);
    }

    #[test]
    fn a_primary_for_another_module_fails_closed() {
        let root = TestDirectory::new("wrong_module");
        let store = store(&root);
        write_json(
            &store.config_path(),
            &config_value(1, "another-module", 1, 2, json!(20)),
        );
        write_config(&store.backup_path(), 1, 10);

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::ModuleIdMismatch { .. })
        ));
        assert!(store.config_path().exists());
        assert!(store.backup_path().exists());
    }

    #[test]
    fn corrupt_primary_recovers_only_the_backup_and_is_idempotent() {
        let root = TestDirectory::new("corrupt_primary");
        let store = store(&root);
        fs::create_dir_all(store.module_dir()).unwrap();
        fs::write(store.config_path(), "{broken-primary").unwrap();
        write_config(&store.backup_path(), 4, 40);
        write_config(&store.staging_path(), 9, 90);

        let first = store.load::<Payload>().unwrap().unwrap();
        assert_eq!(first.generation, 4);
        assert_eq!(first.payload.sequence, 40);
        assert!(!store.staging_path().exists());
        assert!(!store.backup_path().exists());
        let corrupt_count = fs::read_dir(store.module_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("config.corrupt-")
            })
            .count();
        assert_eq!(corrupt_count, 1);
        assert_eq!(
            fs::read_dir(store.module_dir())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("config.staging-abandoned-"))
                .count(),
            1
        );

        let second = store.load::<Payload>().unwrap().unwrap();
        assert_eq!(second, first);
        let second_corrupt_count = fs::read_dir(store.module_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("config.corrupt-")
            })
            .count();
        assert_eq!(second_corrupt_count, 1);
    }

    #[test]
    fn corrupt_primary_never_uses_staging_before_the_commit_point() {
        let root = TestDirectory::new("primary_ignores_tmp");
        let store = store(&root);
        fs::create_dir_all(store.module_dir()).unwrap();
        fs::write(store.config_path(), "{broken-primary").unwrap();
        write_config(&store.staging_path(), 9, 90);

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::UnsafeRecovery { .. })
        ));
        assert_eq!(
            fs::read_to_string(store.config_path()).unwrap(),
            "{broken-primary"
        );
        assert!(store.staging_path().exists());
    }

    #[test]
    fn corrupt_primary_with_an_unsupported_backup_still_cannot_use_staging() {
        let root = TestDirectory::new("primary_future_backup");
        let store = store(&root);
        fs::create_dir_all(store.module_dir()).unwrap();
        fs::write(store.config_path(), "{broken-primary").unwrap();
        write_json(
            &store.backup_path(),
            &config_value(1, "room-automation", 2, 8, json!(80)),
        );
        write_config(&store.staging_path(), 9, 90);

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::UnsupportedSchema { found: 2, .. })
        ));
        assert_eq!(
            fs::read_to_string(store.config_path()).unwrap(),
            "{broken-primary"
        );
        assert!(store.backup_path().exists());
        assert!(store.staging_path().exists());
    }

    #[test]
    fn missing_primary_promotes_valid_staging_before_backup() {
        let root = TestDirectory::new("tmp_priority");
        let store = store(&root);
        write_config(&store.staging_path(), 8, 80);
        write_config(&store.backup_path(), 7, 70);

        let loaded = store.load::<Payload>().unwrap().unwrap();

        assert_eq!(loaded.generation, 8);
        assert_eq!(loaded.payload.sequence, 80);
        assert!(store.config_path().exists());
        assert!(!store.staging_path().exists());
        assert!(store.backup_path().exists());
    }

    #[test]
    fn missing_primary_can_skip_corrupt_staging_for_a_valid_backup() {
        let root = TestDirectory::new("corrupt_tmp_backup");
        let store = store(&root);
        fs::create_dir_all(store.module_dir()).unwrap();
        fs::write(store.staging_path(), "{broken-staging").unwrap();
        write_config(&store.backup_path(), 7, 70);

        let loaded = store.load::<Payload>().unwrap().unwrap();

        assert_eq!(loaded.generation, 7);
        assert_eq!(loaded.payload.sequence, 70);
        assert!(store.config_path().exists());
        assert!(!store.staging_path().exists());
        assert!(!store.backup_path().exists());
    }

    #[test]
    fn unsupported_staging_blocks_backup_when_primary_is_missing() {
        let root = TestDirectory::new("future_tmp");
        let store = store(&root);
        write_json(
            &store.staging_path(),
            &config_value(1, "room-automation", 2, 8, json!(80)),
        );
        write_config(&store.backup_path(), 7, 70);

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::UnsupportedSchema { found: 2, .. })
        ));
        assert!(!store.config_path().exists());
        assert!(store.staging_path().exists());
        assert!(store.backup_path().exists());
    }

    #[test]
    fn backup_only_recovery_is_installed_and_repeatable() {
        let root = TestDirectory::new("backup_only");
        let store = store(&root);
        write_config(&store.backup_path(), 3, 30);

        let first = store.load::<Payload>().unwrap().unwrap();
        let second = store.load::<Payload>().unwrap().unwrap();

        assert_eq!(first, second);
        assert_eq!(second.generation, 3);
        assert!(store.config_path().exists());
        assert!(!store.backup_path().exists());
    }

    #[test]
    fn artifacts_without_a_safe_candidate_fail_closed_and_stay_untouched() {
        let root = TestDirectory::new("no_safe_candidate");
        let store = store(&root);
        fs::create_dir_all(store.module_dir()).unwrap();
        fs::write(store.backup_path(), "{broken-backup").unwrap();

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::CorruptArtifact { .. })
        ));
        assert!(!store.config_path().exists());
        assert_eq!(
            fs::read_to_string(store.backup_path()).unwrap(),
            "{broken-backup"
        );
    }

    #[test]
    fn typed_payload_corruption_uses_backup_but_non_regular_primary_does_not() {
        let root = TestDirectory::new("typed_corruption");
        let store = store(&root);
        write_json(
            &store.config_path(),
            &config_value(1, "room-automation", 1, 2, json!("wrong-type")),
        );
        write_config(&store.backup_path(), 1, 10);

        let recovered = store.load::<Payload>().unwrap().unwrap();
        assert_eq!(recovered.payload.sequence, 10);

        let other_root = TestDirectory::new("non_regular");
        let other_store = ModuleConfigStore::new(other_root.path(), "room-automation", 1).unwrap();
        fs::create_dir_all(other_store.config_path()).unwrap();
        write_config(&other_store.backup_path(), 1, 10);
        assert!(matches!(
            other_store.load::<Payload>(),
            Err(ModuleConfigError::UnsafeArtifactType { .. })
        ));
        assert!(other_store.config_path().is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_primary_fails_closed_without_using_backup() {
        use std::os::unix::fs::symlink;

        let root = TestDirectory::new("symlink_primary");
        let store = store(&root);
        fs::create_dir_all(store.module_dir()).unwrap();
        let target = root.path().join("outside.json");
        write_config(&target, 9, 90);
        symlink(&target, store.config_path()).unwrap();
        write_config(&store.backup_path(), 1, 10);

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::UnsafeArtifactType { .. })
        ));
        assert!(store.config_path().is_symlink());
        assert!(store.backup_path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directory_layers_fail_closed_without_writing_the_targets() {
        use std::os::unix::fs::symlink;

        let modules_root = TestDirectory::new("symlink_modules_root");
        let modules_target = TestDirectory::new("symlink_modules_target");
        symlink(modules_target.path(), modules_root.path().join("modules")).unwrap();
        let modules_store = store(&modules_root);
        assert!(matches!(
            modules_store.save_if_generation(0, Payload { sequence: 1 }),
            Err(ModuleConfigError::UnsafeDirectory { path, .. })
                if path == modules_root.path().join("modules")
        ));
        assert!(!modules_target.path().join("room-automation").exists());

        let module_root = TestDirectory::new("symlink_module_root");
        let module_target = TestDirectory::new("symlink_module_target");
        fs::create_dir(module_root.path().join("modules")).unwrap();
        symlink(
            module_target.path(),
            module_root.path().join("modules").join("room-automation"),
        )
        .unwrap();
        let module_store = store(&module_root);
        assert!(matches!(
            module_store.load::<Payload>(),
            Err(ModuleConfigError::UnsafeDirectory { path, .. })
                if path == module_root.path().join("modules").join("room-automation")
        ));
        assert!(!module_target.path().join("config.json").exists());

        let app_data_parent = TestDirectory::new("symlink_app_data_parent");
        let app_data_target = TestDirectory::new("symlink_app_data_target");
        let linked_app_data = app_data_parent.path().join("app-data");
        symlink(app_data_target.path(), &linked_app_data).unwrap();
        let app_data_store =
            ModuleConfigStore::new(&linked_app_data, "room-automation", 1).unwrap();
        assert!(matches!(
            app_data_store.save_if_generation(0, Payload { sequence: 1 }),
            Err(ModuleConfigError::UnsafeDirectory { path, .. }) if path == linked_app_data
        ));
        assert!(!app_data_target.path().join("modules").exists());
    }

    #[cfg(windows)]
    #[test]
    fn a_symlink_primary_fails_closed_without_using_backup() {
        use std::os::windows::fs::symlink_file;

        let root = TestDirectory::new("symlink_primary");
        let store = store(&root);
        fs::create_dir_all(store.module_dir()).unwrap();
        let target = root.path().join("outside.json");
        write_config(&target, 9, 90);
        if symlink_file(&target, store.config_path()).is_err() {
            // Creating symlinks may require Developer Mode on older Windows
            // hosts. The directory-artifact test exercises the same metadata
            // rejection path when that host policy is unavailable.
            return;
        }
        write_config(&store.backup_path(), 1, 10);

        assert!(matches!(
            store.load::<Payload>(),
            Err(ModuleConfigError::UnsafeArtifactType { .. })
        ));
        assert!(store.config_path().is_symlink());
        assert!(store.backup_path().exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_reparse_attribute_guard_recognizes_junctions() {
        assert!(super::file_attributes_include_reparse_point(
            super::FILE_ATTRIBUTE_REPARSE_POINT
        ));
        assert!(super::file_attributes_include_reparse_point(
            super::FILE_ATTRIBUTE_REPARSE_POINT | 0x10
        ));
        assert!(!super::file_attributes_include_reparse_point(0x10));
    }

    #[test]
    fn maximum_generation_cannot_wrap() {
        let root = TestDirectory::new("generation_overflow");
        let store = store(&root);
        write_config(&store.config_path(), u64::MAX, 10);

        assert!(matches!(
            store.save_if_generation(u64::MAX, Payload { sequence: 11 }),
            Err(ModuleConfigError::GenerationOverflow(value)) if value == u64::MAX
        ));
        assert_eq!(
            read_value(&store.config_path())["generation"],
            json!(u64::MAX)
        );
    }
}
