use std::sync::atomic::Ordering;

use crate::internal::hanafubuki_process::executor::manager::{
    ExecutorManager, ExecutorReservation, release_executor_reservation, reserve_executor_id,
    start_reserved_executor,
};
use crate::internal::hanafubuki_process::executor::model::ExecutorStartRequest;
use crate::internal::hanafubuki_process::ownership::{
    ProcessOwnerId, ReservedProcessLaunchError, ReservedProcessReceipt,
};

use super::identity::{
    executor_id_for, finalize_app_task_reservation_handle, now_millis, reserve_app_task_identity,
};
use super::model::{
    AppTask, AppTaskInfo, AppTaskReservation, AppTaskScope, AppTaskState, StartAppTaskRequest,
};
use super::queries::to_info;
use super::receipt::{dispose_reserved_app_task_inner, terminate_reserved_app_task_inner};

/// Start an App task without keeping its receipt, for tests that only need a
/// process. Production owners use [`start_backend_app_task_with_receipt_inner`].
#[cfg(any(test, feature = "test-support"))]
pub fn start_backend_app_task_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    request: StartAppTaskRequest,
) -> Result<AppTaskInfo, String> {
    start_backend_app_task_with_receipt_inner(executor, tasks, request)
        .map(|(info, _)| info)
        .map_err(|error| error.message)
}

/// Start an App task and hand its exact process receipt to the owning backend
/// domain. The receipt never crosses IPC. When the process started but could
/// not be cleaned up after a later failure, the error carries its receipt, so
/// the owner keeps the cleanup obligation.
pub fn start_backend_app_task_with_receipt_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    mut request: StartAppTaskRequest,
) -> Result<(AppTaskInfo, ReservedProcessReceipt), ReservedProcessLaunchError> {
    if request.command.is_empty() || request.command[0].trim().is_empty() {
        return Err(ReservedProcessLaunchError::before_spawn(
            "task command must not be empty",
        ));
    }
    let task_id = match request.task_id.as_deref().map(str::trim) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => format!("app-task-{}", tasks.next_id.fetch_add(1, Ordering::Relaxed)),
    };
    request.task_id = Some(task_id.clone());
    let executor_id = executor_id_for(&task_id);
    let recovery_started_at = now_millis();
    let recovery_task = AppTask {
        task_id: task_id.clone(),
        service_id: request.service_id.clone(),
        executor_id: executor_id.clone(),
        label: request.label.clone(),
        created_at: recovery_started_at,
        updated_at: recovery_started_at,
    };
    let owner = ProcessOwnerId::generate();
    let task_reservation = reserve_app_task_identity(tasks, &task_id, &owner)
        .map_err(ReservedProcessLaunchError::before_spawn)?;
    let executor_reservation = match reserve_executor_id(executor, &executor_id, &owner) {
        Ok(reservation) => reservation,
        Err(error) => {
            finalize_app_task_reservation_handle(tasks, &task_reservation);
            return Err(ReservedProcessLaunchError::before_spawn(error));
        }
    };

    #[cfg(any(test, feature = "test-support"))]
    if let Some(gate) = tasks.generic_commit_gate.lock().clone() {
        gate.pause();
    }
    let result = start_reserved_owned_app_task(
        executor,
        tasks,
        request,
        &task_reservation,
        &executor_reservation,
    );
    match result {
        Ok(started) => Ok(started),
        Err(error) => {
            if let Some(receipt) = error.created.as_ref() {
                let cleanup = terminate_reserved_app_task_inner(executor, tasks, receipt)
                    .and_then(|_| dispose_reserved_app_task_inner(executor, tasks, receipt));
                if let Err(cleanup_error) = cleanup {
                    if let Err(commit_error) = tasks.registry.lock().commit_reserved(
                        &task_reservation,
                        executor_id,
                        AppTaskScope,
                        recovery_task,
                        receipt.clone(),
                    ) {
                        log::error!(
                            "generic App Task ownership retention commit failed task={task_id}: {commit_error}"
                        );
                        return Err(ReservedProcessLaunchError::after_spawn(
                            format!(
                                "{}; process cleanup failed and ownership metadata could not be committed: {cleanup_error}; commit: {commit_error}",
                                error.message
                            ),
                            receipt.clone(),
                        ));
                    }
                    return Err(ReservedProcessLaunchError::after_spawn(
                        format!(
                            "{}; process cleanup failed and task ownership was retained: {cleanup_error}",
                            error.message
                        ),
                        receipt.clone(),
                    ));
                }
            }
            finalize_app_task_reservation_handle(tasks, &task_reservation);
            release_executor_reservation(executor, &executor_reservation);
            Err(ReservedProcessLaunchError::before_spawn(error.message))
        }
    }
}

