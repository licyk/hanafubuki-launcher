use super::*;

impl WebUiInstanceService {
    pub fn update_migration_phase(
        &self,
        operation_id: &str,
        phase: InstanceMigrationPhase,
        cleanup_obligation: bool,
        error: Option<String>,
    ) -> Result<InstanceMaintenanceJournalEntry, String> {
        self.maintenance
            .edit_migration(operation_id, error, |checkpoint| {
                checkpoint.phase = phase;
                checkpoint.cleanup_obligation = cleanup_obligation;
                if matches!(
                    phase,
                    InstanceMigrationPhase::RemovingSource | InstanceMigrationPhase::Completed
                ) {
                    checkpoint.config_committed = true;
                }
                Ok(())
            })
    }

    pub fn cancel_migration_recovery(
        &self,
        operation_id: &str,
        plan: &InstanceMigrationPlan,
    ) -> Result<InstanceMaintenanceOperation, String> {
        let entry = self.maintenance.get_entry(operation_id)?;
        let checkpoint = migration_checkpoint(&entry)?;
        if !checkpoint.can_cancel_without_transfer() {
            return Err(
                "instance migration can only be cancelled before any file transfer or config commit"
                    .to_string(),
            );
        }
        let InstanceMaintenanceIntent::Migration(stored_plan) = entry.intent else {
            return Err(format!(
                "maintenance operation {operation_id} is not a migration"
            ));
        };
        if &stored_plan != plan {
            return Err(
                "instance migration cancellation plan does not match the journal".to_string(),
            );
        }

        let snapshot = self.get(&plan.instance_id)?;
        if snapshot.install.instance_identity != plan.expected_instance_identity
            || snapshot.instance.config_revision != plan.expected_config_revision
            || snapshot.install_dir != plan.source_install_dir
            || snapshot.instance.component_paths.as_ref() != Some(&plan.source_component_paths)
        {
            return Err(
                "instance configuration no longer matches the pre-migration state; recovery cannot be cancelled"
                    .to_string(),
            );
        }

        InstancePathPolicy::revalidate_migration_source(
            &plan.source_install_dir,
            &plan.source_component_paths,
        )?;
        if let Some(expected) = checkpoint.source_identity.as_ref() {
            let actual = directory_identity(&plan.source_install_dir)?;
            require_directory_identity("migration source before cancellation", expected, &actual)?;
        }

        let staging = migration_staging_path(plan, operation_id)?;
        if staging.exists() {
            return Err(format!(
                "migration staging exists; recovery cannot be cancelled: {}",
                staging.display()
            ));
        }
        if plan.destination_install_dir.exists() {
            return Err(format!(
                "migration destination now exists; recovery cannot be cancelled: {}",
                plan.destination_install_dir.display()
            ));
        }

        self.update_migration_phase(operation_id, InstanceMigrationPhase::Cancelled, false, None)?;
        self.maintenance.get_public(operation_id)
    }
}

pub(super) fn migration_checkpoint(
    entry: &InstanceMaintenanceJournalEntry,
) -> Result<InstanceMigrationCheckpoint, String> {
    match &entry.state {
        InstanceMaintenanceJournalState::Migration { checkpoint } => Ok(checkpoint.clone()),
        _ => Err(format!(
            "maintenance operation {} is not a migration",
            entry.operation_id
        )),
    }
}

pub(super) fn mutate_migration_checkpoint(
    journal: &InstanceMaintenanceJournal,
    operation_id: &str,
    error: Option<String>,
    mutation: impl FnOnce(&mut InstanceMigrationCheckpoint) -> Result<(), String>,
) -> Result<(), String> {
    journal
        .edit_migration(operation_id, error, mutation)
        .map(|_| ())
}
