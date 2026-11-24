use std::ffi::OsStr;
use std::mem::{size_of, zeroed};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE, WAIT_FAILED};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::registry::wide;

pub fn run(
    executable: &OsStr,
    parameters: &OsStr,
    action: &str,
    cancellation: &str,
) -> Result<u32, String> {
    let verb = wide("runas");
    let file = wide(executable);
    let parameters = wide(parameters);
    let mut execution: SHELLEXECUTEINFOW = unsafe { zeroed() };
    execution.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    execution.fMask = SEE_MASK_NOCLOSEPROCESS;
    execution.lpVerb = verb.as_ptr();
    execution.lpFile = file.as_ptr();
    execution.lpParameters = parameters.as_ptr();
    execution.nShow = SW_SHOWNORMAL;

    if unsafe { ShellExecuteExW(&mut execution) } == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            return Err(cancellation.to_string());
        }
        return Err(format!("{action}: {error}"));
    }
    if execution.hProcess.is_null() {
        return Err(format!(
            "{action}: Windows did not return a process handle."
        ));
    }

    let process = ProcessHandle(execution.hProcess);
    if unsafe { WaitForSingleObject(process.0, INFINITE) } == WAIT_FAILED {
        return Err(format!(
            "{action}: wait for process: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut exit_code = 0_u32;
    if unsafe { GetExitCodeProcess(process.0, &mut exit_code) } == 0 {
        return Err(format!(
            "{action}: read process result: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(exit_code)
}

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
