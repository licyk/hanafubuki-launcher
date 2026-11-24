use super::super::{
    DeleteInstanceRequest, InstanceMaintenanceIntent, InstanceMaintenanceJournalEntry,
    InstanceMaintenanceJournalPhase, InstanceMaintenanceOperation, WebUiInstanceLifecycle,
};
use super::WebUiInstanceService;

impl WebUiInstanceService {
    pub fn begin_instance_deletion(
        &self,
        request: &DeleteInstanceRequest,
    ) -> Result<InstanceMaintenanceOperation, String> {
        self.validate_delete_request(request)?
            .install
            .require_available()?;
        self.begin_deletion(request, InstanceMaintenanceIntent::Delete(request.clone()))
    }

    pub fn begin_instance_record_deletion(
        &self,
        request: &DeleteInstanceRequest,
    ) -> Result<InstanceMaintenanceOperation, String> {
        self.begin_deletion(
            request,
            InstanceMaintenanceIntent::DeleteRecord(request.clone()),
        )
    }

    fn begin_deletion(
        &self,
        request: &DeleteInstanceRequest,
        intent: InstanceMaintenanceIntent,
    ) -> Result<InstanceMaintenanceOperation, String> {
        self.validate_delete_request(request)?;
        let entry = self.maintenance.reserve(intent)?;
        if let Err(error) = self.transition(&request.instance_id, WebUiInstanceLifecycle::Deleting)
        {
            self.maintenance.update(
                &entry.operation_id,
                InstanceMaintenanceJournalPhase::Failed,
                false,
                Some(error.clone()),
            )?;
            return Err(error);
        }
        self.maintenance.get_public(&entry.operation_id)
    }

    pub fn reserve_lifecycle_maintenance(
        &self,
        intent: InstanceMaintenanceIntent,
    ) -> Result<InstanceMaintenanceJournalEntry, String> {
        self.maintenance.reserve(intent)
    }

    pub fn update_lifecycle_maintenance(
        &self,
        operation_id: &str,
        phase: InstanceMaintenanceJournalPhase,
        cleanup_obligation: bool,
        error: Option<String>,
    ) -> Result<InstanceMaintenanceJournalEntry, String> {
        self.maintenance
            .update(operation_id, phase, cleanup_obligation, error)
    }

    pub fn pending_lifecycle_maintenance(
        &self,
    ) -> Result<Vec<InstanceMaintenanceJournalEntry>, String> {
        self.maintenance.pending()
    }

    pub fn list_maintenance_operations(&self) -> Vec<InstanceMaintenanceOperation> {
        self.maintenance
            .list_public()
            .map(|operations| {
                operations
                    .into_iter()
                    .filter(|operation| {
                        !self
                            .hidden_maintenance_operations
                            .contains(&operation.operation_id)
                    })
                    .collect()
            })
            .unwrap_or_else(|error| {
                log::error!("read instance maintenance journal: {error}");
                Vec::new()
            })
    }

    pub fn get_maintenance_operation(
        &self,
        operation_id: &str,
    ) -> Result<InstanceMaintenanceOperation, String> {
        if self.hidden_maintenance_operations.contains(operation_id) {
            return Err(format!(
                "maintenance activity is not retained in this application session: {operation_id}"
            ));
        }
        self.get_durable_maintenance_operation(operation_id)
    }

    pub fn get_durable_maintenance_operation(
        &self,
        operation_id: &str,
    ) -> Result<InstanceMaintenanceOperation, String> {
        self.maintenance.get_public(operation_id)
    }
}
