use std::collections::HashMap;

use crate::internal::hanafubuki_process::executor::ExecutorIoMode;

use super::super::StartInstanceTaskRequest;

pub(super) fn request(instance_id: &str, command: Vec<String>) -> StartInstanceTaskRequest {
    StartInstanceTaskRequest {
        instance_id: instance_id.to_string(),
        command,
        label: None,
        task_id: Some("task-one".to_string()),
        cwd: None,
        env: HashMap::new(),
        io_mode: ExecutorIoMode::Pipe,
        cols: None,
        rows: None,
        timeout_ms: None,
        output_bytes_cap: None,
    }
}
