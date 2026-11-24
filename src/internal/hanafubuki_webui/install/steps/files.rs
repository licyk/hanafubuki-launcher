//! The steps that touch only files: each writes beside its target first and
//! moves into place only once it has everything, so a step that fails
//! leaves the files it would have replaced as they were.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_python_runtime::{
    PythonInstallError, UpwardLinks, download_verified_file, extract_archive_confined,
};

use super::model::{ComponentPath, DownloadTarget, StepEnvironment};
use crate::internal::hanafubuki_webui::install::InstanceInstallStageError;

const PROBE_INTERVAL: Duration = Duration::from_millis(250);
const PROBE_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn download(
    url: &str,
    sha256: &str,
    target: &DownloadTarget,
    environment: &StepEnvironment,
    cancellation: &CancellationToken,
) -> Result<(), InstanceInstallStageError> {
    let urls = environment.download_urls(url);
    let proxy = environment.proxy.as_deref();
    match target {
        DownloadTarget::File(path) => {
            let destination = contained_parent(path)?.join(file_name(path)?);
            download_verified_file(&urls, sha256, &destination, proxy, cancellation)
                .map_err(stage_error)?;
        }
        DownloadTarget::UnpackInto {
            into,
            strip,
            replace,
        } => {
            let staging = staging_dir(into)?;
            let archive = staging.path().join(archive_name(url));
            download_verified_file(&urls, sha256, &archive, proxy, cancellation)
                .map_err(stage_error)?;
            unpack(&archive, into, *strip, *replace, cancellation)?;
        }
    }
    Ok(())
}

/// Extracts `archive` beside `into`, drops the first `strip` components of
/// each entry's path, then moves each entry into it, replacing an entry of
/// the same name, or with `replace` swaps the extracted folder in for
/// `into` whole. Until the last move nothing in `into` has changed, so a
/// failed or cancelled step leaves it as it was.
pub(crate) fn unpack(
    archive: &Path,
    into: &ComponentPath,
    strip: u32,
    replace: bool,
    cancellation: &CancellationToken,
) -> Result<(), InstanceInstallStageError> {
    let staging = staging_dir(into)?;
    let upward_links = UpwardLinks::Within(strip as usize);
    extract_archive_confined(archive, staging.path(), upward_links, cancellation)
        .map_err(stage_error)?;
    let stripped = match strip {
        0 => None,
        _ => {
            let stripped = staging_dir(into)?;
            strip_components(staging.path(), stripped.path(), strip)?;
            Some(stripped)
        }
    };
    let extracted = stripped.as_ref().unwrap_or(&staging).path();
    if replace {
        if cancellation.is_cancelled() {
            return Err(InstanceInstallStageError::Cancelled);
        }
        return swap_into_place(extracted, into);
    }
    move_into_place(extracted, into)
}

/// Puts `source` where `into` is: the old folder is moved aside beside it
/// first, `source` moved in, and the old one deleted. A rename that fails
/// (on Windows, a file open in the old folder) moves the old folder back,
/// so `into` always holds one whole version.
fn swap_into_place(source: &Path, into: &ComponentPath) -> Result<(), InstanceInstallStageError> {
    let target = into.path();
    if fs::symlink_metadata(&target).is_err() {
        return fs::rename(source, &target)
            .map_err(|error| failed(format!("move files to {}: {error}", target.display())));
    }
    let aside = staging_dir(into)?;
    let old = aside.path().join("replaced");
    fs::rename(&target, &old)
        .map_err(|error| failed(format!("move {} aside: {error}", target.display())))?;
    if let Err(error) = fs::rename(source, &target) {
        return Err(match fs::rename(&old, &target) {
            Ok(()) => failed(format!("move files to {}: {error}", target.display())),
            Err(restore) => failed(format!(
                "move files to {}: {error}; the previous files stay in {}: {restore}",
                target.display(),
                aside.keep().join("replaced").display()
            )),
        });
    }
    // The old folder goes with `aside`; one that cannot be deleted only
    // leaves a staging folder behind.
    Ok(())
}

