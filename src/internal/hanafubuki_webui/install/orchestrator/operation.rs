use std::path::PathBuf;
use std::sync::Arc;

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;

use crate::internal::hanafubuki_webui::instances::resolve_webui_instance_paths;
use crate::internal::hanafubuki_webui::runtime::{
    SdWebuiAllInOneRunRequest, build_sd_webui_all_in_one_run_spec,
};

use super::completion::{finish, update_owned};
use super::core_task::{CoreTaskOutcome, run_core_task};
use super::model::{
    CoreInstallTaskRequest, InstanceInstallPhase, InstanceInstallPreparation,
    InstanceInstallStageError, InstanceInstallTaskRef,
};
use super::registry::ServiceInner;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

pub(super) fn run_operation(
    inner: Arc<ServiceInner>,
    operation_id: String,
    ownership: u64,
    prepared: InstanceInstallPreparation,
    python_dir: PathBuf,
    python_version: PythonVersion,
    cli_args: Vec<String>,
    task: InstanceInstallTaskRef,
    cancellation: CancellationToken,
    _instance_identity: String,
    force_recreate_python: bool,
    force_recreate_core: bool,
) {
    let component_paths = match resolve_webui_instance_paths(
        prepared.instance_path.clone(),
        prepared.component_paths.as_ref(),
    ) {
        Ok(paths) => paths,
        Err(error) => {
            finish(
                &inner,
                &operation_id,
                ownership,
                InstanceInstallPhase::Failed,
                None,
                Some(error),
            );
            return;
        }
    };
    if force_recreate_python {
        if let Err(error) = inner
            .backend
            .remove_component_path("python", &component_paths.python_dir)
        {
            finish(
                &inner,
                &operation_id,
                ownership,
                InstanceInstallPhase::Failed,
                None,
                Some(error),
            );
            return;
        }
    }
    if force_recreate_core {
        if !update_owned(&inner, &operation_id, ownership, |snapshot| {
            snapshot.phase = InstanceInstallPhase::RemovingCore
        }) {
            return;
        }
        if let Err(error) = inner
            .backend
            .remove_component_path("core", &component_paths.core_path)
        {
            finish(
                &inner,
                &operation_id,
                ownership,
                InstanceInstallPhase::Failed,
                None,
                Some(error),
            );
            return;
        }
    }
    if cancellation.is_cancelled() {
        finish(
            &inner,
            &operation_id,
            ownership,
            InstanceInstallPhase::Cancelled,
            None,
            None,
        );
        return;
    }
    if !update_owned(&inner, &operation_id, ownership, |snapshot| {
        snapshot.phase = InstanceInstallPhase::InstallingPython
    }) {
        return;
    }
    let python_path =
        match inner
            .backend
            .ensure_python(python_dir.clone(), python_version, &cancellation)
        {
            Ok(path) => path,
            Err(InstanceInstallStageError::Cancelled) => {
                finish(
                    &inner,
                    &operation_id,
                    ownership,
                    InstanceInstallPhase::Cancelled,
                    None,
                    None,
                );
                return;
            }
            Err(InstanceInstallStageError::Failed(error)) => {
                finish(
                    &inner,
                    &operation_id,
                    ownership,
                    InstanceInstallPhase::Failed,
                    None,
                    Some(error),
                );
                return;
            }
        };
    if cancellation.is_cancelled() {
        finish(
            &inner,
            &operation_id,
            ownership,
            InstanceInstallPhase::Cancelled,
            None,
            None,
        );
        return;
    }

    let process_path = std::env::var_os("PATH");
    let environment =
        match crate::internal::hanafubuki_webui::runtime::merge_python_path_environment(
            prepared.env.into_iter().collect(),
            &python_dir,
            process_path.as_deref(),
        ) {
            Ok(environment) => environment.into_iter().collect(),
            Err(error) => {
                finish(
                    &inner,
                    &operation_id,
                    ownership,
                    InstanceInstallPhase::Failed,
                    None,
                    Some(error),
                );
                return;
            }
        };
    let run_spec = match build_sd_webui_all_in_one_run_spec(SdWebuiAllInOneRunRequest {
        instance_id: task.instance_id.clone(),
        python_path,
        cli_args,
        cwd: prepared.instance_path,
        env: environment,
        source_dir: prepared.launcher_source_dir,
    }) {
        Ok(spec) => spec,
        Err(error) => {
            finish(
                &inner,
                &operation_id,
                ownership,
                InstanceInstallPhase::Failed,
                None,
                Some(error),
            );
            return;
        }
    };
    if !update_owned(&inner, &operation_id, ownership, |snapshot| {
        snapshot.phase = InstanceInstallPhase::InstallingCore;
    }) {
        return;
    }
    if cancellation.is_cancelled() {
        finish(
            &inner,
            &operation_id,
            ownership,
            InstanceInstallPhase::Cancelled,
            None,
            None,
        );
        return;
    }
    let outcome = run_core_task(
        &inner,
        &operation_id,
        ownership,
        CoreInstallTaskRequest::from_run_spec(task.clone(), run_spec),
        &cancellation,
    );
    match outcome {
        CoreTaskOutcome::Succeeded(_) => finish(
            &inner,
            &operation_id,
            ownership,
            InstanceInstallPhase::Completed,
            Some(0),
            None,
        ),
        CoreTaskOutcome::Cancelled { exit_code } => finish(
            &inner,
            &operation_id,
            ownership,
            InstanceInstallPhase::Cancelled,
            exit_code,
            None,
        ),
        CoreTaskOutcome::Failed { exit_code, error } => finish(
            &inner,
            &operation_id,
            ownership,
            InstanceInstallPhase::Failed,
            exit_code,
            error,
        ),
        CoreTaskOutcome::Settled => {}
    }
}
