use std::path::Path;

use super::*;

fn restart_service(root: &Path) -> WebUiInstanceService {
    WebUiInstanceService::new(
        super::repository(
            AppPaths::from_app_data_dir(root.to_path_buf()).unwrap(),
            Arc::new(ConfigState::default()),
            Arc::new(InstanceTypeRegistry::with_builtin_types()),
        ),
        InstanceInstallService::new(Arc::new(FakeBackend::new(
            root.join("restart-install-root"),
            Vec::new(),
        ))),
    )
    .unwrap()
}

fn deletion_intent(instance_id: &str) -> InstanceMaintenanceIntent {
    InstanceMaintenanceIntent::Delete(DeleteInstanceRequest {
        instance_id: instance_id.to_string(),
        expected_instance_identity: format!("identity-{instance_id}"),
        expected_config_revision: 1,
    })
}

#[test]
fn terminal_maintenance_history_is_visible_only_in_the_process_that_completed_it() {
    let (root, service) = setup("terminal-maintenance-visibility", vec![]);
    let entry = service
        .reserve_lifecycle_maintenance(deletion_intent("terminal-instance"))
        .unwrap();
    service
        .update_lifecycle_maintenance(
            &entry.operation_id,
            InstanceMaintenanceJournalPhase::Failed,
            false,
            Some("persisted maintenance failure".to_string()),
        )
        .unwrap();

    let current_operation = service
        .list_maintenance_operations()
        .into_iter()
        .find(|operation| operation.operation_id == entry.operation_id)
        .unwrap();
    assert_eq!(
        current_operation.error.as_deref(),
        Some("persisted maintenance failure")
    );

    let restarted = restart_service(&root);
    assert!(
        restarted
            .list_maintenance_operations()
            .iter()
            .all(|operation| operation.operation_id != entry.operation_id)
    );
    assert!(
        restarted
            .get_maintenance_operation(&entry.operation_id)
            .unwrap_err()
            .contains("not retained in this application session")
    );
    let durable_operation = restarted
        .get_durable_maintenance_operation(&entry.operation_id)
        .unwrap();
    assert_eq!(durable_operation.operation_id, entry.operation_id);
    assert_eq!(
        durable_operation.error.as_deref(),
        Some("persisted maintenance failure")
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn pending_maintenance_stays_visible_after_restart_and_recovery_completion() {
    let (root, service) = setup("pending-maintenance-visibility", vec![]);
    let entry = service
        .reserve_lifecycle_maintenance(deletion_intent("pending-instance"))
        .unwrap();

    let restarted = restart_service(&root);
    assert!(
        restarted
            .list_maintenance_operations()
            .iter()
            .any(|operation| operation.operation_id == entry.operation_id)
    );

    restarted
        .update_lifecycle_maintenance(
            &entry.operation_id,
            InstanceMaintenanceJournalPhase::Completed,
            false,
            None,
        )
        .unwrap();
    assert!(
        restarted
            .list_maintenance_operations()
            .iter()
            .any(|operation| operation.operation_id == entry.operation_id)
    );
    restarted
        .get_maintenance_operation(&entry.operation_id)
        .unwrap();

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn reinstall_and_component_path_maintenance_are_visible_while_pending() {
    let (root, service) = setup("standard-maintenance-visibility", vec![]);
    let reinstall = service
        .reserve_lifecycle_maintenance(InstanceMaintenanceIntent::Reinstall(
            ReinstallWebUiInstanceRequest {
                instance_id: "reinstall-instance".to_string(),
                expected_instance_identity: "identity-reinstall-instance".to_string(),
                expected_config_revision: 1,
                force_recreate_python: false,
                force_recreate_core: false,
                spec: None,
            },
        ))
        .unwrap();
    service
        .update_lifecycle_maintenance(
            &reinstall.operation_id,
            InstanceMaintenanceJournalPhase::CleanupRequired,
            true,
            Some("install start failed".to_string()),
        )
        .unwrap();
    let install_dir = root.join("paths-instance");
    let paths = service
        .reserve_lifecycle_maintenance(InstanceMaintenanceIntent::ComponentPathUpdate(
            UpdateWebUiInstanceComponentPathsRequest {
                instance_id: "paths-instance".to_string(),
                expected_config_revision: 1,
                component_paths:
                    crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths::defaults(
                        &install_dir,
                        crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths::PYTHON_AND_CORE,
                    ),
            },
        ))
        .unwrap();
    service
        .update_lifecycle_maintenance(
            &paths.operation_id,
            InstanceMaintenanceJournalPhase::Committing,
            true,
            None,
        )
        .unwrap();

    let operations = service.list_maintenance_operations();
    let reinstall = operations
        .iter()
        .find(|operation| operation.operation_id == reinstall.operation_id)
        .unwrap();
    assert_eq!(reinstall.kind, InstanceMaintenanceKind::Reinstall);
    assert_eq!(reinstall.phase, InstanceMaintenancePhase::RecoveryRequired);
    assert_eq!(reinstall.error.as_deref(), Some("install start failed"));
    let paths = operations
        .iter()
        .find(|operation| operation.operation_id == paths.operation_id)
        .unwrap();
    assert_eq!(paths.kind, InstanceMaintenanceKind::ComponentPathUpdate);
    assert_eq!(
        paths.phase,
        InstanceMaintenancePhase::UpdatingComponentPaths
    );

    let _ = std::fs::remove_dir_all(root);
}
