use std::ffi::OsStr;
use std::fs;

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;

use super::{CoreManifest, CoreManifestFile, load, store};

fn child(root: &ResolvedPath, name: &str) -> ResolvedPath {
    ResolvedPath::resolve_child(root, OsStr::new(name)).unwrap()
}

fn manifest(version: &str) -> CoreManifest {
    CoreManifest {
        version: version.to_string(),
        files: vec![CoreManifestFile {
            path: "hanafubuki".to_string(),
            sha256: "0".repeat(64),
        }],
    }
}

#[test]
fn load_removes_unknown_manifest_keys() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let path = child(&root, "core-manifest.json");
    fs::write(
        path.system(),
        format!(
            r#"{{
                "version": "1.0.0",
                "files": [{{"path": "hanafubuki", "sha256": "{}", "size": 4}}],
                "obsolete": true
            }}"#,
            "0".repeat(64)
        ),
    )
    .unwrap();

    let loaded = load(&path).unwrap().unwrap();
    assert_eq!(loaded.version, "1.0.0");
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(path.system()).unwrap()).unwrap();
    assert!(stored.get("obsolete").is_none());
    assert!(stored["files"][0].get("size").is_none());
}

#[test]
fn load_rejects_invalid_authority_fields() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let path = child(&root, "core-manifest.json");
    fs::write(path.system(), r#"{"version": 1, "files": []}"#).unwrap();

    assert!(load(&path).unwrap().is_none());
}

#[test]
fn load_recovers_from_the_safe_file_backup() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let path = child(&root, "core-manifest.json");
    store(&manifest("1.0.0"), &path).unwrap();
    store(&manifest("2.0.0"), &path).unwrap();
    fs::write(path.system(), b"not json").unwrap();

    let loaded = load(&path).unwrap().unwrap();
    assert_eq!(loaded.version, "1.0.0");
}
