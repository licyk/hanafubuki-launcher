#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LauncherPhase {
    #[cfg(windows)]
    CheckingLongPathSupport,
    CheckingSystemProxy,
    #[cfg(windows)]
    CheckingVcRuntime,
    #[cfg(windows)]
    DownloadingVcRuntime,
    #[cfg(windows)]
    InstallingVcRuntime,
    CheckingLauncherUpdate,
    PreparingLauncherUpdate,
    InstallingLauncherUpdate,
    PreparingWorkspace,
    ScanningWebUis,
    PreparingPortablePaths,
    PreparingDiscoveredInstances,
    CheckingRelease,
    VerifyingInstalledFiles,
    CleaningInstallation,
    AnalyzingTrustedCore,
    UpdatingApplication,
    RepairingApplication,
    InstallingApplication,
    StartingApplication,
    StartingOffline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LauncherFailure {
    OfflineRepairRequired,
    EventChannelUnavailable,
    WorkerStoppedUnexpectedly,
    #[cfg(windows)]
    VcRuntimePrerequisite(String),
    Diagnostic(String),
}

impl From<String> for LauncherFailure {
    fn from(error: String) -> Self {
        Self::Diagnostic(error)
    }
}

impl From<&str> for LauncherFailure {
    fn from(error: &str) -> Self {
        Self::Diagnostic(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Phase(LauncherPhase),
    FileProgress(LauncherFileProgress),
    Download {
        version: String,
        completed: u64,
        total: Option<u64>,
    },
    LauncherDownload {
        version: String,
        completed: u64,
        total: Option<u64>,
    },
    #[cfg(windows)]
    VcRuntimeDownload {
        completed: u64,
        total: Option<u64>,
    },
    Scan {
        files: u64,
        directories: u64,
        webuis: u64,
        current_path: String,
    },
    Failed(LauncherFailure),
    Launched,
    Restarted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LauncherFileOperation {
    ExtractingLauncher,
    UpdatingLauncher,
    RemovingLauncher,
    ExtractingApplication,
    UpdatingApplication,
    RemovingApplication,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LauncherFileProgress {
    pub operation: LauncherFileOperation,
    pub completed: u64,
    pub total: Option<u64>,
    pub current_path: String,
}
