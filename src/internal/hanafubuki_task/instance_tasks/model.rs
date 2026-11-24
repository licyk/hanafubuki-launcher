use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use crate::internal::hanafubuki_process::executor::{ExecutorIoMode, ExecutorStatus};
use crate::internal::hanafubuki_process::ownership::RegistryReservation;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::internal::hanafubuki_task::owned_task::OwnedTaskRegistry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InstanceTaskScope {
    instance_id: String,
}

impl InstanceTaskScope {
    pub(super) fn new(instance_id: String) -> Self {
        Self { instance_id }
    }

    pub(super) fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

pub(super) type InstanceTaskRegistry = OwnedTaskRegistry<InstanceTaskScope, InstanceTask>;
pub type InstanceTaskReservation = RegistryReservation;

#[derive(Clone, Default)]
pub struct InstanceTaskState {
    pub(super) registry: Arc<Mutex<InstanceTaskRegistry>>,
    pub(super) next_id: Arc<AtomicU64>,
    pub(super) fail_reserved_metadata: Arc<AtomicBool>,
}

/// Metadata for one instance-owned task. Live status remains Executor-derived.
#[derive(Clone, Debug)]
pub struct InstanceTask {
    pub task_id: String,
    pub instance_id: String,
    pub executor_id: String,
    pub label: Option<String>,
    pub created_at: u128,
    pub updated_at: u128,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInstanceTaskRequest {
    pub instance_id: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: HashMap<String, Option<String>>,
    #[serde(default)]
    pub io_mode: ExecutorIoMode,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub output_bytes_cap: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceTaskInfo {
    pub task_id: String,
    pub instance_id: String,
    pub executor_id: String,
    pub label: Option<String>,
    pub created_at: u128,
    pub updated_at: u128,
    pub status: Option<ExecutorStatus>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub io_mode: Option<ExecutorIoMode>,
    pub command: Option<Vec<String>>,
    pub output_offset: Option<u64>,
}
