use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::internal::hanafubuki_webui::install::{
    ComfyuiInstallSpec, FooocusInstallSpec, InstanceInstallCoreStartError, InvokeaiInstallSpec,
    MizuotoTtsInstallSpec, QwenTtsInstallSpec, RvcNextInstallSpec, SdTrainerInstallSpec,
    StableDiffusionWebuiInstallSpec,
};

struct FakeBackend {
    root: PathBuf,
    component_paths: Mutex<
        Option<crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths>,
    >,
    prepare_error: Mutex<Option<String>>,
    python_error: Mutex<Option<String>>,
    python_delay_ms: AtomicUsize,
    start_delay_ms: AtomicUsize,
    /// The core task starts, then fails in a way its cleanup cannot undo.
    start_retains_process: AtomicBool,
    python_targets: Mutex<Vec<PathBuf>>,
    core_requests: Mutex<Vec<CoreInstallTaskRequest>>,
    states: Mutex<VecDeque<InstanceInstallTaskState>>,
    task_error: Mutex<Option<String>>,
    terminated_tasks: Mutex<HashSet<String>>,
    termination_state: Mutex<InstanceInstallTaskState>,
    terminate_attempts: AtomicUsize,
    terminate_error: Mutex<Option<String>>,
    disposed: Mutex<Vec<InstanceInstallTaskRef>>,
    /// Core tasks as started, terminated, and disposed, receipts included.
    started_cores: Mutex<Vec<InstanceInstallCoreTask>>,
    terminated_cores: Mutex<Vec<InstanceInstallCoreTask>>,
    disposed_cores: Mutex<Vec<InstanceInstallCoreTask>>,
    dispose_attempts: Mutex<Vec<InstanceInstallTaskRef>>,
    dispose_results: Mutex<VecDeque<Result<InstanceInstallTaskCleanupResult, String>>>,
    output: Mutex<Vec<InstanceInstallOutputChunk>>,
}

impl FakeBackend {
    fn new(root: impl Into<PathBuf>, states: Vec<InstanceInstallTaskState>) -> Self {
        let root = absolute_fixture_path(root.into());
        Self {
            root,
            component_paths: Mutex::new(None),
            prepare_error: Mutex::new(None),
            python_error: Mutex::new(None),
            python_delay_ms: AtomicUsize::new(0),
            start_delay_ms: AtomicUsize::new(0),
            start_retains_process: AtomicBool::new(false),
            python_targets: Mutex::new(Vec::new()),
            core_requests: Mutex::new(Vec::new()),
            states: Mutex::new(states.into()),
            task_error: Mutex::new(None),
            terminated_tasks: Mutex::new(HashSet::new()),
            termination_state: Mutex::new(InstanceInstallTaskState::Cancelled),
            terminate_attempts: AtomicUsize::new(0),
            terminate_error: Mutex::new(None),
            disposed: Mutex::new(Vec::new()),
            started_cores: Mutex::new(Vec::new()),
            terminated_cores: Mutex::new(Vec::new()),
            disposed_cores: Mutex::new(Vec::new()),
            dispose_attempts: Mutex::new(Vec::new()),
            dispose_results: Mutex::new(VecDeque::new()),
            output: Mutex::new(vec![InstanceInstallOutputChunk {
                stream: InstanceInstallOutputStream::Stdout,
                data: b"core diagnostics".to_vec(),
            }]),
        }
    }
}

impl InstanceInstallBackend for FakeBackend {
    fn prepare(&self, instance_id: &str) -> Result<InstanceInstallPreparation, String> {
        if let Some(error) = self.prepare_error.lock().unwrap().take() {
            return Err(error);
        }
        let instance_path = if instance_id == "demo" {
            self.root.clone()
        } else {
            self.root.parent().unwrap().join(instance_id)
        };
        Ok(InstanceInstallPreparation {
            instance_path,
            component_paths: self.component_paths.lock().unwrap().clone(),
            launcher_source_dir: absolute_fixture_path(PathBuf::from(
                "app-data/runtime/launcher-code",
            )),
            env: vec![("INSTANCE_ENV".to_string(), "one".to_string())],
            settings: ResolvedInstallSettings::default(),
            catalog: WebUiInstallOptionCatalog::default(),
        })
    }

