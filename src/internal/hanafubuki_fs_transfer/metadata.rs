use std::fs;
use std::io;
use std::path::Path;
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::Duration;
use std::time::SystemTime;

/// Applies source access/modification times and permissions to a copied path.
///
/// The order is intentional: a copied Git object may be read-only on Windows,
/// and `SetFileTime` fails with access denied after that attribute is applied.
/// Any read-only destination left by an interrupted attempt is made writable,
/// timestamps are written, and the exact source permissions are restored last.
pub fn preserve_path_metadata(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::metadata(source).map_err(|error| {
        format!(
            "inspect transfer source metadata {}: {error}",
            source.display()
        )
    })?;
    let source_permissions = metadata.permissions();
    let (accessed, modified) = times(&metadata)?;

    make_destination_writable(destination)?;
    retry_metadata_io(|| {
        fs::set_times(
            destination,
            fs::FileTimes::new()
                .set_accessed(accessed)
                .set_modified(modified),
        )
    })
    .map_err(|error| {
        format!(
            "set transfer destination timestamps {}: {error}",
            destination.display()
        )
    })?;
    retry_metadata_io(|| fs::set_permissions(destination, source_permissions.clone())).map_err(
        |error| {
            format!(
                "set transfer destination permissions {}: {error}",
                destination.display()
            )
        },
    )
}

/// Preserves access and modification times on a copied symlink without
/// following its target.
pub fn preserve_symlink_times(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "inspect transfer symlink metadata {}: {error}",
            source.display()
        )
    })?;
    let (accessed, modified) = times(&metadata)?;
    retry_metadata_io(|| {
        fs::set_times_nofollow(
            destination,
            fs::FileTimes::new()
                .set_accessed(accessed)
                .set_modified(modified),
        )
    })
    .map_err(|error| {
        format!(
            "set transfer destination symlink timestamps {}: {error}",
            destination.display()
        )
    })
}

/// A source's access and modification times. A filesystem that reports neither keeps the
/// destination's own, which is what copying without metadata preservation leaves.
fn times(metadata: &fs::Metadata) -> Result<(SystemTime, SystemTime), String> {
    let accessed = metadata
        .accessed()
        .map_err(|error| format!("read transfer source access time: {error}"))?;
    let modified = metadata
        .modified()
        .map_err(|error| format!("read transfer source modification time: {error}"))?;
    Ok((accessed, modified))
}

/// Removes a caller-authorized directory tree after making Windows read-only
/// entries deletable. This function does not decide whether a path may be
/// deleted; migration proof and path policy must be checked by the caller.
pub fn remove_dir_all(path: &Path) -> Result<(), String> {
    prepare_tree_for_removal(path)?;
    retry_metadata_io(|| match fs::remove_dir_all(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    })
    .map_err(|error| format!("remove transfer directory {}: {error}", path.display()))
}

#[cfg(windows)]
fn prepare_tree_for_removal(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect transfer removal path {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if let Err(error) =
        crate::internal::hanafubuki_fs_transfer::reject_unsupported_reparse(path, &metadata)
    {
        return Err(format!(
            "refusing to traverse transfer removal path: {error}"
        ));
    }
    if !metadata.is_dir() {
        return make_destination_writable(path);
    }

    let children = fs::read_dir(path)
        .map_err(|error| {
            format!(
                "read transfer removal directory {}: {error}",
                path.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            format!(
                "read transfer removal directory entry {}: {error}",
                path.display()
            )
        })?;
    for child in children {
        prepare_tree_for_removal(&child.path())?;
    }
    make_destination_writable(path)
}

#[cfg(not(windows))]
fn prepare_tree_for_removal(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn make_destination_writable(destination: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(destination)
        .map_err(|error| {
            format!(
                "inspect transfer destination metadata {}: {error}",
                destination.display()
            )
        })?
        .permissions();
    if !permissions.readonly() {
        return Ok(());
    }
    permissions.set_readonly(false);
    retry_metadata_io(|| fs::set_permissions(destination, permissions.clone())).map_err(|error| {
        format!(
            "make transfer destination writable {}: {error}",
            destination.display()
        )
    })
}

#[cfg(not(windows))]
fn make_destination_writable(_destination: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn retry_metadata_io(mut operation: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    const RETRY_DELAYS_MS: [u64; 5] = [15, 30, 60, 120, 240];
    let mut retries = RETRY_DELAYS_MS.into_iter();
    loop {
        match operation() {
            Ok(()) => return Ok(()),
            Err(error) if is_retryable_windows_metadata_error(&error) => {
                let Some(delay_ms) = retries.next() else {
                    return Err(error);
                };
                thread::sleep(Duration::from_millis(delay_ms));
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(windows)]
fn is_retryable_windows_metadata_error(error: &io::Error) -> bool {
    // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION and ERROR_LOCK_VIOLATION.
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(not(windows))]
fn retry_metadata_io(mut operation: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    operation()
}
