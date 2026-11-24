use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::internal::hanafubuki_foundation::persistence::quarantine_file;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Read a JSON document, falling back to the `.bak` sibling the writer keeps.
///
/// Only a successfully read but invalid JSON document is quarantined before
/// the verified backup is atomically restored. An I/O failure uses the backup
/// without mutating the main path: it may be a transient sharing conflict.
/// When neither is usable the error names the main file requested by the caller.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    log::debug!("safe json read requested path={}", path.display());
    let main_error = match read_main(path) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    let Some((value, backup)) = read_backup_json(path) else {
        return Err(main_error.message());
    };
    if matches!(main_error, ReadError::Io(_)) {
        log::warn!(
            "safe json main read failed; using backup without modifying main path={} error={}",
            path.display(),
            main_error.message()
        );
        return Ok(Some(value));
    }
    let main_error = main_error.message();
    match quarantine_file(path) {
        Ok(quarantined) => {
            log::warn!(
                "safe json main document is unusable; quarantined it and restored the backup path={} quarantined={} error={main_error}",
                path.display(),
                quarantined
                    .as_deref()
                    .map_or_else(|| "none".into(), Path::to_string_lossy),
            );
            if let Err(error) = publish_bytes(path, &backup) {
                log::warn!(
                    "safe json backup restore failed path={} error={error}",
                    path.display()
                );
            }
        }
        // The main file stays where it was: overwriting a document that could
        // not be moved aside would lose it.
        Err(error) => log::warn!(
            "safe json main document is unusable and could not be quarantined; using the backup without restoring it path={} error={main_error}; quarantine: {error}",
            path.display()
        ),
    }
    Ok(Some(value))
}

/// Read a JSON document, or its `.bak` sibling, without writing anything.
///
/// For readers that run before the owning authority exists and so must not
/// repair: nothing is restored, moved, or created. The error names the main
/// file when neither document is usable.
pub fn peek_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match read_main(path) {
        Ok(value) => Ok(value),
        Err(main_error) => match read_backup_json(path) {
            Some((value, _)) => Ok(Some(value)),
            None => Err(main_error.message()),
        },
    }
}

enum ReadError {
    Io(String),
    InvalidJson(String),
}

impl ReadError {
    fn message(self) -> String {
        match self {
            Self::Io(message) | Self::InvalidJson(message) => message,
        }
    }
}

fn read_main<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, ReadError> {
    match fs::read(path) {
        Ok(content) => parse_json(path, &content)
            .map(Some)
            .map_err(ReadError::InvalidJson),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            log::debug!("safe json read missing path={}", path.display());
            Ok(None)
        }
        Err(err) => Err(ReadError::Io(format!("read {}: {err}", path.display()))),
    }
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    log::info!("safe json atomic write requested path={}", path.display());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }

    let is_file = match fs::metadata(path) {
        Ok(metadata) => metadata.is_file(),
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => return Err(format!("inspect {}: {error}", path.display())),
    };
    if is_file {
        let backup = backup_path(path);
        log::info!(
            "safe json backup existing file path={} backup={}",
            path.display(),
            backup.display()
        );
        publish_backup(path, &backup)?;
    }

    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| format!("serialize {}: {err}", path.display()))?;
    publish_bytes(path, &bytes)?;
    log::info!("safe json atomic write succeeded path={}", path.display());
    Ok(())
}

fn parse_json<T: DeserializeOwned>(path: &Path, content: &[u8]) -> Result<T, String> {
    serde_json::from_slice(content).map_err(|err| format!("parse {}: {err}", path.display()))
}

/// Keep the exact verified bytes so restoration never rereads a newer backup.
fn read_backup_json<T: DeserializeOwned>(path: &Path) -> Option<(T, Vec<u8>)> {
    let backup = backup_path(path);
    let content = match fs::read(&backup) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) => {
            log::warn!("read safe json backup {} failed: {error}", backup.display());
            return None;
        }
    };
    match parse_json(&backup, &content) {
        Ok(value) => Some((value, content)),
        Err(error) => {
            log::warn!("safe json backup is unusable: {error}");
            None
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    PathBuf::from(backup)
}

fn publish_backup(path: &Path, backup: &Path) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|err| format!("read backup source {}: {err}", path.display()))?;
    publish_bytes(backup, &bytes)
}

fn publish_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = temp_path(path);
    fs::write(&tmp, bytes).map_err(|err| format!("write {}: {err}", tmp.display()))?;
    publish_with_retry(&tmp, path).map_err(|err| {
        let _ = fs::remove_file(&tmp);
        format!("publish {} -> {}: {err}", tmp.display(), path.display())
    })
}

/// A staging path unique to this *write*, not merely to this process.
///
/// Two threads publishing the same file concurrently used to stage through the
/// same `<path>.<pid>.tmp`: their `fs::write` calls interleave, so the file that
/// then gets renamed into place can hold a mixture of both payloads. The rename
/// is atomic, which means the torn content is published atomically — readers see
/// a complete-looking file that does not parse. A per-write counter keeps the
/// two stagings apart, so each rename publishes exactly what its caller wrote.
fn temp_path(path: &Path) -> PathBuf {
    static NEXT_WRITE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT_WRITE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.{sequence}.tmp", std::process::id()));
    PathBuf::from(tmp)
}

fn publish_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last_err = None;
    for _ in 0..8 {
        match publish_file(from, to) {
            Ok(()) => return Ok(()),
            Err(err) if should_retry_publish(&err) => {
                last_err = Some(err);
                thread::sleep(Duration::from_millis(25));
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_err.unwrap_or_else(|| std::io::Error::new(ErrorKind::Other, "rename retry exhausted")))
}

fn should_retry_publish(_error: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        // Only sharing and lock violations identify a transient handle conflict.
        // Access denied can be a permanent permissions failure.
        matches!(_error.raw_os_error(), Some(32 | 33))
    }

    #[cfg(not(windows))]
    {
        false
    }
}

fn publish_file(from: &Path, to: &Path) -> std::io::Result<()> {
    // Rust's Windows rename falls back to POSIX replacement on access denied,
    // so open readers keep the old file while new opens see the replacement.
    // Calling MoveFileExW alone skips that fallback and can starve publishers.
    fs::rename(from, to)
}
