use super::*;

#[test]
fn webui_instance_paths_derive_canonical_children() {
    let instance = std::env::temp_dir().join("canonical-instance");
    let paths = resolve_webui_instance_paths(instance.clone(), None).unwrap();
    assert_eq!(paths.instance_path, instance);
    assert_eq!(paths.python_dir, paths.instance_path.join("python"));
    assert_eq!(paths.core_path, paths.instance_path.join("core"));
}

#[test]
fn webui_instance_paths_reject_empty_and_relative_roots() {
    for invalid in [PathBuf::new(), PathBuf::from("relative/instance")] {
        assert!(resolve_webui_instance_paths(invalid, None).is_err());
    }
}

#[test]
fn only_invokeai_may_defer_a_missing_core_directory() {
    let root =
        std::env::temp_dir().join(format!("webui-core-directory-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let core = root.join("core");
    let invokeai: WebUiInstallSpec =
        serde_json::from_value(serde_json::json!({"type": "invokeai"})).unwrap();
    let comfyui: WebUiInstallSpec =
        serde_json::from_value(serde_json::json!({"type": "comfyui"})).unwrap();

    assert_eq!(
        runtime_core_directory_state(&invokeai, &core).unwrap(),
        WebUiCoreDirectoryState::Deferred
    );
    assert!(runtime_core_directory_state(&comfyui, &core).is_err());

    std::fs::create_dir_all(&core).unwrap();
    assert_eq!(
        runtime_core_directory_state(&invokeai, &core).unwrap(),
        WebUiCoreDirectoryState::Materialized
    );
    std::fs::remove_dir_all(&core).unwrap();
    std::fs::write(&core, b"not a directory").unwrap();
    assert!(runtime_core_directory_state(&invokeai, &core).is_err());
    let _ = std::fs::remove_dir_all(root);
}
