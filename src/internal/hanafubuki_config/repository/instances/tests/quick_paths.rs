use super::*;

fn quick_path(name: &str, path: &str) -> InstanceQuickPath {
    InstanceQuickPath {
        name: name.to_string(),
        path: PathBuf::from(path),
    }
}

fn absolute(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hanafubuki-quick-path-{label}"))
}

#[test]
fn quick_path_update_checks_revision_and_normalizes_entries() {
    let paths = test_paths("quick-paths");
    let state = ConfigState::default();
    let created = create_instance_config_inner(&paths, &state, sample_instance()).unwrap();
    let outputs = absolute("outputs");

    let updated = update_instance_quick_paths_inner(
        &paths,
        &state,
        "demo",
        created.config_revision,
        vec![quick_path(
            "  Outputs  ",
            outputs.join("nested").join("..").to_str().unwrap(),
        )],
    )
    .unwrap();

    assert_eq!(updated.config_revision, created.config_revision + 1);
    assert_eq!(
        updated.quick_paths,
        vec![quick_path("Outputs", outputs.to_str().unwrap())]
    );

    let stale = update_instance_quick_paths_inner(
        &paths,
        &state,
        "demo",
        created.config_revision,
        Vec::new(),
    )
    .unwrap_err();
    assert!(stale.contains("revision is stale"));

    // The list is owned wholesale, so an empty save clears it.
    let cleared = update_instance_quick_paths_inner(
        &paths,
        &state,
        "demo",
        updated.config_revision,
        Vec::new(),
    )
    .unwrap();
    assert!(cleared.quick_paths.is_empty());
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn an_unnamed_or_repeated_name_is_kept_because_position_is_the_address() {
    let paths = test_paths("quick-paths-free-names");
    let state = ConfigState::default();
    let created = create_instance_config_inner(&paths, &state, sample_instance()).unwrap();
    let outputs = absolute("free-outputs");
    let outputs = outputs.to_str().unwrap();
    let models = absolute("free-models");
    let models = models.to_str().unwrap();

    let updated = update_instance_quick_paths_inner(
        &paths,
        &state,
        "demo",
        created.config_revision,
        vec![
            quick_path("   ", outputs),
            quick_path("Models", models),
            quick_path("models", outputs),
        ],
    )
    .unwrap();

    assert_eq!(
        updated.quick_paths,
        vec![
            quick_path("", outputs),
            quick_path("Models", models),
            quick_path("models", outputs),
        ]
    );
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn quick_path_names_and_paths_stay_bounded_and_safe() {
    let paths = test_paths("quick-paths-invalid");
    let state = ConfigState::default();
    let created = create_instance_config_inner(&paths, &state, sample_instance()).unwrap();
    let target = absolute("models");
    let target = target.to_str().unwrap();

    let cases: Vec<(Vec<InstanceQuickPath>, &str)> = vec![
        (
            vec![quick_path(&"n".repeat(65), target)],
            "longer than 64 characters",
        ),
        (
            vec![quick_path("Line\nbreak", target)],
            "must not contain control characters",
        ),
        (
            vec![quick_path("Root", "/")],
            "refusing to use filesystem root",
        ),
        (
            (0..33)
                .map(|index| quick_path(&format!("entry-{index}"), target))
                .collect(),
            "exceed the limit of 32 entries",
        ),
    ];

    for (quick_paths, expected) in cases {
        let error = update_instance_quick_paths_inner(
            &paths,
            &state,
            "demo",
            created.config_revision,
            quick_paths,
        )
        .unwrap_err();
        assert!(error.contains(expected), "expected {expected} in {error}");
    }

    // Every rejection left the persisted configuration untouched.
    let stored = get_instance_config_inner(&paths, &state, "demo").unwrap();
    assert_eq!(stored.config_revision, created.config_revision);
    assert!(stored.quick_paths.is_empty());
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn an_unusable_persisted_entry_is_dropped_alone_rather_than_rejecting_the_instance() {
    let paths = test_paths("quick-paths-recovery");
    let state = ConfigState::default();
    create_instance_config_inner(&paths, &state, sample_instance()).unwrap();

    // An empty name is ordinary now, so the entry is unusable for the one
    // reason that still matters: it names no directory at all.
    let mut raw = serde_json::to_value(sample_instance()).unwrap();
    let models = paths.app_data_dir.join("models");
    raw["quickPaths"] = json!([
        { "name": "Outputs", "path": "" },
        { "name": "Models", "path": models, "pinned": true }
    ]);
    write_raw_instance(&paths, "demo", raw);

    let decoded = PersistedInstanceConfigDecoder::new(&paths)
        .decode(
            "demo",
            safe_file::read_json(&paths.instance_config_file("demo"))
                .unwrap()
                .unwrap(),
        )
        .unwrap();

    assert_eq!(
        decoded.config.quick_paths,
        [InstanceQuickPath {
            name: "Models".to_string(),
            path: models,
        }]
    );
    assert!(decoded.issues.iter().any(|issue| {
        issue.kind == PersistedInstanceIssueKind::InvalidField
            && issue.json_pointer == "/quickPaths/0"
    }));
    assert!(decoded.issues.iter().any(|issue| {
        issue.kind == PersistedInstanceIssueKind::UnsupportedField
            && issue.json_pointer == "/quickPaths/1/pinned"
    }));
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
