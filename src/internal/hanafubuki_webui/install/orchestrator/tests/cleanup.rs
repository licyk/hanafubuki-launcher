use super::*;

#[test]
fn different_instance_starts_while_first_cleanup_is_exhausted() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    *backend.task_error.lock().unwrap() = Some("status lost".to_string());
    backend.dispose_results.lock().unwrap().extend([
        Err("one".to_string()),
        Err("two".to_string()),
        Err("three".to_string()),
    ]);
    let service = InstanceInstallService::new(backend);
    let first = service.start(request(family_specs().remove(0).0)).unwrap();
    wait_terminal(&service, &first.operation_id);
    wait_pending_status(&service, InstanceInstallPendingCleanupStatus::Exhausted);
    let mut other = request(family_specs().remove(0).0);
    other.instance_id = "other".to_string();
    assert!(service.start(other).is_ok());
}

#[test]
fn stale_cleanup_completion_cannot_release_newer_ownership() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    let service = InstanceInstallService::new(backend.clone());
    let old = service.start(request(family_specs().remove(0).0)).unwrap();
    wait_terminal(&service, &old.operation_id);
    *backend.states.lock().unwrap() = VecDeque::from([InstanceInstallTaskState::Running]);
    let newer = service.start(request(family_specs().remove(0).0)).unwrap();
    let stale_task = InstanceInstallTaskRef {
        instance_id: "demo".to_string(),
        task_id: "stale-task".to_string(),
    };
    let id = cleanup_id(&stale_task);
    service.inner.registry.lock().pending_cleanups.insert(
        id.clone(),
        PendingCleanupEntry {
            info: InstanceInstallPendingCleanup {
                cleanup_id: id.clone(),
                task: stale_task.clone(),
                operation_id: Some(old.operation_id.clone()),
                kind: InstanceInstallPendingCleanupKind::Ownership,
                status: InstanceInstallPendingCleanupStatus::Pending,
                attempts: 0,
                error: None,
                queued_tasks: 0,
            },
            purpose: PendingCleanupPurpose::Ownership {
                instance_id: "demo".to_string(),
                operation_id: old.operation_id,
                generation: 1,
            },
            in_progress: false,
            core_task: fake_core_task(stale_task),
            queued_evictions: VecDeque::new(),
        },
    );
    complete_pending_cleanup(&service.inner, &id);
    let duplicate = service
        .start(request(family_specs().remove(0).0))
        .unwrap_err();
    assert!(duplicate.contains(&newer.operation_id));
    service.cancel(&newer.operation_id).unwrap();
    wait_terminal(&service, &newer.operation_id);
}

#[test]
fn eviction_cleanup_retries_then_succeeds_and_already_absent_is_success() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    backend.dispose_results.lock().unwrap().extend([
        Err("transient disposal error".to_string()),
        Ok(InstanceInstallTaskCleanupResult::Disposed),
    ]);
    let service = InstanceInstallService::with_terminal_retention(backend.clone(), 1);
    for _ in 0..2 {
        let operation = service.start(request(family_specs().remove(0).0)).unwrap();
        wait_terminal(&service, &operation.operation_id);
    }
    wait_for_cleanups(&service);
    assert_eq!(backend.dispose_attempts.lock().unwrap().len(), 2);
    assert_eq!(backend.disposed.lock().unwrap().len(), 1);
    // An evicted operation's task is disposed with the receipt its start returned.
    let started = backend.started_cores.lock().unwrap().clone();
    assert!(
        backend
            .disposed_cores
            .lock()
            .unwrap()
            .iter()
            .all(|core| started.contains(core))
    );

    let absent_backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    absent_backend
        .dispose_results
        .lock()
        .unwrap()
        .push_back(Ok(InstanceInstallTaskCleanupResult::AlreadyAbsent));
    let absent_service = InstanceInstallService::with_terminal_retention(absent_backend.clone(), 1);
    for _ in 0..2 {
        let operation = absent_service
            .start(request(family_specs().remove(0).0))
            .unwrap();
        wait_terminal(&absent_service, &operation.operation_id);
    }
    wait_for_cleanups(&absent_service);
    assert_eq!(absent_backend.dispose_attempts.lock().unwrap().len(), 1);
}

