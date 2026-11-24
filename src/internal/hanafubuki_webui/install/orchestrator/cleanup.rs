use std::collections::VecDeque;
use std::sync::Arc;
use std::thread;

use crate::internal::hanafubuki_foundation::background_worker::spawn_contained_thread;

use super::model::{
    InstanceInstallCleanupStatus, InstanceInstallCoreTask, InstanceInstallPendingCleanup,
    InstanceInstallPendingCleanupKind, InstanceInstallPendingCleanupStatus,
    InstanceInstallTaskCleanupResult, InstanceInstallTaskRef, InstanceInstallTaskState,
};
use super::registry::{
    OwnershipLifecycle, PendingCleanupEntry, PendingCleanupPurpose, Registry, ServiceInner,
};
use super::{
    CLEANUP_MAX_ATTEMPTS, CLEANUP_RETRY_DELAYS, INSTALL_MODULE, MAX_PENDING_EVICTION_CLEANUPS,
    MAX_PENDING_OWNERSHIP_CLEANUPS, now_millis,
};

pub(super) fn cleanup_id(task: &InstanceInstallTaskRef) -> String {
    format!(
        "{}:{}:{}:{}",
        task.instance_id.len(),
        task.instance_id,
        task.task_id.len(),
        task.task_id
    )
}

pub(super) fn schedule_pending_cleanup(
    inner: Arc<ServiceInner>,
    task: InstanceInstallCoreTask,
    purpose: PendingCleanupPurpose,
) -> bool {
    let id = cleanup_id(task.task());
    let (accepted, spawn_id) = {
        let mut registry = inner.registry.lock();
        if registry.pending_cleanups.contains_key(&id) {
            (false, None)
        } else if matches!(purpose, PendingCleanupPurpose::Ownership { .. })
            && pending_cleanup_count(&registry, InstanceInstallPendingCleanupKind::Ownership)
                >= MAX_PENDING_OWNERSHIP_CLEANUPS
        {
            if let PendingCleanupPurpose::Ownership {
                operation_id,
                generation,
                instance_id,
            } = &purpose
            {
                mark_cleanup_capacity_failure(
                    &mut registry,
                    instance_id,
                    operation_id,
                    *generation,
                );
            }
            (false, None)
        } else if matches!(purpose, PendingCleanupPurpose::Evicted) {
            let spawn_id = queue_eviction_cleanup(&mut registry, task, None);
            (true, spawn_id)
        } else {
            let operation_id = match &purpose {
                PendingCleanupPurpose::Ownership { operation_id, .. } => Some(operation_id.clone()),
                PendingCleanupPurpose::Evicted => unreachable!(),
            };
            registry.pending_cleanups.insert(
                id.clone(),
                PendingCleanupEntry {
                    info: InstanceInstallPendingCleanup {
                        cleanup_id: id.clone(),
                        task: task.task().clone(),
                        operation_id,
                        kind: InstanceInstallPendingCleanupKind::Ownership,
                        status: InstanceInstallPendingCleanupStatus::Pending,
                        attempts: 0,
                        error: None,
                        queued_tasks: 0,
                    },
                    purpose,
                    in_progress: false,
                    core_task: task,
                    queued_evictions: VecDeque::new(),
                },
            );
            (true, Some(id.clone()))
        }
    };
    if let Some(spawn_id) = spawn_id {
        spawn_cleanup_worker(inner, spawn_id);
    }
    accepted
}

fn pending_cleanup_count(registry: &Registry, kind: InstanceInstallPendingCleanupKind) -> usize {
    registry
        .pending_cleanups
        .values()
        .filter(|entry| entry.info.kind == kind)
        .count()
}

