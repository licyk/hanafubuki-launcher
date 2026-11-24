use crate::internal::hanafubuki_process::executor::ExecutorOutput;
use crate::internal::hanafubuki_process::executor::manager::{
    ExecutorManager, get_executor_info, read_executor_output,
};

use crate::internal::hanafubuki_task::instance_manager::InstanceIdentity;

use super::identity::resolve_owned_task;
use super::model::{InstanceTask, InstanceTaskInfo, InstanceTaskState};

pub(super) fn to_info(task: &InstanceTask, executor: &ExecutorManager) -> InstanceTaskInfo {
    let mut info = InstanceTaskInfo {
        task_id: task.task_id.clone(),
        instance_id: task.instance_id.clone(),
        executor_id: task.executor_id.clone(),
        label: task.label.clone(),
        created_at: task.created_at,
        updated_at: task.updated_at,
        status: None,
        exit_code: None,
        error: None,
        io_mode: None,
        command: None,
        output_offset: None,
    };
    if let Ok(exec) = get_executor_info(executor, &task.executor_id) {
        info.status = Some(exec.status);
        info.exit_code = exec.exit_code;
        info.error = exec.error;
        info.io_mode = Some(exec.io_mode);
        info.command = Some(exec.command);
        info.output_offset = Some(exec.output_offset);
    }
    info
}

pub fn list_all_instance_tasks_inner(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
) -> Vec<InstanceTaskInfo> {
    task_infos(snapshot_all_tasks(tasks), executor)
}

pub(super) fn snapshot_all_tasks(tasks: &InstanceTaskState) -> Vec<InstanceTask> {
    {
        let registry = tasks.registry.lock();
        registry
            .values()
            .map(|record| record.metadata.clone())
            .collect()
    }
}

pub fn list_instance_tasks_inner(
    identity: &InstanceIdentity,
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
) -> Result<Vec<InstanceTaskInfo>, String> {
    let instance_id = identity.as_str();
    Ok(task_infos(
        snapshot_instance_tasks(tasks, instance_id),
        executor,
    ))
}

pub(super) fn snapshot_instance_tasks(
    tasks: &InstanceTaskState,
    instance_id: &str,
) -> Vec<InstanceTask> {
    {
        let registry = tasks.registry.lock();
        registry
            .values()
            .filter(|record| record.scope.instance_id() == instance_id)
            .map(|record| record.metadata.clone())
            .filter(|task| task.instance_id == instance_id)
            .collect()
    }
}

fn task_infos(snapshot: Vec<InstanceTask>, executor: &ExecutorManager) -> Vec<InstanceTaskInfo> {
    let mut out: Vec<InstanceTaskInfo> = snapshot
        .iter()
        .map(|task| to_info(task, executor))
        .collect();
    out.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then(a.task_id.cmp(&b.task_id))
    });
    out
}

pub fn get_instance_task_inner(
    identity: &InstanceIdentity,
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    task_id: String,
) -> Result<InstanceTaskInfo, String> {
    get_owned_instance_task_inner(executor, tasks, identity.as_str(), &task_id)
}

pub fn get_owned_instance_task_inner(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    instance_id: &str,
    task_id: &str,
) -> Result<InstanceTaskInfo, String> {
    let task = resolve_owned_task(tasks, instance_id, task_id)?;
    Ok(to_info(&task, executor))
}

pub fn read_owned_instance_task_output_inner(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    instance_id: &str,
    task_id: &str,
    from_offset: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<ExecutorOutput, String> {
    let task = resolve_owned_task(tasks, instance_id, task_id)?;
    read_executor_output(executor, &task.executor_id, from_offset, max_bytes)
}

pub fn read_owned_instance_task_output_tail_inner(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    instance_id: &str,
    task_id: &str,
    max_bytes: usize,
) -> Result<ExecutorOutput, String> {
    if max_bytes == 0 {
        return Err("instance task output tail limit must be positive".to_string());
    }
    let task = resolve_owned_task(tasks, instance_id, task_id)?;
    let output_offset = get_executor_info(executor, &task.executor_id)?.output_offset;
    let from_offset = output_offset.saturating_sub(max_bytes as u64);
    let mut output = read_executor_output(
        executor,
        &task.executor_id,
        Some(from_offset),
        Some(max_bytes),
    )?;
    output.truncated |= from_offset > 0;
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
pub fn read_instance_task_output_inner(
    identity: &InstanceIdentity,
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    task_id: String,
    from_offset: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<ExecutorOutput, String> {
    let task = resolve_owned_task(tasks, identity.as_str(), &task_id)?;
    read_executor_output(executor, &task.executor_id, from_offset, max_bytes)
}
