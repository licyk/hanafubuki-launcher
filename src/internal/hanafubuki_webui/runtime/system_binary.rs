use std::ffi::OsStr;
use std::path::PathBuf;

use crate::internal::hanafubuki_instance_contracts::InstanceRuntime;

/// The program a `system-binary` runtime names, found on `path` (the
/// process's `PATH`); an instance of such a type installs and starts only
/// when it is there. Any other runtime needs none.
pub fn require_runtime_program(
    runtime: &InstanceRuntime,
    path: Option<&OsStr>,
) -> Result<Option<PathBuf>, String> {
    let InstanceRuntime::SystemBinary { name } = runtime else {
        return Ok(None);
    };
    let candidates: Vec<String> = if cfg!(windows) {
        vec![name.clone(), format!("{name}.exe"), format!("{name}.cmd")]
    } else {
        vec![name.clone()]
    };
    path.into_iter()
        .flat_map(std::env::split_paths)
        .flat_map(|dir| candidates.iter().map(move |candidate| dir.join(candidate)))
        .find(|candidate| candidate.is_file())
        .map(Some)
        .ok_or_else(|| format!("the instance's type runs on `{name}`, which is not on PATH"))
}

#[cfg(test)]
mod tests;
