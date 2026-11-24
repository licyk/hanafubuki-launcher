use std::sync::atomic::Ordering;

use crate::internal::hanafubuki_process::executor::manager::{
    ExecutorManager, release_executor_reservation, reserve_executor_id,
};
use crate::internal::hanafubuki_process::ownership::{
    ProcessOwnerId, ReservedProcessLaunchError, ReservedProcessReceipt,
};

use crate::internal::hanafubuki_task::instance_manager::{
    InstanceIdentity, InstanceManagerState, InstanceProcessKind,
    ensure_reserved_instance_process_inner,
};

use super::reserved::start_reserved_owned_instance_task;
use crate::internal::hanafubuki_task::instance_tasks::identity::{
    executor_id_for, finalize_instance_task_reservation_handle, now_millis,
    require_request_identity, reserve_instance_task_identity,
};
use crate::internal::hanafubuki_task::instance_tasks::model::{
    InstanceTask, InstanceTaskInfo, InstanceTaskScope, InstanceTaskState, StartInstanceTaskRequest,
};
use crate::internal::hanafubuki_task::instance_tasks::{
    dispose_reserved_instance_task_inner, terminate_reserved_instance_task_inner,
};

/// Start an Instance Task and hand its exact process receipt to the owning
/// backend domain, which mutates the task only with it. The receipt never
/// crosses IPC.
pub fn start_backend_instance_task_with_receipt_inner(
    identity: &InstanceIdentity,
    executor: &ExecutorManager,
    instances: &InstanceManagerState,
    tasks: &InstanceTaskState,
    request: StartInstanceTaskRequest,
) -> Result<(InstanceTaskInfo, ReservedProcessReceipt), ReservedProcessLaunchError> {
    require_request_identity(identity, &request.instance_id)
        .map_err(ReservedProcessLaunchError::before_spawn)?;
    start_owned_instance_task(executor, instances, tasks, identity, request)
}

fn start_owned_instance_task(
    executor: &ExecutorManager,
    instances: &InstanceManagerState,
    tasks: &InstanceTaskState,
    identity: &InstanceIdentity,
    mut request: StartInstanceTaskRequest,
) -> Result<(InstanceTaskInfo, ReservedProcessReceipt), ReservedProcessLaunchError> {
    if request.command.is_empty() || request.command[0].trim().is_empty() {
        return Err(ReservedProcessLaunchError::before_spawn(
            "task command must not be empty",
        ));
    }
    let instance_id = identity.as_str().to_string();
    let task_id = match request.task_id.as_deref().map(str::trim) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => format!("task-{}", tasks.next_id.fetch_add(1, Ordering::Relaxed)),
    };
    request.task_id = Some(task_id.clone());
    let executor_id = executor_id_for(&instance_id, &task_id);
    let now = now_millis();
    let recovery_task = InstanceTask {
        task_id: task_id.clone(),
        instance_id: instance_id.clone(),
        executor_id: executor_id.clone(),
        label: request.label.clone(),
        created_at: now,
        updated_at: now,
    };
    let owner = ProcessOwnerId::generate();
    let task_reservation = reserve_instance_task_identity(tasks, &instance_id, &task_id, &owner)
        .map_err(ReservedProcessLaunchError::before_spawn)?;
    let executor_reservation = match reserve_executor_id(executor, &executor_id, &owner) {
        Ok(reservation) => reservation,
        Err(error) => {
            finalize_instance_task_reservation_handle(tasks, &task_reservation);
            return Err(ReservedProcessLaunchError::before_spawn(error));
        }
    };
    let result = start_reserved_owned_instance_task(
        identity,
        executor,
        instances,
        tasks,
        request,
        &task_reservation,
        &executor_reservation,
        false,
    );
    match result {
        Ok(started) => Ok(started),
        Err(error) => {
            let message = generic_launch_error_message(&error.message);
            if let Some(receipt) = error.created.as_ref() {
                let cleanup = terminate_reserved_instance_task_inner(executor, tasks, receipt)
                    .and_then(|_| {
                        dispose_reserved_instance_task_inner(executor, instances, tasks, receipt)
                    });
                if let Err(cleanup_error) = cleanup {
                    let index_restore = ensure_reserved_instance_process_inner(
                        instances,
                        identity,
                        executor_id.clone(),
                        InstanceProcessKind::Executor,
                        recovery_task.label.clone(),
                        receipt.reservation_id(),
                    );
                    if let Err(commit_error) = tasks.registry.lock().commit_reserved(
                        &task_reservation,
                        executor_id,
                        InstanceTaskScope::new(instance_id),
                        recovery_task,
                        receipt.clone(),
                    ) {
                        log::error!(
                            "generic Instance Task ownership retention commit failed task={task_id}: {commit_error}"
                        );
                        return Err(ReservedProcessLaunchError::after_spawn(
                            format!(
                                "{}; process cleanup failed and ownership metadata could not be committed: {cleanup_error}; commit: {commit_error}",
                                message
                            ),
                            receipt.clone(),
                        ));
                    }
                    if let Err(index_error) = index_restore {
                        return Err(ReservedProcessLaunchError::after_spawn(
                            format!(
                                "{}; process cleanup failed and task ownership was retained: {cleanup_error}; instance process index retention failed: {index_error}",
                                message
                            ),
                            receipt.clone(),
                        ));
                    }
                    return Err(ReservedProcessLaunchError::after_spawn(
                        format!(
                            "{}; process cleanup failed and task ownership was retained: {cleanup_error}",
                            message
                        ),
                        receipt.clone(),
                    ));
                }
            }
            finalize_instance_task_reservation_handle(tasks, &task_reservation);
            release_executor_reservation(executor, &executor_reservation);
            Err(ReservedProcessLaunchError::before_spawn(message))
        }
    }
}

fn generic_launch_error_message(message: &str) -> String {
    message
        .strip_prefix("reserved instance process registration failed: ")
        .map(|error| {
            format!(
                "started task executor but failed to register it with instance manager: {error}"
            )
        })
        .unwrap_or_else(|| message.to_string())
}
