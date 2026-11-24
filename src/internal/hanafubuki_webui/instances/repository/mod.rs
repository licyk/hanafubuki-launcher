//! Durable storage boundary for WebUI instance lifecycle records.
//!
//! This repository owns the lifecycle write lock and the cross-file consistency
//! rules between WebUI lifecycle metadata and the generic instance config. It
//! does not own installation or maintenance orchestration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use serde_json::Value;

use record::decode_record;

use crate::internal::hanafubuki_python_runtime::resolve_existing_python_executable;

mod record;

use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use crate::internal::hanafubuki_config::policy::instance_path::{
    InstancePathPolicy, ValidatedInstanceMigrationPaths,
};
use crate::internal::hanafubuki_config::repository::instances::{
    InstanceComponentPaths, InstanceConfig, create_instance_config_inner,
    get_instance_config_inner, get_resolved_instance_config_inner, list_instance_configs_inner,
    synchronize_builtin_instance_launch_args_inner, update_instance_component_paths_inner,
    update_instance_location_inner, update_instance_name_inner, update_instance_quick_paths_inner,
    validate_instance_id,
};
use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_config::repository::settings::get_global_settings_inner;
use crate::internal::hanafubuki_foundation::persistence::quarantine_safe_json;
use crate::internal::hanafubuki_webui::instance_types::presets::PresetCatalog;
use crate::internal::hanafubuki_webui::instance_types::{InstanceLocation, InstanceTypeRegistry};
use crate::internal::hanafubuki_webui::instances::{
    InstanceMigrationPlan, InstanceSpec, RenameWebUiInstanceRequest,
    UpdateWebUiInstanceComponentPathsRequest, UpdateWebUiInstanceQuickPathsRequest,
    WebUiInstanceInstallActions, WebUiInstanceInstallSource, WebUiInstanceLifecycle,
    WebUiInstanceRecord, WebUiInstanceSnapshot,
};

#[derive(Clone)]
pub struct WebUiInstanceRepository {
    paths: AppPaths,
    config: Arc<ConfigState>,
    types: Arc<InstanceTypeRegistry>,
    presets: Arc<PresetCatalog>,
    /// Serializes record and configuration file transitions. It guards no
    /// memory, and each section leaves its files as an abort would, so it
    /// does not poison: an Agent tool that panicked inside one must not lock
    /// the interface out of every instance.
    lifecycle_lock: Arc<Mutex<()>>,
}

pub(crate) enum LifecycleTransitionPolicy {
    Standard,
    ReadyReinstall,
}

