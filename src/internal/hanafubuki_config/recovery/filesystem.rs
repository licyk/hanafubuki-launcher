use std::fs;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_config::AppPaths;

#[cfg(unix)]
pub(super) fn restrict_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("restrict recovery file permissions: {error}"))
}

#[cfg(not(unix))]
pub(super) fn restrict_file(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
pub(super) fn restrict_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("restrict recovery directory permissions: {error}"))
}

#[cfg(not(unix))]
pub(super) fn restrict_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(super) fn sibling_staging(source: &Path, recovery_id: &str) -> PathBuf {
    let mut staging = source.as_os_str().to_owned();
    staging.push(format!(".{recovery_id}.recovery.tmp"));
    PathBuf::from(staging)
}

pub(super) fn relative_display(paths: &AppPaths, path: &Path) -> String {
    crate::internal::hanafubuki_foundation::paths::relative_below(path, &paths.app_data_dir)
        .as_deref()
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(super) fn quarantine_target(
    paths: &AppPaths,
    source: &Path,
    recovery_id: &str,
) -> Result<PathBuf, String> {
    let relative =
        crate::internal::hanafubuki_foundation::paths::relative_below(source, &paths.app_data_dir)
            .ok_or_else(|| "configuration source is outside app data".to_string())?;
    let mut target = paths.config_quarantine_dir.join(recovery_id).join(relative);
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "invalid configuration file name".to_string())?;
    target.set_file_name(format!("{file_name}.invalid"));
    let parent = target
        .parent()
        .ok_or_else(|| "invalid quarantine target".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("create quarantine directory: {error}"))?;
    restrict_directory(&paths.config_quarantine_dir)?;
    restrict_directory(parent)?;
    if target.exists() {
        return Err("quarantine target already exists".to_string());
    }
    Ok(target)
}
