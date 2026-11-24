use std::sync::atomic::Ordering;

use crate::internal::hanafubuki_process::executor::ExecutorManager;

use super::identity::resolve_owned_task;
use super::model::InstanceTaskState;
use super::receipt::terminate_reserved_instance_task_inner;

pub fn instance_task_registry_counts(tasks: &InstanceTaskState) -> (usize, usize) {
    tasks.registry.lock().counts()
}

pub fn inject_reserved_instance_metadata_failure(tasks: &InstanceTaskState) {
    tasks.fail_reserved_metadata.store(true, Ordering::SeqCst);
}

/// Stop a test's sleeper task by name. Production owners always hold the
/// task's receipt; tests that only need a process gone use this instead.
pub fn terminate_instance_task_for_test(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    instance_id: &str,
    task_id: &str,
) -> Result<(), String> {
    let task = resolve_owned_task(tasks, instance_id, task_id)?;
    let receipt = tasks
        .registry
        .lock()
        .process(&task.executor_id)
        .ok_or_else(|| format!("instance task {instance_id}/{task_id} has no receipt"))?;
    terminate_reserved_instance_task_inner(executor, tasks, &receipt)
}
