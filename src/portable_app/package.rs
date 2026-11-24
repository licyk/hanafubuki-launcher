use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::time::{Duration, Instant};

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_foundation::user_agent::BROWSER_USER_AGENT;

use crate::network::NetworkContext;
use crate::platform::{self, TargetSpec};
use crate::signature::UpdaterKey;
use crate::workspace::paths::WorkspacePaths;

use super::package_signature::{self, PackageCheck};
use super::release::{Release, RemoteError};
use super::{DownloadProgress, FileProgress};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageProgress {
    Download(DownloadProgress),
    Materializing,
    File(FileProgress),
}

pub fn prepare(
    network: &NetworkContext,
    paths: &WorkspacePaths,
    target: &TargetSpec,
    release: &Release,
    mut report: impl FnMut(PackageProgress),
) -> Result<ResolvedPath, RemoteError> {
    log::info!(
        target: "launcher::core",
        "preparing trusted core staging={}",
        paths.staging.native_text()
    );
    clean(&paths.staging)?;
    fs::create_dir_all(paths.staging.system())
        .map_err(|error| format!("Create staging directory: {error}"))?;
    let staging = platform::require_existing(&paths.staging)?;
    let package = child(&staging, "package.tmp")?;
    let staged_core = child(&staging, "core")?;
    fs::create_dir_all(staged_core.system())
        .map_err(|error| format!("Create staged core: {error}"))?;
    let staged_core = platform::require_existing(&staged_core)?;

    download(network, release, &package, |progress| {
        report(PackageProgress::Download(progress));
    })?;
    report(PackageProgress::Materializing);
    materialize(target, &package, &staged_core, |progress| {
        report(PackageProgress::File(progress));
    })?;
    if platform::installed_executable(&staged_core, target)?.is_none() {
        return Err(RemoteError::Fatal(
            "The downloaded package does not contain Hanafubuki.".into(),
        ));
    }
    log::info!(
        target: "launcher::core",
        "trusted core materialized root={}",
        staged_core.native_text()
    );
    Ok(staged_core)
}

pub fn clean(path: &ResolvedPath) -> Result<(), String> {
    match fs::symlink_metadata(path.system()) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            log::info!(target: "launcher::filesystem", "removing directory path={}", path.native_text());
            fs::remove_dir_all(path.system())
                .map_err(|error| format!("Remove {}: {error}", path.native_text()))
        }
        Ok(_) => {
            log::info!(target: "launcher::filesystem", "removing file path={}", path.native_text());
            fs::remove_file(path.system())
                .map_err(|error| format!("Remove {}: {error}", path.native_text()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            log::debug!(target: "launcher::filesystem", "remove skipped missing path={}", path.native_text());
            Ok(())
        }
        Err(error) => Err(format!("Inspect {}: {error}", path.native_text())),
    }
}

