#[cfg(unix)]
use std::sync::atomic::Ordering;

use crate::internal::hanafubuki_process::executor::manager::ExecutorManager;
#[cfg(unix)]
use crate::internal::hanafubuki_process::executor::manager::{
    executor_registry_counts, release_executor_reservation, reserve_executor_id,
};
#[cfg(unix)]
use crate::internal::hanafubuki_process::ownership::{OwnedReleaseStatus, ProcessOwnerId};

#[cfg(unix)]
use super::super::test_support::app_task_registry_counts;
use super::super::{AppTaskState, start_backend_app_task_inner};
#[cfg(unix)]
use super::super::{
    dispose_reserved_app_task_inner, executor_id_for, finalize_app_task_reservation,
    reserve_app_task_identity, start_reserved_app_task_inner, terminate_reserved_app_task_inner,
};
use super::fixtures::start_req;

#[test]
fn rejects_empty_command() {
    let executor = ExecutorManager::default();
    let tasks = AppTaskState::default();
    assert!(start_backend_app_task_inner(&executor, &tasks, start_req(&[])).is_err());
}

#[cfg(unix)]
#[test]
fn generic_app_task_cannot_commit_reserved_task_or_executor_identity() {
    use std::sync::{Arc, Barrier};

    let executor = Arc::new(ExecutorManager::default());
    let tasks = Arc::new(AppTaskState::default());
    let task_id = "reserved-app-race";
    let prepared = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let mut request = start_req(&["sh", "-c", "sleep 1"]);
    request.task_id = Some(task_id.to_string());
    let worker_executor = executor.clone();
    let worker_tasks = tasks.clone();
    let worker_prepared = prepared.clone();
    let worker_resume = resume.clone();
    let worker = std::thread::spawn(move || {
        worker_prepared.wait();
        worker_resume.wait();
        start_backend_app_task_inner(&worker_executor, &worker_tasks, request)
    });
    prepared.wait();
    let reservation_id = ProcessOwnerId::generate();
    let _task_reservation = reserve_app_task_identity(&tasks, task_id, &reservation_id).unwrap();
    let executor_reservation =
        reserve_executor_id(&executor, &executor_id_for(task_id), &reservation_id).unwrap();
    resume.wait();
    assert!(worker.join().unwrap().is_err());
    assert_eq!(app_task_registry_counts(&tasks), (0, 1));
    assert_eq!(executor_registry_counts(&executor), (0, 1, 0));
    assert_eq!(
        finalize_app_task_reservation(&tasks, task_id, &reservation_id),
        OwnedReleaseStatus::Released
    );
    assert!(release_executor_reservation(
        &executor,
        &executor_reservation
    ));
}

#[cfg(unix)]
#[test]
fn matching_app_reservation_commits_and_metadata_failure_cleans_only_receipt_executor() {
    let executor = ExecutorManager::default();
    let tasks = AppTaskState::default();
    let task_id = "reserved-app-start";
    let reservation_id = ProcessOwnerId::generate();
    let task_reservation = reserve_app_task_identity(&tasks, task_id, &reservation_id).unwrap();
    let executor_reservation =
        reserve_executor_id(&executor, &executor_id_for(task_id), &reservation_id).unwrap();
    let mut request = start_req(&["sh", "-c", "sleep 1"]);
    request.task_id = Some(task_id.to_string());
    let (_, receipt) = start_reserved_app_task_inner(
        &executor,
        &tasks,
        request,
        &task_reservation,
        &executor_reservation,
    )
    .unwrap();
    assert_eq!(app_task_registry_counts(&tasks), (1, 0));
    dispose_reserved_app_task_inner(&executor, &tasks, &receipt).unwrap();

    let failed_id = "reserved-app-metadata-failure";
    let failed_capability = ProcessOwnerId::generate();
    let failed_task = reserve_app_task_identity(&tasks, failed_id, &failed_capability).unwrap();
    let failed_executor =
        reserve_executor_id(&executor, &executor_id_for(failed_id), &failed_capability).unwrap();
    tasks.fail_reserved_metadata.store(true, Ordering::SeqCst);
    let mut failed_request = start_req(&["sh", "-c", "sleep 1"]);
    failed_request.task_id = Some(failed_id.to_string());
    let error = start_reserved_app_task_inner(
        &executor,
        &tasks,
        failed_request,
        &failed_task,
        &failed_executor,
    )
    .unwrap_err();
    let created = error.created.expect("executor commit receipt");
    assert_eq!(executor_registry_counts(&executor), (1, 0, 1));
    assert_eq!(app_task_registry_counts(&tasks), (0, 1));
    terminate_reserved_app_task_inner(&executor, &tasks, &created).unwrap();
    dispose_reserved_app_task_inner(&executor, &tasks, &created).unwrap();
    assert_eq!(
        finalize_app_task_reservation(&tasks, failed_id, &failed_capability),
        OwnedReleaseStatus::Released
    );
    assert_eq!(executor_registry_counts(&executor), (0, 0, 0));
    assert_eq!(app_task_registry_counts(&tasks), (0, 0));
}

#[cfg(unix)]
#[test]
fn reserved_spawn_failure_has_no_created_receipt() {
    let executor = ExecutorManager::default();
    let tasks = AppTaskState::default();
    let task_id = "reserved-spawn-failure";
    let reservation_id = ProcessOwnerId::generate();
    let task_reservation = reserve_app_task_identity(&tasks, task_id, &reservation_id).unwrap();
    let executor_reservation =
        reserve_executor_id(&executor, &executor_id_for(task_id), &reservation_id).unwrap();
    let mut request = start_req(&["/definitely/missing/hanafubuki-command"]);
    request.task_id = Some(task_id.to_string());
    let error = start_reserved_app_task_inner(
        &executor,
        &tasks,
        request,
        &task_reservation,
        &executor_reservation,
    )
    .unwrap_err();
    assert!(error.created.is_none());
    assert_eq!(executor_registry_counts(&executor), (0, 1, 0));
    assert!(release_executor_reservation(
        &executor,
        &executor_reservation
    ));
    assert_eq!(app_task_registry_counts(&tasks), (0, 1));
    assert_eq!(
        finalize_app_task_reservation(&tasks, task_id, &reservation_id),
        OwnedReleaseStatus::Released
    );
}
