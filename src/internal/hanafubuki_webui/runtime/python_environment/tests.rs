use super::*;

fn split_path(value: &str) -> Vec<PathBuf> {
    std::env::split_paths(OsStr::new(value)).collect()
}

#[test]
fn path_entries_match_the_required_windows_and_unix_order() {
    let root = Path::new("runtime/python");
    assert_eq!(
        python_path_entries_for_platform(root, PythonPathPlatform::Windows).unwrap(),
        vec![root.to_path_buf(), root.join("Scripts")]
    );
    assert_eq!(
        python_path_entries_for_platform(root, PythonPathPlatform::Unix).unwrap(),
        vec![root.to_path_buf(), root.join("bin")]
    );
}

#[test]
fn configured_path_wins_over_the_process_fallback() {
    let configured = std::env::join_paths(["configured/one", "configured/two"])
        .unwrap()
        .into_string()
        .unwrap();
    let process = std::env::join_paths(["process/only"]).unwrap();
    let environment = merge_python_path_environment(
        BTreeMap::from([
            ("PATH".to_string(), configured),
            ("KEEP".to_string(), "value".to_string()),
        ]),
        Path::new("python"),
        Some(process.as_os_str()),
    )
    .unwrap();

    assert_eq!(environment["KEEP"], "value");
    let entries = split_path(&environment["PATH"]);
    assert_eq!(entries[0], PathBuf::from("python"));
    assert_eq!(
        &entries[2..],
        &[
            PathBuf::from("configured/one"),
            PathBuf::from("configured/two")
        ]
    );
    assert!(!entries.contains(&PathBuf::from("process/only")));
}

#[test]
fn process_path_is_used_when_no_configured_path_exists() {
    let process = std::env::join_paths(["process/one", "process/two"]).unwrap();
    let environment = merge_python_path_environment(
        BTreeMap::new(),
        Path::new("python"),
        Some(process.as_os_str()),
    )
    .unwrap();
    let entries = split_path(&environment["PATH"]);
    assert_eq!(
        &entries[2..],
        &[PathBuf::from("process/one"), PathBuf::from("process/two")]
    );
}

#[test]
fn an_explicit_empty_configured_path_does_not_fall_back_to_process_path() {
    let process = std::env::join_paths(["process/only"]).unwrap();
    let environment = merge_python_path_environment(
        BTreeMap::from([("PATH".to_string(), String::new())]),
        Path::new("python"),
        Some(process.as_os_str()),
    )
    .unwrap();
    assert_eq!(split_path(&environment["PATH"]).len(), 2);
}

#[test]
fn an_empty_python_root_is_rejected() {
    assert_eq!(
        merge_python_path_environment(BTreeMap::new(), Path::new(""), None),
        Err("Python root directory must not be empty".to_string())
    );
}

#[cfg(windows)]
#[test]
fn windows_path_keys_are_normalized_case_insensitively() {
    let environment = merge_python_path_environment(
        BTreeMap::from([
            ("Path".to_string(), "configured/one".to_string()),
            ("PATH".to_string(), "configured/two".to_string()),
        ]),
        Path::new("python"),
        None,
    )
    .unwrap();
    assert_eq!(
        environment
            .keys()
            .filter(|name| {
                crate::internal::hanafubuki_task::environment::native_environment_names_equal(
                    name, "PATH",
                )
            })
            .count(),
        1
    );
}