fn queue_eviction_cleanup(
    registry: &mut Registry,
    task: InstanceInstallCoreTask,
    operation_id: Option<String>,
) -> Option<String> {
    let already_queued = registry.pending_cleanups.values().any(|entry| {
        entry.core_task == task
            || entry
                .queued_evictions
                .iter()
                .any(|(queued, _)| queued == &task)
    });
    if already_queued {
        return None;
    }

    if pending_cleanup_count(registry, InstanceInstallPendingCleanupKind::Eviction)
        < MAX_PENDING_EVICTION_CLEANUPS
    {
        let id = cleanup_id(task.task());
        registry.pending_cleanups.insert(
            id.clone(),
            PendingCleanupEntry {
                info: InstanceInstallPendingCleanup {
                    cleanup_id: id.clone(),
                    task: task.task().clone(),
                    operation_id,
                    kind: InstanceInstallPendingCleanupKind::Eviction,
                    status: InstanceInstallPendingCleanupStatus::Pending,
                    attempts: 0,
                    error: None,
                    queued_tasks: 0,
                },
                purpose: PendingCleanupPurpose::Evicted,
                in_progress: false,
                core_task: task,
                queued_evictions: VecDeque::new(),
            },
        );
        return Some(id);
    }

    let target_id = registry
        .pending_cleanups
        .iter()
        .filter(|(_, entry)| entry.info.kind == InstanceInstallPendingCleanupKind::Eviction)
        .min_by_key(|(_, entry)| entry.queued_evictions.len())
        .map(|(id, _)| id.clone())
        .expect("a full eviction cleanup set has an existing record");
    let entry = registry.pending_cleanups.get_mut(&target_id).unwrap();
    entry.queued_evictions.push_back((task, operation_id));
    entry.info.queued_tasks = entry.queued_evictions.len();
    None
}

fn mark_cleanup_capacity_failure(
    registry: &mut Registry,
    instance_id: &str,
    operation_id: &str,
    generation: u64,
) {
    let error = format!(
        "pending ownership cleanup capacity exhausted at {} records",
        MAX_PENDING_OWNERSHIP_CLEANUPS
    );
    if let Some(entry) = registry.operations.get_mut(operation_id) {
        if entry.ownership == generation {
            entry.snapshot.cleanup_status = InstanceInstallCleanupStatus::CleanupFailed;
            entry.snapshot.cleanup_error = Some(error);
        }
    }
    if let Some(owner) = registry.active_by_instance.get_mut(instance_id) {
        if owner.operation_id == operation_id && owner.generation == generation {
            owner.lifecycle = OwnershipLifecycle::CleanupFailed;
        }
    }
}

pub(super) fn retain_terminal_operation(
    registry: &mut Registry,
    operation_id: String,
) -> Vec<String> {
    let mut cleanup_ids = Vec::new();
    if registry.terminal_order.contains(&operation_id) {
        return cleanup_ids;
    }
    if let Some(evicted_id) = registry.terminal_order.push(operation_id) {
        let task = registry
            .operations
            .remove(&evicted_id)
            .and_then(|entry| entry.core_task);
        if let Some(task) = task {
            if let Some(id) = queue_eviction_cleanup(registry, task, Some(evicted_id)) {
                cleanup_ids.push(id);
            }
        }
    }
    cleanup_ids
}

pub(super) fn spawn_cleanup_worker(
    inner: Arc<ServiceInner>,
    cleanup_id: String,
) -> thread::JoinHandle<()> {
    let name = format!("install-cleanup-{cleanup_id}");
    spawn_contained_thread(INSTALL_MODULE, name, move || {
        run_cleanup_worker(inner, cleanup_id)
    })
    .expect("spawn install cleanup thread")
}

fn run_cleanup_worker(inner: Arc<ServiceInner>, cleanup_id: String) {
    loop {
        let (task, purpose, attempt) = {
            let mut registry = inner.registry.lock();
            let Some(entry) = registry.pending_cleanups.get_mut(&cleanup_id) else {
                return;
            };
            if entry.in_progress
                || entry.info.status == InstanceInstallPendingCleanupStatus::Exhausted
            {
                return;
            }
            entry.in_progress = true;
            entry.info.status = InstanceInstallPendingCleanupStatus::Retrying;
            entry.info.attempts += 1;
            (
                entry.core_task.clone(),
                entry.purpose.clone(),
                entry.info.attempts,
            )
        };

        if matches!(purpose, PendingCleanupPurpose::Ownership { .. }) {
            if let Ok(state) = inner.backend.get_core_task(&task) {
                if !matches!(state, InstanceInstallTaskState::Running) {
                    complete_pending_cleanup(&inner, &cleanup_id);
                    return;
                }
            }
        }

        match inner.backend.dispose_core_task(&task) {
            Ok(InstanceInstallTaskCleanupResult::Disposed)
            | Ok(InstanceInstallTaskCleanupResult::AlreadyAbsent) => {
                complete_pending_cleanup(&inner, &cleanup_id);
                return;
            }
            Err(error) => {
                let exhausted = {
                    let mut registry = inner.registry.lock();
                    let Some(entry) = registry.pending_cleanups.get_mut(&cleanup_id) else {
                        return;
                    };
                    entry.in_progress = false;
                    entry.info.error = Some(error.clone());
                    if attempt >= CLEANUP_MAX_ATTEMPTS {
                        entry.info.status = InstanceInstallPendingCleanupStatus::Exhausted;
                        mark_ownership_cleanup_failed(&mut registry, &cleanup_id, &error);
                        true
                    } else {
                        entry.info.status = InstanceInstallPendingCleanupStatus::Pending;
                        false
                    }
                };
                if exhausted {
                    return;
                }
                thread::sleep(CLEANUP_RETRY_DELAYS[(attempt - 1) as usize]);
            }
        }
    }
}

