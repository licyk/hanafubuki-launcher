//! Archive extraction confined to the target directory, with cancellation.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::paths::PathKey;

use super::{CANCELLED_MARKER, check_cancelled};

pub(super) const COPY_BUFFER_SIZE: usize = 64 * 1024;

/// How far up a tar's symbolic link may point when it is extracted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpwardLinks {
    /// No link names `..`: its target is below the directory it is in.
    Refused,
    /// A link may climb with leading `..` (only before the names it then
    /// descends into), through directories that are not links, but its
    /// target stays inside the directory its own first `N` path components
    /// name (the target directory itself when 0, or when it is not that
    /// deep): a caller that later moves what is `N` directories deep moves no
    /// link that points out of what it moved.
    Within(usize),
}

/// Extracts `archive_path` into `target_dir`, a tar's symbolic links as
/// `upward_links` allows. A hard link names an entry from the archive's
/// root.
pub(super) fn extract_archive(
    archive_path: &Path,
    target_dir: &Path,
    upward_links: UpwardLinks,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    extract_archive_with_observer(
        archive_path,
        target_dir,
        upward_links,
        cancellation,
        &NoopExtractionObserver,
    )
}

pub(super) trait ArchiveExtractionObserver {
    fn entry_extracted(&self, _path: &Path) {}
}

struct NoopExtractionObserver;

impl ArchiveExtractionObserver for NoopExtractionObserver {}

pub(super) fn extract_archive_with_observer(
    archive_path: &Path,
    target_dir: &Path,
    upward_links: UpwardLinks,
    cancellation: &CancellationToken,
    observer: &dyn ArchiveExtractionObserver,
) -> Result<(), String> {
    check_cancelled(cancellation)?;
    let name = archive_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid archive path: {}", archive_path.display()))?
        .to_ascii_lowercase();

    log::info!(
        "extract archive={} target_dir={}",
        archive_path.display(),
        target_dir.display()
    );

    if name.ends_with(".zip") {
        return extract_zip(archive_path, target_dir, cancellation, observer);
    }
    if name.ends_with(".tar") {
        let file = File::open(archive_path)
            .map_err(|err| format!("open archive {}: {err}", archive_path.display()))?;
        return extract_tar(file, target_dir, upward_links, cancellation, observer);
    }
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let file = File::open(archive_path)
            .map_err(|err| format!("open archive {}: {err}", archive_path.display()))?;
        let decoder = flate2::read::GzDecoder::new(file);
        return extract_tar(decoder, target_dir, upward_links, cancellation, observer);
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        let file = File::open(archive_path)
            .map_err(|err| format!("open archive {}: {err}", archive_path.display()))?;
        let decoder = xz2::read::XzDecoder::new(file);
        return extract_tar(decoder, target_dir, upward_links, cancellation, observer);
    }
    if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        let file = File::open(archive_path)
            .map_err(|err| format!("open archive {}: {err}", archive_path.display()))?;
        let decoder = bzip2::read::BzDecoder::new(file);
        return extract_tar(decoder, target_dir, upward_links, cancellation, observer);
    }

    Err(format!(
        "unsupported archive format: {}",
        archive_path.display()
    ))
}

