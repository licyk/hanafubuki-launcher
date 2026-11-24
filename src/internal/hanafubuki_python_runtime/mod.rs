//! Standalone Python download, installation, verification, and lifecycle.
//!
//! Callers own application paths and domain coordination. This crate accepts
//! resolved filesystem paths and never depends on Tauri, Config, or a business
//! registry.

mod installer;
mod manager;
mod version;

pub use installer::{
    InstallStandalonePythonRequest, InstallStandalonePythonResult, PythonInstallError, UpwardLinks,
    download_verified_file, download_with_digest, extract_archive_confined, fetch_bounded,
    install_standalone_python_cancellable, install_standalone_python_inner,
};
pub use manager::{
    StandalonePythonRuntimeCheckResult, StandalonePythonRuntimeEnsureRequest,
    StandalonePythonRuntimeInfo, StandalonePythonRuntimeInstallRequest,
    StandalonePythonRuntimeSwitchRequest, check_standalone_python_runtime,
    cleanup_after_failed_install, ensure_standalone_python_runtime,
    ensure_standalone_python_runtime_preserving_failure_cancellable,
    install_standalone_python_runtime, resolve_existing_python_executable, resolve_usable_python,
    standalone_python_executable, switch_standalone_python_runtime_version,
};
pub use version::{PythonVersion, UnsupportedPythonVersion};
