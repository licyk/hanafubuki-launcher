use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateProvider {
    Modelscope,
    Huggingface,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppPlatform {
    Windows,
    Macos,
    Linux,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppArchitecture {
    X86_64,
    Aarch64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstalledBundle {
    Nsis,
    Msi,
    AppImage,
    Deb,
    Rpm,
    MacosApp,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateSource {
    pub provider: UpdateProvider,
    pub latest_url: String,
    pub catalog_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateRequestContext {
    pub current_version: String,
    pub product_name: String,
    pub identifier: String,
    pub platform: AppPlatform,
    pub architecture: AppArchitecture,
    pub installed_bundle: InstalledBundle,
    pub portable_mode: bool,
    pub proxy: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedCheckRequest {
    pub source: UpdateSource,
    pub context: UpdateRequestContext,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedUpdateMetadata {
    pub provider: UpdateProvider,
    pub version: String,
    pub published_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignedCheckOutcome {
    NoUpdate,
    Update(SignedUpdateMetadata),
}

/// Why an update source produced no usable answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// No answer arrived: the connection failed or timed out. The same host is
    /// not asked again within one operation.
    Unreachable(String),
    /// The source answered, but the answer could not be used.
    Rejected(String),
}

impl SourceError {
    pub fn message(&self) -> &str {
        match self {
            Self::Unreachable(message) | Self::Rejected(message) => message,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedPrepareRequest {
    pub source: UpdateSource,
    pub context: UpdateRequestContext,
    pub expected_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManualUpdateReason {
    InstalledBundleUnsupported,
    SignedUpdateUnavailable,
    PreparationFailed,
    InstallationFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogFetchRequest {
    pub source: UpdateSource,
    pub proxy: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualPackage {
    pub package_id: String,
    pub provider: UpdateProvider,
    pub kind: String,
    pub file_name: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedUpdateOffer {
    pub offer_id: String,
    pub current_version: String,
    pub version: String,
    pub provider: UpdateProvider,
    pub published_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualUpdateOffer {
    pub offer_id: String,
    pub current_version: String,
    pub version: String,
    pub provider: UpdateProvider,
    pub recommended_package_id: String,
    pub packages: Vec<ManualPackage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AppUpdateCheckResult {
    PortableModeUnsupported {
        current_version: String,
    },
    UpToDate {
        activity_id: String,
        current_version: String,
    },
    ReadyToInstall {
        activity_id: String,
        offer: SignedUpdateOffer,
    },
    ManualDownloadRequired {
        activity_id: String,
        offer: ManualUpdateOffer,
        reason: ManualUpdateReason,
    },
    Unavailable {
        activity_id: String,
        current_version: String,
        error: String,
    },
    AlreadyRunning {
        activity_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AppUpdateInstallResult {
    Installed {
        activity_id: String,
        version: String,
    },
    ManualDownloadRequired {
        activity_id: String,
        offer: ManualUpdateOffer,
        reason: ManualUpdateReason,
    },
    Unavailable {
        activity_id: String,
        error: String,
    },
    AlreadyRunning {
        activity_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateOperationPhase {
    CheckingSignedSources,
    CheckingCatalogSources,
    ReadyToInstall,
    Downloading,
    Installing,
    /// A new process confirmed that the installed package version matches the
    /// exact target recorded before updater handoff.
    Installed,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "availability", rename_all = "camelCase")]
pub enum AppUpdateAvailabilitySnapshot {
    None,
    ReadyToInstall {
        offer: SignedUpdateOffer,
    },
    ManualDownloadRequired {
        offer: ManualUpdateOffer,
        reason: ManualUpdateReason,
    },
    Installing {
        version: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateOperationStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAttemptDiagnostic {
    pub provider: UpdateProvider,
    pub stage: &'static str,
    pub classification: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppUpdateOperationSnapshot {
    pub activity_id: String,
    pub phase: AppUpdateOperationPhase,
    pub status: AppUpdateOperationStatus,
    pub created_at: u128,
    pub updated_at: u128,
    pub completed_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub completed_bytes: u64,
    pub total_bytes: Option<u64>,
}
