mod backend;
mod cleanup;
mod completion;
mod core_task;
mod model;
mod operation;
mod registry;
mod reservation;
mod service;
mod steps;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use backend::InstanceInstallBackend;
pub use model::{
    CoreInstallTaskRequest, InstanceInstallCleanupStatus, InstanceInstallCoreStartError,
    InstanceInstallCoreTask, InstanceInstallLogOutput, InstanceInstallOperation,
    InstanceInstallOutputChunk, InstanceInstallOutputStream, InstanceInstallPendingCleanup,
    InstanceInstallPendingCleanupKind, InstanceInstallPendingCleanupStatus, InstanceInstallPhase,
    InstanceInstallPreparation, InstanceInstallStageError, InstanceInstallTaskCleanupResult,
    InstanceInstallTaskRef, InstanceInstallTaskState, StartInstanceInstallRequest,
    StepInstallPreparation,
};
pub use registry::InstanceInstallService;

/// A panic on an install's operation, cleanup or monitor thread is a fault
/// of the WebUI install module: it ends that thread, never the application.
pub(crate) const INSTALL_MODULE: FaultOwner = FaultOwner::module("webui-install");
const TASK_POLL_INTERVAL: Duration = Duration::from_millis(25);
const CANCELLATION_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_TERMINAL_OPERATION_RETENTION: usize = 20;
pub const MAX_PENDING_OWNERSHIP_CLEANUPS: usize = 20;
pub const MAX_PENDING_EVICTION_CLEANUPS: usize = 20;
const CLEANUP_MAX_ATTEMPTS: u32 = 3;
const CLEANUP_RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(25), Duration::from_millis(50)];

fn install_activity_id(operation_id: &str) -> String {
    format!("install:{}:{operation_id}", operation_id.len())
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
use super::{ResolvedInstallSettings, WebUiInstallOptionCatalog, WebUiInstallSpec};
#[cfg(test)]
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::module_faults::FaultOwner;
#[cfg(test)]
use crate::internal::hanafubuki_python_runtime::PythonVersion;
#[cfg(test)]
use cleanup::{cleanup_id, complete_pending_cleanup, spawn_cleanup_worker};
#[cfg(test)]
use completion::finish;
#[cfg(test)]
use registry::{PendingCleanupEntry, PendingCleanupPurpose};

#[cfg(test)]
mod tests;
