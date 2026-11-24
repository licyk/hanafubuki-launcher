use super::*;

#[test]
fn name_update_checks_revision_and_is_idempotent() {
    let paths = test_paths("rename");
    let state = ConfigState::default();
    let created = create_instance_config_inner(&paths, &state, sample_instance()).unwrap();

    let renamed = update_instance_name_inner(
        &paths,
        &state,
        "demo",
        created.config_revision,
        "Renamed".to_string(),
    )
    .unwrap();
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.config_revision, created.config_revision + 1);

    let unchanged = update_instance_name_inner(
        &paths,
        &state,
        "demo",
        renamed.config_revision,
        "Renamed".to_string(),
    )
    .unwrap();
    assert_eq!(unchanged.config_revision, renamed.config_revision);

    let error = update_instance_name_inner(
        &paths,
        &state,
        "demo",
        created.config_revision,
        "Stale".to_string(),
    )
    .unwrap_err();
    assert!(error.contains("revision is stale"));
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
