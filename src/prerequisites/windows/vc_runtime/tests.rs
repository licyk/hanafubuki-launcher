use std::io::Cursor;

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;

use super::{MAX_DOWNLOAD_BYTES, TemporaryInstaller, copy_bounded};

#[test]
fn bounded_copy_reports_progress_and_preserves_bytes() {
    let input = vec![7_u8; 128 * 1024 + 13];
    let mut reader = Cursor::new(input.clone());
    let mut output = Vec::new();
    let mut progress = Vec::new();

    let completed = copy_bounded(
        &mut reader,
        &mut output,
        Some(input.len() as u64),
        |completed, total| progress.push((completed, total)),
    )
    .unwrap();

    assert_eq!(completed, input.len() as u64);
    assert_eq!(output, input);
    assert_eq!(
        progress.last(),
        Some(&(input.len() as u64, Some(input.len() as u64)))
    );
}

#[test]
fn bounded_copy_rejects_empty_incomplete_and_oversized_downloads() {
    let mut output = Vec::new();
    assert!(
        copy_bounded(
            &mut Cursor::new(Vec::<u8>::new()),
            &mut output,
            None,
            |_, _| {}
        )
        .unwrap_err()
        .contains("empty")
    );

    output.clear();
    assert!(
        copy_bounded(
            &mut Cursor::new(vec![1_u8; 4]),
            &mut output,
            Some(5),
            |_, _| {}
        )
        .unwrap_err()
        .contains("before it was complete")
    );

    output.clear();
    assert!(
        copy_bounded(
            &mut Cursor::new(vec![1_u8; 1]),
            &mut output,
            Some(MAX_DOWNLOAD_BYTES + 1),
            |_, _| {}
        )
        .unwrap_err()
        .contains("exceeds")
    );
}

#[test]
fn temporary_installer_is_removed_when_guard_drops() {
    let directory = tempfile::tempdir().unwrap();
    let directory = ResolvedPath::resolve(directory.path()).unwrap();
    let path = {
        let installer = TemporaryInstaller::create_in(&directory).unwrap();
        assert!(installer.path().system().exists());
        installer.path().native().to_path_buf()
    };

    assert!(!path.exists());
}
