use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;
use crate::internal::hanafubuki_foundation::background_worker::spawn_contained_thread;
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;

use crate::internal::hanafubuki_webui::instances::resolve_webui_instance_paths;

use super::super::{
    WebUiInstallRequest, build_webui_install_plan, build_webui_snapshot_restore_plan,
};
use super::model::{
    InstanceInstallCleanupStatus, InstanceInstallOperation, InstanceInstallPhase,
    InstanceInstallTaskRef, StartInstanceInstallRequest,
};
use super::operation::run_operation;
use super::registry::{
    InstallWork, InstanceInstallReservation, InstanceInstallService, OperationEntry,
    OwnershipLifecycle, OwnershipRecord,
};
use super::steps::{StepRun, removals, run_steps};
use super::{INSTALL_MODULE, install_activity_id, now_millis};
use crate::internal::hanafubuki_webui::install::steps::StepPlan;

impl InstanceInstallService {
    /// Convenience entry point for core callers that do not own a durable
    /// lifecycle. Desktop instance installation uses reserve/commit instead.
    pub fn start(
        &self,
        request: StartInstanceInstallRequest,
    ) -> Result<InstanceInstallOperation, String> {
        let reservation = self.reserve_start(request)?;
        Ok(self.commit_start(reservation))
    }

    /// Prepare and reserve one instance without starting Stage 1. The returned
    /// value is deliberately non-cloneable so it can be committed or aborted
    /// exactly once after the caller's durable write succeeds or fails.
    pub(crate) fn reserve_start(
        &self,
        request: StartInstanceInstallRequest,
    ) -> Result<InstanceInstallReservation, String> {
        self.reserve_start_with_policy(request, String::new(), false, false)
    }

    pub(crate) fn reserve_start_with_policy(
        &self,
        request: StartInstanceInstallRequest,
        instance_identity: String,
        force_recreate_python: bool,
        force_recreate_core: bool,
    ) -> Result<InstanceInstallReservation, String> {
        self.reserve_start_with_source(
            request,
            instance_identity,
            force_recreate_python,
            force_recreate_core,
            None,
        )
    }

    pub(crate) fn reserve_snapshot_start_with_policy(
        &self,
        request: StartInstanceInstallRequest,
        instance_identity: String,
        snapshot_path: PathBuf,
        force_recreate_python: bool,
        force_recreate_core: bool,
    ) -> Result<InstanceInstallReservation, String> {
        self.reserve_start_with_source(
            request,
            instance_identity,
            force_recreate_python,
            force_recreate_core,
            Some(snapshot_path),
        )
    }

    fn reserve_start_with_source(
        &self,
        request: StartInstanceInstallRequest,
        instance_identity: String,
        force_recreate_python: bool,
        force_recreate_core: bool,
        snapshot_path: Option<PathBuf>,
    ) -> Result<InstanceInstallReservation, String> {
        let instance_id = request.instance_id.trim().to_string();
        if instance_id.is_empty() {
            return Err("instance_id must not be empty".to_string());
        }

        let prepared = self.inner.backend.prepare(&instance_id)?;
        let paths = resolve_webui_instance_paths(
            prepared.instance_path.clone(),
            prepared.component_paths.as_ref(),
        )?;
        InstancePathPolicy::validate_installation_layout(
            &paths.instance_path,
            &crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths::python_and_core(
                paths.python_dir.clone(),
                paths.core_path.clone(),
            ),
            force_recreate_python,
            force_recreate_core,
        )?;
        let family = request.spec.family();
        let (python_version, cli_args) = if let Some(snapshot_path) = snapshot_path {
            let python_version = request.spec.python_version().ok_or_else(|| {
                "snapshot installation requires a recorded Python version".to_string()
            })?;
            let plan = build_webui_snapshot_restore_plan(
                family,
                python_version,
                snapshot_path,
                paths.core_path,
                &prepared.settings,
            )
            .map_err(|error| error.to_string())?;
            (plan.python_version(), plan.cli_args().to_vec())
        } else {
            let plan = build_webui_install_plan(
                WebUiInstallRequest {
                    target_path: paths.core_path,
                    spec: request.spec,
                },
                &prepared.settings,
                &prepared.catalog,
            )
            .map_err(|error| error.to_string())?;
            (plan.python_version(), plan.cli_args().to_vec())
        };

        self.register(
            instance_id,
            instance_identity,
            InstallWork::Builtin {
                prepared,
                python_dir: paths.python_dir,
                python_version,
                cli_args,
            },
            force_recreate_python,
            force_recreate_core,
        )
    }

    /// Reserve one instance for a declarative type's steps, resolved by its
    /// provider. Nothing here needs the application's Python runtime.
    pub(crate) fn reserve_steps_with_policy(
        &self,
        instance_id: &str,
        plan: StepPlan,
        instance_identity: String,
        force_recreate_python: bool,
        force_recreate_core: bool,
    ) -> Result<InstanceInstallReservation, String> {
        let prepared = self.inner.backend.prepare_steps(instance_id)?;
        InstancePathPolicy::validate_installation_layout(
            &prepared.instance_path,
            &prepared.components,
            force_recreate_python,
            force_recreate_core,
        )?;
        self.register(
            instance_id.to_string(),
            instance_identity,
            InstallWork::Steps {
                plan,
                environment: prepared.environment,
                components: prepared.components,
            },
            force_recreate_python,
            force_recreate_core,
        )
    }

