use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn test_paths(name: &str) -> AppPaths {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::SeqCst);
    let app_data_dir = crate::internal::hanafubuki_config::test_support::temp_root().join(format!(
        "hanafubuki-config-instances-{name}-{}-{id}",
        std::process::id()
    ));
    let paths = AppPaths::from_app_data_dir(app_data_dir).unwrap();
    paths.prepare_managed_directories().unwrap();
    paths
}

pub(super) fn sample_instance() -> InstanceConfig {
    InstanceConfig {
        id: "demo".to_string(),
        name: "Demo".to_string(),
        config_revision: 1,
        install_dir: None,
        component_paths: None,
        quick_paths: Vec::new(),
        env_vars: BTreeMap::new(),
        launch_args: Vec::new(),
        launch_argument_origin: LaunchArgumentOrigin::User,
        settings_overrides: json!({}),
    }
}

pub(super) fn write_raw_instance(paths: &AppPaths, file_id: &str, raw: Value) {
    safe_file::write_json_atomic(&paths.instance_config_file(file_id), &raw).unwrap();
}
