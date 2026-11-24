use crate::internal::hanafubuki_process::executor::manager::read_executor_output;
use crate::internal::hanafubuki_process::executor::{ExecutorManager, ExecutorOutput};

use super::{
    InstanceIdentity, InstanceManagerState, InstanceProcessKind, assert_instance_owns_process,
};

pub fn get_instance_executor_output_inner(
    manager: &InstanceManagerState,
    executor: &ExecutorManager,
    identity: &InstanceIdentity,
    executor_id: &str,
    from_offset: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<ExecutorOutput, String> {
    assert_instance_owns_process(
        manager,
        identity,
        executor_id,
        InstanceProcessKind::Executor,
    )?;
    read_executor_output(executor, executor_id, from_offset, max_bytes)
}
