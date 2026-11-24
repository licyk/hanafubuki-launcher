use super::*;

#[test]
fn standalone_python_executable_is_platform_correct() {
    let dir = PathBuf::from("/opt/py");
    let exe = standalone_python_executable(&dir);
    #[cfg(windows)]
    assert_eq!(exe, dir.join("python.exe"));
    #[cfg(not(windows))]
    assert_eq!(exe, dir.join("bin").join("python3"));
}

#[test]
fn parses_python_version_output() {
    assert_eq!(
        parse_python_version("Python 3.11.14").as_deref(),
        Some("3.11.14")
    );
    assert_eq!(
        parse_python_version("Python 3.11.14\r\n").as_deref(),
        Some("3.11.14")
    );
    assert_eq!(
        parse_python_version("Python 3.14.3\n").as_deref(),
        Some("3.14.3")
    );
    assert_eq!(
        parse_python_version("some warning\nPython 3.12.12\n").as_deref(),
        Some("3.12.12")
    );
    assert_eq!(parse_python_version("no version here"), None);
    assert_eq!(parse_python_version(""), None);
}

#[test]
fn cleanup_after_failed_install_removes_dir_and_preserves_cause() {
    let dir = std::env::temp_dir().join(format!("hanafubuki-runtime-cleanup-{}", now_millis()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("partial.bin"), b"junk").unwrap();

    let out = cleanup_after_failed_install(&dir, "download failed".to_string());
    assert_eq!(out, "download failed");
    assert!(!dir.exists(), "broken dir was not removed");
    assert_eq!(cleanup_after_failed_install(&dir, "x".to_string()), "x");
}

#[test]
fn preserve_policy_keeps_partial_instance_python_directory() {
    let dir = std::env::temp_dir().join(format!("hanafubuki-runtime-preserve-{}", now_millis()));
    std::fs::create_dir_all(&dir).unwrap();
    let error = failed_install_error(&dir, "extract failed".to_string(), false);
    assert_eq!(error, "extract failed");
    assert!(
        dir.exists(),
        "partial instance Python directory was removed"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cancellable_ensure_stops_before_touching_python_directory() {
    let dir = std::env::temp_dir().join(format!("hanafubuki-runtime-cancel-{}", now_millis()));
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let error = ensure_standalone_python_runtime_preserving_failure_cancellable(
        &ExecutorManager::default(),
        StandalonePythonRuntimeEnsureRequest {
            python_dir: dir.clone(),
            default_version: PythonVersion::Python311,
        },
        &cancellation,
    )
    .unwrap_err();
    assert_eq!(error, PythonInstallError::Cancelled);
    assert!(!dir.exists());
}

#[test]
fn verify_removes_dir_when_executable_missing() {
    let dir = std::env::temp_dir().join(format!("hanafubuki-runtime-noexe-{}", now_millis()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("garbage"), b"not a python").unwrap();

    let executor = ExecutorManager::default();
    let err = verify_installed_python(&executor, &dir, PythonVersion::Python311).unwrap_err();
    assert!(
        err.contains("executable not found"),
        "unexpected error: {err}"
    );
    assert!(!dir.exists(), "partial dir was not removed");
}

#[cfg(unix)]
fn write_fake_python(dir: &Path, body: &str) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("python3");
    crate::internal::hanafubuki_process::test_support::write_executable_script(
        &exe,
        &format!("#!/bin/sh\n{body}\n"),
    );
    exe
}

#[cfg(unix)]
#[test]
fn check_detects_usable_fake_python_and_ensure_returns_it() {
    let dir = std::env::temp_dir().join(format!("hanafubuki-runtime-ensure-{}", now_millis()));
    let exe = write_fake_python(&dir, "echo 'Python 3.11.14'");
    let executor = ExecutorManager::default();

    let check = check_standalone_python_runtime(&executor, &exe);
    // Report why the interpreter never ran, so a spawn failure is not mistaken
    // for the interpreter itself being unusable.
    assert_eq!(check.error, None, "the check could not be run");
    assert!(check.usable, "check output was {:?}", check.version_output);
    assert_eq!(check.exit_code, Some(0));

    let info = ensure_standalone_python_runtime(
        &executor,
        StandalonePythonRuntimeEnsureRequest {
            python_dir: dir.clone(),
            default_version: PythonVersion::Python311,
        },
    )
    .unwrap();
    assert!(info.usable);
    assert_eq!(info.version, "3.11.14");
    assert_eq!(info.python_path, exe);

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn verify_removes_dir_when_check_unusable() {
    let dir = std::env::temp_dir().join(format!("hanafubuki-runtime-badinstall-{}", now_millis()));
    write_fake_python(&dir, "exit 3");
    let executor = ExecutorManager::default();
    let err = verify_installed_python(&executor, &dir, PythonVersion::Python311).unwrap_err();
    assert!(err.contains("not usable"), "unexpected error: {err}");
    assert!(!dir.exists(), "unusable install dir was not removed");
}

#[cfg(unix)]
#[test]
fn check_reports_unusable_for_nonzero_exit() {
    let dir = std::env::temp_dir().join(format!("hanafubuki-runtime-bad-{}", now_millis()));
    let exe = write_fake_python(&dir, "echo 'boom' >&2\nexit 1");
    let executor = ExecutorManager::default();
    let check = check_standalone_python_runtime(&executor, &exe);
    assert_eq!(check.error, None, "the check could not be run");
    assert!(!check.usable);
    assert_eq!(check.exit_code, Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}
