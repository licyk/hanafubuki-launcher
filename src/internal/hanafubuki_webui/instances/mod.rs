//! Durable WebUI instance creation and installation lifecycle.
//!
//! Generic instance configuration remains product-neutral. This module owns the
//! selected WebUI spec and durable Created/Installing/Ready/InstallFailed/Deleting
//! state.

mod directory;
mod identity;
mod lifecycle;
mod maintenance_journal;
mod models;
mod paths;
mod repository;
mod service;

pub use directory::delete_configured_directory;
pub use maintenance_journal::InstanceMaintenanceJournal;
pub use models::{
    CreateWebUiInstanceFromSnapshotRequest, CreateWebUiInstanceRequest, DeleteInstanceRequest,
    ImportWebUiInstanceRequest, InstanceMaintenanceIntent, InstanceMaintenanceJournalEntry,
    InstanceMaintenanceJournalPhase, InstanceMaintenanceJournalState, InstanceMaintenanceKind,
    InstanceMaintenanceOperation, InstanceMaintenancePhase, InstanceMigrationCheckpoint,
    InstanceMigrationDirectoryIdentity, InstanceMigrationPhase, InstanceMigrationPlan,
    InstanceMigrationPreservedComponent, InstanceMigrationPreview, InstanceMigrationRequest,
    InstanceMigrationTransferMode, InstanceSpec, InstanceTypeAvailability,
    ReinstallWebUiInstanceRequest, RenameWebUiInstanceRequest,
    UpdateWebUiInstanceComponentPathsRequest, UpdateWebUiInstanceQuickPathsRequest,
    WebUiInstanceInstallAction, WebUiInstanceInstallActions, WebUiInstanceInstallSource,
    WebUiInstanceLifecycle, WebUiInstanceRecord, WebUiInstanceSnapshot, maintenance_activity_id,
};
pub use paths::{
    WebUiCoreDirectoryState, WebUiInstancePaths, resolve_webui_instance_paths,
    runtime_core_directory_state,
};
pub use repository::WebUiInstanceRepository;
pub use service::{
    InstallCompletionSink, InstancePythonValidator, PrepareDiscoveredCoreRequest,
    PrepareDiscoveredWebUiRequest, PreparedDiscoveredCore, PreparedDiscoveredWebUi, PreparedImport,
    WebUiInstanceService,
};

use lifecycle::validate_transition;

#[cfg(test)]
use crate::internal::hanafubuki_config::repository::instances::LaunchArgumentOrigin;
#[cfg(test)]
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
#[cfg(test)]
use crate::internal::hanafubuki_webui::install::{
    InstanceInstallCleanupStatus, InstanceInstallLogOutput, InstanceInstallPhase,
    InstanceInstallService, WebUiInstallSpec, default_launch_arguments,
};
#[cfg(test)]
use identity::NEXT_INSTANCE_ID;
#[cfg(test)]
use service::MAX_SNAPSHOT_BYTES;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::Ordering;

#[cfg(test)]
mod tests;
