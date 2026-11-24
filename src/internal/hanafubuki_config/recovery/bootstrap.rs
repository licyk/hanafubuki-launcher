use crate::internal::hanafubuki_config::repository::settings::AppSettings;
use crate::internal::hanafubuki_config::{AppPaths, ConfigState};

use super::instances::recover_instance_configs;
use super::journal::{persist_report, resume_incomplete_transactions};
use super::model::{ConfigPersistenceHealth, ConfigRecoveryReport};
use super::settings::recover_global_settings;

pub fn bootstrap_config(paths: &AppPaths, state: &ConfigState) {
    resume_incomplete_transactions(paths, state);
    load_settings_into_state(paths, state);

    let writable = state.persistence_health() == ConfigPersistenceHealth::Healthy;
    let (issues, reports) = recover_instance_configs(paths, writable);
    state.replace_instance_issues(issues);
    record_reports(paths, state, reports);
}

/// Read every settings document through the one settings reader, commit its
/// repairs unless persistence is read-only degraded, and publish the result.
///
/// Bootstrap and a settings read that finds no published snapshot both come
/// here, so there is exactly one way settings are read, repaired, and
/// reported.
pub(crate) fn load_settings_into_state(paths: &AppPaths, state: &ConfigState) -> AppSettings {
    let writable = state.persistence_health() == ConfigPersistenceHealth::Healthy;
    let (settings, health, reports) = recover_global_settings(paths, writable);
    state.publish_settings(settings.clone());
    if health == ConfigPersistenceHealth::ReadOnlyDegraded {
        state.set_persistence_health(health);
    }
    record_reports(paths, state, reports);
    settings
}

/// Read the settings as the settings reader repairs them, without writing,
/// moving, or reporting anything.
pub fn read_settings(paths: &AppPaths) -> AppSettings {
    recover_global_settings(paths, false).0
}

pub(crate) fn record_reports(
    paths: &AppPaths,
    state: &ConfigState,
    reports: Vec<ConfigRecoveryReport>,
) {
    for report in reports {
        persist_report(paths, &report);
        state.record_recovery_report(report);
    }
}
