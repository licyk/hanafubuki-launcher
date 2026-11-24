use serde::{Deserialize, Serialize};

use crate::internal::hanafubuki_config::model::settings_domain::SettingsDomain;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigPersistenceHealth {
    #[default]
    Healthy,
    ReadOnlyDegraded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigRecoveryStatus {
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigRecoveryAttention {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ConfigFileKind {
    /// One per-domain global settings document. Recovery is per document, so a
    /// report names the domain it repaired rather than "the settings file".
    GlobalSettings {
        domain: SettingsDomain,
    },
    InstanceConfig {
        instance_id: String,
    },
    /// One of the Agent's configuration files (`file` is its name), which
    /// the Agent repairs itself; the integration maps its recoveries here.
    AgentConfig {
        file: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigRecoveryPhase {
    Detected,
    Staged,
    Quarantined,
    ReplacementCommitted,
    ReportCommitted,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigRecoveryActionKind {
    DefaultApplied,
    FieldRemoved,
    GroupReset,
    BackupRestored,
    OriginalQuarantined,
    FileRegenerated,
    InMemoryFallback,
    PersistenceFailed,
    InstanceDisabled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRecoveryAction {
    pub kind: ConfigRecoveryActionKind,
    pub relative_path: String,
    pub json_pointer: Option<String>,
    pub reason_code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRecoveryReport {
    pub recovery_id: String,
    pub file_kind: ConfigFileKind,
    pub status: ConfigRecoveryStatus,
    pub attention: ConfigRecoveryAttention,
    pub phase: ConfigRecoveryPhase,
    pub created_at: u128,
    pub updated_at: u128,
    pub actions: Vec<ConfigRecoveryAction>,
    pub quarantine_relative_path: Option<String>,
    pub error: Option<String>,
    pub log_lines: Vec<String>,
}

impl ConfigRecoveryReport {
    pub fn log_bytes(&self) -> Vec<u8> {
        let mut bytes = self.log_lines.join("\n").into_bytes();
        if !bytes.is_empty() {
            bytes.push(b'\n');
        }
        bytes
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfigIssue {
    pub instance_id: String,
    pub recovery_id: String,
    pub reason: String,
    pub quarantine_relative_path: Option<String>,
}
