use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::model::{
    ConfigFileKind, ConfigRecoveryAction, ConfigRecoveryActionKind, ConfigRecoveryAttention,
    ConfigRecoveryPhase, ConfigRecoveryReport, ConfigRecoveryStatus,
};

const MAX_REPORT_LOG_BYTES: usize = 256 * 1024;

pub(super) struct ReportBuilder {
    pub(super) report: ConfigRecoveryReport,
    relative_path: String,
}

impl ReportBuilder {
    pub(super) fn new(kind: ConfigFileKind, relative_path: String) -> Self {
        let now = now_ms();
        Self {
            report: ConfigRecoveryReport {
                recovery_id: next_recovery_id(),
                file_kind: kind,
                status: ConfigRecoveryStatus::Completed,
                attention: ConfigRecoveryAttention::Info,
                phase: ConfigRecoveryPhase::Detected,
                created_at: now,
                updated_at: now,
                actions: Vec::new(),
                quarantine_relative_path: None,
                error: None,
                log_lines: Vec::new(),
            },
            relative_path,
        }
    }

    pub(super) fn action(
        &mut self,
        kind: ConfigRecoveryActionKind,
        pointer: Option<&str>,
        reason_code: &str,
        message: impl Into<String>,
        attention: ConfigRecoveryAttention,
    ) {
        let message = message.into();
        self.report.attention = max_attention(self.report.attention, attention);
        self.report.actions.push(ConfigRecoveryAction {
            kind,
            relative_path: self.relative_path.clone(),
            json_pointer: pointer.map(str::to_string),
            reason_code: reason_code.to_string(),
            message: message.clone(),
        });
        let pointer = pointer.unwrap_or("/");
        self.report.log_lines.push(format!(
            "[{}] {} {}: {}",
            action_label(kind),
            self.relative_path,
            pointer,
            message
        ));
        trim_report_log(&mut self.report.log_lines);
        self.report.updated_at = now_ms();
    }

    pub(super) fn fail(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.report.status = ConfigRecoveryStatus::Failed;
        self.report.attention = ConfigRecoveryAttention::Error;
        self.report.phase = ConfigRecoveryPhase::Failed;
        self.report.error = Some(error.clone());
        self.report.log_lines.push(format!("[failed] {error}"));
        trim_report_log(&mut self.report.log_lines);
        self.report.updated_at = now_ms();
    }

    pub(super) fn finish(mut self) -> ConfigRecoveryReport {
        if self.report.status == ConfigRecoveryStatus::Completed {
            self.report.phase = ConfigRecoveryPhase::Completed;
        }
        self.report.updated_at = now_ms();
        self.report
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn next_recovery_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("{:x}-{:x}", now_ms(), NEXT.fetch_add(1, Ordering::Relaxed))
}

fn max_attention(
    left: ConfigRecoveryAttention,
    right: ConfigRecoveryAttention,
) -> ConfigRecoveryAttention {
    match (left, right) {
        (ConfigRecoveryAttention::Error, _) | (_, ConfigRecoveryAttention::Error) => {
            ConfigRecoveryAttention::Error
        }
        (ConfigRecoveryAttention::Warning, _) | (_, ConfigRecoveryAttention::Warning) => {
            ConfigRecoveryAttention::Warning
        }
        _ => ConfigRecoveryAttention::Info,
    }
}

fn action_label(kind: ConfigRecoveryActionKind) -> &'static str {
    match kind {
        ConfigRecoveryActionKind::DefaultApplied => "defaulted",
        ConfigRecoveryActionKind::FieldRemoved => "removed",
        ConfigRecoveryActionKind::GroupReset => "reset",
        ConfigRecoveryActionKind::BackupRestored => "backup",
        ConfigRecoveryActionKind::OriginalQuarantined => "quarantined",
        ConfigRecoveryActionKind::FileRegenerated => "persisted",
        ConfigRecoveryActionKind::InMemoryFallback => "degraded",
        ConfigRecoveryActionKind::PersistenceFailed => "failed",
        ConfigRecoveryActionKind::InstanceDisabled => "disabled",
    }
}

fn trim_report_log(lines: &mut Vec<String>) {
    while lines.iter().map(|line| line.len() + 1).sum::<usize>() > MAX_REPORT_LOG_BYTES {
        if lines.is_empty() {
            break;
        }
        lines.remove(0);
    }
}

pub(super) fn redact_error(error: &str) -> String {
    let first_line = error
        .lines()
        .next()
        .unwrap_or("configuration recovery failed");
    let mut chars = first_line.chars();
    let shortened: String = chars.by_ref().take(512).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}
