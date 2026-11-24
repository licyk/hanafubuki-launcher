use std::sync::atomic::Ordering;

use crate::internal::hanafubuki_process::executor::manager::{
    ExecutorManager, ExecutorReservation, start_reserved_executor,
};
use crate::internal::hanafubuki_process::executor::model::ExecutorStartRequest;
use crate::internal::hanafubuki_process::ownership::{
    ReservedProcessLaunchError, ReservedProcessReceipt,
};

use crate::internal::hanafubuki_task::instance_manager::{
    InstanceIdentity, InstanceManagerState, InstanceProcessKind,
    register_reserved_instance_process_inner,
};

use crate::internal::hanafubuki_task::instance_tasks::identity::{
    executor_id_for, now_millis, require_request_identity, reservation_key,
};
use crate::internal::hanafubuki_task::instance_tasks::model::{
    InstanceTask, InstanceTaskInfo, InstanceTaskReservation, InstanceTaskScope, InstanceTaskState,
    StartInstanceTaskRequest,
};
use crate::internal::hanafubuki_task::instance_tasks::queries::to_info;

#[allow(clippy::too_many_arguments)]
pub fn start_reserved_instance_task_inner(
    identity: &InstanceIdentity,
    executor: &ExecutorManager,
    instances: &InstanceManagerState,
    tasks: &InstanceTaskState,
    request: StartInstanceTaskRequest,
    task_reservation: &InstanceTaskReservation,
    executor_reservation: &ExecutorReservation,
) -> Result<(InstanceTaskInfo, ReservedProcessReceipt), ReservedProcessLaunchError> {
    start_reserved_owned_instance_task(
        identity,
        executor,
        instances,
        tasks,
        request,
        task_reservation,
        executor_reservation,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn start_reserved_owned_instance_task(
    identity: &InstanceIdentity,
    executor: &ExecutorManager,
    instances: &InstanceManagerState,
    tasks: &InstanceTaskState,
    request: StartInstanceTaskRequest,
    task_reservation: &InstanceTaskReservation,
    executor_reservation: &ExecutorReservation,
    consume_metadata_failure_injection: bool,
) -> Result<(InstanceTaskInfo, ReservedProcessReceipt), ReservedProcessLaunchError> {
    require_request_identity(identity, &request.instance_id)
        .map_err(ReservedProcessLaunchError::before_spawn)?;
    let instance_id = identity.as_str().to_string();
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
    let executor_id = executor_id_for(&instance_id, &task_id);
    if task_reservation.key() != reservation_key(&instance_id, &task_id)
        || task_reservation.reservation_id() != executor_reservation.owner_id()
        || executor_reservation.process_id() != executor_id
    {
        return Err(ReservedProcessLaunchError::before_spawn(
            "instance task reservation identity mismatch",
        ));
    }
    {
        let registry = tasks.registry.lock();
        if !registry.reservation_matches(task_reservation) {
            return Err(ReservedProcessLaunchError::before_spawn(format!(
                "instance task reservation is missing, consumed, or stale: {task_id}"
            )));
        }
        if registry.contains(&executor_id) {
            return Err(ReservedProcessLaunchError::before_spawn(format!(
                "task {task_id} already exists for instance {instance_id}"
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

    if let Err(error) = register_reserved_instance_process_inner(
        instances,
        identity,
        executor_id.clone(),
        InstanceProcessKind::Executor,
        request.label.clone(),
        receipt.reservation_id(),
    ) {
        return Err(ReservedProcessLaunchError::after_spawn(
            format!("reserved instance process registration failed: {error}"),
            receipt,
        ));
    }

    let now = now_millis();
    let task = InstanceTask {
        task_id: task_id.clone(),
        instance_id: instance_id.clone(),
        executor_id: executor_id.clone(),
        label: request.label,
        created_at: now,
        updated_at: now,
    };
    if consume_metadata_failure_injection
        && tasks.fail_reserved_metadata.swap(false, Ordering::SeqCst)
    {
        return Err(ReservedProcessLaunchError::after_spawn(
            format!("injected reserved instance task metadata failure: {task_id}"),
            receipt,
        ));
    }
    {
        let mut registry = tasks.registry.lock();
        if !registry.reservation_matches(task_reservation) || registry.contains(&executor_id) {
            return Err(ReservedProcessLaunchError::after_spawn(
                format!("reserved instance task metadata commit failed: {task_id}"),
                receipt,
            ));
        }
        registry
            .commit_reserved(
                task_reservation,
                executor_id.clone(),
                InstanceTaskScope::new(instance_id.clone()),
                task.clone(),
                receipt.clone(),
            )
            .map_err(|error| ReservedProcessLaunchError::after_spawn(error, receipt.clone()))?;
    }
    log::info!(
        "reserved instance task started instance_id={instance_id} task_id={task_id} executor_id={executor_id} io_mode={:?}",
        exec_info.io_mode
    );
    Ok((to_info(&task, executor), receipt))
}
