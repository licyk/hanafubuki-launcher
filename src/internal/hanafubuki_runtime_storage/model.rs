use std::path::PathBuf;

use crate::internal::hanafubuki_config::{ManagedRuntimeLayoutKind, ManagedRuntimePaths};

pub const RUNTIME_STORAGE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStorageConfig {
    pub schema_version: u32,
    pub revision: u64,
    pub managed_runtime_root: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MigrationPhase {
    Scheduled,
    Transferring,
    CommittingConfig,
    CleanupPending,
    Completed,
    RecoveryRequired,
}

impl MigrationPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed)
    }

    /// A start that opens the storage authority holds the exclusive process
    /// lock, so the application that scheduled this work has exited. That is
    /// the only precondition: how the previous run ended — an ordered shutdown
    /// with or without diagnostics, a crash, or a kill — does not gate it.
    pub fn may_migrate_at_startup(self) -> bool {
        matches!(
            self,
            Self::Scheduled | Self::Transferring | Self::CommittingConfig
        )
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStorageJournal {
    pub schema_version: u32,
    pub operation_id: String,
    pub expected_config_revision: u64,
    pub source_layout: ManagedRuntimeLayoutKind,
    pub source_root: PathBuf,
    pub destination_root: PathBuf,
    pub phase: MigrationPhase,
    pub last_error: Option<String>,
}

impl RuntimeStorageJournal {
    pub fn source_paths(&self) -> Result<ManagedRuntimePaths, String> {
        match self.source_layout {
            ManagedRuntimeLayoutKind::Unified => {
                ManagedRuntimePaths::under(self.source_root.clone())
            }
            ManagedRuntimeLayoutKind::Legacy => {
                ManagedRuntimePaths::legacy_under(self.source_root.clone())
            }
        }
    }
}

/// Where one managed component ends up once the migration commits. The plan is
/// pure path arithmetic: it never walks, counts or hashes source files.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationComponentTarget {
    pub name: String,
    pub destination: PathBuf,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStorageMigrationPlan {
    pub destination_root: PathBuf,
    pub components: Vec<MigrationComponentTarget>,
}

/// Why an existing runtime storage config cannot be used.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeStorageConfigProblem {
    /// The config cannot be read or decoded.
    Invalid,
    /// A newer version wrote it; this version does not overwrite it.
    NewerSchema,
}

/// An existing config this version cannot use. The file is left in place
/// untouched: the session runs on the default (or legacy) location without
/// committing it, and moving storage is refused until the user resolves it.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStorageConfigAttention {
    pub problem: RuntimeStorageConfigProblem,
    pub config_file: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeStorageSnapshot {
    pub config_revision: u64,
    pub current_layout: ManagedRuntimeLayoutKind,
    pub current_root: PathBuf,
    pub default_root: PathBuf,
    pub paths: ManagedRuntimePaths,
    pub pending: Option<RuntimeStorageJournal>,
    /// Set while the existing config needs the user's attention.
    pub config_attention: Option<RuntimeStorageConfigAttention>,
}