pub(super) fn strip_single_nested_dir(
    target_dir: &Path,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    check_cancelled(cancellation)?;
    let entries = fs::read_dir(target_dir)
        .map_err(|err| format!("read target dir {}: {err}", target_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("read target dir entry {}: {err}", target_dir.display()))?;

    if entries.len() != 1 {
        return Ok(());
    }

    let nested_dir = entries[0].path();
    if !nested_dir.is_dir() {
        return Ok(());
    }

    log::info!(
        "strip standalone python nested dir nested_dir={} target_dir={}",
        nested_dir.display(),
        target_dir.display()
    );

    let nested_entries = fs::read_dir(&nested_dir)
        .map_err(|err| format!("read nested dir {}: {err}", nested_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("read nested dir entry {}: {err}", nested_dir.display()))?;

    for entry in nested_entries {
        check_cancelled(cancellation)?;
        let from = entry.path();
        let to = target_dir.join(entry.file_name());
        fs::rename(&from, &to)
            .map_err(|err| format!("move {} -> {}: {err}", from.display(), to.display()))?;
    }

    fs::remove_dir(&nested_dir)
        .map_err(|err| format!("remove nested dir {}: {err}", nested_dir.display()))?;
    Ok(())
}

fn extract_zip(
    archive_path: &Path,
    target_dir: &Path,
    cancellation: &CancellationToken,
    observer: &dyn ArchiveExtractionObserver,
) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|err| format!("open {}: {err}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| format!("read zip {}: {err}", archive_path.display()))?;

    for index in 0..archive.len() {
        check_cancelled(cancellation)?;
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("read zip entry #{index}: {err}"))?;
        let entry_name = entry
            .name()
            .map_err(|err| format!("read zip entry #{index} name: {err}"))?;
        let enclosed_name = entry
            .enclosed_name()
            .ok_or_else(|| format!("unsafe zip entry path: {entry_name}"))?;
        let output_path = target_dir.join(enclosed_name);
        ensure_inside(target_dir, &output_path)?;

        if entry.is_dir() {
            fs::create_dir_all(&output_path)
                .map_err(|err| format!("create dir {}: {err}", output_path.display()))?;
            observer.entry_extracted(&output_path);
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("create dir {}: {err}", parent.display()))?;
        }
        let mut output = File::create(&output_path)
            .map_err(|err| format!("create file {}: {err}", output_path.display()))?;
        copy_with_cancellation(&mut entry, &mut output, cancellation)
            .map_err(|err| format_copy_error(&output_path, err))?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output_path, fs::Permissions::from_mode(mode))
                .map_err(|err| format!("set permissions {}: {err}", output_path.display()))?;
        }
        observer.entry_extracted(&output_path);
    }
    Ok(())
}

fn extract_tar<R: io::Read>(
    reader: R,
    target_dir: &Path,
    upward_links: UpwardLinks,
    cancellation: &CancellationToken,
    observer: &dyn ArchiveExtractionObserver,
) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|err| format!("read tar entries: {err}"))?;

    for entry in entries {
        check_cancelled(cancellation)?;
        let mut entry = entry.map_err(|err| format!("read tar entry: {err}"))?;
        let path = entry
            .path()
            .map_err(|err| format!("read tar path: {err}"))?;
        let safe_path = sanitize_relative_path(&path)?;
        let output_path = target_dir.join(&safe_path);
        ensure_inside(target_dir, &output_path)?;
        validate_tar_link_target(&entry, target_dir, &safe_path, upward_links)?;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("create dir {}: {err}", parent.display()))?;
        }
        if entry.header().entry_type().is_file() {
            let mut output = File::create(&output_path)
                .map_err(|err| format!("create file {}: {err}", output_path.display()))?;
            copy_with_cancellation(&mut entry, &mut output, cancellation)
                .map_err(|err| format_copy_error(&output_path, err))?;
            #[cfg(unix)]
            if let Ok(mode) = entry.header().mode() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&output_path, fs::Permissions::from_mode(mode))
                    .map_err(|err| format!("set permissions {}: {err}", output_path.display()))?;
            }
        } else {
            entry
                .unpack(&output_path)
                .map_err(|err| format!("extract tar entry {}: {err}", output_path.display()))?;
        }
        observer.entry_extracted(&output_path);
    }
    Ok(())
}

