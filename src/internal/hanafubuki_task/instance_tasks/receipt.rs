use crate::internal::hanafubuki_process::executor::{
    ExecutorInfo, ExecutorManager, ExecutorOutput,
};
use crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt;

use crate::internal::hanafubuki_task::owned_task::OwnedTaskExecutorAdapter;

use super::model::{InstanceTask, InstanceTaskScope, InstanceTaskState};

pub(super) fn adapter<'a>(
    executor: &'a ExecutorManager,
    tasks: &'a InstanceTaskState,
) -> OwnedTaskExecutorAdapter<'a, InstanceTaskScope, InstanceTask> {
    OwnedTaskExecutorAdapter::new(executor, tasks.registry.as_ref(), "instance task")
}

pub fn reserved_instance_task_exists(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    receipt: &ReservedProcessReceipt,
) -> bool {
    adapter(executor, tasks).exists(receipt)
}

pub fn get_reserved_instance_task_executor_info(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    receipt: &ReservedProcessReceipt,
) -> Result<ExecutorInfo, String> {
    adapter(executor, tasks).info(receipt)
}

pub fn read_reserved_instance_task_output_inner(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    receipt: &ReservedProcessReceipt,
    from_offset: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<ExecutorOutput, String> {
    adapter(executor, tasks).read_output(receipt, from_offset, max_bytes)
}

pub fn resize_reserved_instance_task_pty_inner(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    receipt: &ReservedProcessReceipt,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    adapter(executor, tasks).resize(receipt, cols, rows)
}

pub fn terminate_reserved_instance_task_inner(
    executor: &ExecutorManager,
    tasks: &InstanceTaskState,
    receipt: &ReservedProcessReceipt,
) -> Result<(), String> {
    adapter(executor, tasks).terminate(receipt)
}
