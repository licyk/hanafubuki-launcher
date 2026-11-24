use super::*;

#[test]
fn deleting_an_imported_instance_deletes_its_external_component_paths() {
    let (root, service) = setup("delete-removes-external-components", vec![]);
    let install_dir = root.join("existing-instance");
    let python_dir = install_dir.join("python");
    let external_core = root.join("shared-core");
    std::fs::create_dir_all(&python_dir).unwrap();
    std::fs::create_dir_all(&external_core).unwrap();
    std::fs::write(external_core.join("keep.txt"), b"keep").unwrap();

    let imported = service
        .import_existing(
            ImportWebUiInstanceRequest {
                name: Some("Imported".to_string()),
                install_dir,
                component_paths: std::collections::BTreeMap::from([
                    ("python".to_string(), python_dir),
                    ("core".to_string(), external_core.clone()),
                ]),
                type_id: type_of(&spec()),
                spec: spec().to_object(),
            },
            &AcceptInstancePython,
        )
        .unwrap();
    let request = DeleteInstanceRequest {
        instance_id: imported.instance.id.clone(),
        expected_instance_identity: imported.install.instance_identity.clone(),
        expected_config_revision: imported.instance.config_revision,
    };

    let validated = service.validate_delete_request(&request).unwrap();
    service.delete_instance_files(&validated).unwrap();

    assert!(!imported.install_dir.exists());
    assert!(!external_core.exists());
    // Deleting the same instance again is the retry path after a partial
    // failure, so directories that are already gone must not fail it.
    service.delete_instance_files(&validated).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn record_only_deletion_admission_accepts_external_components_without_touching_paths() {
    let (root, service) = setup("record-only-external-components", vec![]);
    let install_dir = root.join("existing-instance");
    let python_dir = install_dir.join("python");
    let external_core = root.join("shared-core");
    std::fs::create_dir_all(&python_dir).unwrap();
    std::fs::create_dir_all(&external_core).unwrap();
    std::fs::write(install_dir.join("keep.txt"), b"instance").unwrap();
    std::fs::write(external_core.join("keep.txt"), b"component").unwrap();
    let imported = service
        .import_existing(
            ImportWebUiInstanceRequest {
                name: Some("Imported".to_string()),
                install_dir,
                component_paths: std::collections::BTreeMap::from([
                    ("python".to_string(), python_dir),
                    ("core".to_string(), external_core.clone()),
                ]),
                type_id: type_of(&spec()),
                spec: spec().to_object(),
            },
            &AcceptInstancePython,
        )
        .unwrap();

    let operation = service
        .begin_instance_record_deletion(&DeleteInstanceRequest {
            instance_id: imported.instance.id,
            expected_instance_identity: imported.install.instance_identity,
            expected_config_revision: imported.instance.config_revision,
        })
        .unwrap();

    assert_eq!(operation.kind, InstanceMaintenanceKind::DeleteRecord);
    assert_eq!(
        std::fs::read(imported.install_dir.join("keep.txt")).unwrap(),
        b"instance"
    );
    assert_eq!(
        std::fs::read(external_core.join("keep.txt")).unwrap(),
        b"component"
    );
    let _ = std::fs::remove_dir_all(root);
}