fn validate_tar_link_target<R: io::Read>(
    entry: &tar::Entry<'_, R>,
    target_dir: &Path,
    entry_path: &Path,
    upward_links: UpwardLinks,
) -> Result<(), String> {
    let entry_type = entry.header().entry_type();
    if !entry_type.is_symlink() && !entry_type.is_hard_link() {
        return Ok(());
    }
    let output_path = target_dir.join(entry_path);
    let link_name = entry
        .link_name()
        .map_err(|error| format!("read tar link target: {error}"))?
        .ok_or_else(|| format!("tar link {} has no target", output_path.display()))?;
    let unsafe_target = || {
        format!(
            "unsafe tar link target {} for {}",
            link_name.display(),
            output_path.display()
        )
    };
    if entry_type.is_hard_link() {
        let safe_target = sanitize_relative_path(&link_name).map_err(|_| unsafe_target())?;
        return ensure_inside(target_dir, &target_dir.join(safe_target));
    }
    let directory = entry_path.parent().unwrap_or(Path::new(""));
    let depth = directory.components().count();
    let (climbs, names) = climbing_link_target(&link_name).ok_or_else(unsafe_target)?;
    // `..` is resolved by name, which is where it leads only through real
    // directories: an earlier entry may have made one of them a link.
    let may_climb = match upward_links {
        UpwardLinks::Refused => 0,
        UpwardLinks::Within(floor) => depth.saturating_sub(floor),
    };
    if climbs > may_climb || (climbs > 0 && !all_real_directories(target_dir, directory)) {
        return Err(unsafe_target());
    }
    let kept: PathBuf = directory.components().take(depth - climbs).collect();
    ensure_inside(target_dir, &target_dir.join(kept).join(names))
}

/// A link target's leading `..` count and the names after them; `None`
/// for an absolute target, a `..` after a name, or no name at all.
fn climbing_link_target(target: &Path) -> Option<(usize, PathBuf)> {
    let (mut climbs, mut names) = (0, PathBuf::new());
    for component in target.components() {
        match component {
            Component::ParentDir if names.as_os_str().is_empty() => climbs += 1,
            Component::Normal(name) => names.push(name),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!names.as_os_str().is_empty()).then_some((climbs, names))
}

/// Whether every directory from `target_dir` down to `directory` (relative)
/// that exists is a directory and not a link; a missing one is created as a
/// directory.
fn all_real_directories(target_dir: &Path, directory: &Path) -> bool {
    let mut path = target_dir.to_path_buf();
    directory.components().all(|component| {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata.is_dir(),
            Err(error) => error.kind() == io::ErrorKind::NotFound,
        }
    })
}

pub(super) fn copy_with_cancellation<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    cancellation: &CancellationToken,
) -> io::Result<u64> {
    let mut buffer = [0_u8; COPY_BUFFER_SIZE];
    let mut written = 0_u64;
    loop {
        if cancellation.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, CANCELLED_MARKER));
        }
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(written);
        }
        writer.write_all(&buffer[..count])?;
        written += count as u64;
    }
}

pub(super) fn format_copy_error(path: &Path, error: io::Error) -> String {
    if error.kind() == io::ErrorKind::Interrupted && error.to_string() == CANCELLED_MARKER {
        CANCELLED_MARKER.to_string()
    } else {
        format!("write {}: {error}", path.display())
    }
}

pub(super) fn sanitize_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => sanitized.push(value),
            Component::CurDir => {}
            _ => return Err(format!("unsafe archive path: {}", path.display())),
        }
    }
    if sanitized.as_os_str().is_empty() {
        return Err("empty archive path".to_string());
    }
    Ok(sanitized)
}

/// Reject an archive entry that would land outside the extraction root.
///
/// Compared on path identity: on Windows `C:\Target` and `c:\target` are the
/// same directory, and a byte-wise prefix test would let a case-shifted entry
/// look like an escape — or, worse, let a case-shifted base look like a match.
fn ensure_inside(base: &Path, target: &Path) -> Result<(), String> {
    if PathKey::of(target).starts_with(&PathKey::of(base)) {
        Ok(())
    } else {
        Err(format!(
            "archive entry escaped target dir: {}",
            target.display()
        ))
    }
}
