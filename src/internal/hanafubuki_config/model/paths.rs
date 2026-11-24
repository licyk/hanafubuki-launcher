use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_foundation::paths::normalize_path;

use super::settings_domain::SettingsDomain;

/// Paths that must remain available before the managed runtime location can be
/// resolved. They deliberately stay below the platform application-data root
/// and are never part of a runtime relocation.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPaths {
    pub app_data_dir: PathBuf,
    /// Global-settings documents, one per [`SettingsDomain`]. Here rather than
    /// only on [`AppPaths`] so a setting the process needs before Tauri starts
    /// is read from the same place the settings load writes it.
    pub settings_dir: PathBuf,
    pub runtime_storage_config_file: PathBuf,
    pub runtime_storage_journal_file: PathBuf,
    pub runtime_storage_lock_file: PathBuf,
}

impl BootstrapPaths {
    pub fn from_app_data_dir(app_data_dir: PathBuf) -> Result<Self, String> {
        let app_data_dir = normalize_path(app_data_dir)?;
        let state_dir = app_data_dir.join("state").join("runtime-storage");
        Ok(Self {
            runtime_storage_config_file: app_data_dir.join("config").join("runtime-storage.json"),
            settings_dir: app_data_dir.join("config"),
            runtime_storage_journal_file: state_dir.join("migration-journal.json"),
            runtime_storage_lock_file: state_dir.join("process.lock"),
            app_data_dir,
        })
    }

    /// Path of one persisted global-settings document.
    pub fn settings_file(&self, domain: SettingsDomain) -> PathBuf {
        self.settings_dir.join(domain.file_name())
    }

    pub fn default_managed_runtime_root(&self) -> PathBuf {
        self.app_data_dir.join("managed-runtime")
    }

