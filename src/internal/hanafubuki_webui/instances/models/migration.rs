use std::path::PathBuf;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InstanceMigrationRequest {
    pub instance_id: String,
    pub expected_instance_identity: String,
    pub expected_config_revision: u64,
    pub destination_parent_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMigrationPreservedComponent {
    pub component: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMigrationPreview {
    pub instance_id: String,
    pub source_install_dir: PathBuf,
    pub destination_parent_dir: PathBuf,
    pub destination_install_dir: PathBuf,
    pub source_component_paths: InstanceComponentPaths,
    pub destination_component_paths: InstanceComponentPaths,
    pub preserved_components: Vec<InstanceMigrationPreservedComponent>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMigrationPlan {
    pub instance_id: String,
    pub expected_instance_identity: String,
    pub expected_config_revision: u64,
    pub source_install_dir: PathBuf,
    pub destination_install_dir: PathBuf,
    pub source_component_paths: InstanceComponentPaths,
    pub destination_component_paths: InstanceComponentPaths,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceMigrationPhase {
    Reserved,
    Quiescing,
    Transferring,
    SwitchingConfig,
    RemovingSource,
    RecoveryRequired,
    Completed,
    Failed,
    Cancelled,
}

impl InstanceMigrationPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceMigrationTransferMode {
    Pending,
    SameFilesystemRename,
    CrossFilesystemCopy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMigrationDirectoryIdentity {
    pub storage_id: u64,
    pub file_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMigrationCheckpoint {
    pub phase: InstanceMigrationPhase,
    pub transfer_mode: InstanceMigrationTransferMode,
    pub source_identity: Option<InstanceMigrationDirectoryIdentity>,
    pub transfer_root_identity: Option<InstanceMigrationDirectoryIdentity>,
    pub config_committed: bool,
    pub cleanup_obligation: bool,
}

impl InstanceMigrationCheckpoint {
    pub fn can_cancel_without_transfer(&self) -> bool {
        self.phase == InstanceMigrationPhase::RecoveryRequired
            && self.transfer_mode == InstanceMigrationTransferMode::Pending
            && self.transfer_root_identity.is_none()
            && !self.config_committed
    }
}
