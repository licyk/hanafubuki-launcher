use super::*;

#[test]
fn cancellation_waits_for_instance_task_termination_confirmation() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    let service = InstanceInstallService::new(backend.clone());
    let started = service.start(request(family_specs().remove(0).0)).unwrap();
    while service
        .task_for_operation(&started.operation_id)
        .unwrap()
        .is_none()
    {
        thread::sleep(Duration::from_millis(5));
    }
    let cancelling = service.cancel(&started.operation_id).unwrap();
    assert_eq!(cancelling.phase, InstanceInstallPhase::InstallingCore);
    assert_eq!(
        wait_terminal(&service, &started.operation_id).phase,
        InstanceInstallPhase::Cancelled
    );
    assert_eq!(backend.terminated_tasks.lock().unwrap().len(), 1);
}

#[test]
fn cancellation_maps_every_confirmed_low_level_terminal_state_to_cancelled() {
    let terminal_states = [
        InstanceInstallTaskState::Cancelled,
        InstanceInstallTaskState::Exited { exit_code: 143 },
        InstanceInstallTaskState::Failed {
            exit_code: Some(1),
            error: "terminated process reported failure".to_string(),
        },
    ];
    for terminal_state in terminal_states {
        let backend = Arc::new(FakeBackend::new(
            "/instances/demo",
            vec![InstanceInstallTaskState::Running],
        ));
        *backend.termination_state.lock().unwrap() = terminal_state;
        let service = InstanceInstallService::new(backend.clone());
        let started = service.start(request(family_specs().remove(0).0)).unwrap();
        while service
            .task_for_operation(&started.operation_id)
            .unwrap()
            .is_none()
        {
            thread::sleep(Duration::from_millis(2));
        }
        service.cancel(&started.operation_id).unwrap();
        assert_eq!(
            wait_terminal(&service, &started.operation_id).phase,
            InstanceInstallPhase::Cancelled
        );
        assert_eq!(backend.terminated_tasks.lock().unwrap().len(), 1);
    }
}

#[test]
fn termination_failure_fails_with_the_exact_error() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    *backend.terminate_error.lock().unwrap() = Some("process-tree termination failed".to_string());
    let service = InstanceInstallService::new(backend);
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
    assert_eq!(
        failed.error.as_deref(),
        Some("process-tree termination failed")
    );
}

#[test]
fn python_and_core_startup_cancellation_never_leave_a_core_task_running() {
    let python_backend = Arc::new(FakeBackend::new("/instances/demo", vec![]));
    python_backend.python_delay_ms.store(100, Ordering::Release);
    let python_service = InstanceInstallService::new(python_backend.clone());
    let python = python_service
        .start(request(family_specs().remove(0).0))
        .unwrap();
    python_service.cancel(&python.operation_id).unwrap();
    assert_eq!(
        wait_terminal(&python_service, &python.operation_id).phase,
        InstanceInstallPhase::Cancelled
    );
    assert!(python_backend.core_requests.lock().unwrap().is_empty());

    let startup_backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    startup_backend.start_delay_ms.store(100, Ordering::Release);
    let startup_service = InstanceInstallService::new(startup_backend.clone());
    let startup = startup_service
        .start(request(family_specs().remove(0).0))
        .unwrap();
    while startup_service.get(&startup.operation_id).unwrap().phase
        != InstanceInstallPhase::InstallingCore
    {
        thread::sleep(Duration::from_millis(2));
    }
    startup_service.cancel(&startup.operation_id).unwrap();
    assert_eq!(
        wait_terminal(&startup_service, &startup.operation_id).phase,
        InstanceInstallPhase::Cancelled
    );
    assert_eq!(startup_backend.terminated_tasks.lock().unwrap().len(), 1);
}

#[test]
fn cancelling_every_terminal_result_is_fully_idempotent() {
    let completed_backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    let completed_service = InstanceInstallService::new(completed_backend);
    let completed = completed_service
        .start(request(family_specs().remove(0).0))
        .unwrap();
    wait_terminal(&completed_service, &completed.operation_id);
    assert_cancel_is_terminal_noop(&completed_service, &completed.operation_id);

    let failed_backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Failed {
            exit_code: Some(9),
            error: "core failed".to_string(),
        }],
    ));
    let failed_service = InstanceInstallService::new(failed_backend);
    let failed = failed_service
        .start(request(family_specs().remove(0).0))
        .unwrap();
    wait_terminal(&failed_service, &failed.operation_id);
    assert_cancel_is_terminal_noop(&failed_service, &failed.operation_id);

    let cancelled_backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    cancelled_backend
        .start_delay_ms
        .store(20, Ordering::Release);
    let cancelled_service = InstanceInstallService::new(cancelled_backend);
    let cancelled = cancelled_service
        .start(request(family_specs().remove(0).0))
        .unwrap();
    cancelled_service.cancel(&cancelled.operation_id).unwrap();
    wait_terminal(&cancelled_service, &cancelled.operation_id);
    assert_cancel_is_terminal_noop(&cancelled_service, &cancelled.operation_id);

    let cleanup_backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    *cleanup_backend.task_error.lock().unwrap() = Some("status unavailable".to_string());
    cleanup_backend.dispose_results.lock().unwrap().extend([
        Err("dispose one".to_string()),
        Err("dispose two".to_string()),
        Err("dispose three".to_string()),
    ]);
    let cleanup_service = InstanceInstallService::new(cleanup_backend);
    let cleanup_failed = cleanup_service
        .start(request(family_specs().remove(0).0))
        .unwrap();
    wait_cleanup_status(
        &cleanup_service,
        &cleanup_failed.operation_id,
        InstanceInstallCleanupStatus::CleanupFailed,
    );
    assert_cancel_is_terminal_noop(&cleanup_service, &cleanup_failed.operation_id);
}

#[test]
fn repeated_active_cancellation_requests_only_one_core_termination() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    backend.start_delay_ms.store(50, Ordering::Release);
    let service = InstanceInstallService::new(backend.clone());
    let operation = service.start(request(family_specs().remove(0).0)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while service.get(&operation.operation_id).unwrap().phase
        != InstanceInstallPhase::InstallingCore
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    service.cancel(&operation.operation_id).unwrap();
    service.cancel(&operation.operation_id).unwrap();
    assert_eq!(
        wait_terminal(&service, &operation.operation_id).phase,
        InstanceInstallPhase::Cancelled
    );
    assert_eq!(backend.terminate_attempts.load(Ordering::Acquire), 1);
    // The termination named the task by the receipt its start returned.
    assert_eq!(
        *backend.terminated_cores.lock().unwrap(),
        *backend.started_cores.lock().unwrap()
    );
    assert!(service.list_pending_cleanups().is_empty());
}
