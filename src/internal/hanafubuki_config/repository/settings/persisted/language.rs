use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};
use crate::internal::hanafubuki_config::repository::settings::validation::validate_language;

pub(super) const FIELDS: &[&str] = &["language"];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.language =
        fields.read_checked("language", defaults.language.clone(), |value: String| {
            validate_language(&value).map(|()| value)
        });
}