    pub fn prepare(&self) -> Result<(), String> {
        for directory in [
            self.runtime_storage_config_file.parent(),
            self.runtime_storage_journal_file.parent(),
        ]
        .into_iter()
        .flatten()
        {
            std::fs::create_dir_all(directory)
                .map_err(|error| format!("create {}: {error}", directory.display()))?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ManagedRuntimeLayoutKind {
    Unified,
    Legacy,
}

/// Immutable, fully resolved managed-runtime layout shared by every runtime
/// consumer. Normal consumers never derive these directories themselves.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimePaths {
    layout: ManagedRuntimeLayoutKind,
    root: PathBuf,
    python_dir: PathBuf,
    micromamba_runtime_dir: PathBuf,
    micromamba_root_prefix: PathBuf,
    launcher_code_dir: PathBuf,
}

impl ManagedRuntimePaths {
    pub fn under(root: PathBuf) -> Result<Self, String> {
        let root = normalize_path(root)?;
        Ok(Self {
            layout: ManagedRuntimeLayoutKind::Unified,
            python_dir: root.join("python"),
            micromamba_runtime_dir: root.join("micromamba"),
            micromamba_root_prefix: root.join("conda"),
            launcher_code_dir: root.join("launcher-code"),
            root,
        })
    }

    /// Explicit transition layout for installations created before the
    /// unified root existed. This is selected only by bootstrap migration
    /// resolution and is never inferred by normal runtime consumers.
    pub fn legacy_under(app_data_dir: PathBuf) -> Result<Self, String> {
        let root = normalize_path(app_data_dir)?;
        Ok(Self {
            layout: ManagedRuntimeLayoutKind::Legacy,
            python_dir: root.join("runtime").join("python"),
            micromamba_runtime_dir: root.join("runtime").join("micromamba"),
            micromamba_root_prefix: root.join("conda"),
            launcher_code_dir: root.join("launcher-code"),
            root,
        })
    }

    pub fn layout(&self) -> ManagedRuntimeLayoutKind {
        self.layout
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn python_dir(&self) -> &Path {
        &self.python_dir
    }

    pub fn micromamba_runtime_dir(&self) -> &Path {
        &self.micromamba_runtime_dir
    }

    pub fn micromamba_root_prefix(&self) -> &Path {
        &self.micromamba_root_prefix
    }

    pub fn launcher_code_dir(&self) -> &Path {
        &self.launcher_code_dir
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub bootstrap: BootstrapPaths,
    pub managed_runtime: ManagedRuntimePaths,
    pub app_data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
    /// Durable, user-retrievable diagnostic bundles. Bundles are never removed
    /// automatically; incomplete staging data lives in an OS temporary directory.
    pub diagnostics_dir: PathBuf,
    /// Directory holding the per-domain global settings documents
    /// (`config/<domain>.json`). One file per [`SettingsDomain`], so a document
    /// that cannot be read costs only its own domain.
    pub settings_dir: PathBuf,
    /// Per-instance durable record root
    /// (`data/instances/<id>/{config,lifecycle,source-snapshot}.json`).
    pub instance_records_dir: PathBuf,
    /// Default root for application-managed instance workspaces.
    pub managed_instances_dir: PathBuf,
    /// The extension store (`data/extensions`): each installed extension's
    /// versions, its settings and its own data, and `state.json`.
    pub extensions_dir: PathBuf,
    /// Backend-owned appearance assets (`data/appearance`). Holds the wallpaper
    /// library and its image files. The frontend never receives a path
    /// into this directory; bytes are served through a Rust command.
    pub appearance_dir: PathBuf,
    /// Root of the WebView data stores the embedded WebUI browses in
    /// (`webview/`). It holds cache, cookies, and site storage — data the
    /// WebView engine owns and this application only locates, sizes, and
    /// clears. It is deliberately not below `cache`, whose default is also the
    /// user-relocatable artifact cache.
    pub webview_dir: PathBuf,
    /// Where a file the embedded WebUI downloads goes unless the user has
    /// chosen somewhere else (`downloads/` under the application data root).
    ///
    /// Deliberately the application's own directory rather than the operating
    /// system's downloads folder: what a WebUI fetches is a model or a dataset
    /// belonging to this application, it travels with a portable installation,
    /// and it does not silently mix into the folder the user's browser writes
    /// to. Settings → Paths moves it for anyone who wants it elsewhere.
    pub default_downloads_dir: PathBuf,
    /// Original invalid configuration files retained for explicit user recovery.
    pub config_quarantine_dir: PathBuf,
    /// Typed journals and bounded reports for startup configuration recovery.
    pub config_recovery_state_dir: PathBuf,
}

impl AppPaths {
    /// Build the path layout from a concrete app data directory.
    ///
    /// This constructor is deliberately side-effect free. The platform
    /// composition root resolves this contract once, calls
    /// [`Self::prepare_managed_directories`] during startup, and then shares the
    /// immutable result with command adapters and domain services.
    pub fn from_app_data_dir(app_data_dir: PathBuf) -> Result<Self, String> {
        let managed_runtime = ManagedRuntimePaths::legacy_under(app_data_dir.clone())?;
        Self::from_resolved_runtime(app_data_dir, managed_runtime)
    }

    pub fn from_managed_runtime_root(
        app_data_dir: PathBuf,
        managed_runtime_root: PathBuf,
    ) -> Result<Self, String> {
        let managed_runtime = ManagedRuntimePaths::under(managed_runtime_root)?;
        Self::from_resolved_runtime(app_data_dir, managed_runtime)
    }

    pub fn from_resolved_runtime(
        app_data_dir: PathBuf,
        managed_runtime: ManagedRuntimePaths,
    ) -> Result<Self, String> {
        let app_data_dir = normalize_path(app_data_dir)?;
        let bootstrap = BootstrapPaths::from_app_data_dir(app_data_dir.clone())?;
        let logs_dir = app_data_dir.join("logs");
        log::info!(
            "config paths building layout app_data_dir={}",
            app_data_dir.display()
        );

        let config_dir = app_data_dir.join("config");
        let data_dir = app_data_dir.join("data");
        let state_dir = app_data_dir.join("state");
        let cache_dir = app_data_dir.join("cache");
        let diagnostics_dir = data_dir.join("diagnostics");

        let instance_records_dir = data_dir.join("instances");
        let managed_instances_dir = app_data_dir.join("instances");
        let extensions_dir = data_dir.join("extensions");
        let appearance_dir = data_dir.join("appearance");
        let webview_dir = app_data_dir.join("webview");
        let default_downloads_dir = app_data_dir.join("downloads");
        let settings_dir = bootstrap.settings_dir.clone();
        let config_quarantine_dir = data_dir.join("config-quarantine");
        let config_recovery_state_dir = state_dir.join("diagnostics").join("config-recovery");
        Ok(Self {
            bootstrap,
            managed_runtime,
            settings_dir,
            instance_records_dir,
            managed_instances_dir,
            extensions_dir,
            appearance_dir,
            webview_dir,
            default_downloads_dir,
            config_quarantine_dir,
            config_recovery_state_dir,
            app_data_dir,
            config_dir,
            data_dir,
            state_dir,
            cache_dir,
            logs_dir,
            diagnostics_dir,
        })
    }

    /// Create the application-owned directory skeleton exactly once during
    /// platform startup. Domain repositories remain responsible for preparing
    /// their deeper private layout when they open it.
    pub fn prepare_managed_directories(&self) -> Result<(), String> {
        let instance_state_dir = self.state_dir.join("instances");
        let diagnostics_state_dir = self.state_dir.join("diagnostics");
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.state_dir,
            &self.cache_dir,
            &self.logs_dir,
            &self.diagnostics_dir,
            &self.instance_records_dir,
            &self.managed_instances_dir,
            &self.extensions_dir,
            self.managed_runtime.root(),
            &self.appearance_dir,
            &self.webview_dir,
            &self.config_quarantine_dir,
            &self.config_recovery_state_dir,
            &instance_state_dir,
            &diagnostics_state_dir,
        ] {
            std::fs::create_dir_all(dir)
                .map_err(|err| format!("create {}: {err}", dir.display()))?;
        }
        log::info!(
            "config paths resolved config_dir={} data_dir={} state_dir={} cache_dir={} logs_dir={} instance_records_dir={} managed_instances_dir={} settings_dir={}",
            self.config_dir.display(),
            self.data_dir.display(),
            self.state_dir.display(),
            self.cache_dir.display(),
            self.logs_dir.display(),
            self.instance_records_dir.display(),
            self.managed_instances_dir.display(),
            self.settings_dir.display()
        );
        Ok(())
    }

    /// Path of one persisted global-settings document.
    pub fn settings_file(&self, domain: SettingsDomain) -> PathBuf {
        self.bootstrap.settings_file(domain)
    }

    /// Directory that holds the application-owned launcher standalone Python
    /// (`python/` under the managed-runtime root).
    pub fn launcher_python_dir(&self) -> PathBuf {
        self.managed_runtime.python_dir().to_path_buf()
    }

    /// Directory that holds the application-owned micromamba executable copied
    /// from the packaged resource during application startup.
    pub fn micromamba_runtime_dir(&self) -> PathBuf {
        self.managed_runtime.micromamba_runtime_dir().to_path_buf()
    }

    /// Root prefix owned by bundled micromamba. Named environments live below
    /// `conda/envs`, and micromamba's shared package cache lives below
    /// `conda/pkgs`.
    pub fn micromamba_root_prefix(&self) -> PathBuf {
        self.managed_runtime.micromamba_root_prefix().to_path_buf()
    }

    pub fn launcher_code_dir(&self) -> PathBuf {
        self.managed_runtime.launcher_code_dir().to_path_buf()
    }

    /// Durable record directory for one validated instance id.
    pub fn instance_record_dir(&self, instance_id: &str) -> PathBuf {
        self.instance_records_dir.join(instance_id)
    }

    /// The user's instance presets (`config/instance-presets.json`), which
    /// only Rust writes.
    pub fn instance_presets_file(&self) -> PathBuf {
        self.config_dir.join("instance-presets.json")
    }

    /// Path to a single instance's generic configuration.
    pub fn instance_config_file(&self, instance_id: &str) -> PathBuf {
        self.instance_record_dir(instance_id).join("config.json")
    }

    /// Typed cross-domain instance maintenance journal.
    pub fn instance_maintenance_journal_file(&self) -> PathBuf {
        self.state_dir
            .join("instances")
            .join("maintenance-journal.json")
    }

    /// Durable results from the once-per-day automatic instance update scan.
    pub fn instance_update_status_file(&self) -> PathBuf {
        self.state_dir.join("instances").join("update-status.json")
    }

    /// The record of an installer handoff, read back by the next start.
    pub fn app_update_handoff_file(&self) -> PathBuf {
        self.state_dir.join("app-update-handoff.json")
    }

    /// Automatic update check times and a deferred install request.
    pub fn app_update_automation_file(&self) -> PathBuf {
        self.state_dir.join("app-update-automation.json")
    }

    /// The external MCP clients the user created, private to the user.
    pub fn management_mcp_clients_file(&self) -> PathBuf {
        self.config_dir.join("mcp").join("clients.json")
    }

    /// The management MCP server's URL, for the `hanafubuki-mcp` bridge,
    /// while it serves with external access on.
    pub fn management_mcp_endpoint_file(&self) -> PathBuf {
        self.data_dir.join("mcp").join("endpoint.json")
    }

    pub fn instance_lifecycle_file(&self, instance_id: &str) -> PathBuf {
        self.instance_record_dir(instance_id).join("lifecycle.json")
    }

    pub fn instance_source_snapshot_file(&self, instance_id: &str) -> PathBuf {
        self.instance_record_dir(instance_id)
            .join("source-snapshot.json")
    }

    pub fn process_watchdog_file(&self) -> PathBuf {
        self.state_dir
            .join("diagnostics")
            .join("process-watchdog.tsv")
    }

    /// The previous run's heartbeat file, kept so a bundle collected after a
    /// crash still shows when that run last answered.
    pub fn previous_process_watchdog_file(&self) -> PathBuf {
        self.state_dir
            .join("diagnostics")
            .join("process-watchdog.previous.tsv")
    }
}

#[cfg(test)]
mod tests;
