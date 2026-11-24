use serde::{Deserialize, Serialize};

/// One persisted global-settings document.
///
/// Global settings used to live in a single `config/settings.json`. That made
/// one corrupt file cost every preference the application has, so each domain
/// now owns its own file: a document that fails to parse or validate loses only
/// its own fields and the rest of the configuration survives untouched.
///
/// A domain is also the unit of recovery. `PersistedSettingsDecoder` already
/// reset exactly these groups to backend defaults when a group was invalid;
/// giving each group a file makes that isolation physical rather than internal.
///
/// No domain holds credentials: the Agent's settings, its Provider registry
/// among them, are the Agent's own configuration (`config/agent/`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingsDomain {
    Language,
    Appearance,
    Paths,
    Network,
    Mirrors,
    Runtime,
    EnvironmentTools,
    Logging,
    Diagnostics,
    Automation,
    Webview,
    Modules,
    Onboarding,
}

impl SettingsDomain {
    /// Every domain, in the order settings files are read and written.
    pub const ALL: &'static [Self] = &[
        Self::Language,
        Self::Appearance,
        Self::Paths,
        Self::Network,
        Self::Mirrors,
        Self::Runtime,
        Self::EnvironmentTools,
        Self::Logging,
        Self::Diagnostics,
        Self::Automation,
        Self::Webview,
        Self::Modules,
        Self::Onboarding,
    ];

    /// File name this domain is persisted under, inside `AppPaths::settings_dir`.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Language => "language.json",
            Self::Appearance => "appearance.json",
            Self::Paths => "paths.json",
            Self::Network => "network.json",
            Self::Mirrors => "mirrors.json",
            Self::Runtime => "runtime.json",
            Self::EnvironmentTools => "environment-tools.json",
            Self::Logging => "logging.json",
            Self::Diagnostics => "diagnostics.json",
            Self::Automation => "automation.json",
            Self::Webview => "webview.json",
            Self::Modules => "modules.json",
            Self::Onboarding => "onboarding.json",
        }
    }

    /// Human-readable name used in recovery reports and diagnostic issues.
    pub fn label(self) -> &'static str {
        match self {
            Self::Language => "language",
            Self::Appearance => "appearance",
            Self::Paths => "paths",
            Self::Network => "network",
            Self::Mirrors => "mirrors",
            Self::Runtime => "runtime",
            Self::EnvironmentTools => "environment tools",
            Self::Logging => "logging",
            Self::Diagnostics => "diagnostics",
            Self::Automation => "automation",
            Self::Webview => "WebView",
            Self::Modules => "modules",
            Self::Onboarding => "onboarding",
        }
    }
}

#[cfg(test)]
mod tests;
