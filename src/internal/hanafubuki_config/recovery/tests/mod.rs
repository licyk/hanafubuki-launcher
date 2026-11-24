use std::fs;

mod settings_corpus;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::internal::hanafubuki_config::repository::settings::{
    AppSettings, settings_domain_document,
};
use crate::internal::hanafubuki_config::{AppPaths, ConfigState, SettingsDomain};

use super::{ConfigPersistenceHealth, ConfigRecoveryAttention, bootstrap_config};

fn paths(name: &str) -> AppPaths {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let paths = AppPaths::from_app_data_dir(std::env::temp_dir().join(format!(
        "hanafubuki-config-recovery-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )))
    .unwrap();
    paths.prepare_managed_directories().unwrap();
    paths
}

/// Write one domain's document exactly as the settings hold it.
fn write_domain(paths: &AppPaths, domain: SettingsDomain, settings: &AppSettings) {
    let document = settings_domain_document(domain, settings).unwrap();
    fs::write(
        paths.settings_file(domain),
        serde_json::to_vec_pretty(&document).unwrap(),
    )
    .unwrap();
}

#[test]
fn invalid_global_field_keeps_valid_fields_and_quarantines_original() {
    let paths = paths("settings-field");
    fs::write(
        paths.settings_file(SettingsDomain::Language),
        r#"{"language":"en"}"#,
    )
    .unwrap();
    fs::write(
        paths.settings_file(SettingsDomain::Appearance),
        r#"{"theme":"invalid"}"#,
    )
    .unwrap();
    let state = ConfigState::default();

    bootstrap_config(&paths, &state);

    let settings = state.settings_snapshot().unwrap();
    assert_eq!(settings.language, "en");
    assert!(!state.recovery_reports().is_empty());
    assert_eq!(state.persistence_health(), ConfigPersistenceHealth::Healthy);
    let repaired: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.settings_file(SettingsDomain::Appearance)).unwrap())
            .unwrap();
    assert_eq!(repaired["theme"], "system");
    assert!(
        fs::read_dir(&paths.config_quarantine_dir)
            .unwrap()
            .next()
            .is_some()
    );
    let _ = fs::remove_dir_all(paths.app_data_dir);
}

#[test]
fn one_corrupt_document_does_not_disturb_the_others() {
    // The whole point of splitting the settings file: a document that cannot be
    // parsed costs its own domain and nothing else.
    let paths = paths("settings-isolation");
    let mut settings = AppSettings::defaults(&paths);
    settings.language = "en".to_string();
    settings.log_retention_days = 42;
    settings.enable_uv = false;
    write_domain(&paths, SettingsDomain::Language, &settings);
    write_domain(&paths, SettingsDomain::Logging, &settings);
    write_domain(&paths, SettingsDomain::EnvironmentTools, &settings);
    fs::write(
        paths.settings_file(SettingsDomain::Appearance),
        "{not json at all",
    )
    .unwrap();
    let state = ConfigState::default();

    bootstrap_config(&paths, &state);

    let recovered = state.settings_snapshot().unwrap();
    assert_eq!(recovered.language, "en");
    assert_eq!(recovered.log_retention_days, 42);
    assert!(!recovered.enable_uv);
    // Only the unreadable domain fell back to its defaults.
    assert_eq!(
        recovered.theme,
        AppSettings::defaults(&paths).theme,
        "the corrupt appearance document should reset only itself"
    );
    assert_eq!(state.persistence_health(), ConfigPersistenceHealth::Healthy);
    let _ = fs::remove_dir_all(paths.app_data_dir);
}

#[test]
fn invalid_json_falls_back_without_aborting_bootstrap() {
    let paths = paths("settings-json");
    fs::write(paths.settings_file(SettingsDomain::Language), "{invalid").unwrap();
    let state = ConfigState::default();

    bootstrap_config(&paths, &state);

    assert!(state.settings_snapshot().is_some());
    assert_eq!(state.persistence_health(), ConfigPersistenceHealth::Healthy);
    assert!(
        state
            .recovery_reports()
            .iter()
            .any(|report| report.attention == ConfigRecoveryAttention::Warning)
    );
    let _ = fs::remove_dir_all(paths.app_data_dir);
}

#[test]
fn duplicate_keys_are_rejected_instead_of_using_last_value() {
    let paths = paths("settings-duplicate");
    fs::write(
        paths.settings_file(SettingsDomain::Language),
        r#"{"language":"en","language":"zh"}"#,
    )
    .unwrap();
    let state = ConfigState::default();

    bootstrap_config(&paths, &state);

    assert_eq!(state.settings_snapshot().unwrap().language, "auto");
    assert!(state.recovery_reports().iter().any(|report| {
        report
            .actions
            .iter()
            .any(|action| action.reason_code == "invalid_json")
    }));
    let _ = fs::remove_dir_all(paths.app_data_dir);
}

#[test]
fn bad_instance_does_not_hide_valid_instances() {
    let paths = paths("instances");
    let good_dir = paths.instance_record_dir("good");
    let bad_dir = paths.instance_record_dir("bad");
    fs::create_dir_all(&good_dir).unwrap();
    fs::create_dir_all(&bad_dir).unwrap();
    fs::write(
        good_dir.join("config.json"),
        r#"{"id":"good","name":"Good","launchArgumentOrigin":"user"}"#,
    )
    .unwrap();
    fs::write(
        bad_dir.join("config.json"),
        r#"{"id":"other","name":"Bad","launchArgumentOrigin":"user"}"#,
    )
    .unwrap();
    let state = ConfigState::default();

    bootstrap_config(&paths, &state);
    let listed =
        crate::internal::hanafubuki_config::repository::instances::list_instance_configs_inner(
            &paths, &state,
        )
        .unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "good");
    assert!(state.instance_issue("bad").is_some());
    let _ = fs::remove_dir_all(paths.app_data_dir);
}

