//! Shared standalone Python runtime manager.
//!
//! This is the generic, **path-agnostic** lifecycle for a standalone Python
//! runtime: resolve the interpreter executable,
//! check usability by running `<python> --version` through the command executor
//! ([`crate::internal::hanafubuki_process::executor`]), install/re-check via the low-level
//! [`crate::internal::hanafubuki_python_runtime::installer`] downloader, switch versions, and clean up
//! broken/partial directories on failure.
//!
//! Callers supply the `python_dir` and version policy, so the same logic
//! serves the launcher Python (`python_manager`) and, later, per-instance Python
//! runtimes. It never uses an external shell, `std::process::Command`, or a
//! manifest/`runtime.json` trust source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_process::executor::manager::{ExecutorManager, run_and_wait};
use crate::internal::hanafubuki_process::executor::model::{ExecutorIoMode, ExecutorStartRequest};
use crate::internal::hanafubuki_python_runtime::installer::{
    InstallStandalonePythonRequest, PythonInstallError, install_standalone_python_cancellable,
    install_standalone_python_inner,
};
use crate::internal::hanafubuki_python_runtime::version::PythonVersion;

/// Time budget for the `python --version` usability check.
const CHECK_TIMEOUT_MS: u64 = 10_000;

// ──────────────────────────────────────────────────────────────────────────
// Shared request / result types.
// ──────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct StandalonePythonRuntimeInstallRequest {
    pub python_dir: PathBuf,
    pub version: PythonVersion,
}

#[derive(Clone, Debug)]
pub struct StandalonePythonRuntimeEnsureRequest {
    pub python_dir: PathBuf,
    /// The version installed when no usable runtime exists.
    pub default_version: PythonVersion,
}

#[derive(Clone, Debug)]
pub struct StandalonePythonRuntimeSwitchRequest {
    pub python_dir: PathBuf,
    pub version: PythonVersion,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandalonePythonRuntimeCheckResult {
    pub usable: bool,
    pub python_path: PathBuf,
    pub version_output: String,
    pub exit_code: Option<i32>,
    /// Why the interpreter could not be run at all, as opposed to running and
    /// reporting something unusable.
    ///
    /// A check that never started and a check that exited non-zero both leave
    /// `usable` false, but only one of them says anything about the
    /// interpreter. Keeping the reason means a transient spawn failure is
    /// diagnosable instead of arriving as a bare "not usable".
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandalonePythonRuntimeInfo {
    pub python_dir: PathBuf,
    pub python_path: PathBuf,
    pub version: String,
    pub usable: bool,
}

// ──────────────────────────────────────────────────────────────────────────
// Executable resolution.
// ──────────────────────────────────────────────────────────────────────────

/// The primary standalone Python executable path for a python directory.
pub fn standalone_python_executable(python_dir: &Path) -> PathBuf {
    python_executable_candidates(python_dir)
        .into_iter()
        .next()
        .expect("at least one candidate")
}

/// Platform-specific candidate executable paths, most-preferred first
/// (Windows: `python.exe`; Unix: `bin/python3`, then `bin/python`).
fn python_executable_candidates(python_dir: &Path) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        vec![python_dir.join("python.exe")]
    }
    #[cfg(not(windows))]
    {
        vec![
            python_dir.join("bin").join("python3"),
            python_dir.join("bin").join("python"),
        ]
    }
}

