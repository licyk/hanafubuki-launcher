//! A declarative type through the instance service: its steps install the
//! instance, a failing step leaves its files as they were, and a ready
//! instance updates through its update steps.

use crate::internal::hanafubuki_foundation::module_faults::ContributionOwner;
use std::io::{Read, Write as _};
use std::net::TcpListener;

use crate::internal::hanafubuki_config::repository::instances::get_resolved_instance_config_inner;
use sha2::{Digest, Sha256};

use super::*;
use crate::internal::hanafubuki_webui::install::steps::StepEnvironment;
use crate::internal::hanafubuki_webui::install::{
    InstanceInstallOperation, InstanceInstallPhase, StepInstallPreparation,
};
use crate::internal::hanafubuki_webui::instance_types::DeclarativeProvider;
use crate::internal::hanafubuki_webui::instance_types::declarative_fixtures as fixtures;
use serde_json::Value;

/// The fake backend, with what a declarative install needs from the
/// application: the instance's layout and its steps' environment.
pub(super) struct StepsBackend {
    inner: FakeBackend,
    paths: AppPaths,
    config: Arc<ConfigState>,
    output: Mutex<Vec<u8>>,
    /// Git is preparing: [`InstanceInstallBackend::git_executable`] waits
    /// until the run's cancellation, as the Git runtime's wait does.
    git_preparing: std::sync::atomic::AtomicBool,
    /// A step is waiting for Git.
    git_waiting: std::sync::atomic::AtomicBool,
}

impl InstanceInstallBackend for StepsBackend {
    fn prepare(&self, instance_id: &str) -> Result<InstanceInstallPreparation, String> {
        self.inner.prepare(instance_id)
    }
    fn install_catalog(&self) -> Result<WebUiInstallOptionCatalog, String> {
        self.inner.install_catalog()
    }
    fn prepare_steps(&self, instance_id: &str) -> Result<StepInstallPreparation, String> {
        let resolved = get_resolved_instance_config_inner(&self.paths, &self.config, instance_id)?;
        Ok(StepInstallPreparation {
            instance_path: resolved.install_dir,
            components: resolved.component_paths,
            environment: StepEnvironment::default(),
        })
    }
    fn git_executable(&self, cancellation: &CancellationToken) -> Result<PathBuf, String> {
        if self.git_preparing.load(Ordering::SeqCst) {
            self.git_waiting.store(true, Ordering::SeqCst);
            while !cancellation.wait_timeout(Duration::from_millis(5)) {}
            return Err("waiting for the Git runtime was abandoned".to_string());
        }
        Ok(PathBuf::from("git-of-the-git-runtime"))
    }
    fn ensure_python(
        &self,
        python_dir: PathBuf,
        version: crate::internal::hanafubuki_python_runtime::PythonVersion,
        cancellation: &CancellationToken,
    ) -> Result<PathBuf, InstanceInstallStageError> {
        self.inner.ensure_python(python_dir, version, cancellation)
    }
    fn remove_component_path(&self, component: &str, path: &std::path::Path) -> Result<(), String> {
        self.inner.remove_component_path(component, path)
    }
    fn start_core_task(
        &self,
        request: CoreInstallTaskRequest,
    ) -> Result<InstanceInstallCoreTask, InstanceInstallCoreStartError> {
        self.inner.start_core_task(request)
    }
    fn get_core_task(
        &self,
        task: &InstanceInstallCoreTask,
    ) -> Result<InstanceInstallTaskState, String> {
        self.inner.get_core_task(task)
    }
    fn terminate_core_task(&self, task: &InstanceInstallCoreTask) -> Result<(), String> {
        self.inner.terminate_core_task(task)
    }
    fn dispose_core_task(
        &self,
        task: &InstanceInstallCoreTask,
    ) -> Result<InstanceInstallTaskCleanupResult, String> {
        self.inner.dispose_core_task(task)
    }
    fn read_core_task_output(
        &self,
        _task: &InstanceInstallTaskRef,
        _from_offset: u64,
        _max_bytes: Option<usize>,
    ) -> Result<InstanceInstallLogOutput, String> {
        Ok(InstanceInstallLogOutput {
            chunks: vec![crate::internal::hanafubuki_task::output::OutputDelta {
                stream: crate::internal::hanafubuki_task::output::OutputStream::Stdout,
                data: self.output.lock().unwrap().clone(),
            }],
            start_offset: 0,
            next_offset: 0,
            truncated: false,
        })
    }
}