    fn install_catalog(&self) -> Result<WebUiInstallOptionCatalog, String> {
        Ok(WebUiInstallOptionCatalog::default())
    }

    fn ensure_python(
        &self,
        python_dir: PathBuf,
        _version: PythonVersion,
        cancellation: &CancellationToken,
    ) -> Result<PathBuf, InstanceInstallStageError> {
        self.python_targets.lock().unwrap().push(python_dir.clone());
        let deadline = Instant::now()
            + Duration::from_millis(self.python_delay_ms.load(Ordering::Acquire) as u64);
        while Instant::now() < deadline {
            if cancellation.is_cancelled() {
                return Err(InstanceInstallStageError::Cancelled);
            }
            thread::sleep(Duration::from_millis(2));
        }
        if let Some(error) = self.python_error.lock().unwrap().take() {
            return Err(InstanceInstallStageError::Failed(error));
        }
        Ok(python_dir.join("resolved-python"))
    }

    fn start_core_task(
        &self,
        request: CoreInstallTaskRequest,
    ) -> Result<InstanceInstallCoreTask, InstanceInstallCoreStartError> {
        let core = fake_core_task(request.task.clone());
        self.started_cores.lock().unwrap().push(core.clone());
        if self.start_retains_process.load(Ordering::Acquire) {
            return Err(InstanceInstallCoreStartError {
                message: "core task registration failed; ownership was retained".into(),
                created: Some(Box::new(core)),
            });
        }
        thread::sleep(Duration::from_millis(
            self.start_delay_ms.load(Ordering::Acquire) as u64,
        ));
        self.core_requests.lock().unwrap().push(request);
        Ok(core)
    }

    fn get_core_task(
        &self,
        task: &InstanceInstallCoreTask,
    ) -> Result<InstanceInstallTaskState, String> {
        if let Some(error) = self.task_error.lock().unwrap().take() {
            return Err(error);
        }
        if self
            .terminated_tasks
            .lock()
            .unwrap()
            .contains(&task.task().task_id)
        {
            return Ok(self.termination_state.lock().unwrap().clone());
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

    fn terminate_core_task(&self, task: &InstanceInstallCoreTask) -> Result<(), String> {
        self.terminate_attempts.fetch_add(1, Ordering::AcqRel);
        if let Some(error) = self.terminate_error.lock().unwrap().take() {
            return Err(error);
        }
        self.terminated_tasks
            .lock()
            .unwrap()
            .insert(task.task().task_id.clone());
        self.terminated_cores.lock().unwrap().push(task.clone());
        Ok(())
    }

    fn dispose_core_task(
        &self,
        task: &InstanceInstallCoreTask,
    ) -> Result<InstanceInstallTaskCleanupResult, String> {
        self.dispose_attempts
            .lock()
            .unwrap()
            .push(task.task().clone());
        let result = self
            .dispose_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(InstanceInstallTaskCleanupResult::Disposed));
        if result.is_ok() {
            self.disposed.lock().unwrap().push(task.task().clone());
            self.disposed_cores.lock().unwrap().push(task.clone());
        }
        result
    }

    fn read_core_task_output(
        &self,
        _task: &InstanceInstallTaskRef,
        from_offset: u64,
        max_bytes: Option<usize>,
    ) -> Result<InstanceInstallLogOutput, String> {
        let output = self.output.lock().unwrap();
        let total: usize = output.iter().map(|chunk| chunk.data.len()).sum();
        let start = usize::try_from(from_offset)
            .unwrap_or(usize::MAX)
            .min(total);
        let end = max_bytes
            .map(|max| start.saturating_add(max).min(total))
            .unwrap_or(total);
        let mut cursor = 0_usize;
        let mut chunks = Vec::new();
        for chunk in output.iter() {
            let chunk_end = cursor + chunk.data.len();
            let slice_start = start.saturating_sub(cursor).min(chunk.data.len());
            let slice_end = end.saturating_sub(cursor).min(chunk.data.len());
            if slice_start < slice_end {
                chunks.push(InstanceInstallOutputChunk {
                    stream: chunk.stream,
                    data: chunk.data[slice_start..slice_end].to_vec(),
                });
            }
            cursor = chunk_end;
        }
        Ok(InstanceInstallLogOutput {
            chunks,
            start_offset: start as u64,
            next_offset: end as u64,
            truncated: false,
        })
    }
}

fn absolute_fixture_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    let relative = path
        .to_string_lossy()
        .trim_start_matches(['/', '\\'])
        .to_string();
    crate::internal::hanafubuki_webui::test_support::temp_root().join(relative)
}

