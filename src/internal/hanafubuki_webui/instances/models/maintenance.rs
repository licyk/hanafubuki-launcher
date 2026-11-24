use serde::{Deserialize, Serialize};

use super::{
    InstanceMigrationCheckpoint, InstanceMigrationPlan, ReinstallWebUiInstanceRequest,
    UpdateWebUiInstanceComponentPathsRequest,
};

pub fn maintenance_activity_id(operation_id: &str) -> String {
    format!("maintenance:{}:{operation_id}", operation_id.len())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteInstanceRequest {
    pub instance_id: String,
    pub expected_instance_identity: String,
    pub expected_config_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceMaintenanceKind {
    ComponentPathUpdate,
    Reinstall,
    Delete,
    DeleteRecord,
    Migration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceMaintenancePhase {
    Stopping,
    UpdatingComponentPaths,
    StartingReinstall,
    DeletingInstance,
    RemovingRecord,
    MovingFiles,
    SwitchingConfig,
    RemovingSource,
    RecoveryRequired,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMaintenanceOperation {
    pub operation_id: String,
    pub activity_id: String,
    pub instance_id: String,
    pub kind: InstanceMaintenanceKind,
    pub phase: InstanceMaintenancePhase,
    pub created_at: u128,
    pub updated_at: u128,
    pub error: Option<String>,
    pub cancellable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InstanceMaintenanceIntent {
    ComponentPathUpdate(UpdateWebUiInstanceComponentPathsRequest),
    Reinstall(ReinstallWebUiInstanceRequest),
    Delete(DeleteInstanceRequest),
    DeleteRecord(DeleteInstanceRequest),
    Migration(InstanceMigrationPlan),
}

impl InstanceMaintenanceIntent {
    pub fn instance_id(&self) -> &str {
        match self {
            Self::ComponentPathUpdate(request) => &request.instance_id,
            Self::Reinstall(request) => &request.instance_id,
            Self::Delete(request) => &request.instance_id,
            Self::DeleteRecord(request) => &request.instance_id,
            Self::Migration(plan) => &plan.instance_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InstanceMaintenanceJournalState {
    Standard {
        phase: InstanceMaintenanceJournalPhase,
        cleanup_obligation: bool,
    },
    Migration {
        checkpoint: InstanceMigrationCheckpoint,
    },
}

impl InstanceMaintenanceJournalState {
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Standard { phase, .. } => phase.is_terminal(),
            Self::Migration { checkpoint } => checkpoint.phase.is_terminal(),
        }
    }

    pub fn cleanup_obligation(&self) -> bool {
        match self {
            Self::Standard {
                cleanup_obligation, ..
            } => *cleanup_obligation,
            Self::Migration { checkpoint } => checkpoint.cleanup_obligation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceMaintenanceJournalPhase {
    Reserved,
    Quiescing,
    Committing,
    CleanupRequired,
    Completed,
    Failed,
}

impl InstanceMaintenanceJournalPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMaintenanceJournalEntry {
    pub operation_id: String,
    pub intent: InstanceMaintenanceIntent,
    pub state: InstanceMaintenanceJournalState,
    pub created_at: u128,
    pub updated_at: u128,
    pub last_error: Option<String>,
}
