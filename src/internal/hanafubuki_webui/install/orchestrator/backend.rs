use std::path::PathBuf;

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;

use crate::internal::hanafubuki_python_runtime::PythonVersion;

use super::super::WebUiInstallOptionCatalog;
use super::model::{
    CoreInstallTaskRequest, InstanceInstallCoreStartError, InstanceInstallCoreTask,
    InstanceInstallLogOutput, InstanceInstallPreparation, InstanceInstallStageError,
    InstanceInstallTaskCleanupResult, InstanceInstallTaskRef, InstanceInstallTaskState,
};

/// Project-owned adapter boundary. Production implementations delegate Python
/// preparation to the standalone runtime manager and core execution to Instance
/// Task; tests can exercise orchestration without a filesystem or process.
pub trait InstanceInstallBackend: Send + Sync + 'static {
    /// Resolve the generic instance, ensure app runtime/launcher-code and the App
    /// API server, fetch the current catalog, and resolve effective settings.
    fn prepare(&self, instance_id: &str) -> Result<InstanceInstallPreparation, String>;

    /// Fetch the current install-selection catalog alone, for checking a spec
    /// before any instance is reserved.
    fn install_catalog(&self) -> Result<WebUiInstallOptionCatalog, String>;

    /// Resolve the instance and the environment a declarative type's steps
    /// run with. Unlike [`Self::prepare`], it needs no Python runtime.
    fn prepare_steps(
        &self,
        _instance_id: &str,
    ) -> Result<super::model::StepInstallPreparation, String> {
        Err("declarative install steps are unavailable for this backend".to_string())
    }

    /// The Git the `git-clone` and `git-pull` steps run, once the Git
    /// runtime has it: while it prepares, this waits until it is ready, it
    /// failed, or the run's `cancellation` is cancelled.
    fn git_executable(&self, _cancellation: &CancellationToken) -> Result<PathBuf, String> {
        Err("Git is unavailable for this backend".to_string())
    }

    fn ensure_python(
        &self,
        python_dir: PathBuf,
        version: PythonVersion,
        cancellation: &CancellationToken,
    ) -> Result<PathBuf, InstanceInstallStageError>;

    fn remove_component_path(
        &self,
        _component: &str,
        _path: &std::path::Path,
    ) -> Result<(), String> {
        Err("component path deletion is unavailable for this backend".to_string())
    }

    /// Start the core task and return it with its receipt; every later
    /// status, terminate, and dispose call names the task by that receipt.
    fn start_core_task(
        &self,
        request: CoreInstallTaskRequest,
    ) -> Result<InstanceInstallCoreTask, InstanceInstallCoreStartError>;
    fn get_core_task(
        &self,
        task: &InstanceInstallCoreTask,
    ) -> Result<InstanceInstallTaskState, String>;
    fn terminate_core_task(&self, task: &InstanceInstallCoreTask) -> Result<(), String>;
    fn dispose_core_task(
        &self,
        task: &InstanceInstallCoreTask,
    ) -> Result<InstanceInstallTaskCleanupResult, String>;
    fn read_core_task_output(
        &self,
        task: &InstanceInstallTaskRef,
        from_offset: u64,
        max_bytes: Option<usize>,
    ) -> Result<InstanceInstallLogOutput, String>;
}
