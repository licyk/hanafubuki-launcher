use crate::internal::hanafubuki_process::ownership::{OwnedReleaseStatus, ProcessOwnerId};

use super::model::{AppTask, AppTaskReservation, AppTaskState};

/// Deterministic, unambiguous executor id for an app task.
pub fn executor_id_for(task_id: &str) -> String {
    format!("app:task:{}:{task_id}", task_id.len())
}

pub fn reserve_app_task_identity(
    tasks: &AppTaskState,
    task_id: &str,
    reservation_id: &ProcessOwnerId,
) -> Result<AppTaskReservation, String> {
    let executor_id = executor_id_for(task_id);
    let mut registry = tasks.registry.lock();
    if registry.contains(&executor_id) {
        return Err(format!("app task {task_id} already exists"));
    }
    registry
        .reserve(task_id, &executor_id, reservation_id)
        .map_err(|_| format!("app task id is already reserved: {task_id}"))
}

#[cfg(any(test, feature = "test-support"))]
pub fn finalize_app_task_reservation(
    tasks: &AppTaskState,
    task_id: &str,
    reservation_id: &ProcessOwnerId,
) -> OwnedReleaseStatus {
    tasks
        .registry
        .lock()
        .finalize_reservation(task_id, reservation_id)
}

pub fn finalize_app_task_reservation_handle(
    tasks: &AppTaskState,
    reservation: &AppTaskReservation,
) -> OwnedReleaseStatus {
    tasks
        .registry
        .lock()
        .finalize_reservation_handle(reservation)
}

pub(super) fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

pub(super) fn resolve_task(tasks: &AppTaskState, task_id: &str) -> Result<AppTask, String> {
    let executor_id = executor_id_for(task_id);
    tasks
        .registry
        .lock()
        .get(&executor_id)
        .map(|record| record.metadata.clone())
        .ok_or_else(|| format!("no app task {task_id}"))
}
