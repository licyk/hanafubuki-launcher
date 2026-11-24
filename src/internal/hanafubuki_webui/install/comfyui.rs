use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use serde::{Deserialize, Serialize};

use super::common;
use super::error::WebUiInstallBuildError;
use super::model::{FamilyInstallSpec, PytorchInstallSelection, WebUiFamilyInstallOptionCatalog};
use super::settings::ResolvedInstallSettings;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

const FAMILY: WebUiFamily = WebUiFamily::Comfyui;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ComfyuiInstallSpec {
    #[serde(default)]
    pub pytorch: Option<PytorchInstallSelection>,
    #[serde(default)]
    pub python_version: Option<PythonVersion>,
    #[serde(default)]
    pub predownload_models: Option<bool>,
    #[serde(default)]
    pub predownload_extensions: Option<bool>,
}

const LAUNCH_ARGUMENTS: &[&str] = &[
    "--auto-launch",
    "--preview-method",
    "auto",
    "--disable-cuda-malloc",
    "--enable-manager",
];

impl FamilyInstallSpec for ComfyuiInstallSpec {
    const DEFAULT_PYTHON_VERSION: PythonVersion = PythonVersion::Python312;

    fn python_version(&self) -> Option<PythonVersion> {
        self.python_version
    }

    fn set_python_version(&mut self, version: PythonVersion) {
        self.python_version = Some(version);
    }

    fn pytorch_selection(&self) -> Option<&str> {
        self.pytorch.as_ref().map(PytorchInstallSelection::as_str)
    }

    fn validate(
        &self,
        options: &WebUiFamilyInstallOptionCatalog,
    ) -> Result<(), WebUiInstallBuildError> {
        common::validate_pytorch(FAMILY, self.pytorch_selection(), options)
    }

    fn install_args(
        &self,
        settings: &ResolvedInstallSettings,
        target_path: &str,
    ) -> Result<Vec<String>, WebUiInstallBuildError> {
        let mut args = common::begin(FAMILY, target_path);
        common::push_pytorch(&mut args, "--pytorch-mirror-type", self.pytorch.as_ref());
        common::push_package_and_github_settings(&mut args, settings);
        common::push_disabled_flag(
            &mut args,
            self.predownload_extensions,
            "--no-pre-download-extension",
        );
        common::push_disabled_flag(
            &mut args,
            self.predownload_models,
            "--no-pre-download-model",
        );
        common::finish(&mut args, settings);
        Ok(args)
    }

    fn default_launch_arguments(&self) -> &'static [&'static str] {
        LAUNCH_ARGUMENTS
    }
}
