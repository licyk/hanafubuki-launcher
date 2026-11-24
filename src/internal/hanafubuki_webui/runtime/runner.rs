//! Pure execution-spec builder for the `sd_webui_all_in_one` Python CLI.
//!
//! This module only builds a structured [`SdWebuiAllInOneRunSpec`] that the command
//! executor layer can consume. It deliberately does **not** execute anything,
//! create/close terminal sessions, start an executor, render a shell command
//! line, quote arguments, read/write config, infer instance paths, check Python
//! availability, or use any Tauri types.
//!
//! Command construction is delegated to [`crate::internal::hanafubuki_webui::runtime`] rather than
//! duplicated here. The install orchestrator and the API server launcher build
//! their runs through it.
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::internal::hanafubuki_webui::runtime::build_sd_webui_all_in_one_cli_args;

const PYTHONPATH_KEY: &str = "PYTHONPATH";

/// Caller-provided inputs for one CLI run.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdWebuiAllInOneRunRequest {
    /// Identifies the instance/task so multiple runs can be distinguished later.
    pub instance_id: String,
    /// Python interpreter used for this run.
    pub python_path: PathBuf,
    /// Arguments passed after `python -m sd_webui_all_in_one`.
    pub cli_args: Vec<String>,
    /// Working directory for the future terminal/executor.
    pub cwd: PathBuf,
    /// Caller-provided environment variables.
    pub env: Vec<(String, String)>,
    /// Launcher-code source root containing `sitecustomize.py` and `sd_webui_all_in_one/`.
    pub source_dir: PathBuf,
}

/// The structured run specification produced by [`build_sd_webui_all_in_one_run_spec`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SdWebuiAllInOneRunSpec {
    pub instance_id: String,
    /// `["<python>", "-m", "sd_webui_all_in_one", ...cli_args]`.
    pub command: Vec<String>,
    pub cwd: PathBuf,
    /// Environment with a single normalized `PYTHONPATH` entry appended last.
    pub env: Vec<(String, String)>,
}

/// The package-internal hotpatch directory for a given launcher-code source root.
pub fn default_hotpatch_dir(source_dir: &Path) -> PathBuf {
    source_dir.join("sd_webui_all_in_one").join("patcher")
}

/// Build the `PYTHONPATH` value so the selected interpreter imports the intended
/// project source. Order is: `<source_dir>`, package-internal patcher dir, then
/// the entries of each caller-provided `PYTHONPATH`-like value (in encounter
/// order).
///
/// Uses [`std::env::split_paths`] / [`std::env::join_paths`] so the platform path
/// separator is handled correctly (never manual `:`/`;` joining).
pub fn build_pythonpath(
    source_dir: &Path,
    existing_pythonpaths: impl IntoIterator<Item = String>,
) -> Result<String, String> {
    let mut paths: Vec<PathBuf> = Vec::new();
    paths.push(source_dir.to_path_buf());
    paths.push(default_hotpatch_dir(source_dir));
    for existing in existing_pythonpaths {
        paths.extend(std::env::split_paths(&existing));
    }

    let joined =
        std::env::join_paths(paths).map_err(|err| format!("failed to build PYTHONPATH: {err}"))?;
    joined
        .into_string()
        .map_err(|_| "PYTHONPATH contains invalid UTF-8".to_string())
}

/// Replace every case-equivalent `PYTHONPATH` entry with one normalized value.
/// Non-PYTHONPATH entries retain their original order and values.
pub fn normalize_pythonpath_environment(
    environment: Vec<(String, String)>,
    source_dir: &Path,
) -> Result<Vec<(String, String)>, String> {
    let mut passthrough = Vec::with_capacity(environment.len());
    let mut existing_pythonpaths = Vec::new();
    for (key, value) in environment {
        if key.eq_ignore_ascii_case(PYTHONPATH_KEY) {
            existing_pythonpaths.push(value);
        } else {
            passthrough.push((key, value));
        }
    }
    let pythonpath = build_pythonpath(source_dir, existing_pythonpaths)?;
    passthrough.push((PYTHONPATH_KEY.to_string(), pythonpath));
    Ok(passthrough)
}

/// Build a pure [`SdWebuiAllInOneRunSpec`] from a request.
///
/// Validates only cheap structural inputs (non-empty instance id / python path /
/// source dir / cwd); it does **not** check whether any path exists. The command
/// is built via [`crate::internal::hanafubuki_webui::runtime`], and `PYTHONPATH` is injected per
/// [`build_pythonpath`], with caller `PYTHONPATH`-like entries removed from the
/// output env and replaced by one normalized `PYTHONPATH` appended last.
pub fn build_sd_webui_all_in_one_run_spec(
    request: SdWebuiAllInOneRunRequest,
) -> Result<SdWebuiAllInOneRunSpec, String> {
    if request.instance_id.trim().is_empty() {
        return Err("instance_id must not be empty".to_string());
    }
    if request.source_dir.as_os_str().is_empty() {
        return Err("source_dir must not be empty".to_string());
    }
    if request.cwd.as_os_str().is_empty() {
        return Err("cwd must not be empty".to_string());
    }
    let python_path = request
        .python_path
        .to_str()
        .ok_or_else(|| "python_path is not valid UTF-8".to_string())?;
    if python_path.trim().is_empty() {
        return Err("python_path must not be empty".to_string());
    }

    let command = build_sd_webui_all_in_one_cli_args(python_path, &request.cli_args)?;

    let env = normalize_pythonpath_environment(request.env, &request.source_dir)?;

    Ok(SdWebuiAllInOneRunSpec {
        instance_id: request.instance_id,
        command,
        cwd: request.cwd,
        env,
    })
}

#[cfg(test)]
mod tests;
