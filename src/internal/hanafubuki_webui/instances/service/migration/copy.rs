use super::{COPY_BUFFER_SIZE, check_cancelled};
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use std::fs;
use std::path::Path;

/// Copy one directory tree exactly once.
///
/// The previous migration implementation built and hashed a complete source
/// manifest before copying, then hashed both trees repeatedly. Besides turning
/// a copy into six full reads, that delayed all measurable progress until the
/// first source scan completed. This walker starts copying immediately and
/// relies on create-new destinations, sync-on-close, operation proof and the
/// retained source tree for recovery.
pub(super) fn copy_tree(
    source: &Path,
    destination: &Path,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    copy_directory(source, destination, cancellation)
}

fn copy_directory(
    source: &Path,
    destination: &Path,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    check_cancelled(cancellation)?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("read migration directory {}: {error}", source.display()))?
    {
        check_cancelled(cancellation)?;
        let entry = entry.map_err(|error| format!("read migration directory entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path).map_err(|error| {
            format!("inspect migration entry {}: {error}", source_path.display())
        })?;
        validate_migration_entry(&source_path, &metadata)?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            fs::create_dir(&destination_path).map_err(|error| {
                format!(
                    "create migration directory {}: {error}",
                    destination_path.display()
                )
            })?;
            copy_directory(&source_path, &destination_path, cancellation)?;
        } else if file_type.is_file() {
            copy_file(&source_path, &destination_path, cancellation)?;
        } else if file_type.is_symlink() {
            let target = fs::read_link(&source_path).map_err(|error| {
                format!("read migration symlink {}: {error}", source_path.display())
            })?;
            crate::internal::hanafubuki_fs_transfer::copy_symlink(
                &source_path,
                &target,
                &destination_path,
            )?;
        } else {
            return Err(format!(
                "unsupported special file in instance migration: {}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_migration_entry(path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    crate::internal::hanafubuki_fs_transfer::reject_unsupported_reparse(path, metadata)?;
    let file_type = metadata.file_type();
    if file_type.is_dir() || file_type.is_file() || file_type.is_symlink() {
        Ok(())
    } else {
        Err(format!(
            "unsupported special file in instance migration: {}",
            path.display()
        ))
    }
}

pub(super) fn copy_file(
    source: &Path,
    destination: &Path,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    crate::internal::hanafubuki_fs_transfer::copy_file_create_new(
        source,
        destination,
        COPY_BUFFER_SIZE,
        || check_cancelled(cancellation),
        |_| Ok(()),
    )
}
