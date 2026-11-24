use serde_json::{Map, Value};

use crate::internal::hanafubuki_config::AppPaths;
use crate::internal::hanafubuki_config::model::settings_domain::SettingsDomain;
use crate::internal::hanafubuki_config::repository::settings::{
    AppSettings, PersistedSettingsDecoder, PersistedSettingsIssue, PersistedSettingsIssueKind,
    domain_document, settings_from_documents,
};

use super::filesystem::relative_display;
use super::json::{parse_unique_value, read_backup_value, read_bounded_text};
use super::model::{
    ConfigFileKind, ConfigPersistenceHealth, ConfigRecoveryActionKind, ConfigRecoveryAttention,
    ConfigRecoveryReport,
};
use super::report::{ReportBuilder, redact_error};
use super::transaction::quarantine_and_replace;

/// Recover every persisted global-settings document.
///
/// Each domain is read, decoded and repaired on its own, so one unreadable
/// document costs only its own fields: the remaining documents still supply
/// what the user configured. Health degrades to read-only if *any* document
/// could not be read or its repair could not be committed, because a settings
/// write that cannot be persisted must not look like it succeeded.
///
/// Without `commit` nothing is written or moved: the repaired settings are
/// only returned, and the reports describe repairs that are still pending.
pub(super) fn recover_global_settings(
    paths: &AppPaths,
    commit: bool,
) -> (
    AppSettings,
    ConfigPersistenceHealth,
    Vec<ConfigRecoveryReport>,
) {
    let defaults = AppSettings::defaults(paths);
    let mut documents = Vec::new();
    let mut pending = Vec::new();
    let mut health = ConfigPersistenceHealth::Healthy;

    // Pass one repairs each document in memory. Nothing is written yet, and
    // configured directories are not materialized while persisted absolute
    // paths may still need reconciliation by the caller.
    for &domain in SettingsDomain::ALL {
        let outcome = recover_domain(paths, domain, &defaults);
        documents.push(outcome.document);
        pending.push(outcome.recovery);
    }

    let settings = match settings_from_documents(&defaults, documents) {
        Ok(settings) => settings,
        // Every document was validated on its own, so their union cannot fail
        // the struct contract. Falling back to defaults keeps recovery total.
        Err(error) => {
            log::error!(
                "settings union decode failed during recovery; falling back to defaults: {error}"
            );
            defaults.clone()
        }
    };

    // Pass two commits each repaired document.
    for recovery in &mut pending {
        if !commit || !recovery.needs_write || recovery.unreadable {
            continue;
        }
        let file = paths.settings_file(recovery.domain);
        let document = match domain_document(recovery.domain, &settings) {
            Ok(document) => Value::Object(document),
            Err(error) => {
                recovery.report.action(
                    ConfigRecoveryActionKind::InMemoryFallback,
                    None,
                    "projection_failed",
                    "repaired settings are active in memory but could not be serialized",
                    ConfigRecoveryAttention::Error,
                );
                recovery.report.fail(redact_error(&error));
                health = ConfigPersistenceHealth::ReadOnlyDegraded;
                continue;
            }
        };
        let persist = if recovery.quarantine_original
            || recovery.report.report.attention != ConfigRecoveryAttention::Info
        {
            quarantine_and_replace(paths, &file, &document, &mut recovery.report)
        } else {
            crate::internal::hanafubuki_config::repository::safe_file::write_json_atomic(
                &file, &document,
            )
        };
        if let Err(error) = persist {
            recovery.report.action(
                ConfigRecoveryActionKind::PersistenceFailed,
                None,
                "replacement_failed",
                "repaired settings are active in memory but the replacement was not committed",
                ConfigRecoveryAttention::Error,
            );
            recovery.report.fail(redact_error(&error));
            health = ConfigPersistenceHealth::ReadOnlyDegraded;
        }
    }

    for recovery in &pending {
        if recovery.unreadable {
            health = ConfigPersistenceHealth::ReadOnlyDegraded;
        }
    }

    let reports = pending
        .into_iter()
        .filter(|recovery| !recovery.report.report.actions.is_empty())
        .map(|recovery| recovery.report.finish())
        .collect();
    (settings, health, reports)
}

/// One domain's repair outcome, before anything is written.
struct DomainRecovery {
    domain: SettingsDomain,
    report: ReportBuilder,
    /// The on-disk document does not match what the domain should hold.
    needs_write: bool,
    /// The original document was invalid, so it is retained in quarantine
    /// rather than simply overwritten.
    quarantine_original: bool,
    /// The document could not be read at all. Its fields fall back to defaults
    /// in memory, but nothing is written over a file we could not inspect.
    unreadable: bool,
}

