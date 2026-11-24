use super::*;
use crate::internal::hanafubuki_python_runtime::PythonVersion;
use crate::internal::hanafubuki_webui::install::{
    PytorchInstallSelection, WebUiFamilyInstallOptionCatalog,
};

fn assert_reinstall_preserves_user_arguments(name: &str, user_arguments: Vec<String>) {
    let (root, service) = setup(
        name,
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    let instance = create(&service, "User arguments");
    service.start_install(&instance.instance.id).unwrap();
    let ready = wait_lifecycle(&service, &instance.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });

    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    let config = ConfigState::default();
    let user = update_instance_launch_args_inner(
        &paths,
        &config,
        &ready.instance.id,
        ready.instance.config_revision,
        user_arguments.clone(),
    )
    .unwrap();
    service
        .start_reinstall(ReinstallWebUiInstanceRequest {
            instance_id: ready.instance.id.clone(),
            expected_instance_identity: ready.install.instance_identity.clone(),
            expected_config_revision: user.config_revision,
            force_recreate_python: false,
            force_recreate_core: false,
            spec: None,
        })
        .unwrap();
    let reinstalled = wait_lifecycle(&service, &ready.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert_eq!(
        reinstalled.instance.launch_argument_origin,
        LaunchArgumentOrigin::User
    );
    assert_eq!(reinstalled.instance.launch_args, user_arguments);
    assert_eq!(reinstalled.instance.config_revision, user.config_revision);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn reinstall_preserves_user_owned_empty_and_nonempty_arguments() {
    assert_reinstall_preserves_user_arguments("reinstall-user-empty-arguments", Vec::new());
    assert_reinstall_preserves_user_arguments(
        "reinstall-user-nonempty-arguments",
        vec!["--user-value".into(), "custom".into()],
    );
}

#[test]
fn reinstall_allows_core_equal_to_instance_root_without_forced_recreation() {
    let (root, service, backend) = setup_with_backend(
        "reinstall-root-core-without-recreation",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    let install_dir = root.join("existing-instance");
    let python_dir = install_dir.join("python");
    std::fs::create_dir_all(&python_dir).unwrap();
    let imported = service
        .import_existing(
            ImportWebUiInstanceRequest {
                name: Some("Root Core".to_string()),
                install_dir: install_dir.clone(),
                component_paths: std::collections::BTreeMap::from([
                    ("python".to_string(), python_dir),
                    ("core".to_string(), install_dir),
                ]),
                type_id: type_of(&spec()),
                spec: spec().to_object(),
            },
            &AcceptInstancePython,
        )
        .unwrap();
    *backend.preparation.lock().unwrap() = Some(InstanceInstallPreparation {
        instance_path: imported.install_dir.clone(),
        component_paths: imported.instance.component_paths.clone(),
        launcher_source_dir: root.join("launcher-code"),
        env: Vec::new(),
        settings: ResolvedInstallSettings::default(),
        catalog: WebUiInstallOptionCatalog::default(),
    });

    let forced_error = service
        .start_reinstall(ReinstallWebUiInstanceRequest {
            instance_id: imported.instance.id.clone(),
            expected_instance_identity: imported.install.instance_identity.clone(),
            expected_config_revision: imported.instance.config_revision,
            force_recreate_python: false,
            force_recreate_core: true,
            spec: None,
        })
        .unwrap_err();
    assert!(forced_error.contains("Core path equal to the instance directory cannot be deleted"));
    // The switch reports the same refusal before the user confirms anything.
    assert_eq!(
        imported.force_recreate_core_action,
        WebUiInstanceInstallAction::Unavailable {
            reason: "Core path equal to the instance directory cannot be deleted on its own"
                .to_string(),
        }
    );
    assert_eq!(
        imported.force_recreate_python_action,
        WebUiInstanceInstallAction::Available
    );

    service
        .start_reinstall(ReinstallWebUiInstanceRequest {
            instance_id: imported.instance.id.clone(),
            expected_instance_identity: imported.install.instance_identity.clone(),
            expected_config_revision: imported.instance.config_revision,
            force_recreate_python: false,
            force_recreate_core: false,
            spec: None,
        })
        .unwrap();

    let reinstalled = wait_lifecycle(&service, &imported.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert_eq!(
        reinstalled.instance.component_paths.unwrap().core(),
        reinstalled.install_dir
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn standard_attachment_reinstalls_without_recreation_keeps_an_external_core() {
    let (root, service, backend) = setup_with_backend(
        "reinstall-standard-external-core",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    let install_dir = root.join("instance");
    let python_dir = install_dir.join("python");
    let core_path = root.join("attached/core");
    std::fs::create_dir_all(&python_dir).unwrap();
    std::fs::create_dir_all(&core_path).unwrap();
    std::fs::write(core_path.join("keep.txt"), b"keep").unwrap();
    let imported = service
        .import_existing(
            ImportWebUiInstanceRequest {
                name: Some("Attached Core".to_string()),
                install_dir,
                component_paths: std::collections::BTreeMap::from([
                    ("python".to_string(), python_dir),
                    ("core".to_string(), core_path.clone()),
                ]),
                type_id: type_of(&spec()),
                spec: spec().to_object(),
            },
            &AcceptInstancePython,
        )
        .unwrap();
    assert_eq!(
        imported.reinstall_action,
        WebUiInstanceInstallAction::Available
    );
    assert_eq!(
        imported.force_recreate_core_action,
        WebUiInstanceInstallAction::Available
    );
    *backend.preparation.lock().unwrap() = Some(InstanceInstallPreparation {
        instance_path: imported.install_dir.clone(),
        component_paths: imported.instance.component_paths.clone(),
        launcher_source_dir: root.join("launcher-code"),
        env: Vec::new(),
        settings: ResolvedInstallSettings::default(),
        catalog: WebUiInstallOptionCatalog::default(),
    });

    service
        .start_reinstall(ReinstallWebUiInstanceRequest {
            instance_id: imported.instance.id.clone(),
            expected_instance_identity: imported.install.instance_identity.clone(),
            expected_config_revision: imported.instance.config_revision,
            force_recreate_python: false,
            force_recreate_core: false,
            spec: None,
        })
        .unwrap();
    let reinstalled = wait_lifecycle(&service, &imported.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert_eq!(
        reinstalled.instance.component_paths.unwrap().core(),
        core_path
    );
    assert_eq!(std::fs::read(core_path.join("keep.txt")).unwrap(), b"keep");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn forced_core_rebuild_deletes_an_external_core_before_installing() {
    let (root, service, backend) = setup_with_backend(
        "reinstall-forced-external-core",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    let install_dir = root.join("instance");
    let python_dir = install_dir.join("python");
    let core_path = root.join("attached/core");
    std::fs::create_dir_all(&python_dir).unwrap();
    std::fs::create_dir_all(&core_path).unwrap();
    std::fs::write(core_path.join("stale.txt"), b"stale").unwrap();
    let imported = service
        .import_existing(
            ImportWebUiInstanceRequest {
                name: Some("Attached Core".to_string()),
                install_dir,
                component_paths: std::collections::BTreeMap::from([
                    ("python".to_string(), python_dir),
                    ("core".to_string(), core_path.clone()),
                ]),
                type_id: type_of(&spec()),
                spec: spec().to_object(),
            },
            &AcceptInstancePython,
        )
        .unwrap();
    *backend.preparation.lock().unwrap() = Some(InstanceInstallPreparation {
        instance_path: imported.install_dir.clone(),
        component_paths: imported.instance.component_paths.clone(),
        launcher_source_dir: root.join("launcher-code"),
        env: Vec::new(),
        settings: ResolvedInstallSettings::default(),
        catalog: WebUiInstallOptionCatalog::default(),
    });

    service
        .start_reinstall(ReinstallWebUiInstanceRequest {
            instance_id: imported.instance.id.clone(),
            expected_instance_identity: imported.install.instance_identity.clone(),
            expected_config_revision: imported.instance.config_revision,
            force_recreate_python: false,
            force_recreate_core: true,
            spec: None,
        })
        .unwrap();
    wait_lifecycle(&service, &imported.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });

    assert_eq!(
        backend.removed_components.lock().unwrap().as_slice(),
        [("core".to_string(), core_path.clone())]
    );
    assert!(!core_path.join("stale.txt").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn branch_switch_refreshes_only_builtin_owned_arguments() {
    let (root, service) = setup("branch-default-refresh", vec![]);
    let install_dir = root.join("existing-instance");
    let python_dir = install_dir.join("python");
    let core_path = install_dir.join("core");
    std::fs::create_dir_all(&python_dir).unwrap();
    std::fs::create_dir_all(&core_path).unwrap();
    let imported = service
        .import_existing(
            ImportWebUiInstanceRequest {
                name: Some("Stable".to_string()),
                install_dir,
                component_paths: std::collections::BTreeMap::from([
                    ("python".to_string(), python_dir),
                    ("core".to_string(), core_path),
                ]),
                type_id: type_of(&stable_spec("sd_webui_main")),
                spec: stable_spec("sd_webui_main").to_object(),
            },
            &AcceptInstancePython,
        )
        .unwrap();

    service
        .persist_branch_preset_if_identity(
            &imported.instance.id,
            &imported.install.instance_identity,
            "sd_next_main".to_string(),
        )
        .unwrap();
    let switched = service.get(&imported.instance.id).unwrap();
    assert_eq!(
        switched.instance.launch_args,
        default_launch_arguments(&stable_spec("sd_next_main"))
    );
    assert_eq!(
        switched.instance.launch_argument_origin,
        LaunchArgumentOrigin::BuiltIn
    );

    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    let config = ConfigState::default();
    let user = update_instance_launch_args_inner(
        &paths,
        &config,
        &switched.instance.id,
        switched.instance.config_revision,
        Vec::new(),
    )
    .unwrap();
    service
        .persist_branch_preset_if_identity(
            &switched.instance.id,
            &switched.install.instance_identity,
            "sd_webui_main".to_string(),
        )
        .unwrap();
    let preserved = service.get(&switched.instance.id).unwrap();
    assert_eq!(
        preserved.instance.launch_argument_origin,
        LaunchArgumentOrigin::User
    );
    assert!(preserved.instance.launch_args.is_empty());
    assert_eq!(preserved.instance.config_revision, user.config_revision);
    let _ = std::fs::remove_dir_all(root);
}

fn stable_catalog(branches: &[&str]) -> WebUiInstallOptionCatalog {
    let mut catalog = WebUiInstallOptionCatalog::default();
    catalog.families.insert(
        WebUiFamily::StableDiffusionWebui,
        WebUiFamilyInstallOptionCatalog::new(
            WebUiFamily::StableDiffusionWebui,
            ["cu128".to_string()].into(),
            branches.iter().map(|branch| branch.to_string()).collect(),
        ),
    );
    catalog
}

fn installed_stable(
    name: &str,
    branch: &str,
) -> (
    PathBuf,
    WebUiInstanceService,
    Arc<FakeBackend>,
    WebUiInstanceSnapshot,
) {
    let (root, service, backend) = setup_with_backend(
        name,
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    *backend.catalog.lock().unwrap() = stable_catalog(&["sd_webui_main", "sd_next_main"]);
    let created = service
        .create(CreateWebUiInstanceRequest {
            name: Some("Stable".to_string()),
            install_dir: None,
            component_paths: Default::default(),
            type_id: type_of(&stable_spec(branch)),
            spec: stable_spec(branch).to_object(),
            preset: None,
        })
        .unwrap();
    service.start_install(&created.instance.id).unwrap();
    let ready = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    (root, service, backend, ready)
}

fn reinstall_request(
    snapshot: &WebUiInstanceSnapshot,
    force_recreate_python: bool,
    spec: Option<WebUiInstallSpec>,
) -> ReinstallWebUiInstanceRequest {
    ReinstallWebUiInstanceRequest {
        instance_id: snapshot.instance.id.clone(),
        expected_instance_identity: snapshot.install.instance_identity.clone(),
        expected_config_revision: snapshot.instance.config_revision,
        force_recreate_python,
        force_recreate_core: false,
        spec: spec.map(|spec| spec.to_object()),
    }
}

fn stable_spec_with(
    branch: &str,
    python_version: Option<PythonVersion>,
    pytorch: Option<&str>,
    predownload_extensions: Option<bool>,
) -> WebUiInstallSpec {
    WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
        pytorch: pytorch.map(|value| PytorchInstallSelection::try_from(value.to_string()).unwrap()),
        python_version,
        branch: Some(BranchSelection::try_from(branch.to_string()).unwrap()),
        predownload_models: None,
        predownload_extensions,
    })
}

#[test]
fn reinstall_installs_with_and_records_a_changed_spec() {
    let (root, service, backend, ready) =
        installed_stable("reinstall-changed-spec", "sd_webui_main");
    assert_eq!(
        ready.reinstall_spec_action,
        WebUiInstanceInstallAction::Available
    );
    let changed = stable_spec_with("sd_next_main", None, None, Some(false));

    service
        .start_reinstall(reinstall_request(&ready, false, Some(changed.clone())))
        .unwrap();
    // The record names the new spec from the moment the install starts, so a
    // retry after a failed reinstall installs what the user chose.
    assert_eq!(
        *service
            .get(&ready.instance.id)
            .unwrap()
            .install
            .install_spec()
            .unwrap(),
        changed
    );
    let reinstalled = wait_lifecycle(&service, &ready.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });

    assert_eq!(*reinstalled.install.install_spec().unwrap(), changed);
    let command = backend
        .core_requests
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .command
        .clone();
    let branch = command
        .iter()
        .position(|arg| arg == "--install-branch")
        .map(|index| command[index + 1].as_str());
    assert_eq!(branch, Some("sd_next_main"));
    assert!(
        command
            .iter()
            .any(|arg| arg == "--no-pre-download-extension")
    );
    // Built-in launch arguments follow the recorded branch.
    assert_eq!(
        reinstalled.instance.launch_args,
        default_launch_arguments(&changed)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn reinstall_without_a_spec_replays_the_recorded_one() {
    let (root, service, backend, ready) =
        installed_stable("reinstall-recorded-spec", "sd_next_main");

    service
        .start_reinstall(reinstall_request(&ready, false, None))
        .unwrap();
    let reinstalled = wait_lifecycle(&service, &ready.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });

    assert_eq!(
        *reinstalled.install.install_spec().unwrap(),
        stable_spec("sd_next_main")
    );
    let requests = backend.core_requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].command, requests[1].command);
    let _ = std::fs::remove_dir_all(root);
}

/// Put a runtime where the instance's Python lives, as a finished Python
/// install leaves it; the fake backend installs nothing there.
fn establish_python_environment(
    service: &WebUiInstanceService,
    snapshot: &WebUiInstanceSnapshot,
) -> WebUiInstanceSnapshot {
    let python_dir = snapshot
        .instance
        .component_paths
        .as_ref()
        .unwrap()
        .python()
        .unwrap();
    let executable =
        crate::internal::hanafubuki_python_runtime::standalone_python_executable(python_dir);
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::write(&executable, b"").unwrap();
    let established = service.get(&snapshot.instance.id).unwrap();
    assert!(established.python_environment_established);
    established
}

#[test]
fn python_environment_changes_require_recreating_python() {
    let (root, service, backend, ready) =
        installed_stable("reinstall-python-environment", "sd_webui_main");
    let ready = establish_python_environment(&service, &ready);
    let python_changed =
        stable_spec_with("sd_webui_main", Some(PythonVersion::Python312), None, None);
    let pytorch_changed = stable_spec_with("sd_webui_main", None, Some("cu128"), None);

    for spec in [python_changed.clone(), pytorch_changed] {
        let error = service
            .start_reinstall(reinstall_request(&ready, false, Some(spec)))
            .unwrap_err();
        assert!(error.contains("Python environment is recreated"), "{error}");
    }
    // The family default named explicitly is the environment already installed.
    let default_named =
        stable_spec_with("sd_webui_main", Some(PythonVersion::Python311), None, None);
    service
        .validate_reinstall_request(&reinstall_request(&ready, false, Some(default_named)))
        .unwrap();
    assert_eq!(backend.core_requests.lock().unwrap().len(), 1);

    service
        .start_reinstall(reinstall_request(
            &ready,
            true,
            Some(python_changed.clone()),
        ))
        .unwrap();
    let reinstalled = wait_lifecycle(&service, &ready.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert_eq!(*reinstalled.install.install_spec().unwrap(), python_changed);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_reinstall_specs_are_refused_before_anything_changes() {
    let (root, service, backend, ready) =
        installed_stable("reinstall-invalid-spec", "sd_webui_main");

    // The spec is read by the instance's own type, which refuses a field it
    // does not declare.
    let mut undeclared = reinstall_request(&ready, false, None);
    undeclared.spec = serde_json::json!({ "pytorchFamily": "cuda" })
        .as_object()
        .cloned();
    let field_error = service.start_reinstall(undeclared).unwrap_err();
    assert!(
        field_error
            .starts_with("Stable Diffusion WebUI install spec: unknown field `pytorchFamily`"),
        "{field_error}"
    );
    // A branch the current catalog lacks is refused by validation, which the
    // lifecycle coordinator runs before it quiesces the instance.
    let catalog_error = service
        .validate_reinstall_request(&reinstall_request(
            &ready,
            false,
            Some(stable_spec("sd_webui_forge")),
        ))
        .unwrap_err();
    assert!(catalog_error.contains("sd_webui_forge"), "{catalog_error}");

    let unchanged = service.get(&ready.instance.id).unwrap();
    assert_eq!(unchanged.install, ready.install);
    assert_eq!(backend.core_requests.lock().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn snapshot_instances_reinstall_only_with_the_snapshot_parameters() {
    let (root, service) = setup(
        "reinstall-snapshot-spec",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    std::fs::create_dir_all(&root).unwrap();
    let snapshot_path = root.join("comfy.json");
    let platform = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        _ => "linux",
    };
    std::fs::write(
        &snapshot_path,
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "webui": { "type": "comfyui" },
            "python": { "version": "3.12.4", "implementation": "CPython", "platform": platform }
        }))
        .unwrap(),
    )
    .unwrap();
    let created = service
        .create_from_snapshot(CreateWebUiInstanceFromSnapshotRequest {
            name: Some("Snapshot".to_string()),
            install_dir: None,
            component_paths: Default::default(),
            snapshot_path,
            expected_type:
                crate::internal::hanafubuki_webui::instance_types::BuiltinProvider::type_id(
                    WebUiFamily::Comfyui,
                ),
        })
        .unwrap();
    service.start_install(&created.instance.id).unwrap();
    let ready = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });

    assert_eq!(
        ready.reinstall_action,
        WebUiInstanceInstallAction::Available
    );
    let WebUiInstanceInstallAction::Unavailable { reason } = &ready.reinstall_spec_action else {
        panic!("snapshot instance offered editable reinstall parameters");
    };
    let error = service
        .start_reinstall(reinstall_request(
            &ready,
            false,
            Some(ready.install.install_spec().unwrap().clone()),
        ))
        .unwrap_err();
    assert_eq!(&error, reason);
    service
        .start_reinstall(reinstall_request(&ready, false, None))
        .unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn reinstall_request_spec_is_optional_on_the_wire() {
    let request: ReinstallWebUiInstanceRequest = serde_json::from_value(serde_json::json!({
        "instanceId": "a",
        "expectedInstanceIdentity": "identity",
        "expectedConfigRevision": 1
    }))
    .unwrap();
    assert_eq!(request.spec, None);
    let request: ReinstallWebUiInstanceRequest = serde_json::from_value(serde_json::json!({
        "instanceId": "a",
        "expectedInstanceIdentity": "identity",
        "expectedConfigRevision": 1,
        "spec": { "predownloadExtensions": false }
    }))
    .unwrap();
    assert_eq!(
        request.spec,
        serde_json::json!({ "predownloadExtensions": false })
            .as_object()
            .cloned()
    );
}

#[test]
fn python_environment_changes_need_no_rebuild_before_a_runtime_exists() {
    let (root, service, _backend, ready) =
        installed_stable("reinstall-python-absent", "sd_webui_main");
    assert!(!ready.python_environment_established);
    let changed = stable_spec_with(
        "sd_webui_main",
        Some(PythonVersion::Python312),
        Some("cu128"),
        None,
    );

    service
        .start_reinstall(reinstall_request(&ready, false, Some(changed.clone())))
        .unwrap();
    let reinstalled = wait_lifecycle(&service, &ready.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert_eq!(*reinstalled.install.install_spec().unwrap(), changed);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_unstarted_install_can_start_with_different_parameters() {
    let (root, service, backend) = setup_with_backend(
        "resume-created-with-spec",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    *backend.catalog.lock().unwrap() = stable_catalog(&["sd_webui_main", "sd_next_main"]);
    let created = service
        .create(CreateWebUiInstanceRequest {
            name: Some("Stable".to_string()),
            install_dir: None,
            component_paths: Default::default(),
            type_id: type_of(&stable_spec("sd_webui_main")),
            spec: stable_spec("sd_webui_main").to_object(),
            preset: None,
        })
        .unwrap();
    assert_eq!(created.install.lifecycle, WebUiInstanceLifecycle::Created);
    assert_eq!(
        created.reinstall_spec_action,
        WebUiInstanceInstallAction::Available
    );
    let changed = stable_spec_with("sd_next_main", Some(PythonVersion::Python312), None, None);

    service
        .start_reinstall(reinstall_request(&created, false, Some(changed.clone())))
        .unwrap();
    let installed = wait_lifecycle(&service, &created.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert_eq!(*installed.install.install_spec().unwrap(), changed);
    let command = &backend.core_requests.lock().unwrap()[0].command;
    let branch = command
        .iter()
        .position(|arg| arg == "--install-branch")
        .map(|index| command[index + 1].as_str());
    assert_eq!(branch, Some("sd_next_main"));
    let _ = std::fs::remove_dir_all(root);
}