fn download(
    network: &NetworkContext,
    release: &Release,
    destination: &ResolvedPath,
    mut report: impl FnMut(DownloadProgress),
) -> Result<(), RemoteError> {
    log::info!(
        target: "launcher::download",
        "download started version={} expected_bytes={} destination={}",
        release.version,
        release.artifact.size,
        destination.native_text()
    );
    let client = network.client(BROWSER_USER_AGENT)?;
    let key = UpdaterKey::embedded()?;
    let signature = package_signature::fetch(&client, &release.artifact, &release.version)?;
    let mut check = PackageCheck::new(&key, signature.as_ref())?;
    let mut response = client
        .get(&release.artifact.url)
        .send()
        .map_err(|error| RemoteError::request("Download Hanafubuki", error))?
        .error_for_status()
        .map_err(|error| RemoteError::Fatal(format!("Download Hanafubuki: {error}")))?;
    if response
        .content_length()
        .is_some_and(|length| length != release.artifact.size)
    {
        return Err(RemoteError::Fatal(
            "The download size does not match the release catalog.".into(),
        ));
    }

    let mut output = File::create(destination.system())
        .map_err(|error| format!("Create package download: {error}"))?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut completed = 0_u64;
    let mut last_progress_emit = None;
    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|error| RemoteError::Offline(format!("Download Hanafubuki: {error}")))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| format!("Write package download: {error}"))?;
        check.update(&buffer[..count]);
        completed = completed.saturating_add(count as u64);
        let now = Instant::now();
        if completed == release.artifact.size
            || last_progress_emit
                .is_none_or(|last: Instant| now.duration_since(last) >= Duration::from_millis(100))
        {
            report(DownloadProgress {
                version: release.version.to_string(),
                completed,
                total: Some(release.artifact.size),
            });
            last_progress_emit = Some(now);
        }
    }
    output
        .sync_all()
        .map_err(|error| format!("Flush package download: {error}"))?;
    if completed != release.artifact.size {
        return Err(RemoteError::Offline(
            "The Hanafubuki download ended before it was complete.".into(),
        ));
    }
    let signed = match check.finish() {
        Ok(signed) => signed,
        Err(error) => {
            drop(output);
            // Bytes that failed the check are never left for extraction.
            let _ = fs::remove_file(destination.system());
            return Err(RemoteError::Fatal(format!(
                "The downloaded Hanafubuki package was rejected: {error}"
            )));
        }
    };
    log::info!(
        target: "launcher::download",
        "download completed version={} bytes={completed} signature_verified={signed} destination={}",
        release.version,
        destination.native_text()
    );
    Ok(())
}

fn materialize(
    target: &TargetSpec,
    package: &ResolvedPath,
    staged_core: &ResolvedPath,
    mut report: impl FnMut(FileProgress),
) -> Result<(), String> {
    log::info!(
        target: "launcher::extract",
        "materialization started package_kind={} package={} destination={}",
        target.package_kind,
        package.native_text(),
        staged_core.native_text()
    );
    match target.package_kind {
        "portable-zip" => unpack_zip(package, staged_core, report),
        "app-tar-gz" => unpack_tar_gz(package, staged_core, report),
        "appimage" => {
            let destination = child(staged_core, "Hanafubuki.AppImage")?;
            let bytes = fs::copy(package.system(), destination.system())
                .map_err(|error| format!("Install AppImage: {error}"))?;
            log::debug!(
                target: "launcher::extract",
                "file installed path=Hanafubuki.AppImage bytes={bytes}"
            );
            log::info!(
                target: "launcher::extract",
                "materialization completed files=1 directories=0 bytes={bytes}"
            );
            report(FileProgress {
                completed: 1,
                total: Some(1),
                current_path: "Hanafubuki.AppImage".into(),
            });
            Ok(())
        }
        kind => Err(format!("Unsupported portable package kind: {kind}")),
    }
}

fn unpack_zip(
    package: &ResolvedPath,
    root: &ResolvedPath,
    mut report: impl FnMut(FileProgress),
) -> Result<(), String> {
    let file = File::open(package.system()).map_err(|error| format!("Open ZIP: {error}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| format!("Read ZIP: {error}"))?;
    let mut file_count = 0_u64;
    let mut directory_count = 0_u64;
    let mut total_bytes = 0_u64;
    let entry_total = archive.len() as u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Read ZIP entry: {error}"))?;
        let relative = entry
            .enclosed_name()
            .ok_or("ZIP entry escapes the portable application directory")?
            .to_path_buf();
        let current_path = if relative.as_os_str().is_empty() {
            ".".into()
        } else {
            relative.to_string_lossy().into_owned()
        };
        if relative.as_os_str().is_empty() {
            report_file(&mut report, index, entry_total, current_path);
            continue;
        }
        if entry.is_dir() {
            ensure_relative_directory(root, &relative)?;
            directory_count += 1;
            log::debug!(target: "launcher::extract", "directory extracted path={current_path}");
            report_file(&mut report, index, entry_total, current_path);
            continue;
        }
        let destination = relative_file(root, &relative)?;
        let mut output = File::create(destination.system())
            .map_err(|error| format!("Create extracted file: {error}"))?;
        let bytes = std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("Extract ZIP entry: {error}"))?;
        file_count += 1;
        total_bytes = total_bytes.saturating_add(bytes);
        log::debug!(target: "launcher::extract", "file extracted path={current_path} bytes={bytes}");
        report_file(&mut report, index, entry_total, current_path);
    }
    log::info!(
        target: "launcher::extract",
        "materialization completed files={file_count} directories={directory_count} bytes={total_bytes}"
    );
    Ok(())
}

