use std::collections::HashMap;

use crate::internal::hanafubuki_process::executor::ExecutorIoMode;

use super::super::StartAppTaskRequest;

pub(super) fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|value| value.to_string()).collect()
}

pub(super) fn start_req(command: &[&str]) -> StartAppTaskRequest {
    StartAppTaskRequest {
        command: owned(command),
        service_id: None,
        label: None,
        task_id: None,
        cwd: Some(std::env::temp_dir()),
        env: HashMap::new(),
        io_mode: ExecutorIoMode::Pipe,
        cols: None,
        rows: None,
        timeout_ms: None,
        output_bytes_cap: None,
    }
}
