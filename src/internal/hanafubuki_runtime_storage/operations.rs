//! What the `runtime-storage` module runs over the storage authority: the
//! moves a user plans, schedules, cancels and retries, and the background
//! cleanup of a committed move's sources. The authority itself (its process
//! lock, the paths in force, this start's migration) belongs to the
//! application's core; each mount of the module builds one of these over it,
//! with workers of its own, so a module that stopped can start again.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::internal::hanafubuki_foundation::background_worker::{
    BackgroundWorkerGroup, WorkerShutdownDiagnostic,
};
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::module_faults::{FaultOwner, module_fault_handler};

use crate::internal::hanafubuki_runtime_storage::model::{
    ManagedRuntimeStorageSnapshot, MigrationPhase, RuntimeStorageMigrationPlan,
};
use crate::internal::hanafubuki_runtime_storage::service::ManagedRuntimeStorageService;

/// One mount of the `runtime-storage` module. Its workers are its own; a
/// cleanup still running from a previous mount keeps the next one from
/// starting another (the storage's flag).
pub struct ManagedRuntimeStorageOperations {
    storage: ManagedRuntimeStorageService,
    workers: BackgroundWorkerGroup,
}

impl ManagedRuntimeStorageOperations {
    pub fn new(storage: ManagedRuntimeStorageService) -> Self {
        Self {
            storage,
            workers: BackgroundWorkerGroup::contained_with_panic_handler(module_fault_handler(
                FaultOwner::module("runtime-storage"),
            )),
        }
    }

    pub fn snapshot(&self) -> ManagedRuntimeStorageSnapshot {
        self.storage.snapshot()
    }

    pub fn plan(
        &self,
        destination_root: PathBuf,
        protected_paths: &[PathBuf],
    ) -> Result<RuntimeStorageMigrationPlan, String> {
        self.storage.plan(destination_root, protected_paths)
    }

    pub fn schedule(
        &self,
        destination_root: PathBuf,
        protected_paths: &[PathBuf],
    ) -> Result<ManagedRuntimeStorageSnapshot, String> {
        self.storage.schedule(destination_root, protected_paths)
    }

    pub fn cancel_scheduled(&self) -> Result<ManagedRuntimeStorageSnapshot, String> {
        self.storage.cancel_scheduled()
    }

    pub fn retry_recovery_on_next_restart(&self) -> Result<ManagedRuntimeStorageSnapshot, String> {
        self.storage.retry_recovery_on_next_restart()
    }

    /// Starts removing a committed migration's sources when that is pending
    /// and not running already; whether it started.
    pub fn start_cleanup_worker(&self) -> Result<bool, String> {
        if self
            .snapshot()
            .pending
            .as_ref()
            .is_none_or(|journal| journal.phase != MigrationPhase::CleanupPending)
        {
            return Ok(false);
        }
        if self.storage.cleanup_running().swap(true, Ordering::AcqRel) {
            return Ok(false);
        }
        let storage = self.storage.clone();
        if let Err(error) = self
            .workers
            .spawn("managed-runtime-storage-cleanup", move |cancellation| {
                run_cleanup(&storage, cancellation)
            })
        {
            self.storage
                .cleanup_running()
                .store(false, Ordering::Release);
            return Err(error);
        }
        Ok(true)
    }

    pub fn shutdown_workers(&self, deadline: Instant) -> Vec<WorkerShutdownDiagnostic> {
        self.workers.shutdown(deadline)
    }
}

fn run_cleanup(storage: &ManagedRuntimeStorageService, cancellation: CancellationToken) {
    let _running = CleanupRunning(storage.cleanup_running());
    if let Err(error) = storage.complete_cleanup(&cancellation) {
        log::warn!("managed runtime source cleanup remains pending: {error}");
    }
}

/// Clears the cleanup worker's running flag however the worker ends, a panic
/// included, so a later start can run the cleanup that is still pending.
struct CleanupRunning<'a>(&'a AtomicBool);

impl Drop for CleanupRunning<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests;