fn report_file(
    report: &mut impl FnMut(FileProgress),
    index: usize,
    total: u64,
    current_path: String,
) {
    report(FileProgress {
        completed: (index + 1) as u64,
        total: Some(total),
        current_path,
    });
}

fn unpack_tar_gz(
    package: &ResolvedPath,
    root: &ResolvedPath,
    mut report: impl FnMut(FileProgress),
) -> Result<(), String> {
    let file =
        File::open(package.system()).map_err(|error| format!("Open macOS archive: {error}"))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| format!("Read macOS archive: {error}"))?;
    let mut file_count = 0_u64;
    let mut directory_count = 0_u64;
    let mut total_bytes = 0_u64;
    let mut completed = 0_u64;
    for entry in entries {
        let mut entry = entry.map_err(|error| format!("Read macOS archive entry: {error}"))?;
        let relative = entry
            .path()
            .map_err(|error| format!("Read macOS archive entry path: {error}"))?
            .into_owned();
        let entry_type = entry.header().entry_type();
        if !entry
            .unpack_in(root.system())
            .map_err(|error| format!("Extract macOS archive entry: {error}"))?
        {
            return Err("macOS archive entry escapes the portable application directory".into());
        }
        if entry_type.is_dir() {
            directory_count += 1;
            log::debug!(
                target: "launcher::extract",
                "directory extracted path={}",
                relative.to_string_lossy()
            );
        } else if entry_type.is_file() {
            let bytes = entry.header().size().unwrap_or(0);
            file_count += 1;
            total_bytes = total_bytes.saturating_add(bytes);
            log::debug!(
                target: "launcher::extract",
                "file extracted path={} bytes={bytes}",
                relative.to_string_lossy()
            );
        } else {
            log::debug!(
                target: "launcher::extract",
                "archive entry extracted path={} type={entry_type:?}",
                relative.to_string_lossy()
            );
        }
        completed += 1;
        report(FileProgress {
            completed,
            total: None,
            current_path: relative.to_string_lossy().into_owned(),
        });
    }
    log::info!(
        target: "launcher::extract",
        "materialization completed files={file_count} directories={directory_count} bytes={total_bytes}"
    );
    Ok(())
}

fn ensure_relative_directory(root: &ResolvedPath, relative: &Path) -> Result<ResolvedPath, String> {
    let mut current = root.clone();
    for component in ordinary_components(relative)? {
        let next =
            ResolvedPath::resolve_child(&current, component).map_err(|error| error.to_string())?;
        fs::create_dir_all(next.system())
            .map_err(|error| format!("Create extracted directory: {error}"))?;
        current = platform::require_existing(&next)?;
    }
    Ok(current)
}

fn relative_file(root: &ResolvedPath, relative: &Path) -> Result<ResolvedPath, String> {
    let mut components = ordinary_components(relative)?;
    let file_name = components
        .pop()
        .ok_or("ZIP entry does not contain a file name")?;
    let mut parent = root.clone();
    for component in components {
        let next =
            ResolvedPath::resolve_child(&parent, component).map_err(|error| error.to_string())?;
        fs::create_dir_all(next.system())
            .map_err(|error| format!("Create extracted directory: {error}"))?;
        parent = platform::require_existing(&next)?;
    }
    ResolvedPath::resolve_child(&parent, file_name).map_err(|error| error.to_string())
}

fn ordinary_components(path: &Path) -> Result<Vec<&OsStr>, String> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value),
            _ => Err("Archive entry contains an invalid path".into()),
        })
        .collect()
}

fn child(parent: &ResolvedPath, name: &str) -> Result<ResolvedPath, String> {
    ResolvedPath::resolve_child(parent, OsStr::new(name)).map_err(|error| error.to_string())
}
