use super::*;

impl WebUiInstanceService {
    pub fn commit_migration(&self, plan: &InstanceMigrationPlan) -> Result<(), String> {
        let current = self.get(&plan.instance_id)?;
        let committed_revision = plan
            .expected_config_revision
            .checked_add(1)
            .ok_or_else(|| "instance config revision exhausted".to_string())?;
        if current.install.instance_identity == plan.expected_instance_identity
            && current.instance.config_revision == committed_revision
            && current.install_dir == plan.destination_install_dir
            && current.instance.component_paths.as_ref() == Some(&plan.destination_component_paths)
        {
            return Ok(());
        }
        self.repository.commit_migration(plan)
    }

    pub fn finish_migration_files(
        &self,
        operation_id: &str,
        plan: &InstanceMigrationPlan,
        cancellation: &CancellationToken,
    ) -> Result<(), String> {
        let checkpoint = migration_checkpoint(&self.maintenance.get_entry(operation_id)?)?;
        let (mode, source_identity, transfer_root_identity) = transfer_proof(&checkpoint)?;
        let destination_identity = directory_identity(&plan.destination_install_dir)?;
        require_directory_identity(
            "migration destination before cleanup",
            transfer_root_identity,
            &destination_identity,
        )?;
        if mode == InstanceMigrationTransferMode::CrossFilesystemCopy {
            check_cancelled(cancellation)?;
            if plan.source_install_dir.exists() {
                InstancePathPolicy::revalidate_migration_source(
                    &plan.source_install_dir,
                    &plan.source_component_paths,
                )?;
                let actual_source_identity = directory_identity(&plan.source_install_dir)?;
                require_directory_identity(
                    "migration source before cleanup",
                    source_identity,
                    &actual_source_identity,
                )?;
                verify_source_directory(&plan.source_install_dir)?;
                crate::internal::hanafubuki_fs_transfer::remove_dir_all(&plan.source_install_dir)?;
            }
        }
        Ok(())
    }

    pub fn rollback_migration_files(
        &self,
        operation_id: &str,
        plan: &InstanceMigrationPlan,
    ) -> Result<(), String> {
        let checkpoint = migration_checkpoint(&self.maintenance.get_entry(operation_id)?)?;
        let (mode, source_identity, transfer_root_identity) = transfer_proof(&checkpoint)?;
        let destination_identity = directory_identity(&plan.destination_install_dir)?;
        require_directory_identity(
            "migration destination before rollback",
            transfer_root_identity,
            &destination_identity,
        )?;
        match mode {
            InstanceMigrationTransferMode::SameFilesystemRename => {
                if plan.source_install_dir.exists() {
                    return Err("migration source reappeared before rollback".to_string());
                }
                move_no_replace(&plan.destination_install_dir, &plan.source_install_dir)
                    .map_err(|error| format!("rollback migration destination: {error}"))?;
                let source_identity = directory_identity(&plan.source_install_dir)?;
                require_directory_identity(
                    "rolled back migration source",
                    transfer_root_identity,
                    &source_identity,
                )
            }
            InstanceMigrationTransferMode::CrossFilesystemCopy => {
                verify_source_directory(&plan.source_install_dir)?;
                let actual_source_identity = directory_identity(&plan.source_install_dir)?;
                require_directory_identity(
                    "migration source before rollback",
                    source_identity,
                    &actual_source_identity,
                )?;
                crate::internal::hanafubuki_fs_transfer::remove_dir_all(
                    &plan.destination_install_dir,
                )
            }
            InstanceMigrationTransferMode::Pending => {
                Err("migration transfer mode was not committed".to_string())
            }
        }
    }

    pub fn complete_migration(
        &self,
        operation_id: &str,
    ) -> Result<InstanceMaintenanceOperation, String> {
        self.update_migration_phase(operation_id, InstanceMigrationPhase::Completed, false, None)?;
        self.maintenance.get_public(operation_id)
    }
}

fn transfer_proof(
    checkpoint: &InstanceMigrationCheckpoint,
) -> Result<
    (
        InstanceMigrationTransferMode,
        &InstanceMigrationDirectoryIdentity,
        &InstanceMigrationDirectoryIdentity,
    ),
    String,
> {
    if checkpoint.transfer_mode == InstanceMigrationTransferMode::Pending {
        return Err("migration transfer mode was not committed".to_string());
    }
    let source_identity = checkpoint
        .source_identity
        .as_ref()
        .ok_or_else(|| "migration source identity was not committed".to_string())?;
    let transfer_root_identity = checkpoint
        .transfer_root_identity
        .as_ref()
        .ok_or_else(|| "migration transfer root identity was not committed".to_string())?;
    Ok((
        checkpoint.transfer_mode,
        source_identity,
        transfer_root_identity,
    ))
}