fn mark_ownership_cleanup_failed(registry: &mut Registry, cleanup_id: &str, error: &str) {
    let purpose = registry
        .pending_cleanups
        .get(cleanup_id)
        .map(|entry| entry.purpose.clone());
    let Some(PendingCleanupPurpose::Ownership {
        instance_id,
        operation_id,
        generation,
    }) = purpose
    else {
        return;
    };
    if let Some(operation) = registry.operations.get_mut(&operation_id) {
        if operation.ownership == generation {
            operation.snapshot.cleanup_status = InstanceInstallCleanupStatus::CleanupFailed;
            operation.snapshot.cleanup_error = Some(error.to_string());
            operation.snapshot.updated_at = now_millis();
        }
    }
    if let Some(owner) = registry.active_by_instance.get_mut(&instance_id) {
        if owner.operation_id == operation_id && owner.generation == generation {
            owner.lifecycle = OwnershipLifecycle::CleanupFailed;
        }
    }
}

pub(super) fn complete_pending_cleanup(inner: &Arc<ServiceInner>, cleanup_id: &str) {
    let cleanup_ids = {
        let mut registry = inner.registry.lock();
        let Some(mut entry) = registry.pending_cleanups.remove(cleanup_id) else {
            return;
        };
        let mut cleanup_ids = Vec::new();
        if matches!(entry.purpose, PendingCleanupPurpose::Evicted) {
            if let Some((task, operation_id)) = entry.queued_evictions.pop_front() {
                entry.info.task = task.task().clone();
                entry.core_task = task;
                entry.info.operation_id = operation_id;
                entry.info.status = InstanceInstallPendingCleanupStatus::Pending;
                entry.info.attempts = 0;
                entry.info.error = None;
                entry.info.queued_tasks = entry.queued_evictions.len();
                entry.in_progress = false;
                registry
                    .pending_cleanups
                    .insert(cleanup_id.to_string(), entry);
                cleanup_ids.push(cleanup_id.to_string());
            }
        } else if let PendingCleanupPurpose::Ownership {
            instance_id,
            operation_id,
            generation,
        } = entry.purpose
        {
            let owned = registry
                .active_by_instance
                .get(&instance_id)
                .is_some_and(|owner| {
                    owner.operation_id == operation_id && owner.generation == generation
                });
            if owned {
                if let Some(operation) = registry.operations.get_mut(&operation_id) {
                    if operation.ownership == generation {
                        operation.snapshot.cleanup_status =
                            InstanceInstallCleanupStatus::TerminationConfirmed;
                        operation.snapshot.cleanup_error = None;
                        operation.snapshot.updated_at = now_millis();
                        operation.snapshot.cleanup_status = InstanceInstallCleanupStatus::Released;
                    }
                }
                if let Some(owner) = registry.active_by_instance.get_mut(&instance_id) {
                    owner.lifecycle = OwnershipLifecycle::Released;
                }
                registry.active_by_instance.remove(&instance_id);
                cleanup_ids.extend(retain_terminal_operation(&mut registry, operation_id));
            }
        }
        cleanup_ids
    };
    for id in cleanup_ids {
        spawn_cleanup_worker(inner.clone(), id);
    }
}
