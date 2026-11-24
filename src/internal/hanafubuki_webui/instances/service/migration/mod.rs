use std::fs;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_fs_transfer::{
    InitialMoveOutcome, attempt_no_replace_move, move_no_replace,
};

use super::super::{
    InstanceMaintenanceIntent, InstanceMaintenanceJournal, InstanceMaintenanceJournalEntry,
    InstanceMaintenanceJournalState, InstanceMaintenanceOperation, InstanceMigrationCheckpoint,
    InstanceMigrationDirectoryIdentity, InstanceMigrationPhase, InstanceMigrationPlan,
    InstanceMigrationTransferMode,
};
use super::WebUiInstanceService;

const COPY_BUFFER_SIZE: usize = 4 * 1024 * 1024;

mod checkpoint;
mod copy;
mod identity;
mod lifecycle;
mod preview;
mod transfer;

use checkpoint::{migration_checkpoint, mutate_migration_checkpoint};
use copy::copy_tree;
use identity::{directory_identity, require_directory_identity};

pub(super) fn migration_staging_path(
    plan: &InstanceMigrationPlan,
    operation_id: &str,
) -> Result<PathBuf, String> {
    let parent = plan
        .destination_install_dir
        .parent()
        .ok_or_else(|| "migration destination has no parent".to_string())?;
    let name = plan
        .destination_install_dir
        .file_name()
        .ok_or_else(|| "migration destination has no directory name".to_string())?;
    Ok(parent.join(format!(
        ".{}.hanafubuki-migrate-{operation_id}",
        name.to_string_lossy()
    )))
}

pub(super) fn verify_source_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect migration source {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        Err(format!(
            "migration source is not a real directory: {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

pub(super) fn check_cancelled(cancellation: &CancellationToken) -> Result<(), String> {
    if cancellation.is_cancelled() {
        Err("application shutdown interrupted instance migration".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
