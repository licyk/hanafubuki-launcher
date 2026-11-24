use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};

pub(super) const FIELDS: &[&str] = &[
    "autoCheckAppUpdates",
    "autoCheckInstanceUpdates",
    "autoCreateInstanceSnapshots",
    "autoCheckExtensionUpdates",
];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.auto_check_app_updates =
        fields.read("autoCheckAppUpdates", defaults.auto_check_app_updates);
    settings.auto_check_instance_updates = fields.read(
        "autoCheckInstanceUpdates",
        defaults.auto_check_instance_updates,
    );
    settings.auto_create_instance_snapshots = fields.read(
        "autoCreateInstanceSnapshots",
        defaults.auto_create_instance_snapshots,
    );
    settings.auto_check_extension_updates = fields.read(
        "autoCheckExtensionUpdates",
        defaults.auto_check_extension_updates,
    );
}
