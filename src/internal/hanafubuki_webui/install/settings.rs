use serde::{Deserialize, Serialize};

use super::error::WebUiInstallBuildError;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelDownloadSource {
    Huggingface,
    #[default]
    Modelscope,
}

impl ModelDownloadSource {
    pub(crate) fn as_cli_value(self) -> &'static str {
        match self {
            Self::Huggingface => "huggingface",
            Self::Modelscope => "modelscope",
        }
    }
}

impl TryFrom<&str> for ModelDownloadSource {
    type Error = WebUiInstallBuildError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "huggingface" => Ok(Self::Huggingface),
            "modelscope" => Ok(Self::Modelscope),
            other => Err(WebUiInstallBuildError::InvalidGlobalSetting {
                setting: "modelDownloadSource",
                detail: format!("expected huggingface or modelscope, got {other:?}"),
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubMirrorSetting {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub custom: Option<String>,
}

/// The install CLI supports only its built-in PyPI mirror or explicit disablement;
/// it has no argument that carries a custom URL.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PypiMirrorMode {
    BuiltIn,
    Disabled,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInstallSettings {
    #[serde(default)]
    pub automatic_mirror: Option<bool>,
    #[serde(default)]
    pub pypi_mirror: Option<PypiMirrorMode>,
    #[serde(default)]
    pub github_mirror: Option<GithubMirrorSetting>,
    #[serde(default)]
    pub model_download_source: Option<ModelDownloadSource>,
    #[serde(default)]
    pub use_uv: Option<bool>,
}

impl ResolvedInstallSettings {
    pub(crate) fn validate(&self) -> Result<(), WebUiInstallBuildError> {
        if let Some(github) = self.github_mirror.as_ref() {
            if github
                .custom
                .as_ref()
                .is_some_and(|mirror| mirror.trim().is_empty())
            {
                return Err(WebUiInstallBuildError::InvalidGlobalSetting {
                    setting: "githubMirror.custom",
                    detail: "value must not be empty".to_string(),
                });
            }
        }
        Ok(())
    }
}
