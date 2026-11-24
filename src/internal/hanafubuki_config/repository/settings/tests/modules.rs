use super::*;

use crate::internal::hanafubuki_config::model::settings_domain::SettingsDomain;

#[test]
fn disabled_modules_is_a_bounded_set_of_names_persisted_in_its_own_document() {
    let paths = test_paths("disabled-modules");
    let state = ConfigState::default();
    assert!(AppSettings::defaults(&paths).disabled_modules.is_empty());

    // A name no module carries is kept; the module host ignores it.
    let updated = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({"disabledModules": ["diagnostics", "not-a-module", "diagnostics"]}),
    )
    .unwrap();
    assert_eq!(
        updated.disabled_modules.iter().collect::<Vec<_>>(),
        ["diagnostics", "not-a-module"]
    );
    let document: Value = safe_file::read_json(&paths.settings_file(SettingsDomain::Modules))
        .unwrap()
        .unwrap();
    assert_eq!(
        document,
        json!({"disabledModules": ["diagnostics", "not-a-module"], "modules": {}})
    );
    assert_eq!(
        get_global_settings_inner(&paths, &ConfigState::default())
            .unwrap()
            .disabled_modules,
        updated.disabled_modules
    );

    let too_many = (0..=MAX_DISABLED_MODULES)
        .map(|index| format!("module-{index}"))
        .collect::<Vec<_>>();
    for invalid in [
        json!("diagnostics"),
        json!([1]),
        json!([" diagnostics"]),
        json!(["x".repeat(MAX_MODULE_NAME_BYTES + 1)]),
        json!(too_many),
        Value::Null,
    ] {
        assert!(
            update_global_settings_patch_inner(&paths, &state, json!({"disabledModules": invalid}))
                .is_err(),
            "{invalid} must be rejected"
        );
    }

    let overrides = serde_json::Map::from_iter([("disabledModules".to_string(), json!([]))]);
    assert!(
        validate_settings_overrides(&paths, &overrides)
            .unwrap_err()
            .contains("global-only")
    );
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn module_settings_are_objects_by_module_name_read_entry_by_entry() {
    let paths = test_paths("module-settings");
    let state = ConfigState::default();
    assert!(AppSettings::defaults(&paths).modules.is_empty());

    let updated = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({"modules": {"an-extension": {"interval": 5, "unknown": true}}}),
    )
    .unwrap();
    assert_eq!(
        Value::Object(updated.modules["an-extension"].clone()),
        json!({"interval": 5, "unknown": true})
    );
    for invalid in [
        json!([]),
        json!({"an-extension": 5}),
        json!({" an-extension": {}}),
        Value::Null,
    ] {
        assert!(
            update_global_settings_patch_inner(&paths, &state, json!({"modules": invalid}))
                .is_err(),
            "{invalid} must be rejected"
        );
    }

    // Persisted, an entry that cannot be one is dropped alone; the rest is
    // read, and the document is repaired.
    let file = paths.settings_file(SettingsDomain::Modules);
    std::fs::write(
        &file,
        json!({
            "disabledModules": [],
            "modules": {"kept": {"a": 1}, "broken": "not an object", "": {}}
        })
        .to_string(),
    )
    .unwrap();
    let read = get_global_settings_inner(&paths, &ConfigState::default()).unwrap();
    assert_eq!(read.modules.keys().collect::<Vec<_>>(), ["kept"]);

    let overrides = serde_json::Map::from_iter([("modules".to_string(), json!({}))]);
    assert!(
        validate_settings_overrides(&paths, &overrides)
            .unwrap_err()
            .contains("global-only")
    );
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
