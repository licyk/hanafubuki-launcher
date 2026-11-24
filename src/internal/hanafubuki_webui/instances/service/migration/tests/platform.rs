use super::*;
#[cfg(unix)]
use crate::internal::hanafubuki_webui::instances::service::migration::copy::validate_migration_entry;

#[test]
fn cross_device_is_the_only_move_error_that_enables_copy_fallback() {
    #[cfg(unix)]
    let cross_device_code = 18;
    #[cfg(windows)]
    let cross_device_code = 17;
    let source = Path::new("source");
    let destination = Path::new("destination");
    let outcome = attempt_no_replace_move(source, destination, |_, _| {
        Err(std::io::Error::from_raw_os_error(cross_device_code))
    })
    .unwrap();
    assert_eq!(outcome, InitialMoveOutcome::CrossDevice);

    let error = attempt_no_replace_move(source, destination, |_, _| {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "locked or denied",
        ))
    })
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
}

#[test]
fn no_replace_publish_never_overwrites_a_competing_destination() {
    let root = temporary_root("destination-race");
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(source.join("source-value"), b"source").unwrap();
    fs::write(destination.join("competitor"), b"keep").unwrap();

    assert!(move_no_replace(&source, &destination).is_err());
    assert_eq!(fs::read(destination.join("competitor")).unwrap(), b"keep");
    assert_eq!(fs::read(source.join("source-value")).unwrap(), b"source");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn unsupported_special_file_is_rejected_without_following_it() {
    // A socket is a special file the test can create itself, so the check does
    // not depend on what the host's `/dev/null` happens to be.
    let root = crate::internal::hanafubuki_webui::test_support::tempdir();
    let path = root.path().join("s");
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let metadata = fs::symlink_metadata(&path).unwrap();
    let error = validate_migration_entry(&path, &metadata).unwrap_err();
    assert!(error.contains("unsupported special file"), "{error}");
}
