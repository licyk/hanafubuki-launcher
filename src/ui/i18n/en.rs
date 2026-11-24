use crate::workflow::{LauncherFailure, LauncherFileOperation, LauncherPhase};

use super::message::{Message, PhaseText, mib};

pub(super) fn message(message: Message<'_>) -> String {
    match message {
        Message::RetryingPortableSetup => "Retrying the portable setup".into(),
        Message::PreparationFailed => "Hanafubuki could not be prepared".into(),
        Message::Retry => "Retry".into(),
        Message::Exit => "Exit".into(),
        Message::DownloadingHanafubuki { version } => {
            format!("Downloading Hanafubuki {version}")
        }
        Message::UpdatingLauncher { version } => {
            format!("Updating Hanafubuki Launcher {version}")
        }
        Message::ScanProgress {
            files,
            directories,
            webuis,
            current_path,
        } => format!(
            "Scanned {files} files in {directories} directories; found {webuis} WebUIs\nCurrent: {current_path}"
        ),
        Message::DownloadProgress {
            completed,
            total: Some(total),
        } => format!("{:.1} MiB of {:.1} MiB", mib(completed), mib(total)),
        Message::DownloadProgress {
            completed,
            total: None,
        } => format!("{:.1} MiB received", mib(completed)),
        Message::FileProgress {
            operation,
            completed,
            total,
            current_path,
        } => file_progress(operation, completed, total, current_path),
    }
}

fn file_progress(
    operation: LauncherFileOperation,
    completed: u64,
    total: Option<u64>,
    current_path: &str,
) -> String {
    let action = match operation {
        LauncherFileOperation::ExtractingLauncher => "Extracting launcher files",
        LauncherFileOperation::UpdatingLauncher => "Updating launcher files",
        LauncherFileOperation::RemovingLauncher => "Removing launcher files",
        LauncherFileOperation::ExtractingApplication => "Extracting application files",
        LauncherFileOperation::UpdatingApplication => "Updating application files",
        LauncherFileOperation::RemovingApplication => "Removing application files",
    };
    match total {
        Some(total) => format!("{action}: {completed} of {total}\nCurrent: {current_path}"),
        None => format!("{action}: {completed}\nCurrent: {current_path}"),
    }
}

pub(super) fn phase(phase: LauncherPhase) -> PhaseText {
    let (title, detail) = match phase {
        #[cfg(windows)]
        LauncherPhase::CheckingLongPathSupport => (
            "Checking Windows long-path support",
            "Reading the system FileSystem configuration",
        ),
        LauncherPhase::CheckingSystemProxy => (
            "Checking system proxy",
            "Testing the configured proxy before network requests",
        ),
        #[cfg(windows)]
        LauncherPhase::CheckingVcRuntime => (
            "Checking Microsoft Visual C++ Runtime",
            "Reading the installed x64 runtime registration",
        ),
        #[cfg(windows)]
        LauncherPhase::DownloadingVcRuntime => (
            "Downloading Microsoft Visual C++ Runtime",
            "Receiving the x64 redistributable from Microsoft",
        ),
        #[cfg(windows)]
        LauncherPhase::InstallingVcRuntime => (
            "Installing Microsoft Visual C++ Runtime",
            "Waiting for the administrator-approved installer",
        ),
        LauncherPhase::CheckingLauncherUpdate => (
            "Checking Hanafubuki Launcher",
            "Contacting the launcher release service",
        ),
        LauncherPhase::PreparingLauncherUpdate => (
            "Preparing Hanafubuki Launcher update",
            "Creating a trusted launcher update workspace",
        ),
        LauncherPhase::InstallingLauncherUpdate => (
            "Installing Hanafubuki Launcher update",
            "Handing replacement to the trusted update helper",
        ),
        LauncherPhase::PreparingWorkspace => {
            ("Preparing Hanafubuki", "Creating the portable workspace")
        }
        LauncherPhase::ScanningWebUis => (
            "Scanning for existing WebUIs",
            "Inspecting the portable directory structure",
        ),
        LauncherPhase::PreparingPortablePaths => (
            "Preparing portable paths",
            "Synchronizing Hanafubuki storage locations",
        ),
        LauncherPhase::PreparingDiscoveredInstances => (
            "Preparing discovered instances",
            "Synchronizing portable instance records",
        ),
        LauncherPhase::CheckingRelease => {
            ("Checking for Hanafubuki", "Contacting the release service")
        }
        LauncherPhase::VerifyingInstalledFiles => (
            "Checking Hanafubuki",
            "Verifying the installed portable files",
        ),
        LauncherPhase::CleaningInstallation => (
            "Cleaning Hanafubuki",
            "Removing files that are not part of the portable application",
        ),
        LauncherPhase::AnalyzingTrustedCore => (
            "Analyzing Hanafubuki",
            "Building the trusted portable file manifest",
        ),
        LauncherPhase::UpdatingApplication => (
            "Updating Hanafubuki",
            "Reconciling the installed portable files",
        ),
        LauncherPhase::RepairingApplication => (
            "Repairing Hanafubuki",
            "Reconciling the installed portable files",
        ),
        LauncherPhase::InstallingApplication => (
            "Installing Hanafubuki",
            "Preparing the trusted portable application",
        ),
        LauncherPhase::StartingApplication => (
            "Starting Hanafubuki",
            "Handing control to the portable application",
        ),
        LauncherPhase::StartingOffline => (
            "Starting Hanafubuki offline",
            "The release service is unavailable; using the local portable application",
        ),
    };
    PhaseText { title, detail }
}

pub(super) fn failure(failure: &LauncherFailure) -> String {
    match failure {
        LauncherFailure::OfflineRepairRequired => "Hanafubuki could not start from the local core. Connect to the internet and restart the launcher to download or repair it.".into(),
        LauncherFailure::EventChannelUnavailable => {
            "The launcher event channel is unavailable.".into()
        }
        LauncherFailure::WorkerStoppedUnexpectedly => {
            "The launcher worker stopped unexpectedly.".into()
        }
        #[cfg(windows)]
        LauncherFailure::VcRuntimePrerequisite(detail) => format!(
            "Microsoft Visual C++ Redistributable (x64) could not be prepared.\n{detail}"
        ),
        LauncherFailure::Diagnostic(error) => error.clone(),
    }
}
