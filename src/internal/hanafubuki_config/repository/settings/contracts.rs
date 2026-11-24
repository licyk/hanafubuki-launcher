use super::modules::ModuleSettings;
use super::*;
use crate::internal::hanafubuki_config::model::choice::setting_choice;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelDownloadSource {
    Modelscope,
    Huggingface,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    System,
    Manual,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitRuntimeSource {
    System,
    Micromamba,
}

/// The most verbose records the application log keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevelSetting {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Off,
}

/// What happens when an embedded WebUI asks for a new window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebviewNewWindow {
    /// The WebView engine opens its own popup, and the application offers to
    /// open the same destination in the system browser instead.
    PopupWithNotice,
    /// The WebView engine opens its own popup, and nothing else is shown.
    Popup,
    /// The page's request is refused and the destination opens in the system
    /// browser. Popup-based sign-in reports "blocked" under this choice.
    Browser,
}

setting_choice!(WebviewNewWindow, "webviewNewWindow", {
    PopupWithNotice => "popup_with_notice",
    Popup => "popup",
    Browser => "browser",
});

setting_choice!(LogLevelSetting, "logLevel", {
    Trace => "trace",
    Debug => "debug",
    Info => "info",
    Warn => "warn",
    Error => "error",
    Off => "off",
});

setting_choice!(GitRuntimeSource, "gitRuntimeSource", {
    System => "system",
    Micromamba => "micromamba",
});

setting_choice!(ProxyMode, "proxyMode", {
    System => "system",
    Manual => "manual",
    Disabled => "disabled",
});

setting_choice!(ModelDownloadSource, "modelDownloadSource", {
    Modelscope => "modelscope",
    Huggingface => "huggingface",
});

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub language: String,
    /// Appearance: theme preference plus the wallpaper presentation contract.
    /// The imported image itself (path, format, size, revision) is backend-owned
    /// appearance metadata, not a settings field; the appearance domain owns it.
    pub theme: ThemePreference,
    /// Seed the interface accent is derived from, as `#rrggbb`. See
    /// [`crate::internal::hanafubuki_config::model::presentation::DEFAULT_THEME_COLOR`].
    pub theme_color: String,
    pub background_image_enabled: bool,
    /// Paint a different wallpaper from the library on each application start
    /// instead of the one the user selected.
    pub background_random_enabled: bool,
    /// Alpha applied to surface *colors* at 0..=100, clamped to
    /// [`MIN_SURFACE_OPACITY`]..=[`MAX_SURFACE_OPACITY`].
    pub background_surface_opacity: u8,
    /// Frosted-glass blur radius in CSS px behind translucent surfaces,
    /// clamped to [`MIN_BLUR_RADIUS`]..=[`MAX_BLUR_RADIUS`]; 0 disables it.
    pub background_blur_radius: u8,
    pub background_image_fit: BackgroundFit,
    pub background_image_position: BackgroundPosition,
    /// User-configurable cache for downloaded models and Python-managed
    /// artifacts. Its default is the App Data `cache` directory.
    pub artifact_cache_dir: PathBuf,
    pub default_webui_install_dir: PathBuf,
    /// Where a file the embedded WebUI downloads is written. Defaults to
    /// [`AppPaths::default_downloads_dir`](crate::internal::hanafubuki_config::model::paths::AppPaths).
    pub webview_download_dir: PathBuf,
    pub proxy_mode: ProxyMode,
    pub proxy_address: Option<String>,
    pub auto_mirror: bool,
    pub use_micromamba_mirror: bool,
    pub use_pypi_mirror: bool,
    pub use_github_mirror: bool,
    pub custom_github_mirror: Option<String>,
    pub use_huggingface_mirror: bool,
    pub custom_huggingface_mirror: Option<String>,
    pub model_download_source: ModelDownloadSource,
    /// Desired version of the application-owned launcher Python. Changes are
    /// reconciled during the next application bootstrap, before the App API is
    /// started from this runtime.
    pub app_python_version: PythonVersion,
    /// Selects whether Git initialization first uses the desktop process PATH
    /// or goes directly through the application-owned Micromamba environment.
    pub git_runtime_source: GitRuntimeSource,
    pub enable_uv: bool,
    pub enable_environment_check: bool,
    /// Ask the launcher to install the CUDA asynchronous allocator before a
    /// WebUI starts. It only takes effect on a supported NVIDIA device; the
    /// launcher decides that at launch time, so the desktop side stores the
    /// user's intent and nothing else.
    pub use_cuda_malloc: bool,
    pub log_retention_days: u32,
    pub log_level: LogLevelSetting,
    /// Whether runtime logs are also written to the application log directory.
    /// In-memory collection is unconditional, so disabling this only stops the
    /// file publication; diagnostic bundles still contain the session log.
    pub log_file_persistence_enabled: bool,
    /// Size threshold at which the active log file is rotated.
    pub log_file_max_size_mb: u32,
    /// Include every current WebUI instance in an App-scoped diagnostic
    /// bundle. The instance target set is frozen when collection starts.
    pub app_diagnostics_include_all_instances: bool,
    /// Whether the application checks for its own updates in the background.
    /// A check the user starts from Settings is unaffected, and portable mode
    /// never checks automatically.
    pub auto_check_app_updates: bool,
    /// Whether the daily run checks every installed instance for kernel,
    /// extension, and PyTorch updates.
    pub auto_check_instance_updates: bool,
    /// Whether the daily run creates a snapshot of every installed instance.
    pub auto_create_instance_snapshots: bool,
    /// Whether the daily run checks every installed extension that names an
    /// update source for a newer version. It only reports; nothing installs.
    pub auto_check_extension_updates: bool,
    /// Whether every instance's embedded WebUI reads and writes one WebView
    /// data store, so a sign-in in one instance is a sign-in in all of them.
    /// Turning it off gives each instance a store of its own; a WebView already
    /// on screen keeps the store it was created with until it is next attached.
    pub webview_shared_cache: bool,
    /// Whether leaving an instance's workspace keeps its embedded WebUI loaded.
    /// The page is hidden rather than closed, so returning to the workspace
    /// finds it exactly as it was instead of reloading it. The WebUI itself
    /// keeps running either way; this decides only whether its *page* survives.
    pub webview_preserve_page_state: bool,
    /// How a WebUI's request for a new window (`window.open`, `target="_blank"`)
    /// is answered. Read when a WebUI page is opened, so a change applies to
    /// pages opened afterwards.
    pub webview_new_window: WebviewNewWindow,
    /// Whether WebKitGTK paints with the graphics card on Linux. On by default,
    /// since CPU painting is noticeably slower on most machines; off, it paints
    /// on the CPU with Mesa's software GL, the way out for the drivers and
    /// virtual GPUs that freeze or blank its windows. Read once, before the process
    /// starts any thread, so a change applies at the next start; other
    /// platforms persist it and ignore it.
    pub webview_gpu_acceleration: bool,
    /// Names of the application modules the user turned off. The module
    /// host leaves them unmounted and disposes one that is running when it is
    /// added; a name no module carries is kept but means nothing, and a
    /// module that cannot be turned off (a platform module) ignores it.
    /// Changed through `app_set_module_enabled`, never patched by Vue.
    pub disabled_modules: BTreeSet<String>,
    /// Each application module's own settings, by module name: a module
    /// that declares a configuration reads its entry (`modules.<name>`) and
    /// decodes it itself, tolerantly; a name no module carries is kept but
    /// means nothing. Written by Rust only, never patched by Vue.
    pub modules: ModuleSettings,
    /// Whether the one-time offer to scan this computer for existing WebUIs has
    /// already been answered. It records only that the question was put and
    /// closed — accepting and declining are the same answer here, because both
    /// mean the application must not ask again on the next start.
    pub system_webui_scan_prompted: bool,
}

