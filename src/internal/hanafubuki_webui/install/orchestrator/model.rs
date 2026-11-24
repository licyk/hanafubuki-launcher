use std::path::PathBuf;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use serde::{Deserialize, Serialize};

use super::super::steps::{InstallStepProgress, StepEnvironment};
use super::super::{ResolvedInstallSettings, WebUiInstallOptionCatalog, WebUiInstallSpec};
use crate::internal::hanafubuki_python_runtime::PythonVersion;

pub use crate::internal::hanafubuki_task::output::{
    OutputDelta as InstanceInstallOutputChunk, OutputStream as InstanceInstallOutputStream,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInstanceInstallRequest {
    pub instance_id: String,
    pub spec: WebUiInstallSpec,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceInstallPhase {
    RemovingPython,
    RemovingCore,
    InstallingPython,
    InstallingCore,
    /// A declarative type's steps; the operation's `step` says which.
    RunningSteps,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceInstallCleanupStatus {
    Active,
    CancellationRequested,
    CleanupPending,
    CleanupFailed,
    TerminationConfirmed,
    Released,
}

impl InstanceInstallPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInstallTaskRef {
    pub instance_id: String,
    pub task_id: String,
}

/// The core task an install started, with the receipt that is its only handle
/// for mutation. Snapshots publish the reference; the receipt stays in the
/// install registry and never leaves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceInstallCoreTask {
    task: InstanceInstallTaskRef,
    receipt: crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt,
}

impl InstanceInstallCoreTask {
    pub fn new(
        task: InstanceInstallTaskRef,
        receipt: crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt,
    ) -> Self {
        Self { task, receipt }
    }

    pub fn task(&self) -> &InstanceInstallTaskRef {
        &self.task
    }

    pub fn receipt(
        &self,
    ) -> &crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt {
        &self.receipt
    }
}

/// A core-task start that failed. When the process started but its cleanup
/// failed, `created` is that task, which the install keeps until it confirms
/// the cleanup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceInstallCoreStartError {
    pub message: String,
    pub created: Option<Box<InstanceInstallCoreTask>>,
}

impl From<String> for InstanceInstallCoreStartError {
    fn from(message: String) -> Self {
        Self {
            message,
            created: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInstallOperation {
    pub operation_id: String,
    /// Canonical Task Activity id of this installation in the task center.
    pub activity_id: String,
    pub instance_id: String,
    pub phase: InstanceInstallPhase,
    pub created_at: u128,
    pub updated_at: u128,
    pub task: Option<InstanceInstallTaskRef>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    /// The Python a built-in type installs on; none for a declarative type,
    /// whose steps prepare what it runs on.
    pub python_version: Option<PythonVersion>,
    /// The step a declarative type's install is on.
    pub step: Option<InstallStepProgress>,
    pub cleanup_status: InstanceInstallCleanupStatus,
    pub cleanup_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InstanceInstallPreparation {
    pub instance_path: PathBuf,
    pub component_paths: Option<InstanceComponentPaths>,
    pub launcher_source_dir: PathBuf,
    pub env: Vec<(String, String)>,
    pub settings: ResolvedInstallSettings,
    pub catalog: WebUiInstallOptionCatalog,
}

/// What a declarative type's install runs with: the instance's layout and
/// the environment its steps get. Nothing in it needs Python.
#[derive(Clone, Debug)]
pub struct StepInstallPreparation {
    pub instance_path: PathBuf,
    pub components: InstanceComponentPaths,
    pub environment: StepEnvironment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreInstallTaskRequest {
    pub task: InstanceInstallTaskRef,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    /// Variables set (`Some`) or removed from what the task inherits (`None`).
    pub env: Vec<(String, Option<String>)>,
}

impl CoreInstallTaskRequest {
    /// The built-in install's task: the launcher's command, which sets its
    /// variables and removes none.
    pub(crate) fn from_run_spec(
        task: InstanceInstallTaskRef,
        run_spec: crate::internal::hanafubuki_webui::runtime::SdWebuiAllInOneRunSpec,
    ) -> Self {
        Self {
            task,
            command: run_spec.command,
            cwd: run_spec.cwd,
            env: run_spec
                .env
                .into_iter()
                .map(|(name, value)| (name, Some(value)))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstanceInstallTaskState {
    Running,
    Exited {
        exit_code: i32,
    },
    Failed {
        exit_code: Option<i32>,
        error: String,
    },
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstanceInstallStageError {
    Cancelled,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceInstallTaskCleanupResult {
    Disposed,
    AlreadyAbsent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceInstallPendingCleanupStatus {
    Pending,
    Retrying,
    Exhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceInstallPendingCleanupKind {
    Ownership,
    Eviction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInstallPendingCleanup {
    pub cleanup_id: String,
    pub task: InstanceInstallTaskRef,
    pub operation_id: Option<String>,
    pub kind: InstanceInstallPendingCleanupKind,
    pub status: InstanceInstallPendingCleanupStatus,
    pub attempts: u32,
    pub error: Option<String>,
    pub queued_tasks: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInstallLogOutput {
    pub chunks: Vec<InstanceInstallOutputChunk>,
    pub start_offset: u64,
    pub next_offset: u64,
    pub truncated: bool,
}
