use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};

pub(super) const FIELDS: &[&str] = &[
    "webviewSharedCache",
    "webviewPreservePageState",
    "webviewNewWindow",
    "webviewGpuAcceleration",
];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.webview_shared_cache =
        fields.read("webviewSharedCache", defaults.webview_shared_cache);
    settings.webview_preserve_page_state = fields.read(
        "webviewPreservePageState",
        defaults.webview_preserve_page_state,
    );
    settings.webview_new_window = fields.read("webviewNewWindow", defaults.webview_new_window);
    settings.webview_gpu_acceleration =
        fields.read("webviewGpuAcceleration", defaults.webview_gpu_acceleration);
}
