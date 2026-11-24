//! Application and instance task ownership built on `hanafubuki-process`.
//!
//! This crate owns task identity, metadata, process receipts, and the
//! instance-to-process index. It deliberately has no dependency on Tauri or
//! `hanafubuki-config`: callers must resolve an [`instance_manager::InstanceIdentity`]
//! in the application layer before invoking instance-scoped task operations.

pub mod app_tasks;
pub mod instance_manager;
pub mod instance_tasks;
mod owned_task;

/// Read-only task output contract exposed to domain task ports without giving
/// those domains access to Executor mutation APIs.
pub mod output {
    pub use crate::internal::hanafubuki_process::executor::{OutputDelta, OutputStream};
}

/// Process-environment helpers exposed through the task boundary. Domains
/// which launch owned tasks can prepare an environment without importing the
/// executor crate or gaining access to its mutation APIs.
pub mod environment {
    #[cfg(windows)]
    pub use crate::internal::hanafubuki_process::environment::native_environment_names_equal;
    pub use crate::internal::hanafubuki_process::environment::{
        merge_prepended_path_environment, prepend_path_entries,
    };
}
