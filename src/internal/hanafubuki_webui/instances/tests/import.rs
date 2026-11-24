use super::*;

#[test]
fn import_request_rejects_the_removed_ownership_confirmation() {
    let request = serde_json::json!({
        "name": "Imported",
        "installDir": "/instances/imported",
        "spec": { "type": "comfyui" },
        "confirmInstanceRootOwnership": true
    });
    assert!(serde_json::from_value::<ImportWebUiInstanceRequest>(request).is_err());
}

#[test]
fn import_persists_and_returns_only_normalized_native_paths() {
    let (root, service) = setup("import-native-paths", vec![]);
    let install_dir = root.join("existing-instance");
    let python_dir = install_dir.join("python");
    let core_path = root.join("attached-core");
    std::fs::create_dir_all(&python_dir).unwrap();
    std::fs::create_dir_all(&core_path).unwrap();

    let imported = service
        .import_existing(
            ImportWebUiInstanceRequest {
                name: Some("Imported".to_string()),
                install_dir: install_dir.clone(),
                component_paths: std::collections::BTreeMap::from([
                    ("python".to_string(), python_dir.clone()),
                    ("core".to_string(), core_path.clone()),
                ]),
                type_id: type_of(&spec()),
                spec: spec().to_object(),
            },
            &AcceptInstancePython,
        )
        .unwrap();
    let expected_install = crate::internal::hanafubuki_foundation::paths::normalize_path(
        std::fs::canonicalize(install_dir).unwrap(),
    )
    .unwrap();
    let expected_python = crate::internal::hanafubuki_foundation::paths::normalize_path(
        std::fs::canonicalize(python_dir).unwrap(),
    )
    .unwrap();
    let expected_core = crate::internal::hanafubuki_foundation::paths::normalize_path(
        std::fs::canonicalize(core_path).unwrap(),
    )
    .unwrap();
    assert_eq!(imported.install_dir, expected_install);
    let components = imported.instance.component_paths.as_ref().unwrap();
    assert_eq!(components.python().unwrap(), expected_python);
    assert_eq!(components.core(), expected_core);

    let config_path = AppPaths::from_app_data_dir(root.clone())
        .unwrap()
        .instance_config_file(&imported.instance.id);
    let persisted: serde_json::Value = safe_file::read_json(&config_path).unwrap().unwrap();
    assert_eq!(persisted["installDir"], serde_json::json!(expected_install));
    assert_eq!(
        persisted["componentPaths"]["python"]["path"],
        serde_json::json!(expected_python)
    );
    assert_eq!(
        persisted["componentPaths"]["core"]["path"],
        serde_json::json!(expected_core)
    );
    assert_eq!(
        imported.instance.launch_args,
        default_launch_arguments(&spec())
    );
    assert_eq!(
        imported.instance.launch_argument_origin,
        LaunchArgumentOrigin::BuiltIn
    );
    #[cfg(windows)]
    assert!(!persisted.to_string().contains(r"\\\\?\\"));
    let _ = std::fs::remove_dir_all(root);
}
