use super::*;

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

fn sample_request() -> SdWebuiAllInOneRunRequest {
    SdWebuiAllInOneRunRequest {
        instance_id: "demo".to_string(),
        python_path: PathBuf::from("/path/to/python"),
        cli_args: owned(&["self-manager", "api", "serve"]),
        cwd: PathBuf::from("/work/dir"),
        env: Vec::new(),
        source_dir: PathBuf::from("/src/launcher-code"),
    }
}

/// Parse a produced PYTHONPATH string back into ordered path components,
/// platform-independently (avoids hard-coding `:` vs `;`).
fn pythonpath_parts(spec: &SdWebuiAllInOneRunSpec) -> Vec<PathBuf> {
    let value = spec
        .env
        .iter()
        .find(|(key, _)| key == PYTHONPATH_KEY)
        .map(|(_, value)| value.clone())
        .expect("PYTHONPATH present");
    std::env::split_paths(&value).collect()
}

#[test]
fn builds_basic_run_spec_shape() {
    let spec = build_sd_webui_all_in_one_run_spec(sample_request()).unwrap();
    assert_eq!(spec.instance_id, "demo");
    assert_eq!(spec.cwd, PathBuf::from("/work/dir"));
    assert_eq!(
        spec.command,
        owned(&[
            "/path/to/python",
            "-m",
            "sd_webui_all_in_one",
            "self-manager",
            "api",
            "serve",
        ])
    );
    // Exactly one PYTHONPATH entry, appended last.
    assert_eq!(spec.env.len(), 1);
    assert_eq!(spec.env[0].0, "PYTHONPATH");
}

#[test]
fn rejects_empty_instance_id() {
    let mut req = sample_request();
    req.instance_id = "   ".to_string();
    assert!(build_sd_webui_all_in_one_run_spec(req).is_err());
}

#[test]
fn rejects_empty_python_path() {
    let mut req = sample_request();
    req.python_path = PathBuf::from("");
    assert!(build_sd_webui_all_in_one_run_spec(req).is_err());
}

#[test]
fn rejects_empty_source_dir_and_cwd() {
    let mut req = sample_request();
    req.source_dir = PathBuf::from("");
    assert!(build_sd_webui_all_in_one_run_spec(req).is_err());

    let mut req = sample_request();
    req.cwd = PathBuf::from("");
    assert!(build_sd_webui_all_in_one_run_spec(req).is_err());
}

#[test]
fn pythonpath_order_source_then_package_patcher_then_existing() {
    let mut req = sample_request();
    req.env = vec![("PYTHONPATH".to_string(), "/extra/one".to_string())];
    let spec = build_sd_webui_all_in_one_run_spec(req).unwrap();
    assert_eq!(
        pythonpath_parts(&spec),
        vec![
            PathBuf::from("/src/launcher-code"),
            PathBuf::from("/src/launcher-code/sd_webui_all_in_one/patcher"),
            PathBuf::from("/extra/one"),
        ]
    );
}

#[test]
fn build_pythonpath_helper_orders_entries() {
    let value =
        build_pythonpath(Path::new("/src"), vec!["/a".to_string(), "/b".to_string()]).unwrap();
    let parts: Vec<PathBuf> = std::env::split_paths(&value).collect();
    assert_eq!(
        parts,
        vec![
            PathBuf::from("/src"),
            PathBuf::from("/src/sd_webui_all_in_one/patcher"),
            PathBuf::from("/a"),
            PathBuf::from("/b"),
        ]
    );
}

#[test]
fn normalized_environment_replaces_every_pythonpath_variant_once() {
    let environment = vec![
        ("KEEP".to_string(), "opaque".to_string()),
        ("pythonpath".to_string(), "/first".to_string()),
        ("PYTHONPATH".to_string(), "/second".to_string()),
    ];
    let normalized = normalize_pythonpath_environment(environment, Path::new("/launcher")).unwrap();
    assert_eq!(normalized[0], ("KEEP".to_string(), "opaque".to_string()));
    assert_eq!(
        normalized
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(PYTHONPATH_KEY))
            .count(),
        1
    );
    let parts: Vec<PathBuf> = std::env::split_paths(&normalized[1].1).collect();
    assert_eq!(
        parts,
        vec![
            PathBuf::from("/launcher"),
            PathBuf::from("/launcher/sd_webui_all_in_one/patcher"),
            PathBuf::from("/first"),
            PathBuf::from("/second"),
        ]
    );
}

#[test]
fn normalized_environment_rejects_a_source_path_with_native_list_separator() {
    #[cfg(unix)]
    let invalid = Path::new("/launcher:other");
    #[cfg(windows)]
    let invalid = Path::new("C:\\launcher\"other");
    assert!(normalize_pythonpath_environment(Vec::new(), invalid).is_err());
}

#[test]
fn merges_multiple_pythonpath_like_keys_in_encounter_order() {
    let mut req = sample_request();
    req.env = vec![
        ("PYTHONPATH".to_string(), "/a".to_string()),
        ("FOO".to_string(), "1".to_string()),
        ("pythonpath".to_string(), "/b".to_string()),
        ("PythonPath".to_string(), "/c".to_string()),
    ];
    let spec = build_sd_webui_all_in_one_run_spec(req).unwrap();

    // FOO preserved, then the single normalized PYTHONPATH.
    assert_eq!(spec.env[0], ("FOO".to_string(), "1".to_string()));
    assert_eq!(spec.env.len(), 2);
    assert_eq!(spec.env[1].0, "PYTHONPATH");
    assert_eq!(
        pythonpath_parts(&spec),
        vec![
            PathBuf::from("/src/launcher-code"),
            PathBuf::from("/src/launcher-code/sd_webui_all_in_one/patcher"),
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/c"),
        ]
    );
}

#[test]
fn case_insensitive_key_match_is_platform_independent() {
    // The same rule the builder uses to detect PYTHONPATH-like keys.
    assert!("pythonpath".eq_ignore_ascii_case(PYTHONPATH_KEY));
    assert!("PythonPath".eq_ignore_ascii_case(PYTHONPATH_KEY));
    assert!(!"PYTHONPATHX".eq_ignore_ascii_case(PYTHONPATH_KEY));
}

#[test]
fn preserves_non_pythonpath_env_in_input_order() {
    let mut req = sample_request();
    req.env = vec![
        ("A".to_string(), "1".to_string()),
        ("B".to_string(), "2".to_string()),
        ("C".to_string(), "3".to_string()),
    ];
    let spec = build_sd_webui_all_in_one_run_spec(req).unwrap();
    assert_eq!(spec.env[0], ("A".to_string(), "1".to_string()));
    assert_eq!(spec.env[1], ("B".to_string(), "2".to_string()));
    assert_eq!(spec.env[2], ("C".to_string(), "3".to_string()));
    assert_eq!(spec.env[3].0, "PYTHONPATH");
}

#[test]
fn does_not_check_path_existence() {
    let mut req = sample_request();
    req.python_path = PathBuf::from("/definitely/missing/python");
    req.cwd = PathBuf::from("/definitely/missing/cwd");
    req.source_dir = PathBuf::from("/definitely/missing/src");
    // Non-existent paths still build a spec successfully.
    assert!(build_sd_webui_all_in_one_run_spec(req).is_ok());
}
