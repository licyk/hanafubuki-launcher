use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::internal::hanafubuki_config::{BootstrapPaths, ManagedRuntimePaths};
use crate::internal::hanafubuki_foundation::paths::{
    PathKey, normalize_path, validate_not_dangerous,
};
use parking_lot::Mutex;

use crate::internal::hanafubuki_runtime_storage::model::{
    ManagedRuntimeStorageSnapshot, MigrationComponentTarget, MigrationPhase,
    RUNTIME_STORAGE_SCHEMA_VERSION, RuntimeStorageJournal, RuntimeStorageMigrationPlan,
};
use crate::internal::hanafubuki_runtime_storage::repository::{
    ConfigRead, RuntimeStorageProcessLock, commit_config, load_config, load_journal, write_journal,
};
use crate::internal::hanafubuki_runtime_storage::transfer::{
    cleanup_sources, execute_transfer, rollback_published_destination,
};

struct ServiceInner {
    bootstrap: BootstrapPaths,
    _process_lock: RuntimeStorageProcessLock,
    snapshot: Mutex<ManagedRuntimeStorageSnapshot>,
    /// Whether a source cleanup runs, in this process: one at a time,
    /// whichever mount of the module started it.
    cleanup_running: AtomicBool,
}

/// The managed runtime storage authority: its process lock, the paths in
/// force, this start's migration, and the durable record of a move a user
/// plans. Its background cleanup is [`crate::internal::hanafubuki_runtime_storage::ManagedRuntimeStorageOperations`].
#[derive(Clone)]
pub struct ManagedRuntimeStorageService {
    inner: Arc<ServiceInner>,
}

impl ManagedRuntimeStorageService {
    /// Acquire the process lock and resolve the currently configured paths.
    ///
    /// Opening never transfers anything. A pending migration is durable work
    /// that can take minutes, so it is left for `run_startup_migration`, which
    /// the composition root drives while the window is already painting.
    pub fn open(app_data_dir: PathBuf) -> Result<Self, String> {
        let bootstrap = BootstrapPaths::from_app_data_dir(app_data_dir)?;
        let process_lock = RuntimeStorageProcessLock::acquire(&bootstrap)?;
        let journal = load_journal(&bootstrap)?;
        let snapshot = resolve_snapshot(&bootstrap, journal)?;
        Ok(Self {
            inner: Arc::new(ServiceInner {
                bootstrap,
                _process_lock: process_lock,
                snapshot: Mutex::new(snapshot),
                cleanup_running: AtomicBool::new(false),
            }),
        })
    }

    /// Whether this start owes durable migration work before the application
    /// paths are final. The caller must not resolve `AppPaths` until
    /// `run_startup_migration` has returned.
    ///
    /// A scheduled migration waits while the config needs attention: its
    /// commit would replace that config, so it is not started at all.
    pub fn has_startup_migration(&self) -> bool {
        let snapshot = self.inner.snapshot.lock();
        snapshot.config_attention.is_none()
            && snapshot
                .pending
                .as_ref()
                .is_some_and(startup_migration_pending)
    }

