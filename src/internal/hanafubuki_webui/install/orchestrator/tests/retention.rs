use super::*;

#[test]
fn terminal_retention_evicts_old_tasks_once_and_keeps_recent_logs() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    let service = InstanceInstallService::with_terminal_retention(backend.clone(), 2);
    // History eviction is visible even while the backend is still disposing the task.
    let disposal = backend.dispose_results.lock().unwrap();
    let mut completed = Vec::new();
    for _ in 0..3 {
        let operation = service.start(request(family_specs().remove(0).0)).unwrap();
        wait_terminal(&service, &operation.operation_id);
        completed.push(operation);
    }
    assert!(
        service
            .get(&completed[0].operation_id)
            .unwrap_err()
            .contains("evicted")
    );
    assert!(service.get(&completed[1].operation_id).is_ok());
    assert!(service.get(&completed[2].operation_id).is_ok());
    assert!(
        !service
            .read_output(&completed[1].operation_id, 0, None)
            .unwrap()
            .chunks
            .is_empty()
    );

    let first_task = backend.core_requests.lock().unwrap()[0].task.clone();
    let pending = service.list_pending_cleanups();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].task, first_task);
    drop(disposal);
    wait_for_cleanups(&service);
    assert_eq!(backend.disposed.lock().unwrap().as_slice(), &[first_task]);
}

#[test]
fn terminal_history_stays_strictly_bounded_when_eviction_records_are_full() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    let retention = 2;
    let service = InstanceInstallService::with_terminal_retention(backend.clone(), retention);
    fill_exhausted_eviction_cleanups(&service);

    let mut started = Vec::new();
    for index in 0..6 {
        let mut install = request(family_specs().remove(0).0);
        install.instance_id = format!("parallel-{index}");
        started.push(service.start(install).unwrap());
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while backend.core_requests.lock().unwrap().len() < started.len() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let registry = service.inner.registry.lock();
        if registry.active_by_instance.is_empty() && registry.terminal_order.len() == retention {
            assert_eq!(registry.operations.len(), retention);
            break;
        }
        drop(registry);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }

    let cleanups = service.list_pending_cleanups();
    assert_eq!(cleanups.len(), MAX_PENDING_EVICTION_CLEANUPS);
    assert!(cleanups.iter().any(|cleanup| cleanup.queued_tasks > 0));

    for cleanup in cleanups {
        service.retry_pending_cleanup(&cleanup.cleanup_id).unwrap();
    }
    wait_for_cleanups(&service);
    assert_eq!(service.list().len(), retention);
}
