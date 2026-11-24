use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use crate::internal::hanafubuki_process::executor::{ExecutorIoMode, ExecutorStatus};
use crate::internal::hanafubuki_process::ownership::RegistryReservation;
#[cfg(any(test, feature = "test-support"))]
use parking_lot::Condvar;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::internal::hanafubuki_task::owned_task::OwnedTaskRegistry;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct AppTaskScope;

pub(super) type AppTaskRegistry = OwnedTaskRegistry<AppTaskScope, AppTask>;
pub type AppTaskReservation = RegistryReservation;

/// In-memory registry of app-owned tasks. App and Instance tasks deliberately
/// keep separate registry values and locks.
#[derive(Clone, Default)]
pub struct AppTaskState {
    pub(super) registry: Arc<Mutex<AppTaskRegistry>>,
    pub(super) next_id: Arc<AtomicU64>,
    pub(super) fail_reserved_metadata: Arc<AtomicBool>,
    #[cfg(any(test, feature = "test-support"))]
    pub(super) generic_commit_gate: Arc<Mutex<Option<Arc<TaskCommitGate>>>>,
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct TaskCommitGate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

#[cfg(any(test, feature = "test-support"))]
impl TaskCommitGate {
    pub fn wait_until_entered(&self) {
        let mut state = self.state.lock();
        while !state.0 {
            self.changed.wait(&mut state);
        }
    }

    pub fn release(&self) {
        self.state.lock().1 = true;
        self.changed.notify_all();
    }

    pub(super) fn pause(&self) {
        let mut state = self.state.lock();
        state.0 = true;
        self.changed.notify_all();
        while !state.1 {
            self.changed.wait(&mut state);
        }
    }
}

/// Metadata for one app-owned task. Status is derived live from the executor.
#[derive(Clone, Debug)]
pub struct AppTask {
    pub task_id: String,
    pub service_id: Option<String>,
    pub executor_id: String,
    pub label: Option<String>,
    pub created_at: u128,
    pub updated_at: u128,
}

/// Request to start an app-owned task. `command` is argv (no shell parsing).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAppTaskRequest {
    pub command: Vec<String>,
    #[serde(default)]
    pub service_id: Option<String>,
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

/// Task metadata plus executor-derived status for the frontend.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTaskInfo {
    pub task_id: String,
    pub service_id: Option<String>,
    pub executor_id: String,
    pub label: Option<String>,
    pub created_at: u128,
    pub updated_at: u128,
    pub status: Option<ExecutorStatus>,
    pub exit_code: Option<i32>,
    pub io_mode: Option<ExecutorIoMode>,
    pub command: Option<Vec<String>>,
    pub output_offset: Option<u64>,
}
