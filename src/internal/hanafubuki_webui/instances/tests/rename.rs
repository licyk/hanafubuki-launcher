use super::*;

fn request(snapshot: &WebUiInstanceSnapshot, name: &str) -> RenameWebUiInstanceRequest {
    RenameWebUiInstanceRequest {
        instance_id: snapshot.instance.id.clone(),
        expected_instance_identity: snapshot.install.instance_identity.clone(),
        expected_config_revision: snapshot.instance.config_revision,
        name: name.to_string(),
    }
}

#[test]
fn rename_normalizes_the_name_and_increments_revision_once() {
    let (root, service) = setup("rename", Vec::new());
    let created = create(&service, "Original");

    let renamed = service
        .rename(request(&created, "  Renamed   instance  "))
        .unwrap();
    assert_eq!(renamed.instance.name, "Renamed instance");
    assert_eq!(
        renamed.instance.config_revision,
        created.instance.config_revision + 1
    );

    let unchanged = service
        .rename(request(&renamed, "Renamed instance"))
        .unwrap();
    assert_eq!(
        unchanged.instance.config_revision,
        renamed.instance.config_revision
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rename_rejects_stale_revision_and_replaced_identity() {
    let (root, service) = setup("rename-stale", Vec::new());
    let created = create(&service, "Original");

    let mut stale = request(&created, "Stale");
    stale.expected_config_revision = stale.expected_config_revision.saturating_sub(1);
    assert!(
        service
            .rename(stale)
            .unwrap_err()
            .contains("revision is stale")
    );

    let mut replaced = request(&created, "Replaced");
    replaced.expected_instance_identity = "other-identity".to_string();
    assert!(
        service
            .rename(replaced)
            .unwrap_err()
            .contains("identity changed")
    );
    let _ = std::fs::remove_dir_all(root);
}
