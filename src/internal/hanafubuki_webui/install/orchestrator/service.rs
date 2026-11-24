use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::thread;
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

use parking_lot::Mutex;

use super::super::WebUiInstallSpec;
use super::super::validation::validate_spec;
use super::backend::InstanceInstallBackend;
use super::cleanup::{cleanup_id, schedule_pending_cleanup, spawn_cleanup_worker};
use super::model::{
    InstanceInstallCleanupStatus, InstanceInstallLogOutput, InstanceInstallOperation,
    InstanceInstallPendingCleanup, InstanceInstallPendingCleanupStatus, InstanceInstallTaskRef,
};
use super::registry::{
    InstanceInstallService, OwnershipLifecycle, PendingCleanupPurpose, Registry, ServiceInner,
};
use super::{
    CANCELLATION_CONFIRM_TIMEOUT, DEFAULT_TERMINAL_OPERATION_RETENTION, TASK_POLL_INTERVAL,
    now_millis,
};

impl InstanceInstallService {
    pub fn new(backend: Arc<dyn InstanceInstallBackend>) -> Self {
        Self::with_terminal_retention(backend, DEFAULT_TERMINAL_OPERATION_RETENTION)
    }

    pub fn with_terminal_retention(
        backend: Arc<dyn InstanceInstallBackend>,
        terminal_retention: usize,
    ) -> Self {
        assert!(
            terminal_retention > 0,
            "terminal retention must be positive"
        );
        Self {
            inner: Arc::new(ServiceInner {
                backend,
                registry: Mutex::new(Registry::new(terminal_retention)),
                next_id: AtomicU64::new(1),
                cancellation_confirm_timeout: CANCELLATION_CONFIRM_TIMEOUT,
            }),
        }
    }

    #[cfg(test)]
    pub(super) fn with_test_policy(
        backend: Arc<dyn InstanceInstallBackend>,
        terminal_retention: usize,
        cancellation_confirm_timeout: Duration,
    ) -> Self {
        let service = Self::with_terminal_retention(backend, terminal_retention);
        Self {
            inner: Arc::new(ServiceInner {
                backend: service.inner.backend.clone(),
                registry: Mutex::new(Registry::new(terminal_retention)),
                next_id: AtomicU64::new(1),
                cancellation_confirm_timeout,
            }),
        }
    }
}

impl InstanceInstallService {
    /// Check a spec against the current catalog without reserving anything.
    /// Starting an install checks again against the catalog it prepares.
    pub fn validate_spec(&self, spec: &WebUiInstallSpec) -> Result<(), String> {
        let catalog = self.inner.backend.install_catalog()?;
        validate_spec(spec, &catalog).map_err(|error| error.to_string())
    }

    pub fn get(&self, operation_id: &str) -> Result<InstanceInstallOperation, String> {
        self.inner
            .registry
            .lock()
            .operations
            .get(operation_id)
            .map(|entry| entry.snapshot.clone())
            .ok_or_else(|| format!("install operation not found or evicted: {operation_id}"))
    }

    pub fn list(&self) -> Vec<InstanceInstallOperation> {
        let mut operations: Vec<_> = self
            .inner
            .registry
            .lock()
            .operations
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect();
        operations.sort_by_key(|operation| operation.created_at);
        operations
    }

    pub fn cancel(&self, operation_id: &str) -> Result<InstanceInstallOperation, String> {
        request_cancellation(&mut self.inner.registry.lock(), operation_id)
            .ok_or_else(|| format!("install operation not found or evicted: {operation_id}"))
    }

    /// Cancels every run that has not ended, as [`Self::cancel`] does, and
    /// waits up to `deadline` for each to reach a terminal phase. From then
    /// on no install can start. Returns
    /// those runs as they then stand, so one that is not terminal did not
    /// end in time.
    pub fn shutdown(&self, deadline: Instant) -> Vec<InstanceInstallOperation> {
        let cancelled: Vec<String> = {
            let mut registry = self.inner.registry.lock();
            registry.shutting_down = true;
            let active: Vec<String> = registry
                .operations
                .iter()
                .filter(|(_, entry)| !entry.snapshot.phase.is_terminal())
                .map(|(operation_id, _)| operation_id.clone())
                .collect();
            for operation_id in &active {
                request_cancellation(&mut registry, operation_id);
            }
            active
        };
        loop {
            let operations: Vec<_> = {
                let registry = self.inner.registry.lock();
                cancelled
                    .iter()
                    .filter_map(|operation_id| registry.operations.get(operation_id))
                    .map(|entry| entry.snapshot.clone())
                    .collect()
            };
            if operations
                .iter()
                .all(|operation| operation.phase.is_terminal())
                || Instant::now() >= deadline
            {
                return operations;
            }
            thread::sleep(TASK_POLL_INTERVAL);
        }
    }

