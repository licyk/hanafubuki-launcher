use super::super::identity::now_millis;
use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;

use super::super::{
    WebUiInstanceInstallAction, WebUiInstanceInstallActions, WebUiInstanceInstallSource,
    WebUiInstanceLifecycle, WebUiInstanceSnapshot,
};
use super::WebUiInstanceService;
use super::install::SNAPSHOT_REINSTALL_SPEC_UNAVAILABLE;
use crate::internal::hanafubuki_instance_contracts::InstanceTypeDescriptor;
use crate::internal::hanafubuki_webui::instance_types::presets::PresetCatalog;
use crate::internal::hanafubuki_webui::instance_types::{InstanceLocation, InstanceTypeRegistry};

impl WebUiInstanceService {
    pub fn get(&self, instance_id: &str) -> Result<WebUiInstanceSnapshot, String> {
        let record = self.repository.load_record(instance_id)?;
        // Left exactly as it was read: nothing can act on an instance whose
        // type is unavailable, so it only shows its record and paths.
        if let Err(reason) = record.require_available() {
            return self
                .repository
                .snapshot(record, WebUiInstanceInstallActions::unavailable(reason));
        }
        let record = self.recover_interrupted_install(record)?;
        let can_install = matches!(
            record.lifecycle,
            WebUiInstanceLifecycle::Created | WebUiInstanceLifecycle::InstallFailed { .. }
        );
        // An install that has not finished can also be started over with
        // different parameters, through the same request as a reinstall.
        let can_reinstall = matches!(
            record.lifecycle,
            WebUiInstanceLifecycle::Created
                | WebUiInstanceLifecycle::Ready { .. }
                | WebUiInstanceLifecycle::InstallFailed { .. }
        );
        let unavailable_reinstall = WebUiInstanceInstallAction::Unavailable {
            reason: format!("reinstall is unavailable from {:?}", record.lifecycle),
        };
        let mut snapshot = self.repository.snapshot(
            record.clone(),
            WebUiInstanceInstallActions {
                install: WebUiInstanceInstallAction::Unavailable {
                    reason: format!("installation is unavailable from {:?}", record.lifecycle),
                },
                reinstall: unavailable_reinstall.clone(),
                force_recreate_python: unavailable_reinstall.clone(),
                force_recreate_core: unavailable_reinstall.clone(),
                reinstall_spec: unavailable_reinstall,
            },
        )?;
        if can_install || can_reinstall {
            let paths = snapshot
                .instance
                .component_paths
                .as_ref()
                .ok_or_else(|| "instance component paths are not configured".to_string())?;
            let action = match InstancePathPolicy::validate_installation_layout(
                &snapshot.install_dir,
                paths,
                false,
                false,
            ) {
                Ok(()) => WebUiInstanceInstallAction::Available,
                Err(reason) => WebUiInstanceInstallAction::Unavailable { reason },
            };
            // A layout the installer cannot consume at all answers for the
            // destructive switches too; otherwise each one is asked separately,
            // because only the deletion it would perform can refuse it.
            let force_recreate = |recreate_python: bool, recreate_core: bool| match &action {
                WebUiInstanceInstallAction::Unavailable { reason } => {
                    WebUiInstanceInstallAction::Unavailable {
                        reason: reason.clone(),
                    }
                }
                WebUiInstanceInstallAction::Available => {
                    match InstancePathPolicy::validate_reinstall(
                        &snapshot.install_dir,
                        paths,
                        recreate_python,
                        recreate_core,
                    ) {
                        Ok(()) => WebUiInstanceInstallAction::Available,
                        Err(reason) => WebUiInstanceInstallAction::Unavailable { reason },
                    }
                }
            };
            let force_recreate_python_action = force_recreate(true, false);
            let force_recreate_core_action = force_recreate(false, true);
            if can_install {
                snapshot.install_action = action.clone();
            }
            if can_reinstall {
                snapshot.reinstall_spec_action = match &record.install_source {
                    WebUiInstanceInstallSource::Snapshot { .. } => {
                        WebUiInstanceInstallAction::Unavailable {
                            reason: SNAPSHOT_REINSTALL_SPEC_UNAVAILABLE.to_string(),
                        }
                    }
                    WebUiInstanceInstallSource::Standard
                    | WebUiInstanceInstallSource::DiscoveredCore => action.clone(),
                };
                snapshot.reinstall_action = action;
                snapshot.force_recreate_python_action = force_recreate_python_action;
                snapshot.force_recreate_core_action = force_recreate_core_action;
            }
        }
        Ok(snapshot)
    }

    pub fn instance_exists(&self, instance_id: &str) -> bool {
        self.repository.record_or_config_exists(instance_id)
    }

    pub fn list(&self) -> Result<Vec<WebUiInstanceSnapshot>, String> {
        let mut instances = Vec::new();
        for id in self.repository.list_record_ids()? {
            match self.get(&id) {
                Ok(instance) => instances.push(instance),
                Err(error) => log::warn!(
                    "skipping unavailable WebUI instance during list instance={id} error={error}"
                ),
            }
        }
        Ok(instances)
    }

    /// List every valid durable instance record ID without dropping records
    /// whose complete snapshot cannot currently be resolved.
    ///
    /// Diagnostic collection uses this inventory so an unreadable instance is
    /// represented as an issue instead of silently disappearing from an
    /// "all instances" collection.
    pub fn list_ids(&self) -> Result<Vec<String>, String> {
        self.repository.list_record_ids()
    }

    /// Refuses, with the reason, an instance whose type is unavailable.
    pub fn require_available(&self, instance_id: &str) -> Result<(), String> {
        self.repository
            .load_record(instance_id)?
            .require_available()
    }

    /// The instance's type, refused for a type no provider is registered for.
    pub fn instance_type(&self, instance_id: &str) -> Result<InstanceTypeDescriptor, String> {
        let record = self.repository.load_record(instance_id)?;
        Ok(self
            .repository
            .types()
            .require(&record.type_id)?
            .descriptor()
            .clone())
    }

    /// The types an instance can be created as.
    pub fn instance_types(&self) -> &InstanceTypeRegistry {
        self.repository.types()
    }

    /// The presets a creation can start from.
    pub fn presets(&self) -> &PresetCatalog {
        self.repository.presets()
    }

    /// Where the instance is, as its type's placeholders resolve.
    pub fn location(&self, instance_id: &str) -> Result<InstanceLocation, String> {
        self.repository.location(instance_id)
    }

    pub fn instance_identity(&self, instance_id: &str) -> Result<String, String> {
        Ok(self.repository.load_record(instance_id)?.instance_identity)
    }

    pub fn persist_branch_preset_if_identity(
        &self,
        instance_id: &str,
        instance_identity: &str,
        preset_id: String,
    ) -> Result<(), String> {
        self.repository.persist_branch_preset_if_identity(
            instance_id,
            instance_identity,
            preset_id,
            now_millis(),
        )
    }

    pub fn delete_state_if_identity(
        &self,
        instance_id: &str,
        instance_identity: &str,
    ) -> Result<(), String> {
        self.repository
            .delete_state_if_identity(instance_id, instance_identity)
    }
}
