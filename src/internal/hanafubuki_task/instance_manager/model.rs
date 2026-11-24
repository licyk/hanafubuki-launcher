use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::internal::hanafubuki_process::ownership::ProcessOwnerId;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Capability proving that the application layer has resolved an instance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InstanceIdentity(String);

impl InstanceIdentity {
    pub fn from_validated(instance_id: impl Into<String>) -> Result<Self, String> {
        let instance_id = instance_id.into();
        if instance_id.trim().is_empty() {
            return Err("instance_id must not be empty".to_string());
        }
        Ok(Self(instance_id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancePaths {
    pub instance_path: PathBuf,
    pub data_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceProcessKind {
    Executor,
    InteractiveTerminal,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceProcessRef {
    pub instance_id: String,
    pub process_id: String,
    pub kind: InstanceProcessKind,
    pub label: Option<String>,
    pub created_at: String,
    #[serde(skip)]
    pub(super) reservation_id: Option<ProcessOwnerId>,
}

#[derive(Clone, Default)]
pub struct InstanceManagerState {
    pub(super) processes: Arc<Mutex<HashMap<String, Vec<InstanceProcessRef>>>>,
    pub(super) fail_reserved_registration: Arc<AtomicBool>,
}
