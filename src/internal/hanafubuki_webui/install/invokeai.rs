use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use serde::{Deserialize, Serialize};

use super::common;
use super::error::WebUiInstallBuildError;
use super::model::{
    FamilyInstallSpec, InvokeAiPytorchFamily, PytorchOptionSource, WebUiFamilyInstallOptionCatalog,
};
use super::settings::ResolvedInstallSettings;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

const FAMILY: WebUiFamily = WebUiFamily::Invokeai;

/// InvokeAI installs PyTorch for a device family (`cuda`, `cpu`, ...) rather
/// than a concrete build, so its `pytorch` selection is a device family.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InvokeaiInstallSpec {
    #[serde(default)]
    pub pytorch: Option<InvokeAiPytorchFamily>,
    #[serde(default)]
    pub python_version: Option<PythonVersion>,
    #[serde(default)]
    pub predownload_models: Option<bool>,
}

impl FamilyInstallSpec for InvokeaiInstallSpec {
    const DEFAULT_PYTHON_VERSION: PythonVersion = PythonVersion::Python312;
    const PYTORCH_OPTIONS: PytorchOptionSource = PytorchOptionSource::DeviceCategories;
    /// InvokeAI creates its Core directory during first launch.
    const REQUIRES_PREEXISTING_CORE_DIRECTORY: bool = false;

    fn python_version(&self) -> Option<PythonVersion> {
        self.python_version
    }

    fn set_python_version(&mut self, version: PythonVersion) {
        self.python_version = Some(version);
    }

    fn pytorch_selection(&self) -> Option<&str> {
        self.pytorch.as_ref().map(InvokeAiPytorchFamily::as_str)
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
        if let Some(family) = self.pytorch.as_ref() {
            args.push("--device-type".to_string());
            args.push(family.as_str().to_string());
        }
        common::push_package_and_github_settings(&mut args, settings);
        common::push_disabled_flag(
            &mut args,
            self.predownload_models,
            "--no-pre-download-model",
        );
        common::finish(&mut args, settings);
        Ok(args)
    }

    fn default_launch_arguments(&self) -> &'static [&'static str] {
        &[]
    }
}
