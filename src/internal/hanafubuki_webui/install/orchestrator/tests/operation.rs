use super::*;

#[test]
fn all_families_use_fixed_python_and_core_paths_and_runner_contract() {
    for (spec, path_flag) in family_specs() {
        let instance_path = absolute_fixture_path(PathBuf::from("instances/demo"));
        let python_path = instance_path.join("python");
        let core_path = instance_path.join("core");
        let launcher_source =
            absolute_fixture_path(PathBuf::from("app-data/runtime/launcher-code"));
        let backend = Arc::new(FakeBackend::new(
            instance_path.clone(),
            vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
        ));
        let service = InstanceInstallService::new(backend.clone());
        let started = service.start(request(spec)).unwrap();
        assert_eq!(
            wait_terminal(&service, &started.operation_id).phase,
            InstanceInstallPhase::Completed
        );

        assert_eq!(
            backend.python_targets.lock().unwrap().as_slice(),
            &[python_path.clone()]
        );
        let requests = backend.core_requests.lock().unwrap();
        let run = &requests[0];
        assert_eq!(run.cwd, instance_path);
        assert_eq!(
            &run.command[..3],
            &[
                python_path
                    .join("resolved-python")
                    .to_string_lossy()
                    .into_owned(),
                "-m".to_string(),
                "sd_webui_all_in_one".to_string(),
            ]
        );
        let flag_index = run.command.iter().position(|arg| arg == path_flag).unwrap();
        assert_eq!(
            run.command[flag_index + 1],
            core_path.to_string_lossy().as_ref()
        );
        let pythonpath = run
            .env
            .iter()
            .find_map(|(key, value)| (key == "PYTHONPATH").then_some(value.as_ref()))
            .flatten()
            .expect("PYTHONPATH");
        assert!(std::env::split_paths(pythonpath).any(|path| path == launcher_source));
        assert!(
            !run.command
                .iter()
                .any(|arg| arg.contains("pytorch") || arg.contains("branch") || arg == "--no-uv")
        );
    }
}

#[test]
fn custom_and_default_instance_roots_derive_their_own_canonical_children() {
    let base = crate::internal::hanafubuki_webui::test_support::temp_root().join(format!(
        "webui-install-canonical-paths-{}",
        std::process::id()
    ));
    for instance_path in [
        base.join("custom/location"),
        base.join("global-instances/demo"),
    ] {
        let backend = Arc::new(FakeBackend::new(
            instance_path.clone(),
            vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
        ));
        let service = InstanceInstallService::new(backend.clone());
        let started = service.start(request(family_specs().remove(0).0)).unwrap();
        assert_eq!(
            wait_terminal(&service, &started.operation_id).phase,
            InstanceInstallPhase::Completed
        );
        assert_eq!(
            backend.python_targets.lock().unwrap().as_slice(),
            &[instance_path.join("python")]
        );
        let expected_core = instance_path.join("core").to_string_lossy().into_owned();
        let requests = backend.core_requests.lock().unwrap();
        assert_eq!(requests[0].cwd, instance_path);
        assert!(requests[0].command.iter().any(|arg| arg == &expected_core));
    }
}

#[test]
fn external_core_install_keeps_python_managed_and_targets_the_existing_core() {
    use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;

    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let managed = temporary.path().join("managed");
    let core = temporary.path().join("external/core");
    std::fs::create_dir_all(&managed).unwrap();
    std::fs::create_dir_all(&core).unwrap();
    let backend = Arc::new(FakeBackend::new(
        managed.clone(),
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    *backend.component_paths.lock().unwrap() = Some(InstanceComponentPaths::python_and_core(
        managed.join("python"),
        core.clone(),
    ));
    let service = InstanceInstallService::new(backend.clone());
    let spec = family_specs().remove(1).0;

    let started = service.start(request(spec)).unwrap();
    assert_eq!(
        wait_terminal(&service, &started.operation_id).phase,
        InstanceInstallPhase::Completed
    );
    assert_eq!(
        backend.python_targets.lock().unwrap().as_slice(),
        &[managed.join("python")]
    );
    let requests = backend.core_requests.lock().unwrap();
    assert!(
        requests[0]
            .command
            .iter()
            .any(|argument| argument == core.to_string_lossy().as_ref())
    );
}

#[test]
fn preparation_and_python_failures_stop_before_the_next_stage() {
    let backend = Arc::new(FakeBackend::new("/instances/demo", vec![]));
    *backend.prepare_error.lock().unwrap() = Some("catalog unavailable".to_string());
    let service = InstanceInstallService::new(backend.clone());
    assert!(
        service
            .start(request(family_specs().remove(0).0))
            .unwrap_err()
            .contains("catalog")
    );
    assert!(backend.python_targets.lock().unwrap().is_empty());

    *backend.python_error.lock().unwrap() = Some("python extraction failed".to_string());
    let started = service.start(request(family_specs().remove(0).0)).unwrap();
    let failed = wait_terminal(&service, &started.operation_id);
    assert_eq!(failed.phase, InstanceInstallPhase::Failed);
    assert!(failed.error.unwrap().contains("python extraction"));
    assert!(backend.core_requests.lock().unwrap().is_empty());
}

#[test]
fn backend_owned_paths_and_task_identity_are_not_request_fields() {
    let json = r#"{
        "instanceId":"demo",
        "spec":{"type":"comfyui"},
        "targetPath":"/attacker/core",
        "pythonDir":"/attacker/python",
        "taskId":"chosen"
    }"#;
    let request = serde_json::from_str::<StartInstanceInstallRequest>(json).unwrap();
    assert_eq!(request.instance_id, "demo");
    assert_eq!(
        request.spec.family(),
        crate::internal::hanafubuki_instance_contracts::WebUiFamily::Comfyui
    );
}

#[test]
fn task_outcomes_and_logs_map_to_terminal_operations() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 17 }],
    ));
    let service = InstanceInstallService::new(backend);
    let started = service.start(request(family_specs().remove(1).0)).unwrap();
    let failed = wait_terminal(&service, &started.operation_id);
    assert_eq!(failed.phase, InstanceInstallPhase::Failed);
    assert_eq!(failed.exit_code, Some(17));
    assert_eq!(failed.error.as_deref(), Some("core diagnostics"));
    assert_eq!(
        service
            .read_output(&started.operation_id, 5, Some(4))
            .unwrap()
            .chunks
            .into_iter()
            .flat_map(|chunk| chunk.data)
            .collect::<Vec<_>>(),
        b"diag"
    );
}

#[test]
fn task_infrastructure_error_fails_the_operation() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    *backend.task_error.lock().unwrap() = Some("task registry unavailable".to_string());
    let service = InstanceInstallService::new(backend);
    let started = service.start(request(family_specs().remove(0).0)).unwrap();
    let failed = wait_terminal(&service, &started.operation_id);
    assert_eq!(failed.phase, InstanceInstallPhase::Failed);
    assert_eq!(failed.error.as_deref(), Some("task registry unavailable"));
}
