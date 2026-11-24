use super::*;
use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_webui::instances::DeleteInstanceRequest;
use std::path::PathBuf;

#[test]
fn pending_entries_survive_reconstruction_and_block_duplicates() {
    let root = std::env::temp_dir().join(format!("hanafubuki-journal-{}", now_millis()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("journal.json");
    let journal = InstanceMaintenanceJournal::new(file.clone());
    let intent = InstanceMaintenanceIntent::Delete(DeleteInstanceRequest {
        instance_id: "one".to_string(),
        expected_instance_identity: "identity".to_string(),
        expected_config_revision: 1,
    });
    let entry = journal.reserve(intent.clone()).unwrap();
    assert!(journal.reserve(intent).unwrap_err().contains("durable"));
    assert!(
        journal
            .reserve(InstanceMaintenanceIntent::DeleteRecord(
                DeleteInstanceRequest {
                    instance_id: "one".to_string(),
                    expected_instance_identity: "identity".to_string(),
                    expected_config_revision: 1,
                },
            ))
            .unwrap_err()
            .contains("durable")
    );

    let reconstructed = InstanceMaintenanceJournal::new(file);
    assert_eq!(
        reconstructed.pending().unwrap()[0].operation_id,
        entry.operation_id
    );
    reconstructed
        .update(
            &entry.operation_id,
            InstanceMaintenanceJournalPhase::Completed,
            false,
            None,
        )
        .unwrap();
    assert!(reconstructed.pending().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn record_only_intent_has_its_own_public_kind_and_commit_phase() {
    let root = std::env::temp_dir().join(format!("hanafubuki-record-journal-{}", now_millis()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("journal.json");
    let journal = InstanceMaintenanceJournal::new(file);
    let entry = journal
        .reserve(InstanceMaintenanceIntent::DeleteRecord(
            DeleteInstanceRequest {
                instance_id: "one".to_string(),
                expected_instance_identity: "identity".to_string(),
                expected_config_revision: 1,
            },
        ))
        .unwrap();

    let reserved = journal.get_public(&entry.operation_id).unwrap();
    assert_eq!(reserved.kind, InstanceMaintenanceKind::DeleteRecord);
    assert_eq!(reserved.phase, InstanceMaintenancePhase::Stopping);
    journal
        .update(
            &entry.operation_id,
            InstanceMaintenanceJournalPhase::Committing,
            true,
            None,
        )
        .unwrap();
    assert_eq!(
        journal.get_public(&entry.operation_id).unwrap().phase,
        InstanceMaintenancePhase::RemovingRecord
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn migration_checkpoint_survives_reconstruction() {
    let root = std::env::temp_dir().join(format!("hanafubuki-migration-journal-{}", now_millis()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("journal.json");
    let source = PathBuf::from("/tmp/source");
    let destination = PathBuf::from("/tmp/destination");
    let plan = crate::internal::hanafubuki_webui::instances::InstanceMigrationPlan {
        instance_id: "one".to_string(),
        expected_instance_identity: "identity".to_string(),
        expected_config_revision: 3,
        source_install_dir: source.clone(),
        destination_install_dir: destination.clone(),
        source_component_paths: InstanceComponentPaths::defaults(
            &source,
            InstanceComponentPaths::PYTHON_AND_CORE,
        ),
        destination_component_paths: InstanceComponentPaths::defaults(
            &destination,
            InstanceComponentPaths::PYTHON_AND_CORE,
        ),
    };
    let journal = InstanceMaintenanceJournal::new(file.clone());
    let entry = journal
        .reserve(InstanceMaintenanceIntent::Migration(plan.clone()))
        .unwrap();
    let InstanceMaintenanceJournalState::Migration { checkpoint } = entry.state else {
        panic!("migration intent must reserve a migration checkpoint");
    };
    assert_eq!(checkpoint.phase, InstanceMigrationPhase::Reserved);
    assert_eq!(
        checkpoint.transfer_mode,
        crate::internal::hanafubuki_webui::instances::InstanceMigrationTransferMode::Pending
    );
    assert_eq!(
        InstanceMaintenanceJournal::new(file.clone())
            .pending()
            .unwrap()
            .len(),
        1
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn incompatible_fields_are_removed_and_current_document_is_rewritten() {
    let root = std::env::temp_dir().join(format!("hanafubuki-journal-repair-{}", now_millis()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("journal.json");
    let source = PathBuf::from("/tmp/source-repair");
    let destination = PathBuf::from("/tmp/destination-repair");
    let journal = InstanceMaintenanceJournal::new(file.clone());
    journal
        .reserve(InstanceMaintenanceIntent::Migration(
            crate::internal::hanafubuki_webui::instances::InstanceMigrationPlan {
                instance_id: "repair".to_string(),
                expected_instance_identity: "identity".to_string(),
                expected_config_revision: 1,
                source_install_dir: source.clone(),
                destination_install_dir: destination.clone(),
                source_component_paths: InstanceComponentPaths::defaults(
                    &source,
                    InstanceComponentPaths::PYTHON_AND_CORE,
                ),
                destination_component_paths: InstanceComponentPaths::defaults(
                    &destination,
                    InstanceComponentPaths::PYTHON_AND_CORE,
                ),
            },
        ))
        .unwrap();
    let mut raw: serde_json::Value = safe_file::read_json(&file).unwrap().unwrap();
    raw["obsoleteRoot"] = serde_json::json!(true);
    raw["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["state"]["checkpoint"]["progress"] = serde_json::json!({"old": true});
    let entry = raw["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    entry["intent"]["sourceComponentPaths"]["python"]["ownership"] = serde_json::json!("managed");
    entry["intent"]["destinationComponentPaths"]["core"]["ownership"] =
        serde_json::json!("attached");
    std::fs::write(&file, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

    assert_eq!(
        InstanceMaintenanceJournal::new(file.clone())
            .pending()
            .unwrap()
            .len(),
        1
    );
    let repaired: serde_json::Value = safe_file::read_json(&file).unwrap().unwrap();
    assert!(repaired.get("obsoleteRoot").is_none());
    let checkpoint = &repaired["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()["state"]["checkpoint"];
    assert!(checkpoint.get("progress").is_none());
    let intent = &repaired["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()["intent"];
    assert!(
        intent["sourceComponentPaths"]["python"]
            .get("ownership")
            .is_none()
    );
    assert!(
        intent["destinationComponentPaths"]["core"]
            .get("ownership")
            .is_none()
    );
    assert_eq!(
        intent["sourceComponentPaths"]["python"]["path"],
        serde_json::to_value(source.join("python")).unwrap()
    );
    assert_eq!(
        intent["destinationComponentPaths"]["core"]["path"],
        serde_json::to_value(destination.join("core")).unwrap()
    );
    assert!(file.parent().unwrap().join(".recovery").is_dir());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn incompatible_backup_is_repaired_without_aborting_journal_startup() {
    let root =
        std::env::temp_dir().join(format!("hanafubuki-journal-backup-repair-{}", now_millis()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("journal.json");
    let journal = InstanceMaintenanceJournal::new(file.clone());
    let entry = journal
        .reserve(InstanceMaintenanceIntent::Delete(DeleteInstanceRequest {
            instance_id: "backup".to_string(),
            expected_instance_identity: "identity".to_string(),
            expected_config_revision: 1,
        }))
        .unwrap();
    let mut backup: serde_json::Value = safe_file::read_json(&file).unwrap().unwrap();
    backup["entries"][&entry.operation_id]["oldProgress"] = serde_json::json!(25);
    std::fs::write(
        file.with_extension("json.bak"),
        serde_json::to_vec_pretty(&backup).unwrap(),
    )
    .unwrap();
    std::fs::write(&file, b"broken-main").unwrap();

    let pending = InstanceMaintenanceJournal::new(file.clone())
        .pending()
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].operation_id, entry.operation_id);
    let repaired: serde_json::Value = safe_file::read_json(&file).unwrap().unwrap();
    assert!(
        repaired["entries"][&entry.operation_id]
            .get("oldProgress")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn untransferred_recovery_is_cancellable_and_cancelled_is_terminal() {
    let root =
        std::env::temp_dir().join(format!("hanafubuki-cancellable-migration-{}", now_millis()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("journal.json");
    let source = PathBuf::from("/tmp/cancellable-source");
    let destination = PathBuf::from("/tmp/cancellable-destination");
    let plan = crate::internal::hanafubuki_webui::instances::InstanceMigrationPlan {
        instance_id: "one".to_string(),
        expected_instance_identity: "identity".to_string(),
        expected_config_revision: 3,
        source_install_dir: source.clone(),
        destination_install_dir: destination.clone(),
        source_component_paths: InstanceComponentPaths::defaults(
            &source,
            InstanceComponentPaths::PYTHON_AND_CORE,
        ),
        destination_component_paths: InstanceComponentPaths::defaults(
            &destination,
            InstanceComponentPaths::PYTHON_AND_CORE,
        ),
    };
    let journal = InstanceMaintenanceJournal::new(file);
    let entry = journal
        .reserve(InstanceMaintenanceIntent::Migration(plan))
        .unwrap();
    let InstanceMaintenanceJournalState::Migration { mut checkpoint } = entry.state else {
        panic!("migration intent must reserve a migration checkpoint");
    };
    checkpoint.phase = InstanceMigrationPhase::RecoveryRequired;
    checkpoint.cleanup_obligation = true;
    journal
        .update_migration(
            &entry.operation_id,
            checkpoint.clone(),
            Some("unused recovery".to_string()),
        )
        .unwrap();
    assert!(journal.get_public(&entry.operation_id).unwrap().cancellable);

    checkpoint.phase = InstanceMigrationPhase::Cancelled;
    checkpoint.cleanup_obligation = false;
    journal
        .update_migration(&entry.operation_id, checkpoint, None)
        .unwrap();
    let cancelled = journal.get_public(&entry.operation_id).unwrap();
    assert_eq!(cancelled.phase, InstanceMaintenancePhase::Cancelled);
    assert!(!cancelled.cancellable);
    assert!(journal.pending().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

fn recovery_files(root: &std::path::Path) -> Vec<PathBuf> {
    match std::fs::read_dir(root.join(".recovery")) {
        Ok(entries) => entries.map(|entry| entry.unwrap().path()).collect(),
        Err(_) => Vec::new(),
    }
}

#[test]
fn an_unrecoverable_entry_is_quarantined_rather_than_dropped() {
    let root =
        std::env::temp_dir().join(format!("hanafubuki-journal-unrecoverable-{}", now_millis()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("journal.json");
    let journal = InstanceMaintenanceJournal::new(file.clone());
    let kept = journal
        .reserve(InstanceMaintenanceIntent::Delete(DeleteInstanceRequest {
            instance_id: "kept".to_string(),
            expected_instance_identity: "identity".to_string(),
            expected_config_revision: 1,
        }))
        .unwrap();
    let mut raw: serde_json::Value = safe_file::read_json(&file).unwrap().unwrap();
    let unreadable = serde_json::json!({
        "operationId": "maintenance-future",
        "intent": {"kind": "teleport", "instanceId": "lost"},
        "state": {"type": "standard", "phase": "cleanupRequired", "cleanupObligation": true},
        "createdAt": 1,
        "updatedAt": 1,
        "lastError": null
    });
    raw["entries"]["maintenance-future"] = unreadable.clone();
    std::fs::write(&file, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

    let pending = InstanceMaintenanceJournal::new(file.clone())
        .pending()
        .unwrap();

    assert_eq!(
        pending
            .iter()
            .map(|entry| entry.operation_id.as_str())
            .collect::<Vec<_>>(),
        [kept.operation_id.as_str()]
    );
    let unrecoverable = recovery_files(&root)
        .into_iter()
        .find(|path| path.to_string_lossy().contains(".unrecoverable."))
        .expect("the unrecoverable entry has a quarantine file of its own");
    let quarantined: serde_json::Value =
        serde_json::from_slice(&std::fs::read(unrecoverable).unwrap()).unwrap();
    assert_eq!(
        quarantined["entries"]["maintenance-future"]["entry"],
        unreadable
    );
    assert!(
        quarantined["entries"]["maintenance-future"]["reason"]
            .as_str()
            .unwrap()
            .contains("cannot be decoded")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_newer_journal_is_quarantined_whole_and_not_rewritten() {
    let root = std::env::temp_dir().join(format!("hanafubuki-journal-newer-{}", now_millis()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("journal.json");
    let newer = serde_json::json!({
        "schemaVersion": JOURNAL_SCHEMA_VERSION + 1,
        "entries": {"maintenance-next": {"shape": "unknown to this version"}},
        "futureField": true
    });
    let content = serde_json::to_vec_pretty(&newer).unwrap();
    std::fs::write(&file, &content).unwrap();

    let journal = InstanceMaintenanceJournal::new(file.clone());
    assert!(journal.pending().unwrap().is_empty());

    assert!(
        !file.exists(),
        "the newer journal is not rewritten in place"
    );
    let quarantined = recovery_files(&root);
    assert_eq!(quarantined.len(), 1);
    assert_eq!(std::fs::read(&quarantined[0]).unwrap(), content);
    // This version then keeps a journal of its own.
    journal
        .reserve(InstanceMaintenanceIntent::Delete(DeleteInstanceRequest {
            instance_id: "after".to_string(),
            expected_instance_identity: "identity".to_string(),
            expected_config_revision: 1,
        }))
        .unwrap();
    assert_eq!(std::fs::read(&quarantined[0]).unwrap(), content);
    let _ = std::fs::remove_dir_all(root);
}
