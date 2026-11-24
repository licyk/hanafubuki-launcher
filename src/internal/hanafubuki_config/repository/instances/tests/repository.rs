use super::*;

#[cfg(unix)]
#[test]
fn instance_listing_does_not_follow_record_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let paths = test_paths("record-symlink");
    let external = paths.app_data_dir.join("external-record");
    std::fs::create_dir_all(&external).unwrap();
    safe_file::write_json_atomic(&external.join("config.json"), &sample_instance()).unwrap();
    symlink(&external, paths.instance_records_dir.join("demo")).unwrap();

    let listed = list_instance_configs_inner(&paths, &ConfigState::default()).unwrap();
    assert!(listed.is_empty());

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn create_and_get_instance_without_webui_type() {
    let paths = test_paths("create-generic");
    let state = ConfigState::default();
    create_instance_config_inner(&paths, &state, sample_instance()).unwrap();
    let got = get_instance_config_inner(&paths, &state, "demo").unwrap();
    assert_eq!(got.id, "demo");
    assert_eq!(got.name, "Demo");
    // The serialized generic config carries no product/WebUI type.
    let value = serde_json::to_value(&got).unwrap();
    assert!(value.get("webuiType").is_none());
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
