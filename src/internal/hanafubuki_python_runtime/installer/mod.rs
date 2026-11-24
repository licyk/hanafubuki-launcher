mod catalog;
mod download;
mod extract;
mod verified;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use serde::{Deserialize, Serialize};
use tempfile::Builder;

use crate::internal::hanafubuki_python_runtime::version::PythonVersion;
use catalog::{current_arch, current_platform, find_download};
use download::{download_client, download_with_fallback};
pub use extract::UpwardLinks;
use extract::{extract_archive, strip_single_nested_dir};
pub use verified::{
    download_verified_file, download_with_digest, extract_archive_confined, fetch_bounded,
};

const CANCELLED_MARKER: &str = "standalone Python installation cancelled";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PythonInstallError {
    Cancelled,
    Failed(String),
}

impl fmt::Display for PythonInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str(CANCELLED_MARKER),
            Self::Failed(error) => formatter.write_str(error),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStandalonePythonRequest {
    pub version: PythonVersion,
    pub target_dir: PathBuf,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStandalonePythonResult {
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub archive_name: String,
    pub target_dir: PathBuf,
    pub source_url: String,
}

#[derive(Debug, Clone, Copy)]
struct PythonDownload {
    name: &'static str,
    version: &'static str,
    platform: &'static str,
    arch: &'static str,
    /// Lowercase hex SHA-256 of the archive, the same for every URL.
    sha256: &'static str,
    urls: &'static [&'static str],
}

pub fn install_standalone_python_inner(
    request: InstallStandalonePythonRequest,
) -> Result<InstallStandalonePythonResult, String> {
    install_standalone_python_cancellable(request, &CancellationToken::default())
        .map_err(|error| error.to_string())
}

pub fn install_standalone_python_cancellable(
    request: InstallStandalonePythonRequest,
    cancellation: &CancellationToken,
) -> Result<InstallStandalonePythonResult, PythonInstallError> {
    install_standalone_python_impl(request, cancellation).map_err(classify_install_error)
}

fn classify_install_error(error: String) -> PythonInstallError {
    if error == CANCELLED_MARKER {
        PythonInstallError::Cancelled
    } else {
        PythonInstallError::Failed(error)
    }
}

fn install_standalone_python_impl(
    request: InstallStandalonePythonRequest,
    cancellation: &CancellationToken,
) -> Result<InstallStandalonePythonResult, String> {
    check_cancelled(cancellation)?;
    let platform = current_platform()?;
    let arch = current_arch()?;
    let source = find_download(request.version, platform, arch)?;
    let client = download_client()?;
    install_from_source(request, source, &client, cancellation)
}

/// Downloads `source`, checks it against its pinned digest, and only then
/// replaces the target: an archive that fails the check leaves an existing
/// installation, or the empty target, as it was.
fn install_from_source(
    request: InstallStandalonePythonRequest,
    source: PythonDownload,
    client: &reqwest::blocking::Client,
    cancellation: &CancellationToken,
) -> Result<InstallStandalonePythonResult, String> {
    let overwrite = request.overwrite.unwrap_or(false);
    check_target_dir(&request.target_dir, overwrite)?;
    let download_workspace = Builder::new()
        .prefix("hanafubuki-python-download-")
        .tempdir()
        .map_err(|error| format!("create Python download temporary directory: {error}"))?;
    let archive_path = download_workspace.path().join(source.name);

    log::info!(
        "install standalone python selected version={} platform={} arch={} archive={} target_dir={}",
        source.version,
        source.platform,
        source.arch,
        source.name,
        request.target_dir.display()
    );

    check_cancelled(cancellation)?;
    let source_url = download_with_fallback(client, source, &archive_path, cancellation)?;
    check_cancelled(cancellation)?;
    prepare_target_dir(&request.target_dir, overwrite)?;
    extract_archive(
        &archive_path,
        &request.target_dir,
        UpwardLinks::Refused,
        cancellation,
    )?;
    strip_single_nested_dir(&request.target_dir, cancellation)?;

    log::info!(
        "install standalone python completed version={} archive={} target_dir={}",
        source.version,
        archive_path.display(),
        request.target_dir.display()
    );

    Ok(InstallStandalonePythonResult {
        version: source.version.to_string(),
        platform: source.platform.to_string(),
        arch: source.arch.to_string(),
        archive_name: source.name.to_string(),
        target_dir: request.target_dir,
        source_url,
    })
}

/// The non-destructive half of [`prepare_target_dir`], checked before the
/// download so a refused target costs no transfer.
fn check_target_dir(target_dir: &Path, overwrite: bool) -> Result<(), String> {
    if overwrite || !target_dir.exists() {
        return Ok(());
    }
    if target_dir
        .read_dir()
        .map_err(|err| format!("read target dir {}: {err}", target_dir.display()))?
        .next()
        .is_some()
    {
        return Err(format!(
            "target dir is not empty: {}. Pass overwrite=true to replace it.",
            target_dir.display()
        ));
    }
    Ok(())
}

fn prepare_target_dir(target_dir: &Path, overwrite: bool) -> Result<(), String> {
    check_target_dir(target_dir, overwrite)?;
    if overwrite && target_dir.exists() {
        log::warn!(
            "install standalone python removing existing target_dir={}",
            target_dir.display()
        );
        fs::remove_dir_all(target_dir)
            .map_err(|err| format!("remove target dir {}: {err}", target_dir.display()))?;
    }

    fs::create_dir_all(target_dir)
        .map_err(|err| format!("create target dir {}: {err}", target_dir.display()))
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), String> {
    if cancellation.is_cancelled() {
        Err(CANCELLED_MARKER.to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
