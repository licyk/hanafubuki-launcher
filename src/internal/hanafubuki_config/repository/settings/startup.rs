use serde::Deserialize;

use super::DEFAULT_WEBVIEW_GPU_ACCELERATION;
use super::safe_file;
use crate::internal::hanafubuki_config::model::paths::BootstrapPaths;
use crate::internal::hanafubuki_config::model::settings_domain::SettingsDomain;

/// The part of the WebView document the process needs before Tauri starts.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartupWebviewDocument {
    webview_gpu_acceleration: Option<bool>,
}

/// Read [`super::AppSettings::webview_gpu_acceleration`] before the settings
/// load.
///
/// The desktop shell needs it before any thread starts, long before the
/// configuration authority exists, so this only reads and never writes: a
/// missing document or field is the default, an unusable main document falls
/// back to its backup without restoring it, and repairing either stays with
/// the settings load. A field of the wrong type is an error the caller reports
/// once logging is up, and uses the default for.
pub fn read_startup_webview_gpu_acceleration(paths: &BootstrapPaths) -> Result<bool, String> {
    let file = paths.settings_file(SettingsDomain::Webview);
    let Some(document) = safe_file::peek_json::<serde_json::Value>(&file)? else {
        return Ok(DEFAULT_WEBVIEW_GPU_ACCELERATION);
    };
    let document: StartupWebviewDocument =
        serde_json::from_value(document).map_err(|error| format!("{}: {error}", file.display()))?;
    Ok(document
        .webview_gpu_acceleration
        .unwrap_or(DEFAULT_WEBVIEW_GPU_ACCELERATION))
}
