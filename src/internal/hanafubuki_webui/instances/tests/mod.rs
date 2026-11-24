use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use crate::internal::hanafubuki_config::repository::instances::{
    InstanceComponentPaths, update_instance_launch_args_inner,
};
use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_webui::install::{
    BranchSelection, ComfyuiInstallSpec, CoreInstallTaskRequest, InstanceInstallBackend,
    InstanceInstallCoreStartError, InstanceInstallCoreTask, InstanceInstallPreparation,
    InstanceInstallStageError, InstanceInstallTaskCleanupResult, InstanceInstallTaskRef,
    InstanceInstallTaskState, ResolvedInstallSettings, StableDiffusionWebuiInstallSpec,
    WebUiInstallOptionCatalog,
};
use crate::internal::hanafubuki_webui::instance_types::InstanceTypeRegistry;
use crate::internal::hanafubuki_webui::instance_types::presets::PresetCatalog;

struct FakeBackend {
    root: PathBuf,
    preparation: Mutex<Option<InstanceInstallPreparation>>,
    catalog: Mutex<WebUiInstallOptionCatalog>,
    python_targets: Mutex<Vec<PathBuf>>,
    core_requests: Mutex<Vec<CoreInstallTaskRequest>>,
    removed_components: Mutex<Vec<(String, PathBuf)>>,
    states: Mutex<VecDeque<InstanceInstallTaskState>>,
    terminated: Mutex<bool>,
}

impl FakeBackend {
    fn new(root: PathBuf, states: Vec<InstanceInstallTaskState>) -> Self {
        Self {
            root,
            preparation: Mutex::new(None),
            catalog: Mutex::new(WebUiInstallOptionCatalog::default()),
            python_targets: Mutex::new(Vec::new()),
            core_requests: Mutex::new(Vec::new()),
            removed_components: Mutex::new(Vec::new()),
            states: Mutex::new(states.into()),
            terminated: Mutex::new(false),
        }
    }
}

impl InstanceInstallBackend for FakeBackend {
    fn prepare(&self, _instance_id: &str) -> Result<InstanceInstallPreparation, String> {
        if let Some(preparation) = self.preparation.lock().unwrap().clone() {
            return Ok(preparation);
        }
        Ok(InstanceInstallPreparation {
            instance_path: self.root.clone(),
            component_paths: None,
            launcher_source_dir: self.root.join("launcher-code"),
            env: Vec::new(),
            settings: ResolvedInstallSettings::default(),
            catalog: self.catalog.lock().unwrap().clone(),
        })
    }

    fn install_catalog(&self) -> Result<WebUiInstallOptionCatalog, String> {
        Ok(self.catalog.lock().unwrap().clone())
    }

    fn ensure_python(
        &self,
        python_dir: PathBuf,
        _version: crate::internal::hanafubuki_python_runtime::PythonVersion,
        cancellation: &CancellationToken,
    ) -> Result<PathBuf, InstanceInstallStageError> {
        self.python_targets.lock().unwrap().push(python_dir.clone());
        if cancellation.is_cancelled() {
            Err(InstanceInstallStageError::Cancelled)
        } else {
            Ok(python_dir.join("python"))
        }
    }

    fn remove_component_path(&self, component: &str, path: &std::path::Path) -> Result<(), String> {
        self.removed_components
            .lock()
            .unwrap()
            .push((component.to_string(), path.to_path_buf()));
        crate::internal::hanafubuki_webui::instances::delete_configured_directory(path)
    }

    fn start_core_task(
        &self,
        request: CoreInstallTaskRequest,
    ) -> Result<InstanceInstallCoreTask, InstanceInstallCoreStartError> {
        let core = fake_core_task(request.task.clone());
        self.core_requests.lock().unwrap().push(request);
        Ok(core)
    }

    fn get_core_task(
        &self,
        _task: &InstanceInstallCoreTask,
    ) -> Result<InstanceInstallTaskState, String> {
        if *self.terminated.lock().unwrap() {
            return Ok(InstanceInstallTaskState::Cancelled);
        }
        let mut states = self.states.lock().unwrap();
        if states.len() > 1 {
            Ok(states.pop_front().unwrap())
        } else {
            Ok(states
                .front()
                .cloned()
                .unwrap_or(InstanceInstallTaskState::Running))
        }
    }

    fn terminate_core_task(&self, _task: &InstanceInstallCoreTask) -> Result<(), String> {
        *self.terminated.lock().unwrap() = true;
        Ok(())
    }

    fn dispose_core_task(
        &self,
        _task: &InstanceInstallCoreTask,
    ) -> Result<InstanceInstallTaskCleanupResult, String> {
        Ok(InstanceInstallTaskCleanupResult::Disposed)
    }

