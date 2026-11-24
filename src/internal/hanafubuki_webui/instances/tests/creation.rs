use super::*;

#[test]
fn creation_resolves_default_and_explicit_paths_and_keeps_generic_config_neutral() {
    let (root, service) = setup("paths", vec![]);
    let created = create(&service, "  My   Comfy UI  ");
    assert_eq!(created.instance.name, "My Comfy UI");
    assert!(created.install_dir.ends_with(&created.instance.id));
    assert_eq!(
        created.instance.install_dir,
        Some(created.install_dir.clone())
    );
    let components = created.instance.component_paths.as_ref().unwrap();
    assert_eq!(
        components.python().unwrap(),
        created.install_dir.join("python")
    );
    assert_eq!(components.core(), created.install_dir.join("core"));
    assert!(
        !serde_json::to_value(components)
            .unwrap()
            .to_string()
            .contains("ownership")
    );
    assert!(created.instance.launch_args.is_empty());
    assert_eq!(
        created.instance.launch_argument_origin,
        LaunchArgumentOrigin::PendingDefault
    );
    let generic = serde_json::to_value(&created.instance).unwrap();
    assert!(generic.get("spec").is_none());
    assert!(generic.get("lifecycle").is_none());

    let explicit = root.join("custom-install");
    let second = service
        .create(CreateWebUiInstanceRequest {
            name: Some("Explicit".to_string()),
            install_dir: Some(explicit.clone()),
            component_paths: Default::default(),
            type_id: type_of(&spec()),
            spec: spec().to_object(),
            preset: None,
        })
        .unwrap();
    assert_eq!(second.install_dir, explicit);
    assert_eq!(
        second.instance.install_dir,
        Some(second.install_dir.clone())
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_absent_or_blank_name_is_generated_from_the_webui_product() {
    let (root, service) = setup("generated-name", vec![]);

    // Omitted entirely.
    let generated = service
        .create(CreateWebUiInstanceRequest {
            name: None,
            install_dir: None,
            component_paths: Default::default(),
            type_id: type_of(&spec()),
            spec: spec().to_object(),
            preset: None,
        })
        .unwrap();
    assert!(
        generated.instance.name.starts_with("ComfyUI "),
        "{}",
        generated.instance.name
    );

    // Blank input is treated the same as omitting it.
    let blank = service
        .create(CreateWebUiInstanceRequest {
            name: Some("   ".to_string()),
            install_dir: None,
            component_paths: Default::default(),
            type_id: type_of(&spec()),
            spec: spec().to_object(),
            preset: None,
        })
        .unwrap();
    assert!(
        blank.instance.name.starts_with("ComfyUI "),
        "{}",
        blank.instance.name
    );

    // The random suffix keeps concurrently created instances distinguishable.
    assert_ne!(generated.instance.name, blank.instance.name);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn creation_rejects_invalid_names_dangerous_paths_and_conflicts() {
    let (root, service) = setup("invalid", vec![]);
    // A provided name still has to be usable; blank is not invalid, it simply
    // asks the backend to generate one (covered separately).
    assert!(
        service
            .create(CreateWebUiInstanceRequest {
                name: Some("bad\u{7}name".to_string()),
                install_dir: None,
                component_paths: Default::default(),
                type_id: type_of(&spec()),
                spec: spec().to_object(),
                preset: None,
            })
            .is_err()
    );
    assert!(
        service
            .create(CreateWebUiInstanceRequest {
                name: Some("Root".to_string()),
                install_dir: Some(PathBuf::from("/")),
                component_paths: Default::default(),
                type_id: type_of(&spec()),
                spec: spec().to_object(),
                preset: None,
            })
            .is_err()
    );
    let shared = root.join("shared");
    service
        .create(CreateWebUiInstanceRequest {
            name: Some("One".to_string()),
            install_dir: Some(shared.clone()),
            component_paths: Default::default(),
            type_id: type_of(&spec()),
            spec: spec().to_object(),
            preset: None,
        })
        .unwrap();
    let error = service
        .create(CreateWebUiInstanceRequest {
            name: Some("Two".to_string()),
            install_dir: Some(shared.join("nested")),
            component_paths: Default::default(),
            type_id: type_of(&spec()),
            spec: spec().to_object(),
            preset: None,
        })
        .unwrap_err();
    assert!(error.contains("conflicts"), "{error}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn creation_rolls_back_lifecycle_metadata_without_touching_user_directory() {
    let (root, service) = setup("rollback", vec![]);
    let user_dir = root.join("user-owned");
    std::fs::create_dir_all(&user_dir).unwrap();
    std::fs::write(user_dir.join("keep.txt"), b"keep").unwrap();
    let id = "rollback-instance".to_string();
    let error = service
        .create_with_id(
            id.clone(),
            "Rollback".to_string(),
            Some(user_dir.clone()),
            Default::default(),
            type_of(&spec()),
            InstanceSpec::Builtin(spec()),
            WebUiInstanceInstallSource::Standard,
            Some(&serde_json::json!({})),
            None,
        )
        .unwrap_err();
    assert!(error.contains("unexpectedly supplied snapshot data"));
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    assert!(!paths.instance_lifecycle_file(&id).exists());
    assert!(!paths.instance_config_file(&id).exists());
    assert!(!paths.instance_record_dir(&id).exists());
    assert_eq!(std::fs::read(user_dir.join("keep.txt")).unwrap(), b"keep");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn same_id_recreation_gets_a_new_identity() {
    let (root, service) = setup("identity-recreation", vec![]);
    let id = "demo".to_string();
    let first = service
        .create_with_id(
            id.clone(),
            "Demo".to_string(),
            Some(root.join("first")),
            Default::default(),
            type_of(&spec()),
            InstanceSpec::Builtin(spec()),
            WebUiInstanceInstallSource::Standard,
            None,
            None,
        )
        .unwrap();
    let first_identity = first.install.instance_identity.clone();
    assert!(!first_identity.is_empty());

    let lifecycle_path = AppPaths::from_app_data_dir(root.clone())
        .unwrap()
        .instance_lifecycle_file(&id);
    assert_eq!(
        lifecycle_path.parent(),
        AppPaths::from_app_data_dir(root.clone())
            .unwrap()
            .instance_config_file(&id)
            .parent()
    );
    service
        .delete_state_if_identity(&id, &first_identity)
        .unwrap();
    assert!(!lifecycle_path.parent().unwrap().exists());
    let recreated = service
        .create_with_id(
            id.clone(),
            "Demo recreated".to_string(),
            Some(root.join("second")),
            Default::default(),
            type_of(&spec()),
            InstanceSpec::Builtin(spec()),
            WebUiInstanceInstallSource::Standard,
            None,
            None,
        )
        .unwrap();
    assert_ne!(recreated.install.instance_identity, first_identity);
    assert!(
        service
            .delete_state_if_identity(&id, "stale-identity")
            .unwrap_err()
            .contains("identity changed")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lifecycle_unknown_fields_are_removed_without_losing_the_instance() {
    let (root, service) = setup("lifecycle-repair", vec![]);
    let snapshot = create(&service, "Repair");
    let path = AppPaths::from_app_data_dir(root.clone())
        .unwrap()
        .instance_lifecycle_file(&snapshot.instance.id);
    let mut raw: serde_json::Value = safe_file::read_json(&path).unwrap().unwrap();
    raw["removedField"] = serde_json::json!(true);
    raw["spec"]["legacyOption"] = serde_json::json!("old");
    std::fs::write(&path, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

    assert_eq!(
        service.get(&snapshot.instance.id).unwrap().instance.id,
        snapshot.instance.id
    );
    let repaired: serde_json::Value = safe_file::read_json(&path).unwrap().unwrap();
    assert!(repaired.get("removedField").is_none());
    assert!(repaired["spec"].get("legacyOption").is_none());
    assert!(path.parent().unwrap().join(".recovery").is_dir());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn malformed_lifecycle_is_isolated_without_breaking_instance_list() {
    let (root, service) = setup("lifecycle-isolation", vec![]);
    let healthy = create(&service, "Healthy");
    let broken = create(&service, "Broken");
    let path = AppPaths::from_app_data_dir(root.clone())
        .unwrap()
        .instance_lifecycle_file(&broken.instance.id);
    std::fs::write(&path, b"not-json").unwrap();

    let listed = service.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].instance.id, healthy.instance.id);
    assert!(path.parent().unwrap().join(".recovery").is_dir());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn transition_validation_accepts_only_owned_install_results() {
    let installing = WebUiInstanceLifecycle::Installing {
        operation_id: "op-a".to_string(),
    };
    assert!(validate_transition(&WebUiInstanceLifecycle::Created, &installing).is_ok());
    assert!(
        validate_transition(
            &installing,
            &WebUiInstanceLifecycle::Ready {
                operation_id: "op-a".to_string()
            }
        )
        .is_ok()
    );
    assert!(
        validate_transition(
            &installing,
            &WebUiInstanceLifecycle::Ready {
                operation_id: "op-b".to_string()
            }
        )
        .is_err()
    );
    assert!(
        validate_transition(
            &WebUiInstanceLifecycle::Ready {
                operation_id: "op-a".to_string()
            },
            &installing
        )
        .is_err()
    );
}

#[test]
fn creation_request_rejects_frontend_derived_paths_and_task_identity() {
    let request = serde_json::json!({
        "name": "Unsafe",
        "spec": { "type": "comfyui" },
        "pythonDir": "/attacker/python",
        "corePath": "/attacker/core",
        "taskId": "chosen",
        "executorId": "chosen"
    });
    assert!(serde_json::from_value::<CreateWebUiInstanceRequest>(request).is_err());
}