fn recover_domain(
    paths: &AppPaths,
    domain: SettingsDomain,
    defaults: &AppSettings,
) -> DomainRecoveryOutcome {
    let default_document = domain_document(domain, defaults).unwrap_or_default();
    let file = paths.settings_file(domain);
    let mut report = ReportBuilder::new(
        ConfigFileKind::GlobalSettings { domain },
        relative_display(paths, &file),
    );

    let (raw, source_was_invalid) = match read_bounded_text(&file) {
        Ok(Some(content)) => match parse_unique_value(&content) {
            Ok(value) => (Some(value), false),
            Err(error) => {
                report.action(
                    ConfigRecoveryActionKind::FieldRemoved,
                    None,
                    "invalid_json",
                    format!(
                        "JSON syntax is invalid at line {} column {}",
                        error.line(),
                        error.column()
                    ),
                    ConfigRecoveryAttention::Warning,
                );
                match read_backup_value(&file) {
                    Ok(Some(value)) => {
                        report.action(
                            ConfigRecoveryActionKind::BackupRestored,
                            None,
                            "valid_backup",
                            "a verified backup was selected as the recovery source",
                            ConfigRecoveryAttention::Warning,
                        );
                        (Some(value), true)
                    }
                    _ => (None, true),
                }
            }
        },
        // A missing document is ordinary on first run and after adding a
        // domain: defaults are written without an attention-worthy report.
        Ok(None) => {
            return DomainRecoveryOutcome {
                document: default_document,
                recovery: DomainRecovery {
                    domain,
                    report,
                    needs_write: true,
                    quarantine_original: false,
                    unreadable: false,
                },
            };
        }
        Err(error) => {
            report.action(
                ConfigRecoveryActionKind::InMemoryFallback,
                None,
                "settings_read_failed",
                "this settings document could not be read; backend defaults are active for it in read-only degraded mode",
                ConfigRecoveryAttention::Error,
            );
            report.fail(redact_error(&error));
            return DomainRecoveryOutcome {
                document: default_document,
                recovery: DomainRecovery {
                    domain,
                    report,
                    needs_write: false,
                    quarantine_original: false,
                    unreadable: true,
                },
            };
        }
    };

    let (document, changed) = match raw {
        Some(value) => repair_domain_value(paths, domain, value, &mut report, &default_document),
        None => {
            report.action(
                ConfigRecoveryActionKind::FileRegenerated,
                None,
                "no_valid_candidate",
                "no valid main or backup document remained; backend defaults were generated",
                ConfigRecoveryAttention::Warning,
            );
            (default_document.clone(), true)
        }
    };

    DomainRecoveryOutcome {
        document,
        recovery: DomainRecovery {
            domain,
            report,
            needs_write: changed || source_was_invalid,
            quarantine_original: source_was_invalid,
            unreadable: false,
        },
    }
}

struct DomainRecoveryOutcome {
    /// Exactly the repaired document for this domain.
    document: Map<String, Value>,
    recovery: DomainRecovery,
}

fn repair_domain_value(
    paths: &AppPaths,
    domain: SettingsDomain,
    value: Value,
    report: &mut ReportBuilder,
    default_document: &Map<String, Value>,
) -> (Map<String, Value>, bool) {
    match PersistedSettingsDecoder::new(paths).decode(domain, value) {
        Ok(decoded) => {
            for issue in decoded.issues {
                record_persisted_settings_issue(report, issue);
            }
            (decoded.document, decoded.changed)
        }
        Err(_) => {
            report.action(
                ConfigRecoveryActionKind::FileRegenerated,
                None,
                "root_or_document_invalid",
                "settings document could not be decoded; backend defaults were generated",
                ConfigRecoveryAttention::Warning,
            );
            (default_document.clone(), true)
        }
    }
}

fn record_persisted_settings_issue(report: &mut ReportBuilder, issue: PersistedSettingsIssue) {
    let pointer = (!issue.json_pointer.is_empty()).then_some(issue.json_pointer.as_str());
    match issue.kind {
        PersistedSettingsIssueKind::MissingField => report.action(
            ConfigRecoveryActionKind::DefaultApplied,
            pointer,
            "missing_field",
            issue.message,
            ConfigRecoveryAttention::Info,
        ),
        PersistedSettingsIssueKind::UnsupportedField => report.action(
            ConfigRecoveryActionKind::FieldRemoved,
            pointer,
            "unsupported_field",
            issue.message,
            ConfigRecoveryAttention::Warning,
        ),
        PersistedSettingsIssueKind::InvalidField => report.action(
            ConfigRecoveryActionKind::DefaultApplied,
            pointer,
            "invalid_field",
            issue.message,
            ConfigRecoveryAttention::Warning,
        ),
        PersistedSettingsIssueKind::InvalidPolicyField => report.action(
            ConfigRecoveryActionKind::DefaultApplied,
            pointer,
            "invalid_policy_field",
            issue.message,
            ConfigRecoveryAttention::Error,
        ),
        PersistedSettingsIssueKind::InvalidSection => report.action(
            ConfigRecoveryActionKind::GroupReset,
            pointer,
            "invalid_section",
            issue.message,
            ConfigRecoveryAttention::Warning,
        ),
    }
}