    fn read_core_task_output(
        &self,
        _task: &InstanceInstallTaskRef,
        from_offset: u64,
        max_bytes: Option<usize>,
    ) -> Result<InstanceInstallLogOutput, String> {
        let bytes = b"install output";
        let start = usize::try_from(from_offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = start
            .saturating_add(max_bytes.unwrap_or(usize::MAX))
            .min(bytes.len());
        Ok(InstanceInstallLogOutput {
            chunks: if start < end {
                vec![crate::internal::hanafubuki_task::output::OutputDelta {
                    stream: crate::internal::hanafubuki_task::output::OutputStream::Stdout,
                    data: bytes[start..end].to_vec(),
                }]
            } else {
                Vec::new()
            },
            start_offset: start as u64,
            next_offset: end as u64,
            truncated: false,
        })
    }
}

fn spec() -> WebUiInstallSpec {
    WebUiInstallSpec::Comfyui(ComfyuiInstallSpec {
        pytorch: None,
        python_version: None,
        predownload_models: None,
        predownload_extensions: None,
    })
}

fn stable_spec(branch: &str) -> WebUiInstallSpec {
    WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
        pytorch: None,
        python_version: None,
        branch: Some(BranchSelection::try_from(branch.to_string()).unwrap()),
        predownload_models: None,
        predownload_extensions: None,
    })
}

fn type_of(
    spec: &WebUiInstallSpec,
) -> crate::internal::hanafubuki_instance_contracts::InstanceTypeId {
    crate::internal::hanafubuki_webui::instance_types::BuiltinProvider::type_id(spec.family())
}

fn setup(name: &str, states: Vec<InstanceInstallTaskState>) -> (PathBuf, WebUiInstanceService) {
    let (root, service, _) = setup_with_backend(name, states);
    (root, service)
}

fn setup_with_backend(
    name: &str,
    states: Vec<InstanceInstallTaskState>,
) -> (PathBuf, WebUiInstanceService, Arc<FakeBackend>) {
    let root = crate::internal::hanafubuki_webui::test_support::temp_root().join(format!(
        "hanafubuki-webui-instances-{name}-{}-{}",
        std::process::id(),
        NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    let config = Arc::new(ConfigState::default());
    let backend = Arc::new(FakeBackend::new(root.join("install-root"), states));
    let installs = InstanceInstallService::new(backend.clone());
    (
        root,
        WebUiInstanceService::new(
            repository(
                paths,
                config,
                Arc::new(InstanceTypeRegistry::with_builtin_types()),
            ),
            installs,
        )
        .unwrap(),
        backend,
    )
}

/// The repository over `types`, its presets those of `paths`' user file.
fn repository(
    paths: AppPaths,
    config: Arc<ConfigState>,
    types: Arc<InstanceTypeRegistry>,
) -> WebUiInstanceRepository {
    let presets = Arc::new(PresetCatalog::new(
        types.clone(),
        Arc::default(),
        paths.instance_presets_file(),
    ));
    WebUiInstanceRepository::new(paths, config, types, presets)
}

fn create(service: &WebUiInstanceService, name: &str) -> WebUiInstanceSnapshot {
    service
        .create(CreateWebUiInstanceRequest {
            name: Some(name.to_string()),
            install_dir: None,
            component_paths: Default::default(),
            type_id: type_of(&spec()),
            spec: spec().to_object(),
            preset: None,
        })
        .unwrap()
}

struct AcceptInstancePython;

impl InstancePythonValidator for AcceptInstancePython {
    fn validate(&self, _instance_id: &str, _python_dir: &Path) -> Result<(), String> {
        Ok(())
    }
}

fn wait_lifecycle(
    service: &WebUiInstanceService,
    instance_id: &str,
    predicate: impl Fn(&WebUiInstanceLifecycle) -> bool,
) -> WebUiInstanceSnapshot {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = service.get(instance_id).unwrap();
        if predicate(&snapshot.install.lifecycle) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "lifecycle did not update");
        thread::sleep(Duration::from_millis(5));
    }
}

mod attach_only;
mod creation;
mod declarative;
mod default_name;
mod deletion;
mod discovered;
mod external;
mod import;
mod install;
mod instance_types;
mod maintenance_visibility;
mod presets;
mod reinstall;
mod rename;
mod snapshot;

/// A core task carrying a receipt minted for the test, as a backend returns it.
pub(super) fn fake_core_task(task: InstanceInstallTaskRef) -> InstanceInstallCoreTask {
    let receipt = crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt::for_test(
        task.task_id.clone(),
        format!("instance:{}:{}", task.instance_id, task.task_id),
        crate::internal::hanafubuki_process::ownership::ProcessOwnerId::generate(),
    );
    InstanceInstallCoreTask::new(task, receipt)
}