pub(super) fn setup_type(
    name: &str,
    descriptor: &str,
    states: Vec<InstanceInstallTaskState>,
) -> (PathBuf, WebUiInstanceService, Arc<StepsBackend>) {
    let root = crate::internal::hanafubuki_webui::test_support::temp_root().join(format!(
        "hanafubuki-webui-declarative-{name}-{}-{}",
        std::process::id(),
        NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    let config = Arc::new(ConfigState::default());
    let backend = Arc::new(StepsBackend {
        inner: FakeBackend::new(root.join("install-root"), states),
        paths: paths.clone(),
        config: config.clone(),
        output: Mutex::new(Vec::new()),
        git_preparing: Default::default(),
        git_waiting: Default::default(),
    });
    let types = InstanceTypeRegistry::with_builtin_types();
    types
        .register(
            ContributionOwner::Extension(fixtures::NAMESPACE.to_string()),
            Arc::new(DeclarativeProvider::from_toml(fixtures::NAMESPACE, descriptor).unwrap()),
        )
        .unwrap();
    let service = WebUiInstanceService::new(
        super::repository(paths, config, Arc::new(types)),
        InstanceInstallService::new(backend.clone()),
    )
    .unwrap();
    (root, service, backend)
}

/// Serves `body` to every request on a loopback port.
fn serve(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/site.zip", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.read(&mut [0_u8; 4096]);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    url
}

fn site(index: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("index.html", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(index.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

fn create_declarative(
    service: &WebUiInstanceService,
    url: &str,
    sha256: &str,
) -> WebUiInstanceSnapshot {
    service
        .create(CreateWebUiInstanceRequest {
            name: None,
            install_dir: None,
            component_paths: Default::default(),
            type_id: crate::internal::hanafubuki_instance_contracts::InstanceTypeId::parse(
                "test.fixtures:two-servers",
            )
            .unwrap(),
            spec: serde_json::json!({ "archiveUrl": url, "sha256": sha256 })
                .as_object()
                .unwrap()
                .clone(),
            preset: None,
        })
        .unwrap()
}

fn reinstall(
    service: &WebUiInstanceService,
    snapshot: &WebUiInstanceSnapshot,
    spec: Option<Value>,
) -> Result<InstanceInstallOperation, String> {
    service.start_reinstall(ReinstallWebUiInstanceRequest {
        instance_id: snapshot.instance.id.clone(),
        expected_instance_identity: snapshot.install.instance_identity.clone(),
        expected_config_revision: snapshot.instance.config_revision,
        force_recreate_python: false,
        force_recreate_core: false,
        spec: spec.map(|spec| spec.as_object().unwrap().clone()),
    })
}

/// Records each install run's completion, as `<instance> <identity>`.
#[derive(Default)]
struct Completions(Mutex<Vec<String>>);

impl InstallCompletionSink for Completions {
    fn completed(&self, instance_id: &str, instance_identity: &str) {
        self.0
            .lock()
            .unwrap()
            .push(format!("{instance_id} {instance_identity}"));
    }
}

impl Completions {
    /// The completions told once `count` were, which the monitor tells
    /// just after it records the run ready.
    fn wait_for(&self, count: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.0.lock().unwrap().len() < count {
            assert!(Instant::now() < deadline, "no completion was told");
            thread::sleep(Duration::from_millis(10));
        }
        self.0.lock().unwrap().clone()
    }
}

#[test]
fn a_declarative_type_installs_by_its_steps_and_updates_by_its_update_steps() {
    let (root, service, _) = setup_type("install", fixtures::TWO_SERVERS, vec![]);
    let completions = Arc::new(Completions::default());
    let service = service.with_install_completions(completions.clone());
    let archive = site("<h1>v1</h1>");
    let url = serve(archive.clone());
    let created = create_declarative(&service, &url, &hex::encode(Sha256::digest(&archive)));
    assert!(created.install.is_declarative());
    assert!(
        created.instance.name.starts_with("Two servers "),
        "{}",
        created.instance.name
    );
    let components = created.instance.component_paths.clone().unwrap();
    assert_eq!(components.names().collect::<Vec<_>>(), ["core", "data"]);

    let operation = service.start_install(&created.instance.id).unwrap();
    assert_eq!(operation.python_version, None);
    let ready = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    let finished = service.get_install(&operation.operation_id).unwrap();
    assert_eq!(finished.phase, InstanceInstallPhase::Completed);
    assert_eq!(
        finished.step.map(|step| (step.index, step.count)),
        Some((2, 2))
    );
    let core = components.core();
    assert_eq!(
        std::fs::read_to_string(core.join("index.html")).unwrap(),
        "<h1>v1</h1>"
    );
    let data = components.get("data").unwrap();
    assert_eq!(
        std::fs::read_to_string(data.join("settings.txt")).unwrap(),
        format!("flavour=plain instance={}", created.instance.id)
    );
    // A declarative type's servers take no launch arguments.
    assert!(ready.instance.launch_args.is_empty());
    // Its completion is told once, however often the run is read after.
    let completed = format!(
        "{} {}",
        created.instance.id, created.install.instance_identity
    );
    assert_eq!(completions.wait_for(1), [completed.clone()]);
    service.get_install(&operation.operation_id).unwrap();
    assert_eq!(completions.wait_for(1), [completed.clone()]);

    reinstall(&service, &ready, None).unwrap();
    wait_lifecycle(
        &service,
        &created.instance.id,
        |lifecycle| matches!(lifecycle, WebUiInstanceLifecycle::Ready { operation_id } if *operation_id != finished.operation_id),
    );
    assert_eq!(
        std::fs::read_to_string(data.join("updated.txt")).unwrap(),
        "updated"
    );
    // The update is an install run of its own, of the same identity.
    assert_eq!(completions.wait_for(2), [completed.clone(), completed]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_failing_step_fails_the_install_and_leaves_the_files_as_they_were() {
    let (root, service, _) = setup_type("failing", fixtures::TWO_SERVERS, vec![]);
    let archive = site("<h1>v1</h1>");
    let url = serve(archive.clone());
    let created = create_declarative(&service, &url, &"0".repeat(64));
    let core = created
        .instance
        .component_paths
        .clone()
        .unwrap()
        .core()
        .to_path_buf();
    std::fs::create_dir_all(&core).unwrap();
    std::fs::write(core.join("index.html"), "<h1>kept</h1>").unwrap();

    service.start_install(&created.instance.id).unwrap();
    let failed = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::InstallFailed { .. })
    });
    let WebUiInstanceLifecycle::InstallFailed { error, phase, .. } = failed.install.lifecycle
    else {
        unreachable!()
    };
    assert_eq!(phase, InstanceInstallPhase::Failed);
    let error = error.unwrap();
    assert!(
        error.starts_with("step 1 of 2 (download) failed: "),
        "{error}"
    );
    assert!(error.contains("SHA-256 mismatch"), "{error}");
    assert_eq!(
        std::fs::read_to_string(core.join("index.html")).unwrap(),
        "<h1>kept</h1>"
    );
    assert_eq!(std::fs::read_dir(&core).unwrap().count(), 1);
    assert!(
        !failed
            .instance
            .component_paths
            .unwrap()
            .get("data")
            .unwrap()
            .join("settings.txt")
            .exists()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_run_step_is_a_task_of_the_install_and_its_failure_keeps_the_end_of_its_output() {
    let descriptor = fixtures::TWO_SERVERS.replacen(
        "[[update.steps]]",
        "[[install.steps]]\nkind = \"run\"\ncommand = [\"{component.core}/setup\", \"--for\", \"{instance.id}\"]\nenv = { STEP = \"1\" }\n\n[[update.steps]]",
        1,
    );
    let (root, service, backend) = setup_type(
        "run",
        &descriptor,
        vec![InstanceInstallTaskState::Exited { exit_code: 3 }],
    );
    *backend.output.lock().unwrap() = [b"x".repeat(10_000), b"the reason".to_vec()].concat();
    let archive = site("<h1>v1</h1>");
    let url = serve(archive.clone());
    let created = create_declarative(&service, &url, &hex::encode(Sha256::digest(&archive)));
    let operation = service.start_install(&created.instance.id).unwrap();
    let failed = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::InstallFailed { .. })
    });
    let requests = backend.inner.core_requests.lock().unwrap().clone();
    let [request] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    assert!(
        request.task.task_id.ends_with("-step-3"),
        "{}",
        request.task.task_id
    );
    assert_eq!(
        service.get_install(&operation.operation_id).unwrap().task,
        Some(request.task.clone())
    );
    let core = created
        .instance
        .component_paths
        .unwrap()
        .core()
        .to_path_buf();
    assert_eq!(
        request.command,
        [
            format!("{}/setup", core.to_string_lossy()),
            "--for".to_string(),
            created.instance.id.clone()
        ]
    );
    assert_eq!(request.cwd, core);
    assert!(
        request
            .env
            .contains(&("STEP".to_string(), Some("1".to_string())))
    );
    let WebUiInstanceLifecycle::InstallFailed {
        error, exit_code, ..
    } = failed.install.lifecycle
    else {
        unreachable!()
    };
    assert_eq!(exit_code, Some(3));
    let error = error.unwrap();
    assert!(
        error.starts_with("step 3 of 3 (run) failed: it exited with 3\n…"),
        "{error}"
    );
    assert!(error.ends_with("the reason"), "{error}");
    assert!(error.len() < 4_200, "{}", error.len());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_ready_instance_of_a_type_without_update_is_not_updated() {
    let descriptor = fixtures::TWO_SERVERS.replace("operations = [\"update\"]", "operations = []");
    let (root, service, _) = setup_type("no-update", &descriptor, vec![]);
    let archive = site("<h1>v1</h1>");
    let url = serve(archive.clone());
    let created = create_declarative(&service, &url, &hex::encode(Sha256::digest(&archive)));
    service.start_install(&created.instance.id).unwrap();
    let ready = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert_eq!(
        reinstall(&service, &ready, None).unwrap_err(),
        "instance type `test.fixtures:two-servers` declares no update"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_git_pull_step_is_the_git_runtimes_git_as_a_task_of_the_update() {
    let descriptor = fixtures::TWO_SERVERS.replace(
        "kind = \"write-file\"\npath = \"{component.data}/updated.txt\"\ncontent = \"updated\"",
        "kind = \"git-pull\"\ndir = \"{component.core}\"",
    );
    assert!(descriptor.contains("git-pull"));
    let (root, service, backend) = setup_type(
        "git-pull",
        &descriptor,
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    let archive = site("<h1>v1</h1>");
    let url = serve(archive.clone());
    let created = create_declarative(&service, &url, &hex::encode(Sha256::digest(&archive)));
    service.start_install(&created.instance.id).unwrap();
    let ready = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    let installed = ready.install.lifecycle.clone();
    reinstall(&service, &ready, None).unwrap();
    wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. }) && *lifecycle != installed
    });
    let requests = backend.inner.core_requests.lock().unwrap().clone();
    let [pull] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    assert_eq!(
        pull.command,
        ["git-of-the-git-runtime", "pull", "--ff-only"]
    );
    assert_eq!(pull.cwd, ready.instance.component_paths.unwrap().core());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_cancel_while_a_git_step_waits_for_the_git_runtime_cancels_the_update() {
    let descriptor = fixtures::TWO_SERVERS.replace(
        "kind = \"write-file\"\npath = \"{component.data}/updated.txt\"\ncontent = \"updated\"",
        "kind = \"git-pull\"\ndir = \"{component.core}\"",
    );
    let (root, service, backend) = setup_type("git-wait", &descriptor, vec![]);
    let archive = site("<h1>v1</h1>");
    let url = serve(archive.clone());
    let created = create_declarative(&service, &url, &hex::encode(Sha256::digest(&archive)));
    service.start_install(&created.instance.id).unwrap();
    let ready = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });

    backend.git_preparing.store(true, Ordering::SeqCst);
    let update = reinstall(&service, &ready, None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !backend.git_waiting.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "the step never asked for Git");
        thread::sleep(Duration::from_millis(5));
    }
    service.cancel_install(&update.operation_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let ended = loop {
        let operation = service.get_install(&update.operation_id).unwrap();
        if operation.phase.is_terminal() {
            break operation;
        }
        assert!(Instant::now() < deadline, "the update never ended");
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(ended.phase, InstanceInstallPhase::Cancelled);
    assert!(backend.inner.core_requests.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_shutdown_ends_a_git_step_waiting_for_the_git_runtime_as_a_cancel_does() {
    let descriptor = fixtures::TWO_SERVERS.replace(
        "kind = \"write-file\"\npath = \"{component.data}/updated.txt\"\ncontent = \"updated\"",
        "kind = \"git-pull\"\ndir = \"{component.core}\"",
    );
    let (root, service, backend) = setup_type("git-quit", &descriptor, vec![]);
    let archive = site("<h1>v1</h1>");
    let url = serve(archive.clone());
    let created = create_declarative(&service, &url, &hex::encode(Sha256::digest(&archive)));
    service.start_install(&created.instance.id).unwrap();
    let ready = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });

    backend.git_preparing.store(true, Ordering::SeqCst);
    let update = reinstall(&service, &ready, None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !backend.git_waiting.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "the step never asked for Git");
        thread::sleep(Duration::from_millis(5));
    }
    let bound = Duration::from_secs(5);
    let started = Instant::now();
    let diagnostics = service.shutdown_installs(started + bound);
    assert!(started.elapsed() < bound);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let ended = service.installs.get(&update.operation_id).unwrap();
    assert_eq!(ended.phase, InstanceInstallPhase::Cancelled);
    // Recorded by the shutdown itself, as a Cancel's monitor records it.
    match service
        .repository
        .load_record(&created.instance.id)
        .unwrap()
        .lifecycle
    {
        WebUiInstanceLifecycle::InstallFailed {
            operation_id,
            phase,
            error,
            ..
        } => {
            assert_eq!(operation_id, update.operation_id);
            assert_eq!(phase, InstanceInstallPhase::Cancelled);
            assert_eq!(error.as_deref(), Some("installation cancelled"));
        }
        other => panic!("{other:?}"),
    }
    assert!(backend.inner.core_requests.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}
