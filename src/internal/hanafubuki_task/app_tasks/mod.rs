//! Application-owned Task façade built on receipt-gated Executors.

mod identity;
mod launch;
mod model;
mod queries;
mod receipt;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(any(test, feature = "test-support"))]
pub use identity::finalize_app_task_reservation;
pub use identity::{
    executor_id_for, finalize_app_task_reservation_handle, reserve_app_task_identity,
};
#[cfg(any(test, feature = "test-support"))]
pub use launch::start_backend_app_task_inner;
pub use launch::{start_backend_app_task_with_receipt_inner, start_reserved_app_task_inner};
#[cfg(any(test, feature = "test-support"))]
pub use model::TaskCommitGate;
pub use model::{AppTask, AppTaskInfo, AppTaskReservation, AppTaskState, StartAppTaskRequest};
pub use queries::{get_app_task_inner, list_app_tasks_inner, read_app_task_output_inner};
pub use receipt::{
    dispose_reserved_app_task_inner, get_reserved_app_task_executor_info,
    read_reserved_app_task_output_inner, reserved_app_task_exists,
    resize_reserved_app_task_pty_inner, terminate_reserved_app_task_inner,
    write_reserved_app_task_stdin_inner,
};

#[cfg(test)]
mod tests;
