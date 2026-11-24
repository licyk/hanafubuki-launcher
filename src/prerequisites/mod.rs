#[cfg(not(windows))]
use crate::internal::hanafubuki_process::executor::ExecutorManager;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LongPathPreparation {
    Ready,
    Restarted,
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    VcRuntimeProgress, elevated_long_path_helper_requested, ensure_vc_runtime, prepare_long_paths,
    run_elevated_long_path_helper,
};

#[cfg(not(windows))]
pub fn prepare_long_paths(_executor: &ExecutorManager) -> Result<LongPathPreparation, String> {
    Ok(LongPathPreparation::Ready)
}

#[cfg(not(windows))]
pub fn elevated_long_path_helper_requested() -> bool {
    false
}

#[cfg(not(windows))]
pub fn run_elevated_long_path_helper() -> Result<(), String> {
    Err("The Windows long-path helper is unavailable on this platform.".to_string())
}