/// Default of [`AppSettings::webview_gpu_acceleration`], shared with the
/// startup read that runs before the settings load.
pub const DEFAULT_WEBVIEW_GPU_ACCELERATION: bool = true;

impl AppSettings {
    pub fn defaults(paths: &AppPaths) -> Self {
        Self {
            language: "auto".to_string(),
            theme: ThemePreference::System,
            theme_color: DEFAULT_THEME_COLOR.to_string(),
            background_image_enabled: false,
            background_random_enabled: false,
            background_surface_opacity: DEFAULT_SURFACE_OPACITY,
            background_blur_radius: DEFAULT_BLUR_RADIUS,
            background_image_fit: BackgroundFit::Cover,
            background_image_position: BackgroundPosition::Center,
            artifact_cache_dir: paths.cache_dir.clone(),
            default_webui_install_dir: paths.managed_instances_dir.clone(),
            webview_download_dir: paths.default_downloads_dir.clone(),
            proxy_mode: ProxyMode::System,
            proxy_address: None,
            auto_mirror: true,
            use_micromamba_mirror: true,
            use_pypi_mirror: true,
            use_github_mirror: true,
            custom_github_mirror: None,
            use_huggingface_mirror: true,
            custom_huggingface_mirror: None,
            model_download_source: ModelDownloadSource::Modelscope,
            app_python_version: PythonVersion::Python315,
            git_runtime_source: GitRuntimeSource::System,
            enable_uv: true,
            enable_environment_check: true,
            use_cuda_malloc: true,
            log_retention_days: 14,
            log_level: LogLevelSetting::Info,
            log_file_persistence_enabled: true,
            log_file_max_size_mb: 8,
            app_diagnostics_include_all_instances: true,
            auto_check_app_updates: true,
            auto_check_instance_updates: true,
            auto_create_instance_snapshots: true,
            auto_check_extension_updates: true,
            webview_shared_cache: true,
            webview_preserve_page_state: true,
            webview_new_window: WebviewNewWindow::PopupWithNotice,
            webview_gpu_acceleration: DEFAULT_WEBVIEW_GPU_ACCELERATION,
            disabled_modules: BTreeSet::new(),
            modules: ModuleSettings::new(),
            system_webui_scan_prompted: false,
        }
    }

    /// Restore every Appearance-section field to its default. This is a
    /// presentation reset only: an imported background image is deliberately
    /// left on disk, so re-enabling the background brings it straight back.
    /// Deleting the asset is the separate, explicit Clear action.
    pub fn reset_appearance(&mut self) {
        self.theme = ThemePreference::System;
        self.theme_color = DEFAULT_THEME_COLOR.to_string();
        self.background_image_enabled = false;
        self.background_random_enabled = false;
        self.background_surface_opacity = DEFAULT_SURFACE_OPACITY;
        self.background_blur_radius = DEFAULT_BLUR_RADIUS;
        self.background_image_fit = BackgroundFit::Cover;
        self.background_image_position = BackgroundPosition::Center;
    }
}