/// Moves the entries `strip` directories deep in `extracted` into
/// `destination`. A file or a link above that depth would have nothing left
/// and is refused; a directory there only strips away. No link
/// is followed, and the extractor kept every link's target inside the
/// directory `strip` deep that the link moves with, so nothing moved can
/// reach out of `into`.
fn strip_components(
    extracted: &Path,
    destination: &Path,
    strip: u32,
) -> Result<(), InstanceInstallStageError> {
    let mut level = vec![extracted.to_path_buf()];
    for _ in 0..strip {
        let mut deeper = Vec::new();
        for dir in level {
            for path in entries(&dir)? {
                let metadata =
                    fs::symlink_metadata(&path).map_err(|error| failed(error.to_string()))?;
                if !metadata.is_dir() {
                    let name = path.strip_prefix(extracted).unwrap_or(&path);
                    return Err(failed(format!(
                        "archive entry `{}` has nothing left once {strip} leading path \
                         components are stripped",
                        name.display()
                    )));
                }
                deeper.push(path);
            }
        }
        level = deeper;
    }
    for dir in level {
        for path in entries(&dir)? {
            let Some(name) = path.file_name() else {
                continue;
            };
            let target = destination.join(name);
            if fs::symlink_metadata(&target).is_ok() {
                return Err(failed(format!(
                    "two archive entries strip to `{}`",
                    name.to_string_lossy()
                )));
            }
            fs::rename(&path, &target)
                .map_err(|error| failed(format!("move {}: {error}", path.display())))?;
        }
    }
    Ok(())
}

fn entries(dir: &Path) -> Result<Vec<PathBuf>, InstanceInstallStageError> {
    fs::read_dir(dir)
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.path()))
                .collect()
        })
        .map_err(|error| failed(format!("read {}: {error}", dir.display())))
}

/// Moves `source` to `into` when nothing is there, and otherwise each of
/// its entries into `into`, replacing an entry of the same name.
pub(crate) fn move_into_place(
    source: &Path,
    into: &ComponentPath,
) -> Result<(), InstanceInstallStageError> {
    let target = into.path();
    if !target.exists() {
        return fs::rename(source, &target)
            .map_err(|error| failed(format!("move files to {}: {error}", target.display())));
    }
    for entry in fs::read_dir(source).map_err(|error| failed(error.to_string()))? {
        let entry = entry.map_err(|error| failed(error.to_string()))?;
        let destination = target.join(entry.file_name());
        remove_existing(&destination)?;
        fs::rename(entry.path(), &destination).map_err(|error| {
            failed(format!(
                "move {} into place: {error}",
                destination.display()
            ))
        })?;
    }
    Ok(())
}

pub(crate) fn write_file(
    path: &ComponentPath,
    content: &str,
) -> Result<(), InstanceInstallStageError> {
    let parent = contained_parent(path)?;
    let destination = parent.join(file_name(path)?);
    let mut staged = tempfile::Builder::new()
        .prefix(".write-")
        .tempfile_in(&parent)
        .map_err(|error| failed(format!("write {}: {error}", destination.display())))?;
    staged
        .write_all(content.as_bytes())
        .map_err(|error| failed(format!("write {}: {error}", destination.display())))?;
    staged
        .persist(&destination)
        .map_err(|error| failed(format!("write {}: {}", destination.display(), error.error)))?;
    Ok(())
}

/// Waits for a 2xx answer to `GET url`, a loopback `http://` URL.
pub(crate) fn probe(
    url: &str,
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<(), InstanceInstallStageError> {
    let (authority, path) = loopback_http(url).map_err(failed)?;
    let deadline = Instant::now() + timeout;
    loop {
        if cancellation.is_cancelled() {
            return Err(InstanceInstallStageError::Cancelled);
        }
        let last = match http_status(authority, path) {
            Ok(status) if (200..300).contains(&status) => return Ok(()),
            Ok(status) => format!("status {status}"),
            Err(error) => error,
        };
        if Instant::now() >= deadline {
            return Err(failed(format!(
                "probe {url} was not answered within {timeout:?}: {last}"
            )));
        }
        std::thread::sleep(PROBE_INTERVAL);
    }
}

fn loopback_http(url: &str) -> Result<(&str, &str), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("probe URL `{url}` is not http://"))?;
    let (authority, path) = match rest.find('/') {
        Some(index) => rest.split_at(index),
        None => (rest, "/"),
    };
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host);
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return Err(format!(
            "probe URL `{url}` is not on the loopback interface"
        ));
    }
    Ok((authority, path))
}

