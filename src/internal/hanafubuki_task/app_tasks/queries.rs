use crate::internal::hanafubuki_process::executor::ExecutorOutput;
use crate::internal::hanafubuki_process::executor::manager::{
    ExecutorManager, get_executor_info, read_executor_output,
};

use super::identity::resolve_task;
use super::model::{AppTask, AppTaskInfo, AppTaskScope, AppTaskState};

pub(super) fn to_info(task: &AppTask, executor: &ExecutorManager) -> AppTaskInfo {
    let mut info = AppTaskInfo {
        task_id: task.task_id.clone(),
        service_id: task.service_id.clone(),
        executor_id: task.executor_id.clone(),
        label: task.label.clone(),
        created_at: task.created_at,
        updated_at: task.updated_at,
        status: None,
        exit_code: None,
        io_mode: None,
        command: None,
        output_offset: None,
    };
    if let Ok(exec) = get_executor_info(executor, &task.executor_id) {
        info.status = Some(exec.status);
        info.exit_code = exec.exit_code;
        info.io_mode = Some(exec.io_mode);
        info.command = Some(exec.command);
        info.output_offset = Some(exec.output_offset);
    }
    info
}

/// List all app-owned tasks with live executor status.
pub fn list_app_tasks_inner(executor: &ExecutorManager, tasks: &AppTaskState) -> Vec<AppTaskInfo> {
    let snapshot = snapshot_tasks(tasks);
    let mut out: Vec<AppTaskInfo> = snapshot
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

pub(super) fn snapshot_tasks(tasks: &AppTaskState) -> Vec<AppTask> {
    {
        let registry = tasks.registry.lock();
        registry
            .values()
            .filter(|record| record.scope == AppTaskScope)
            .map(|record| record.metadata.clone())
            .collect()
    }
}

pub fn get_app_task_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    task_id: String,
) -> Result<AppTaskInfo, String> {
    let task = resolve_task(tasks, &task_id)?;
    Ok(to_info(&task, executor))
}

pub fn read_app_task_output_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    task_id: String,
    from_offset: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<ExecutorOutput, String> {
    let task = resolve_task(tasks, &task_id)?;
    read_executor_output(executor, &task.executor_id, from_offset, max_bytes)
}
