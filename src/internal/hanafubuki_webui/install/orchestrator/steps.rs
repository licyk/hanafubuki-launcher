//! A declarative type's install: its planned steps in order, each a file
//! operation or an Instance task of this operation. The first step that
//! fails ends the operation; a file step stages what it writes, so the files
//! it would have replaced stay as they were.

use std::path::PathBuf;
use std::sync::Arc;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;

use super::super::steps::{
    ComponentPath, InstallStepProgress, PlannedStep, ProcessEnvironment, StepEnvironment, StepPlan,
    files,
};
use super::completion::{fail_and_schedule_cleanup, finish, update_owned};
use super::core_task::{CoreTaskOutcome, run_core_task};
use super::model::{
    CoreInstallTaskRequest, InstanceInstallPhase, InstanceInstallStageError,
    InstanceInstallTaskCleanupResult, InstanceInstallTaskRef,
};
use super::registry::ServiceInner;

/// A failed task's output kept in the operation's error: its end, where the
/// reason usually is.
const STEP_OUTPUT_TAIL: usize = 4 * 1024;

/// The operation a step run belongs to.
pub(super) struct StepRun {
    pub(super) operation_id: String,
    pub(super) ownership: u64,
    /// The operation's task id; each process step's task is named after it.
    pub(super) task: InstanceInstallTaskRef,
    pub(super) cancellation: CancellationToken,
}

enum StepFailure {
    Cancelled {
        exit_code: Option<i32>,
    },
    Failed {
        exit_code: Option<i32>,
        error: String,
    },
    /// The operation is already settled (a task's cleanup is owed).
    Settled,
}

impl From<InstanceInstallStageError> for StepFailure {
    fn from(error: InstanceInstallStageError) -> Self {
        match error {
            InstanceInstallStageError::Cancelled => Self::Cancelled { exit_code: None },
            InstanceInstallStageError::Failed(error) => Self::Failed {
                exit_code: None,
                error,
            },
        }
    }
}

/// The components a forced reinstall removes before its steps run.
pub(super) fn removals(
    components: &InstanceComponentPaths,
    force_recreate_python: bool,
    force_recreate_core: bool,
) -> Vec<(String, PathBuf)> {
    let mut removals = Vec::new();
    if let Some(python) = components.python().filter(|_| force_recreate_python) {
        removals.push((
            InstanceComponentPaths::PYTHON.to_string(),
            python.to_path_buf(),
        ));
    }
    if force_recreate_core {
        removals.push((
            InstanceComponentPaths::CORE.to_string(),
            components.core().to_path_buf(),
        ));
    }
    removals
}

pub(super) fn run_steps(
    inner: Arc<ServiceInner>,
    run: StepRun,
    plan: StepPlan,
    environment: StepEnvironment,
    removals: Vec<(String, PathBuf)>,
) {
    let outcome = run_plan(&inner, &run, &plan, &environment, removals);
    let (phase, exit_code, error) = match outcome {
        Ok(()) => (InstanceInstallPhase::Completed, None, None),
        Err(StepFailure::Cancelled { exit_code }) => {
            (InstanceInstallPhase::Cancelled, exit_code, None)
        }
        Err(StepFailure::Failed { exit_code, error }) => {
            (InstanceInstallPhase::Failed, exit_code, Some(error))
        }
        Err(StepFailure::Settled) => return,
    };
    finish(
        &inner,
        &run.operation_id,
        run.ownership,
        phase,
        exit_code,
        error,
    );
}

