use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebUiInstallBuildError {
    MissingTargetPath,
    NonAbsoluteTargetPath(PathBuf),
    NonUtf8TargetPath(PathBuf),
    InvalidPytorchSelection(String),
    InvalidPytorchSelectionGranularity {
        webui: &'static str,
        selection: String,
        expected: &'static str,
    },
    InvalidBranch {
        webui: &'static str,
        branch: String,
    },
    PytorchSelectionNotInCatalog {
        webui: &'static str,
        selection: String,
    },
    BranchNotInCatalog {
        webui: &'static str,
        branch: String,
    },
    UnsupportedOption {
        webui: &'static str,
        option: &'static str,
    },
    InvalidGlobalSetting {
        setting: &'static str,
        detail: String,
    },
    ContradictoryGlobalSettings(String),
    CliContractMismatch(String),
}

impl fmt::Display for WebUiInstallBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTargetPath => write!(formatter, "WebUI target path is required"),
            Self::NonAbsoluteTargetPath(path) => {
                write!(formatter, "WebUI target path must be absolute: {path:?}")
            }
            Self::NonUtf8TargetPath(path) => {
                write!(formatter, "WebUI target path is not valid UTF-8: {path:?}")
            }
            Self::InvalidPytorchSelection(selection) => {
                write!(formatter, "invalid PyTorch selection: {selection:?}")
            }
            Self::InvalidPytorchSelectionGranularity {
                webui,
                selection,
                expected,
            } => write!(
                formatter,
                "invalid PyTorch selection {selection:?} for {webui}; expected {expected}"
            ),
            Self::InvalidBranch { webui, branch } => {
                write!(formatter, "invalid branch {branch:?} for {webui}")
            }
            Self::PytorchSelectionNotInCatalog { webui, selection } => write!(
                formatter,
                "PyTorch selection {selection:?} is not in the current {webui} catalog"
            ),
            Self::BranchNotInCatalog { webui, branch } => write!(
                formatter,
                "branch {branch:?} is not in the current {webui} catalog"
            ),
            Self::UnsupportedOption { webui, option } => {
                write!(
                    formatter,
                    "{webui} install does not support option {option}"
                )
            }
            Self::InvalidGlobalSetting { setting, detail } => {
                write!(
                    formatter,
                    "invalid resolved install setting {setting}: {detail}"
                )
            }
            Self::ContradictoryGlobalSettings(detail) => {
                write!(
                    formatter,
                    "contradictory resolved install settings: {detail}"
                )
            }
            Self::CliContractMismatch(detail) => {
                write!(
                    formatter,
                    "sd_webui_all_in_one CLI contract mismatch: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for WebUiInstallBuildError {}
