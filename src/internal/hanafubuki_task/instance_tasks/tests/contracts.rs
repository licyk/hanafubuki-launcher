use crate::internal::hanafubuki_process::executor::ExecutorIoMode;

use super::super::StartInstanceTaskRequest;

#[test]
fn instance_task_io_mode_contract_defaults_to_pipe_and_ignores_the_removed_boolean() {
    let defaulted: StartInstanceTaskRequest = serde_json::from_value(serde_json::json!({
        "instanceId": "one",
        "command": ["tool"]
    }))
    .unwrap();
    assert_eq!(defaulted.io_mode, ExecutorIoMode::Pipe);

    let terminal: StartInstanceTaskRequest = serde_json::from_value(serde_json::json!({
        "instanceId": "one",
        "command": ["tool"],
        "ioMode": "pty"
    }))
    .unwrap();
    assert_eq!(terminal.io_mode, ExecutorIoMode::Pty);

    let removed: StartInstanceTaskRequest = serde_json::from_value(serde_json::json!({
        "instanceId": "one",
        "command": ["tool"],
        "pty": true
    }))
    .unwrap();
    assert_eq!(removed.io_mode, ExecutorIoMode::Pipe);
}
