use super::*;

#[test]
fn log_file_persistence_defaults_to_on_with_a_bounded_file_size() {
    let paths = test_paths("log-file-defaults");
    let defaults = AppSettings::defaults(&paths);

    assert!(defaults.log_file_persistence_enabled);
    assert_eq!(defaults.log_file_max_size_mb, 8);
    assert_eq!(defaults.log_retention_days, 14);

    let encoded = serde_json::to_value(&defaults).unwrap();
    assert_eq!(encoded["logFilePersistenceEnabled"], json!(true));
    assert_eq!(encoded["logFileMaxSizeMb"], json!(8));
}

#[test]
fn log_file_settings_are_validated_and_persisted() {
    let paths = test_paths("log-file-patch");
    let state = ConfigState::default();

    let updated = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({"logFilePersistenceEnabled": false, "logFileMaxSizeMb": 64}),
    )
    .unwrap();
    assert!(!updated.log_file_persistence_enabled);
    assert_eq!(updated.log_file_max_size_mb, 64);

    for invalid in [json!(0), json!(513), json!("32"), json!(1.5), Value::Null] {
        assert!(
            update_global_settings_patch_inner(
                &paths,
                &state,
                json!({"logFileMaxSizeMb": invalid})
            )
            .is_err()
        );
    }
    for invalid in [json!("true"), json!(1), Value::Null] {
        assert!(
            update_global_settings_patch_inner(
                &paths,
                &state,
                json!({"logFilePersistenceEnabled": invalid})
            )
            .is_err()
        );
    }

    let reloaded = get_global_settings_inner(&paths, &ConfigState::default()).unwrap();
    assert!(!reloaded.log_file_persistence_enabled);
    assert_eq!(reloaded.log_file_max_size_mb, 64);

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn the_log_section_exposes_the_file_size_field_as_gated_by_the_persistence_toggle() {
    let fields = schema_fields();
    let toggle = fields
        .iter()
        .find(|field| field["key"] == "logFilePersistenceEnabled")
        .unwrap();
    assert_eq!(toggle["kind"], "boolean");
    assert_eq!(toggle["commit"], "change");

    let size = fields
        .iter()
        .find(|field| field["key"] == "logFileMaxSizeMb")
        .unwrap();
    assert_eq!(size["kind"], "number");
    assert_eq!(size["min"], json!(1));
    assert_eq!(size["max"], json!(512));
    assert_eq!(
        size["disabledWhen"],
        json!({"key": "logFilePersistenceEnabled", "equals": false})
    );
}
