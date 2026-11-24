//! Safe recursive deletion for directories authorized by a domain path policy.

use std::path::Path;

use crate::internal::hanafubuki_foundation::paths::validate_not_dangerous;

/// Delete a configured directory without following filesystem links.
///
/// The caller is responsible for checking path scope, configuration revision,
/// cross-instance conflicts, and any required user authorization.
pub fn delete_configured_directory(path: &Path) -> Result<(), String> {
    validate_not_dangerous(path)?;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "inspect configured directory {}: {error}",
                path.display()
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "configured directory must not be a symbolic link: {}",
            path.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "configured path must be a directory: {}",
            path.display()
        ));
    }
    std::fs::remove_dir_all(path)
        .map_err(|error| format!("delete configured directory {}: {error}", path.display()))
}
