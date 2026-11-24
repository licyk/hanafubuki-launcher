use super::*;

#[test]
fn reserved_install_does_not_start_work_until_commit_and_can_be_aborted() {
    let backend = Arc::new(FakeBackend::new(
        "/tmp/reserved-install",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    ));
    let service = InstanceInstallService::new(backend.clone());
    let request = || StartInstanceInstallRequest {
        instance_id: "demo".to_string(),
        spec: WebUiInstallSpec::Comfyui(ComfyuiInstallSpec {
            pytorch: None,
            python_version: None,
            predownload_models: None,
            predownload_extensions: None,
        }),
    };

    let abandoned = service.reserve_start(request()).unwrap();
    assert!(backend.python_targets.lock().unwrap().is_empty());
    service.abort_start(abandoned);
    assert!(service.list().is_empty());

    let reservation = service.reserve_start(request()).unwrap();
    let operation_id = reservation.operation().operation_id.clone();
    assert!(backend.python_targets.lock().unwrap().is_empty());
    service.commit_start(reservation);

    let deadline = Instant::now() + Duration::from_secs(2);
    while backend.python_targets.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "committed install did not start");
        thread::sleep(Duration::from_millis(2));
    }
    while !service.get(&operation_id).unwrap().phase.is_terminal() {
        assert!(
            Instant::now() < deadline,
            "committed install did not finish"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn same_instance_is_rejected_while_different_instances_are_independent() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    backend.python_delay_ms.store(100, Ordering::Release);
    let service = InstanceInstallService::new(backend);
    let first = service.start(request(family_specs().remove(0).0)).unwrap();
    let duplicate = service
        .start(request(family_specs().remove(0).0))
        .unwrap_err();
    assert!(duplicate.contains("remains owned"));

    let mut other = request(family_specs().remove(0).0);
    other.instance_id = "other".to_string();
    let second = service.start(other).unwrap();
    assert_ne!(first.operation_id, second.operation_id);
    while service
        .task_for_operation(&first.operation_id)
        .unwrap()
        .is_none()
        || service
            .task_for_operation(&second.operation_id)
            .unwrap()
            .is_none()
    {
        thread::sleep(Duration::from_millis(5));
    }
    assert_ne!(
        service.task_for_operation(&first.operation_id).unwrap(),
        service.task_for_operation(&second.operation_id).unwrap()
    );
    service.cancel(&first.operation_id).unwrap();
    service.cancel(&second.operation_id).unwrap();
    assert_eq!(
        wait_terminal(&service, &first.operation_id).phase,
        InstanceInstallPhase::Cancelled
    );
    assert_eq!(
        wait_terminal(&service, &second.operation_id).phase,
        InstanceInstallPhase::Cancelled
    );
}

#[test]
fn an_install_started_after_the_shutdown_is_refused() {
    let backend = Arc::new(FakeBackend::new(
        "/instances/demo",
        vec![InstanceInstallTaskState::Running],
    ));
    let service = InstanceInstallService::new(backend);
    let running = service.start(request(family_specs().remove(0).0)).unwrap();

    let ended = service.shutdown(Instant::now() + Duration::from_secs(5));
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].operation_id, running.operation_id);
    assert_eq!(ended[0].phase, InstanceInstallPhase::Cancelled);

    let mut late = request(family_specs().remove(0).0);
    late.instance_id = "other".to_string();
    let refused = service.start(late).unwrap_err();
    assert!(refused.contains("quitting"), "{refused}");
    assert_eq!(service.list().len(), 1);
}
