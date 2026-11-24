use super::*;
use crate::internal::hanafubuki_config::SettingsDomain;

#[test]
fn app_diagnostics_include_all_instances_defaults_to_on() {
    let paths = test_paths("diagnostics-defaults");
    let defaults = AppSettings::defaults(&paths);

    assert!(defaults.app_diagnostics_include_all_instances);
    assert_eq!(
        serde_json::to_value(&defaults).unwrap()["appDiagnosticsIncludeAllInstances"],
        json!(true)
    );
}

#[test]
fn app_diagnostics_include_all_instances_is_validated_and_persisted() {
    let paths = test_paths("diagnostics-patch");
    let state = ConfigState::default();

    let updated = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({"appDiagnosticsIncludeAllInstances": false}),
    )
    .unwrap();
    assert!(!updated.app_diagnostics_include_all_instances);

    for invalid in [json!("true"), json!(1), Value::Null] {
        assert!(
            update_global_settings_patch_inner(
                &paths,
                &state,
                json!({"appDiagnosticsIncludeAllInstances": invalid})
            )
            .is_err()
        );
    }

    let reloaded = get_global_settings_inner(&paths, &ConfigState::default()).unwrap();
    assert!(!reloaded.app_diagnostics_include_all_instances);
    assert!(paths.settings_file(SettingsDomain::Diagnostics).is_file());

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn diagnostics_schema_exposes_the_include_instances_toggle() {
    let field = schema_fields()
        .into_iter()
        .find(|field| field["key"] == "appDiagnosticsIncludeAllInstances")
        .unwrap();
    assert_eq!(field["kind"], "boolean");
    assert_eq!(field["commit"], "change");
}