fn wait_terminal(service: &InstanceInstallService, operation_id: &str) -> InstanceInstallOperation {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let operation = service.get(operation_id).unwrap();
        if operation.phase.is_terminal() {
            return operation;
        }
        assert!(Instant::now() < deadline, "operation did not finish");
        thread::sleep(Duration::from_millis(5));
    }
}

fn wait_cleanup_status(
    service: &InstanceInstallService,
    operation_id: &str,
    status: InstanceInstallCleanupStatus,
) -> InstanceInstallOperation {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let operation = service.get(operation_id).unwrap();
        if operation.cleanup_status == status {
            return operation;
        }
        assert!(Instant::now() < deadline, "cleanup status did not change");
        thread::sleep(Duration::from_millis(2));
    }
}

fn wait_for_cleanups(service: &InstanceInstallService) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let pending = service.list_pending_cleanups();
        if pending.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "unfinished cleanups: {pending:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn wait_pending_status(
    service: &InstanceInstallService,
    status: InstanceInstallPendingCleanupStatus,
) -> InstanceInstallPendingCleanup {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(cleanup) = service
            .list_pending_cleanups()
            .into_iter()
            .find(|cleanup| cleanup.status == status)
        {
            return cleanup;
        }
        assert!(
            Instant::now() < deadline,
            "pending cleanup did not reach status"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn wait_pending_kind_status(
    service: &InstanceInstallService,
    kind: InstanceInstallPendingCleanupKind,
    status: InstanceInstallPendingCleanupStatus,
) -> InstanceInstallPendingCleanup {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(cleanup) = service
            .list_pending_cleanups()
            .into_iter()
            .find(|cleanup| cleanup.kind == kind && cleanup.status == status)
        {
            return cleanup;
        }
        assert!(Instant::now() < deadline, "pending cleanup did not change");
        thread::sleep(Duration::from_millis(2));
    }
}

fn fill_exhausted_eviction_cleanups(service: &InstanceInstallService) {
    let mut registry = service.inner.registry.lock();
    for index in 0..MAX_PENDING_EVICTION_CLEANUPS {
        let task = InstanceInstallTaskRef {
            instance_id: format!("evicted-{index}"),
            task_id: format!("task-{index}"),
        };
        let id = cleanup_id(&task);
        registry.pending_cleanups.insert(
            id.clone(),
            PendingCleanupEntry {
                info: InstanceInstallPendingCleanup {
                    cleanup_id: id,
                    task: task.clone(),
                    operation_id: Some(format!("evicted-operation-{index}")),
                    kind: InstanceInstallPendingCleanupKind::Eviction,
                    status: InstanceInstallPendingCleanupStatus::Exhausted,
                    attempts: CLEANUP_MAX_ATTEMPTS,
                    error: Some("injected exhausted eviction".to_string()),
                    queued_tasks: 0,
                },
                purpose: PendingCleanupPurpose::Evicted,
                in_progress: false,
                core_task: fake_core_task(task),
                queued_evictions: VecDeque::new(),
            },
        );
    }
}

fn assert_cancel_is_terminal_noop(
    service: &InstanceInstallService,
    operation_id: &str,
) -> InstanceInstallOperation {
    let before = service.get(operation_id).unwrap();
    let (token_before, owner_before) = {
        let registry = service.inner.registry.lock();
        (
            registry
                .operations
                .get(operation_id)
                .unwrap()
                .cancellation
                .is_cancelled(),
            registry
                .active_by_instance
                .get(&before.instance_id)
                .map(|owner| owner.lifecycle),
        )
    };
    let returned = service.cancel(operation_id).unwrap();
    let after = service.get(operation_id).unwrap();
    let (token_after, owner_after) = {
        let registry = service.inner.registry.lock();
        (
            registry
                .operations
                .get(operation_id)
                .unwrap()
                .cancellation
                .is_cancelled(),
            registry
                .active_by_instance
                .get(&before.instance_id)
                .map(|owner| owner.lifecycle),
        )
    };
    for snapshot in [&returned, &after] {
        assert_eq!(snapshot.phase, before.phase);
        assert_eq!(snapshot.updated_at, before.updated_at);
        assert_eq!(snapshot.cleanup_status, before.cleanup_status);
        assert_eq!(snapshot.cleanup_error, before.cleanup_error);
        assert_eq!(snapshot.error, before.error);
    }
    assert_eq!(token_after, token_before);
    assert_eq!(owner_after, owner_before);
    after
}

fn request(spec: WebUiInstallSpec) -> StartInstanceInstallRequest {
    StartInstanceInstallRequest {
        instance_id: "demo".to_string(),
        spec,
    }
}

fn family_specs() -> Vec<(WebUiInstallSpec, &'static str)> {
    vec![
        (
            WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
                pytorch: None,
                python_version: None,
                branch: None,
                predownload_models: None,
                predownload_extensions: None,
            }),
            "--sd-webui-path",
        ),
        (
            WebUiInstallSpec::Comfyui(ComfyuiInstallSpec {
                pytorch: None,
                python_version: None,
                predownload_models: None,
                predownload_extensions: None,
            }),
            "--comfyui-path",
        ),
        (
            WebUiInstallSpec::Invokeai(InvokeaiInstallSpec {
                pytorch: None,
                python_version: None,
                predownload_models: None,
            }),
            "--invokeai-path",
        ),
        (
            WebUiInstallSpec::Fooocus(FooocusInstallSpec {
                pytorch: None,
                python_version: None,
                branch: None,
                predownload_models: None,
            }),
            "--fooocus-path",
        ),
        (
            WebUiInstallSpec::SdTrainer(SdTrainerInstallSpec {
                pytorch: None,
                python_version: None,
                branch: None,
                predownload_models: None,
            }),
            "--sd-trainer-path",
        ),
        (
            WebUiInstallSpec::QwenTts(QwenTtsInstallSpec {
                pytorch: None,
                python_version: None,
                predownload_models: None,
            }),
            "--qwen-tts-webui-path",
        ),
        (
            WebUiInstallSpec::RvcNext(RvcNextInstallSpec {
                pytorch: None,
                python_version: None,
            }),
            "--rvc-next-webui-path",
        ),
        (
            WebUiInstallSpec::MizuotoTts(MizuotoTtsInstallSpec {
                pytorch: None,
                python_version: None,
            }),
            "--mizuoto-tts-webui-path",
        ),
    ]
}

mod cancellation;
mod cleanup;
mod operation;
mod output;
mod ownership;
mod reservation;
mod retention;

/// A core task carrying a receipt minted for the test, as a backend returns it.
pub(super) fn fake_core_task(task: InstanceInstallTaskRef) -> InstanceInstallCoreTask {
    let receipt = crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt::for_test(
        task.task_id.clone(),
        format!("instance:{}:{}", task.instance_id, task.task_id),
        crate::internal::hanafubuki_process::ownership::ProcessOwnerId::generate(),
    );
    InstanceInstallCoreTask::new(task, receipt)
}
