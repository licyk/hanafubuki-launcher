use tempfile::tempdir;

use super::*;

#[cfg(windows)]
#[test]
fn launcher_update_helper_can_be_flushed_on_windows() {
    let temp = tempdir().unwrap();
    let root = ResolvedPath::resolve(temp.path()).unwrap();
    let helper = child(&root, HELPER_NAME).unwrap();
    fs::write(helper.system(), b"helper").unwrap();

    sync_file(&helper).unwrap();
}

#[test]
fn file_replacement_can_be_rolled_back() {
    let temp = tempdir().unwrap();
    let root = ResolvedPath::resolve(temp.path()).unwrap();
    let target = child(&root, installed_file_name()).unwrap();
    let candidate = child(&root, "candidate").unwrap();
    let backup = child(&root, "backup").unwrap();
    fs::write(target.system(), b"old").unwrap();
    fs::write(candidate.system(), b"new").unwrap();
    let target = ResolvedPath::resolve(target.native()).unwrap();
    let candidate = ResolvedPath::resolve(candidate.native()).unwrap();

    replace(&target, &candidate, &backup, LauncherInstallKind::File).unwrap();
    assert_eq!(fs::read(target.system()).unwrap(), b"new");
    rollback(&target, &backup, LauncherInstallKind::File).unwrap();
    assert_eq!(fs::read(target.system()).unwrap(), b"old");
}

#[test]
fn transaction_decoder_rejects_unknown_fields_and_nonce_mismatch() {
    let temp = tempdir().unwrap();
    let root = ResolvedPath::resolve(temp.path()).unwrap();
    let nonce = "0123456789abcdef0123456789abcdef";
    let path = transaction_path(&root, nonce).unwrap();
    fs::write(path.system(), br#"{"nonce":"wrong","unknown":true}"#).unwrap();
    assert!(load_transaction(&root, nonce).is_err());
    assert!(validate_nonce(nonce).is_ok());
    assert!(validate_nonce("../transaction").is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn application_bundle_replacement_can_be_rolled_back() {
    let temp = tempdir().unwrap();
    let root = ResolvedPath::resolve(temp.path()).unwrap();
    let target_path = temp.path().join("Hanafubuki Launcher.app");
    let candidate_path = temp.path().join("candidate.app");
    fs::create_dir(&target_path).unwrap();
    fs::create_dir(&candidate_path).unwrap();
    fs::write(target_path.join("marker"), b"old").unwrap();
    fs::write(candidate_path.join("marker"), b"new").unwrap();
    let target = ResolvedPath::resolve(&target_path).unwrap();
    let candidate = ResolvedPath::resolve(&candidate_path).unwrap();
    let backup = child(&root, "previous.app").unwrap();

    replace(&target, &candidate, &backup, LauncherInstallKind::MacApp).unwrap();
    assert_eq!(fs::read(target.system().join("marker")).unwrap(), b"new");
    rollback(&target, &backup, LauncherInstallKind::MacApp).unwrap();
    assert_eq!(fs::read(target.system().join("marker")).unwrap(), b"old");
}
