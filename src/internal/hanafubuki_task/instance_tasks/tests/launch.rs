use crate::internal::hanafubuki_process::executor::manager::ExecutorManager;

#[cfg(unix)]
use crate::internal::hanafubuki_task::instance_manager::inject_reserved_instance_registration_failure;
use crate::internal::hanafubuki_task::instance_manager::{
    InstanceIdentity, InstanceManagerState, list_instance_processes_registered_inner,
};

use super::super::test_support::instance_task_registry_counts;
use super::super::{InstanceTaskState, start_backend_instance_task_with_receipt_inner};
use super::fixtures::request;

#[test]
fn mismatched_validated_identity_fails_before_reservation_or_spawn() {
    let identity = InstanceIdentity::from_validated("one").unwrap();
    let executor = ExecutorManager::default();
    let instances = InstanceManagerState::default();
    let tasks = InstanceTaskState::default();

    let error = start_backend_instance_task_with_receipt_inner(
        &identity,
        &executor,
        &instances,
        &tasks,
        request("two", vec!["unused".to_string()]),
    )
    .unwrap_err();

    assert!(error.message.contains("identity does not match"));
    assert!(error.created.is_none());
    assert_eq!(instance_task_registry_counts(&tasks), (0, 0));
    assert!(list_instance_processes_registered_inner(&instances, &identity).is_empty());
}

#[cfg(unix)]
#[test]
fn generic_registration_failure_cleans_exact_spawn_without_retaining_indexes() {
    let identity = InstanceIdentity::from_validated("one").unwrap();
    let executor = ExecutorManager::default();
    let instances = InstanceManagerState::default();
    let tasks = InstanceTaskState::default();
    inject_reserved_instance_registration_failure(&instances);

    let error = start_backend_instance_task_with_receipt_inner(
        &identity,
        &executor,
        &instances,
        &tasks,
        request(
            "one",
            vec!["sh".to_string(), "-c".to_string(), "sleep 30".to_string()],
        ),
    )
    .unwrap_err();

    assert!(
        error
            .message
            .contains("failed to register it with instance manager")
    );
    assert!(error.created.is_none());
    assert_eq!(instance_task_registry_counts(&tasks), (0, 0));
    assert!(list_instance_processes_registered_inner(&instances, &identity).is_empty());
}
