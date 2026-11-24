use super::*;

#[test]
fn completed_and_failed_installs_persist_and_failed_install_can_retry() {
    let (root, service) = setup(
        "outcomes",
        vec![
            InstanceInstallTaskState::Failed {
                exit_code: Some(7),
                error: "first install failed".to_string(),
            },
            InstanceInstallTaskState::Exited { exit_code: 0 },
        ],
    );
    let instance = create(&service, "Retryable");
    let first = service.start_install(&instance.instance.id).unwrap();
    let failed = wait_lifecycle(&service, &instance.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::InstallFailed { .. })
    });
    match failed.install.lifecycle {
        WebUiInstanceLifecycle::InstallFailed {
            operation_id,
            exit_code,
            error,
            ..
        } => {
            assert_eq!(operation_id, first.operation_id);
            assert_eq!(exit_code, Some(7));
            assert!(error.unwrap().contains("first install failed"));
        }
        _ => unreachable!(),
    }
    assert_eq!(failed.install_action, WebUiInstanceInstallAction::Available);
    assert!(failed.instance.launch_args.is_empty());
    assert_eq!(
        failed.instance.launch_argument_origin,
        LaunchArgumentOrigin::PendingDefault
    );
    assert_eq!(failed.instance.config_revision, 1);

    let second = service.start_install(&instance.instance.id).unwrap();
    assert_ne!(first.operation_id, second.operation_id);
    let ready = wait_lifecycle(&service, &instance.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert!(matches!(
        ready.install_action,
        WebUiInstanceInstallAction::Unavailable { .. }
    ));
    assert_eq!(
        ready.instance.launch_args,
        default_launch_arguments(&spec())
    );
    assert_eq!(
        ready.instance.launch_argument_origin,
        LaunchArgumentOrigin::BuiltIn
    );
    assert_eq!(ready.instance.config_revision, 2);
    service.get_install(&second.operation_id).unwrap();
    let repeated = service.get(&instance.instance.id).unwrap();
    assert_eq!(repeated.instance.config_revision, 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cancellation_is_persisted_as_retryable_install_failure() {
    let (root, service) = setup("cancel", vec![InstanceInstallTaskState::Running]);
    let instance = create(&service, "Cancelled");
    let operation = service.start_install(&instance.instance.id).unwrap();
    service.cancel_install(&operation.operation_id).unwrap();
    let failed = wait_lifecycle(&service, &instance.instance.id, |lifecycle| {
        matches!(
            lifecycle,
            WebUiInstanceLifecycle::InstallFailed {
                phase: InstanceInstallPhase::Cancelled,
                ..
            }
        )
    });
    match &failed.install.lifecycle {
        WebUiInstanceLifecycle::InstallFailed { error, .. } => {
            assert_eq!(error.as_deref(), Some("installation cancelled"));
        }
        _ => unreachable!(),
    }
    assert_eq!(failed.install_action, WebUiInstanceInstallAction::Available);
    assert_eq!(
        failed.instance.launch_argument_origin,
        LaunchArgumentOrigin::PendingDefault
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn persisted_installing_without_a_live_operation_recovers_as_retryable() {
    let (root, service) = setup("restart-recovery", vec![]);
    let instance = create(&service, "Interrupted");
    let interrupted_operation_id = "install-before-restart".to_string();
    service
        .transition(
            &instance.instance.id,
            WebUiInstanceLifecycle::Installing {
                operation_id: interrupted_operation_id.clone(),
            },
        )
        .unwrap();

    let restarted = WebUiInstanceService::new(
        super::repository(
            AppPaths::from_app_data_dir(root.clone()).unwrap(),
            Arc::new(ConfigState::default()),
            Arc::new(InstanceTypeRegistry::with_builtin_types()),
        ),
        InstanceInstallService::new(Arc::new(FakeBackend::new(
            root.join("install-root"),
            vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
        ))),
    )
    .unwrap();
    let recovered = restarted.get(&instance.instance.id).unwrap();
    match recovered.install.lifecycle {
        WebUiInstanceLifecycle::InstallFailed {
            operation_id,
            phase,
            cleanup_status,
            error,
            ..
        } => {
            assert_eq!(operation_id, interrupted_operation_id);
            assert_eq!(phase, InstanceInstallPhase::Failed);
            assert_eq!(cleanup_status, InstanceInstallCleanupStatus::Released);
            assert!(error.unwrap().contains("interrupted before completion"));
        }
        lifecycle => panic!("expected recovered failure, got {lifecycle:?}"),
    }
    assert_eq!(
        recovered.install_action,
        WebUiInstanceInstallAction::Available
    );

    restarted.start_install(&instance.instance.id).unwrap();
    let ready = wait_lifecycle(&restarted, &instance.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert!(matches!(
        ready.install_action,
        WebUiInstanceInstallAction::Unavailable { .. }
    ));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn structured_output_serializes_and_recovers_after_no_frontend_reads() {
    let (root, service) = setup(
        "output-recovery",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    let instance = create(&service, "Output recovery");
    let operation = service.start_install(&instance.instance.id).unwrap();
    wait_lifecycle(&service, &instance.instance.id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    thread::sleep(Duration::from_millis(20));

    let restored = service.get_install(&operation.operation_id).unwrap();
    assert_eq!(restored.phase, InstanceInstallPhase::Completed);
    let first = service
        .read_install_output(&operation.operation_id, 0, Some(7))
        .unwrap();
    let second = service
        .read_install_output(&operation.operation_id, first.next_offset, None)
        .unwrap();
    let drained = service
        .read_install_output(&operation.operation_id, second.next_offset, None)
        .unwrap();
    assert_eq!(first.start_offset, 0);
    assert_eq!(first.next_offset, 7);
    assert_eq!(second.next_offset, 14);
    assert!(drained.chunks.is_empty());
    let serialized = serde_json::to_value(&first).unwrap();
    assert_eq!(serialized["chunks"][0]["stream"], "stdout");
    assert!(serialized["chunks"][0]["data"].is_array());
    assert!(serialized.get("nextOffset").is_some());
    let _ = std::fs::remove_dir_all(root);
}
