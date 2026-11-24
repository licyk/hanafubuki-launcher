//! Validated instance identity, filesystem paths, and process index façade.

mod model;
mod output;
mod paths;
mod registry;

pub use model::{
    InstanceIdentity, InstanceManagerState, InstancePaths, InstanceProcessKind, InstanceProcessRef,
};
pub use output::get_instance_executor_output_inner;
pub use paths::build_instance_paths;
pub use registry::{
    assert_instance_owns_process, inject_reserved_instance_registration_failure,
    list_instance_processes_registered_inner, register_instance_process_inner,
    register_reserved_instance_process_inner, unregister_instance_process_inner,
    unregister_reserved_instance_process_by_receipt_inner,
    unregister_reserved_instance_process_inner,
};
pub(crate) use registry::{
    ensure_reserved_instance_process_inner, find_reserved_instance_process_by_receipt,
};

#[cfg(test)]
mod tests;
