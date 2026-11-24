//! Neutral contracts shared by the instance domains.
//!
//! This crate owns no registry, service implementation, Tauri adapter or
//! infrastructure client. It is the only permitted cross-domain dependency
//! between `hanafubuki-launch` and `hanafubuki-management`, the instance type
//! identity and descriptor the type registry reports (with the server roles
//! a type declares), the built-in WebUI families the built-in provider
//! addresses the upstream library by, the launch argument bounds the configuration repository and Launch both hold an
//! instance's arguments to, and the contracts the instance modules reach each
//! other through (`docs/rules/modules.md`, The instance modules).

mod activity;
mod instance_type;
mod launch_arguments;
mod launch_support;
mod pytorch;
mod server_role;
mod webui_family;

pub use activity::{InstanceLaunchActivity, InstanceManagementActivity};
pub use instance_type::{
    ExternalComponent, InstanceRuntime, InstanceTypeDescriptor, InstanceTypeId, InstanceTypeKind,
};
pub use launch_arguments::{
    LaunchArgumentIssue, MAX_LAUNCH_ARGUMENT_BYTES, MAX_LAUNCH_ARGUMENTS, launch_argument_issue,
    launch_arguments_within_bounds,
};
pub use launch_support::{
    DesktopHotpatchConfig, DesktopHotpatchLaunchConfig, InstanceLaunchReservation,
    InstanceLaunchSupport,
};
pub use pytorch::PytorchCheckResult;
pub use server_role::{
    ServerPort, ServerReadiness, ServerRestart, ServerRole, ServerRoleDescriptor, ServerStart,
    server_start_order,
};
pub use webui_family::WebUiFamily;
