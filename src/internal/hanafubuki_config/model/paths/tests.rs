use super::AppPaths;

fn test_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "hanafubuki-config-paths-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn constructs_the_complete_application_path_contract() {
    let root = test_root("complete-contract");
    let _ = std::fs::remove_dir_all(&root);
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();

    assert!(
        !root.exists(),
        "path construction must not mutate the filesystem"
    );

    assert_eq!(paths.instance_records_dir, root.join("data/instances"));
    assert_eq!(
        paths.bootstrap.runtime_storage_config_file,
        root.join("config/runtime-storage.json")
    );
    assert_eq!(paths.launcher_python_dir(), root.join("runtime/python"));
    assert_eq!(paths.launcher_code_dir(), root.join("launcher-code"));
    assert_eq!(paths.managed_instances_dir, root.join("instances"));
    assert_eq!(
        paths.instance_config_file("demo"),
        root.join("data/instances/demo/config.json")
    );
    assert_eq!(
        paths.instance_lifecycle_file("demo"),
        root.join("data/instances/demo/lifecycle.json")
    );
    assert_eq!(
        paths.instance_source_snapshot_file("demo"),
        root.join("data/instances/demo/source-snapshot.json")
    );
    assert_eq!(
        paths.instance_maintenance_journal_file(),
        root.join("state/instances/maintenance-journal.json")
    );
    assert_eq!(
        paths.instance_update_status_file(),
        root.join("state/instances/update-status.json")
    );
    assert_eq!(
        paths.process_watchdog_file(),
        root.join("state/diagnostics/process-watchdog.tsv")
    );
    assert_eq!(
        paths.previous_process_watchdog_file(),
        root.join("state/diagnostics/process-watchdog.previous.tsv")
    );
    assert_eq!(
        paths.app_update_handoff_file(),
        root.join("state/app-update-handoff.json")
    );
    assert_eq!(
        paths.app_update_automation_file(),
        root.join("state/app-update-automation.json")
    );
    assert_eq!(paths.cache_dir, root.join("cache"));

    paths.prepare_managed_directories().unwrap();
    for directory in [
        &paths.instance_records_dir,
        &paths.managed_instances_dir,
        &root.join("state/instances"),
        &root.join("state/diagnostics"),
    ] {
        assert!(directory.is_dir(), "missing {}", directory.display());
    }
    assert!(!root.join("data/webui-instances").exists());
    assert!(!root.join("webui-instances").exists());
    assert!(!root.join("state/port-locks").exists());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn constructs_a_unified_managed_runtime_contract() {
    let root = test_root("managed-runtime");
    let managed = root.join("custom-runtime");
    let paths = AppPaths::from_managed_runtime_root(root.clone(), managed.clone()).unwrap();

    assert_eq!(paths.managed_runtime.root(), managed);
    assert_eq!(paths.launcher_python_dir(), managed.join("python"));
    assert_eq!(paths.micromamba_runtime_dir(), managed.join("micromamba"));
    assert_eq!(paths.micromamba_root_prefix(), managed.join("conda"));
    assert_eq!(paths.launcher_code_dir(), managed.join("launcher-code"));
}
