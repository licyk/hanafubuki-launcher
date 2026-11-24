use super::*;

#[test]
fn discovered_attachment_uses_the_inferred_existing_instance_root() {
    let (root, service) = setup("discovered-attachment", vec![]);
    let external = root.join("external");
    let core = external.join("core");
    let python = external.join("python");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::create_dir_all(&python).unwrap();

    let prepared = service
        .prepare_discovered(PrepareDiscoveredWebUiRequest {
            family: WebUiFamily::Comfyui,
            instance_path: external.clone(),
            core_path: core.clone(),
            python_dir: python.clone(),
        })
        .unwrap();
    assert_eq!(prepared.install_dir(), external);
    let imported = service
        .commit_discovered(prepared, &AcceptInstancePython)
        .unwrap();
    assert!(imported.install_dir.is_dir());
    let components = imported.instance.component_paths.unwrap();
    assert_eq!(components.core(), core);
    assert_eq!(components.python().unwrap(), python);
    assert!(matches!(
        imported.install.lifecycle,
        WebUiInstanceLifecycle::Ready { .. }
    ));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn failed_discovered_commit_preserves_the_existing_instance_root() {
    struct RejectPython;
    impl InstancePythonValidator for RejectPython {
        fn validate(&self, _instance_id: &str, _python_dir: &Path) -> Result<(), String> {
            Err("invalid Python".to_string())
        }
    }

    let (root, service) = setup("discovered-invalid-python", vec![]);
    let external = root.join("external");
    let core = external.join("core");
    let python = external.join("python");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::create_dir_all(&python).unwrap();
    let prepared = service
        .prepare_discovered(PrepareDiscoveredWebUiRequest {
            family: WebUiFamily::Comfyui,
            instance_path: external.clone(),
            core_path: core.clone(),
            python_dir: python.clone(),
        })
        .unwrap();
    let instance_root = prepared.install_dir().to_path_buf();
    assert!(service.commit_discovered(prepared, &RejectPython).is_err());
    assert!(instance_root.is_dir());
    assert!(core.is_dir());
    assert!(python.is_dir());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn discovered_core_without_python_is_created_with_a_managed_python_target() {
    let (root, service) = setup("discovered-core-only", vec![]);
    let core = root.join("external/core");
    std::fs::create_dir_all(&core).unwrap();

    let prepared = service
        .prepare_discovered_core(PrepareDiscoveredCoreRequest {
            family: WebUiFamily::Comfyui,
            core_path: core.clone(),
        })
        .unwrap();
    let managed = prepared.install_dir().to_path_buf();
    let imported = service.commit_discovered_core(prepared).unwrap();

    assert_eq!(imported.install_dir, managed);
    assert!(matches!(
        imported.install.install_source,
        WebUiInstanceInstallSource::DiscoveredCore
    ));
    assert!(matches!(
        imported.install.lifecycle,
        WebUiInstanceLifecycle::Created
    ));
    let components = imported.instance.component_paths.unwrap();
    assert_eq!(components.core(), core);
    assert_eq!(components.python().unwrap(), managed.join("python"));
    assert_eq!(
        imported.instance.launch_argument_origin,
        LaunchArgumentOrigin::PendingDefault
    );
    assert!(imported.instance.launch_args.is_empty());
    assert_eq!(
        imported.install_action,
        WebUiInstanceInstallAction::Available
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn pending_discovered_core_rejects_an_external_python_path_update() {
    let (root, service) = setup("discovered-core-path-update", vec![]);
    let core = root.join("external/core");
    let external_python = root.join("external/python");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::create_dir_all(&external_python).unwrap();
    let prepared = service
        .prepare_discovered_core(PrepareDiscoveredCoreRequest {
            family: WebUiFamily::Comfyui,
            core_path: core.clone(),
        })
        .unwrap();
    let imported = service.commit_discovered_core(prepared).unwrap();
    let error = service
        .update_component_paths(&UpdateWebUiInstanceComponentPathsRequest {
            instance_id: imported.instance.id.clone(),
            expected_config_revision: imported.instance.config_revision,
            component_paths: InstanceComponentPaths::python_and_core(external_python, core),
        })
        .unwrap_err();
    assert!(error.contains("requires Python inside"));

    let unchanged = service.get(&imported.instance.id).unwrap();
    assert_eq!(
        unchanged.install_action,
        WebUiInstanceInstallAction::Available
    );
    assert_eq!(
        unchanged
            .instance
            .component_paths
            .unwrap()
            .python()
            .unwrap(),
        imported.install_dir.join("python")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn discovered_core_created_instance_continues_through_the_complete_installer() {
    let (root, service, backend) = setup_with_backend(
        "discovered-core-install",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    let core = root.join("external/core");
    std::fs::create_dir_all(&core).unwrap();
    let prepared = service
        .prepare_discovered_core(PrepareDiscoveredCoreRequest {
            family: WebUiFamily::Comfyui,
            core_path: core.clone(),
        })
        .unwrap();
    let imported = service.commit_discovered_core(prepared).unwrap();
    let instance_id = imported.instance.id.clone();
    let canonical_core = imported
        .instance
        .component_paths
        .as_ref()
        .unwrap()
        .core()
        .to_path_buf();
    let python = imported
        .instance
        .component_paths
        .as_ref()
        .unwrap()
        .python()
        .unwrap()
        .to_path_buf();
    *backend.preparation.lock().unwrap() = Some(InstanceInstallPreparation {
        instance_path: imported.install_dir.clone(),
        component_paths: imported.instance.component_paths.clone(),
        launcher_source_dir: root.join("launcher-code"),
        env: Vec::new(),
        settings: ResolvedInstallSettings::default(),
        catalog: WebUiInstallOptionCatalog::default(),
    });

    service.start_install(&instance_id).unwrap();
    let ready = wait_lifecycle(&service, &instance_id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });

    assert_eq!(backend.python_targets.lock().unwrap().as_slice(), &[python]);
    assert!(
        backend.core_requests.lock().unwrap()[0]
            .command
            .iter()
            .any(|argument| argument == canonical_core.to_string_lossy().as_ref())
    );
    assert_eq!(
        ready.instance.launch_argument_origin,
        LaunchArgumentOrigin::BuiltIn
    );
    assert!(!ready.instance.launch_args.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn concurrent_discovered_plans_leave_one_instance_and_clean_the_loser() {
    let (root, service) = setup("discovered-race", vec![]);
    let core = root.join("external/core");
    let python = root.join("external/python");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::create_dir_all(&python).unwrap();
    let request = || PrepareDiscoveredWebUiRequest {
        family: WebUiFamily::Comfyui,
        instance_path: root.join("external"),
        core_path: core.clone(),
        python_dir: python.clone(),
    };
    let first = service.prepare_discovered(request()).unwrap();
    let second = service.prepare_discovered(request()).unwrap();
    let second_root = second.install_dir().to_path_buf();
    service
        .commit_discovered(first, &AcceptInstancePython)
        .unwrap();
    assert!(
        service
            .commit_discovered(second, &AcceptInstancePython)
            .is_err()
    );
    assert!(second_root.exists());
    assert_eq!(service.list().unwrap().len(), 1);
    assert!(core.is_dir());
    assert!(python.is_dir());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn discovered_attachment_accepts_core_as_root_with_nested_python() {
    let (root, service) = setup("discovered-nested-python", vec![]);
    let core = root.join("external/core");
    let python = core.join("python");
    std::fs::create_dir_all(&python).unwrap();

    let prepared = service
        .prepare_discovered(PrepareDiscoveredWebUiRequest {
            family: WebUiFamily::Comfyui,
            instance_path: core.clone(),
            core_path: core.clone(),
            python_dir: python.clone(),
        })
        .unwrap();
    let imported = service
        .commit_discovered(prepared, &AcceptInstancePython)
        .unwrap();

    assert_eq!(imported.install_dir, core);
    let components = imported.instance.component_paths.unwrap();
    assert_eq!(components.core(), imported.install_dir);
    assert_eq!(components.python().unwrap(), python);
    let _ = std::fs::remove_dir_all(root);
}
