use super::*;

#[test]
fn a_system_binary_runtime_needs_its_program_on_path() {
    let dir = tempfile::tempdir().unwrap();
    let name = if cfg!(windows) { "tool.exe" } else { "tool" };
    std::fs::write(dir.path().join(name), b"").unwrap();
    let path = std::env::join_paths([dir.path()]).unwrap();
    let runtime = |name: &str| InstanceRuntime::SystemBinary {
        name: name.to_string(),
    };
    assert_eq!(
        require_runtime_program(&runtime("tool"), Some(&path)).unwrap(),
        Some(dir.path().join(name))
    );
    assert_eq!(
        require_runtime_program(&runtime("missing"), Some(&path)).unwrap_err(),
        "the instance's type runs on `missing`, which is not on PATH"
    );
    assert_eq!(
        require_runtime_program(&InstanceRuntime::None, None).unwrap(),
        None
    );
}
