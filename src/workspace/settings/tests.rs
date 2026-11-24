use std::ffi::OsStr;
use std::fs;

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use tempfile::TempDir;

use super::{DiscoveredInstance, load_or_create, store};

fn child(root: &ResolvedPath, name: &str) -> ResolvedPath {
    ResolvedPath::resolve_child(root, OsStr::new(name)).unwrap()
}

#[test]
fn settings_defaults_old_empty_document_and_round_trips_scan_results() {
    let temporary = TempDir::new().unwrap();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let path = child(&root, "settings.json");
    fs::write(path.system(), b"{}").unwrap();

    let mut settings = load_or_create(&path).unwrap();
    assert!(!settings.scan_completed);
    assert!(settings.discovered_instances.is_empty());

    settings.scan_completed = true;
    settings.discovered_instances.push(DiscoveredInstance {
        instance_id: "launcher-import-1".to_string(),
        instance_identity: "0123456789abcdef0123456789abcdef".to_string(),
        name: "ComfyUI".to_string(),
        family: WebUiFamily::Comfyui,
        instance_path: Some("instance".to_string()),
        core_path: "instance/core".to_string(),
        python_path: Some("instance/python".to_string()),
    });
    store(&settings, &path).unwrap();

    let reloaded = load_or_create(&path).unwrap();
    assert!(reloaded.scan_completed);
    assert_eq!(reloaded.discovered_instances.len(), 1);
    assert_eq!(reloaded.discovered_instances[0].core_path, "instance/core");
    assert_eq!(
        reloaded.discovered_instances[0].instance_path.as_deref(),
        Some("instance")
    );
    assert_eq!(
        reloaded.discovered_instances[0].python_path.as_deref(),
        Some("instance/python")
    );
}

#[test]
fn load_removes_unknown_keys_and_restarts_an_invalid_scan() {
    let temporary = TempDir::new().unwrap();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let path = child(&root, "settings.json");
    fs::write(
        path.system(),
        r#"{
            "scanCompleted": "yes",
            "discoveredInstances": [],
            "obsolete": true
        }"#,
    )
    .unwrap();

    let loaded = load_or_create(&path).unwrap();
    assert!(!loaded.scan_completed);
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(path.system()).unwrap()).unwrap();
    assert!(stored.get("obsolete").is_none());
    assert_eq!(stored["scanCompleted"], false);
}

#[test]
fn load_recovers_from_the_safe_file_backup() {
    let temporary = TempDir::new().unwrap();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let path = child(&root, "settings.json");
    store(&super::LauncherSettings::default(), &path).unwrap();
    store(
        &super::LauncherSettings {
            scan_completed: true,
            discovered_instances: Vec::new(),
        },
        &path,
    )
    .unwrap();
    fs::write(path.system(), b"not json").unwrap();

    let loaded = load_or_create(&path).unwrap();
    assert!(!loaded.scan_completed);
}
