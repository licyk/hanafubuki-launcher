use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;
use crate::internal::hanafubuki_config::repository::instances::{
    InstanceComponentPaths, InstanceConfig, LaunchArgumentOrigin, create_instance_config_inner,
    get_instance_config_inner, update_instance_location_inner,
};
use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_config::repository::settings::get_global_settings_inner;
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_webui::discovery::{
    WebUiDiscoveryCandidate, WebUiDiscoveryProgress, scan_webui_roots,
};
use crate::internal::hanafubuki_webui::install::{WebUiInstallSpec, default_launch_arguments};
use crate::internal::hanafubuki_webui::instance_types::BuiltinProvider;
use crate::internal::hanafubuki_webui::instances::{
    InstanceSpec, WebUiInstanceInstallSource, WebUiInstanceLifecycle, WebUiInstanceRecord,
};
use serde::Deserialize;
use serde_json::json;

use crate::workspace::settings::{DiscoveredInstance, LauncherSettings};

pub type ScanProgress = WebUiDiscoveryProgress;

pub fn scan(
    launcher_directory: &ResolvedPath,
    app_data_directory: &ResolvedPath,
    mut report: impl FnMut(ScanProgress),
) -> Result<Vec<DiscoveredInstance>, String> {
    log::info!(
        target: "launcher::discovery",
        "WebUI discovery scan started root={}",
        launcher_directory.native_text()
    );
    // The launcher runs no extension, so it knows no type's discovery
    // signatures: only the built-in layouts are found.
    let scan = scan_webui_roots(
        std::slice::from_ref(launcher_directory),
        &[],
        &CancellationToken::default(),
        |mut progress| {
            progress.current_path =
                relative_resolved_text(launcher_directory, Path::new(&progress.current_path))
                    .unwrap_or(progress.current_path);
            report(progress);
        },
    );
    if let Some(failure) = scan.root_failures.first() {
        return Err(format!(
            "Scan directory {}: {}",
            failure.path, failure.message
        ));
    }
    for warning in &scan.warnings {
        log::warn!(
            target: "launcher::discovery",
            "scan warning path={} error={}",
            warning.path,
            warning.message
        );
    }
    let app_paths = AppPaths::from_app_data_dir(app_data_directory.native().to_path_buf())?;
    let mut reserved_ids = existing_instance_ids(&app_paths.instance_records_dir)?;
    let mut discovered = Vec::with_capacity(scan.candidates.len());
    let mut next_id = 1_u64;
    for candidate in scan.candidates {
        let WebUiDiscoveryCandidate::Builtin(candidate) = candidate else {
            continue;
        };
        let instance_id = loop {
            let candidate = format!("launcher-import-{next_id}");
            next_id += 1;
            if reserved_ids.insert(candidate.clone()) {
                break candidate;
            }
        };
        discovered.push(DiscoveredInstance {
            instance_id,
            instance_identity: new_instance_identity()?,
            name: candidate.family.product_name().to_string(),
            family: candidate.family,
            instance_path: candidate
                .instance_path
                .as_ref()
                .map(|path| relative_resolved_text(launcher_directory, path.native()))
                .transpose()?,
            core_path: relative_resolved_text(launcher_directory, candidate.core_path.native())?,
            python_path: candidate
                .python_dir
                .as_ref()
                .map(|path| relative_resolved_text(launcher_directory, path.native()))
                .transpose()?,
        });
    }
    Ok(discovered)
}

pub fn reconcile(
    settings: &mut LauncherSettings,
    launcher_directory: &ResolvedPath,
    app_paths: &AppPaths,
    config: &ConfigState,
) -> Result<bool, String> {
    log::info!(
        target: "launcher::discovery",
        "instance reconciliation started candidates={}",
        settings.discovered_instances.len()
    );
    fs::create_dir_all(&app_paths.instance_records_dir).map_err(|error| {
        format!(
            "Create discovered instance records directory {}: {error}",
            app_paths.instance_records_dir.display()
        )
    })?;

    let before = settings.discovered_instances.len();
    let mut retained = Vec::with_capacity(before);
    for discovered in settings.discovered_instances.drain(..) {
        let core = resolve_relative(launcher_directory, &discovered.core_path)?;
        match fs::symlink_metadata(core.system()) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                log::debug!(
                    target: "launcher::discovery",
                    "retaining discovered instance id={} core_path={}",
                    discovered.instance_id,
                    discovered.core_path
                );
                retained.push(discovered)
            }
            Ok(_) => {
                log::info!(
                    target: "launcher::discovery",
                    "pruning discovered instance with invalid core type id={} core_path={}",
                    discovered.instance_id,
                    discovered.core_path
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                log::info!(
                    target: "launcher::discovery",
                    "pruning discovered instance with missing core id={} core_path={}",
                    discovered.instance_id,
                    discovered.core_path
                );
            }
            Err(error) => {
                return Err(format!(
                    "Inspect discovered WebUI {}: {error}",
                    core.native_text()
                ));
            }
        }
    }
    settings.discovered_instances = retained;
    let changed = settings.discovered_instances.len() != before;

    for discovered in &settings.discovered_instances {
        log::info!(
            target: "launcher::discovery",
            "reconciling instance record id={} family={:?}",
            discovered.instance_id,
            discovered.family
        );
        reconcile_instance(discovered, launcher_directory, app_paths, config)?;
        log::info!(
            target: "launcher::discovery",
            "instance record reconciled id={}",
            discovered.instance_id
        );
    }
    Ok(changed)
}