    pub fn list_pending_cleanups(&self) -> Vec<InstanceInstallPendingCleanup> {
        let mut cleanups: Vec<_> = self
            .inner
            .registry
            .lock()
            .pending_cleanups
            .values()
            .map(|entry| entry.info.clone())
            .collect();
        cleanups.sort_by(|left, right| left.cleanup_id.cmp(&right.cleanup_id));
        cleanups
    }

    /// Retry an exhausted cleanup. The retry still calls backend disposal and
    /// releases ownership only after that stronger primitive succeeds.
    pub fn retry_pending_cleanup(&self, cleanup_id: &str) -> Result<(), String> {
        {
            let mut registry = self.inner.registry.lock();
            let entry = registry
                .pending_cleanups
                .get_mut(cleanup_id)
                .ok_or_else(|| format!("pending install cleanup not found: {cleanup_id}"))?;
            if entry.in_progress {
                return Err(format!("install cleanup is already running: {cleanup_id}"));
            }
            entry.info.status = InstanceInstallPendingCleanupStatus::Pending;
            entry.info.attempts = 0;
            entry.info.error = None;
        }
        spawn_cleanup_worker(self.inner.clone(), cleanup_id.to_string());
        Ok(())
    }

    pub fn retry_operation_cleanup(&self, operation_id: &str) -> Result<(), String> {
        let (task, purpose, existing_cleanup) = {
            let registry = self.inner.registry.lock();
            let operation = registry
                .operations
                .get(operation_id)
                .ok_or_else(|| format!("install operation not found or evicted: {operation_id}"))?;
            if !matches!(
                operation.snapshot.cleanup_status,
                InstanceInstallCleanupStatus::CleanupPending
                    | InstanceInstallCleanupStatus::CleanupFailed
            ) {
                return Err(format!(
                    "operation {operation_id} does not require process cleanup"
                ));
            }
            let task = operation
                .core_task
                .clone()
                .ok_or_else(|| format!("operation {operation_id} has no retained task"))?;
            let owner = registry
                .active_by_instance
                .get(&operation.snapshot.instance_id)
                .filter(|owner| {
                    owner.operation_id == operation_id && owner.generation == operation.ownership
                })
                .ok_or_else(|| format!("operation {operation_id} no longer owns its instance"))?;
            let id = cleanup_id(task.task());
            (
                task,
                PendingCleanupPurpose::Ownership {
                    instance_id: operation.snapshot.instance_id.clone(),
                    operation_id: operation_id.to_string(),
                    generation: owner.generation,
                },
                registry.pending_cleanups.contains_key(&id).then_some(id),
            )
        };
        if let Some(cleanup_id) = existing_cleanup {
            self.retry_pending_cleanup(&cleanup_id)
        } else if schedule_pending_cleanup(self.inner.clone(), task, purpose) {
            Ok(())
        } else {
            Err(format!(
                "operation {operation_id} cleanup could not be scheduled because the pending queue is full or the task is already queued"
            ))
        }
    }

    pub fn task_for_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<InstanceInstallTaskRef>, String> {
        Ok(self.get(operation_id)?.task)
    }

    pub fn read_output(
        &self,
        operation_id: &str,
        from_offset: u64,
        max_bytes: Option<usize>,
    ) -> Result<InstanceInstallLogOutput, String> {
        let task = self
            .task_for_operation(operation_id)?
            .ok_or_else(|| "core installation task has not started".to_string())?;
        self.inner
            .backend
            .read_core_task_output(&task, from_offset, max_bytes)
    }
}

/// What a Cancel does to a run that has not ended: fires its cancellation
/// and marks the run and its ownership as cancellation requested. `None` for
/// an unknown (or evicted) run; a terminal one is returned as it is.
fn request_cancellation(
    registry: &mut Registry,
    operation_id: &str,
) -> Option<InstanceInstallOperation> {
    let entry = registry.operations.get_mut(operation_id)?;
    if entry.snapshot.phase.is_terminal() {
        return Some(entry.snapshot.clone());
    }
    entry.cancellation.cancel();
    entry.snapshot.cleanup_status = InstanceInstallCleanupStatus::CancellationRequested;
    entry.snapshot.updated_at = now_millis();
    let (instance_id, ownership, snapshot) = (
        entry.snapshot.instance_id.clone(),
        entry.ownership,
        entry.snapshot.clone(),
    );
    if let Some(owner) = registry.active_by_instance.get_mut(&instance_id) {
        if owner.operation_id == operation_id && owner.generation == ownership {
            owner.lifecycle = OwnershipLifecycle::CancellationRequested;
        }
    }
    Some(snapshot)
}
