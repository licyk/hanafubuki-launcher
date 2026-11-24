use super::*;

#[test]
fn validate_instance_id_rejects_traversal() {
    assert!(validate_instance_id("../escape").is_err());
    assert!(validate_instance_id("a/b").is_err());
    assert!(validate_instance_id("").is_err());
    assert!(validate_instance_id("ok-instance_1").is_ok());
}

#[cfg(unix)]
#[test]
fn validate_instance_config_rejects_bad_override_key() {
    let paths = test_paths("bad-override");
    let mut instance = sample_instance();
    instance.settings_overrides = json!({ "theme": "banana" });
    let err = validate_instance_config(&paths, &mut instance).unwrap_err();
    assert!(err.contains("theme"), "unexpected error: {err}");
}

fn quarantined_files(paths: &AppPaths) -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&paths.config_quarantine_dir, &mut found);
    found
}

fn persisted(paths: &AppPaths, id: &str) -> Value {
    serde_json::from_slice(&std::fs::read(paths.instance_config_file(id)).unwrap()).unwrap()
}

/// A persisted record is repaired field by field: what is unknown or invalid
/// goes (its original kept in quarantine), everything else survives.
#[test]
fn load_instance_repairs_fields_and_keeps_the_rest() {
    let cases = [
        (
            "unknown field",
            json!({"webuiType": "banana"}),
            "/webuiType",
        ),
        (
            "unknown nested component field",
            json!({"componentPaths": {
                "python": {"path": "PYTHON", "ownership": "managed"},
                "core": {"path": "CORE"}
            }}),
            "/componentPaths/python/ownership",
        ),
        (
            "invalid settings override",
            json!({"settingsOverrides": {"theme": "banana", "language": "en"}}),
            "/settingsOverrides/theme",
        ),
        (
            "invalid environment variable",
            json!({"envVars": {"BAD=KEY": "v", "GOOD": "kept"}}),
            "/envVars/BAD=KEY",
        ),
        (
            "unknown launch argument origin",
            json!({"launchArgumentOrigin": "fromTheFuture"}),
            "/launchArgumentOrigin",
        ),
    ];
    for (label, mut overlay, pointer) in cases {
        let paths = test_paths("field-repair");
        let state = ConfigState::default();
        let mut raw = json!({
            "id": "demo",
            "name": "Demo",
            "launchArgs": ["--listen"],
            "launchArgumentOrigin": "user",
            "quickPaths": [{"name": "Models", "path": paths.app_data_dir.join("models")}],
            "envVars": {"GOOD": "kept"},
            "settingsOverrides": {"language": "en"}
        });
        if let Some(components) = overlay.get_mut("componentPaths") {
            components["python"]["path"] = json!(paths.app_data_dir.join("python"));
            components["core"]["path"] = json!(paths.app_data_dir.join("core"));
        }
        for (key, value) in overlay.as_object().unwrap() {
            raw[key] = value.clone();
        }
        write_raw_instance(&paths, "demo", raw);

        let config = load_instance(&paths, &state, "demo")
            .unwrap_or_else(|error| panic!("{label}: {error}"));

        assert_eq!(config.name, "Demo", "{label}");
        assert_eq!(config.launch_args, ["--listen"], "{label}");
        assert_eq!(
            config.launch_argument_origin,
            LaunchArgumentOrigin::User,
            "{label}"
        );
        assert_eq!(
            config.env_vars.get("GOOD").map(String::as_str),
            Some("kept"),
            "{label}"
        );
        assert_eq!(config.quick_paths.len(), 1, "{label}");
        assert_eq!(config.settings_overrides["language"], "en", "{label}");
        let report = state.recovery_reports().pop().unwrap();
        assert!(
            report
                .actions
                .iter()
                .any(|action| action.json_pointer.as_deref() == Some(pointer)),
            "{label}: {:?}",
            report.actions
        );
        assert_eq!(quarantined_files(&paths).len(), 1, "{label}: original kept");
        let repaired = persisted(&paths, "demo");
        assert!(repaired.get("webuiType").is_none(), "{label}");
        assert_eq!(repaired["launchArgumentOrigin"], "user", "{label}");
        assert!(state.instance_issue("demo").is_none(), "{label}");
        let _ = std::fs::remove_dir_all(&paths.app_data_dir);
    }
}

