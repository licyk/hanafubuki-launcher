use super::*;
use crate::internal::hanafubuki_config::SettingsDomain;

const AUTOMATION_KEYS: [&str; 4] = [
    "autoCheckAppUpdates",
    "autoCheckInstanceUpdates",
    "autoCreateInstanceSnapshots",
    "autoCheckExtensionUpdates",
];

#[test]
fn automation_switches_default_to_on() {
    let paths = test_paths("automation-defaults");
    let defaults = AppSettings::defaults(&paths);

    assert!(defaults.auto_check_app_updates);
    assert!(defaults.auto_check_instance_updates);
    assert!(defaults.auto_create_instance_snapshots);
    assert!(defaults.auto_check_extension_updates);
    let serialized = serde_json::to_value(&defaults).unwrap();
    for key in AUTOMATION_KEYS {
        assert_eq!(serialized[key], json!(true), "{key}");
    }
}

#[test]
fn automation_switches_are_validated_and_persisted_independently() {
    let paths = test_paths("automation-patch");
    let state = ConfigState::default();

    let updated = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({
            "autoCheckAppUpdates": false,
            "autoCreateInstanceSnapshots": false,
            "autoCheckExtensionUpdates": false,
        }),
    )
    .unwrap();
    assert!(!updated.auto_check_app_updates);
    assert!(updated.auto_check_instance_updates);
    assert!(!updated.auto_create_instance_snapshots);
    assert!(!updated.auto_check_extension_updates);

    for key in AUTOMATION_KEYS {
        for invalid in [json!("true"), json!(1), Value::Null] {
            assert!(
                update_global_settings_patch_inner(&paths, &state, json!({ key: invalid }))
                    .is_err(),
                "{key}"
            );
        }
    }

    let reloaded = get_global_settings_inner(&paths, &ConfigState::default()).unwrap();
    assert!(!reloaded.auto_check_app_updates);
    assert!(reloaded.auto_check_instance_updates);
    assert!(!reloaded.auto_create_instance_snapshots);
    assert!(!reloaded.auto_check_extension_updates);
    assert!(paths.settings_file(SettingsDomain::Automation).is_file());

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn automation_switches_are_global_only() {
    for key in AUTOMATION_KEYS {
        assert!(crate::internal::hanafubuki_config::repository::settings::validation::is_instance_override_forbidden(key));
    }
}

#[test]
fn automation_schema_exposes_four_switches() {
    let fields = schema_fields();
    for key in AUTOMATION_KEYS {
        let field = fields.iter().find(|field| field["key"] == key).unwrap();
        assert_eq!(field["kind"], "boolean", "{key}");
        assert_eq!(field["commit"], "change", "{key}");
    }
}
