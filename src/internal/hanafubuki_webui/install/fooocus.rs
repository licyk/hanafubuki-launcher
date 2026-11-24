use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use serde::{Deserialize, Serialize};

use super::common::{self, BranchPresets};
use super::error::WebUiInstallBuildError;
use super::model::{
    BranchSelection, FamilyInstallSpec, PytorchInstallSelection, WebUiFamilyInstallOptionCatalog,
};
use super::settings::ResolvedInstallSettings;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

const FAMILY: WebUiFamily = WebUiFamily::Fooocus;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FooocusInstallSpec {
    #[serde(default)]
    pub pytorch: Option<PytorchInstallSelection>,
    #[serde(default)]
    pub python_version: Option<PythonVersion>,
    #[serde(default)]
    pub branch: Option<BranchSelection>,
    #[serde(default)]
    pub predownload_models: Option<bool>,
}

const LAUNCH_ARGUMENTS: &[&str] = &[
    "--language",
    "zh",
    "--preset",
    "sd_webui_all_in_one",
    "--disable-offload-from-vram",
    "--disable-analytics",
    "--always-download-new-model",
];

const BRANCHES: BranchPresets = &[
    ("fooocus_main", LAUNCH_ARGUMENTS),
    ("ruined_fooocus_main", LAUNCH_ARGUMENTS),
    ("fooocus_mre_main", LAUNCH_ARGUMENTS),
];

impl FamilyInstallSpec for FooocusInstallSpec {
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

    fn branch(&self) -> Option<&BranchSelection> {
        self.branch.as_ref()
    }

    fn set_branch(&mut self, branch: BranchSelection) -> Result<(), String> {
        self.branch = Some(branch);
        Ok(())
    }

    fn validate(
        &self,
        options: &WebUiFamilyInstallOptionCatalog,
    ) -> Result<(), WebUiInstallBuildError> {
        common::validate_pytorch(FAMILY, self.pytorch_selection(), options)?;
        common::validate_branch(FAMILY, self.branch.as_ref(), BRANCHES, options)
    }

    fn install_args(
        &self,
        settings: &ResolvedInstallSettings,
        target_path: &str,
    ) -> Result<Vec<String>, WebUiInstallBuildError> {
        let mut args = common::begin(FAMILY, target_path);
        common::push_pytorch(&mut args, "--pytorch-mirror-type", self.pytorch.as_ref());
        common::push_package_and_github_settings(&mut args, settings);
        common::push_branch(&mut args, self.branch.as_ref());
        common::push_disabled_flag(
            &mut args,
            self.predownload_models,
            "--no-pre-download-model",
        );
        common::finish(&mut args, settings);
        Ok(args)
    }

    fn default_launch_arguments(&self) -> &'static [&'static str] {
        common::branch_launch_arguments(self.branch.as_ref(), BRANCHES, LAUNCH_ARGUMENTS)
    }
}