fn run_plan(
    inner: &Arc<ServiceInner>,
    run: &StepRun,
    plan: &StepPlan,
    environment: &StepEnvironment,
    removals: Vec<(String, PathBuf)>,
) -> Result<(), StepFailure> {
    for (component, path) in removals {
        let phase = if component == InstanceComponentPaths::PYTHON {
            InstanceInstallPhase::RemovingPython
        } else {
            InstanceInstallPhase::RemovingCore
        };
        if !update_owned(inner, &run.operation_id, run.ownership, |snapshot| {
            snapshot.phase = phase
        }) {
            return Err(StepFailure::Settled);
        }
        inner
            .backend
            .remove_component_path(&component, &path)
            .map_err(|error| StepFailure::Failed {
                exit_code: None,
                error,
            })?;
    }
    let count = plan.steps.len();
    for (index, step) in plan.steps.iter().enumerate() {
        if run.cancellation.is_cancelled() {
            return Err(StepFailure::Cancelled { exit_code: None });
        }
        let progress = InstallStepProgress {
            index: index + 1,
            count,
            kind: step.kind(),
        };
        if !update_owned(inner, &run.operation_id, run.ownership, |snapshot| {
            snapshot.phase = InstanceInstallPhase::RunningSteps;
            snapshot.step = Some(progress);
        }) {
            return Err(StepFailure::Settled);
        }
        run_step(inner, run, index, step, plan, environment).map_err(|failure| match failure {
            StepFailure::Failed { exit_code, error } => StepFailure::Failed {
                exit_code,
                error: format!(
                    "step {} of {count} ({}) failed: {error}",
                    index + 1,
                    serde_json::to_value(step.kind())
                        .ok()
                        .and_then(|kind| kind.as_str().map(str::to_string))
                        .unwrap_or_default()
                ),
            },
            other => other,
        })?;
    }
    Ok(())
}

fn run_step(
    inner: &Arc<ServiceInner>,
    run: &StepRun,
    index: usize,
    step: &PlannedStep,
    plan: &StepPlan,
    environment: &StepEnvironment,
) -> Result<(), StepFailure> {
    let cancellation = &run.cancellation;
    match step {
        PlannedStep::Download {
            url,
            sha256,
            target,
        } => files::download(url, sha256, target, environment, cancellation)?,
        PlannedStep::Unpack {
            archive,
            into,
            strip,
            replace,
        } => files::unpack(&archive.path(), into, *strip, *replace, cancellation)?,
        PlannedStep::WriteFile { path, content } => files::write_file(path, content)?,
        PlannedStep::Probe { url, timeout } => files::probe(url, *timeout, cancellation)?,
        PlannedStep::PythonEnv {
            python_dir,
            version,
        } => {
            inner
                .backend
                .ensure_python(python_dir.clone(), *version, cancellation)?;
        }
        PlannedStep::Run { command, cwd, env } => {
            let env = environment_for(plan, environment, env)?;
            run_task(inner, run, index, command.clone(), cwd.path(), env)?;
        }
        PlannedStep::GitClone { url, into, branch } => git_clone(
            inner,
            run,
            index,
            url,
            into,
            branch.as_deref(),
            plan,
            environment,
        )?,
        PlannedStep::GitPull { repository } => {
            let git = git(inner, run)?;
            let command = vec![git, "pull".to_string(), "--ff-only".to_string()];
            let env = environment_for(plan, environment, &ProcessEnvironment::default())?;
            run_task(inner, run, index, command, repository.path(), env)?;
        }
    }
    Ok(())
}

/// Clones beside `into` and moves the clone into place. A repository that
/// is already there is left as it is: `git-pull` updates it.
#[allow(clippy::too_many_arguments)]
fn git_clone(
    inner: &Arc<ServiceInner>,
    run: &StepRun,
    index: usize,
    url: &str,
    into: &ComponentPath,
    branch: Option<&str>,
    plan: &StepPlan,
    environment: &StepEnvironment,
) -> Result<(), StepFailure> {
    let target = into.path();
    if target.join(".git").exists() {
        return Ok(());
    }
    let staging = files::staging_dir(into)?;
    let clone = staging.path().join("clone");
    let mut command = vec![git(inner, run)?, "clone".to_string()];
    if let Some(branch) = branch {
        command.extend(["--branch".to_string(), branch.to_string()]);
    }
    command.extend([
        "--".to_string(),
        url.to_string(),
        clone.to_string_lossy().into_owned(),
    ]);
    let env = environment_for(plan, environment, &ProcessEnvironment::default())?;
    run_task(
        inner,
        run,
        index,
        command,
        staging.path().to_path_buf(),
        env,
    )?;
    files::move_into_place(&clone, into)?;
    Ok(())
}

