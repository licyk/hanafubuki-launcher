use crate::internal::hanafubuki_process::executor::manager::{ExecutorManager, get_executor_info};

use super::super::test_support::{dispose_app_task_for_test, terminate_app_task_for_test};
use super::super::{
    AppTaskState, get_app_task_inner, list_app_tasks_inner, read_app_task_output_inner,
    start_backend_app_task_inner,
};
use super::fixtures::start_req;

#[cfg(unix)]
#[test]
fn committed_app_task_blocks_reservation_and_is_not_adopted() {
    use crate::internal::hanafubuki_process::executor::manager::reserve_executor_id;
    use crate::internal::hanafubuki_process::ownership::ProcessOwnerId;

    use super::super::{executor_id_for, reserve_app_task_identity};

    let executor = ExecutorManager::default();
    let tasks = AppTaskState::default();
    let mut request = start_req(&["sh", "-c", "sleep 30"]);
    request.task_id = Some("generic-app-wins".to_string());
    let info = start_backend_app_task_inner(&executor, &tasks, request).unwrap();
    let reservation_id = ProcessOwnerId::generate();
    assert!(reserve_app_task_identity(&tasks, &info.task_id, &reservation_id).is_err());
    assert!(reserve_executor_id(&executor, &info.executor_id, &reservation_id).is_err());
    assert!(get_app_task_inner(&executor, &tasks, info.task_id.clone()).is_ok());
    terminate_app_task_for_test(&executor, &tasks, info.task_id.clone()).unwrap();
    dispose_app_task_for_test(&executor, &tasks, info.task_id.clone()).unwrap();
    assert!(executor_id_for(&info.task_id).starts_with("app:task:"));
}

#[cfg(unix)]
#[test]
fn start_lists_without_instance_and_reads_output() {
    use std::time::{Duration, Instant};

    let executor = ExecutorManager::default();
    let tasks = AppTaskState::default();
    let mut request = start_req(&["/bin/echo", "app-marker"]);
    request.service_id = Some("self-manager-api".to_string());
    let info = start_backend_app_task_inner(&executor, &tasks, request).unwrap();
    assert_eq!(info.service_id.as_deref(), Some("self-manager-api"));
    assert_eq!(list_app_tasks_inner(&executor, &tasks).len(), 1);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output =
            read_app_task_output_inner(&executor, &tasks, info.task_id.clone(), Some(0), None)
                .unwrap();
        if output.status.is_terminal() {
            let bytes = output
                .chunks
                .iter()
                .flat_map(|chunk| chunk.data.iter().copied())
                .collect::<Vec<_>>();
            assert!(String::from_utf8_lossy(&bytes).contains("app-marker"));
            break;
        }
        assert!(Instant::now() < deadline, "app task did not finish");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn dispose_removes_metadata_and_disposes_executor() {
    let executor = ExecutorManager::default();
    let tasks = AppTaskState::default();
    let info =
        start_backend_app_task_inner(&executor, &tasks, start_req(&["/bin/echo", "hi"])).unwrap();
    dispose_app_task_for_test(&executor, &tasks, info.task_id.clone()).unwrap();
    assert!(get_executor_info(&executor, &info.executor_id).is_err());
    assert!(list_app_tasks_inner(&executor, &tasks).is_empty());
    assert!(get_app_task_inner(&executor, &tasks, info.task_id).is_err());
}

#[cfg(unix)]
#[test]
fn list_snapshot_releases_registry_before_live_executor_projection() {
    use super::super::queries::snapshot_tasks;

    let executor = ExecutorManager::default();
    let tasks = AppTaskState::default();
    let info =
        start_backend_app_task_inner(&executor, &tasks, start_req(&["/bin/echo", "snapshot"]))
            .unwrap();
    let snapshot = snapshot_tasks(&tasks);
    assert_eq!(snapshot.len(), 1);
    assert!(tasks.registry.try_lock().is_some());
    dispose_app_task_for_test(&executor, &tasks, info.task_id).unwrap();
}
