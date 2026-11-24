use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

pub(super) static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn test_paths(name: &str) -> AppPaths {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::SeqCst);
    let app_data_dir = std::env::temp_dir().join(format!(
        "hanafubuki-config-settings-{name}-{}-{id}",
        std::process::id()
    ));
    // Exercise the Tauri-free constructor rather than hand-building fields.
    AppPaths::from_app_data_dir(app_data_dir).unwrap()
}

pub(super) fn schema_fields() -> Vec<Value> {
    serde_json::to_value(get_global_settings_schema()).unwrap()["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|section| section["fields"].as_array().unwrap().clone())
        .collect()
}

/// Read the persisted settings the way the application does when nothing is
/// published yet: through the one settings reader, committing its repairs.
pub(super) fn load_settings(paths: &AppPaths) -> Result<AppSettings, String> {
    get_global_settings_inner(paths, &ConfigState::default())
}