fn http_status(authority: &str, path: &str) -> Result<u16, String> {
    let address = authority
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| format!("{authority} has no address"))?;
    let mut stream = TcpStream::connect_timeout(&address, PROBE_CONNECT_TIMEOUT)
        .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(PROBE_CONNECT_TIMEOUT))
        .map_err(|error| error.to_string())?;
    // One write: a server that reads the request once sees all of it.
    let request = format!("GET {path} HTTP/1.0\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;
    let mut head = [0_u8; 64];
    let read = stream.read(&mut head).map_err(|error| error.to_string())?;
    let line = String::from_utf8_lossy(&head[..read]);
    line.split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .ok_or_else(|| "the answer is not HTTP".to_string())
}

/// The directory `path` is written into, created, and checked to resolve
/// inside its component: a link in the component cannot carry a write out.
fn contained_parent(path: &ComponentPath) -> Result<PathBuf, InstanceInstallStageError> {
    let target = path.path();
    let parent = target
        .parent()
        .filter(|_| !path.relative.as_os_str().is_empty())
        .ok_or_else(|| failed(format!("{} names no file", target.display())))?;
    fs::create_dir_all(&path.root)
        .and_then(|()| fs::create_dir_all(parent))
        .map_err(|error| failed(format!("create {}: {error}", parent.display())))?;
    let root = resolve(&path.root)?;
    let resolved = resolve(parent)?;
    if !resolved.starts_with(&root) {
        return Err(failed(format!(
            "{} resolves outside the `{}` component",
            target.display(),
            path.component
        )));
    }
    Ok(resolved.system().to_path_buf())
}

/// A directory beside `into` that is removed when the step ends, unless the
/// step moved it into place. `into`, when it exists, must resolve inside its
/// component, or its entries would be moved through a link out of it.
pub(crate) fn staging_dir(
    into: &ComponentPath,
) -> Result<tempfile::TempDir, InstanceInstallStageError> {
    let target = into.path();
    let parent = if into.relative.as_os_str().is_empty() {
        let parent = into
            .root
            .parent()
            .ok_or_else(|| failed(format!("{} has no parent", target.display())))?;
        fs::create_dir_all(parent)
            .map_err(|error| failed(format!("create {}: {error}", parent.display())))?;
        resolve(parent)?.system().to_path_buf()
    } else {
        contained_parent(into)?
    };
    if target.exists() {
        let root = resolve(&into.root)?;
        if !resolve(&target)?.starts_with(&root) {
            return Err(failed(format!(
                "{} resolves outside the `{}` component",
                target.display(),
                into.component
            )));
        }
    }
    tempfile::Builder::new()
        .prefix(".unpack-")
        .tempdir_in(&parent)
        .map_err(|error| failed(format!("stage files beside {}: {error}", target.display())))
}

fn remove_existing(path: &Path) -> Result<(), InstanceInstallStageError> {
    let removed = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    removed.map_err(|error| failed(format!("replace {}: {error}", path.display())))
}

fn resolve(path: &Path) -> Result<ResolvedPath, InstanceInstallStageError> {
    ResolvedPath::resolve(path).map_err(|error| failed(error.to_string()))
}

fn file_name(path: &ComponentPath) -> Result<&std::ffi::OsStr, InstanceInstallStageError> {
    path.relative
        .file_name()
        .ok_or_else(|| failed(format!("{} names no file", path.path().display())))
}

/// The archive's name from its URL, which carries the format extraction
/// reads from it.
fn archive_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && !name.contains(['\\', ':']))
        .unwrap_or("download")
        .to_string()
}

fn stage_error(error: PythonInstallError) -> InstanceInstallStageError {
    match error {
        PythonInstallError::Cancelled => InstanceInstallStageError::Cancelled,
        PythonInstallError::Failed(error) => InstanceInstallStageError::Failed(error),
    }
}

fn failed(message: impl Into<String>) -> InstanceInstallStageError {
    InstanceInstallStageError::Failed(message.into())
}
