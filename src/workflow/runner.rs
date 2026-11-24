use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_process::executor::ExecutorManager;

use crate::network::NetworkContext;
use crate::platform::{self, TargetSpec};
use crate::portable_app::package::PackageProgress;
use crate::portable_app::{FileProgress, handoff, manifest, package, release};
use crate::self_update::{self, SelfUpdateFileOperation, SelfUpdateOutcome, SelfUpdateProgress};
use crate::workspace::paths::{self, WorkspacePaths};
use crate::workspace::prepare::{self, WorkspaceProgress};

use super::{
    LauncherFailure, LauncherFileOperation, LauncherFileProgress, LauncherPhase, WorkerEvent,
};

pub fn spawn(executor: ExecutorManager) -> (Receiver<WorkerEvent>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        if let Err(error) = run(&executor, &sender) {
            match &error {
                LauncherFailure::Diagnostic(detail) => {
                    log::error!(target: "launcher::workflow", "preparation failed: {detail}");
                }
                #[cfg(windows)]
                LauncherFailure::VcRuntimePrerequisite(detail) => {
                    log::error!(
                        target: "launcher::workflow",
                        "VC Runtime prerequisite failed: {detail}"
                    );
                }
                other => {
                    log::error!(target: "launcher::workflow", "preparation failed: {other:?}");
                }
            }
            log::logger().flush();
            let _ = sender.send(WorkerEvent::Failed(error));
        }
    });
    (receiver, worker)
}

fn run(executor: &ExecutorManager, sender: &Sender<WorkerEvent>) -> Result<(), LauncherFailure> {
    #[cfg(windows)]
    phase(sender, LauncherPhase::CheckingLongPathSupport);
    if crate::prerequisites::prepare_long_paths(executor)?
        == crate::prerequisites::LongPathPreparation::Restarted
    {
        let _ = sender.send(WorkerEvent::Restarted);
        return Ok(());
    }

    let launcher_layout = platform::launcher_layout()?;
    paths::initialize_logging(&launcher_layout)?;
    phase(sender, LauncherPhase::CheckingSystemProxy);
    let network = NetworkContext::detect(executor);
    let target = platform::current_target()?;
    log::info!(
        target: "launcher::platform",
        "application target platform={:?} architecture={:?} package_kind={}",
        target.platform,
        target.architecture,
        target.package_kind
    );
    // Looked up alongside the launcher's own update check and workspace
    // preparation; a launcher restart simply abandons it.
    let release_lookup = release::ReleaseLookup::start(network.clone(), target.clone());
    #[cfg(windows)]
    crate::prerequisites::ensure_vc_runtime(&network, |progress| {
        use crate::prerequisites::VcRuntimeProgress;

        match progress {
            VcRuntimeProgress::Checking => phase(sender, LauncherPhase::CheckingVcRuntime),
            VcRuntimeProgress::Downloading => phase(sender, LauncherPhase::DownloadingVcRuntime),
            VcRuntimeProgress::Download { completed, total } => {
                let _ = sender.send(WorkerEvent::VcRuntimeDownload { completed, total });
            }
            VcRuntimeProgress::Installing => phase(sender, LauncherPhase::InstallingVcRuntime),
        }
    })
    .map_err(LauncherFailure::VcRuntimePrerequisite)?;
    log::info!(
        target: "launcher::platform",
        "platform preflight completed executable={} portable_root={} install_kind={:?}",
        launcher_layout.executable.native_text(),
        launcher_layout.portable_root.native_text(),
        launcher_layout.install_kind
    );

    phase(sender, LauncherPhase::CheckingLauncherUpdate);
    match self_update::check_and_apply(executor, &launcher_layout, &network, |progress| {
        match progress {
            SelfUpdateProgress::Preparing => phase(sender, LauncherPhase::PreparingLauncherUpdate),
            SelfUpdateProgress::Download {
                version,
                completed,
                total,
            } => {
                let _ = sender.send(WorkerEvent::LauncherDownload {
                    version: version.to_string(),
                    completed,
                    total,
                });
            }
            SelfUpdateProgress::File(progress) => {
                let operation = match progress.operation {
                    SelfUpdateFileOperation::Extracting => {
                        LauncherFileOperation::ExtractingLauncher
                    }
                    SelfUpdateFileOperation::Updating => LauncherFileOperation::UpdatingLauncher,
                    SelfUpdateFileOperation::Removing => LauncherFileOperation::RemovingLauncher,
                };
                let _ = sender.send(WorkerEvent::FileProgress(LauncherFileProgress {
                    operation,
                    completed: progress.completed,
                    total: progress.total,
                    current_path: progress.current_path,
                }));
            }
            SelfUpdateProgress::Installing => {
                phase(sender, LauncherPhase::InstallingLauncherUpdate)
            }
        }
    }) {
        SelfUpdateOutcome::Current => {
            log::info!(target: "launcher::self_update", "launcher is current; continuing startup");
        }
        SelfUpdateOutcome::Restarted => {
            log::info!(target: "launcher::self_update", "launcher update handed off; restarting");
            log::logger().flush();
            let _ = sender.send(WorkerEvent::Restarted);
            return Ok(());
        }
    }

    phase(sender, LauncherPhase::PreparingWorkspace);
    let paths = paths::prepare(&launcher_layout)?;
    prepare::execute(&paths, |progress| match progress {
        WorkspaceProgress::ScanningStarted => phase(sender, LauncherPhase::ScanningWebUis),
        WorkspaceProgress::Scan(progress) => {
            let _ = sender.send(WorkerEvent::Scan {
                files: progress.files,
                directories: progress.directories,
                webuis: progress.webuis,
                current_path: progress.current_path,
            });
        }
        WorkspaceProgress::PreparingPortablePaths => {
            phase(sender, LauncherPhase::PreparingPortablePaths);
        }
        WorkspaceProgress::PreparingDiscoveredInstances => {
            phase(sender, LauncherPhase::PreparingDiscoveredInstances);
        }
    })?;
    prepare_application(executor, sender, &paths, &network, target, release_lookup)
}

