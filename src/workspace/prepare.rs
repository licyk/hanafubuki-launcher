use super::discovery::ScanProgress;
use super::paths::WorkspacePaths;
use super::{discovery, portable_config, settings};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceProgress {
    ScanningStarted,
    Scan(ScanProgress),
    PreparingPortablePaths,
    PreparingDiscoveredInstances,
}

pub fn execute(
    paths: &WorkspacePaths,
    mut report: impl FnMut(WorkspaceProgress),
) -> Result<(), String> {
    let mut launcher_settings = settings::load_or_create(&paths.settings)?;
    log::info!(
        target: "launcher::settings",
        "settings loaded scan_completed={} discovered_instances={}",
        launcher_settings.scan_completed,
        launcher_settings.discovered_instances.len()
    );
    if !launcher_settings.scan_completed {
        report(WorkspaceProgress::ScanningStarted);
        launcher_settings.discovered_instances =
            discovery::scan(&paths.launcher_directory, &paths.data, |progress| {
                log::debug!(
                    target: "launcher::discovery",
                    "scan progress files={} directories={} webuis={} current_path={}",
                    progress.files,
                    progress.directories,
                    progress.webuis,
                    progress.current_path
                );
                report(WorkspaceProgress::Scan(progress));
            })?;
        log::info!(
            target: "launcher::discovery",
            "scan completed discovered_instances={}",
            launcher_settings.discovered_instances.len()
        );
        for instance in &launcher_settings.discovered_instances {
            log::info!(
                target: "launcher::discovery",
                "webui discovered id={} family={:?} core_path={} python_path={}",
                instance.instance_id,
                instance.family,
                instance.core_path,
                instance.python_path.as_deref().unwrap_or("missing")
            );
        }
        launcher_settings.scan_completed = true;
        settings::store(&launcher_settings, &paths.settings)?;
        log::info!(target: "launcher::settings", "discovery results stored");
    }

    report(WorkspaceProgress::PreparingPortablePaths);
    let portable_configuration = portable_config::reconcile(&paths.data)?;
    log::info!(target: "launcher::config", "portable application paths reconciled");

    report(WorkspaceProgress::PreparingDiscoveredInstances);
    let instances_changed = discovery::reconcile(
        &mut launcher_settings,
        &paths.launcher_directory,
        &portable_configuration.app_paths,
        &portable_configuration.config_state,
    )?;
    log::info!(
        target: "launcher::discovery",
        "instance reconciliation completed retained_instances={} settings_changed={instances_changed}",
        launcher_settings.discovered_instances.len()
    );
    if instances_changed {
        settings::store(&launcher_settings, &paths.settings)?;
        log::info!(target: "launcher::settings", "reconciled discovery settings stored");
    }
    Ok(())
}
