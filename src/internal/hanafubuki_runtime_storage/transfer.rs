use std::fs;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_config::{
    BootstrapPaths, ManagedRuntimeLayoutKind, ManagedRuntimePaths,
};
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_fs_transfer::{
    InitialMoveOutcome, attempt_no_replace_move, move_no_replace,
};

use crate::internal::hanafubuki_runtime_storage::model::{MigrationPhase, RuntimeStorageJournal};
use crate::internal::hanafubuki_runtime_storage::repository::write_journal;

pub(super) fn execute_transfer(
    bootstrap: &BootstrapPaths,
    journal: &mut RuntimeStorageJournal,
) -> Result<(), String> {
    if journal.destination_root.exists() {
        return if matches!(
            journal.phase,
            MigrationPhase::Transferring | MigrationPhase::CommittingConfig
        ) {
            Ok(())
        } else {
            Err(format!(
                "managed runtime destination appeared before transfer: {}",
                journal.destination_root.display()
            ))
        };
    }

    journal.phase = MigrationPhase::Transferring;
    journal.last_error = None;
    write_journal(bootstrap, journal)?;

    let source = journal.source_paths()?;
    if source.layout() == ManagedRuntimeLayoutKind::Unified {
        if !source.root().exists() {
            return Err(format!(
                "managed runtime source disappeared: {}",
                source.root().display()
            ));
        }
        match attempt_no_replace_move(source.root(), &journal.destination_root, move_no_replace) {
            Ok(InitialMoveOutcome::Moved) => return Ok(()),
            Ok(InitialMoveOutcome::CrossDevice) => {}
            Err(error) => return Err(format!("move managed runtime root: {error}")),
        }
    }

    let staging_root = staging_path(journal)?;
    prepare_staging(&staging_root)?;
    copy_components(&source, &staging_root)?;
    publish_staging(journal, &staging_root)
}

pub(super) fn rollback_published_destination(
    journal: &RuntimeStorageJournal,
) -> Result<(), String> {
    let source = journal.source_paths()?;
    if source.layout() == ManagedRuntimeLayoutKind::Unified && !source.root().exists() {
        move_no_replace(&journal.destination_root, source.root())
            .map_err(|error| format!("restore managed runtime source: {error}"))?;
        return Ok(());
    }
    crate::internal::hanafubuki_fs_transfer::remove_dir_all(&journal.destination_root)
}

fn prepare_staging(staging_root: &Path) -> Result<(), String> {
    if let Some(parent) = staging_root.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create staging parent {}: {error}", parent.display()))?;
    }
    if staging_root.exists() {
        crate::internal::hanafubuki_fs_transfer::remove_dir_all(staging_root)?;
    }
    fs::create_dir(staging_root)
        .map_err(|error| format!("create staging {}: {error}", staging_root.display()))
}

fn copy_components(source: &ManagedRuntimePaths, staging_root: &Path) -> Result<(), String> {
    let staging = ManagedRuntimePaths::under(staging_root.to_path_buf())?;
    if source.layout() == ManagedRuntimeLayoutKind::Unified {
        for child in fs::read_dir(source.root())
            .map_err(|error| format!("read directory {}: {error}", source.root().display()))?
        {
            let child = child.map_err(|error| {
                format!("read directory entry {}: {error}", source.root().display())
            })?;
            copy_tree(
                source.root(),
                &child.path(),
                &staging_root.join(child.file_name()),
            )?;
        }
        return Ok(());
    }

    for (from, to) in [
        (source.python_dir(), staging.python_dir()),
        (
            source.micromamba_runtime_dir(),
            staging.micromamba_runtime_dir(),
        ),
        (
            source.micromamba_root_prefix(),
            staging.micromamba_root_prefix(),
        ),
        (source.launcher_code_dir(), staging.launcher_code_dir()),
    ] {
        if from.exists() {
            copy_tree(from, from, to)?;
        }
    }
    Ok(())
}

