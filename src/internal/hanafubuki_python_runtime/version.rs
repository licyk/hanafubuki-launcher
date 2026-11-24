//! The Python feature releases a standalone runtime can be installed at.
//!
//! This is the one version list: the download table in [`crate::internal::hanafubuki_python_runtime::installer`]
//! must offer every variant, and the application runtime setting and every
//! WebUI install spec select from it.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum PythonVersion {
    #[serde(rename = "3.10")]
    Python310,
    #[serde(rename = "3.11")]
    Python311,
    #[serde(rename = "3.12")]
    Python312,
    #[serde(rename = "3.13")]
    Python313,
    #[serde(rename = "3.14")]
    Python314,
    #[serde(rename = "3.15")]
    Python315,
}

impl PythonVersion {
    pub const ALL: [Self; 6] = [
        Self::Python310,
        Self::Python311,
        Self::Python312,
        Self::Python313,
        Self::Python314,
        Self::Python315,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Python310 => "3.10",
            Self::Python311 => "3.11",
            Self::Python312 => "3.12",
            Self::Python313 => "3.13",
            Self::Python314 => "3.14",
            Self::Python315 => "3.15",
        }
    }

    /// Whether `version` (such as `3.11.14` from `python --version`) is a
    /// patch release of this feature release.
    pub fn matches_release(self, version: &str) -> bool {
        let mut components = version.split('.');
        match (components.next(), components.next()) {
            (Some(major), Some(minor)) => {
                let mut expected = self.as_str().split('.');
                expected.next() == Some(major) && expected.next() == Some(minor)
            }
            _ => false,
        }
    }
}

impl fmt::Display for PythonVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A version string that names no supported feature release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedPythonVersion(pub String);

impl fmt::Display for UnsupportedPythonVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unsupported python version: {}", self.0)
    }
}

impl std::error::Error for UnsupportedPythonVersion {}

impl TryFrom<&str> for PythonVersion {
    type Error = UnsupportedPythonVersion;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|version| version.as_str() == value)
            .ok_or_else(|| UnsupportedPythonVersion(value.to_string()))
    }
}

#[cfg(test)]
mod tests;