#[test]
fn removed_nested_instance_fields_are_canonicalized_without_losing_current_data() {
    let paths = paths("instance-nested-fields");
    let instance_dir = paths.instance_record_dir("legacy");
    let install_dir = paths.app_data_dir.join("webui-legacy");
    let python_dir = install_dir.join("python-custom");
    let core_dir = install_dir.join("core-custom");
    let models_dir = paths.app_data_dir.join("elsewhere").join("models");
    fs::create_dir_all(&instance_dir).unwrap();
    fs::write(
        instance_dir.join("config.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "legacy",
            "name": "Legacy",
            "configRevision": 7,
            "installDir": install_dir,
            "componentPaths": {
                "python": {
                    "path": python_dir,
                    "ownership": "managed",
                    "removedLeaf": true
                },
                "core": {
                    "path": core_dir,
                    "ownership": "attached"
                },
                "models": {"path": models_dir}
            },
            "envVars": {"OWNERSHIP": "user-data", "KEEP": "yes"},
            "launchArgs": ["--api"],
            "launchArgumentOrigin": "user",
            "settingsOverrides": {
                "theme": "dark",
                "removedSetting": true,
                "agentMemory": {"enabled": false}
            },
            "removedRoot": true
        }))
        .unwrap(),
    )
    .unwrap();
    let state = ConfigState::default();

    bootstrap_config(&paths, &state);

    let listed =
        crate::internal::hanafubuki_config::repository::instances::list_instance_configs_inner(
            &paths, &state,
        )
        .unwrap();
    assert_eq!(listed.len(), 1);
    let config = &listed[0];
    assert_eq!(config.id, "legacy");
    assert_eq!(config.config_revision, 7);
    assert_eq!(
        config.env_vars.get("OWNERSHIP").map(String::as_str),
        Some("user-data")
    );
    assert_eq!(config.env_vars.get("KEEP").map(String::as_str), Some("yes"));
    assert_eq!(config.launch_args, vec!["--api"]);
    assert_eq!(config.settings_overrides["theme"], "dark");
    assert!(config.settings_overrides.get("removedSetting").is_none());
    assert!(config.settings_overrides.get("agentMemory").is_none());

    let repaired: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.instance_config_file("legacy")).unwrap()).unwrap();
    assert!(repaired.get("removedRoot").is_none());
    // The configuration does not know the instance's type, so a component
    // it does not know is the type's own and stays.
    assert_eq!(
        repaired["componentPaths"]["models"]["path"],
        serde_json::json!(models_dir)
    );
    assert!(
        repaired["componentPaths"]["python"]
            .get("ownership")
            .is_none()
    );
    assert!(
        repaired["componentPaths"]["python"]
            .get("removedLeaf")
            .is_none()
    );
    assert!(
        repaired["componentPaths"]["core"]
            .get("ownership")
            .is_none()
    );
    assert_eq!(repaired["envVars"]["OWNERSHIP"], "user-data");

    let reports = state.recovery_reports();
    let pointers: Vec<_> = reports
        .iter()
        .flat_map(|report| report.actions.iter())
        .filter_map(|action| action.json_pointer.as_deref())
        .collect();
    assert!(pointers.contains(&"/componentPaths/python/ownership"));
    assert!(pointers.contains(&"/componentPaths/core/ownership"));
    assert!(!pointers.contains(&"/componentPaths/models"));
    assert!(pointers.contains(&"/settingsOverrides/removedSetting"));
    assert!(pointers.contains(&"/settingsOverrides/agentMemory"));
    assert!(pointers.contains(&"/removedRoot"));

    let first_bytes = fs::read(paths.instance_config_file("legacy")).unwrap();
    let second_state = ConfigState::default();
    bootstrap_config(&paths, &second_state);
    assert!(second_state.recovery_reports().is_empty());
    assert_eq!(
        fs::read(paths.instance_config_file("legacy")).unwrap(),
        first_bytes
    );
    let _ = fs::remove_dir_all(paths.app_data_dir);
}

