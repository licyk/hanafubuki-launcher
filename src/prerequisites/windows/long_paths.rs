use std::ffi::OsStr;

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_process::executor::ExecutorManager;
use windows_sys::Win32::System::Registry::{KEY_QUERY_VALUE, KEY_SET_VALUE};

use crate::platform::windows::{elevation, registry};
use crate::process::ProcessCommand;

use super::super::LongPathPreparation;

const ELEVATED_HELPER_ARGUMENT: &str = "--hanafubuki-enable-long-paths";
const FILE_SYSTEM_KEY: &str = r"SYSTEM\CurrentControlSet\Control\FileSystem";
const LONG_PATHS_VALUE: &str = "LongPathsEnabled";

pub fn elevated_helper_requested() -> bool {
    std::env::args_os()
        .skip(1)
        .any(|argument| argument == OsStr::new(ELEVATED_HELPER_ARGUMENT))
}

pub fn run_elevated_helper() -> Result<(), String> {
    registry::write_machine_dword(
        FILE_SYSTEM_KEY,
        LONG_PATHS_VALUE,
        1,
        KEY_SET_VALUE,
        "Windows FileSystem",
    )?;
    if enabled()? {
        Ok(())
    } else {
        Err("Windows did not retain the LongPathsEnabled registry value.".to_string())
    }
}

pub fn prepare(executor: &ExecutorManager) -> Result<LongPathPreparation, String> {
    if enabled()? {
        return Ok(LongPathPreparation::Ready);
    }

    run_elevated_self()?;
    if !enabled()? {
        return Err(
            "Windows long-path support is still disabled after administrator approval.".to_string(),
        );
    }

    let executable = current_executable()?;
    ProcessCommand::new(executable.native_text())
        .start_independent(executor)
        .map_err(|error| format!("Restart Hanafubuki Launcher: {error}"))?;
    Ok(LongPathPreparation::Restarted)
}

fn enabled() -> Result<bool, String> {
    registry::read_machine_dword(
        FILE_SYSTEM_KEY,
        LONG_PATHS_VALUE,
        KEY_QUERY_VALUE,
        "Windows FileSystem",
    )
    .map(|value| value == Some(1))
}

fn run_elevated_self() -> Result<(), String> {
    let executable = current_executable()?;
    let exit_code = elevation::run(
        executable.native().as_os_str(),
        OsStr::new(ELEVATED_HELPER_ARGUMENT),
        "Start the Windows long-path administrator helper",
        "Administrator approval for Windows long paths was cancelled.",
    )?;
    if exit_code != 0 {
        return Err(format!(
            "The Windows long-path administrator helper failed with exit code {exit_code}."
        ));
    }
    Ok(())
}

fn current_executable() -> Result<ResolvedPath, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Find Hanafubuki Launcher executable: {error}"))?;
    ResolvedPath::resolve(executable).map_err(|error| error.to_string())
}
