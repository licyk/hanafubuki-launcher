use std::sync::Arc;

use super::cleanup::{retain_terminal_operation, schedule_pending_cleanup, spawn_cleanup_worker};
use super::model::{
    InstanceInstallCleanupStatus, InstanceInstallCoreTask, InstanceInstallOperation,
    InstanceInstallPhase, InstanceInstallTaskRef,
};
use super::now_millis;
use super::registry::{OwnershipLifecycle, PendingCleanupPurpose, ServiceInner};

pub(super) fn read_all_diagnostics(
    inner: &ServiceInner,
    task: &InstanceInstallTaskRef,
) -> Option<String> {
    inner
        .backend
        .read_core_task_output(task, 0, None)
        .ok()
        .map(|output| {
            let bytes: Vec<u8> = output
                .chunks
                .into_iter()
                .flat_map(|chunk| chunk.data)
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        })
}

pub(super) fn update_owned(
    inner: &ServiceInner,
    operation_id: &str,
    ownership: u64,
    update: impl FnOnce(&mut InstanceInstallOperation),
) -> bool {
    let mut registry = inner.registry.lock();
    let Some(entry) = registry.operations.get_mut(operation_id) else {
        return false;
    };
    if entry.ownership != ownership || entry.snapshot.phase.is_terminal() {
        return false;
    }
    update(&mut entry.snapshot);
    entry.snapshot.updated_at = now_millis();
    true
}

/// Record the started core task on the operation it belongs to, keeping its
/// receipt in the registry and publishing only its reference.
pub(super) fn record_core_task(
    inner: &ServiceInner,
    operation_id: &str,
    ownership: u64,
    task: &InstanceInstallCoreTask,
) -> bool {
    let mut registry = inner.registry.lock();
    let Some(entry) = registry.operations.get_mut(operation_id) else {
        return false;
    };
    if entry.ownership != ownership || entry.snapshot.phase.is_terminal() {
        return false;
    }
    entry.snapshot.task = Some(task.task().clone());
    entry.snapshot.updated_at = now_millis();
    entry.core_task = Some(task.clone());
    true
}

pub(super) fn finish(
    inner: &Arc<ServiceInner>,
    operation_id: &str,
    ownership: u64,
    phase: InstanceInstallPhase,
    exit_code: Option<i32>,
    error: Option<String>,
) {
    let cleanup_ids = {
        let mut registry = inner.registry.lock();
        let instance_id = {
            let Some(entry) = registry.operations.get_mut(operation_id) else {
                return;
            };
            if entry.ownership != ownership || entry.snapshot.phase.is_terminal() {
                return;
            }
            entry.snapshot.phase = phase;
            entry.snapshot.exit_code = exit_code;
            entry.snapshot.error = error;
            entry.snapshot.cleanup_status = InstanceInstallCleanupStatus::TerminationConfirmed;
            entry.snapshot.cleanup_error = None;
            entry.snapshot.updated_at = now_millis();
            entry.snapshot.instance_id.clone()
        };
        if let Some(owner) = registry.active_by_instance.get_mut(&instance_id) {
            if owner.operation_id == operation_id && owner.generation == ownership {
                owner.lifecycle = OwnershipLifecycle::TerminationConfirmed;
            }
        }
        let release_owned = registry
            .active_by_instance
            .get(&instance_id)
            .is_some_and(|owner| {
                owner.operation_id == operation_id && owner.generation == ownership
            });
        if release_owned {
            if let Some(entry) = registry.operations.get_mut(operation_id) {
                entry.snapshot.cleanup_status = InstanceInstallCleanupStatus::Released;
            }
            registry.active_by_instance.remove(&instance_id);
        }
        retain_terminal_operation(&mut registry, operation_id.to_string())
    };
    for cleanup_id in cleanup_ids {
        spawn_cleanup_worker(inner.clone(), cleanup_id);
    }
}

pub(super) fn fail_and_schedule_cleanup(
    inner: &Arc<ServiceInner>,
    operation_id: &str,
    ownership: u64,
    task: InstanceInstallCoreTask,
    error: String,
) {
    let instance_id = {
        let mut registry = inner.registry.lock();
        let instance_id = {
            let Some(entry) = registry.operations.get_mut(operation_id) else {
                return;
            };
            if entry.ownership != ownership {
                return;
            }
            entry.snapshot.phase = InstanceInstallPhase::Failed;
            entry.snapshot.error = Some(error.clone());
            entry.snapshot.cleanup_status = InstanceInstallCleanupStatus::CleanupPending;
            entry.snapshot.cleanup_error = Some(error.clone());
            entry.snapshot.updated_at = now_millis();
            entry.snapshot.instance_id.clone()
        };
        if let Some(owner) = registry.active_by_instance.get_mut(&instance_id) {
            if owner.operation_id == operation_id && owner.generation == ownership {
                owner.lifecycle = OwnershipLifecycle::CleanupPending;
            }
        }
        instance_id
    };
    schedule_pending_cleanup(
        inner.clone(),
        task,
        PendingCleanupPurpose::Ownership {
            instance_id,
            operation_id: operation_id.to_string(),
            generation: ownership,
        },
    );
}