fn prepare_application(
    executor: &ExecutorManager,
    sender: &Sender<WorkerEvent>,
    paths: &WorkspacePaths,
    network: &NetworkContext,
    target: TargetSpec,
    release_lookup: release::ReleaseLookup,
) -> Result<(), LauncherFailure> {
    phase(sender, LauncherPhase::CheckingRelease);
    let release = match release_lookup.wait() {
        Ok(release) => release,
        Err(release::RemoteError::Offline(error)) => {
            log::warn!(target: "launcher::release", "release lookup is offline: {error}");
            return launch_offline(
                executor,
                sender,
                paths,
                &target,
                LauncherFailure::OfflineRepairRequired,
            );
        }
        Err(release::RemoteError::Fatal(error)) => {
            // A rejected catalog says nothing about the installed core, so keep
            // starting it; only a missing local application reports the error.
            log::warn!(target: "launcher::release", "release lookup was rejected: {error}");
            return launch_offline(executor, sender, paths, &target, error.into());
        }
    };
    log::info!(
        target: "launcher::core",
        "selected release version={} package_kind={} package_bytes={}",
        release.version,
        target.package_kind,
        release.artifact.size
    );

    let stored_manifest = manifest::load(&paths.manifest)?;
    let version_changed = stored_manifest
        .as_ref()
        .is_none_or(|stored| stored.version != release.version.to_string());
    log::info!(
        target: "launcher::core",
        "installed manifest version={} update_required={version_changed}",
        stored_manifest
            .as_ref()
            .map_or("missing", |stored| stored.version.as_str())
    );
    if !version_changed {
        phase(sender, LauncherPhase::VerifyingInstalledFiles);
        let stored = stored_manifest
            .as_ref()
            .expect("an unchanged version requires a manifest");
        let integrity = manifest::verify(&paths.core, stored)?;
        log::info!(target: "launcher::core", "installed core integrity={integrity:?}");
        if integrity == manifest::IntegrityStatus::ExtrasOnly {
            phase(sender, LauncherPhase::CleaningInstallation);
            manifest::remove_extras(&paths.core, stored, |progress| {
                send_file_progress(sender, LauncherFileOperation::RemovingApplication, progress);
            })?;
        }
        if integrity != manifest::IntegrityStatus::RepairRequired {
            platform::restore_executable_permissions(&paths.core, &target)?;
            log::info!(target: "launcher::core", "restored executable permissions");
            let executable = platform::installed_executable(&paths.core, &target)?
                .ok_or("The verified Hanafubuki executable is missing.")?;
            return launch(executor, sender, paths, executable);
        }
    }

    let staged_core =
        match package::prepare(
            network,
            paths,
            &target,
            &release,
            |progress| match progress {
                PackageProgress::Download(progress) => {
                    let _ = sender.send(WorkerEvent::Download {
                        version: progress.version,
                        completed: progress.completed,
                        total: progress.total,
                    });
                }
                PackageProgress::Materializing => {
                    phase(sender, LauncherPhase::InstallingApplication)
                }
                PackageProgress::File(progress) => {
                    send_file_progress(
                        sender,
                        LauncherFileOperation::ExtractingApplication,
                        progress,
                    );
                }
            },
        ) {
            Ok(staged_core) => staged_core,
            Err(release::RemoteError::Offline(error)) => {
                log::warn!(target: "launcher::download", "package download is offline: {error}");
                return launch_offline(
                    executor,
                    sender,
                    paths,
                    &target,
                    LauncherFailure::OfflineRepairRequired,
                );
            }
            Err(release::RemoteError::Fatal(error)) => return Err(error.into()),
        };

    phase(sender, LauncherPhase::AnalyzingTrustedCore);
    let trusted_manifest = manifest::scan(&release.version, &staged_core)?;
    if stored_manifest.as_ref() != Some(&trusted_manifest) {
        manifest::store(&trusted_manifest, &paths.manifest)?;
    }

    phase(
        sender,
        if version_changed {
            LauncherPhase::UpdatingApplication
        } else {
            LauncherPhase::RepairingApplication
        },
    );
    manifest::repair(&paths.core, &staged_core, &trusted_manifest, |progress| {
        send_file_progress(sender, LauncherFileOperation::UpdatingApplication, progress);
    })?;
    platform::restore_executable_permissions(&paths.core, &target)?;
    log::info!(target: "launcher::core", "restored executable permissions");
    package::clean(&paths.staging)?;

    let executable = platform::installed_executable(&paths.core, &target)?
        .ok_or("The repaired Hanafubuki executable is missing.")?;
    launch(executor, sender, paths, executable)
}

