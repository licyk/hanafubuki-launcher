use std::path::PathBuf;

use super::error::WebUiInstallBuildError;
use super::model::{WebUiInstallOptionCatalog, WebUiInstallRequest, WebUiInstallSpec};

pub struct ValidatedWebUiInstallRequest {
    target_path: PathBuf,
    target_path_arg: String,
    spec: WebUiInstallSpec,
}

impl ValidatedWebUiInstallRequest {
    pub(crate) fn target_path(&self) -> &PathBuf {
        &self.target_path
    }

    pub(crate) fn target_path_arg(&self) -> &str {
        &self.target_path_arg
    }

    pub(crate) fn spec(&self) -> &WebUiInstallSpec {
        &self.spec
    }
}

pub(crate) fn validate_request(
    request: WebUiInstallRequest,
    catalog: &WebUiInstallOptionCatalog,
) -> Result<ValidatedWebUiInstallRequest, WebUiInstallBuildError> {
    if request.target_path.as_os_str().is_empty() {
        return Err(WebUiInstallBuildError::MissingTargetPath);
    }
    if !request.target_path.is_absolute() {
        return Err(WebUiInstallBuildError::NonAbsoluteTargetPath(
            request.target_path,
        ));
    }
    let Some(target_path_arg) = request.target_path.to_str().map(str::to_string) else {
        return Err(WebUiInstallBuildError::NonUtf8TargetPath(
            request.target_path,
        ));
    };
    validate_spec(&request.spec, catalog)?;

    Ok(ValidatedWebUiInstallRequest {
        target_path: request.target_path,
        target_path_arg,
        spec: request.spec,
    })
}

/// Check every selection a spec names against the current catalog, without a
/// target. Reinstall uses it to refuse a changed spec before the instance is
/// quiesced, since a journaled reinstall can only be retried as it was asked.
pub(crate) fn validate_spec(
    spec: &WebUiInstallSpec,
    catalog: &WebUiInstallOptionCatalog,
) -> Result<(), WebUiInstallBuildError> {
    spec.validate(catalog)
}
