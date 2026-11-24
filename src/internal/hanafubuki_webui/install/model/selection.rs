use serde::{Deserialize, Serialize};

use super::super::error::WebUiInstallBuildError;

fn validate_nonempty_selection(value: String) -> Result<String, WebUiInstallBuildError> {
    if value.trim().is_empty() {
        Err(WebUiInstallBuildError::InvalidPytorchSelection(value))
    } else {
        Ok(value)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct PytorchInstallSelection(String);

impl PytorchInstallSelection {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PytorchInstallSelection {
    type Error = WebUiInstallBuildError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_nonempty_selection(value).map(Self)
    }
}

impl From<PytorchInstallSelection> for String {
    fn from(value: PytorchInstallSelection) -> Self {
        value.0
    }
}

const INVOKEAI_PYTORCH_FAMILIES: &[&str] = &["cuda", "rocm", "xpu", "mps", "cpu"];

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct InvokeAiPytorchFamily(String);

impl InvokeAiPytorchFamily {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for InvokeAiPytorchFamily {
    type Error = WebUiInstallBuildError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if INVOKEAI_PYTORCH_FAMILIES.contains(&value.as_str()) {
            Ok(Self(value))
        } else {
            Err(WebUiInstallBuildError::InvalidPytorchSelectionGranularity {
                webui: "invokeai",
                selection: value,
                expected: "one of cuda, rocm, xpu, mps, or cpu",
            })
        }
    }
}

impl From<InvokeAiPytorchFamily> for String {
    fn from(value: InvokeAiPytorchFamily) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct BranchSelection(String);

impl BranchSelection {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BranchSelection {
    type Error = WebUiInstallBuildError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            Err(WebUiInstallBuildError::InvalidBranch {
                webui: "unknown",
                branch: value,
            })
        } else {
            Ok(Self(value))
        }
    }
}

impl From<BranchSelection> for String {
    fn from(value: BranchSelection) -> Self {
        value.0
    }
}
