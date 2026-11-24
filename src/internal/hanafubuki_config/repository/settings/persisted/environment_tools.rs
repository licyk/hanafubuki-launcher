use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};

pub(super) const FIELDS: &[&str] = &["enableUv", "enableEnvironmentCheck", "useCudaMalloc"];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.enable_uv = fields.read("enableUv", defaults.enable_uv);
    settings.enable_environment_check =
        fields.read("enableEnvironmentCheck", defaults.enable_environment_check);
    settings.use_cuda_malloc = fields.read("useCudaMalloc", defaults.use_cuda_malloc);
}
