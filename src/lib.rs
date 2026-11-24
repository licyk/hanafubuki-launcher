//! Hanafubuki's launcher: it prepares the workspace, updates itself and the application,
//! and starts Hanafubuki.
//!
//! The crate is a single binary crate. Everything it used to depend on as a workspace crate
//! lives under [`internal`], one module per former crate, and keeps that crate's module
//! paths below [`internal`] (`internal::hanafubuki_foundation::paths` was
//! `hanafubuki_foundation::paths`). [`internal`] keeps those names out of the launcher's own
//! namespace: the launcher itself owns `process`, `workspace`, `platform`, `ui`, and friends.
//!
//! `src` is hosted by the module tree alone, so every module name below is also a path.

pub mod internal;
pub mod logger;
pub mod network;
pub mod platform;
pub mod portable_app;
pub mod prerequisites;
pub mod process;
pub mod self_update;
pub mod signature;
pub mod ui;
pub mod workflow;
pub mod workspace;

/// Runs the launcher to completion and returns the process exit code.
pub fn run() -> std::process::ExitCode {
    let executor = internal::hanafubuki_process::executor::ExecutorManager::default();
    if let Some(exit_code) = self_update::run_internal_mode(&executor) {
        // The internal modes report 0 or 1; anything else is outside the exit code's range and
        // becomes the generic failure.
        return std::process::ExitCode::from(u8::try_from(exit_code).unwrap_or(1));
    }
    if prerequisites::elevated_long_path_helper_requested() {
        let exit_code = if prerequisites::run_elevated_long_path_helper().is_ok() {
            0
        } else {
            1
        };
        return std::process::ExitCode::from(exit_code);
    }
    ui::run(executor)
}
