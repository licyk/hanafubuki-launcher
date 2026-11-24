use std::fs;

use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_config::repository::settings::read_settings;
use crate::internal::hanafubuki_config::{AppPaths, BootstrapPaths};
use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_runtime_storage::RuntimeStorageConfig;

use super::reconcile;

#[test]
fn moved_portable_tree_rebases_all_main_project_paths() {
    let temporary = crate::workspace::test_support::tempdir();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let first_launcher = root.native().join("first");
    let second_launcher = root.native().join("second");
    let relative_data = ".hanafubuki-launcher/data";
    let first_data = first_launcher.join(relative_data);
    fs::create_dir_all(&first_data).unwrap();

    reconcile(&ResolvedPath::resolve(&first_data).unwrap()).unwrap();
    fs::rename(&first_launcher, &second_launcher).unwrap();

    let second_data = second_launcher.join(relative_data);
    reconcile(&ResolvedPath::resolve(&second_data).unwrap()).unwrap();
    assert!(
        !first_launcher.exists(),
        "reconciliation must not materialize directories from stale absolute settings paths"
    );

    let bootstrap = BootstrapPaths::from_app_data_dir(second_data.clone()).unwrap();
    let runtime: RuntimeStorageConfig =
        safe_file::read_json(&bootstrap.runtime_storage_config_file)
            .unwrap()
            .unwrap();
    assert_eq!(runtime.revision, 2);
    assert_eq!(
        runtime.managed_runtime_root,
        second_data.join("managed-runtime")
    );

    let paths = AppPaths::from_managed_runtime_root(
        second_data.clone(),
        second_data.join("managed-runtime"),
    )
    .unwrap();
    let settings = read_settings(&paths);
    assert_eq!(settings.artifact_cache_dir, second_data.join("cache"));
    assert_eq!(
        settings.default_webui_install_dir,
        second_data.join("instances")
    );
    assert_eq!(settings.webview_download_dir, second_data.join("downloads"));
}
