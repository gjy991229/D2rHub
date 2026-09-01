//! Sidecar configuration controller for the optional room-automation capability.
//!
//! The global v9 configuration remains a read-only compatibility source. Once
//! this controller creates the module sidecar, every later read and write is
//! sidecar-only so old binaries can keep preserving their unknown field while
//! the new capability evolves independently.

use crate::capabilities::room_automation::{
    NormalizationReport, RoomAutomationConfig, RoomAutomationConfigError,
};
use crate::infrastructure::module_config::{
    ModuleConfigEnvelope, ModuleConfigError, ModuleConfigStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub const ROOM_AUTOMATION_MODULE_ID: &str = "room-automation";
pub const ROOM_AUTOMATION_CONFIG_SCHEMA_VERSION: u32 = 1;

const LEGACY_GLOBAL_SOURCE: &str = "global-v9.room_rotation";
const MAX_CAS_ATTEMPTS: usize = 64;

/// Serializable state returned to commands and the settings UI.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RoomAutomationConfigSnapshot {
    pub schema_version: u32,
    pub generation: u64,
    pub config: RoomAutomationConfig,
    pub normalization: NormalizationReport,
    pub consent_notice: Option<ChatBindingConsentNotice>,
}

/// Explains why a legacy F13 preference was deliberately not trusted.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ChatBindingConsentNotice {
    pub source: String,
    pub original_strategy_version: u8,
    pub requires_user_reauthorization: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
