use parking_lot::{Mutex, RwLock};
use serde_json::Value;

use crate::domain::config::GlobalConfig;
use crate::error::AppError;

const CONFIG_NOT_LOADED_MESSAGE: &str = "尚未完成首次配置";
const UNSAFE_CONFIG_OVERWRITE_MESSAGE: &str =
    "现有配置未能安全加载，已阻止用新配置覆盖；请先检查日志和 global_config.json";

/// Persistence boundary for the global configuration.
///
/// Filesystem layout, recovery and atomic-write details belong in an adapter;
/// the application runtime only coordinates their ordering with its cache.
pub trait ConfigurationRepository: Send + Sync {
    fn load(&self) -> Result<GlobalConfig, AppError>;

    fn save(&self, config: &GlobalConfig) -> Result<(), AppError>;

    fn artifacts_exist(&self) -> bool;

    fn ensure_directories(&self, config: &GlobalConfig) -> Result<(), AppError>;
}

/// Compatibility and validation boundary applied before a configuration is
/// persisted. Migrations and module-specific normalization remain outside the
/// transaction coordinator.
pub trait ConfigurationPolicy: Send + Sync {
    fn apply_patch(&self, current: &GlobalConfig, patch: Value) -> Result<GlobalConfig, AppError>;

    fn prepare(
        &self,
        previous: Option<&GlobalConfig>,
        candidate: GlobalConfig,
    ) -> Result<GlobalConfig, AppError>;
}

/// Infallible, bounded projections derived from committed configuration.
/// Implementations must not call back into `ConfigurationRuntime`; publication
/// runs inside the configuration transaction to preserve commit order.
pub trait ConfigurationObserver: Send + Sync {
    /// Applies bounded runtime projections before the new snapshot becomes
    /// visible to concurrent readers.
    fn apply(&self, _config: &GlobalConfig) {}

    /// Publishes the already-visible committed snapshot to external readers.
    fn publish(&self, config: &GlobalConfig);
}

#[derive(Debug, Clone)]
pub struct ConfigurationLoad {
    pub config: GlobalConfig,
}

#[derive(Debug, Clone)]
pub enum ConfigurationMutation {
    Missing,
    Unchanged,
    Updated,
}

/// Serializes configuration transactions and keeps the in-memory snapshot in
/// sync with the last fully successful persistence operation.
#[derive(Default)]
pub struct ConfigurationRuntime {
    transaction: Mutex<()>,
    cache: RwLock<Option<GlobalConfig>>,
}