#[test]
fn an_unknown_launch_argument_origin_without_arguments_awaits_the_built_in_defaults() {
    let paths = test_paths("origin-pending");
    let state = ConfigState::default();
    write_raw_instance(
        &paths,
        "demo",
        json!({"id": "demo", "name": "Demo", "launchArgumentOrigin": "fromTheFuture"}),
    );

    let config = load_instance(&paths, &state, "demo").unwrap();

    assert_eq!(
        config.launch_argument_origin,
        LaunchArgumentOrigin::PendingDefault
    );
    assert!(state.instance_issue("demo").is_none());
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

/// Identity and paths are never guessed: such a record is quarantined and the
/// instance made unavailable.
#[test]
fn load_instance_disables_an_untrustworthy_record() {
    for (label, raw) in [
        (
            "id mismatch",
            json!({"id": "other", "name": "Demo", "launchArgumentOrigin": "user"}),
        ),
        (
            "path policy",
            json!({"id": "demo", "name": "Demo", "installDir": "/", "launchArgumentOrigin": "user"}),
        ),
    ] {
        let paths = test_paths("disable");
        let state = ConfigState::default();
        write_raw_instance(&paths, "demo", raw);

        let error = load_instance(&paths, &state, "demo").unwrap_err();

        assert!(error.contains("unavailable"), "{label}: {error}");
        let issue = state.instance_issue("demo").expect(label);
        assert!(issue.quarantine_relative_path.is_some(), "{label}");
        assert!(!paths.instance_config_file("demo").exists(), "{label}");
        assert_eq!(quarantined_files(&paths).len(), 1, "{label}");
        // Reading again reports the same issue rather than a new recovery.
        let reports = state.recovery_reports().len();
        assert!(load_instance(&paths, &state, "demo").is_err(), "{label}");
        assert_eq!(state.recovery_reports().len(), reports, "{label}");
        let _ = std::fs::remove_dir_all(&paths.app_data_dir);
    }
}

#[test]
fn one_unusable_instance_config_does_not_fail_the_list() {
    let paths = test_paths("list-skips");
    let state = ConfigState::default();
    write_raw_instance(
        &paths,
        "good",
        json!({"id": "good", "name": "Good", "launchArgumentOrigin": "user"}),
    );
    write_raw_instance(
        &paths,
        "bad",
        json!({"id": "other", "name": "Bad", "launchArgumentOrigin": "user"}),
    );
    std::fs::create_dir_all(paths.instance_record_dir("torn")).unwrap();
    std::fs::write(paths.instance_config_file("torn"), "{ torn").unwrap();

    let configs = list_instance_configs_inner(&paths, &state).unwrap();

    assert_eq!(
        configs
            .iter()
            .map(|config| config.id.as_str())
            .collect::<Vec<_>>(),
        ["good"]
    );
    assert!(state.instance_issue("bad").is_some());
    assert!(state.instance_issue("torn").is_some());
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn a_duplicate_key_is_handled_by_the_same_reader_at_bootstrap_and_at_runtime() {
    let raw = r#"{"id": "demo", "name": "Demo", "name": "Other", "launchArgumentOrigin": "user"}"#;
    let runtime = test_paths("duplicate-runtime");
    std::fs::create_dir_all(runtime.instance_record_dir("demo")).unwrap();
    std::fs::write(runtime.instance_config_file("demo"), raw).unwrap();
    let runtime_state = ConfigState::default();
    assert!(load_instance(&runtime, &runtime_state, "demo").is_err());

    let bootstrap = test_paths("duplicate-bootstrap");
    std::fs::create_dir_all(bootstrap.instance_record_dir("demo")).unwrap();
    std::fs::write(bootstrap.instance_config_file("demo"), raw).unwrap();
    let bootstrap_state = ConfigState::default();
    crate::internal::hanafubuki_config::recovery::bootstrap_config(&bootstrap, &bootstrap_state);

    for (paths, state) in [(&runtime, &runtime_state), (&bootstrap, &bootstrap_state)] {
        let issue = state.instance_issue("demo").unwrap();
        assert!(issue.reason.contains("duplicate"), "{}", issue.reason);
        assert_eq!(quarantined_files(paths).len(), 1);
        let _ = std::fs::remove_dir_all(&paths.app_data_dir);
    }
}

#[test]
fn a_torn_config_is_restored_from_its_backup_once() {
    let paths = test_paths("backup-restore");
    let state = ConfigState::default();
    let created = create_instance_config_inner(&paths, &state, sample_instance()).unwrap();
    // A second write leaves the first one behind as the backup.
    update_instance_name_inner(
        &paths,
        &state,
        "demo",
        created.config_revision,
        "Renamed".into(),
    )
    .unwrap();
    std::fs::write(paths.instance_config_file("demo"), "{ torn").unwrap();

    let restored = load_instance(&paths, &state, "demo").unwrap();

    assert_eq!(restored.name, "Demo");
    assert_eq!(persisted(&paths, "demo")["name"], "Demo");
    let quarantined = quarantined_files(&paths);
    assert_eq!(quarantined.len(), 1);
    assert_eq!(std::fs::read_to_string(&quarantined[0]).unwrap(), "{ torn");
    let reports = state.recovery_reports().len();
    assert_eq!(reports, 1);

    load_instance(&paths, &state, "demo").unwrap();
    assert_eq!(
        state.recovery_reports().len(),
        reports,
        "a repaired config is not repaired again"
    );
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