struct LegacyImportMetadata {
    source: String,
    original_strategy_version: u8,
    requires_chat_binding_reauthorization: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RoomAutomationConfigControllerError {
    #[error(transparent)]
    Storage(#[from] ModuleConfigError),
    #[error(transparent)]
    InvalidConfig(#[from] RoomAutomationConfigError),
    #[error("legacy room-automation configuration is not a valid object: {message}")]
    InvalidLegacyPayload { message: String },
    #[error("room-automation sidecar has not been initialized")]
    NotInitialized,
    #[error("room-automation configuration kept changing during {attempts} merge attempts")]
    ConcurrentUpdate { attempts: usize },
    #[error("room sequence space is exhausted at {0}")]
    SequenceExhausted(u32),
}

/// Owns all module-specific persistence and compatibility policy.
#[derive(Clone, Debug)]
pub struct RoomAutomationConfigController {
    store: ModuleConfigStore,
}

impl RoomAutomationConfigController {
    pub fn new(
        app_data_dir: impl AsRef<Path>,
    ) -> Result<Self, RoomAutomationConfigControllerError> {
        Ok(Self {
            store: ModuleConfigStore::new(
                app_data_dir,
                ROOM_AUTOMATION_MODULE_ID,
                ROOM_AUTOMATION_CONFIG_SCHEMA_VERSION,
            )?,
        })
    }

    /// Loads the sidecar or creates generation one from the one-shot legacy
    /// value/defaults. A sidecar always wins, including when the caller still
    /// passes a stale global `room_rotation` value.
    pub fn load_or_initialize(
        &self,
        legacy_global_value: Option<Value>,
        account_shortcuts: &[String],
    ) -> Result<RoomAutomationConfigSnapshot, RoomAutomationConfigControllerError> {
        for _ in 0..MAX_CAS_ATTEMPTS {
            match self.store.load::<RoomAutomationConfig>()? {
                Some(envelope) => {
                    let (config, normalization) =
                        normalize_and_validate(envelope.payload.clone(), account_shortcuts)?;
                    if !normalization.changed {
                        return Ok(snapshot_from_envelope(envelope, config, normalization));
                    }

                    match self
                        .store
                        .save_if_generation(envelope.generation, config.clone())
                    {
                        Ok(saved) => {
                            return Ok(snapshot_from_envelope(saved, config, normalization));
                        }
                        Err(ModuleConfigError::GenerationConflict { .. }) => continue,
                        Err(error) => return Err(error.into()),
                    }
                }
                None => {
                    let prepared =
                        prepare_initial_config(legacy_global_value.as_ref(), account_shortcuts)?;
                    let save_result = match prepared.legacy_import {
                        Some(metadata) => self.store.save_with_legacy_import_if_generation(
                            0,
                            prepared.config.clone(),
                            serde_json::to_value(metadata).map_err(|error| {
                                RoomAutomationConfigControllerError::InvalidLegacyPayload {
                                    message: error.to_string(),
                                }
                            })?,
                        ),
                        None => self.store.save_if_generation(0, prepared.config.clone()),
                    };
                    match save_result {
                        Ok(saved) => {
                            return Ok(snapshot_from_envelope(
                                saved,
                                prepared.config,
                                prepared.normalization,
                            ));
                        }
                        Err(ModuleConfigError::GenerationConflict { .. }) => continue,
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }

        Err(RoomAutomationConfigControllerError::ConcurrentUpdate {
            attempts: MAX_CAS_ATTEMPTS,
        })
    }

    /// Validates and commits an explicit UI edit. Generation conflicts are
    /// intentionally returned to the caller rather than silently overwriting
    /// a newer edit.
    pub fn save(
        &self,
        expected_generation: u64,
        candidate: RoomAutomationConfig,
        account_shortcuts: &[String],
    ) -> Result<RoomAutomationConfigSnapshot, RoomAutomationConfigControllerError> {
        let (config, normalization) = normalize_and_validate(candidate, account_shortcuts)?;
        let saved = self
            .store
            .save_if_generation(expected_generation, config.clone())?;
        Ok(snapshot_from_envelope(saved, config, normalization))
    }

    /// Makes the next persisted sequence strictly greater than the sequence
    /// that was actually used. Conflicts reload and merge only this field, so
    /// concurrent settings edits are retained.
    pub fn advance_sequence_at_least(
        &self,
        used_sequence: u32,
    ) -> Result<RoomAutomationConfigSnapshot, RoomAutomationConfigControllerError> {
        let next_sequence = used_sequence.checked_add(1).ok_or(
            RoomAutomationConfigControllerError::SequenceExhausted(used_sequence),
        )?;
        self.merge_sidecar(|config| {
            let merged = config.next_sequence.max(next_sequence);
            let changed = merged != config.next_sequence;
            config.next_sequence = merged;
            changed
        })
    }

    /// Removes all references to a deleted account without clobbering newer UI
    /// fields. Losing the primary or the last follower disables activation but
    /// retains the remaining user preferences for easy repair.
    pub fn remove_account(
        &self,
        account_id: &str,
    ) -> Result<RoomAutomationConfigSnapshot, RoomAutomationConfigControllerError> {
        if account_id.is_empty() || account_id.trim() != account_id {
            return Err(RoomAutomationConfigError::InvalidAccountId(account_id.to_string()).into());
        }

        self.merge_sidecar(|config| {
            let mut changed = false;
            if config.primary_account_id.eq_ignore_ascii_case(account_id) {
                config.primary_account_id.clear();
                config.enabled = false;
                changed = true;
            }

            let follower_count = config.follower_account_ids.len();
            config
                .follower_account_ids
                .retain(|follower| !follower.eq_ignore_ascii_case(account_id));
            if config.follower_account_ids.len() != follower_count {
                changed = true;
                if config.follower_account_ids.is_empty() {
                    config.enabled = false;
                }
            }

            let binding_count = config.account_flow_bindings.len();
            config
                .account_flow_bindings
                .retain(|selected, _| !selected.eq_ignore_ascii_case(account_id));
            if config.account_flow_bindings.len() != binding_count {
                changed = true;
            }
            changed
        })
    }

    /// Persists the independent, explicit consent controlling automatic F13
    /// patching. This merge never overwrites concurrent settings edits.
    pub fn set_chat_binding_consent(
        &self,
        granted: bool,
    ) -> Result<RoomAutomationConfigSnapshot, RoomAutomationConfigControllerError> {
        self.merge_sidecar(|config| {
            let changed = config.chat_f13_auto_patch_enabled != granted;
            config.chat_f13_auto_patch_enabled = granted;
            changed
        })
    }

    fn merge_sidecar(
        &self,
        mut mutate: impl FnMut(&mut RoomAutomationConfig) -> bool,
    ) -> Result<RoomAutomationConfigSnapshot, RoomAutomationConfigControllerError> {
        for _ in 0..MAX_CAS_ATTEMPTS {
            let envelope = self
                .store
                .load::<RoomAutomationConfig>()?
                .ok_or(RoomAutomationConfigControllerError::NotInitialized)?;
            let (mut config, normalization) =
                normalize_and_validate(envelope.payload.clone(), &[])?;
            let mutation_changed = mutate(&mut config);
            // These internal mutations cannot introduce shortcut conflicts, and
            // validation still checks every intrinsic activation invariant.
            config.validate(std::iter::empty::<&str>())?;

            if !normalization.changed && !mutation_changed {
                return Ok(snapshot_from_envelope(envelope, config, normalization));
            }

            match self
                .store
                .save_if_generation(envelope.generation, config.clone())
            {
                Ok(saved) => return Ok(snapshot_from_envelope(saved, config, normalization)),
                Err(ModuleConfigError::GenerationConflict { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }

        Err(RoomAutomationConfigControllerError::ConcurrentUpdate {
            attempts: MAX_CAS_ATTEMPTS,
        })
    }
}

struct PreparedInitialConfig {
    config: RoomAutomationConfig,
    normalization: NormalizationReport,
    legacy_import: Option<LegacyImportMetadata>,
}

fn prepare_initial_config(
    legacy_global_value: Option<&Value>,
    account_shortcuts: &[String],
) -> Result<PreparedInitialConfig, RoomAutomationConfigControllerError> {
    let Some(legacy_value) = legacy_global_value else {
        let (config, normalization) =
            normalize_and_validate(RoomAutomationConfig::default(), account_shortcuts)?;
        return Ok(PreparedInitialConfig {
            config,
            normalization,
            legacy_import: None,
        });
    };

    let mut config: RoomAutomationConfig =
        serde_json::from_value(legacy_value.clone()).map_err(|error| {
            RoomAutomationConfigControllerError::InvalidLegacyPayload {
                message: error.to_string(),
            }
        })?;
    let original_strategy_version = config.strategy_version;
    let legacy_claimed_chat_binding_consent = config.chat_f13_auto_patch_enabled;

    // Before strategy v13 this flag could be inferred from module enablement,
    // so it did not prove user consent. Since v13 it was written only after a
    // successful explicit install and can be preserved; the service still
    // verifies every live key file before resuming a watcher.
    let consent_is_explicit = original_strategy_version >= 13;
    if !consent_is_explicit {
        config.chat_f13_auto_patch_enabled = false;
    }
    let (config, mut normalization) = normalize_and_validate(config, account_shortcuts)?;
    normalization.changed |= legacy_claimed_chat_binding_consent && !consent_is_explicit;
    let requires_chat_binding_reauthorization = !consent_is_explicit
        && (legacy_claimed_chat_binding_consent || normalization.requires_chat_binding_consent);

    Ok(PreparedInitialConfig {
        config,
        normalization,
        legacy_import: Some(LegacyImportMetadata {
            source: LEGACY_GLOBAL_SOURCE.to_string(),
            original_strategy_version,
            requires_chat_binding_reauthorization,
        }),
    })
}

fn normalize_and_validate(
    mut config: RoomAutomationConfig,
    account_shortcuts: &[String],
) -> Result<(RoomAutomationConfig, NormalizationReport), RoomAutomationConfigControllerError> {
    let normalization = config.normalize_legacy()?;
    config.validate(account_shortcuts.iter().map(String::as_str))?;
    Ok((config, normalization))
}

fn snapshot_from_envelope(
    envelope: ModuleConfigEnvelope<RoomAutomationConfig>,
    config: RoomAutomationConfig,
    normalization: NormalizationReport,
) -> RoomAutomationConfigSnapshot {
    let consent_notice = consent_notice(envelope.legacy_import.as_ref(), &config);
    RoomAutomationConfigSnapshot {
        schema_version: envelope.schema_version,
        generation: envelope.generation,
        config,
        normalization,
        consent_notice,
    }
}

fn consent_notice(
    legacy_import: Option<&Value>,
    config: &RoomAutomationConfig,
) -> Option<ChatBindingConsentNotice> {
    let metadata: LegacyImportMetadata = serde_json::from_value(legacy_import?.clone()).ok()?;
    (metadata.requires_chat_binding_reauthorization && !config.chat_f13_auto_patch_enabled)
        .then_some(ChatBindingConsentNotice {
            source: metadata.source,
            original_strategy_version: metadata.original_strategy_version,
            requires_user_reauthorization: true,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::room_automation::CURRENT_STRATEGY_VERSION;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Barrier};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "d2rhub_room_config_{label}_{}",
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

    fn controller(root: &TestDirectory) -> RoomAutomationConfigController {
        RoomAutomationConfigController::new(root.path()).unwrap()
    }

    fn enabled_config() -> RoomAutomationConfig {
        RoomAutomationConfig {
            enabled: true,
            primary_account_id: "main".to_string(),
            follower_account_ids: vec!["follower-a".to_string(), "follower-b".to_string()],
            ..RoomAutomationConfig::default()
        }
    }

    #[test]
    fn imports_the_global_value_once_without_sensitive_metadata() {
        let root = TestDirectory::new("one_shot_import");
        let controller = controller(&root);
        let mut first = enabled_config();
        first.strategy_version = 12;
        first.name_prefix = "first-".to_string();
        first.password = "secret-one".to_string();

        let initial = controller
            .load_or_initialize(Some(serde_json::to_value(first).unwrap()), &[])
            .unwrap();
        assert_eq!(initial.generation, 1);
        assert_eq!(initial.config.name_prefix, "first-");
        assert_eq!(initial.config.strategy_version, CURRENT_STRATEGY_VERSION);
        assert!(initial.consent_notice.is_some());

        let mut stale_global = enabled_config();
        stale_global.name_prefix = "must-not-win-".to_string();
        stale_global.password = "secret-two".to_string();
        stale_global.chat_f13_auto_patch_enabled = true;
        let reloaded = controller
            .load_or_initialize(Some(serde_json::to_value(stale_global).unwrap()), &[])
            .unwrap();

        assert_eq!(reloaded.generation, initial.generation);
        assert_eq!(reloaded.config.name_prefix, "first-");
        assert_eq!(reloaded.config.password, "secret-one");
        let envelope = controller
            .store
            .load::<RoomAutomationConfig>()
            .unwrap()
            .unwrap();
        let metadata = envelope.legacy_import.unwrap();
        assert_eq!(metadata["source"], LEGACY_GLOBAL_SOURCE);
        assert_eq!(metadata["original_strategy_version"], 12);
        assert_eq!(metadata["requires_chat_binding_reauthorization"], true);
        let encoded_metadata = metadata.to_string();
        assert!(!encoded_metadata.contains("secret-one"));
        assert!(!encoded_metadata.contains("secret-two"));
    }

    #[test]
    fn preserves_explicit_v16_consent_during_sidecar_import() {
        let root = TestDirectory::new("consent_preserved");
        let controller = controller(&root);
        let mut legacy = enabled_config();
        legacy.strategy_version = CURRENT_STRATEGY_VERSION;
        legacy.chat_f13_auto_patch_enabled = true;

        let imported = controller
            .load_or_initialize(Some(serde_json::to_value(legacy).unwrap()), &[])
            .unwrap();
        assert!(imported.config.chat_f13_auto_patch_enabled);
        assert!(!imported.normalization.changed);
        assert!(imported.consent_notice.is_none());
    }

    #[test]
    fn stale_ui_generation_is_rejected_without_overwriting_the_winner() {
        let root = TestDirectory::new("stale_generation");
        let controller = controller(&root);
        let initial = controller.load_or_initialize(None, &[]).unwrap();

        let mut winner = initial.config.clone();
        winner.name_prefix = "winner-".to_string();
        let saved = controller.save(initial.generation, winner, &[]).unwrap();

        let mut stale = initial.config;
        stale.name_prefix = "stale-".to_string();
        let error = controller.save(initial.generation, stale, &[]).unwrap_err();
        assert!(matches!(
            error,
            RoomAutomationConfigControllerError::Storage(ModuleConfigError::GenerationConflict {
                expected: 1,
                actual: 2
            })
        ));
        let loaded = controller.load_or_initialize(None, &[]).unwrap();
        assert_eq!(loaded.generation, saved.generation);
        assert_eq!(loaded.config.name_prefix, "winner-");
    }

    #[test]
    fn concurrent_sequence_advances_merge_to_the_max_and_keep_ui_fields() {
        let root = TestDirectory::new("sequence_merge");
        let controller = controller(&root);
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let mut configured = initial.config;
        configured.name_prefix = "kept-".to_string();
        configured.password = "kept_secret".to_string();
        controller
            .save(initial.generation, configured, &[])
            .unwrap();

        let start = Arc::new(Barrier::new(9));
        let workers = [2_u32, 80, 13, 120, 3, 99, 7, 42]
            .into_iter()
            .map(|used_sequence| {
                let controller = controller.clone();
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    controller.advance_sequence_at_least(used_sequence)
                })
            })
            .collect::<Vec<_>>();
        start.wait();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }

        let merged = controller.load_or_initialize(None, &[]).unwrap();
        assert_eq!(merged.config.next_sequence, 121);
        assert_eq!(merged.config.name_prefix, "kept-");
        assert_eq!(merged.config.password, "kept_secret");
    }

    #[test]
    fn account_removal_is_merged_and_disables_an_unusable_configuration() {
        let root = TestDirectory::new("account_removal");
        let controller = controller(&root);
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let mut enabled = enabled_config();
        enabled
            .account_flow_bindings
            .insert("follower-a".to_string(), "direct_lobby".to_string());
        let configured = controller.save(initial.generation, enabled, &[]).unwrap();

        let without_one_follower = controller.remove_account("FOLLOWER-A").unwrap();
        assert!(without_one_follower.config.enabled);
        assert_eq!(
            without_one_follower.config.follower_account_ids,
            vec!["follower-b"]
        );
        assert!(without_one_follower.config.account_flow_bindings.is_empty());

        let without_last_follower = controller.remove_account("follower-b").unwrap();
        assert!(!without_last_follower.config.enabled);
        assert!(without_last_follower.config.follower_account_ids.is_empty());

        let mut repaired = without_last_follower.config;
        repaired.enabled = true;
        repaired.follower_account_ids = vec!["new-follower".to_string()];
        let repaired = controller
            .save(without_last_follower.generation, repaired, &[])
            .unwrap();
        assert!(repaired.config.enabled);

        let without_primary = controller.remove_account("MAIN").unwrap();
        assert!(!without_primary.config.enabled);
        assert!(without_primary.config.primary_account_id.is_empty());
        assert_eq!(
            without_primary.config.follower_account_ids,
            vec!["new-follower"]
        );
        assert!(without_primary.generation > configured.generation);
    }

    #[test]
    fn unknown_envelope_fields_survive_controller_writes() {
        let root = TestDirectory::new("unknown_envelope_fields");
        let controller = controller(&root);
        let initial = controller.load_or_initialize(None, &[]).unwrap();
        let path = controller.store.config_path();
        let mut envelope: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        envelope.as_object_mut().unwrap().insert(
            "future_same_format".to_string(),
            json!({ "keep": [1, 2, 3] }),
        );
        fs::write(&path, serde_json::to_vec_pretty(&envelope).unwrap()).unwrap();

        let loaded = controller.load_or_initialize(None, &[]).unwrap();
        assert_eq!(loaded.generation, initial.generation);
        let mut candidate = loaded.config;
        candidate.name_prefix = "saved-".to_string();
        controller.save(loaded.generation, candidate, &[]).unwrap();

        let written: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(written["future_same_format"], json!({ "keep": [1, 2, 3] }));
    }

    #[test]
    fn invalid_or_future_legacy_payloads_fail_closed_without_a_sidecar() {
        let root = TestDirectory::new("invalid_legacy");
        let controller = controller(&root);
        assert!(matches!(
            controller.load_or_initialize(Some(json!("not-an-object")), &[]),
            Err(RoomAutomationConfigControllerError::InvalidLegacyPayload { .. })
        ));
        assert!(!controller.store.config_path().exists());

        let mut future = enabled_config();
        future.strategy_version = CURRENT_STRATEGY_VERSION.saturating_add(1);
        assert!(matches!(
            controller.load_or_initialize(Some(serde_json::to_value(future).unwrap()), &[]),
            Err(RoomAutomationConfigControllerError::InvalidConfig(
                RoomAutomationConfigError::UnsupportedStrategyVersion { .. }
            ))
        ));
        assert!(!controller.store.config_path().exists());
    }

    #[test]
    fn corrupt_or_future_sidecars_fail_closed_instead_of_falling_back_to_global() {
        let future_root = TestDirectory::new("future_sidecar");
        let future_controller = controller(&future_root);
        future_controller.load_or_initialize(None, &[]).unwrap();
        let future_path = future_controller.store.config_path();
        let mut future_envelope: Value =
            serde_json::from_slice(&fs::read(&future_path).unwrap()).unwrap();
        future_envelope["schema_version"] = json!(2);
        fs::write(
            &future_path,
            serde_json::to_vec_pretty(&future_envelope).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            future_controller
                .load_or_initialize(Some(serde_json::to_value(enabled_config()).unwrap()), &[]),
            Err(RoomAutomationConfigControllerError::Storage(
                ModuleConfigError::UnsupportedSchema { found: 2, .. }
            ))
        ));
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(future_path).unwrap()).unwrap()
                ["schema_version"],
            2
        );

        let corrupt_root = TestDirectory::new("corrupt_sidecar");
        let corrupt_controller = controller(&corrupt_root);
        corrupt_controller.load_or_initialize(None, &[]).unwrap();
        let corrupt_path = corrupt_controller.store.config_path();
        fs::write(&corrupt_path, "{corrupt-primary").unwrap();
        assert!(matches!(
            corrupt_controller
                .load_or_initialize(Some(serde_json::to_value(enabled_config()).unwrap()), &[]),
            Err(RoomAutomationConfigControllerError::Storage(
                ModuleConfigError::UnsafeRecovery { .. }
            ))
        ));
        assert_eq!(
            fs::read_to_string(corrupt_path).unwrap(),
            "{corrupt-primary"
        );
    }

    #[test]
    fn sequence_persistence_errors_are_returned_and_leave_payload_untouched() {
        let root = TestDirectory::new("sequence_save_error");
        let controller = controller(&root);
        controller.load_or_initialize(None, &[]).unwrap();
        let path = controller.store.config_path();
        let mut envelope: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        envelope["generation"] = json!(u64::MAX);
        fs::write(&path, serde_json::to_vec_pretty(&envelope).unwrap()).unwrap();

        assert!(matches!(
            controller.advance_sequence_at_least(10),
            Err(RoomAutomationConfigControllerError::Storage(
                ModuleConfigError::GenerationOverflow(value)
            )) if value == u64::MAX
        ));
        let unchanged: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(unchanged["payload"]["next_sequence"], 1);
    }

    #[test]
    fn exhausted_room_sequence_never_wraps_or_rewrites_the_sidecar() {
        let root = TestDirectory::new("sequence_value_overflow");
        let controller = controller(&root);
        let initial = controller.load_or_initialize(None, &[]).unwrap();

        assert!(matches!(
            controller.advance_sequence_at_least(u32::MAX),
            Err(RoomAutomationConfigControllerError::SequenceExhausted(value))
                if value == u32::MAX
        ));
        let unchanged = controller.load_or_initialize(None, &[]).unwrap();
        assert_eq!(unchanged.generation, initial.generation);
        assert_eq!(unchanged.config.next_sequence, 1);
    }

    #[test]
    fn enabled_saves_run_activation_and_account_shortcut_validation() {
        let root = TestDirectory::new("activation_validation");
        let controller = controller(&root);
        let initial = controller.load_or_initialize(None, &[]).unwrap();

        let mut incomplete = initial.config.clone();
        incomplete.enabled = true;
        assert!(matches!(
            controller.save(initial.generation, incomplete, &[]),
            Err(RoomAutomationConfigControllerError::InvalidConfig(
                RoomAutomationConfigError::MissingPrimaryAccount
            ))
        ));

        let enabled = enabled_config();
        assert!(matches!(
            controller.save(initial.generation, enabled, &["Ctrl+Alt+R".to_string()]),
            Err(RoomAutomationConfigControllerError::InvalidConfig(
                RoomAutomationConfigError::AccountShortcutConflict(_)
            ))
        ));
    }
}
