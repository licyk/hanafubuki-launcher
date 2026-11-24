use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::internal::hanafubuki_config::{AppPaths, ConfigState};

use super::filesystem::{relative_display, restrict_directory, restrict_file};
use super::model::{
    ConfigFileKind, ConfigRecoveryActionKind, ConfigRecoveryAttention, ConfigRecoveryPhase,
    ConfigRecoveryReport,
};
use super::report::ReportBuilder;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConfigRecoveryJournal {
    pub(super) recovery_id: String,
    /// Which document this transaction was repairing. Settings recovery is now
    /// per domain, so a resumed transaction must name the same document its
    /// original report did rather than "global settings".
    pub(super) file_kind: ConfigFileKind,
    pub(super) phase: ConfigRecoveryPhase,
    pub(super) source: PathBuf,
    pub(super) staging: PathBuf,
    pub(super) quarantine: PathBuf,
}

pub(super) fn resume_incomplete_transactions(paths: &AppPaths, state: &ConfigState) {
    let entries = match fs::read_dir(&paths.config_recovery_state_dir) {
        Ok(entries) => entries,
        Err(error) => {
            log::warn!("read config recovery state directory failed: {error}");
            return;
        }
    };
    for entry in entries.filter_map(|entry| {
        entry
            .map_err(|error| log::warn!("config recovery state entry could not be listed: {error}"))
            .ok()
    }) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("active-") || !name.ends_with(".json") {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::error!(
                    "read config recovery journal {} failed: {error}",
                    path.display()
                );
                continue;
            }
        };
        let journal = match serde_json::from_slice::<ConfigRecoveryJournal>(&bytes) {
            Ok(journal) => journal,
            Err(error) => {
                log::error!(
                    "decode config recovery journal {} failed: {error}",
                    path.display()
                );
                continue;
            }
        };
        if !journal.source.exists() && journal.staging.is_file() {
            if let Err(error) = fs::rename(&journal.staging, &journal.source) {
                log::warn!(
                    "resume config recovery: restore staged {} failed: {error}",
                    journal.source.display()
                );
            }
        } else if journal.source.exists() && journal.staging.exists() {
            if let Err(error) = fs::remove_file(&journal.staging) {
                log::warn!(
                    "resume config recovery: remove stale staging {} failed: {error}",
                    journal.staging.display()
                );
            }
        } else if !journal.source.exists() && journal.quarantine.is_file() {
            if let Err(error) = fs::copy(&journal.quarantine, &journal.source) {
                log::warn!(
                    "resume config recovery: restore quarantined {} failed: {error}",
                    journal.source.display()
                );
            }
        }
        if let Err(error) = fs::remove_file(&path) {
            log::warn!(
                "resume config recovery: remove journal {} failed: {error}",
                path.display()
            );
        }
        let mut report = ReportBuilder::new(
            journal.file_kind.clone(),
            relative_display(paths, &journal.source),
        );
        report.action(
            ConfigRecoveryActionKind::FileRegenerated,
            None,
            "interrupted_transaction_resumed",
            "an interrupted configuration recovery transaction was reconciled at startup",
            ConfigRecoveryAttention::Warning,
        );
        state.record_recovery_report(report.finish());
    }
}

pub(super) fn write_journal(
    paths: &AppPaths,
    journal: &ConfigRecoveryJournal,
) -> Result<(), String> {
    fs::create_dir_all(&paths.config_recovery_state_dir)
        .map_err(|error| format!("create recovery state directory: {error}"))?;
    restrict_directory(&paths.config_recovery_state_dir)?;
    let path = journal_path(paths, &journal.recovery_id);
    let bytes = serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?;
    fs::write(&path, bytes).map_err(|error| format!("write recovery journal: {error}"))?;
    restrict_file(&path)
}

fn remove_journal(paths: &AppPaths, recovery_id: &str) {
    if let Err(error) = fs::remove_file(journal_path(paths, recovery_id)) {
        log::debug!("remove config recovery journal for {recovery_id} failed: {error}");
    }
}

fn journal_path(paths: &AppPaths, recovery_id: &str) -> PathBuf {
    paths
        .config_recovery_state_dir
        .join(format!("active-{recovery_id}.json"))
}

pub(super) fn persist_report(paths: &AppPaths, report: &ConfigRecoveryReport) {
    if let Err(error) = fs::create_dir_all(&paths.config_recovery_state_dir) {
        log::warn!("create config recovery state directory failed: {error}");
        return;
    }
    let path = paths
        .config_recovery_state_dir
        .join(format!("report-{}.json", report.recovery_id));
    match serde_json::to_vec_pretty(report) {
        Ok(bytes) => match fs::write(&path, bytes) {
            Ok(()) => {
                if let Err(error) = restrict_file(&path) {
                    log::warn!(
                        "restrict config recovery report {} failed: {error}",
                        path.display()
                    );
                } else {
                    remove_journal(paths, &report.recovery_id);
                }
            }
            Err(error) => log::warn!(
                "write config recovery report {} failed: {error}",
                path.display()
            ),
        },
        Err(error) => log::warn!("serialize config recovery report failed: {error}"),
    }
}