/// The Git runtime's Git, waited for while it prepares; a cancel of the
/// operation during that wait cancels the step.
fn git(inner: &ServiceInner, run: &StepRun) -> Result<String, StepFailure> {
    inner
        .backend
        .git_executable(&run.cancellation)
        .map(|git| git.to_string_lossy().into_owned())
        .map_err(|error| {
            if run.cancellation.is_cancelled() {
                StepFailure::Cancelled { exit_code: None }
            } else {
                StepFailure::Failed {
                    exit_code: None,
                    error,
                }
            }
        })
}

/// A step's environment ([`ProcessEnvironment::compose_over_inherited`]):
/// what it sets, and the inherited variables its type unsets; a Python
/// runtime's steps find its Python first on `PATH`.
fn environment_for(
    plan: &StepPlan,
    environment: &StepEnvironment,
    step: &ProcessEnvironment,
) -> Result<Vec<(String, Option<String>)>, StepFailure> {
    let process_path = std::env::var_os("PATH");
    let env = step
        .compose_over_inherited(
            &environment.env,
            process_path.as_deref(),
            crate::internal::hanafubuki_process::environment::process_environment_names(),
            |env| match &plan.python_dir {
                Some(python_dir) => {
                    crate::internal::hanafubuki_webui::runtime::merge_python_path_environment(
                        env,
                        python_dir,
                        process_path.as_deref(),
                    )
                }
                None => Ok(env),
            },
        )
        .map_err(|error| StepFailure::Failed {
            exit_code: None,
            error,
        })?;
    Ok(env.into_iter().collect())
}

/// Runs one process step as an Instance task of this operation and
/// disposes of the task once it ended well; a failed one stays for the
/// operation's own cleanup.
fn run_task(
    inner: &Arc<ServiceInner>,
    run: &StepRun,
    index: usize,
    command: Vec<String>,
    cwd: PathBuf,
    env: Vec<(String, Option<String>)>,
) -> Result<(), StepFailure> {
    let task = InstanceInstallTaskRef {
        instance_id: run.task.instance_id.clone(),
        task_id: format!("{}-step-{}", run.task.task_id, index + 1),
    };
    let outcome = run_core_task(
        inner,
        &run.operation_id,
        run.ownership,
        CoreInstallTaskRequest {
            task,
            command,
            cwd,
            env,
        },
        &run.cancellation,
    );
    match outcome {
        CoreTaskOutcome::Succeeded(core) => match inner.backend.dispose_core_task(&core) {
            Ok(
                InstanceInstallTaskCleanupResult::Disposed
                | InstanceInstallTaskCleanupResult::AlreadyAbsent,
            ) => Ok(()),
            Err(error) => {
                fail_and_schedule_cleanup(inner, &run.operation_id, run.ownership, core, error);
                Err(StepFailure::Settled)
            }
        },
        CoreTaskOutcome::Cancelled { exit_code } => Err(StepFailure::Cancelled { exit_code }),
        CoreTaskOutcome::Failed { exit_code, error } => {
            let output = tail(error.unwrap_or_default());
            Err(StepFailure::Failed {
                exit_code,
                error: match exit_code {
                    Some(code) => format!("it exited with {code}\n{output}"),
                    None => output,
                },
            })
        }
        CoreTaskOutcome::Settled => Err(StepFailure::Settled),
    }
}

/// The last [`STEP_OUTPUT_TAIL`] bytes of `text`, on a character boundary.
fn tail(text: String) -> String {
    if text.len() <= STEP_OUTPUT_TAIL {
        return text;
    }
    let mut start = text.len() - STEP_OUTPUT_TAIL;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &text[start..])
}