impl ConfigurationRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Option<GlobalConfig> {
        let _transaction = self.transaction.lock();
        self.cache.read().clone()
    }

    /// Reprojects bounded runtime intent in commit order without writing disk.
    /// The callback must not re-enter configuration or perform native UI/I/O.
    pub(crate) fn project_current<T>(&self, project: impl FnOnce(Option<&GlobalConfig>) -> T) -> T {
        let _transaction = self.transaction.lock();
        let current = self.cache.read();
        project(current.as_ref())
    }

    /// Loads once even when several callers arrive concurrently. Reads share
    /// the transaction barrier so no caller can observe a half-published
    /// configuration epoch.
    pub fn get_or_load(
        &self,
        repository: &dyn ConfigurationRepository,
        observer: &dyn ConfigurationObserver,
    ) -> Result<ConfigurationLoad, AppError> {
        let _transaction = self.transaction.lock();
        if let Some(config) = self.cache.read().clone() {
            return Ok(ConfigurationLoad { config });
        }

        let config = repository.load()?;
        observer.apply(&config);
        *self.cache.write() = Some(config.clone());
        observer.publish(&config);
        Ok(ConfigurationLoad { config })
    }

    /// Saves an externally supplied candidate. When configuration artifacts
    /// exist but none could be loaded, overwriting them is deliberately denied.
    pub fn save_candidate(
        &self,
        repository: &dyn ConfigurationRepository,
        policy: &dyn ConfigurationPolicy,
        observer: &dyn ConfigurationObserver,
        candidate: GlobalConfig,
    ) -> Result<GlobalConfig, AppError> {
        let _transaction = self.transaction.lock();
        let previous = self.cache.read().clone();
        if previous.is_none() && repository.artifacts_exist() {
            return Err(AppError::ConfigWriteError(
                UNSAFE_CONFIG_OVERWRITE_MESSAGE.to_string(),
            ));
        }

        let prepared = policy.prepare(previous.as_ref(), candidate)?;
        self.commit(repository, observer, prepared)
    }

    /// Applies a user patch to the latest cached value while holding the full
    /// transaction, so a concurrent save cannot be overwritten by a stale base.
    pub fn patch_current(
        &self,
        repository: &dyn ConfigurationRepository,
        policy: &dyn ConfigurationPolicy,
        observer: &dyn ConfigurationObserver,
        patch: Value,
    ) -> Result<GlobalConfig, AppError> {
        let _transaction = self.transaction.lock();
        let previous = self
            .cache
            .read()
            .clone()
            .ok_or_else(|| AppError::ConfigReadError(CONFIG_NOT_LOADED_MESSAGE.to_string()))?;
        let candidate = policy.apply_patch(&previous, patch)?;
        let prepared = policy.prepare(Some(&previous), candidate)?;
        self.commit(repository, observer, prepared)
    }

    /// Mutates an already loaded configuration without implicitly creating or
    /// loading one. The callback reports whether persistence is necessary.
    pub fn mutate_if_loaded<F>(
        &self,
        repository: &dyn ConfigurationRepository,
        policy: &dyn ConfigurationPolicy,
        observer: &dyn ConfigurationObserver,
        mutate: F,
    ) -> Result<ConfigurationMutation, AppError>
    where
        F: FnOnce(&mut GlobalConfig) -> Result<bool, AppError>,
    {
        self.mutate_if_loaded_with_post_commit(repository, policy, observer, mutate, |_| {})
    }

    /// Runs a bounded, infallible post-commit action before another
    /// configuration writer may enter the transaction. The action must not call
    /// back into this runtime. This is intended for durable lifecycle work whose
    /// filesystem state must stay linearized with the configuration commit.
    pub fn mutate_if_loaded_with_post_commit<F, P>(
        &self,
        repository: &dyn ConfigurationRepository,
        policy: &dyn ConfigurationPolicy,
        observer: &dyn ConfigurationObserver,
        mutate: F,
        post_commit: P,
    ) -> Result<ConfigurationMutation, AppError>
    where
        F: FnOnce(&mut GlobalConfig) -> Result<bool, AppError>,
        P: FnOnce(&GlobalConfig),
    {
        let _transaction = self.transaction.lock();
        let Some(previous) = self.cache.read().clone() else {
            return Ok(ConfigurationMutation::Missing);
        };
        let mut candidate = previous.clone();
        if !mutate(&mut candidate)? {
            post_commit(&previous);
            return Ok(ConfigurationMutation::Unchanged);
        }

        let prepared = policy.prepare(Some(&previous), candidate)?;
        let committed = self.commit(repository, observer, prepared)?;
        post_commit(&committed);
        Ok(ConfigurationMutation::Updated)
    }

    fn commit(
        &self,
        repository: &dyn ConfigurationRepository,
        observer: &dyn ConfigurationObserver,
        prepared: GlobalConfig,
    ) -> Result<GlobalConfig, AppError> {
        repository.ensure_directories(&prepared)?;
        repository.save(&prepared)?;
        observer.apply(&prepared);
        *self.cache.write() = Some(prepared.clone());
        observer.publish(&prepared);
        Ok(prepared)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex as StdMutex};
    use std::time::Duration;

    use serde_json::Value;

    use super::{
        ConfigurationMutation, ConfigurationObserver, ConfigurationPolicy, ConfigurationRepository,
        ConfigurationRuntime,
    };
    use crate::domain::config::GlobalConfig;
    use crate::error::AppError;

    #[derive(Clone)]
    struct FakeRepository {
        state: Arc<FakeRepositoryState>,
    }

    struct FakeRepositoryState {
        loaded: StdMutex<GlobalConfig>,
        saved: StdMutex<Vec<GlobalConfig>>,
        load_calls: AtomicUsize,
        ensure_calls: AtomicUsize,
        artifacts: AtomicBool,
        fail_load: AtomicBool,
        fail_save: AtomicBool,
        fail_ensure: AtomicBool,
        slow_load: AtomicBool,
    }

    impl FakeRepository {
        fn with_loaded(config: GlobalConfig) -> Self {
            Self {
                state: Arc::new(FakeRepositoryState {
                    loaded: StdMutex::new(config),
                    saved: StdMutex::new(Vec::new()),
                    load_calls: AtomicUsize::new(0),
                    ensure_calls: AtomicUsize::new(0),
                    artifacts: AtomicBool::new(false),
                    fail_load: AtomicBool::new(false),
                    fail_save: AtomicBool::new(false),
                    fail_ensure: AtomicBool::new(false),
                    slow_load: AtomicBool::new(false),
                }),
            }
        }

        fn save_calls(&self) -> usize {
            self.state.saved.lock().expect("saved config lock").len()
        }
    }

    impl ConfigurationRepository for FakeRepository {
        fn load(&self) -> Result<GlobalConfig, AppError> {
            self.state.load_calls.fetch_add(1, Ordering::SeqCst);
            if self.state.fail_load.load(Ordering::SeqCst) {
                return Err(AppError::ConfigReadError("fake load failure".to_string()));
            }
            if self.state.slow_load.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(self
                .state
                .loaded
                .lock()
                .expect("loaded config lock")
                .clone())
        }

        fn save(&self, config: &GlobalConfig) -> Result<(), AppError> {
            if self.state.fail_save.load(Ordering::SeqCst) {
                return Err(AppError::ConfigWriteError("fake save failure".to_string()));
            }
            self.state
                .saved
                .lock()
                .expect("saved config lock")
                .push(config.clone());
            Ok(())
        }

        fn artifacts_exist(&self) -> bool {
            self.state.artifacts.load(Ordering::SeqCst)
        }

        fn ensure_directories(&self, _config: &GlobalConfig) -> Result<(), AppError> {
            self.state.ensure_calls.fetch_add(1, Ordering::SeqCst);
            if self.state.fail_ensure.load(Ordering::SeqCst) {
                return Err(AppError::ConfigWriteError(
                    "fake directory failure".to_string(),
                ));
            }
            Ok(())
        }
    }

    struct FakePolicy;

    struct NoopObserver;

    impl ConfigurationObserver for NoopObserver {
        fn publish(&self, _config: &GlobalConfig) {}
    }

    impl ConfigurationPolicy for FakePolicy {
        fn apply_patch(
            &self,
            current: &GlobalConfig,
            patch: Value,
        ) -> Result<GlobalConfig, AppError> {
            let mut merged = serde_json::to_value(current)?;
            let patch = patch.as_object().ok_or_else(|| {
                AppError::ConfigWriteError("配置补丁必须是 JSON 对象".to_string())
            })?;
            let merged = merged.as_object_mut().ok_or_else(|| {
                AppError::ConfigWriteError("当前配置无法转换为 JSON 对象".to_string())
            })?;
            for (key, value) in patch {
                merged.insert(key.clone(), value.clone());
            }
            serde_json::from_value(Value::Object(merged.clone())).map_err(Into::into)
        }

        fn prepare(
            &self,
            _previous: Option<&GlobalConfig>,
            candidate: GlobalConfig,
        ) -> Result<GlobalConfig, AppError> {
            Ok(candidate)
        }
    }

    #[test]
    fn get_or_load_reads_the_repository_only_once() {
        let repository = FakeRepository::with_loaded(GlobalConfig {
            theme: "onyx".to_string(),
            ..GlobalConfig::default()
        });
        let runtime = ConfigurationRuntime::new();

        let first = runtime.get_or_load(&repository, &NoopObserver).unwrap();
        let second = runtime.get_or_load(&repository, &NoopObserver).unwrap();

        assert_eq!(first.config.theme, "onyx");
        assert_eq!(second.config.theme, "onyx");
        assert_eq!(repository.state.load_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_get_or_load_calls_share_one_repository_read() {
        const CALLERS: usize = 8;
        let repository = FakeRepository::with_loaded(GlobalConfig {
            theme: "onyx".to_string(),
            ..GlobalConfig::default()
        });
        repository.state.slow_load.store(true, Ordering::SeqCst);
        let runtime = Arc::new(ConfigurationRuntime::new());
        let start = Arc::new(Barrier::new(CALLERS + 1));

        let callers = (0..CALLERS)
            .map(|_| {
                let repository = repository.clone();
                let runtime = Arc::clone(&runtime);
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    runtime
                        .get_or_load(&repository, &NoopObserver)
                        .expect("configuration load")
                        .config
                        .theme
                })
            })
            .collect::<Vec<_>>();
        start.wait();

        for caller in callers {
            assert_eq!(caller.join().expect("configuration caller"), "onyx");
        }
        assert_eq!(repository.state.load_calls.load(Ordering::SeqCst), 1);
        assert_eq!(runtime.snapshot().unwrap().theme, "onyx");
    }

    #[test]
    fn candidate_save_fails_closed_when_artifacts_could_not_be_loaded() {
        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        repository.state.artifacts.store(true, Ordering::SeqCst);
        let runtime = ConfigurationRuntime::new();

        let error = runtime
            .save_candidate(
                &repository,
                &FakePolicy,
                &NoopObserver,
                GlobalConfig::default(),
            )
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "配置写入失败: 现有配置未能安全加载，已阻止用新配置覆盖；请先检查日志和 global_config.json"
        );
        assert!(runtime.snapshot().is_none());
        assert_eq!(repository.save_calls(), 0);
        assert_eq!(repository.state.ensure_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn failed_artifact_load_cannot_be_followed_by_an_unsafe_overwrite() {
        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        repository.state.artifacts.store(true, Ordering::SeqCst);
        repository.state.fail_load.store(true, Ordering::SeqCst);
        let runtime = ConfigurationRuntime::new();

        assert!(runtime.get_or_load(&repository, &NoopObserver).is_err());
        let overwrite = runtime
            .save_candidate(
                &repository,
                &FakePolicy,
                &NoopObserver,
                GlobalConfig::default(),
            )
            .unwrap_err();

        assert!(overwrite.to_string().contains("已阻止用新配置覆盖"));
        assert!(runtime.snapshot().is_none());
        assert_eq!(repository.save_calls(), 0);
    }

    #[test]
    fn failed_save_does_not_replace_the_cached_configuration() {
        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        let runtime = ConfigurationRuntime::new();
        runtime.get_or_load(&repository, &NoopObserver).unwrap();
        repository.state.fail_save.store(true, Ordering::SeqCst);

        let candidate = GlobalConfig {
            theme: "onyx".to_string(),
            ..GlobalConfig::default()
        };
        assert!(runtime
            .save_candidate(&repository, &FakePolicy, &NoopObserver, candidate)
            .is_err());

        assert_eq!(runtime.snapshot().unwrap().theme, "light");
        assert_eq!(repository.state.ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_patch_and_mutation_do_not_replace_the_cached_configuration() {
        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        let runtime = ConfigurationRuntime::new();
        runtime.get_or_load(&repository, &NoopObserver).unwrap();
        repository.state.fail_save.store(true, Ordering::SeqCst);

        assert!(runtime
            .patch_current(
                &repository,
                &FakePolicy,
                &NoopObserver,
                serde_json::json!({ "theme": "onyx" }),
            )
            .is_err());
        assert_eq!(runtime.snapshot().unwrap().theme, "light");

        let post_commit_called = AtomicBool::new(false);
        assert!(runtime
            .mutate_if_loaded_with_post_commit(
                &repository,
                &FakePolicy,
                &NoopObserver,
                |config| {
                    config.theme = "onyx".to_string();
                    Ok(true)
                },
                |_| post_commit_called.store(true, Ordering::SeqCst),
            )
            .is_err());
        assert!(!post_commit_called.load(Ordering::SeqCst));
        assert_eq!(runtime.snapshot().unwrap().theme, "light");
        assert_eq!(repository.save_calls(), 0);
    }

    #[test]
    fn patch_is_applied_to_the_latest_cached_value() {
        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        let runtime = ConfigurationRuntime::new();
        runtime.get_or_load(&repository, &NoopObserver).unwrap();
        runtime
            .save_candidate(
                &repository,
                &FakePolicy,
                &NoopObserver,
                GlobalConfig {
                    font_scale: "large".to_string(),
                    ..GlobalConfig::default()
                },
            )
            .unwrap();

        let patched = runtime
            .patch_current(
                &repository,
                &FakePolicy,
                &NoopObserver,
                serde_json::json!({ "theme": "onyx" }),
            )
            .unwrap();

        assert_eq!(patched.theme, "onyx");
        assert_eq!(patched.font_scale, "large");
    }

    #[test]
    fn concurrent_patches_do_not_lose_unrelated_fields() {
        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        let runtime = Arc::new(ConfigurationRuntime::new());
        runtime.get_or_load(&repository, &NoopObserver).unwrap();
        let start = Arc::new(Barrier::new(3));

        let theme_patch = {
            let repository = repository.clone();
            let runtime = Arc::clone(&runtime);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                runtime
                    .patch_current(
                        &repository,
                        &FakePolicy,
                        &NoopObserver,
                        serde_json::json!({ "theme": "onyx" }),
                    )
                    .unwrap();
            })
        };
        let font_patch = {
            let repository = repository.clone();
            let runtime = Arc::clone(&runtime);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                runtime
                    .patch_current(
                        &repository,
                        &FakePolicy,
                        &NoopObserver,
                        serde_json::json!({ "font_scale": "large" }),
                    )
                    .unwrap();
            })
        };
        start.wait();
        theme_patch.join().unwrap();
        font_patch.join().unwrap();

        let config = runtime.snapshot().unwrap();
        assert_eq!(config.theme, "onyx");
        assert_eq!(config.font_scale, "large");
        assert_eq!(repository.save_calls(), 2);
    }

    #[test]
    fn loaded_mutation_distinguishes_missing_unchanged_and_updated() {
        let missing_repository = FakeRepository::with_loaded(GlobalConfig::default());
        let missing_runtime = ConfigurationRuntime::new();
        assert!(matches!(
            missing_runtime
                .mutate_if_loaded(&missing_repository, &FakePolicy, &NoopObserver, |_| Ok(
                    true
                ))
                .unwrap(),
            ConfigurationMutation::Missing
        ));

        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        let runtime = ConfigurationRuntime::new();
        runtime.get_or_load(&repository, &NoopObserver).unwrap();
        let post_commit_called = AtomicBool::new(false);
        let unchanged = runtime
            .mutate_if_loaded_with_post_commit(
                &repository,
                &FakePolicy,
                &NoopObserver,
                |_| Ok(false),
                |_| post_commit_called.store(true, Ordering::SeqCst),
            )
            .unwrap();
        assert!(matches!(unchanged, ConfigurationMutation::Unchanged));
        assert!(post_commit_called.load(Ordering::SeqCst));
        assert_eq!(repository.save_calls(), 0);

        let updated = runtime
            .mutate_if_loaded(&repository, &FakePolicy, &NoopObserver, |config| {
                config.theme = "onyx".to_string();
                Ok(true)
            })
            .unwrap();
        let ConfigurationMutation::Updated = updated else {
            panic!("changed mutation must be reported as updated");
        };
        assert_eq!(runtime.snapshot().unwrap().theme, "onyx");
        assert_eq!(repository.save_calls(), 1);
    }

    #[test]
    fn post_commit_work_keeps_later_writers_out_of_the_transaction() {
        struct EntryPolicy {
            entered: std::sync::mpsc::SyncSender<()>,
        }

        impl ConfigurationPolicy for EntryPolicy {
            fn apply_patch(
                &self,
                current: &GlobalConfig,
                patch: Value,
            ) -> Result<GlobalConfig, AppError> {
                self.entered.send(()).unwrap();
                FakePolicy.apply_patch(current, patch)
            }

            fn prepare(
                &self,
                previous: Option<&GlobalConfig>,
                candidate: GlobalConfig,
            ) -> Result<GlobalConfig, AppError> {
                FakePolicy.prepare(previous, candidate)
            }
        }

        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        let runtime = Arc::new(ConfigurationRuntime::new());
        runtime.get_or_load(&repository, &NoopObserver).unwrap();
        let post_entered = Arc::new(Barrier::new(2));
        let release_post = Arc::new(Barrier::new(2));

        let first = {
            let repository = repository.clone();
            let runtime = Arc::clone(&runtime);
            let post_entered = Arc::clone(&post_entered);
            let release_post = Arc::clone(&release_post);
            std::thread::spawn(move || {
                runtime
                    .mutate_if_loaded_with_post_commit(
                        &repository,
                        &FakePolicy,
                        &NoopObserver,
                        |config| {
                            config.theme = "onyx".to_string();
                            Ok(true)
                        },
                        |_| {
                            post_entered.wait();
                            release_post.wait();
                        },
                    )
                    .unwrap();
            })
        };
        post_entered.wait();

        let (second_started, second_start) = std::sync::mpsc::sync_channel(0);
        let (policy_entered, policy_entry) = std::sync::mpsc::sync_channel(1);
        let second = {
            let repository = repository.clone();
            let runtime = Arc::clone(&runtime);
            std::thread::spawn(move || {
                second_started.send(()).unwrap();
                let policy = EntryPolicy {
                    entered: policy_entered,
                };
                runtime
                    .patch_current(
                        &repository,
                        &policy,
                        &NoopObserver,
                        serde_json::json!({ "font_scale": "large" }),
                    )
                    .unwrap();
            })
        };

        second_start.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(policy_entry
            .recv_timeout(Duration::from_millis(25))
            .is_err());
        assert_eq!(repository.save_calls(), 1);
        release_post.wait();
        policy_entry.recv_timeout(Duration::from_secs(1)).unwrap();
        first.join().unwrap();
        second.join().unwrap();

        let config = runtime.snapshot().unwrap();
        assert_eq!(config.theme, "onyx");
        assert_eq!(config.font_scale, "large");
        assert_eq!(repository.save_calls(), 2);
    }

    #[test]
    fn committed_projections_are_published_in_transaction_order() {
        struct BlockingObserver {
            calls: AtomicUsize,
            first_entered: Barrier,
            release_first: Barrier,
            second_published: std::sync::mpsc::SyncSender<()>,
            published: StdMutex<Vec<GlobalConfig>>,
        }

        impl ConfigurationObserver for BlockingObserver {
            fn publish(&self, config: &GlobalConfig) {
                let call = self.calls.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    self.first_entered.wait();
                    self.release_first.wait();
                }
                self.published.lock().unwrap().push(config.clone());
                if call == 1 {
                    self.second_published.send(()).unwrap();
                }
            }
        }

        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        let runtime = Arc::new(ConfigurationRuntime::new());
        runtime.get_or_load(&repository, &NoopObserver).unwrap();
        let (second_published, second_publication) = std::sync::mpsc::sync_channel(1);
        let observer = Arc::new(BlockingObserver {
            calls: AtomicUsize::new(0),
            first_entered: Barrier::new(2),
            release_first: Barrier::new(2),
            second_published,
            published: StdMutex::new(Vec::new()),
        });

        let first = {
            let repository = repository.clone();
            let runtime = Arc::clone(&runtime);
            let observer = Arc::clone(&observer);
            std::thread::spawn(move || {
                runtime
                    .patch_current(
                        &repository,
                        &FakePolicy,
                        observer.as_ref(),
                        serde_json::json!({ "theme": "onyx" }),
                    )
                    .unwrap();
            })
        };
        observer.first_entered.wait();
        let (second_started, second_start) = std::sync::mpsc::sync_channel(1);
        let second = {
            let repository = repository.clone();
            let runtime = Arc::clone(&runtime);
            let observer = Arc::clone(&observer);
            std::thread::spawn(move || {
                second_started.send(()).unwrap();
                runtime
                    .patch_current(
                        &repository,
                        &FakePolicy,
                        observer.as_ref(),
                        serde_json::json!({ "font_scale": "large" }),
                    )
                    .unwrap();
            })
        };

        second_start.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(second_publication
            .recv_timeout(Duration::from_millis(25))
            .is_err());
        observer.release_first.wait();
        first.join().unwrap();
        second.join().unwrap();
        second_publication
            .recv_timeout(Duration::from_secs(1))
            .unwrap();

        let published = observer.published.lock().unwrap();
        assert_eq!(published.len(), 2);
        assert_eq!(published[0].theme, "onyx");
        assert_eq!(published[1].theme, "onyx");
        assert_eq!(published[1].font_scale, "large");
    }

    #[test]
    fn candidate_directory_failure_keeps_the_previous_cache() {
        let repository = FakeRepository::with_loaded(GlobalConfig::default());
        let runtime = ConfigurationRuntime::new();
        runtime.get_or_load(&repository, &NoopObserver).unwrap();
        repository.state.fail_ensure.store(true, Ordering::SeqCst);

        assert!(runtime
            .save_candidate(
                &repository,
                &FakePolicy,
                &NoopObserver,
                GlobalConfig {
                    theme: "onyx".to_string(),
                    ..GlobalConfig::default()
                },
            )
            .is_err());

        assert_eq!(repository.save_calls(), 0);
        assert_eq!(repository.state.ensure_calls.load(Ordering::SeqCst), 1);
        assert_eq!(runtime.snapshot().unwrap().theme, "light");
    }
}
