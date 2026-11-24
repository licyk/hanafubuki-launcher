use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::internal::hanafubuki_process::executor::manager::{
    ExecutorManager, dispose_reserved_executor,
};

use super::identity::resolve_task;
use super::model::{AppTaskState, TaskCommitGate};
use super::receipt::{dispose_reserved_app_task_inner, terminate_reserved_app_task_inner};

pub fn app_task_registry_counts(tasks: &AppTaskState) -> (usize, usize) {
    tasks.registry.lock().counts()
}

pub fn inject_reserved_app_metadata_failure(tasks: &AppTaskState) {
    tasks.fail_reserved_metadata.store(true, Ordering::SeqCst);
}

pub fn pause_next_generic_app_metadata_commit(tasks: &AppTaskState) -> Arc<TaskCommitGate> {
    let gate = Arc::new(TaskCommitGate::default());
    *tasks.generic_commit_gate.lock() = Some(gate.clone());
    gate
}

pub fn dispose_app_task_process_for_test(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    task_id: &str,
) -> Result<(), String> {
    let task = resolve_task(tasks, task_id)?;
    let receipt = tasks
        .registry
        .lock()
        .process(&task.executor_id)
        .ok_or_else(|| "app task has no process receipt".to_string())?;
    dispose_reserved_executor(executor, receipt.process())
}

fn app_task_receipt_for_test(
    tasks: &AppTaskState,
    task_id: &str,
) -> Result<crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt, String> {
    let task = resolve_task(tasks, task_id)?;
    tasks
        .registry
        .lock()
        .process(&task.executor_id)
        .ok_or_else(|| format!("app task {task_id} has no process receipt"))
}

/// Stop a test's task by name. Production owners always hold the receipt.
pub fn terminate_app_task_for_test(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    task_id: String,
) -> Result<(), String> {
    let receipt = app_task_receipt_for_test(tasks, &task_id)?;
    terminate_reserved_app_task_inner(executor, tasks, &receipt)
}

/// Dispose a test's task by name. Production owners always hold the receipt.
pub fn dispose_app_task_for_test(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    task_id: String,
) -> Result<(), String> {
    let receipt = app_task_receipt_for_test(tasks, &task_id)?;
    dispose_reserved_app_task_inner(executor, tasks, &receipt)
}
