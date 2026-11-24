use super::*;

pub fn apply_setting_patch(
    settings: &mut AppSettings,
    key: &str,
    value: Value,
) -> Result<(), String> {
    match key {
        "language" => {
            let language = parse_required_string(&value, key)?;
            validate_language(&language)?;
            settings.language = language
        }
        "theme" => {
            settings.theme =
                ThemePreference::try_from(parse_required_string(&value, key)?.as_str())?
        }
        "themeColor" => {
            settings.theme_color = normalize_theme_color(&parse_required_string(&value, key)?)?
        }
        "backgroundImageEnabled" => settings.background_image_enabled = parse_bool(&value, key)?,
        "backgroundRandomEnabled" => settings.background_random_enabled = parse_bool(&value, key)?,
        "backgroundSurfaceOpacity" => {
            settings.background_surface_opacity = parse_surface_opacity(&value)?
        }
        "backgroundBlurRadius" => settings.background_blur_radius = parse_blur_radius(&value)?,
        "backgroundImageFit" => {
            settings.background_image_fit =
                BackgroundFit::try_from(parse_required_string(&value, key)?.as_str())?
        }
        "backgroundImagePosition" => {
            settings.background_image_position =
                BackgroundPosition::try_from(parse_required_string(&value, key)?.as_str())?
        }
        "artifactCacheDir" => settings.artifact_cache_dir = parse_path(&value, key)?,
        "defaultWebuiInstallDir" => settings.default_webui_install_dir = parse_path(&value, key)?,
        "webviewDownloadDir" => settings.webview_download_dir = parse_path(&value, key)?,
        "proxyMode" => {
            settings.proxy_mode = ProxyMode::try_from(parse_required_string(&value, key)?.as_str())?
        }
        "proxyAddress" => settings.proxy_address = parse_optional_proxy_address(&value, key)?,
        "autoMirror" => settings.auto_mirror = parse_bool(&value, key)?,
        "useMicromambaMirror" => settings.use_micromamba_mirror = parse_bool(&value, key)?,
        "usePypiMirror" => settings.use_pypi_mirror = parse_bool(&value, key)?,
        "useGithubMirror" => settings.use_github_mirror = parse_bool(&value, key)?,
        "customGithubMirror" => {
            settings.custom_github_mirror = parse_optional_http_url(&value, key)?
        }
        "useHuggingfaceMirror" => settings.use_huggingface_mirror = parse_bool(&value, key)?,
        "customHuggingfaceMirror" => {
            settings.custom_huggingface_mirror = parse_optional_http_url(&value, key)?
        }
        "modelDownloadSource" => {
            settings.model_download_source =
                ModelDownloadSource::try_from(parse_required_string(&value, key)?.as_str())?
        }
        "appPythonVersion" => {
            let value = parse_required_string(&value, key)?;
            settings.app_python_version = PythonVersion::try_from(value.as_str())
                .map_err(|_| format!("appPythonVersion has unsupported value: {value}"))?
        }
        "gitRuntimeSource" => {
            settings.git_runtime_source =
                GitRuntimeSource::try_from(parse_required_string(&value, key)?.as_str())?
        }
        "enableUv" => settings.enable_uv = parse_bool(&value, key)?,
        "enableEnvironmentCheck" => settings.enable_environment_check = parse_bool(&value, key)?,
        "useCudaMalloc" => settings.use_cuda_malloc = parse_bool(&value, key)?,
        "logRetentionDays" => {
            settings.log_retention_days = parse_u32_range(&value, key, LOG_RETENTION_DAYS)?
        }
        "logFilePersistenceEnabled" => {
            settings.log_file_persistence_enabled = parse_bool(&value, key)?
        }
        "logFileMaxSizeMb" => {
            settings.log_file_max_size_mb = parse_u32_range(&value, key, LOG_FILE_MAX_SIZE_MB)?
        }
        "logLevel" => {
            settings.log_level =
                LogLevelSetting::try_from(parse_required_string(&value, key)?.as_str())?
        }
        "appDiagnosticsIncludeAllInstances" => {
            settings.app_diagnostics_include_all_instances = parse_bool(&value, key)?
        }
        "autoCheckAppUpdates" => settings.auto_check_app_updates = parse_bool(&value, key)?,
        "autoCheckInstanceUpdates" => {
            settings.auto_check_instance_updates = parse_bool(&value, key)?
        }
        "autoCreateInstanceSnapshots" => {
            settings.auto_create_instance_snapshots = parse_bool(&value, key)?
        }
        "autoCheckExtensionUpdates" => {
            settings.auto_check_extension_updates = parse_bool(&value, key)?
        }
        "webviewSharedCache" => settings.webview_shared_cache = parse_bool(&value, key)?,
        "webviewPreservePageState" => {
            settings.webview_preserve_page_state = parse_bool(&value, key)?
        }
        "webviewNewWindow" => {
            settings.webview_new_window =
                WebviewNewWindow::try_from(parse_required_string(&value, key)?.as_str())?
        }
        "webviewGpuAcceleration" => settings.webview_gpu_acceleration = parse_bool(&value, key)?,
        "disabledModules" => settings.disabled_modules = DISABLED_MODULES.parse(&value)?,
        "modules" => settings.modules = parse_module_settings(&value)?,
        "systemWebuiScanPrompted" => settings.system_webui_scan_prompted = parse_bool(&value, key)?,
        _ => return Err(format!("unsupported settings key: {key}")),
    }
    Ok(())
}

