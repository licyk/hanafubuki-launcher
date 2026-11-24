use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;

use super::super::identity::now_millis;
use super::super::repository::LifecycleTransitionPolicy;
use super::super::{
    InstanceSpec, ReinstallWebUiInstanceRequest, WebUiInstanceInstallSource,
    WebUiInstanceLifecycle, WebUiInstanceRecord,
};
use super::WebUiInstanceService;
use crate::internal::hanafubuki_webui::install::steps::StepOperation;
use crate::internal::hanafubuki_webui::install::{
    InstanceInstallOperation, StartInstanceInstallRequest,
};
use crate::internal::hanafubuki_webui::instance_types::unavailable_reason;

pub(super) const SNAPSHOT_REINSTALL_SPEC_UNAVAILABLE: &str =
    "an instance restored from a snapshot reinstalls with the snapshot's own parameters";

impl WebUiInstanceService {
    pub fn start_install(&self, instance_id: &str) -> Result<InstanceInstallOperation, String> {
        let record = self.repository.load_record(instance_id)?;
        record.require_available()?;
        let record = self.recover_interrupted_install(record)?;
        if !matches!(
            record.lifecycle,
            WebUiInstanceLifecycle::Created | WebUiInstanceLifecycle::InstallFailed { .. }
        ) {
            return Err(format!(
                "instance {instance_id} cannot start installation from {:?}",
                record.lifecycle
            ));
        }
        self.start_install_record(record, None, false, false)
    }

    pub fn start_reinstall(
        &self,
        request: ReinstallWebUiInstanceRequest,
    ) -> Result<InstanceInstallOperation, String> {
        let record = self.repository.load_record(&request.instance_id)?;
        record.require_available()?;
        let record = self.recover_interrupted_install(record)?;
        let spec = self.checked_reinstall_spec(&request)?;
        self.start_install_record(
            record,
            spec,
            request.force_recreate_python,
            request.force_recreate_core,
        )
    }

    pub fn validate_reinstall_request(
        &self,
        request: &ReinstallWebUiInstanceRequest,
    ) -> Result<(), String> {
        self.checked_reinstall_spec(request).map(|_| ())
    }

    /// The request's checks, and the install spec it replaces the recorded
    /// one with, read by the instance's type.
    fn checked_reinstall_spec(
        &self,
        request: &ReinstallWebUiInstanceRequest,
    ) -> Result<Option<InstanceSpec>, String> {
        let record = self.repository.load_record(&request.instance_id)?;
        record.require_available()?;
        let snapshot = self.repository.snapshot(
            record.clone(),
            super::super::WebUiInstanceInstallActions::unavailable("reinstall validation snapshot"),
        )?;
        if record.instance_identity != request.expected_instance_identity {
            return Err("instance identity changed; reload maintenance details".to_string());
        }
        if snapshot.instance.config_revision != request.expected_config_revision {
            return Err("instance configuration changed; reload maintenance details".to_string());
        }
        if !matches!(
            record.lifecycle,
            WebUiInstanceLifecycle::Created
                | WebUiInstanceLifecycle::Ready { .. }
                | WebUiInstanceLifecycle::InstallFailed { .. }
        ) {
            return Err(format!(
                "instance {} cannot be reinstalled from {:?}",
                request.instance_id, record.lifecycle
            ));
        }
        let provider = self.repository.types().require(&record.type_id)?;
        let spec = match &request.spec {
            Some(spec) => Some(provider.validate_instance_spec(
                spec.clone(),
                &self.repository.location(&request.instance_id)?,
            )?),
            None => None,
        };
        if spec.is_some()
            && matches!(
                record.install_source,
                WebUiInstanceInstallSource::Snapshot { .. }
            )
        {
            return Err(SNAPSHOT_REINSTALL_SPEC_UNAVAILABLE.to_string());
        }
        if let (InstanceSpec::Builtin(recorded), Some(InstanceSpec::Builtin(spec))) =
            (&record.spec, &spec)
        {
            if !request.force_recreate_python
                && snapshot.python_environment_established
                && recorded.python_environment_differs(spec)
            {
                return Err(
                    "a different Python version or PyTorch build takes effect only when the Python environment is recreated"
                        .to_string(),
                );
            }
        }
        let paths = snapshot
            .instance
            .component_paths
            .as_ref()
            .ok_or_else(|| "instance component paths are not configured".to_string())?;
        InstancePathPolicy::validate_installation_layout(
            &snapshot.install_dir,
            paths,
            request.force_recreate_python,
            request.force_recreate_core,
        )?;
        // Checked here rather than only when the install is reserved: by then
        // the instance is quiesced under a journaled reinstall that can only be
        // retried exactly as asked, so a selection the catalog lacks would
        // leave it unable to finish. An unchanged spec is what the instance
        // already installs with and is left to the reservation.
        if record.is_declarative()
            && matches!(record.lifecycle, WebUiInstanceLifecycle::Ready { .. })
            && !request.force_recreate_python
            && !request.force_recreate_core
            && !provider.offers_update()
        {
            return Err(format!(
                "instance type `{}` declares no update",
                record.type_id
            ));
        }
        if let Some(InstanceSpec::Builtin(spec)) =
            spec.as_ref().filter(|spec| **spec != record.spec)
        {
            self.installs.validate_spec(spec)?;
        }
        Ok(spec)
    }