    /// Perform the pending transfer and config commit, then republish the
    /// resolved paths. Source cleanup runs after the application becomes ready.
    ///
    /// A migration failure is not fatal: the journal records
    /// `RecoveryRequired` and the previously configured paths stay in force, so
    /// the application still starts on the storage it was already using.
    pub fn run_startup_migration(&self) -> Result<(), String> {
        if !self.has_startup_migration() {
            return Ok(());
        }
        let bootstrap = &self.inner.bootstrap;
        let mut journal = self.inner.snapshot.lock().pending.clone();
        let mut failure = None;
        if let Some(pending) = journal.as_mut() {
            if !startup_migration_pending(pending) {
                return Ok(());
            }
            if pending.phase.may_migrate_at_startup() {
                if let Err(error) = recover_migration(bootstrap, pending) {
                    let committed = load_config(bootstrap)
                        .ok()
                        .and_then(ConfigRead::valid)
                        .as_ref()
                        .is_some_and(|config| config_matches_destination(config, pending));
                    pending.phase = if committed {
                        MigrationPhase::CleanupPending
                    } else {
                        MigrationPhase::RecoveryRequired
                    };
                    pending.last_error = Some(bounded_error(error.clone()));
                    if let Err(persist_error) = write_journal(bootstrap, pending) {
                        failure = Some(format!(
                            "{error}; persist migration failure: {persist_error}"
                        ));
                    } else {
                        failure = Some(error);
                    }
                }
            }
        }
        let snapshot = resolve_snapshot(bootstrap, journal)?;
        *self.inner.snapshot.lock() = snapshot;
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub fn snapshot(&self) -> ManagedRuntimeStorageSnapshot {
        self.inner.snapshot.lock().clone()
    }

    /// Rebase the configured root after the application-data parent directory
    /// and its complete contents were moved together. This records the new
    /// default location without transferring files because the caller already
    /// moved the owning tree.
    ///
    /// A config that needs attention is left as it is: nothing is rebased.
    pub fn rebase_to_default_after_parent_relocation(&self) -> Result<bool, String> {
        let destination_root = self.inner.bootstrap.default_managed_runtime_root();
        let mut snapshot = self.inner.snapshot.lock();
        if let Some(attention) = &snapshot.config_attention {
            log::warn!(
                "managed runtime root was not rebased: its config needs attention: {}",
                attention.message
            );
            return Ok(false);
        }
        if PathKey::of(&snapshot.current_root) == PathKey::of(&destination_root) {
            return Ok(false);
        }

        let paths = ManagedRuntimePaths::under(destination_root.clone())?;
        let layout = paths.layout();
        let config = commit_config(
            &self.inner.bootstrap,
            snapshot.config_revision,
            &destination_root,
        )?;
        snapshot.config_revision = config.revision;
        snapshot.current_layout = layout;
        snapshot.current_root = destination_root;
        snapshot.paths = paths;
        Ok(true)
    }

    /// Validate a candidate destination and resolve where each managed
    /// component will live afterwards. This is path arithmetic plus the
    /// candidate checks; it never inspects source file contents or sizes.
    pub fn plan(
        &self,
        destination_root: PathBuf,
        protected_paths: &[PathBuf],
    ) -> Result<RuntimeStorageMigrationPlan, String> {
        let snapshot = self.snapshot();
        ensure_can_move(&snapshot)?;
        let destination_root = resolve_destination_root(&self.inner.bootstrap, destination_root)?;
        validate_candidate(
            &destination_root,
            &snapshot.paths,
            protected_paths,
            &self.inner.bootstrap,
        )?;
        let destination = ManagedRuntimePaths::under(destination_root.clone())?;
        Ok(RuntimeStorageMigrationPlan {
            destination_root,
            components: component_targets(&destination),
        })
    }

    /// Record the confirmed migration in the durable journal. Nothing is copied
    /// here: the transfer runs on the next start, whichever way this run ends.
    pub fn schedule(
        &self,
        destination_root: PathBuf,
        protected_paths: &[PathBuf],
    ) -> Result<ManagedRuntimeStorageSnapshot, String> {
        let snapshot = self.snapshot();
        ensure_can_move(&snapshot)?;
        let destination_root = resolve_destination_root(&self.inner.bootstrap, destination_root)?;
        validate_candidate(
            &destination_root,
            &snapshot.paths,
            protected_paths,
            &self.inner.bootstrap,
        )?;
        let operation_id = operation_id()?;
        let journal = RuntimeStorageJournal {
            schema_version: RUNTIME_STORAGE_SCHEMA_VERSION,
            operation_id,
            expected_config_revision: snapshot.config_revision,
            source_layout: snapshot.current_layout,
            source_root: snapshot.current_root.clone(),
            destination_root,
            phase: MigrationPhase::Scheduled,
            last_error: None,
        };
        write_journal(&self.inner.bootstrap, &journal)?;
        let mut guard = self.inner.snapshot.lock();
        guard.pending = Some(journal);
        Ok(guard.clone())
    }

    pub fn cancel_scheduled(&self) -> Result<ManagedRuntimeStorageSnapshot, String> {
        let mut guard = self.inner.snapshot.lock();
        let Some(pending) = guard.pending.as_ref() else {
            return Ok(guard.clone());
        };
        if pending.phase != MigrationPhase::Scheduled {
            return Err(
                "managed runtime migration can only be cancelled while scheduled".to_string(),
            );
        }
        std::fs::remove_file(&self.inner.bootstrap.runtime_storage_journal_file).map_err(
            |error| {
                format!(
                    "remove {}: {error}",
                    self.inner.bootstrap.runtime_storage_journal_file.display()
                )
            },
        )?;
        guard.pending = None;
        Ok(guard.clone())
    }

    pub fn retry_recovery_on_next_restart(&self) -> Result<ManagedRuntimeStorageSnapshot, String> {
        let mut guard = self.inner.snapshot.lock();
        let Some(pending) = guard.pending.as_ref() else {
            return Err("there is no managed runtime migration to retry".to_string());
        };
        if pending.phase != MigrationPhase::RecoveryRequired {
            return Err("managed runtime migration is not awaiting recovery".to_string());
        }
        // Published only once it is durable: a failed write leaves the
        // journal awaiting recovery in memory as it is on disk.
        let journal = RuntimeStorageJournal {
            phase: MigrationPhase::Scheduled,
            last_error: None,
            ..pending.clone()
        };
        write_journal(&self.inner.bootstrap, &journal)?;
        guard.pending = Some(journal);
        Ok(guard.clone())
    }

    /// Set while a source cleanup runs.
    pub(crate) fn cleanup_running(&self) -> &AtomicBool {
        &self.inner.cleanup_running
    }

    /// Removes a committed migration's sources and records the outcome.
    pub(crate) fn complete_cleanup(
        &self,
        cancellation: &crate::internal::hanafubuki_foundation::cancellation::CancellationToken,
    ) -> Result<(), String> {
        let mut journal = self
            .snapshot()
            .pending
            .filter(|journal| journal.phase == MigrationPhase::CleanupPending)
            .ok_or_else(|| "managed runtime cleanup is no longer pending".to_string())?;
        let result = (|| {
            let config = load_config(&self.inner.bootstrap)?.valid().ok_or_else(|| {
                "managed runtime config is no longer usable before source cleanup".to_string()
            })?;
            if !config_matches_destination(&config, &journal) {
                return Err("managed runtime config changed before source cleanup".to_string());
            }
            cleanup_sources(&journal, cancellation)
        })();
        match result {
            Ok(()) => {
                journal.phase = MigrationPhase::Completed;
                journal.last_error = None;
            }
            Err(error) => {
                journal.last_error = Some(bounded_error(error.clone()));
                write_journal(&self.inner.bootstrap, &journal)?;
                self.inner.snapshot.lock().pending = Some(journal);
                return Err(error);
            }
        }
        write_journal(&self.inner.bootstrap, &journal)?;
        self.inner.snapshot.lock().pending = Some(journal);
        Ok(())
    }
}

/// Moving storage is refused while a migration still needs attention or the
/// config cannot be used: the move would commit a new config over it.
fn ensure_can_move(snapshot: &ManagedRuntimeStorageSnapshot) -> Result<(), String> {
    if snapshot
        .pending
        .as_ref()
        .is_some_and(|pending| !pending.phase.is_terminal())
    {
        return Err("a managed runtime storage migration already requires attention".to_string());
    }
    if let Some(attention) = &snapshot.config_attention {
        return Err(format!(
            "managed runtime storage config {} needs attention before storage can move: {}",
            attention.config_file.display(),
            attention.message
        ));
    }
    Ok(())
}

/// Durable work this start owes before the resolved paths are final. A
/// committed migration's source cleanup is deliberately not a startup gate.
fn startup_migration_pending(journal: &RuntimeStorageJournal) -> bool {
    journal.phase.may_migrate_at_startup()
}

/// Resolve the configured managed runtime layout. A clean install commits the
/// unified default; an installation whose data still sits in the pre-unification
/// paths stays `Legacy` until its migration commits. A config that needs
/// attention resolves the same way for this session but commits nothing, so
/// the user's configured root is never replaced by the default.
fn resolve_snapshot(
    bootstrap: &BootstrapPaths,
    journal: Option<RuntimeStorageJournal>,
) -> Result<ManagedRuntimeStorageSnapshot, String> {
    let (revision, paths, config_attention) = match load_config(bootstrap)? {
        ConfigRead::Valid(config) => (
            config.revision,
            ManagedRuntimePaths::under(config.managed_runtime_root)?,
            None,
        ),
        ConfigRead::NeedsAttention(attention) => {
            let (_, paths) = uncommitted_paths(bootstrap)?;
            (0, paths, Some(attention))
        }
        ConfigRead::Missing => match uncommitted_paths(bootstrap)? {
            (true, paths) => (0, paths, None),
            (false, _) => {
                let root = bootstrap.default_managed_runtime_root();
                let config = commit_config(bootstrap, 0, &root)?;
                (config.revision, ManagedRuntimePaths::under(root)?, None)
            }
        },
    };
    Ok(ManagedRuntimeStorageSnapshot {
        config_revision: revision,
        current_layout: paths.layout(),
        current_root: paths.root().to_path_buf(),
        default_root: bootstrap.default_managed_runtime_root(),
        paths,
        pending: journal,
        config_attention,
    })
}

/// The paths in force without a usable config, and whether they are the
/// legacy layout's.
fn uncommitted_paths(bootstrap: &BootstrapPaths) -> Result<(bool, ManagedRuntimePaths), String> {
    if legacy_has_data(&bootstrap.app_data_dir)? {
        Ok((
            true,
            ManagedRuntimePaths::legacy_under(bootstrap.app_data_dir.clone())?,
        ))
    } else {
        Ok((
            false,
            ManagedRuntimePaths::under(bootstrap.default_managed_runtime_root())?,
        ))
    }
}

fn recover_migration(
    bootstrap: &BootstrapPaths,
    journal: &mut RuntimeStorageJournal,
) -> Result<(), String> {
    execute_transfer(bootstrap, journal)?;
    let commit_result = (|| {
        journal.phase = MigrationPhase::CommittingConfig;
        write_journal(bootstrap, journal)?;
        let current = load_config(bootstrap)?.valid();
        if !current
            .as_ref()
            .is_some_and(|config| config_matches_destination(config, journal))
        {
            commit_config(
                bootstrap,
                journal.expected_config_revision,
                &journal.destination_root,
            )?;
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = commit_result {
        let committed = load_config(bootstrap)
            .ok()
            .and_then(ConfigRead::valid)
            .as_ref()
            .is_some_and(|config| config_matches_destination(config, journal));
        if !committed {
            rollback_published_destination(journal)?;
            return Err(error);
        }
    }
    journal.phase = MigrationPhase::CleanupPending;
    journal.last_error = None;
    write_journal(bootstrap, journal)
}

fn config_matches_destination(
    config: &crate::internal::hanafubuki_runtime_storage::model::RuntimeStorageConfig,
    journal: &RuntimeStorageJournal,
) -> bool {
    config.revision == journal.expected_config_revision.saturating_add(1)
        && PathKey::of(&config.managed_runtime_root) == PathKey::of(&journal.destination_root)
}

/// The directory picker may return App Data itself when the user chooses the
/// application's default storage location. App Data is a bootstrap container,
/// not a managed-runtime root, so resolve only that exact selection to its
/// dedicated child. All other inputs retain exact destination semantics.
fn resolve_destination_root(
    bootstrap: &BootstrapPaths,
    destination: PathBuf,
) -> Result<PathBuf, String> {
    let destination = normalize_path(destination)?;
    if PathKey::of(&destination) == PathKey::of(&bootstrap.app_data_dir) {
        normalize_path(bootstrap.default_managed_runtime_root())
    } else {
        Ok(destination)
    }
}

fn validate_candidate(
    destination: &Path,
    current: &ManagedRuntimePaths,
    protected_paths: &[PathBuf],
    bootstrap: &BootstrapPaths,
) -> Result<(), String> {
    validate_not_dangerous(destination)?;
    if destination.exists() {
        return Err(format!(
            "managed runtime destination must not exist: {}",
            destination.display()
        ));
    }
    let mut protected = protected_paths.to_vec();
    if current.layout() == crate::internal::hanafubuki_config::ManagedRuntimeLayoutKind::Unified {
        protected.push(current.root().to_path_buf());
    }
    protected.extend([
        bootstrap.runtime_storage_config_file.clone(),
        bootstrap.runtime_storage_journal_file.clone(),
        bootstrap.runtime_storage_lock_file.clone(),
        current.python_dir().to_path_buf(),
        current.micromamba_runtime_dir().to_path_buf(),
        current.micromamba_root_prefix().to_path_buf(),
        current.launcher_code_dir().to_path_buf(),
    ]);
    let destination_key = PathKey::of(destination);
    for path in protected {
        let path = normalize_path(path)?;
        let path_key = PathKey::of(&path);
        if destination_key.starts_with(&path_key) || path_key.starts_with(&destination_key) {
            return Err(format!(
                "managed runtime destination overlaps protected path {}",
                path.display()
            ));
        }
    }
    if let Some(parent) = destination.parent() {
        let mut cursor = Some(parent);
        while let Some(path) = cursor {
            if path.exists() {
                let metadata = std::fs::symlink_metadata(path)
                    .map_err(|error| format!("inspect {}: {error}", path.display()))?;
                if metadata.file_type().is_symlink() {
                    return Err(format!(
                        "managed runtime destination has a symlink ancestor {}",
                        path.display()
                    ));
                }
            }
            cursor = path.parent();
        }
    }
    Ok(())
}

fn component_targets(destination: &ManagedRuntimePaths) -> Vec<MigrationComponentTarget> {
    [
        ("python", destination.python_dir()),
        ("micromamba", destination.micromamba_runtime_dir()),
        ("conda", destination.micromamba_root_prefix()),
        ("launcher-code", destination.launcher_code_dir()),
    ]
    .into_iter()
    .map(|(name, path)| MigrationComponentTarget {
        name: name.to_string(),
        destination: path.to_path_buf(),
    })
    .collect()
}

fn operation_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("create operation id: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn legacy_has_data(app_data_dir: &Path) -> Result<bool, String> {
    let legacy = ManagedRuntimePaths::legacy_under(app_data_dir.to_path_buf())?;
    for path in [
        legacy.python_dir(),
        legacy.micromamba_runtime_dir(),
        legacy.micromamba_root_prefix(),
        legacy.launcher_code_dir(),
    ] {
        if path.exists() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn bounded_error(error: impl Into<String>) -> String {
    const MAX: usize = 4096;
    let mut error = error.into();
    if error.len() > MAX {
        error.truncate(MAX);
    }
    error
}
