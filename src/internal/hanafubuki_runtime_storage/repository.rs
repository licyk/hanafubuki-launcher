use std::fs::{File, OpenOptions};
use std::path::Path;

use crate::internal::hanafubuki_config::BootstrapPaths;
use crate::internal::hanafubuki_config::repository::safe_file::{read_json, write_json_atomic};
use crate::internal::hanafubuki_foundation::json_normalization::{
    JsonField, JsonShape, OPAQUE_JSON_SHAPE, normalize_current_json,
};
use crate::internal::hanafubuki_foundation::persistence::quarantine_safe_json;
use serde_json::Value;

use crate::internal::hanafubuki_runtime_storage::model::{
    RUNTIME_STORAGE_SCHEMA_VERSION, RuntimeStorageConfig, RuntimeStorageConfigAttention,
    RuntimeStorageConfigProblem, RuntimeStorageJournal,
};

const CONFIG_FIELDS: &[JsonField] = &[
    JsonField::new("schemaVersion", &OPAQUE_JSON_SHAPE),
    JsonField::new("revision", &OPAQUE_JSON_SHAPE),
    JsonField::new("managedRuntimeRoot", &OPAQUE_JSON_SHAPE),
];
const CONFIG_SHAPE: JsonShape = JsonShape::Object(CONFIG_FIELDS);
const JOURNAL_FIELDS: &[JsonField] = &[
    JsonField::new("schemaVersion", &OPAQUE_JSON_SHAPE),
    JsonField::new("operationId", &OPAQUE_JSON_SHAPE),
    JsonField::new("expectedConfigRevision", &OPAQUE_JSON_SHAPE),
    JsonField::new("sourceLayout", &OPAQUE_JSON_SHAPE),
    JsonField::new("sourceRoot", &OPAQUE_JSON_SHAPE),
    JsonField::new("destinationRoot", &OPAQUE_JSON_SHAPE),
    JsonField::new("phase", &OPAQUE_JSON_SHAPE),
    JsonField::new("lastError", &OPAQUE_JSON_SHAPE),
];
const JOURNAL_SHAPE: JsonShape = JsonShape::Object(JOURNAL_FIELDS);

pub(super) struct RuntimeStorageProcessLock {
    file: File,
}

impl RuntimeStorageProcessLock {
    pub(super) fn acquire(paths: &BootstrapPaths) -> Result<Self, String> {
        paths.prepare()?;
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&paths.runtime_storage_lock_file)
            .map_err(|error| {
                format!(
                    "open runtime storage process lock {}: {error}",
                    paths.runtime_storage_lock_file.display()
                )
            })?;
        file.try_lock().map_err(|error| {
            format!(
                "another application process owns managed runtime storage {}: {error}",
                paths.runtime_storage_lock_file.display()
            )
        })?;
        Ok(Self { file })
    }
}

impl Drop for RuntimeStorageProcessLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// What the runtime storage config holds.
pub(super) enum ConfigRead {
    /// No config yet: a clean install, or one still on the legacy layout.
    Missing,
    Valid(RuntimeStorageConfig),
    /// A config exists but this version cannot use it. It stays where it is,
    /// untouched, so no later start mistakes it for a clean install and
    /// commits the default root over the user's choice.
    NeedsAttention(RuntimeStorageConfigAttention),
}

impl ConfigRead {
    pub(super) fn valid(self) -> Option<RuntimeStorageConfig> {
        match self {
            Self::Valid(config) => Some(config),
            Self::Missing | Self::NeedsAttention(_) => None,
        }
    }
}

