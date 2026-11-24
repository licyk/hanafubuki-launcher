use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::concurrency::BoundedHistory;
use parking_lot::Mutex;

use super::backend::InstanceInstallBackend;
use super::model::{
    InstanceInstallCoreTask, InstanceInstallOperation, InstanceInstallPendingCleanup,
    InstanceInstallPreparation, InstanceInstallTaskRef,
};
use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

use super::super::steps::{StepEnvironment, StepPlan};

pub(super) struct OperationEntry {
    pub(super) snapshot: InstanceInstallOperation,
    pub(super) ownership: u64,
    pub(super) cancellation: CancellationToken,
    /// The started core task with its receipt; the snapshot carries only its
    /// reference.
    pub(super) core_task: Option<InstanceInstallCoreTask>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OwnershipLifecycle {
    Active,
    CancellationRequested,
    CleanupPending,
    CleanupFailed,
    TerminationConfirmed,
    Released,
}

#[derive(Clone, Debug)]
pub(super) struct OwnershipRecord {
    pub(super) operation_id: String,
    pub(super) generation: u64,
    pub(super) lifecycle: OwnershipLifecycle,
}

#[derive(Clone, Debug)]
pub(super) enum PendingCleanupPurpose {
    Evicted,
    Ownership {
        instance_id: String,
        operation_id: String,
        generation: u64,
    },
}

#[derive(Clone, Debug)]
pub(super) struct PendingCleanupEntry {
    pub(super) info: InstanceInstallPendingCleanup,
    pub(super) purpose: PendingCleanupPurpose,
    pub(super) in_progress: bool,
    /// The task this cleanup disposes; `info.task` is its published reference.
    pub(super) core_task: InstanceInstallCoreTask,
    pub(super) queued_evictions: VecDeque<(InstanceInstallCoreTask, Option<String>)>,
}

pub(super) struct Registry {
    pub(super) operations: HashMap<String, OperationEntry>,
    pub(super) active_by_instance: HashMap<String, OwnershipRecord>,
    pub(super) terminal_order: BoundedHistory<String>,
    pub(super) pending_cleanups: HashMap<String, PendingCleanupEntry>,
    /// Set by [`InstanceInstallService::shutdown`]; a reservation made
    /// afterwards is refused, so no run starts once the quit has cancelled
    /// the ones it found.
    pub(super) shutting_down: bool,
}

impl Registry {
    pub(super) fn new(terminal_retention: usize) -> Self {
        Self {
            operations: HashMap::new(),
            active_by_instance: HashMap::new(),
            terminal_order: BoundedHistory::new(terminal_retention),
            pending_cleanups: HashMap::new(),
            shutting_down: false,
        }
    }
}

pub(super) struct ServiceInner {
    pub(super) backend: Arc<dyn InstanceInstallBackend>,
    pub(super) registry: Mutex<Registry>,
    pub(super) next_id: AtomicU64,
    pub(super) cancellation_confirm_timeout: Duration,
}

#[derive(Clone)]
pub struct InstanceInstallService {
    pub(super) inner: Arc<ServiceInner>,
}

/// Fully prepared, instance-owning installation that has not started any
/// filesystem or process work. The lifecycle owner persists `Installing`
/// before consuming this value through `commit_start`.
pub(crate) struct InstanceInstallReservation {
    pub(super) snapshot: InstanceInstallOperation,
    pub(super) ownership: u64,
    pub(super) work: InstallWork,
    pub(super) task: InstanceInstallTaskRef,
    pub(super) cancellation: CancellationToken,
    pub(super) instance_identity: String,
    pub(super) force_recreate_python: bool,
    pub(super) force_recreate_core: bool,
}

/// What a reserved install runs: a built-in family's installer on the
/// instance's Python, or a declarative type's steps.
pub(super) enum InstallWork {
    Builtin {
        prepared: InstanceInstallPreparation,
        python_dir: PathBuf,
        python_version: PythonVersion,
        cli_args: Vec<String>,
    },
    Steps {
        plan: StepPlan,
        environment: StepEnvironment,
        components: InstanceComponentPaths,
    },
}

impl InstanceInstallReservation {
    pub(crate) fn operation(&self) -> &InstanceInstallOperation {
        &self.snapshot
    }
}
