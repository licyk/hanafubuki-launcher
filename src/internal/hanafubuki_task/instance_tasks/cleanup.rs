use crate::internal::hanafubuki_process::executor::ExecutorManager;
use crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt;

use crate::internal::hanafubuki_task::instance_manager::{
    InstanceIdentity, InstanceManagerState, ensure_reserved_instance_process_inner,
    find_reserved_instance_process_by_receipt,
    unregister_reserved_instance_process_by_receipt_inner,
};

use super::model::InstanceTaskState;
use super::receipt::adapter;

pub fn dispose_reserved_instance_task_inner(
    executor: &ExecutorManager,
    instances: &InstanceManagerState,
    tasks: &InstanceTaskState,
    receipt: &ReservedProcessReceipt,
) -> Result<(), String> {
    let indexed_process = find_reserved_instance_process_by_receipt(
        instances,
        receipt.executor_id(),
        receipt.reservation_id(),
    );
    let task_executor = adapter(executor, tasks);
    task_executor.confirm_termination(receipt)?;
    unregister_reserved_instance_process_by_receipt_inner(
        instances,
        receipt.executor_id(),
        receipt.reservation_id(),
    )?;
    if let Err(dispose_error) = task_executor.dispose_matching(receipt) {
        if let Some(process) = indexed_process {
            let identity = InstanceIdentity::from_validated(process.instance_id)?;
            if let Err(restore_error) = ensure_reserved_instance_process_inner(
                instances,
                &identity,
                process.process_id,
                process.kind,
                process.label,
                receipt.reservation_id(),
            ) {
                return Err(format!(
                    "{dispose_error}; failed to restore retained instance process index: {restore_error}"
                ));
            }
        }
        return Err(dispose_error);
    }
    Ok(())
}
