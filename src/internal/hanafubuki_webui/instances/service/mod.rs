mod component_paths;
mod create;
mod deletion;
mod discovered;
mod import;
mod install;
mod install_monitor;
mod maintenance;
mod migration;
mod query;
mod quick_paths;
mod rename;
mod snapshot;

use std::collections::HashSet;
use std::sync::Arc;

use crate::internal::hanafubuki_webui::install::InstanceInstallService;

use super::{InstanceMaintenanceJournal, InstanceMaintenancePhase, WebUiInstanceRepository};

#[derive(Clone)]
pub struct WebUiInstanceService {
    pub(super) repository: WebUiInstanceRepository,
    pub(super) installs: InstanceInstallService,
    pub(super) maintenance: InstanceMaintenanceJournal,
    pub(super) hidden_maintenance_operations: Arc<HashSet<String>>,
    pub(super) install_completions: Arc<dyn InstallCompletionSink>,
}

impl WebUiInstanceService {
    pub fn new(
        repository: WebUiInstanceRepository,
        installs: InstanceInstallService,
    ) -> Result<Self, String> {
        let maintenance = InstanceMaintenanceJournal::new(repository.maintenance_journal_path());
        let hidden_maintenance_operations = maintenance
            .list_public()?
            .into_iter()
            .filter(|operation| {
                matches!(
                    operation.phase,
                    InstanceMaintenancePhase::Completed
                        | InstanceMaintenancePhase::Failed
                        | InstanceMaintenancePhase::Cancelled
                )
            })
            .map(|operation| operation.operation_id)
            .collect();
        Ok(Self {
            repository,
            installs,
            maintenance,
            hidden_maintenance_operations: Arc::new(hidden_maintenance_operations),
            install_completions: Arc::new(NoInstallCompletions),
        })
    }

    /// The service, telling `sink` of every install run that completes.
    pub fn with_install_completions(mut self, sink: Arc<dyn InstallCompletionSink>) -> Self {
        self.install_completions = sink;
        self
    }
}

/// Told when an install run (an install, a reinstall or a type's update)
/// completed and its instance is ready again: what was recorded about the
/// installation before it may no longer hold. Called on the thread that
/// recorded the completion, which must not wait on it.
pub trait InstallCompletionSink: Send + Sync {
    fn completed(&self, instance_id: &str, instance_identity: &str);
}

struct NoInstallCompletions;

impl InstallCompletionSink for NoInstallCompletions {
    fn completed(&self, _instance_id: &str, _instance_identity: &str) {}
}

pub use discovered::{
    PrepareDiscoveredCoreRequest, PrepareDiscoveredWebUiRequest, PreparedDiscoveredCore,
    PreparedDiscoveredWebUi,
};
pub use import::{InstancePythonValidator, PreparedImport};
#[cfg(test)]
pub(in crate::internal::hanafubuki_webui::instances) use snapshot::MAX_SNAPSHOT_BYTES;
