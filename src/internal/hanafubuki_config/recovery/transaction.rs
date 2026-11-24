use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::internal::hanafubuki_config::AppPaths;

use super::filesystem::{quarantine_target, relative_display, restrict_file, sibling_staging};
use super::journal::{ConfigRecoveryJournal, write_journal};
use super::model::{ConfigRecoveryActionKind, ConfigRecoveryAttention, ConfigRecoveryPhase};
use super::report::ReportBuilder;

pub(super) fn quarantine_and_replace<T: Serialize>(
    paths: &AppPaths,
    source: &Path,
    value: &T,
    report: &mut ReportBuilder,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    serde_json::from_slice::<Value>(&bytes).map_err(|error| error.to_string())?;
    let staging = sibling_staging(source, &report.report.recovery_id);
    fs::write(&staging, &bytes).map_err(|error| format!("write recovery staging: {error}"))?;
    restrict_file(&staging)?;
    report.report.phase = ConfigRecoveryPhase::Staged;
    let quarantine = quarantine_target(paths, source, &report.report.recovery_id)?;
    let journal = ConfigRecoveryJournal {
        recovery_id: report.report.recovery_id.clone(),
        file_kind: report.report.file_kind.clone(),
        phase: ConfigRecoveryPhase::Staged,
        source: source.to_path_buf(),
        staging: staging.clone(),
        quarantine: quarantine.clone(),
    };
    write_journal(paths, &journal)?;
    if source.exists() {
        fs::rename(source, &quarantine)
            .map_err(|error| format!("move invalid config to quarantine: {error}"))?;
        report.report.phase = ConfigRecoveryPhase::Quarantined;
        report.report.quarantine_relative_path = Some(relative_display(paths, &quarantine));
        report.action(
            ConfigRecoveryActionKind::OriginalQuarantined,
            None,
            "original_retained",
            "the original document was moved to the protected quarantine directory",
            ConfigRecoveryAttention::Warning,
        );
    }
    if let Err(error) = fs::rename(&staging, source) {
        if !source.exists() && quarantine.exists() {
            if let Err(rollback_error) = fs::rename(&quarantine, source) {
                log::warn!(
                    "restore quarantined config {} after commit failure also failed: {rollback_error}",
                    source.display()
                );
            }
        }
        return Err(format!("commit repaired config: {error}"));
    }
    report.report.phase = ConfigRecoveryPhase::ReplacementCommitted;
    report.action(
        ConfigRecoveryActionKind::FileRegenerated,
        None,
        "replacement_committed",
        "a fully validated replacement was committed atomically",
        ConfigRecoveryAttention::Warning,
    );
    Ok(())
}

pub(super) fn quarantine_only(
    paths: &AppPaths,
    source: &Path,
    recovery_id: &str,
) -> Result<PathBuf, String> {
    let target = quarantine_target(paths, source, recovery_id)?;
    fs::rename(source, &target).map_err(|error| format!("move config to quarantine: {error}"))?;
    Ok(target)
}