#[test]
fn missing_component_path_remains_unsafe_to_guess() {
    let paths = paths("instance-missing-component-path");
    let instance_dir = paths.instance_record_dir("missing-path");
    fs::create_dir_all(&instance_dir).unwrap();
    fs::write(
        instance_dir.join("config.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "missing-path",
            "name": "Missing path",
            "componentPaths": {
                "python": {"ownership": "managed"},
                "core": {"path": paths.app_data_dir.join("core")}
            },
            "launchArgumentOrigin": "user"
        }))
        .unwrap(),
    )
    .unwrap();
    let state = ConfigState::default();

    bootstrap_config(&paths, &state);

    let issue = state.instance_issue("missing-path").unwrap();
    assert!(
        issue
            .reason
            .contains("/componentPaths/python/path is required")
    );
    assert!(!paths.instance_config_file("missing-path").exists());
    let _ = fs::remove_dir_all(paths.app_data_dir);
}

#[test]
fn invalid_optional_instance_groups_reset_independently() {
    let paths = paths("instance-optional-groups");
    for id in ["bad-env", "bad-settings"] {
        fs::create_dir_all(paths.instance_record_dir(id)).unwrap();
    }
    fs::write(
        paths.instance_config_file("bad-env"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "bad-env",
            "name": "Bad env",
            "envVars": {"BAD=KEY": "value"},
            "launchArgumentOrigin": "user",
            "settingsOverrides": {"theme": "dark"}
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        paths.instance_config_file("bad-settings"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "bad-settings",
            "name": "Bad settings",
            "envVars": {"KEEP": "yes"},
            "launchArgumentOrigin": "user",
            "settingsOverrides": {"theme": "banana"}
        }))
        .unwrap(),
    )
    .unwrap();
    let state = ConfigState::default();

    bootstrap_config(&paths, &state);

    let listed =
        crate::internal::hanafubuki_config::repository::instances::list_instance_configs_inner(
            &paths, &state,
        )
        .unwrap();
    let bad_env = listed.iter().find(|config| config.id == "bad-env").unwrap();
    assert!(bad_env.env_vars.is_empty());
    assert_eq!(bad_env.settings_overrides["theme"], "dark");
    let bad_settings = listed
        .iter()
        .find(|config| config.id == "bad-settings")
        .unwrap();
    assert_eq!(
        bad_settings.env_vars.get("KEEP").map(String::as_str),
        Some("yes")
    );
    assert_eq!(bad_settings.settings_overrides, serde_json::json!({}));
    let _ = fs::remove_dir_all(paths.app_data_dir);
}
