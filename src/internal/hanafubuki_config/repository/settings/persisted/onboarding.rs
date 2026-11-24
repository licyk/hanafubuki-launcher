use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};

pub(super) const FIELDS: &[&str] = &["systemWebuiScanPrompted"];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.system_webui_scan_prompted = fields.read(
        "systemWebuiScanPrompted",
        defaults.system_webui_scan_prompted,
    );
}