pub fn start_reserved_app_task_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    request: StartAppTaskRequest,
    task_reservation: &AppTaskReservation,
    executor_reservation: &ExecutorReservation,
) -> Result<(AppTaskInfo, ReservedProcessReceipt), ReservedProcessLaunchError> {
    start_reserved_owned_app_task(
        executor,
        tasks,
        request,
        task_reservation,
        executor_reservation,
    )
}

fn start_reserved_owned_app_task(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    request: StartAppTaskRequest,
    task_reservation: &AppTaskReservation,
    executor_reservation: &ExecutorReservation,
) -> Result<(AppTaskInfo, ReservedProcessReceipt), ReservedProcessLaunchError> {
    if request.command.is_empty() || request.command[0].trim().is_empty() {
        return Err(ReservedProcessLaunchError::before_spawn(
            "task command must not be empty",
        ));
    }
    let task_id = request
        .task_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ReservedProcessLaunchError::before_spawn("reserved task id is required"))?
        .to_string();
    let executor_id = executor_id_for(&task_id);
    if task_reservation.key() != task_id
        || task_reservation.reservation_id() != executor_reservation.owner_id()
        || executor_reservation.process_id() != executor_id
    {
        return Err(ReservedProcessLaunchError::before_spawn(
            "app task reservation identity mismatch",
        ));
    }
    {
        let registry = tasks.registry.lock();
        if !registry.reservation_matches(task_reservation) {
            return Err(ReservedProcessLaunchError::before_spawn(format!(
                "app task reservation is missing, consumed, or stale: {task_id}"
            )));
        }
        if registry.contains(&executor_id) {
            return Err(ReservedProcessLaunchError::before_spawn(format!(
                "app task {task_id} already exists"
            )));
        }
    }
    let exec_request = ExecutorStartRequest {
        executor_id: Some(executor_id.clone()),
        command: request.command,
        cwd: request.cwd,
        environment_inheritance:
            crate::internal::hanafubuki_process::executor::EnvironmentInheritance::Inherit,
        env: request.env,
        io_mode: request.io_mode,
        stream_stdin: request.io_mode.uses_pty(),
        stream_output: true,
        cols: request.cols,
        rows: request.rows,
        timeout_ms: request.timeout_ms,
        output_bytes_cap: request.output_bytes_cap,
    };
    let (exec_info, executor_receipt) =
        start_reserved_executor(executor, exec_request, executor_reservation)
            .map_err(ReservedProcessLaunchError::before_spawn)?;
    let receipt = ReservedProcessReceipt::new(task_id.clone(), executor_receipt);
    let now = now_millis();
    let task = AppTask {
        task_id: task_id.clone(),
        service_id: request.service_id,
        executor_id: executor_id.clone(),
        label: request.label,
        created_at: now,
        updated_at: now,
    };
    if tasks.fail_reserved_metadata.swap(false, Ordering::SeqCst) {
        return Err(ReservedProcessLaunchError::after_spawn(
            format!("injected reserved app task metadata failure: {task_id}"),
            receipt,
        ));
    }
    {
        let mut registry = tasks.registry.lock();
        if !registry.reservation_matches(task_reservation) || registry.contains(&executor_id) {
            return Err(ReservedProcessLaunchError::after_spawn(
                format!("reserved app task metadata commit failed: {task_id}"),
                receipt,
            ));
        }
        registry
            .commit_reserved(
                task_reservation,
                executor_id.clone(),
                AppTaskScope,
                task.clone(),
                receipt.clone(),
            )
            .map_err(|error| ReservedProcessLaunchError::after_spawn(error, receipt.clone()))?;
    }
    log::info!(
        "reserved app task started task_id={task_id} executor_id={executor_id} service_id={:?} io_mode={:?}",
        task.service_id,
        exec_info.io_mode
    );
    Ok((to_info(&task, executor), receipt))
}
