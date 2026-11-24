use serde::{Deserialize, Serialize};

/// The PyTorch check Python reports for an instance's environment. Every field
/// is optional on the wire: a missing `is_compatible` is `None`, compatibility
/// unknown, which a launch treats as a warning rather than a decision.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    default,
    rename_all(serialize = "camelCase", deserialize = "snake_case")
)]
pub struct PytorchCheckResult {
    pub available_types: Vec<String>,
    pub gpu_list: Vec<String>,
    pub has_gpu: bool,
    pub installed_version: Option<String>,
    pub installed_type: Option<String>,
    pub status: String,
    pub is_compatible: Option<bool>,
    pub message: String,
}
