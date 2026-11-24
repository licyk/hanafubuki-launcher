use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::internal::hanafubuki_config::AppPaths;
use crate::internal::hanafubuki_config::repository::instances::{
    InstanceConfig, PersistedInstanceConfigDecoder, PersistedInstanceIssue,
    PersistedInstanceIssueKind,
};

use super::filesystem::relative_display;
use super::json::{parse_unique_value, read_backup_value, read_bounded_text};
use super::model::{
    ConfigFileKind, ConfigRecoveryActionKind, ConfigRecoveryAttention, ConfigRecoveryReport,
    InstanceConfigIssue,
};
use super::report::{ReportBuilder, redact_error};
use super::transaction::{quarantine_and_replace, quarantine_only};

const MAX_INSTANCE_RECORDS: usize = 4096;

/// What the one instance reader found for one record.
pub(crate) enum InstanceConfigRead {
    Missing,
    Loaded(InstanceConfig),
    /// The record cannot be used and the instance is unavailable.
    Unavailable(InstanceConfigIssue),
}

pub(super) fn recover_instance_configs(
    paths: &AppPaths,
    writable: bool,
) -> (
    BTreeMap<String, InstanceConfigIssue>,
    Vec<ConfigRecoveryReport>,
) {
    let mut issues = BTreeMap::new();
    let mut reports = Vec::new();
    let Ok(entries) = fs::read_dir(&paths.instance_records_dir) else {
        return (issues, reports);
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    entries.truncate(MAX_INSTANCE_RECORDS);

    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(instance_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if crate::internal::hanafubuki_config::repository::instances::validate_instance_id(
            &instance_id,
        )
        .is_err()
        {
            continue;
        }
        let (read, report) = read_instance_config(paths, &instance_id, writable);
        if let InstanceConfigRead::Unavailable(issue) = read {
            issues.insert(instance_id, issue);
        }
        reports.extend(report);
    }
    (issues, reports)
}

/// Read, decode, and repair one instance config: the reader bootstrap and every
/// later read share.
///
/// The text is bounded and parsed without duplicate keys; an unusable main
/// file falls back to its verified backup. A repair moves the original into
/// quarantine before the replacement is committed, and a record that cannot be
/// used (unreadable, id mismatch, path policy) is quarantined and reported as
/// unavailable. Unless `writable`, nothing is written or moved: the repaired
/// config is used in memory, and only an unavailable record is reported.
pub(crate) fn read_instance_config(
    paths: &AppPaths,
    instance_id: &str,
    writable: bool,
) -> (InstanceConfigRead, Option<ConfigRecoveryReport>) {
    let file = paths.instance_config_file(instance_id);
    if !file.is_file() {
        return (InstanceConfigRead::Missing, None);
    }
    let mut report = ReportBuilder::new(
        ConfigFileKind::InstanceConfig {
            instance_id: instance_id.to_string(),
        },
        relative_display(paths, &file),
    );
    let (value, from_backup) = match read_instance_value(&file, &mut report) {
        Ok(read) => read,
        Err(error) => {
            let issue = disable_instance(paths, &file, instance_id, error, &mut report, writable);
            return (
                InstanceConfigRead::Unavailable(issue),
                Some(report.finish()),
            );
        }
    };

    let read = match repair_instance_value(paths, instance_id, value, &mut report) {
        // A config read from the backup always replaces the unusable main
        // file (which goes to quarantine), even when the backup is canonical.
        Ok((config, changed)) if (changed || from_backup) && writable => {
            match quarantine_and_replace(paths, &file, &config, &mut report) {
                Ok(()) => InstanceConfigRead::Loaded(config),
                Err(error) => InstanceConfigRead::Unavailable(disable_instance(
                    paths,
                    &file,
                    instance_id,
                    error,
                    &mut report,
                    writable,
                )),
            }
        }
        Ok((config, _)) => InstanceConfigRead::Loaded(config),
        Err(error) => InstanceConfigRead::Unavailable(disable_instance(
            paths,
            &file,
            instance_id,
            error,
            &mut report,
            writable,
        )),
    };
    let reported = writable || matches!(read, InstanceConfigRead::Unavailable(_));
    let report = (reported && !report.report.actions.is_empty()).then(|| report.finish());
    (read, report)
}

/// The config's JSON value, and whether it came from the backup.
fn read_instance_value(file: &Path, report: &mut ReportBuilder) -> Result<(Value, bool), String> {
    let error = match read_bounded_text(file)
        .and_then(|content| content.ok_or_else(|| "instance config disappeared".to_string()))
        .and_then(|content| parse_unique_value(&content).map_err(|error| error.to_string()))
    {
        Ok(value) => return Ok((value, false)),
        Err(error) => error,
    };
    match read_backup_value(file) {
        Ok(Some(value)) => {
            report.action(
                ConfigRecoveryActionKind::BackupRestored,
                None,
                "valid_backup",
                format!(
                    "the instance config is unusable ({}); a verified backup was selected as the recovery source",
                    redact_error(&error)
                ),
                ConfigRecoveryAttention::Warning,
            );
            Ok((value, true))
        }
        _ => Err(error),
    }
}

fn repair_instance_value(
    paths: &AppPaths,
    instance_id: &str,
    value: Value,
    report: &mut ReportBuilder,
) -> Result<(InstanceConfig, bool), String> {
    let decoded = PersistedInstanceConfigDecoder::new(paths).decode(instance_id, value)?;
    for issue in decoded.issues {
        record_persisted_instance_issue(report, issue);
    }
    Ok((decoded.config, decoded.changed))
}

fn record_persisted_instance_issue(report: &mut ReportBuilder, issue: PersistedInstanceIssue) {
    let pointer = Some(issue.json_pointer.as_str());
    match issue.kind {
        PersistedInstanceIssueKind::MissingField => report.action(
            ConfigRecoveryActionKind::DefaultApplied,
            pointer,
            "missing_field",
            issue.message,
            ConfigRecoveryAttention::Info,
        ),
        PersistedInstanceIssueKind::UnsupportedField => report.action(
            ConfigRecoveryActionKind::FieldRemoved,
            pointer,
            "unsupported_instance_field",
            issue.message,
            ConfigRecoveryAttention::Warning,
        ),
        PersistedInstanceIssueKind::InvalidField => report.action(
            ConfigRecoveryActionKind::DefaultApplied,
            pointer,
            "invalid_instance_field",
            issue.message,
            ConfigRecoveryAttention::Warning,
        ),
        PersistedInstanceIssueKind::InvalidSection => report.action(
            ConfigRecoveryActionKind::GroupReset,
            pointer,
            "invalid_instance_section",
            issue.message,
            ConfigRecoveryAttention::Warning,
        ),
    }
}

fn disable_instance(
    paths: &AppPaths,
    file: &Path,
    instance_id: &str,
    error: String,
    report: &mut ReportBuilder,
    writable: bool,
) -> InstanceConfigIssue {
    let quarantine = if writable {
        match quarantine_only(paths, file, &report.report.recovery_id) {
            Ok(path) => Some(path),
            Err(error) => {
                log::warn!(
                    "quarantine instance config {} failed: {error}",
                    file.display()
                );
                None
            }
        }
    } else {
        None
    };
    let quarantine_relative_path = quarantine
        .as_ref()
        .map(|path| relative_display(paths, path));
    if let Some(relative) = &quarantine_relative_path {
        report.report.quarantine_relative_path = Some(relative.clone());
        report.action(
            ConfigRecoveryActionKind::OriginalQuarantined,
            None,
            "instance_not_recoverable",
            "the original instance config was retained in quarantine",
            ConfigRecoveryAttention::Error,
        );
    }
    report.action(
        ConfigRecoveryActionKind::InstanceDisabled,
        None,
        "unsafe_to_guess",
        "the instance was made unavailable without changing lifecycle data or instance files",
        ConfigRecoveryAttention::Error,
    );
    report.fail(redact_error(&error));
    InstanceConfigIssue {
        instance_id: instance_id.to_string(),
        recovery_id: report.report.recovery_id.clone(),
        reason: redact_error(&error),
        quarantine_relative_path,
    }
}
