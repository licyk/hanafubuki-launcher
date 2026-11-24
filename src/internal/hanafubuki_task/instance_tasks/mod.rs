//! Instance-owned Task façade built on validated identity and exact receipts.

mod cleanup;
mod identity;
mod launch;
mod model;
mod queries;
mod receipt;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use cleanup::dispose_reserved_instance_task_inner;
#[cfg(any(test, feature = "test-support"))]
pub use identity::finalize_instance_task_reservation;
pub use identity::{
    executor_id_for, finalize_instance_task_reservation_handle, reserve_instance_task_identity,
};
pub use launch::{
    start_backend_instance_task_with_receipt_inner, start_reserved_instance_task_inner,
};
pub use model::{
    InstanceTask, InstanceTaskInfo, InstanceTaskReservation, InstanceTaskState,
    StartInstanceTaskRequest,
};
pub use queries::{
    get_instance_task_inner, get_owned_instance_task_inner, list_all_instance_tasks_inner,
    list_instance_tasks_inner, read_instance_task_output_inner,
    read_owned_instance_task_output_inner, read_owned_instance_task_output_tail_inner,
};
pub use receipt::{
    get_reserved_instance_task_executor_info, read_reserved_instance_task_output_inner,
    reserved_instance_task_exists, resize_reserved_instance_task_pty_inner,
    terminate_reserved_instance_task_inner,
};

#[cfg(test)]
mod tests;
