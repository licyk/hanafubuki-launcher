use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};
use crate::internal::hanafubuki_config::repository::settings::validation::normalize_settings_directory;

pub(super) const FIELDS: &[&str] = &[
    "artifactCacheDir",
    "defaultWebuiInstallDir",
    "webviewDownloadDir",
];

/// A path that fails `normalize_settings_directory` is never used: it falls
/// back to its default location, which is reported.
pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.artifact_cache_dir = fields.read_checked(
        "artifactCacheDir",
        defaults.artifact_cache_dir.clone(),
        normalize_settings_directory,
    );
    settings.default_webui_install_dir = fields.read_checked(
        "defaultWebuiInstallDir",
        defaults.default_webui_install_dir.clone(),
        normalize_settings_directory,
    );
    settings.webview_download_dir = fields.read_checked(
        "webviewDownloadDir",
        defaults.webview_download_dir.clone(),
        normalize_settings_directory,
    );
}
