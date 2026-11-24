use super::*;

use std::time::{SystemTime, UNIX_EPOCH};

fn identity(id: &str) -> InstanceIdentity {
    InstanceIdentity::from_validated(id).unwrap()
}

#[test]
fn identity_rejects_blank_values() {
    assert!(InstanceIdentity::from_validated("  ").is_err());
}

#[test]
fn instance_paths_create_only_the_active_instance_directories() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "hanafubuki-instance-paths-{}-{stamp}",
        std::process::id()
    ));

    let paths = build_instance_paths(root.clone()).unwrap();

    assert_eq!(paths.instance_path, root);
    assert_eq!(paths.data_dir, paths.instance_path.join("data"));
    assert!(paths.instance_path.is_dir());
    assert!(paths.data_dir.is_dir());
    assert!(!paths.instance_path.join("runtime").exists());
    assert!(!paths.instance_path.join("logs").exists());
    assert!(!paths.instance_path.join("terminals").exists());

    std::fs::remove_dir_all(&paths.instance_path).unwrap();
}

#[test]
fn process_index_is_scoped_by_validated_identity() {
    let state = InstanceManagerState::default();
    let one = identity("one");
    let two = identity("two");
    register_instance_process_inner(
        &state,
        &one,
        "process-one".to_string(),
        InstanceProcessKind::Executor,
        None,
    )
    .unwrap();
    assert_eq!(
        list_instance_processes_registered_inner(&state, &one).len(),
        1
    );
    assert!(list_instance_processes_registered_inner(&state, &two).is_empty());
    assert!(
        assert_instance_owns_process(&state, &two, "process-one", InstanceProcessKind::Executor)
            .is_err()
    );
}
