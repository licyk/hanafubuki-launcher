use crate::internal::hanafubuki_webui::instances::InstanceMigrationRequest;

#[test]
fn migration_ipc_requests_reject_unknown_fields() {
    let value = serde_json::json!({
        "instanceId": "one",
        "expectedInstanceIdentity": "identity-one",
        "expectedConfigRevision": 1,
        "destinationParentDir": "/tmp/destination",
        "unexpected": true
    });
    assert!(serde_json::from_value::<InstanceMigrationRequest>(value).is_err());
}

#[test]
fn migration_request_rejects_the_removed_destination_and_confirmation_fields() {
    let value = serde_json::json!({
        "instanceId": "one",
        "expectedInstanceIdentity": "identity-one",
        "expectedConfigRevision": 1,
        "destinationInstallDir": "/tmp/destination",
        "confirmationToken": "token",
        "confirmation": "one"
    });
    assert!(serde_json::from_value::<InstanceMigrationRequest>(value).is_err());
}
