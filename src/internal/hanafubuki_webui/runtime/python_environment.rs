//! Rust-owned PATH contract for child processes that use a standalone Python.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_task::environment::merge_prepended_path_environment;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PythonPathPlatform {
    Windows,
    Unix,
}

fn native_path_platform() -> PythonPathPlatform {
    if cfg!(windows) {
        PythonPathPlatform::Windows
    } else {
        PythonPathPlatform::Unix
    }
}

fn python_path_entries_for_platform(
    python_root: &Path,
    platform: PythonPathPlatform,
) -> Result<Vec<PathBuf>, String> {
    if python_root.as_os_str().is_empty() {
        return Err("Python root directory must not be empty".to_string());
    }

    Ok(match platform {
        PythonPathPlatform::Windows => vec![python_root.to_path_buf(), python_root.join("Scripts")],
        PythonPathPlatform::Unix => {
            vec![python_root.to_path_buf(), python_root.join("bin")]
        }
    })
}

/// Returns the standalone-Python directories that must lead PATH. Entries are
/// planned without checking filesystem state so callers can build a launch
/// environment before installation has completed.
pub fn python_path_entries(python_root: &Path) -> Result<Vec<PathBuf>, String> {
    python_path_entries_for_platform(python_root, native_path_platform())
}

/// Normalizes an environment to exactly one native `PATH`, with the selected
/// standalone Python first. A configured PATH replaces the process fallback;
/// the process PATH is used only when the environment has no native PATH key.
pub fn merge_python_path_environment(
    environment: BTreeMap<String, String>,
    python_root: &Path,
    process_path: Option<&OsStr>,
) -> Result<BTreeMap<String, String>, String> {
    merge_prepended_path_environment(environment, python_path_entries(python_root)?, process_path)
}

#[cfg(test)]
mod tests;
