use super::modules::DISABLED_MODULES;
use super::{AppSettings, ProxyMode, apply_setting_patch};
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use crate::internal::hanafubuki_foundation::paths::{normalize_path, validate_not_dangerous};
use crate::internal::hanafubuki_foundation::url_policy::UrlPolicy;
use serde_json::{Map, Value};
use std::ops::RangeInclusive;
use std::path::PathBuf;

/// The interface languages `language` may name; `auto` follows the system.
pub const SUPPORTED_LANGUAGES: &[&str] = &["auto", "zh", "en"];
/// Days a log file is kept.
pub const LOG_RETENTION_DAYS: RangeInclusive<u32> = 1..=365;
/// Size, in MiB, at which the active log file is rotated.
pub const LOG_FILE_MAX_SIZE_MB: RangeInclusive<u32> = 1..=512;

pub(crate) fn validate_language(value: &str) -> Result<(), String> {
    if SUPPORTED_LANGUAGES.contains(&value) {
        Ok(())
    } else {
        Err(format!("language has unsupported value: {value}"))
    }
}

/// A mirror's address: HTTP(S) with a host, kept as written less the
/// surrounding whitespace. The error completes a sentence that starts with
/// the field's name.
pub(crate) fn normalize_http_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("cannot be empty".to_string());
    }
    UrlPolicy::HTTP
        .parse(raw)
        .map_err(|rejection| rejection.to_string())?;
    Ok(raw.to_string())
}

/// A directory setting (`artifactCacheDir`, `defaultWebuiInstallDir`,
/// `webviewDownloadDir`), as a write and a read both take it: normalized, not
/// a filesystem root, and without NUL. This is the path's one check: the
/// cache path reaches child processes as an environment value unchecked.
pub(crate) fn normalize_settings_directory(path: PathBuf) -> Result<PathBuf, String> {
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err("path must not contain NUL".to_string());
    }
    let path = normalize_path(path)?;
    validate_not_dangerous(&path)?;
    Ok(path)
}

/// Validate a partial settings object (used for instance/runtime overrides)
/// without persisting anything. Runs the same per-field validation as a global
/// patch, so unknown keys or out-of-range/typed values are rejected.
pub fn validate_settings_overrides(
    paths: &AppPaths,
    overrides: &Map<String, Value>,
) -> Result<(), String> {
    let mut scratch = AppSettings::defaults(paths);
    for (key, value) in overrides {
        if is_instance_override_forbidden(key) {
            return Err(format!(
                "{key} is global-only and cannot be overridden per instance"
            ));
        }
        apply_setting_patch(&mut scratch, key, value.clone())?;
    }
    validate_settings(&scratch)?;
    Ok(())
}

/// Current settings fields that cannot participate in instance overrides.
///
/// Persisted instance recovery uses the same classification while deriving
/// field membership from the serialized current [`AppSettings`] contract. The
/// historical `proxy` name is included so ordinary validation continues to
/// reject it even though it is not a current settings field.
pub(crate) fn is_instance_override_forbidden(key: &str) -> bool {
    matches!(
        key,
        "proxyMode"
            | "proxyAddress"
            | "proxy"
            | "appPythonVersion"
            | "gitRuntimeSource"
            | "disabledModules"
            | "modules"
            | "autoCheckAppUpdates"
            | "autoCheckInstanceUpdates"
            | "autoCreateInstanceSnapshots"
            | "autoCheckExtensionUpdates"
    )
}

pub(crate) fn validate_settings(settings: &AppSettings) -> Result<(), String> {
    if settings.proxy_mode == ProxyMode::Manual && settings.proxy_address.is_none() {
        return Err("proxyAddress is required when proxyMode is manual".to_string());
    }
    DISABLED_MODULES.validate(&settings.disabled_modules)?;
    super::modules::validate_module_settings(&settings.modules)?;
    Ok(())
}

pub fn validate_proxy_address(raw: &str) -> Result<String, String> {
    if raw.contains('\0') {
        return Err("proxyAddress must not contain NUL".to_string());
    }
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("proxyAddress cannot be empty".to_string());
    }
    let parsed = url::Url::parse(raw)
        .map_err(|error| format!("proxyAddress must be a valid proxy URL: {error}"))?;
    if !matches!(
        parsed.scheme(),
        "http" | "https" | "socks" | "socks4" | "socks4a" | "socks5" | "socks5h"
    ) {
        return Err("proxyAddress uses an unsupported proxy scheme".to_string());
    }
    if parsed.host_str().is_none() || !has_explicit_proxy_port(raw) {
        return Err("proxyAddress must include a host and explicit port".to_string());
    }
    Ok(raw.to_string())
}

fn has_explicit_proxy_port(raw: &str) -> bool {
    let Some((_, remainder)) = raw.split_once("://") else {
        return false;
    };
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, value)| value);
    let port = if host_port.starts_with('[') {
        host_port.rsplit_once("]:").map(|(_, port)| port)
    } else {
        host_port.rsplit_once(':').map(|(_, port)| port)
    };
    port.and_then(|value| value.parse::<u16>().ok())
        .is_some_and(|value| value > 0)
}
