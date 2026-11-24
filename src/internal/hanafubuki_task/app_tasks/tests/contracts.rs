use crate::internal::hanafubuki_process::executor::ExecutorIoMode;

use super::super::{StartAppTaskRequest, executor_id_for};

#[test]
fn app_task_io_mode_contract_defaults_to_pipe_and_ignores_the_removed_boolean() {
    let defaulted: StartAppTaskRequest =
        serde_json::from_value(serde_json::json!({ "command": ["tool"] })).unwrap();
    assert_eq!(defaulted.io_mode, ExecutorIoMode::Pipe);

    let terminal: StartAppTaskRequest = serde_json::from_value(serde_json::json!({
        "command": ["tool"],
        "ioMode": "pty"
    }))
    .unwrap();
    assert_eq!(terminal.io_mode, ExecutorIoMode::Pty);

    let removed: StartAppTaskRequest = serde_json::from_value(serde_json::json!({
        "command": ["tool"],
        "pty": true
    }))
    .unwrap();
    assert_eq!(removed.io_mode, ExecutorIoMode::Pipe);
}

#[test]
fn executor_id_encoding_is_unambiguous_and_prefixed() {
    assert_ne!(executor_id_for("a:task:1:b"), executor_id_for("a"));
    assert!(executor_id_for("svc").starts_with("app:task:"));
    assert_eq!(executor_id_for("svc"), "app:task:3:svc");
}