pub(super) fn load_config(paths: &BootstrapPaths) -> Result<ConfigRead, String> {
    let file = &paths.runtime_storage_config_file;
    let attention = |problem, message: String| {
        log::warn!(
            "runtime storage config needs attention path={} problem={problem:?}: {message}",
            file.display()
        );
        Ok(ConfigRead::NeedsAttention(RuntimeStorageConfigAttention {
            problem,
            config_file: file.clone(),
            message,
        }))
    };
    let raw = match read_json::<Value>(file) {
        Ok(Some(value)) => value,
        Ok(None) => return Ok(ConfigRead::Missing),
        Err(error) => return attention(RuntimeStorageConfigProblem::Invalid, error),
    };
    let original = raw;
    let mut canonical = original.clone();
    normalize_current_json(&mut canonical, &CONFIG_SHAPE, |_| {});
    let Some(object) = canonical.as_object_mut() else {
        return attention(
            RuntimeStorageConfigProblem::Invalid,
            "the config root is not an object".to_string(),
        );
    };
    match object.get("schemaVersion").map(Value::as_u64) {
        Some(Some(version)) if version > u64::from(RUNTIME_STORAGE_SCHEMA_VERSION) => {
            return attention(
                RuntimeStorageConfigProblem::NewerSchema,
                format!(
                    "the config was written by a newer version (schema {version}, this version reads {RUNTIME_STORAGE_SCHEMA_VERSION})"
                ),
            );
        }
        _ => {}
    }
    object.insert(
        "schemaVersion".to_string(),
        Value::from(RUNTIME_STORAGE_SCHEMA_VERSION),
    );
    if object.get("revision").and_then(Value::as_u64).unwrap_or(0) == 0 {
        object.insert("revision".to_string(), Value::from(1_u64));
    }
    let config = match serde_json::from_value::<RuntimeStorageConfig>(canonical.clone()) {
        Ok(config) => config,
        Err(error) => return attention(RuntimeStorageConfigProblem::Invalid, error.to_string()),
    };
    if canonical != original {
        replace_repaired_document(file, "runtime storage config", &config)?;
    }
    Ok(ConfigRead::Valid(config))
}

/// Commit a new managed runtime root under the observed revision. A config
/// that needs attention is never replaced here: only the user resolves it.
pub(super) fn commit_config(
    paths: &BootstrapPaths,
    expected_revision: u64,
    managed_runtime_root: &Path,
) -> Result<RuntimeStorageConfig, String> {
    let current_revision = match load_config(paths)? {
        ConfigRead::Valid(config) => config.revision,
        ConfigRead::Missing => 0,
        ConfigRead::NeedsAttention(attention) => {
            return Err(format!(
                "runtime storage config {} needs attention and was left unchanged: {}",
                attention.config_file.display(),
                attention.message
            ));
        }
    };
    if current_revision != expected_revision {
        return Err(format!(
            "runtime storage config revision changed: expected {expected_revision}, found {current_revision}"
        ));
    }
    let config = RuntimeStorageConfig {
        schema_version: RUNTIME_STORAGE_SCHEMA_VERSION,
        revision: expected_revision.saturating_add(1),
        managed_runtime_root: managed_runtime_root.to_path_buf(),
    };
    write_json_atomic(&paths.runtime_storage_config_file, &config)?;
    Ok(config)
}

pub(super) fn load_journal(
    paths: &BootstrapPaths,
) -> Result<Option<RuntimeStorageJournal>, String> {
    let file = &paths.runtime_storage_journal_file;
    let raw = match read_json::<Value>(file) {
        Ok(value) => value,
        Err(error) => {
            reset_invalid_document(file, "runtime storage journal", &error)?;
            return Ok(None);
        }
    };
    let Some(raw) = raw else {
        return Ok(None);
    };
    let original = raw;
    let mut canonical = original.clone();
    normalize_current_json(&mut canonical, &JOURNAL_SHAPE, |_| {});
    let Some(object) = canonical.as_object_mut() else {
        reset_invalid_document(file, "runtime storage journal", "root is not an object")?;
        return Ok(None);
    };
    object.insert(
        "schemaVersion".to_string(),
        Value::from(RUNTIME_STORAGE_SCHEMA_VERSION),
    );
    let journal = match serde_json::from_value::<RuntimeStorageJournal>(canonical.clone()) {
        Ok(journal) => journal,
        Err(error) => {
            reset_invalid_document(file, "runtime storage journal", &error.to_string())?;
            return Ok(None);
        }
    };
    if canonical != original {
        replace_repaired_document(file, "runtime storage journal", &journal)?;
    }
    Ok(Some(journal))
}

pub(super) fn write_journal(
    paths: &BootstrapPaths,
    journal: &RuntimeStorageJournal,
) -> Result<(), String> {
    write_json_atomic(&paths.runtime_storage_journal_file, journal)
}

fn replace_repaired_document<T: serde::Serialize>(
    file: &Path,
    label: &str,
    value: &T,
) -> Result<(), String> {
    let quarantined = quarantine_safe_json(file)?;
    write_json_atomic(file, value)?;
    log::warn!(
        "destructively repaired {label} path={} quarantined={}",
        file.display(),
        quarantined.len()
    );
    Ok(())
}

fn reset_invalid_document(file: &Path, label: &str, error: &str) -> Result<(), String> {
    let quarantined = quarantine_safe_json(file)?;
    log::warn!(
        "discarded incompatible {label} path={} quarantined={} error={error}",
        file.display(),
        quarantined.len()
    );
    Ok(())
}