fn phase(sender: &Sender<WorkerEvent>, launcher_phase: LauncherPhase) {
    log::info!(target: "launcher::workflow", "phase={launcher_phase:?}");
    let _ = sender.send(WorkerEvent::Phase(launcher_phase));
}

fn send_file_progress(
    sender: &Sender<WorkerEvent>,
    operation: LauncherFileOperation,
    progress: FileProgress,
) {
    let _ = sender.send(WorkerEvent::FileProgress(LauncherFileProgress {
        operation,
        completed: progress.completed,
        total: progress.total,
        current_path: progress.current_path,
    }));
}

fn launch(
    executor: &ExecutorManager,
    sender: &Sender<WorkerEvent>,
    paths: &WorkspacePaths,
    executable: ResolvedPath,
) -> Result<(), LauncherFailure> {
    phase(sender, LauncherPhase::StartingApplication);
    handoff::start(executor, paths, &executable)?;
    let _ = sender.send(WorkerEvent::Launched);
    Ok(())
}

fn launch_offline(
    executor: &ExecutorManager,
    sender: &Sender<WorkerEvent>,
    paths: &WorkspacePaths,
    target: &TargetSpec,
    missing_local: LauncherFailure,
) -> Result<(), LauncherFailure> {
    log::warn!(target: "launcher::workflow", "release service unavailable; using offline launch");
    let Ok(Some(executable)) = platform::installed_executable(&paths.core, target) else {
        return Err(missing_local);
    };
    phase(sender, LauncherPhase::StartingOffline);
    handoff::start(executor, paths, &executable).map_err(|_| missing_local)?;
    let _ = sender.send(WorkerEvent::Launched);
    Ok(())
}
