use super::*;

impl WebUiInstanceService {
    pub fn publish_migration_files(
        &self,
        operation_id: &str,
        plan: &InstanceMigrationPlan,
        cancellation: &CancellationToken,
    ) -> Result<(), String> {
        let staging = migration_staging_path(plan, operation_id)?;
        let checkpoint = migration_checkpoint(&self.maintenance.get_entry(operation_id)?)?;

        if plan.destination_install_dir.exists() {
            return resume_published_destination(
                &self.maintenance,
                operation_id,
                plan,
                &checkpoint,
            );
        }

        restart_interrupted_copy(&self.maintenance, operation_id, plan, &staging, &checkpoint)?;

        check_cancelled(cancellation)?;
        InstancePathPolicy::revalidate_migration_source(
            &plan.source_install_dir,
            &plan.source_component_paths,
        )?;
        verify_source_directory(&plan.source_install_dir)?;
        let source_identity = directory_identity(&plan.source_install_dir)?;
        mutate_migration_checkpoint(&self.maintenance, operation_id, None, |state| {
            if let Some(expected) = state.source_identity.as_ref() {
                require_directory_identity("migration source", expected, &source_identity)?;
            }
            state.source_identity = Some(source_identity.clone());
            state.phase = InstanceMigrationPhase::Transferring;
            state.cleanup_obligation = true;
            Ok(())
        })?;

        match attempt_no_replace_move(
            &plan.source_install_dir,
            &plan.destination_install_dir,
            move_no_replace,
        ) {
            Ok(InitialMoveOutcome::Moved) => {
                let destination_identity = directory_identity(&plan.destination_install_dir)?;
                require_directory_identity(
                    "same-filesystem migration destination",
                    &source_identity,
                    &destination_identity,
                )?;
                mutate_migration_checkpoint(&self.maintenance, operation_id, None, |state| {
                    state.transfer_mode = InstanceMigrationTransferMode::SameFilesystemRename;
                    state.transfer_root_identity = Some(destination_identity);
                    Ok(())
                })?;
                return Ok(());
            }
            Ok(InitialMoveOutcome::CrossDevice) => {}
            Err(error) => return Err(format!("move instance to destination: {error}")),
        }

        mutate_migration_checkpoint(&self.maintenance, operation_id, None, |state| {
            state.transfer_mode = InstanceMigrationTransferMode::CrossFilesystemCopy;
            Ok(())
        })?;

        fs::create_dir(&staging)
            .map_err(|error| format!("create migration staging {}: {error}", staging.display()))?;
        let staging_identity = directory_identity(&staging)?;
        mutate_migration_checkpoint(&self.maintenance, operation_id, None, |state| {
            state.transfer_root_identity = Some(staging_identity.clone());
            Ok(())
        })?;
        copy_tree(&plan.source_install_dir, &staging, cancellation)?;
        let actual_staging_identity = directory_identity(&staging)?;
        require_directory_identity(
            "migration staging before publish",
            &staging_identity,
            &actual_staging_identity,
        )?;
        move_no_replace(&staging, &plan.destination_install_dir)
            .map_err(|error| format!("publish migration destination: {error}"))?;
        let destination_identity = directory_identity(&plan.destination_install_dir)?;
        require_directory_identity(
            "published migration destination",
            &staging_identity,
            &destination_identity,
        )
    }
}

pub(super) fn restart_interrupted_copy(
    journal: &InstanceMaintenanceJournal,
    operation_id: &str,
    plan: &InstanceMigrationPlan,
    staging: &Path,
    checkpoint: &InstanceMigrationCheckpoint,
) -> Result<(), String> {
    if !staging.exists() {
        return Ok(());
    }
    if checkpoint.transfer_mode != InstanceMigrationTransferMode::CrossFilesystemCopy {
        return Err("migration staging exists without cross-filesystem state".to_string());
    }
    if !plan.source_install_dir.exists() {
        return Err("cross-filesystem migration source disappeared beside staging".to_string());
    }
    if let Some(expected) = checkpoint.transfer_root_identity.as_ref() {
        let actual = directory_identity(staging)?;
        require_directory_identity("migration staging", expected, &actual)?;
        crate::internal::hanafubuki_fs_transfer::remove_dir_all(staging)?;
    } else {
        // Without a durable identity, this can only be the mkdir/checkpoint
        // crash window. Remove only an unchanged empty directory; a non-empty
        // or replaced directory is retained because remove_dir is not recursive.
        fs::remove_dir(staging).map_err(|error| {
            format!(
                "remove empty interrupted migration staging {}: {error}",
                staging.display()
            )
        })?;
    }
    mutate_migration_checkpoint(journal, operation_id, None, |state| {
        state.transfer_root_identity = None;
        Ok(())
    })
}

fn resume_published_destination(
    journal: &InstanceMaintenanceJournal,
    operation_id: &str,
    plan: &InstanceMigrationPlan,
    checkpoint: &InstanceMigrationCheckpoint,
) -> Result<(), String> {
    let mode = match checkpoint.transfer_mode {
        InstanceMigrationTransferMode::Pending if !plan.source_install_dir.exists() => {
            InstanceMigrationTransferMode::SameFilesystemRename
        }
        InstanceMigrationTransferMode::Pending => {
            return Err("published migration destination has no durable transfer mode".to_string());
        }
        mode => mode,
    };
    match mode {
        InstanceMigrationTransferMode::SameFilesystemRename if plan.source_install_dir.exists() => {
            return Err("migration source reappeared after same-filesystem rename".to_string());
        }
        InstanceMigrationTransferMode::CrossFilesystemCopy
            if !plan.source_install_dir.exists() && !checkpoint.config_committed =>
        {
            return Err("cross-filesystem migration source disappeared before commit".to_string());
        }
        _ => {}
    }

    let destination_identity = directory_identity(&plan.destination_install_dir)?;
    let expected = checkpoint
        .transfer_root_identity
        .as_ref()
        .or_else(|| {
            (mode == InstanceMigrationTransferMode::SameFilesystemRename)
                .then_some(checkpoint.source_identity.as_ref())
                .flatten()
        })
        .ok_or_else(|| "published migration destination has no durable identity".to_string())?;
    require_directory_identity(
        "published migration destination",
        expected,
        &destination_identity,
    )?;
    if checkpoint.transfer_mode != mode || checkpoint.transfer_root_identity.is_none() {
        mutate_migration_checkpoint(journal, operation_id, None, |state| {
            state.transfer_mode = mode;
            state.transfer_root_identity = Some(destination_identity);
            Ok(())
        })?;
    }
    Ok(())
}
