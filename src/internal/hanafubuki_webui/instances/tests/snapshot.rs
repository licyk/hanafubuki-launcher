use super::*;

#[test]
fn snapshot_create_request_requires_only_the_snapshot_and_expected_type() {
    let request: CreateWebUiInstanceFromSnapshotRequest =
        serde_json::from_value(serde_json::json!({
            "snapshotPath": "/snapshots/comfy.json",
            "expectedType": "builtin:comfyui"
        }))
        .unwrap();

    assert_eq!(
        request.snapshot_path,
        PathBuf::from("/snapshots/comfy.json")
    );
    assert_eq!(
        request.expected_type,
        crate::internal::hanafubuki_webui::instance_types::BuiltinProvider::type_id(
            WebUiFamily::Comfyui
        )
    );
    for refused in [
        serde_json::json!({
            "snapshotPath": "/snapshots/comfy.json",
            "expectedType": "builtin:comfyui",
            "confirmDestructiveRestore": true
        }),
        serde_json::json!({
            "snapshotPath": "/snapshots/comfy.json",
            "expectedType": "comfyui"
        }),
    ] {
        assert!(serde_json::from_value::<CreateWebUiInstanceFromSnapshotRequest>(refused).is_err());
    }
}

#[test]
fn oversized_snapshot_is_rejected_from_metadata_before_full_file_read() {
    let (root, service) = setup("oversized-snapshot", vec![]);
    std::fs::create_dir_all(&root).unwrap();
    let snapshot_path = root.join("oversized.json");
    let file = std::fs::File::create(&snapshot_path).unwrap();
    file.set_len(MAX_SNAPSHOT_BYTES + 1).unwrap();

    let error = service
        .create_from_snapshot(CreateWebUiInstanceFromSnapshotRequest {
            name: Some("Oversized".to_string()),
            install_dir: None,
            component_paths: Default::default(),
            snapshot_path,
            expected_type:
                crate::internal::hanafubuki_webui::instance_types::BuiltinProvider::type_id(
                    WebUiFamily::Comfyui,
                ),
        })
        .unwrap_err();

    assert_eq!(error, "snapshot file exceeds the 64 MiB installation limit");
    let _ = std::fs::remove_dir_all(root);
}

/// The creation wizard offers "from a snapshot" for the types whose
/// descriptor declares `snapshots`: exactly those this restores.
#[test]
fn a_type_restores_from_a_snapshot_exactly_when_it_declares_snapshots() {
    let (root, service, _) = super::declarative::setup_type(
        "snapshot-types",
        crate::internal::hanafubuki_webui::instance_types::declarative_fixtures::TWO_SERVERS,
        vec![],
    );
    let descriptors = service.instance_types().descriptors();
    let declares = |operations: &[String]| operations.iter().any(|op| op == "snapshots");
    assert!(descriptors.iter().any(|type_| !declares(&type_.operations)));
    for descriptor in descriptors {
        let error = service
            .create_from_snapshot(CreateWebUiInstanceFromSnapshotRequest {
                name: None,
                install_dir: None,
                component_paths: Default::default(),
                snapshot_path: root.join("absent.json"),
                expected_type: descriptor.id.clone(),
            })
            .unwrap_err();
        let refused = error == format!("type `{}` does not restore from a snapshot", descriptor.id);
        assert_eq!(
            refused,
            !declares(&descriptor.operations),
            "{}: {error}",
            descriptor.id
        );
    }
    let _ = std::fs::remove_dir_all(root);
}
