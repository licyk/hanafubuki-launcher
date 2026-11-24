use super::*;

fn configured_instance(paths: &AppPaths) -> (InstanceConfig, std::path::PathBuf) {
    let source = paths.app_data_dir.join("instance-source");
    let mut instance = sample_instance();
    instance.install_dir = Some(source.clone());
    instance.component_paths = Some(InstanceComponentPaths::defaults(
        &source,
        InstanceComponentPaths::PYTHON_AND_CORE,
    ));
    (instance, source)
}

#[test]
fn location_update_is_one_atomic_revisioned_write() {
    let paths = test_paths("migration-location-commit");
    let state = ConfigState::default();
    let (instance, source) = configured_instance(&paths);
    let created = create_instance_config_inner(&paths, &state, instance).unwrap();
    let destination = paths.app_data_dir.join("instance-destination");
    let destination_components =
        InstanceComponentPaths::defaults(&destination, InstanceComponentPaths::PYTHON_AND_CORE);

    let updated = update_instance_location_inner(
        &paths,
        &state,
        "demo",
        created.config_revision,
        &source,
        destination.clone(),
        destination_components.clone(),
    )
    .unwrap();

    assert_eq!(updated.config_revision, created.config_revision + 1);
    assert_eq!(updated.install_dir.as_ref(), Some(&destination));
    assert_eq!(
        updated.component_paths.as_ref(),
        Some(&destination_components)
    );
    let persisted = get_instance_config_inner(&paths, &state, "demo").unwrap();
    assert_eq!(
        serde_json::to_value(persisted).unwrap(),
        serde_json::to_value(updated).unwrap()
    );
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn stale_revision_or_old_location_mismatch_leaves_config_unchanged() {
    let paths = test_paths("migration-location-cas");
    let state = ConfigState::default();
    let (instance, source) = configured_instance(&paths);
    let created = create_instance_config_inner(&paths, &state, instance).unwrap();
    let destination = paths.app_data_dir.join("instance-destination");
    let components =
        InstanceComponentPaths::defaults(&destination, InstanceComponentPaths::PYTHON_AND_CORE);

    assert!(
        update_instance_location_inner(
            &paths,
            &state,
            "demo",
            created.config_revision + 1,
            &source,
            destination.clone(),
            components.clone(),
        )
        .unwrap_err()
        .contains("revision is stale")
    );
    assert!(
        update_instance_location_inner(
            &paths,
            &state,
            "demo",
            created.config_revision,
            &paths.app_data_dir.join("wrong-source"),
            destination,
            components,
        )
        .unwrap_err()
        .contains("install directory changed")
    );
    assert_eq!(
        serde_json::to_value(get_instance_config_inner(&paths, &state, "demo").unwrap()).unwrap(),
        serde_json::to_value(created).unwrap()
    );
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
