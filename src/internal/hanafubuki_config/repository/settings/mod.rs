use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::Value;

use crate::internal::hanafubuki_foundation::paths::validate_not_dangerous;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

use super::safe_file;
use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use crate::internal::hanafubuki_config::model::presentation::{
    BackgroundFit, BackgroundPosition, DEFAULT_BLUR_RADIUS, DEFAULT_SURFACE_OPACITY,
    DEFAULT_THEME_COLOR, MAX_BLUR_RADIUS, MAX_SURFACE_OPACITY, MIN_BLUR_RADIUS,
    MIN_SURFACE_OPACITY, ThemePreference, normalize_theme_color,
};

mod contracts;
mod modules;
mod names;
mod patch;
pub mod persisted;
mod schema;
mod startup;
pub(crate) mod store;
pub(crate) mod validation;

pub use crate::internal::hanafubuki_config::recovery::read_settings;
pub use contracts::{
    AppSettings, DEFAULT_WEBVIEW_GPU_ACCELERATION, GitRuntimeSource, LogLevelSetting,
    ModelDownloadSource, ProxyMode, WebviewNewWindow,
};
pub use modules::{MAX_DISABLED_MODULES, MAX_MODULE_NAME_BYTES, ModuleSettings};
pub use patch::apply_setting_patch;
pub use persisted::document_for as settings_domain_document;
pub use schema::{SettingsSchema, get_global_settings_schema};
pub use startup::read_startup_webview_gpu_acceleration;
pub use store::{
    get_global_settings_inner, prepare_settings_directories, reset_appearance_settings_inner,
    update_global_setting_inner, update_global_settings_patch_inner,
};
pub use validation::{
    LOG_FILE_MAX_SIZE_MB, LOG_RETENTION_DAYS, SUPPORTED_LANGUAGES, validate_proxy_address,
    validate_settings_overrides,
};

use modules::{DISABLED_MODULES, parse_module_settings};
pub(crate) use persisted::{
    PersistedSettingsDecoder, PersistedSettingsIssue, PersistedSettingsIssueKind,
    document_for as domain_document, settings_from_documents,
};
use validation::{
    normalize_http_url, normalize_settings_directory, validate_language, validate_settings,
};

#[cfg(test)]
mod tests;
