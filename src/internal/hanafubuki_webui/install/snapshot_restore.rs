use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_instance_contracts::WebUiFamily;

use super::common;
use super::error::WebUiInstallBuildError;
use super::settings::ResolvedInstallSettings;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebUiSnapshotRestorePlan {
    python_version: PythonVersion,
    cli_args: Vec<String>,
}

impl WebUiSnapshotRestorePlan {
    pub fn python_version(&self) -> PythonVersion {
        self.python_version
    }

    pub fn cli_args(&self) -> &[String] {
        &self.cli_args
    }
}

fn absolute_path_arg(path: &Path, label: &str) -> Result<String, WebUiInstallBuildError> {
    if !path.is_absolute() {
        return Err(WebUiInstallBuildError::CliContractMismatch(format!(
            "{label} must be absolute"
        )));
    }
    path.to_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            WebUiInstallBuildError::CliContractMismatch(format!("{label} is not valid UTF-8"))
        })
}

pub fn build_webui_snapshot_restore_plan(
    family: WebUiFamily,
    python_version: PythonVersion,
    snapshot_path: PathBuf,
    target_path: PathBuf,
    settings: &ResolvedInstallSettings,
) -> Result<WebUiSnapshotRestorePlan, WebUiInstallBuildError> {
    settings.validate()?;
    let snapshot_path = absolute_path_arg(&snapshot_path, "snapshot path")?;
    let target_path = absolute_path_arg(&target_path, "WebUI target path")?;
    let mut cli_args = vec![
        family.cli_command().to_string(),
        "restore".to_string(),
        snapshot_path,
        family.cli_path_flag().to_string(),
        target_path,
        "--prune-packages".to_string(),
        "--prune-extensions".to_string(),
        "--force-git-reset".to_string(),
    ];
    common::push_package_and_github_settings(&mut cli_args, settings);
    common::push_automatic_mirror(&mut cli_args, settings);
    Ok(WebUiSnapshotRestorePlan {
        python_version,
        cli_args,
    })
}

#[cfg(test)]
mod tests;