/// The first candidate executable that exists as a file, if any.
pub fn resolve_existing_python_executable(python_dir: &Path) -> Option<PathBuf> {
    python_executable_candidates(python_dir)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

// ──────────────────────────────────────────────────────────────────────────
// Lifecycle: ensure / install / switch.
// ──────────────────────────────────────────────────────────────────────────

/// Ensure a usable standalone Python exists at `python_dir`, installing the
/// default version if it is missing or unusable. Re-checks after install.
pub fn ensure_standalone_python_runtime(
    executor: &ExecutorManager,
    request: StandalonePythonRuntimeEnsureRequest,
) -> Result<StandalonePythonRuntimeInfo, String> {
    ensure_standalone_python_runtime_with_policy(executor, request, true)
}

pub fn ensure_standalone_python_runtime_preserving_failure_cancellable(
    executor: &ExecutorManager,
    request: StandalonePythonRuntimeEnsureRequest,
    cancellation: &CancellationToken,
) -> Result<StandalonePythonRuntimeInfo, PythonInstallError> {
    if cancellation.is_cancelled() {
        return Err(PythonInstallError::Cancelled);
    }
    let python_dir = request.python_dir.clone();
    if let Some(executable) = resolve_existing_python_executable(&python_dir) {
        let check = check_standalone_python_runtime(executor, &executable);
        if cancellation.is_cancelled() {
            return Err(PythonInstallError::Cancelled);
        }
        if check.usable {
            return Ok(info_from_check(python_dir, executable, check));
        }
        remove_dir_if_exists(&python_dir).map_err(PythonInstallError::Failed)?;
    } else if python_dir.exists() {
        remove_dir_if_exists(&python_dir).map_err(PythonInstallError::Failed)?;
    }
    if cancellation.is_cancelled() {
        return Err(PythonInstallError::Cancelled);
    }

    let version = request.default_version;
    let install = install_standalone_python_cancellable(
        InstallStandalonePythonRequest {
            version,
            target_dir: python_dir.clone(),
            overwrite: Some(true),
        },
        cancellation,
    )?;
    debug_assert_eq!(install.target_dir, python_dir);
    if cancellation.is_cancelled() {
        return Err(PythonInstallError::Cancelled);
    }
    verify_installed_python_with_policy(executor, &python_dir, version, false)
        .map_err(PythonInstallError::Failed)
}

fn ensure_standalone_python_runtime_with_policy(
    executor: &ExecutorManager,
    request: StandalonePythonRuntimeEnsureRequest,
    cleanup_failed_install: bool,
) -> Result<StandalonePythonRuntimeInfo, String> {
    let python_dir = request.python_dir.clone();

    if let Some(executable) = resolve_existing_python_executable(&python_dir) {
        let check = check_standalone_python_runtime(executor, &executable);
        if check.usable {
            log::info!("ensure_standalone_python_runtime: existing runtime is usable");
            return Ok(info_from_check(python_dir, executable, check));
        }
        log::info!("ensure_standalone_python_runtime: existing runtime unusable; clearing dir");
        remove_dir_if_exists(&python_dir)?;
    } else if python_dir.exists() {
        // Executable missing but directory present => partial/broken install.
        log::info!("ensure_standalone_python_runtime: clearing partial install");
        remove_dir_if_exists(&python_dir)?;
    }

    log::info!("ensure_standalone_python_runtime: installing default version");
    install_standalone_python_runtime_with_policy(
        executor,
        StandalonePythonRuntimeInstallRequest {
            python_dir: request.python_dir,
            version: request.default_version,
        },
        cleanup_failed_install,
    )
}

/// Install (or reinstall) a standalone Python at the requested version, replacing any existing install, then verify via `--version`. On any
/// download/extract or verification failure the (possibly partial) directory is
/// removed before returning.
pub fn install_standalone_python_runtime(
    executor: &ExecutorManager,
    request: StandalonePythonRuntimeInstallRequest,
) -> Result<StandalonePythonRuntimeInfo, String> {
    install_standalone_python_runtime_with_policy(executor, request, true)
}

fn install_standalone_python_runtime_with_policy(
    executor: &ExecutorManager,
    request: StandalonePythonRuntimeInstallRequest,
    cleanup_failed_install: bool,
) -> Result<StandalonePythonRuntimeInfo, String> {
    let version = request.version;
    let python_dir = request.python_dir;
    remove_dir_if_exists(&python_dir)?;

    log::info!(
        "install_standalone_python_runtime: version={version} target_dir={}",
        python_dir.display()
    );
    if let Err(install_err) = install_standalone_python_inner(InstallStandalonePythonRequest {
        version,
        target_dir: python_dir.clone(),
        overwrite: Some(true),
    }) {
        return Err(failed_install_error(
            &python_dir,
            install_err,
            cleanup_failed_install,
        ));
    }

    verify_installed_python_with_policy(executor, &python_dir, version, cleanup_failed_install)
}

/// Switch the standalone Python at `python_dir` to `version`, intentionally
/// replacing the existing install, then re-check.
pub fn switch_standalone_python_runtime_version(
    executor: &ExecutorManager,
    request: StandalonePythonRuntimeSwitchRequest,
) -> Result<StandalonePythonRuntimeInfo, String> {
    remove_dir_if_exists(&request.python_dir)?;
    install_standalone_python_runtime(
        executor,
        StandalonePythonRuntimeInstallRequest {
            python_dir: request.python_dir,
            version: request.version,
        },
    )
}

/// Verify a freshly-installed runtime: resolve its executable and run the
/// `--version` usability check. On a missing executable or an unusable result,
/// the (partial/unusable) `python_dir` is removed and an error is returned.
#[cfg(test)]
fn verify_installed_python(
    executor: &ExecutorManager,
    python_dir: &Path,
    version: PythonVersion,
) -> Result<StandalonePythonRuntimeInfo, String> {
    verify_installed_python_with_policy(executor, python_dir, version, true)
}

fn verify_installed_python_with_policy(
    executor: &ExecutorManager,
    python_dir: &Path,
    version: PythonVersion,
    cleanup_failed_install: bool,
) -> Result<StandalonePythonRuntimeInfo, String> {
    let executable = match resolve_existing_python_executable(python_dir) {
        Some(executable) => executable,
        None => {
            let cause = format!(
                "python executable not found after install in {}",
                python_dir.display()
            );
            return Err(failed_install_error(
                python_dir,
                cause,
                cleanup_failed_install,
            ));
        }
    };

    let check = check_standalone_python_runtime(executor, &executable);
    if !check.usable {
        let cause = format!(
            "installed standalone python (version {version}) is not usable: exit_code={:?} version_output={:?}",
            check.exit_code,
            check.version_output.trim()
        );
        return Err(failed_install_error(
            python_dir,
            cause,
            cleanup_failed_install,
        ));
    }

    Ok(info_from_check(python_dir.to_path_buf(), executable, check))
}

// ──────────────────────────────────────────────────────────────────────────
// Usability check (via the command executor) + version parsing.
// ──────────────────────────────────────────────────────────────────────────

/// Run `<python> --version` through the command executor (argv, pipe mode) and
/// decide usability from the exit code and parsed version. The executor bounds
/// the run with `timeout_ms` and disposes the session itself.
pub fn check_standalone_python_runtime(
    executor: &ExecutorManager,
    python_path: &Path,
) -> StandalonePythonRuntimeCheckResult {
    let mut result = StandalonePythonRuntimeCheckResult {
        usable: false,
        python_path: python_path.to_path_buf(),
        version_output: String::new(),
        exit_code: None,
        error: None,
    };

    let request = ExecutorStartRequest {
        executor_id: None,
        command: vec![
            python_path.to_string_lossy().into_owned(),
            "--version".to_string(),
        ],
        cwd: None,
        environment_inheritance:
            crate::internal::hanafubuki_process::executor::EnvironmentInheritance::Inherit,
        env: HashMap::new(),
        io_mode: ExecutorIoMode::Pipe,
        stream_stdin: false,
        stream_output: true,
        cols: None,
        rows: None,
        timeout_ms: Some(CHECK_TIMEOUT_MS),
        output_bytes_cap: None,
    };

    match run_and_wait(
        executor,
        request,
        Duration::from_millis(CHECK_TIMEOUT_MS + 2_000),
    ) {
        Ok(run) => {
            // `python --version` may print to stdout or stderr; both are captured.
            result.exit_code = run.exit_code;
            result.usable = run.exit_code == Some(0) && parse_python_version(&run.output).is_some();
            result.version_output = run.output;
        }
        Err(err) => {
            log::warn!("standalone python check failed: {err}");
            result.error = Some(err);
        }
    }

    result
}

/// Extract a version token (e.g. `3.11.14`) from `python --version` output such
/// as `"Python 3.11.14"` or `"Python 3.11.14\r\n"`.
fn parse_python_version(output: &str) -> Option<String> {
    let mut after_python = false;
    for token in output.split_whitespace() {
        if after_python
            && token
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            return Some(token.to_string());
        }
        if token.eq_ignore_ascii_case("Python") {
            after_python = true;
        }
    }
    None
}