fn copy_tree(component_root: &Path, source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("inspect {}: {error}", source.display()))?;
    crate::internal::hanafubuki_fs_transfer::reject_unsupported_reparse(source, &metadata)?;
    let kind = metadata.file_type();
    if kind.is_symlink() {
        copy_symlink(component_root, source, destination)?;
        return crate::internal::hanafubuki_fs_transfer::preserve_symlink_times(
            source,
            destination,
        );
    }
    if kind.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        return crate::internal::hanafubuki_fs_transfer::copy_file_create_new(
            source,
            destination,
            4 * 1024 * 1024,
            || Ok(()),
            |_| Ok(()),
        );
    }
    if !kind.is_dir() {
        return Err(format!(
            "unsupported special filesystem object {}",
            source.display()
        ));
    }
    fs::create_dir(destination)
        .map_err(|error| format!("create directory {}: {error}", destination.display()))?;
    for child in fs::read_dir(source)
        .map_err(|error| format!("read directory {}: {error}", source.display()))?
    {
        let child =
            child.map_err(|error| format!("read directory entry {}: {error}", source.display()))?;
        copy_tree(
            component_root,
            &child.path(),
            &destination.join(child.file_name()),
        )?;
    }
    crate::internal::hanafubuki_fs_transfer::preserve_path_metadata(source, destination)
}

fn copy_symlink(component_root: &Path, source: &Path, destination: &Path) -> Result<(), String> {
    let target = fs::read_link(source)
        .map_err(|error| format!("read symlink {}: {error}", source.display()))?;
    validate_symlink_target(component_root, source, &target)?;
    crate::internal::hanafubuki_fs_transfer::copy_symlink(source, &target, destination)
}

fn validate_symlink_target(
    component_root: &Path,
    source: &Path,
    target: &Path,
) -> Result<(), String> {
    if target.is_absolute() {
        return Err(format!(
            "symlink {} targets an absolute path outside managed storage",
            source.display()
        ));
    }
    let parent = source
        .parent()
        .ok_or_else(|| format!("symlink {} has no parent", source.display()))?;
    let resolved =
        crate::internal::hanafubuki_foundation::paths::normalize_path(parent.join(target))?;
    let root = crate::internal::hanafubuki_foundation::paths::normalize_path(
        component_root.to_path_buf(),
    )?;
    if !resolved.starts_with(&root) {
        return Err(format!(
            "symlink {} escapes managed component root {}",
            source.display(),
            root.display()
        ));
    }
    Ok(())
}

fn publish_staging(journal: &RuntimeStorageJournal, staging_root: &Path) -> Result<(), String> {
    move_no_replace(staging_root, &journal.destination_root).map_err(|error| {
        format!(
            "publish staging {} -> {}: {error}",
            staging_root.display(),
            journal.destination_root.display()
        )
    })
}

pub(super) fn cleanup_sources(
    journal: &RuntimeStorageJournal,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let source = journal.source_paths()?;
    if source.layout() == ManagedRuntimeLayoutKind::Unified {
        if source.root().exists() {
            if cancellation.is_cancelled() {
                return Err("managed runtime cleanup cancelled".to_string());
            }
            crate::internal::hanafubuki_fs_transfer::remove_dir_all(source.root())?;
        }
        return Ok(());
    }

    let mut failures = Vec::new();
    for path in [
        source.python_dir(),
        source.micromamba_runtime_dir(),
        source.micromamba_root_prefix(),
        source.launcher_code_dir(),
    ] {
        if cancellation.is_cancelled() {
            return Err("managed runtime cleanup cancelled".to_string());
        }
        if path.exists() {
            if let Err(error) = crate::internal::hanafubuki_fs_transfer::remove_dir_all(path) {
                failures.push(error);
            }
        }
    }
    let legacy_runtime = source.root().join("runtime");
    let _ = fs::remove_dir(&legacy_runtime);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

pub(super) fn staging_path(journal: &RuntimeStorageJournal) -> Result<PathBuf, String> {
    let parent = journal
        .destination_root
        .parent()
        .ok_or_else(|| "managed runtime destination has no parent".to_string())?;
    let name = journal
        .destination_root
        .file_name()
        .ok_or_else(|| "managed runtime destination has no name".to_string())?
        .to_string_lossy();
    Ok(parent.join(format!(
        ".{name}.hanafubuki-staging-{}",
        journal.operation_id
    )))
}