fn parse_optional_proxy_address(value: &Value, key: &str) -> Result<Option<String>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let raw = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a proxy URL or null"))?;
    validate_proxy_address(raw).map(Some)
}

fn parse_required_string(value: &Value, key: &str) -> Result<String, String> {
    let value = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))?
        .trim();
    if value.is_empty() {
        return Err(format!("{key} cannot be empty"));
    }
    Ok(value.to_string())
}

fn parse_optional_http_url(value: &Value, key: &str) -> Result<Option<String>, String> {
    parse_optional_url(value, key, normalize_http_url)
}

/// `null` clears the URL; any other value must be text `normalize` accepts.
fn parse_optional_url(
    value: &Value,
    key: &str,
    normalize: fn(&str) -> Result<String, String>,
) -> Result<Option<String>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let raw = value
        .as_str()
        .ok_or_else(|| format!("{key} must be an HTTP/HTTPS URL or null"))?;
    if raw.trim().is_empty() {
        return Err(format!(
            "{key} cannot be empty or whitespace-only; use null to clear it"
        ));
    }
    normalize(raw)
        .map(Some)
        .map_err(|error| format!("{key} {error}"))
}

fn parse_bool(value: &Value, key: &str) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("{key} must be a boolean"))
}

fn parse_path(value: &Value, key: &str) -> Result<PathBuf, String> {
    let value = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a path string"))?
        .trim();
    if value.is_empty() {
        return Err(format!("{key} cannot be empty"));
    }
    normalize_settings_directory(PathBuf::from(value))
}

/// Surface opacity is rejected outside its bounds rather than silently clamped:
/// the floor exists to keep the interface readable, so a caller asking for `0`
/// has misunderstood the contract and should be told, not quietly given `40`.
fn parse_surface_opacity(value: &Value) -> Result<u8, String> {
    let key = "backgroundSurfaceOpacity";
    let number = value
        .as_u64()
        .ok_or_else(|| format!("{key} must be an integer percentage"))?;
    if number < u64::from(MIN_SURFACE_OPACITY) || number > u64::from(MAX_SURFACE_OPACITY) {
        return Err(format!(
            "{key} must be between {MIN_SURFACE_OPACITY} and {MAX_SURFACE_OPACITY}"
        ));
    }
    Ok(number as u8)
}

fn parse_blur_radius(value: &Value) -> Result<u8, String> {
    let key = "backgroundBlurRadius";
    let number = value
        .as_u64()
        .ok_or_else(|| format!("{key} must be an integer pixel radius"))?;
    if number < u64::from(MIN_BLUR_RADIUS) || number > u64::from(MAX_BLUR_RADIUS) {
        return Err(format!(
            "{key} must be between {MIN_BLUR_RADIUS} and {MAX_BLUR_RADIUS}"
        ));
    }
    Ok(number as u8)
}

fn parse_u32_range(
    value: &Value,
    key: &str,
    range: std::ops::RangeInclusive<u32>,
) -> Result<u32, String> {
    let (min, max) = (*range.start(), *range.end());
    let number = value
        .as_u64()
        .ok_or_else(|| format!("{key} must be a positive integer"))?;
    if number < u64::from(min) || number > u64::from(max) {
        return Err(format!("{key} must be between {min} and {max}"));
    }
    Ok(number as u32)
}