#[test]
fn exhausted_eviction_cleanup_remains_discoverable_and_retryable() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    backend.dispose_results.lock().unwrap().extend([
        Err("eviction failure one".to_string()),
        Err("eviction failure two".to_string()),
        Err("eviction failure three".to_string()),
    ]);
    let service = InstanceInstallService::with_terminal_retention(backend.clone(), 1);
    for _ in 0..2 {
        let operation = service.start(request(family_specs().remove(0).0)).unwrap();
        wait_terminal(&service, &operation.operation_id);
    }
    let exhausted = wait_pending_status(&service, InstanceInstallPendingCleanupStatus::Exhausted);
    assert_eq!(exhausted.attempts, 3);
    assert_eq!(exhausted.error.as_deref(), Some("eviction failure three"));
    backend
        .dispose_results
        .lock()
        .unwrap()
        .push_back(Ok(InstanceInstallTaskCleanupResult::Disposed));
    service
        .retry_pending_cleanup(&exhausted.cleanup_id)
        .unwrap();
    wait_for_cleanups(&service);
    assert_eq!(backend.disposed.lock().unwrap().len(), 1);
}

#[test]
fn concurrent_eviction_workers_cannot_dispose_the_same_task_twice() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    // Hold the first disposal inside the backend until the competing worker exits.
    let disposal = backend.dispose_results.lock().unwrap();
    let service = InstanceInstallService::with_terminal_retention(backend.clone(), 1);
    for _ in 0..2 {
        let operation = service.start(request(family_specs().remove(0).0)).unwrap();
        wait_terminal(&service, &operation.operation_id);
    }
    let running = wait_pending_status(&service, InstanceInstallPendingCleanupStatus::Retrying);
    let competing = spawn_cleanup_worker(service.inner.clone(), running.cleanup_id);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !competing.is_finished() {
        assert!(Instant::now() < deadline, "competing worker did not exit");
        thread::sleep(Duration::from_millis(2));
    }
    competing.join().unwrap();
    drop(disposal);
    wait_for_cleanups(&service);
    assert_eq!(backend.dispose_attempts.lock().unwrap().len(), 1);
    assert_eq!(backend.disposed.lock().unwrap().len(), 1);
}

#[test]
fn full_eviction_backlog_does_not_consume_ownership_cleanup_capacity() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    let service = InstanceInstallService::new(backend.clone());
    fill_exhausted_eviction_cleanups(&service);

    *backend.task_error.lock().unwrap() = Some("lost running task status".to_string());
    backend.dispose_results.lock().unwrap().extend([
        Err("confirmed disposal one failed".to_string()),
        Err("confirmed disposal two failed".to_string()),
        Err("confirmed disposal three failed".to_string()),
    ]);
    let affected = service.start(request(family_specs().remove(0).0)).unwrap();
    wait_terminal(&service, &affected.operation_id);
    wait_pending_kind_status(
        &service,
        InstanceInstallPendingCleanupKind::Ownership,
        InstanceInstallPendingCleanupStatus::Exhausted,
    );
    let duplicate = service
        .start(request(family_specs().remove(0).0))
        .unwrap_err();
    assert!(duplicate.contains(&affected.operation_id));
    assert!(duplicate.contains("cleanup"));

    *backend.states.lock().unwrap() =
        VecDeque::from([InstanceInstallTaskState::Exited { exit_code: 0 }]);
    let mut other = request(family_specs().remove(0).0);
    other.instance_id = "healthy-other".to_string();
    let other = service.start(other).unwrap();
    assert_eq!(
        wait_terminal(&service, &other.operation_id).phase,
        InstanceInstallPhase::Completed
    );

    let cleanups = service.list_pending_cleanups();
    assert_eq!(
        cleanups
            .iter()
            .filter(|cleanup| cleanup.kind == InstanceInstallPendingCleanupKind::Eviction)
            .count(),
        MAX_PENDING_EVICTION_CLEANUPS
    );
    assert_eq!(
        cleanups
            .iter()
            .filter(|cleanup| cleanup.kind == InstanceInstallPendingCleanupKind::Ownership)
            .count(),
        1
    );
}

#[test]
fn a_started_core_task_that_fails_to_register_is_disposed_with_its_receipt() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    backend.start_retains_process.store(true, Ordering::Release);
    let service = InstanceInstallService::new(backend.clone());
    let operation = service.start(request(family_specs().remove(0).0)).unwrap();

    let failed = wait_terminal(&service, &operation.operation_id);
    assert_eq!(failed.phase, InstanceInstallPhase::Failed);
    assert!(
        failed
            .error
            .as_deref()
            .is_some_and(|error| error.contains("ownership was retained"))
    );
    // The started task is published and its ownership cleanup disposes it with
    // the receipt the start returned.
    assert!(failed.task.is_some());
    let deadline = Instant::now() + Duration::from_secs(2);
    while backend.disposed_cores.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        *backend.disposed_cores.lock().unwrap(),
        *backend.started_cores.lock().unwrap()
    );
}
