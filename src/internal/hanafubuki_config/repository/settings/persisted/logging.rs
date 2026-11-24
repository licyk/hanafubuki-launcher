use serde_json::{Map, Value};

use super::field::{FieldReader, within};
use super::{AppSettings, PersistedSettingsIssue};
use crate::internal::hanafubuki_config::repository::settings::validation::{
    LOG_FILE_MAX_SIZE_MB, LOG_RETENTION_DAYS,
};

pub(super) const FIELDS: &[&str] = &[
    "logRetentionDays",
    "logLevel",
    "logFilePersistenceEnabled",
    "logFileMaxSizeMb",
];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.log_retention_days = fields.read_checked(
        "logRetentionDays",
        defaults.log_retention_days,
        within(LOG_RETENTION_DAYS),
    );
    settings.log_level = fields.read("logLevel", defaults.log_level);
    settings.log_file_persistence_enabled = fields.read(
        "logFilePersistenceEnabled",
        defaults.log_file_persistence_enabled,
    );
    settings.log_file_max_size_mb = fields.read_checked(
        "logFileMaxSizeMb",
        defaults.log_file_max_size_mb,
        within(LOG_FILE_MAX_SIZE_MB),
    );
}
