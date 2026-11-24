use std::ffi::OsStr;
use std::fs;
use std::fs::OpenOptions;
use std::sync::Mutex;

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use log::{Level, Log, Record};

use super::{FileLogger, create_session_log};

#[test]
fn writes_level_target_and_message() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let path = ResolvedPath::resolve_child(&root, OsStr::new("launcher.log")).unwrap();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.system())
        .unwrap();
    let logger = FileLogger {
        file: Mutex::new(file),
        path: path.clone(),
    };
    logger.log(
        &Record::builder()
            .level(Level::Info)
            .target("launcher::repair")
            .args(format_args!("file installed path=bin/app bytes=42"))
            .build(),
    );
    logger.flush();

    let contents = fs::read_to_string(path.system()).unwrap();
    assert!(contents.contains("[INFO] [launcher::repair]"));
    assert!(contents.contains("file installed path=bin/app bytes=42"));
}

#[test]
fn creates_a_distinct_file_for_each_session() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ResolvedPath::resolve(temporary.path()).unwrap();
    let (first_path, first_file) = create_session_log(&root, 1_234, 56).unwrap();
    drop(first_file);
    let (second_path, second_file) = create_session_log(&root, 1_234, 56).unwrap();
    drop(second_file);

    assert_eq!(
        first_path.file_name(),
        Some(OsStr::new("launcher-1234-56.log"))
    );
    assert_eq!(
        second_path.file_name(),
        Some(OsStr::new("launcher-1234-56-1.log"))
    );
    assert!(first_path.system().is_file());
    assert!(second_path.system().is_file());
}
