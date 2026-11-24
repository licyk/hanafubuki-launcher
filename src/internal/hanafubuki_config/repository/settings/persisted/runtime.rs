use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};

pub(super) const FIELDS: &[&str] = &["appPythonVersion", "gitRuntimeSource"];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    // Runtime choices are closed enums, so deserialization is each field's
    // complete contract.
    let mut fields = FieldReader::new(object, issues);
    settings.app_python_version = fields.read("appPythonVersion", defaults.app_python_version);
    settings.git_runtime_source = fields.read("gitRuntimeSource", defaults.git_runtime_source);
}
