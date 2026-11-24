use std::path::{Path, PathBuf};

use super::build_webui_install_args;
use super::error::WebUiInstallBuildError;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

use super::model::{WebUiInstallOptionCatalog, WebUiInstallRequest};
use super::settings::ResolvedInstallSettings;
use super::validation::validate_request;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebUiInstallPlan {
    target_path: PathBuf,
    python_version: PythonVersion,
    cli_args: Vec<String>,
}

impl WebUiInstallPlan {
    pub fn target_path(&self) -> &Path {
        &self.target_path
    }

    pub fn python_version(&self) -> PythonVersion {
        self.python_version
    }

    pub fn cli_args(&self) -> &[String] {
        &self.cli_args
    }
}

pub fn build_webui_install_plan(
    request: WebUiInstallRequest,
    settings: &ResolvedInstallSettings,
    catalog: &WebUiInstallOptionCatalog,
) -> Result<WebUiInstallPlan, WebUiInstallBuildError> {
    settings.validate()?;
    let validated = validate_request(request, catalog)?;
    let python_version = validated
        .spec()
        .python_version()
        .unwrap_or_else(|| validated.spec().default_python_version());
    let cli_args = build_webui_install_args(&validated, settings)?;
    Ok(WebUiInstallPlan {
        target_path: validated.target_path().clone(),
        python_version,
        cli_args,
    })
}
