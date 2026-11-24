use crate::internal::hanafubuki_config::repository::settings::{
    get_global_settings_inner, prepare_settings_directories, update_global_settings_patch_inner,
};
use crate::internal::hanafubuki_config::{AppPaths, BootstrapPaths, ConfigState};
use crate::internal::hanafubuki_foundation::paths::{PathKey, ResolvedPath};
use crate::internal::hanafubuki_runtime_storage::ManagedRuntimeStorageService;
use serde_json::{Map, Value};

pub struct PortableConfiguration {
    pub app_paths: AppPaths,
    pub config_state: ConfigState,
}

pub fn reconcile(app_data_directory: &ResolvedPath) -> Result<PortableConfiguration, String> {
    log::info!(
        target: "launcher::config",
        "portable configuration reconciliation started app_data={}",
        app_data_directory.native_text()
    );
    let app_data_dir = app_data_directory.native().to_path_buf();
    let bootstrap = BootstrapPaths::from_app_data_dir(app_data_dir.clone())?;
    let managed_runtime_root = bootstrap.default_managed_runtime_root();

    let runtime_storage = ManagedRuntimeStorageService::open(app_data_dir.clone())?;
    runtime_storage.rebase_to_default_after_parent_relocation()?;
    drop(runtime_storage);
    log::info!(target: "launcher::config", "managed runtime root rebased");

    let paths = AppPaths::from_managed_runtime_root(app_data_dir, managed_runtime_root)?;
    let state = ConfigState::default();
    crate::internal::hanafubuki_config::recovery::bootstrap_config(&paths, &state);
    log::info!(target: "launcher::config", "configuration startup recovery completed");
    let settings = get_global_settings_inner(&paths, &state)?;
    if !same_path(&settings.artifact_cache_dir, &paths.cache_dir)
        || !same_path(
            &settings.default_webui_install_dir,
            &paths.managed_instances_dir,
        )
        || !same_path(&settings.webview_download_dir, &paths.default_downloads_dir)
    {
        log::info!(
            target: "launcher::config",
            "updating portable application paths"
        );
        let mut patch = Map::new();
        patch.insert(
            "artifactCacheDir".to_string(),
            path_value(&paths.cache_dir)?,
        );
        patch.insert(
            "defaultWebuiInstallDir".to_string(),
            path_value(&paths.managed_instances_dir)?,
        );
        patch.insert(
            "webviewDownloadDir".to_string(),
            path_value(&paths.default_downloads_dir)?,
        );
        update_global_settings_patch_inner(&paths, &state, Value::Object(patch))?;
    } else {
        prepare_settings_directories(&settings)?;
        log::debug!(
            target: "launcher::config",
            "portable application paths are already current"
        );
    }

    log::info!(target: "launcher::config", "portable configuration reconciliation completed");
    Ok(PortableConfiguration {
        app_paths: paths,
        config_state: state,
    })
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    PathKey::of(left) == PathKey::of(right)
}

fn path_value(path: &std::path::Path) -> Result<Value, String> {
    serde_json::to_value(path)
        .map_err(|error| format!("Encode portable configuration path: {error}"))
}

#[cfg(test)]
mod tests;
