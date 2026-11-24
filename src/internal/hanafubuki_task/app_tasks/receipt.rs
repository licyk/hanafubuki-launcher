use crate::internal::hanafubuki_process::executor::manager::write_reserved_executor_stdin;
use crate::internal::hanafubuki_process::executor::{
    ExecutorInfo, ExecutorManager, ExecutorOutput,
};
use crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt;

use crate::internal::hanafubuki_task::owned_task::OwnedTaskExecutorAdapter;

use super::model::AppTaskState;

fn adapter<'a>(
    executor: &'a ExecutorManager,
    tasks: &'a AppTaskState,
) -> OwnedTaskExecutorAdapter<'a, super::model::AppTaskScope, super::model::AppTask> {
    OwnedTaskExecutorAdapter::new(executor, tasks.registry.as_ref(), "app task")
}

pub fn reserved_app_task_exists(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    receipt: &ReservedProcessReceipt,
) -> bool {
    adapter(executor, tasks).exists(receipt)
}

pub fn get_reserved_app_task_executor_info(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    receipt: &ReservedProcessReceipt,
) -> Result<ExecutorInfo, String> {
    adapter(executor, tasks).info(receipt)
}

pub fn read_reserved_app_task_output_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    receipt: &ReservedProcessReceipt,
    from_offset: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<ExecutorOutput, String> {
    adapter(executor, tasks).read_output(receipt, from_offset, max_bytes)
}

pub fn resize_reserved_app_task_pty_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    receipt: &ReservedProcessReceipt,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    adapter(executor, tasks).resize(receipt, cols, rows)
}

pub fn terminate_reserved_app_task_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    receipt: &ReservedProcessReceipt,
) -> Result<(), String> {
    adapter(executor, tasks).terminate(receipt)
}

pub fn write_reserved_app_task_stdin_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    receipt: &ReservedProcessReceipt,
    bytes: Vec<u8>,
    close_stdin: bool,
) -> Result<(), String> {
    adapter(executor, tasks).info(receipt)?;
    write_reserved_executor_stdin(executor, receipt.process(), bytes, close_stdin)
}

pub fn dispose_reserved_app_task_inner(
    executor: &ExecutorManager,
    tasks: &AppTaskState,
    receipt: &ReservedProcessReceipt,
) -> Result<(), String> {
    adapter(executor, tasks).dispose_matching(receipt)
}
