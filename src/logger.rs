use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use log::{Level, LevelFilter, Log, Metadata, Record};

const MAX_MESSAGE_BYTES: usize = 4 * 1024;

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

struct FileLogger {
    file: Mutex<File>,
    path: ResolvedPath,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Debug
            && (metadata.target() == "launcher" || metadata.target().starts_with("launcher::"))
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let message = truncate_message(&record.args().to_string());
        let line = format!(
            "[{}.{:03}] [{}] [{}] {}\n",
            timestamp.as_secs(),
            timestamp.subsec_millis(),
            record.level(),
            record.target(),
            message
        );
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(line.as_bytes());
            if record.level() <= Level::Warn {
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

pub fn initialize(log_directory: &ResolvedPath) -> Result<ResolvedPath, String> {
    if let Some(logger) = LOGGER.get() {
        return Ok(logger.path.clone());
    }

    let timestamp_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let (path, file) = create_session_log(log_directory, timestamp_millis, std::process::id())?;
    LOGGER
        .set(FileLogger {
            file: Mutex::new(file),
            path: path.clone(),
        })
        .map_err(|_| "Launcher logger was initialized concurrently".to_string())?;
    log::set_logger(LOGGER.get().expect("launcher logger was just initialized"))
        .map_err(|error| format!("Register launcher logger: {error}"))?;
    log::set_max_level(LevelFilter::Debug);
    log::info!(
        target: "launcher",
        "session started version={} log={}",
        env!("CARGO_PKG_VERSION"),
        path.native_text()
    );
    Ok(path)
}

fn create_session_log(
    directory: &ResolvedPath,
    timestamp_millis: u128,
    process_id: u32,
) -> Result<(ResolvedPath, File), String> {
    for sequence in 0..1_000_u16 {
        let suffix = if sequence == 0 {
            String::new()
        } else {
            format!("-{sequence}")
        };
        let name = format!("launcher-{timestamp_millis}-{process_id}{suffix}.log");
        let path = ResolvedPath::resolve_child(directory, OsStr::new(&name))
            .map_err(|error| error.to_string())?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.system())
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Create launcher log {}: {error}",
                    path.native_text()
                ));
            }
        }
    }
    Err("Create launcher log: too many filename collisions".into())
}

fn truncate_message(message: &str) -> String {
    if message.len() <= MAX_MESSAGE_BYTES {
        return message.to_string();
    }
    let mut end = MAX_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...[truncated]", &message[..end])
}

#[cfg(test)]
mod tests;