fn reconcile_instance(
    discovered: &DiscoveredInstance,
    launcher_directory: &ResolvedPath,
    app_paths: &AppPaths,
    config_state: &ConfigState,
) -> Result<(), String> {
    let core = resolve_relative(launcher_directory, &discovered.core_path)?;
    let install_dir = match (&discovered.instance_path, &discovered.python_path) {
        (Some(path), Some(_)) => resolve_relative(launcher_directory, path)?
            .native()
            .to_path_buf(),
        (None, None) => get_global_settings_inner(app_paths, config_state)?
            .default_webui_install_dir
            .join(&discovered.instance_id),
        _ => {
            return Err(format!(
                "Discovered instance {} has inconsistent Python and instance paths",
                discovered.instance_id
            ));
        }
    };
    let python_path = match &discovered.python_path {
        Some(path) => resolve_relative(launcher_directory, path)?
            .native()
            .to_path_buf(),
        None => install_dir.join("python"),
    };

    let core_path = core.native().to_path_buf();
    let record_dir = app_paths.instance_record_dir(&discovered.instance_id);
    fs::create_dir_all(&record_dir)
        .map_err(|error| format!("Create instance record {}: {error}", record_dir.display()))?;

    let lifecycle_path = app_paths.instance_lifecycle_file(&discovered.instance_id);
    let created_lifecycle = ensure_lifecycle(discovered, &lifecycle_path)?;
    let config_result = reconcile_config(
        discovered,
        app_paths,
        config_state,
        install_dir,
        python_path,
        core_path,
    );
    if let Err(error) = config_result {
        if created_lifecycle {
            let _ = fs::remove_file(&lifecycle_path);
        }
        return Err(error);
    }
    Ok(())
}

fn reconcile_config(
    discovered: &DiscoveredInstance,
    app_paths: &AppPaths,
    config_state: &ConfigState,
    install_dir: PathBuf,
    python_path: PathBuf,
    core_path: PathBuf,
) -> Result<(), String> {
    let config_path = app_paths.instance_config_file(&discovered.instance_id);
    if config_path.exists() {
        let existing = get_instance_config_inner(app_paths, config_state, &discovered.instance_id)?;
        let matches = existing.install_dir.as_ref().is_some_and(|path| {
            InstancePathPolicy::paths_refer_to_same_location(path, &install_dir)
        }) && existing.component_paths.as_ref().is_some_and(|paths| {
            paths.python().is_some_and(|python| {
                InstancePathPolicy::paths_refer_to_same_location(python, &python_path)
            }) && InstancePathPolicy::paths_refer_to_same_location(paths.core(), &core_path)
        });
        if matches {
            return Ok(());
        }
        let component_paths = component_paths(python_path, core_path);
        let expected_install_dir = existing.install_dir.as_ref().ok_or_else(|| {
            format!(
                "Discovered instance {} does not have an explicit install directory",
                discovered.instance_id
            )
        })?;
        update_instance_location_inner(
            app_paths,
            config_state,
            &discovered.instance_id,
            existing.config_revision,
            expected_install_dir,
            install_dir,
            component_paths,
        )?;
        return Ok(());
    }

    let spec = WebUiInstallSpec::default_for(discovered.family);
    let complete = discovered.python_path.is_some();
    let launch_args = if complete {
        default_launch_arguments(&spec)
    } else {
        Vec::new()
    };
    let reserved_install_dir = if complete {
        false
    } else {
        let parent = install_dir
            .parent()
            .ok_or("Discovered managed instance root does not have a parent")?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Create discovered instance parent {}: {error}",
                parent.display()
            )
        })?;
        fs::create_dir(&install_dir).map_err(|error| {
            format!(
                "Reserve discovered instance root {}: {error}",
                install_dir.display()
            )
        })?;
        true
    };
    let result = create_instance_config_inner(
        app_paths,
        config_state,
        InstanceConfig {
            id: discovered.instance_id.clone(),
            name: discovered.name.clone(),
            config_revision: 1,
            install_dir: Some(install_dir.clone()),
            component_paths: Some(component_paths(python_path, core_path)),
            quick_paths: Vec::new(),
            env_vars: Default::default(),
            launch_args,
            launch_argument_origin: if complete {
                LaunchArgumentOrigin::BuiltIn
            } else {
                LaunchArgumentOrigin::PendingDefault
            },
            settings_overrides: json!({}),
        },
    );
    if let Err(error) = result {
        if reserved_install_dir {
            let cleanup = fs::remove_dir(&install_dir);
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}; remove reserved discovered instance root {}: {cleanup_error}",
                    install_dir.display()
                )),
            };
        }
        return Err(error);
    }
    Ok(())
}

