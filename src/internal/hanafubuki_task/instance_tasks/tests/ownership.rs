use crate::internal::hanafubuki_process::executor::manager::ExecutorManager;
#[cfg(unix)]
use crate::internal::hanafubuki_process::executor::manager::reserve_executor_id;
use crate::internal::hanafubuki_process::ownership::{ProcessOwnerId, ReservedProcessReceipt};

use crate::internal::hanafubuki_task::instance_manager::{
    InstanceIdentity, InstanceManagerState, InstanceProcessKind,
    list_instance_processes_registered_inner, register_reserved_instance_process_inner,
};

use super::super::model::InstanceTaskScope;
use super::super::test_support::instance_task_registry_counts;
use super::super::{
    InstanceTask, InstanceTaskState, dispose_reserved_instance_task_inner, executor_id_for,
    reserve_instance_task_identity,
};
#[cfg(unix)]
use super::super::{
    get_reserved_instance_task_executor_info, list_instance_tasks_inner,
    read_owned_instance_task_output_tail_inner, reserved_instance_task_exists,
    start_backend_instance_task_with_receipt_inner, start_reserved_instance_task_inner,
    terminate_reserved_instance_task_inner,
};
#[cfg(unix)]
use super::fixtures::request;

#[cfg(unix)]
#[test]
fn validated_identity_drives_start_list_and_confirmed_dispose() {
    let identity = InstanceIdentity::from_validated("one").unwrap();
    let executor = ExecutorManager::default();
    let instances = InstanceManagerState::default();
    let tasks = InstanceTaskState::default();

    let (started, receipt) = start_backend_instance_task_with_receipt_inner(
        &identity,
        &executor,
        &instances,
        &tasks,
        request(
            "one",
            vec![
                "sh".to_string(),
                "-c".to_string(),
                "printf task".to_string(),
            ],
        ),
    )
    .unwrap();
    assert_eq!(started.instance_id, "one");
    assert_eq!(
        list_instance_tasks_inner(&identity, &executor, &tasks)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        list_instance_processes_registered_inner(&instances, &identity).len(),
        1
    );

    terminate_reserved_instance_task_inner(&executor, &tasks, &receipt).unwrap();
    dispose_reserved_instance_task_inner(&executor, &instances, &tasks, &receipt).unwrap();
    assert!(
        list_instance_tasks_inner(&identity, &executor, &tasks)
            .unwrap()
            .is_empty()
    );
    assert!(list_instance_processes_registered_inner(&instances, &identity).is_empty());
    assert!(!reserved_instance_task_exists(&executor, &tasks, &receipt));
}

#[cfg(unix)]
#[test]
fn owned_output_tail_returns_the_latest_bounded_bytes_without_exposing_mutation() {
    let identity = InstanceIdentity::from_validated("one").unwrap();
    let executor = ExecutorManager::default();
    let instances = InstanceManagerState::default();
    let tasks = InstanceTaskState::default();
    let (started, receipt) = start_backend_instance_task_with_receipt_inner(
        &identity,
        &executor,
        &instances,
        &tasks,
        request(
            "one",
            vec![
                "sh".to_string(),
                "-c".to_string(),
                "printf 0123456789".to_string(),
            ],
        ),
    )
    .unwrap();

    let output = (0..100)
        .find_map(|_| {
            let output = read_owned_instance_task_output_tail_inner(
                &executor,
                &tasks,
                "one",
                &started.task_id,
                4,
            )
            .unwrap();
            if output.next_offset >= 10 {
                Some(output)
            } else {
                std::thread::sleep(std::time::Duration::from_millis(2));
                None
            }
        })
        .expect("task output should arrive");
    let bytes = output
        .chunks
        .into_iter()
        .flat_map(|chunk| chunk.data)
        .collect::<Vec<_>>();
    assert_eq!(bytes, b"6789");
    assert_eq!(output.start_offset, 6);
    assert_eq!(output.next_offset, 10);
    assert!(output.truncated);

    dispose_reserved_instance_task_inner(&executor, &instances, &tasks, &receipt).unwrap();
}

