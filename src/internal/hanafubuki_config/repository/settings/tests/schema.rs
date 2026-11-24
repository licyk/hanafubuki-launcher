use super::*;

#[test]
fn language_defaults_to_auto_and_exposes_every_supported_preference() {
    let paths = test_paths("language");
    assert_eq!(AppSettings::defaults(&paths).language, "auto");

    let field = schema_fields()
        .into_iter()
        .find(|field| field["key"] == "language")
        .expect("language must be exposed by the schema");
    assert_eq!(field["kind"], "select");
    assert_eq!(field["commit"], "change");
    assert_eq!(
        field["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| option["value"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["auto", "zh", "en"]
    );

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn app_python_version_is_typed_validated_and_exposed_by_the_runtime_schema() {
    let paths = test_paths("app-python-version");
    let state = ConfigState::default();

    let updated =
        update_global_settings_patch_inner(&paths, &state, json!({"appPythonVersion": "3.13"}))
            .unwrap();
    assert_eq!(updated.app_python_version, PythonVersion::Python313);
    assert_eq!(
        serde_json::to_value(&updated).unwrap()["appPythonVersion"],
        "3.13"
    );

    for invalid in [json!("3.9"), json!("3.16"), json!(3.11), Value::Null] {
        assert!(
            update_global_settings_patch_inner(
                &paths,
                &state,
                json!({"appPythonVersion": invalid})
            )
            .is_err()
        );
        assert_eq!(
            get_global_settings_inner(&paths, &state)
                .unwrap()
                .app_python_version,
            PythonVersion::Python313
        );
    }

    let schema = serde_json::to_value(get_global_settings_schema()).unwrap();
    let field = schema["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|section| section["fields"].as_array().unwrap())
        .find(|field| field["key"] == "appPythonVersion")
        .unwrap();
    assert_eq!(field["kind"], "select");
    assert_eq!(
        field["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| option["value"].as_str().unwrap())
            .collect::<Vec<_>>(),
        PythonVersion::ALL
            .iter()
            .map(|version| version.as_str())
            .collect::<Vec<_>>()
    );

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn git_runtime_source_defaults_to_system_and_persists_micromamba_selection() {
    let paths = test_paths("git-runtime-source");
    let state = ConfigState::default();

    assert_eq!(
        get_global_settings_inner(&paths, &state)
            .unwrap()
            .git_runtime_source,
        GitRuntimeSource::System
    );
    let updated = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({"gitRuntimeSource": "micromamba"}),
    )
    .unwrap();
    assert_eq!(updated.git_runtime_source, GitRuntimeSource::Micromamba);
    assert_eq!(
        get_global_settings_inner(&paths, &state)
            .unwrap()
            .git_runtime_source,
        GitRuntimeSource::Micromamba
    );

    for invalid in [json!("managed"), json!(true), Value::Null] {
        assert!(
            update_global_settings_patch_inner(
                &paths,
                &state,
                json!({"gitRuntimeSource": invalid})
            )
            .is_err()
        );
    }

    let field = schema_fields()
        .into_iter()
        .find(|field| field["key"] == "gitRuntimeSource")
        .expect("Git runtime source must be exposed by the Runtime schema");
    assert_eq!(field["kind"], "select");
    assert_eq!(field["commit"], "change");
    assert_eq!(
        field["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| option["value"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["system", "micromamba"]
    );

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn cuda_memory_optimization_is_on_by_default_and_toggled_as_a_boolean() {
    let paths = test_paths("cuda-malloc");
    let state = ConfigState::default();

    let defaults = AppSettings::defaults(&paths);
    assert!(defaults.use_cuda_malloc);
    assert_eq!(
        serde_json::to_value(&defaults).unwrap()["useCudaMalloc"],
        true
    );

    let updated =
        update_global_settings_patch_inner(&paths, &state, json!({"useCudaMalloc": false}))
            .unwrap();
    assert!(!updated.use_cuda_malloc);
    assert!(
        !get_global_settings_inner(&paths, &state)
            .unwrap()
            .use_cuda_malloc,
        "the toggle must survive a reload from disk"
    );

    for invalid in [json!("false"), json!(0), Value::Null] {
        assert!(
            update_global_settings_patch_inner(&paths, &state, json!({"useCudaMalloc": invalid}))
                .is_err()
        );
    }

    let field = schema_fields()
        .into_iter()
        .find(|field| field["key"] == "useCudaMalloc")
        .expect("CUDA memory optimization must be exposed by the schema");
    assert_eq!(field["kind"], "boolean");
    assert_eq!(field["commit"], "change");

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn every_schema_field_has_a_default_the_editor_can_compare_against() {
    let paths = test_paths("schema-defaults");
    let defaults = serde_json::to_value(AppSettings::defaults(&paths)).unwrap();

    for field in schema_fields() {
        let key = field["key"].as_str().unwrap();
        assert!(
            defaults.get(key).is_some(),
            "{key} has no default to reset to"
        );
    }
}

#[test]
fn path_defaults_follow_the_resolved_application_layout() {
    let paths = test_paths("path-defaults");
    let defaults = AppSettings::defaults(&paths);

    // These two are why defaults cannot live in the static schema.
    assert_eq!(defaults.artifact_cache_dir, paths.cache_dir);
    assert_eq!(
        defaults.default_webui_install_dir,
        paths.managed_instances_dir
    );
}

#[test]
fn every_schema_field_declares_when_it_may_be_committed() {
    for field in schema_fields() {
        let commit = field["commit"]
            .as_str()
            .unwrap_or_else(|| panic!("{} does not declare a commit mode", field["key"]));
        assert!(
            matches!(commit, "change" | "blur" | "manual"),
            "{} declares an unknown commit mode {commit}",
            field["key"]
        );
    }
}

#[test]
fn only_fields_whose_persistence_touches_the_filesystem_require_an_explicit_save() {
    let manual: Vec<String> = schema_fields()
        .iter()
        .filter(|field| field["commit"] == "manual")
        .map(|field| field["key"].as_str().unwrap().to_string())
        .collect();

    // `prepare_settings_directories` creates exactly these directories on save,
    // so they are the only values an intermediate commit could leave on disk.
    assert_eq!(
        manual,
        vec![
            "artifactCacheDir",
            "defaultWebuiInstallDir",
            "webviewDownloadDir"
        ]
    );
}

#[test]
fn discrete_fields_commit_on_change_and_free_text_commits_on_blur() {
    for field in schema_fields() {
        let expected = match field["kind"].as_str().unwrap() {
            "select" | "boolean" => "change",
            // Directories are the manual case asserted separately above.
            "path" => continue,
            _ => "blur",
        };
        assert_eq!(
            field["commit"], expected,
            "{} should commit on {expected}",
            field["key"]
        );
    }
}

#[test]
fn validation_groups_cover_exactly_the_required_companion_rules() {
    let mut grouped: Vec<(String, String)> = schema_fields()
        .iter()
        .filter_map(|field| {
            Some((
                field["patchGroup"].as_str()?.to_string(),
                field["key"].as_str().unwrap().to_string(),
            ))
        })
        .collect();
    grouped.sort();

    // `validate_settings` requires proxyAddress with manual proxyMode. A patch
    // is all-or-nothing, so the pair has to reach Rust together.
    assert_eq!(
        grouped,
        vec![
            (
                validation_group::PROXY.to_string(),
                "proxyAddress".to_string()
            ),
            (validation_group::PROXY.to_string(), "proxyMode".to_string()),
        ]
    );
}
