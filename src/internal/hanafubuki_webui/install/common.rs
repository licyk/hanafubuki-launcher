use crate::internal::hanafubuki_instance_contracts::WebUiFamily;

use super::PypiMirrorMode;
use super::error::WebUiInstallBuildError;
use super::model::{BranchSelection, PytorchInstallSelection, WebUiFamilyInstallOptionCatalog};
use super::settings::ResolvedInstallSettings;

pub(crate) fn begin(family: WebUiFamily, target_path: &str) -> Vec<String> {
    vec![
        family.cli_command().to_string(),
        "install".to_string(),
        family.cli_path_flag().to_string(),
        target_path.to_string(),
    ]
}

pub(crate) fn push_pytorch(
    args: &mut Vec<String>,
    selection_flag: &'static str,
    selection: Option<&PytorchInstallSelection>,
) {
    if let Some(selection) = selection {
        args.push(selection_flag.to_string());
        args.push(selection.as_str().to_string());
    }
}

pub(crate) fn push_package_and_github_settings(
    args: &mut Vec<String>,
    settings: &ResolvedInstallSettings,
) {
    if settings.pypi_mirror == Some(PypiMirrorMode::Disabled) {
        args.push("--no-pypi-mirror".to_string());
    }
    if settings.use_uv == Some(false) {
        args.push("--no-uv".to_string());
    }
    if let Some(github) = settings.github_mirror.as_ref() {
        if github.enabled == Some(false) {
            args.push("--no-github-mirror".to_string());
        }
        if let Some(custom) = github.custom.as_ref() {
            args.push("--custom-github-mirror".to_string());
            args.push(custom.clone());
        }
    }
}

pub(crate) fn push_branch(args: &mut Vec<String>, branch: Option<&BranchSelection>) {
    if let Some(branch) = branch {
        args.push("--install-branch".to_string());
        args.push(branch.as_str().to_string());
    }
}

pub(crate) fn push_disabled_flag(
    args: &mut Vec<String>,
    enabled: Option<bool>,
    disabled_flag: &'static str,
) {
    if enabled == Some(false) {
        args.push(disabled_flag.to_string());
    }
}

pub(crate) fn finish(args: &mut Vec<String>, settings: &ResolvedInstallSettings) {
    if let Some(source) = settings.model_download_source {
        args.push("--model-resource".to_string());
        args.push(source.as_cli_value().to_string());
    }
    push_automatic_mirror(args, settings);
}

pub(crate) fn push_automatic_mirror(args: &mut Vec<String>, settings: &ResolvedInstallSettings) {
    if settings.automatic_mirror == Some(false) {
        args.push("--no-auto-mirror".to_string());
    }
}

pub(crate) fn validate_final_args(args: &[String]) -> Result<(), WebUiInstallBuildError> {
    if args.len() < 4 || args.get(1).map(String::as_str) != Some("install") {
        return Err(WebUiInstallBuildError::CliContractMismatch(
            "install argv must begin with a family command followed by install and a target path"
                .to_string(),
        ));
    }
    if args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "python" | "python3" | "-m" | "sd_webui_all_in_one"
        )
    }) {
        return Err(WebUiInstallBuildError::CliContractMismatch(
            "install argv contains launcher-owned Python arguments".to_string(),
        ));
    }
    Ok(())
}

/// A family's branch presets, each with the launch arguments a new instance
/// on that branch starts with.
pub(crate) type BranchPresets = &'static [(&'static str, &'static [&'static str])];

pub(crate) fn validate_pytorch(
    family: WebUiFamily,
    selection: Option<&str>,
    options: &WebUiFamilyInstallOptionCatalog,
) -> Result<(), WebUiInstallBuildError> {
    let Some(selection) = selection else {
        return Ok(());
    };
    if options.pytorch.contains(selection) {
        Ok(())
    } else {
        Err(WebUiInstallBuildError::PytorchSelectionNotInCatalog {
            webui: family.product_name(),
            selection: selection.to_string(),
        })
    }
}

pub(crate) fn validate_branch(
    family: WebUiFamily,
    branch: Option<&BranchSelection>,
    presets: BranchPresets,
    options: &WebUiFamilyInstallOptionCatalog,
) -> Result<(), WebUiInstallBuildError> {
    let Some(branch) = branch else {
        return Ok(());
    };
    if !presets.iter().any(|(preset, _)| *preset == branch.as_str()) {
        return Err(WebUiInstallBuildError::InvalidBranch {
            webui: family.product_name(),
            branch: branch.as_str().to_string(),
        });
    }
    if !options.branches.contains(branch.as_str()) {
        return Err(WebUiInstallBuildError::BranchNotInCatalog {
            webui: family.product_name(),
            branch: branch.as_str().to_string(),
        });
    }
    Ok(())
}

/// The launch arguments of the selected branch preset, or `fallback` when no
/// preset is selected.
pub(crate) fn branch_launch_arguments(
    branch: Option<&BranchSelection>,
    presets: BranchPresets,
    fallback: &'static [&'static str],
) -> &'static [&'static str] {
    branch
        .and_then(|branch| {
            presets
                .iter()
                .find_map(|(preset, arguments)| (*preset == branch.as_str()).then_some(*arguments))
        })
        .unwrap_or(fallback)
}
