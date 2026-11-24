//! Canonical derived paths for one installed WebUI instance.
//!
//! Generic instance infrastructure owns `instance_path` and `data`. WebUI
//! business consumers derive their standalone Python and core locations here
//! from the persisted component configuration.

use std::path::PathBuf;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;

use crate::internal::hanafubuki_webui::install::WebUiInstallSpec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebUiInstancePaths {
    pub instance_path: PathBuf,
    pub python_dir: PathBuf,
    pub core_path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebUiCoreDirectoryState {
    Materialized,
    Deferred,
}

/// Resolve the family-specific runtime contract for a configured Core path.
///
/// Only InvokeAI may defer a missing directory to first launch. Existing
/// non-directory paths and filesystem inspection failures remain errors for
/// every family.
pub fn runtime_core_directory_state(
    spec: &WebUiInstallSpec,
    core_path: &std::path::Path,
) -> Result<WebUiCoreDirectoryState, String> {
    match std::fs::metadata(core_path) {
        Ok(metadata) if metadata.is_dir() => Ok(WebUiCoreDirectoryState::Materialized),
        Ok(_) => Err(format!(
            "WebUI core path is not a directory: {}",
            core_path.display()
        )),
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && !spec.requires_preexisting_core_directory() =>
        {
            Ok(WebUiCoreDirectoryState::Deferred)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "WebUI core directory does not exist: {}",
            core_path.display()
        )),
        Err(error) => Err(format!(
            "inspect WebUI core directory {}: {error}",
            core_path.display()
        )),
    }
}

pub fn resolve_webui_instance_paths(
    instance_path: PathBuf,
    configured: Option<&InstanceComponentPaths>,
) -> Result<WebUiInstancePaths, String> {
    if instance_path.as_os_str().is_empty() || !instance_path.is_absolute() {
        return Err("resolved WebUI instance path must be non-empty and absolute".to_string());
    }
    let defaults =
        InstanceComponentPaths::defaults(&instance_path, InstanceComponentPaths::PYTHON_AND_CORE);
    let configured = configured.unwrap_or(&defaults);
    let python = configured.require(InstanceComponentPaths::PYTHON)?;
    for (label, path) in [("Python", python), ("core", configured.core())] {
        if path.as_os_str().is_empty() || !path.is_absolute() {
            return Err(format!(
                "resolved instance {label} path must be non-empty and absolute"
            ));
        }
    }
    Ok(WebUiInstancePaths {
        python_dir: python.to_path_buf(),
        core_path: configured.core().to_path_buf(),
        instance_path,
    })
}

#[cfg(test)]
mod tests;