#[cfg(unix)]
#[test]
fn reserved_api_task_is_mutable_only_with_its_receipt() {
    let identity = InstanceIdentity::from_validated("one").unwrap();
    let executor = ExecutorManager::default();
    let instances = InstanceManagerState::default();
    let tasks = InstanceTaskState::default();
    let task_id = "instance-api-owner";
    let owner = ProcessOwnerId::generate();
    let task_reservation = reserve_instance_task_identity(&tasks, "one", task_id, &owner).unwrap();
    let executor_reservation =
        reserve_executor_id(&executor, &executor_id_for("one", task_id), &owner).unwrap();
    let mut start = request(
        "one",
        vec!["sh".to_string(), "-c".to_string(), "sleep 30".to_string()],
    );
    start.task_id = Some(task_id.to_string());
    let (_, receipt) = start_reserved_instance_task_inner(
        &identity,
        &executor,
        &instances,
        &tasks,
        start,
        &task_reservation,
        &executor_reservation,
    )
    .unwrap();

    let stale = ReservedProcessReceipt::for_test(
        task_id,
        receipt.executor_id(),
        ProcessOwnerId::generate(),
    );
    assert!(get_reserved_instance_task_executor_info(&executor, &tasks, &stale).is_err());
    terminate_reserved_instance_task_inner(&executor, &tasks, &receipt).unwrap();
    dispose_reserved_instance_task_inner(&executor, &instances, &tasks, &receipt).unwrap();
}

#[test]
fn failed_confirmed_cleanup_retains_task_receipt_and_instance_process_index() {
    let identity = InstanceIdentity::from_validated("one").unwrap();
    let executor = ExecutorManager::default();
    let instances = InstanceManagerState::default();
    let tasks = InstanceTaskState::default();
    let task_id = "retained-cleanup";
    let executor_id = executor_id_for("one", task_id);
    let owner = ProcessOwnerId::generate();
    let reservation = reserve_instance_task_identity(&tasks, "one", task_id, &owner).unwrap();
    let receipt = ReservedProcessReceipt::for_test(task_id, &executor_id, owner.clone());
    register_reserved_instance_process_inner(
        &instances,
        &identity,
        executor_id.clone(),
        InstanceProcessKind::Executor,
        None,
        &owner,
    )
    .unwrap();
    tasks
        .registry
        .lock()
        .commit_reserved(
            &reservation,
            executor_id,
            InstanceTaskScope::new("one".to_string()),
            InstanceTask {
                task_id: task_id.to_string(),
                instance_id: "one".to_string(),
                executor_id: receipt.executor_id().to_string(),
                label: None,
                created_at: 0,
                updated_at: 0,
            },
            receipt.clone(),
        )
        .unwrap();

    assert!(dispose_reserved_instance_task_inner(&executor, &instances, &tasks, &receipt).is_err());
    assert_eq!(instance_task_registry_counts(&tasks), (1, 0));
    assert_eq!(
        list_instance_processes_registered_inner(&instances, &identity).len(),
        1
    );
}

#[cfg(unix)]
#[test]
fn list_snapshots_release_registry_before_live_executor_projection() {
    use super::super::queries::{snapshot_all_tasks, snapshot_instance_tasks};

    let identity = InstanceIdentity::from_validated("one").unwrap();
    let executor = ExecutorManager::default();
    let instances = InstanceManagerState::default();
    let tasks = InstanceTaskState::default();
    let (_, receipt) = start_backend_instance_task_with_receipt_inner(
        &identity,
        &executor,
        &instances,
        &tasks,
        request("one", vec!["/bin/echo".to_string(), "snapshot".to_string()]),
    )
    .unwrap();

    assert_eq!(snapshot_all_tasks(&tasks).len(), 1);
    assert!(tasks.registry.try_lock().is_some());
    assert_eq!(snapshot_instance_tasks(&tasks, "one").len(), 1);
    assert!(tasks.registry.try_lock().is_some());
    dispose_reserved_instance_task_inner(&executor, &instances, &tasks, &receipt).unwrap();
}
