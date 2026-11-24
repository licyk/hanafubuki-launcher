use std::fs;
use std::path::Path;

/// Rejects Windows reparse points that are not ordinary file/directory
/// symlinks. Traversing junctions or other unknown reparse objects would break
/// the caller's source-root boundary.
#[cfg(windows)]
pub fn reject_unsupported_reparse(path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        && !metadata.file_type().is_symlink()
    {
        return Err(format!(
            "unsupported transfer reparse point {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn reject_unsupported_reparse(_path: &Path, _metadata: &fs::Metadata) -> Result<(), String> {
    Ok(())
}

/// Recreates a symlink object after the caller has validated its target text
/// against the owning domain's path policy.
#[cfg(unix)]
pub fn copy_symlink(_source: &Path, target: &Path, destination: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, destination).map_err(|error| {
        format!(
            "create transfer destination symlink {}: {error}",
            destination.display()
        )
    })
}

#[cfg(windows)]
pub fn copy_symlink(source: &Path, target: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::fs::FileTypeExt;

    let file_type = fs::symlink_metadata(source)
        .map_err(|error| {
            format!(
                "inspect transfer source symlink {}: {error}",
                source.display()
            )
        })?
        .file_type();
    let result = if file_type.is_symlink_dir() {
        std::os::windows::fs::symlink_dir(target, destination)
    } else if file_type.is_symlink_file() {
        std::os::windows::fs::symlink_file(target, destination)
    } else {
        return Err(format!(
            "unsupported transfer source reparse point {}",
            source.display()
        ));
    };
    result.map_err(|error| {
        format!(
            "create transfer destination symlink {}: {error}",
            destination.display()
        )
    })
}