fn component_paths(python_path: PathBuf, core_path: PathBuf) -> InstanceComponentPaths {
    InstanceComponentPaths::python_and_core(python_path, core_path)
}

/// What the launcher reads of a record the application may have written
/// since: only who it is, so any shape of the rest is left to the
/// application's reader.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordIdentity {
    instance_id: String,
    instance_identity: String,
}

fn ensure_lifecycle(discovered: &DiscoveredInstance, path: &Path) -> Result<bool, String> {
    if let Some(existing) = safe_file::read_json::<RecordIdentity>(path)? {
        if existing.instance_id != discovered.instance_id
            || existing.instance_identity != discovered.instance_identity
        {
            return Err(format!(
                "Discovered instance lifecycle identity conflicts with {}",
                discovered.instance_id
            ));
        }
        return Ok(false);
    }

    let now = now_millis();
    let complete = discovered.python_path.is_some();
    let lifecycle = if complete {
        WebUiInstanceLifecycle::Ready {
            operation_id: format!("launcher-import-{now}"),
        }
    } else {
        WebUiInstanceLifecycle::Created
    };
    safe_file::write_json_atomic(
        path,
        &WebUiInstanceRecord {
            instance_id: discovered.instance_id.clone(),
            instance_identity: discovered.instance_identity.clone(),
            type_id: BuiltinProvider::type_id(discovered.family),
            spec: InstanceSpec::Builtin(WebUiInstallSpec::default_for(discovered.family)),
            install_source: if complete {
                WebUiInstanceInstallSource::Standard
            } else {
                WebUiInstanceInstallSource::DiscoveredCore
            },
            lifecycle,
            created_at: now,
            updated_at: now,
            preset_id: None,
            preset_launch_arguments: Vec::new(),
        },
    )?;
    Ok(true)
}

fn resolve_relative(root: &ResolvedPath, relative: &str) -> Result<ResolvedPath, String> {
    if relative == "." {
        return Ok(root.clone());
    }
    if relative.is_empty() || relative.starts_with('/') || relative.contains('\\') {
        return Err("Discovered instance contains an invalid relative path".to_string());
    }
    let mut current = root.clone();
    for component in relative.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err("Discovered instance contains an unsafe relative path".to_string());
        }
        current = ResolvedPath::resolve_child(&current, OsStr::new(component))
            .map_err(|error| error.to_string())?;
    }
    Ok(current)
}

fn relative_resolved_text(root: &ResolvedPath, path: &Path) -> Result<String, String> {
    let path_key = crate::internal::hanafubuki_foundation::paths::PathKey::of(path);
    if !path_key.starts_with(root.key()) {
        return Err(format!(
            "Discovered path {} is outside launcher root {}",
            path.display(),
            root.native_text()
        ));
    }
    let components = path
        .components()
        .skip(root.key().len())
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .ok_or_else(|| "Discovered path is not valid UTF-8".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if components.is_empty() {
        ".".to_string()
    } else {
        components.join("/")
    })
}

fn existing_instance_ids(directory: &Path) -> Result<BTreeSet<String>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(format!("Read existing instance records: {error}")),
    };
    let mut ids = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("Read instance record entry: {error}"))?;
        if entry
            .file_type()
            .map_err(|error| format!("Inspect instance record entry: {error}"))?
            .is_dir()
        {
            if let Some(id) = entry.file_name().to_str() {
                ids.insert(id.to_string());
            }
        }
    }
    Ok(ids)
}

fn new_instance_identity() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("Generate instance identity: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