    fn register(
        &self,
        instance_id: String,
        instance_identity: String,
        work: InstallWork,
        force_recreate_python: bool,
        force_recreate_core: bool,
    ) -> Result<InstanceInstallReservation, String> {
        let sequence = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let now = now_millis();
        let operation_id = format!("install-{now}-{sequence}");
        let ownership = sequence;
        let task = InstanceInstallTaskRef {
            instance_id: instance_id.clone(),
            task_id: format!("webui-install-{now}-{sequence}"),
        };
        let cancellation = CancellationToken::default();
        let (first_phase, python_version) = match &work {
            InstallWork::Builtin { python_version, .. } => (
                InstanceInstallPhase::InstallingPython,
                Some(*python_version),
            ),
            InstallWork::Steps { .. } => (InstanceInstallPhase::RunningSteps, None),
        };
        let snapshot = InstanceInstallOperation {
            activity_id: install_activity_id(&operation_id),
            operation_id: operation_id.clone(),
            instance_id: instance_id.clone(),
            phase: if force_recreate_python {
                InstanceInstallPhase::RemovingPython
            } else if force_recreate_core {
                InstanceInstallPhase::RemovingCore
            } else {
                first_phase
            },
            created_at: now,
            updated_at: now,
            task: None,
            exit_code: None,
            error: None,
            python_version,
            step: None,
            cleanup_status: InstanceInstallCleanupStatus::Active,
            cleanup_error: None,
        };

        {
            let mut registry = self.inner.registry.lock();
            if registry.shutting_down {
                return Err("the application is quitting; installs cannot start".to_string());
            }
            if let Some(active) = registry.active_by_instance.get(&instance_id) {
                return Err(format!(
                    "instance {instance_id} remains owned by operation {} ({:?}); process cleanup is pending or failed",
                    active.operation_id, active.lifecycle
                ));
            }
            registry.active_by_instance.insert(
                instance_id.clone(),
                OwnershipRecord {
                    operation_id: operation_id.clone(),
                    generation: ownership,
                    lifecycle: OwnershipLifecycle::Active,
                },
            );
            registry.operations.insert(
                operation_id.clone(),
                OperationEntry {
                    core_task: None,
                    snapshot: snapshot.clone(),
                    ownership,
                    cancellation: cancellation.clone(),
                },
            );
        }

        Ok(InstanceInstallReservation {
            snapshot,
            ownership,
            work,
            task,
            cancellation,
            instance_identity,
            force_recreate_python,
            force_recreate_core,
        })
    }

    /// Start installation only after the lifecycle owner has durably recorded
    /// this reservation's operation id as `Installing`.
    pub(crate) fn commit_start(
        &self,
        reservation: InstanceInstallReservation,
    ) -> InstanceInstallOperation {
        let snapshot = reservation.snapshot.clone();
        let inner = self.inner.clone();
        let name = format!("install-{}", snapshot.operation_id);
        spawn_contained_thread(INSTALL_MODULE, name, move || match reservation.work {
            InstallWork::Builtin {
                prepared,
                python_dir,
                python_version,
                cli_args,
            } => run_operation(
                inner,
                reservation.snapshot.operation_id,
                reservation.ownership,
                prepared,
                python_dir,
                python_version,
                cli_args,
                reservation.task,
                reservation.cancellation,
                reservation.instance_identity,
                reservation.force_recreate_python,
                reservation.force_recreate_core,
            ),
            InstallWork::Steps {
                plan,
                environment,
                components,
            } => run_steps(
                inner,
                StepRun {
                    operation_id: reservation.snapshot.operation_id,
                    ownership: reservation.ownership,
                    task: reservation.task,
                    cancellation: reservation.cancellation,
                },
                plan,
                environment,
                removals(
                    &components,
                    reservation.force_recreate_python,
                    reservation.force_recreate_core,
                ),
            ),
        })
        .expect("spawn install thread");
        snapshot
    }

    /// Roll back an uncommitted reservation. No cancellation or cleanup is
    /// needed because reserve_start has not entered either installation stage.
    pub(crate) fn abort_start(&self, reservation: InstanceInstallReservation) {
        let mut registry = self.inner.registry.lock();
        let operation_id = &reservation.snapshot.operation_id;
        let instance_id = &reservation.snapshot.instance_id;
        if registry
            .operations
            .get(operation_id)
            .is_some_and(|entry| entry.ownership == reservation.ownership)
        {
            registry.operations.remove(operation_id);
        }
        if registry
            .active_by_instance
            .get(instance_id)
            .is_some_and(|owner| {
                owner.operation_id == *operation_id && owner.generation == reservation.ownership
            })
        {
            registry.active_by_instance.remove(instance_id);
        }
    }
}
