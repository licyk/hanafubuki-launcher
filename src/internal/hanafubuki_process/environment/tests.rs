use super::*;

fn split_path(value: &str) -> Vec<PathBuf> {
    std::env::split_paths(OsStr::new(value)).collect()
}

#[test]
fn reserved_environment_values_override_caller_values() {
    let environment = BTreeMap::from([
        ("RESERVED".to_string(), "caller".to_string()),
        ("CALLER_ONLY".to_string(), "kept".to_string()),
    ]);
    let reserved = BTreeMap::from([("RESERVED".to_string(), "trusted".to_string())]);

    assert_eq!(
        merge_reserved_environment(environment, &reserved),
        BTreeMap::from([
            ("CALLER_ONLY".to_string(), "kept".to_string()),
            ("RESERVED".to_string(), "trusted".to_string()),
        ])
    );
}

#[test]
fn path_entries_are_prepended_and_stably_deduplicated() {
    let first = PathBuf::from("native-path-first");
    let second = PathBuf::from("native-path-second");
    let inherited = std::env::join_paths([
        second.clone(),
        PathBuf::from("native-path-inherited"),
        first.clone(),
    ])
    .unwrap();

    let value = prepend_path_entries(
        [first.clone(), second.clone(), first.clone()],
        Some(&inherited),
    )
    .unwrap();

    assert_eq!(
        split_path(&value),
        vec![first, second, PathBuf::from("native-path-inherited")]
    );
}

#[test]
fn path_prepend_rejects_an_empty_application_entry() {
    assert_eq!(
        prepend_path_entries([PathBuf::new()], None),
        Err("PATH prepend entry must not be empty".to_string())
    );
}

#[cfg(windows)]
#[test]
fn path_deduplication_uses_windows_case_and_separator_rules() {
    let value = prepend_path_entries(
        [PathBuf::from("C:/Tools/Micromamba")],
        Some(OsStr::new("c:\\tools\\MICROMAMBA;C:\\Other")),
    )
    .unwrap();
    assert_eq!(
        split_path(&value),
        vec![
            PathBuf::from("C:/Tools/Micromamba"),
            PathBuf::from("C:\\Other")
        ]
    );
}

#[cfg(windows)]
#[test]
fn native_environment_names_use_windows_ordinal_case_comparison() {
    assert!(native_environment_names_equal("Path", "PATH"));
    assert!(native_environment_names_equal("Straße", "STRAßE"));
    assert!(!native_environment_names_equal("PATH", "PAT H"));
}

#[cfg(windows)]
#[test]
fn reserved_environment_merge_removes_windows_case_aliases() {
    let environment = BTreeMap::from([("Path".to_string(), "caller".to_string())]);
    let reserved = BTreeMap::from([("PATH".to_string(), "trusted".to_string())]);

    assert_eq!(
        merge_reserved_environment(environment, &reserved),
        BTreeMap::from([("PATH".to_string(), "trusted".to_string())])
    );
}

#[test]
fn a_name_matches_exactly_or_by_a_trailing_star_prefix() {
    assert!(native_environment_name_matches("NODE_PATH", "NODE_PATH"));
    assert!(!native_environment_name_matches("NODE_PATH_X", "NODE_PATH"));
    assert!(native_environment_name_matches(
        "npm_config_cache",
        "npm_config_*"
    ));
    assert!(native_environment_name_matches(
        "npm_config_",
        "npm_config_*"
    ));
    assert!(!native_environment_name_matches(
        "npm_confi",
        "npm_config_*"
    ));
    assert!(native_environment_name_matches("anything", "*"));
    // A prefix that ends inside a multi-byte character never matches.
    assert!(!native_environment_name_matches("éa", "a*"));
}

#[cfg(not(windows))]
#[test]
fn names_match_with_their_case_outside_windows() {
    assert!(!native_environment_name_matches(
        "NPM_CONFIG_CACHE",
        "npm_config_*"
    ));
    assert!(!native_environment_name_matches("node_path", "NODE_PATH"));
}

#[cfg(windows)]
#[test]
fn names_match_without_regard_to_case_on_windows() {
    assert!(native_environment_name_matches(
        "NPM_CONFIG_CACHE",
        "npm_config_*"
    ));
    assert!(native_environment_name_matches("node_path", "NODE_PATH"));
}
