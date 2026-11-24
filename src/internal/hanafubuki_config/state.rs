use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use crate::internal::hanafubuki_config::recovery::{
    ConfigPersistenceHealth, ConfigRecoveryReport, InstanceConfigIssue,
};
use crate::internal::hanafubuki_config::repository::settings::AppSettings;

/// Backend-owned configuration state with one lock per persisted category.
#[derive(Clone, Default)]
pub struct ConfigState {
    pub settings_lock: Arc<Mutex<()>>,
    pub instances_lock: Arc<Mutex<()>>,
    settings: Arc<RwLock<Option<AppSettings>>>,
    persistence_health: Arc<RwLock<ConfigPersistenceHealth>>,
    recovery_reports: Arc<RwLock<VecDeque<ConfigRecoveryReport>>>,
    instance_issues: Arc<RwLock<BTreeMap<String, InstanceConfigIssue>>>,
}

impl ConfigState {
    pub fn settings_snapshot(&self) -> Option<AppSettings> {
        self.settings.read().clone()
    }

    pub(crate) fn publish_settings(&self, settings: AppSettings) {
        *self.settings.write() = Some(settings);
    }

    pub fn persistence_health(&self) -> ConfigPersistenceHealth {
        *self.persistence_health.read()
    }

    pub fn ensure_persistence_writable(&self) -> Result<(), String> {
        if self.persistence_health() == ConfigPersistenceHealth::ReadOnlyDegraded {
            return Err(
                "configuration persistence is in read-only degraded mode; inspect the configuration recovery activity"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub(crate) fn set_persistence_health(&self, health: ConfigPersistenceHealth) {
        *self.persistence_health.write() = health;
    }

    pub fn recovery_reports(&self) -> Vec<ConfigRecoveryReport> {
        self.recovery_reports.read().iter().cloned().collect()
    }

    pub(crate) fn record_recovery_report(&self, report: ConfigRecoveryReport) {
        const MAX_REPORTS: usize = 20;
        let mut reports = self.recovery_reports.write();
        reports.push_back(report);
        while reports.len() > MAX_REPORTS {
            reports.pop_front();
        }
    }

    pub fn recovery_report(&self, recovery_id: &str) -> Option<ConfigRecoveryReport> {
        self.recovery_reports
            .read()
            .iter()
            .find(|report| report.recovery_id == recovery_id)
            .cloned()
    }

    pub(crate) fn replace_instance_issues(&self, issues: BTreeMap<String, InstanceConfigIssue>) {
        *self.instance_issues.write() = issues;
    }

    pub(crate) fn record_instance_issue(&self, issue: InstanceConfigIssue) {
        self.instance_issues
            .write()
            .insert(issue.instance_id.clone(), issue);
    }

    /// A newly created config replaces whatever made an earlier one unusable.
    pub(crate) fn clear_instance_issue(&self, instance_id: &str) {
        self.instance_issues.write().remove(instance_id);
    }

    pub fn instance_issue(&self, instance_id: &str) -> Option<InstanceConfigIssue> {
        self.instance_issues.read().get(instance_id).cloned()
    }
}
