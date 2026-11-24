use crate::internal::hanafubuki_process::ownership::{OwnedReleaseStatus, ProcessOwnerId};

use crate::internal::hanafubuki_task::instance_manager::InstanceIdentity;

use super::model::{InstanceTask, InstanceTaskReservation, InstanceTaskState};

/// Length-prefixed App-independent Executor id for one Instance Task.
pub fn executor_id_for(instance_id: &str, task_id: &str) -> String {
    format!(
        "inst:{}:{instance_id}:task:{}:{task_id}",
        instance_id.len(),
        task_id.len()
    )
}

fn task_reservation_key(instance_id: &str, task_id: &str) -> String {
    format!(
        "{}:{instance_id}:{}:{task_id}",
        instance_id.len(),
        task_id.len()
    )
}

pub fn reserve_instance_task_identity(
    tasks: &InstanceTaskState,
    instance_id: &str,
    task_id: &str,
    reservation_id: &ProcessOwnerId,
) -> Result<InstanceTaskReservation, String> {
    let executor_id = executor_id_for(instance_id, task_id);
    let key = task_reservation_key(instance_id, task_id);
    let mut registry = tasks.registry.lock();
    if registry.contains(&executor_id) {
        return Err(format!(
            "task {task_id} already exists for instance {instance_id}"
        ));
    }
    registry
        .reserve(&key, &executor_id, reservation_id)
        .map_err(|_| format!("task {task_id} is already reserved for instance {instance_id}"))
}

#[cfg(any(test, feature = "test-support"))]
pub fn finalize_instance_task_reservation(
    tasks: &InstanceTaskState,
    instance_id: &str,
    task_id: &str,
    reservation_id: &ProcessOwnerId,
) -> OwnedReleaseStatus {
    tasks
        .registry
        .lock()
        .finalize_reservation(&task_reservation_key(instance_id, task_id), reservation_id)
}

pub fn finalize_instance_task_reservation_handle(
    tasks: &InstanceTaskState,
    reservation: &InstanceTaskReservation,
) -> OwnedReleaseStatus {
    tasks
        .registry
        .lock()
        .finalize_reservation_handle(reservation)
}

pub(super) fn require_request_identity(
    identity: &InstanceIdentity,
    requested_instance_id: &str,
) -> Result<(), String> {
    if requested_instance_id.trim() == identity.as_str() {
        Ok(())
    } else {
        Err("validated instance identity does not match task request".to_string())
    }
}

pub(super) fn reservation_key(instance_id: &str, task_id: &str) -> String {
    task_reservation_key(instance_id, task_id)
}

pub(super) fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

pub(super) fn resolve_owned_task(
    tasks: &InstanceTaskState,
    instance_id: &str,
    task_id: &str,
) -> Result<InstanceTask, String> {
    let executor_id = executor_id_for(instance_id, task_id);
    let task = tasks
        .registry
        .lock()
        .get(&executor_id)
        .map(|record| record.metadata.clone())
        .ok_or_else(|| format!("instance {instance_id} has no task {task_id}"))?;
    if task.instance_id != instance_id {
        return Err(format!(
            "instance {instance_id} does not own task {task_id}"
        ));
    }
    Ok(task)
}