fn info_from_check(
    python_dir: PathBuf,
    python_path: PathBuf,
    check: StandalonePythonRuntimeCheckResult,
) -> StandalonePythonRuntimeInfo {
    let version = parse_python_version(&check.version_output).unwrap_or_default();
    StandalonePythonRuntimeInfo {
        python_dir,
        python_path,
        version,
        usable: check.usable,
    }
}

/// Resolves and validates a standalone Python runtime at an application-owned
/// directory.
///
/// Validation is intentionally performed through `ExecutorManager`, so every
/// process start follows the same typed, shell-free executor boundary used by
/// the rest of the desktop application.
pub fn resolve_usable_python(
    executor: &ExecutorManager,
    python_dir: &Path,
) -> Result<StandalonePythonRuntimeInfo, String> {
    let python_path = resolve_existing_python_executable(python_dir)
        .ok_or_else(|| format!("no standalone Python was found in {}", python_dir.display()))?;
    let check = check_standalone_python_runtime(executor, &python_path);
    if !check.usable {
        let output = check.version_output.trim();
        let detail = if output.is_empty() {
            "no version output".to_string()
        } else {
            output.to_string()
        };
        return Err(format!(
            "standalone Python is not usable at {} (exit code: {:?}): {detail}",
            python_path.display(),
            check.exit_code
        ));
    }

    Ok(info_from_check(
        python_dir.to_path_buf(),
        python_path,
        check,
    ))
}

// ──────────────────────────────────────────────────────────────────────────
// Helpers.
// ──────────────────────────────────────────────────────────────────────────

fn remove_dir_if_exists(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|err| format!("remove {}: {err}", dir.display()))?;
    }
    Ok(())
}

/// Remove a broken/partial Python directory after a failed install or failed
/// verification, preserving the root `cause`. If cleanup itself fails, the
/// cleanup error is appended so neither problem is hidden.
pub fn cleanup_after_failed_install(python_dir: &Path, cause: String) -> String {
    match remove_dir_if_exists(python_dir) {
        Ok(()) => cause,
        Err(cleanup_err) => format!(
            "{cause}; additionally failed to clean {}: {cleanup_err}",
            python_dir.display()
        ),
    }
}

fn failed_install_error(python_dir: &Path, cause: String, cleanup: bool) -> String {
    if cleanup {
        cleanup_after_failed_install(python_dir, cause)
    } else {
        cause
    }
}

#[cfg(test)]
fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