    /// Reserve and start an install of `record`, with `spec` replacing its
    /// recorded spec when given. The replacement is recorded together with the
    /// `Installing` transition, so later installs and a retry after failure use it.
    pub(super) fn start_install_record(
        &self,
        record: WebUiInstanceRecord,
        spec: Option<InstanceSpec>,
        force_recreate_python: bool,
        force_recreate_core: bool,
    ) -> Result<InstanceInstallOperation, String> {
        let instance_id = record.instance_id.clone();
        let reinstall_from_ready = matches!(record.lifecycle, WebUiInstanceLifecycle::Ready { .. });
        let reservation = match spec.as_ref().unwrap_or(&record.spec) {
            InstanceSpec::Builtin(install_spec) => {
                let install_request = StartInstanceInstallRequest {
                    instance_id: instance_id.clone(),
                    spec: install_spec.clone(),
                };
                match &record.install_source {
                    WebUiInstanceInstallSource::Standard
                    | WebUiInstanceInstallSource::DiscoveredCore => {
                        self.installs.reserve_start_with_policy(
                            install_request,
                            record.instance_identity.clone(),
                            force_recreate_python,
                            force_recreate_core,
                        )?
                    }
                    WebUiInstanceInstallSource::Snapshot { snapshot_path } => {
                        self.installs.reserve_snapshot_start_with_policy(
                            install_request,
                            record.instance_identity.clone(),
                            snapshot_path.clone(),
                            force_recreate_python,
                            force_recreate_core,
                        )?
                    }
                }
            }
            declarative @ InstanceSpec::Declarative(_) => {
                let forced = force_recreate_python || force_recreate_core;
                let operation = match (reinstall_from_ready, forced) {
                    (true, false) => StepOperation::Update,
                    (true, true) => StepOperation::Reinstall,
                    (false, _) => StepOperation::Install,
                };
                let provider = self.repository.types().require(&record.type_id)?;
                let process_path = std::env::var_os("PATH");
                crate::internal::hanafubuki_webui::runtime::require_runtime_program(
                    &provider.descriptor().runtime,
                    process_path.as_deref(),
                )?;
                let plan = provider.step_plan(
                    operation,
                    declarative,
                    &self.repository.location(&instance_id)?,
                )?;
                self.installs.reserve_steps_with_policy(
                    &instance_id,
                    plan,
                    record.instance_identity.clone(),
                    force_recreate_python,
                    force_recreate_core,
                )?
            }
            InstanceSpec::Unavailable(_) => return Err(unavailable_reason(&record.type_id)),
        };
        let operation = reservation.operation().clone();
        let installing = WebUiInstanceLifecycle::Installing {
            operation_id: operation.operation_id.clone(),
        };
        let policy = if reinstall_from_ready {
            LifecycleTransitionPolicy::ReadyReinstall
        } else {
            LifecycleTransitionPolicy::Standard
        };
        let transition_result = self.repository.transition_lifecycle_recording_spec(
            &instance_id,
            installing,
            spec,
            now_millis(),
            policy,
        );
        if let Err(error) = transition_result {
            self.installs.abort_start(reservation);
            return Err(format!(
                "installation was not started because lifecycle persistence failed: {error}"
            ));
        }
        let operation = self.installs.commit_start(reservation);
        self.spawn_monitor(instance_id.to_string(), operation.operation_id.clone());
        Ok(operation)
    }
}