impl WebUiInstanceRepository {
    pub fn new(
        paths: AppPaths,
        config: Arc<ConfigState>,
        types: Arc<InstanceTypeRegistry>,
        presets: Arc<PresetCatalog>,
    ) -> Self {
        Self {
            paths,
            config,
            types,
            presets,
            lifecycle_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn types(&self) -> &InstanceTypeRegistry {
        &self.types
    }

    pub(crate) fn presets(&self) -> &PresetCatalog {
        &self.presets
    }

    pub(crate) fn new_instance_identity(&self) -> Result<String, String> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|error| format!("generate instance identity: {error}"))?;
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    pub(crate) fn record_or_config_exists(&self, instance_id: &str) -> bool {
        self.paths.instance_config_file(instance_id).exists()
            || self.paths.instance_lifecycle_file(instance_id).exists()
    }

    pub(crate) fn managed_snapshot_path(&self, instance_id: &str) -> PathBuf {
        self.paths.instance_source_snapshot_file(instance_id)
    }

    pub(crate) fn maintenance_journal_path(&self) -> PathBuf {
        self.paths.instance_maintenance_journal_file()
    }

    pub(crate) fn create_instance(
        &self,
        record: WebUiInstanceRecord,
        config: InstanceConfig,
        snapshot_payload: Option<&Value>,
    ) -> Result<(), String> {
        let _guard = self.lifecycle_lock.lock();
        let instance_id = &record.instance_id;
        let lifecycle_file = self.paths.instance_lifecycle_file(instance_id);
        if lifecycle_file.exists() || self.paths.instance_config_file(instance_id).exists() {
            return Err(format!("instance already exists: {instance_id}"));
        }
        safe_file::write_json_atomic(&lifecycle_file, &record)?;

        let snapshot_path = match (&record.install_source, snapshot_payload) {
            (WebUiInstanceInstallSource::Snapshot { snapshot_path }, Some(value)) => {
                if let Err(error) = safe_file::write_json_atomic(snapshot_path, value) {
                    remove_atomic_json(&lifecycle_file);
                    remove_empty_record_dir(&self.paths.instance_record_dir(instance_id));
                    return Err(error);
                }
                Some(snapshot_path.clone())
            }
            (WebUiInstanceInstallSource::Snapshot { .. }, None) => {
                remove_atomic_json(&lifecycle_file);
                remove_empty_record_dir(&self.paths.instance_record_dir(instance_id));
                return Err("snapshot-backed instance omitted its managed snapshot".to_string());
            }
            (
                WebUiInstanceInstallSource::Standard | WebUiInstanceInstallSource::DiscoveredCore,
                Some(_),
            ) => {
                remove_atomic_json(&lifecycle_file);
                remove_empty_record_dir(&self.paths.instance_record_dir(instance_id));
                return Err("non-snapshot instance unexpectedly supplied snapshot data".to_string());
            }
            (
                WebUiInstanceInstallSource::Standard | WebUiInstanceInstallSource::DiscoveredCore,
                None,
            ) => None,
        };

        if let Err(error) = create_instance_config_inner(&self.paths, &self.config, config) {
            remove_atomic_json(&lifecycle_file);
            if let Some(snapshot_path) = snapshot_path {
                remove_atomic_json(&snapshot_path);
            }
            remove_empty_record_dir(&self.paths.instance_record_dir(instance_id));
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn load_record(&self, instance_id: &str) -> Result<WebUiInstanceRecord, String> {
        let _guard = self.lifecycle_lock.lock();
        self.load_record_unlocked(instance_id)
    }

    pub(crate) fn list_record_ids(&self) -> Result<Vec<String>, String> {
        let _guard = self.lifecycle_lock.lock();
        let entries = std::fs::read_dir(&self.paths.instance_records_dir).map_err(|error| {
            format!(
                "read {}: {error}",
                self.paths.instance_records_dir.display()
            )
        })?;
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                continue;
            }
            let path = entry.path();
            if !path.join("lifecycle.json").is_file() {
                continue;
            }
            if let Some(id) = path.file_name().and_then(|value| value.to_str()) {
                if let Err(error) = validate_instance_id(id) {
                    log::warn!(
                        "skipping invalid WebUI instance record directory id={id} error={error}"
                    );
                } else {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    pub(crate) fn snapshot(
        &self,
        record: WebUiInstanceRecord,
        actions: WebUiInstanceInstallActions,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let instance_id = &record.instance_id;
        let instance = get_instance_config_inner(&self.paths, &self.config, instance_id)?;
        if instance.component_paths.is_none() {
            return Err(format!(
                "WebUI instance {instance_id} does not have explicit component paths"
            ));
        }
        let resolved = get_resolved_instance_config_inner(&self.paths, &self.config, instance_id)?;
        let python_environment_established = instance
            .component_paths
            .as_ref()
            .and_then(|paths| paths.python())
            .and_then(resolve_existing_python_executable)
            .is_some();
        let deleted_directories = InstancePathPolicy::plan_instance_deletion(
            &resolved.install_dir,
            instance.component_paths.as_ref(),
        )
        .ok();
        Ok(WebUiInstanceSnapshot {
            instance,
            availability: record.availability(),
            install: record,
            install_dir: resolved.install_dir,
            install_action: actions.install,
            reinstall_action: actions.reinstall,
            force_recreate_python_action: actions.force_recreate_python,
            force_recreate_core_action: actions.force_recreate_core,
            reinstall_spec_action: actions.reinstall_spec,
            python_environment_established,
            deleted_directories,
        })
    }

    /// Where the instance is, as its type's placeholders resolve.
    pub(crate) fn location(&self, instance_id: &str) -> Result<InstanceLocation, String> {
        let resolved = get_resolved_instance_config_inner(&self.paths, &self.config, instance_id)?;
        Ok(InstanceLocation {
            instance_id: instance_id.to_string(),
            instance_root: resolved.install_dir,
            components: resolved.component_paths,
        })
    }

    pub(crate) fn update_component_paths(
        &self,
        request: &UpdateWebUiInstanceComponentPathsRequest,
    ) -> Result<(), String> {
        let record = self.load_record(&request.instance_id)?;
        record.require_available()?;
        let provider = self.types.require(&record.type_id)?;
        let declared = &provider.descriptor().components;
        let named: Vec<&str> = request.component_paths.names().collect();
        if named.len() != declared.len()
            || declared.iter().any(|name| !named.contains(&name.as_str()))
        {
            return Err(format!(
                "the component paths name {}, and the instance's type declares {}",
                named.join(", "),
                declared.join(", ")
            ));
        }
        update_instance_component_paths_inner(
            &self.paths,
            &self.config,
            &request.instance_id,
            request.expected_config_revision,
            request.component_paths.clone(),
        )
        .map(|_| ())
    }

    pub(crate) fn update_quick_paths(
        &self,
        request: &UpdateWebUiInstanceQuickPathsRequest,
    ) -> Result<(), String> {
        self.load_record(&request.instance_id)?
            .require_available()?;
        update_instance_quick_paths_inner(
            &self.paths,
            &self.config,
            &request.instance_id,
            request.expected_config_revision,
            request.quick_paths.clone(),
        )
        .map(|_| ())
    }

    pub(crate) fn rename(&self, request: &RenameWebUiInstanceRequest) -> Result<(), String> {
        let _guard = self.lifecycle_lock.lock();
        let record = self.load_record_unlocked(&request.instance_id)?;
        record.require_available()?;
        if record.instance_identity != request.expected_instance_identity {
            return Err("instance identity changed before rename commit".to_string());
        }
        update_instance_name_inner(
            &self.paths,
            &self.config,
            &request.instance_id,
            request.expected_config_revision,
            request.name.clone(),
        )?;
        Ok(())
    }

    pub(crate) fn plan_migration_paths(
        &self,
        instance_id: &str,
        source_install_dir: PathBuf,
        destination_parent_dir: PathBuf,
        source_component_paths: InstanceComponentPaths,
    ) -> Result<ValidatedInstanceMigrationPaths, String> {
        let existing = list_instance_configs_inner(&self.paths, &self.config)?;
        InstancePathPolicy::plan_migration(
            instance_id,
            source_install_dir,
            destination_parent_dir,
            source_component_paths,
            &existing,
        )
    }

    pub(crate) fn commit_migration(&self, plan: &InstanceMigrationPlan) -> Result<(), String> {
        let _guard = self.lifecycle_lock.lock();
        let record = self.load_record_unlocked(&plan.instance_id)?;
        record.require_available()?;
        if record.instance_identity != plan.expected_instance_identity {
            return Err("instance identity changed before migration commit".to_string());
        }
        if !matches!(record.lifecycle, WebUiInstanceLifecycle::Ready { .. }) {
            return Err(format!(
                "instance {} is not ready for migration commit: {:?}",
                plan.instance_id, record.lifecycle
            ));
        }
        update_instance_location_inner(
            &self.paths,
            &self.config,
            &plan.instance_id,
            plan.expected_config_revision,
            &plan.source_install_dir,
            plan.destination_install_dir.clone(),
            plan.destination_component_paths.clone(),
        )?;
        Ok(())
    }

    pub(crate) fn transition_lifecycle(
        &self,
        instance_id: &str,
        lifecycle: WebUiInstanceLifecycle,
        updated_at: u128,
        policy: LifecycleTransitionPolicy,
    ) -> Result<WebUiInstanceRecord, String> {
        self.transition_lifecycle_recording_spec(instance_id, lifecycle, None, updated_at, policy)
    }

    /// Transition the lifecycle and, when a spec is given, record it in the
    /// same write, so an install never runs with a spec its record does not name.
    pub(crate) fn transition_lifecycle_recording_spec(
        &self,
        instance_id: &str,
        lifecycle: WebUiInstanceLifecycle,
        spec: Option<InstanceSpec>,
        updated_at: u128,
        policy: LifecycleTransitionPolicy,
    ) -> Result<WebUiInstanceRecord, String> {
        let _guard = self.lifecycle_lock.lock();
        let mut record = self.load_record_unlocked(instance_id)?;
        match policy {
            LifecycleTransitionPolicy::Standard => {
                super::validate_transition(&record.lifecycle, &lifecycle)?;
            }
            LifecycleTransitionPolicy::ReadyReinstall => {
                if !matches!(record.lifecycle, WebUiInstanceLifecycle::Ready { .. })
                    || !matches!(lifecycle, WebUiInstanceLifecycle::Installing { .. })
                {
                    return Err(format!(
                        "invalid WebUI reinstall lifecycle transition: {:?} -> {lifecycle:?}",
                        record.lifecycle
                    ));
                }
            }
        }
        record.lifecycle = lifecycle;
        if let Some(spec) = spec {
            record.spec = spec;
        }
        record.updated_at = updated_at;
        safe_file::write_json_atomic(&self.paths.instance_lifecycle_file(instance_id), &record)?;
        Ok(record)
    }

    /// Records the completed run, and whether this call completed it: a
    /// run recorded ready already (a later poll of it) was not.
    pub(crate) fn complete_install_with_defaults(
        &self,
        instance_id: &str,
        operation_id: String,
        updated_at: u128,
    ) -> Result<(WebUiInstanceRecord, bool), String> {
        let _guard = self.lifecycle_lock.lock();
        let mut record = self.load_record_unlocked(instance_id)?;
        let lifecycle = WebUiInstanceLifecycle::Ready { operation_id };
        super::validate_transition(&record.lifecycle, &lifecycle)?;
        let completed_now = record.lifecycle != lifecycle;
        // A declarative type's servers take no launch arguments.
        if matches!(record.spec, InstanceSpec::Builtin(_)) {
            synchronize_builtin_instance_launch_args_inner(
                &self.paths,
                &self.config,
                instance_id,
                record.default_launch_arguments()?,
            )?;
        }
        record.lifecycle = lifecycle;
        record.updated_at = updated_at;
        safe_file::write_json_atomic(&self.paths.instance_lifecycle_file(instance_id), &record)?;
        Ok((record, completed_now))
    }

    pub(crate) fn persist_branch_preset_if_identity(
        &self,
        instance_id: &str,
        instance_identity: &str,
        preset_id: String,
        updated_at: u128,
    ) -> Result<(), String> {
        let _guard = self.lifecycle_lock.lock();
        let mut record = self.load_record_unlocked(instance_id)?;
        if record.instance_identity != instance_identity {
            return Err(format!(
                "instance identity changed before branch preset persistence: {instance_id}"
            ));
        }
        record.install_spec_mut()?.set_branch_name(preset_id)?;
        let arguments = record.default_launch_arguments()?;
        synchronize_builtin_instance_launch_args_inner(
            &self.paths,
            &self.config,
            instance_id,
            arguments,
        )?;
        record.updated_at = updated_at;
        safe_file::write_json_atomic(&self.paths.instance_lifecycle_file(instance_id), &record)
    }

    pub(crate) fn default_install_dir(&self, instance_id: &str) -> Result<PathBuf, String> {
        Ok(get_global_settings_inner(&self.paths, &self.config)?
            .default_webui_install_dir
            .join(instance_id))
    }

    pub(crate) fn delete_state_if_identity(
        &self,
        instance_id: &str,
        instance_identity: &str,
    ) -> Result<(), String> {
        let _lifecycle_guard = self.lifecycle_lock.lock();
        let _instance_guard = self.config.instances_lock.lock();
        let record = self.load_record_unlocked(instance_id)?;
        if record.instance_identity != instance_identity {
            return Err(format!(
                "instance identity changed before lifecycle deletion: {instance_id}"
            ));
        }
        let lifecycle_path = self.paths.instance_lifecycle_file(instance_id);
        let instance_path = self.paths.instance_config_file(instance_id);
        std::fs::remove_file(&lifecycle_path)
            .map_err(|error| format!("delete {}: {error}", lifecycle_path.display()))?;
        if let Err(error) = std::fs::remove_file(&instance_path) {
            let rollback = safe_file::write_json_atomic(&lifecycle_path, &record);
            return Err(match rollback {
                Ok(()) => format!("delete {}: {error}", instance_path.display()),
                Err(rollback) => format!(
                    "delete {}: {error}; restore lifecycle after failed deletion: {rollback}",
                    instance_path.display()
                ),
            });
        }
        remove_atomic_backup(&lifecycle_path);
        remove_atomic_backup(&instance_path);
        if let WebUiInstanceInstallSource::Snapshot { snapshot_path } = record.install_source {
            remove_atomic_json(&snapshot_path);
        }
        remove_empty_record_dir(&self.paths.instance_record_dir(instance_id));
        drop(_instance_guard);
        drop(_lifecycle_guard);
        Ok(())
    }

    fn load_record_unlocked(&self, instance_id: &str) -> Result<WebUiInstanceRecord, String> {
        let path = self.paths.instance_lifecycle_file(instance_id);
        let raw = match safe_file::read_json::<serde_json::Value>(&path) {
            Ok(Some(value)) => value,
            Ok(None) => return Err(format!("WebUI instance lifecycle not found: {instance_id}")),
            Err(error) => {
                quarantine_safe_json(&path)?;
                return Err(format!(
                    "WebUI instance lifecycle was quarantined because it is unreadable: {error}"
                ));
            }
        };
        let decoded = match decode_record(&self.types, &raw) {
            Ok(decoded) => decoded,
            Err(reason) => {
                quarantine_safe_json(&path)?;
                return Err(format!(
                    "WebUI instance lifecycle was quarantined because {reason}"
                ));
            }
        };
        let record = decoded.record;
        if record.instance_id != instance_id {
            quarantine_safe_json(&path)?;
            return Err("WebUI instance lifecycle was quarantined because its identity does not match the record directory".to_string());
        }
        if decoded.repaired {
            quarantine_safe_json(&path)?;
            safe_file::write_json_atomic(&path, &record)?;
            log::warn!(
                "destructively repaired WebUI instance lifecycle instance={instance_id} path={}",
                path.display()
            );
        }
        if record.instance_identity.is_empty() {
            quarantine_safe_json(&path)?;
            return Err(
                "WebUI instance lifecycle was quarantined because its identity is empty"
                    .to_string(),
            );
        }
        Ok(record)
    }
}

fn remove_atomic_json(path: &Path) {
    let _ = std::fs::remove_file(path);
    remove_atomic_backup(path);
}

fn remove_atomic_backup(path: &Path) {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    let _ = std::fs::remove_file(PathBuf::from(backup));
}

fn remove_empty_record_dir(path: &Path) {
    match std::fs::remove_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => log::warn!(
            "instance record directory was retained path={} error={error}",
            path.display()
        ),
    }
}
