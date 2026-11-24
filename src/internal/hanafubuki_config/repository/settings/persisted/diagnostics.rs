use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};

pub(super) const FIELDS: &[&str] = &["appDiagnosticsIncludeAllInstances"];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.app_diagnostics_include_all_instances = fields.read(
        "appDiagnosticsIncludeAllInstances",
        defaults.app_diagnostics_include_all_instances,
    );
}
