use super::*;

#[test]
fn active_operation_is_retained_and_never_disposed() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    let service = InstanceInstallService::with_terminal_retention(backend.clone(), 1);
    let active = service.start(request(family_specs().remove(0).0)).unwrap();
    while service
        .task_for_operation(&active.operation_id)
        .unwrap()
        .is_none()
    {
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        service.get(&active.operation_id).unwrap().phase,
        InstanceInstallPhase::InstallingCore
    );
    assert!(backend.disposed.lock().unwrap().is_empty());
    service.cancel(&active.operation_id).unwrap();
    wait_terminal(&service, &active.operation_id);
}

#[test]
fn stale_completion_cannot_release_or_overwrite_newer_ownership() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    let service = InstanceInstallService::new(backend);
    let first = service.start(request(family_specs().remove(0).0)).unwrap();
    wait_terminal(&service, &first.operation_id);
    let second = service.start(request(family_specs().remove(0).0)).unwrap();

    finish(
        &service.inner,
        &first.operation_id,
        1,
        InstanceInstallPhase::Failed,
        Some(99),
        Some("stale".to_string()),
    );
    let first_after = service.get(&first.operation_id).unwrap();
    assert_eq!(first_after.phase, InstanceInstallPhase::Completed);
    assert_ne!(
        service.get(&second.operation_id).unwrap().phase,
        InstanceInstallPhase::Failed
    );
    wait_terminal(&service, &second.operation_id);
}

#[test]
fn cancellation_timeout_keeps_ownership_until_cleanup_is_proven() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    *backend.termination_state.lock().unwrap() = InstanceInstallTaskState::Running;
    backend.dispose_results.lock().unwrap().extend([
        Err("dispose one failed".to_string()),
        Err("dispose two failed".to_string()),
        Err("dispose three failed".to_string()),
    ]);
    let service = InstanceInstallService::with_test_policy(
        backend,
        DEFAULT_TERMINAL_OPERATION_RETENTION,
        Duration::from_millis(10),
    );
    let started = service.start(request(family_specs().remove(0).0)).unwrap();
    while service
        .task_for_operation(&started.operation_id)
        .unwrap()
        .is_none()
    {
        thread::sleep(Duration::from_millis(2));
    }
    service.cancel(&started.operation_id).unwrap();
    let failed = wait_terminal(&service, &started.operation_id);
    assert_eq!(failed.phase, InstanceInstallPhase::Failed);
    assert!(failed.error.unwrap().contains("timed out confirming"));
    let duplicate = service
        .start(request(family_specs().remove(0).0))
        .unwrap_err();
    assert!(duplicate.contains(&started.operation_id));
    assert!(duplicate.contains("cleanup is pending or failed"));
    let exhausted = wait_pending_status(&service, InstanceInstallPendingCleanupStatus::Exhausted);
    assert_eq!(exhausted.attempts, 3);
    let operation = wait_cleanup_status(
        &service,
        &started.operation_id,
        InstanceInstallCleanupStatus::CleanupFailed,
    );
    assert!(
        operation
            .cleanup_error
            .unwrap()
            .contains("dispose three failed")
    );
}

#[test]
fn lookup_failure_after_termination_does_not_release_ownership() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    backend.start_delay_ms.store(50, Ordering::Release);
    *backend.termination_state.lock().unwrap() = InstanceInstallTaskState::Running;
    *backend.task_error.lock().unwrap() = Some("status lookup unavailable".to_string());
    backend.dispose_results.lock().unwrap().extend([
        Err("cleanup unavailable 1".to_string()),
        Err("cleanup unavailable 2".to_string()),
        Err("cleanup unavailable 3".to_string()),
    ]);
    let service = InstanceInstallService::new(backend);
    let started = service.start(request(family_specs().remove(0).0)).unwrap();
    while service.get(&started.operation_id).unwrap().phase != InstanceInstallPhase::InstallingCore
    {
        thread::sleep(Duration::from_millis(2));
    }
    service.cancel(&started.operation_id).unwrap();
    let failed = wait_terminal(&service, &started.operation_id);
    assert_eq!(failed.phase, InstanceInstallPhase::Failed);
    assert_eq!(failed.error.as_deref(), Some("status lookup unavailable"));
    assert!(
        service
            .start(request(family_specs().remove(0).0))
            .unwrap_err()
            .contains("remains owned")
    );
    wait_pending_status(&service, InstanceInstallPendingCleanupStatus::Exhausted);
}

#[test]
fn ownership_releases_after_later_terminal_status_or_confirmed_disposal() {
    let terminal_backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    *terminal_backend.termination_state.lock().unwrap() = InstanceInstallTaskState::Running;
    terminal_backend
        .dispose_results
        .lock()
        .unwrap()
        .push_back(Err("first dispose failed".to_string()));
    let terminal_service = InstanceInstallService::with_test_policy(
        terminal_backend.clone(),
        DEFAULT_TERMINAL_OPERATION_RETENTION,
        Duration::from_millis(10),
    );
    let terminal = terminal_service
        .start(request(family_specs().remove(0).0))
        .unwrap();
    while terminal_service
        .task_for_operation(&terminal.operation_id)
        .unwrap()
        .is_none()
    {
        thread::sleep(Duration::from_millis(2));
    }
    terminal_service.cancel(&terminal.operation_id).unwrap();
    wait_terminal(&terminal_service, &terminal.operation_id);
    while terminal_backend.dispose_attempts.lock().unwrap().is_empty() {
        thread::sleep(Duration::from_millis(2));
    }
    *terminal_backend.termination_state.lock().unwrap() =
        InstanceInstallTaskState::Exited { exit_code: 143 };
    wait_cleanup_status(
        &terminal_service,
        &terminal.operation_id,
        InstanceInstallCleanupStatus::Released,
    );
    assert!(
        terminal_service
            .start(request(family_specs().remove(0).0))
            .is_ok()
    );

    let dispose_backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    *dispose_backend.termination_state.lock().unwrap() = InstanceInstallTaskState::Running;
    let dispose_service = InstanceInstallService::with_test_policy(
        dispose_backend,
        DEFAULT_TERMINAL_OPERATION_RETENTION,
        Duration::from_millis(10),
    );
    let disposed = dispose_service
        .start(request(family_specs().remove(0).0))
        .unwrap();
    while dispose_service
        .task_for_operation(&disposed.operation_id)
        .unwrap()
        .is_none()
    {
        thread::sleep(Duration::from_millis(2));
    }
    dispose_service.cancel(&disposed.operation_id).unwrap();
    wait_cleanup_status(
        &dispose_service,
        &disposed.operation_id,
        InstanceInstallCleanupStatus::Released,
    );
    assert!(
        dispose_service
            .start(request(family_specs().remove(0).0))
            .is_ok()
    );
}
