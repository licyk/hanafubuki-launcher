//! The verified download and the confined extraction, for the application's
//! other installers (a declarative instance type's steps) as for Python's.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;

use super::download::{client_with_timeout, download_client_with_proxy, download_file};
use super::extract::{UpwardLinks, extract_archive};
use super::{PythonInstallError, classify_install_error};

/// Downloads to `destination` from the first of `urls` whose bytes have the
/// SHA-256 `sha256` (lowercase hex), through `proxy` when one is given, and
/// returns that URL. The bytes are written beside the destination and moved
/// onto it only once they match, so a mismatch or a failure leaves an
/// existing file as it was.
pub fn download_verified_file(
    urls: &[String],
    sha256: &str,
    destination: &Path,
    proxy: Option<&str>,
    cancellation: &CancellationToken,
) -> Result<String, PythonInstallError> {
    let client = download_client_with_proxy(proxy).map_err(PythonInstallError::Failed)?;
    let mut errors = Vec::new();
    for url in urls {
        match download_file(&client, url, destination, Some(sha256), cancellation) {
            Ok(_) => return Ok(url.clone()),
            Err(error) => match classify_install_error(error) {
                PythonInstallError::Cancelled => return Err(PythonInstallError::Cancelled),
                PythonInstallError::Failed(error) => errors.push(format!("{url}: {error}")),
            },
        }
    }
    Err(PythonInstallError::Failed(format!(
        "download {} failed: {}",
        destination.display(),
        errors.join("; ")
    )))
}

/// Downloads `url` to `destination` through `proxy` when one is given and
/// returns the SHA-256 (lowercase hex) of its bytes; with `expected`, bytes
/// of another digest are refused as [`download_verified_file`] refuses
/// them. Bytes are moved onto the destination only once they are all there.
pub fn download_with_digest(
    url: &str,
    expected: Option<&str>,
    destination: &Path,
    proxy: Option<&str>,
    cancellation: &CancellationToken,
) -> Result<String, PythonInstallError> {
    let client = download_client_with_proxy(proxy).map_err(PythonInstallError::Failed)?;
    download_file(&client, url, destination, expected, cancellation).map_err(classify_install_error)
}

/// The body of `url`, read through `proxy` when one is given, within
/// `timeout` for the whole request; a body longer than `limit` bytes is
/// refused, whatever length the response announced.
pub fn fetch_bounded(
    url: &str,
    limit: u64,
    timeout: Duration,
    proxy: Option<&str>,
) -> Result<Vec<u8>, String> {
    let client = client_with_timeout(proxy, timeout)?;
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("send request: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("http status {status}"));
    }
    let mut body = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|error| format!("read the response: {error}"))?;
    if body.len() as u64 > limit {
        return Err(format!("the response is larger than {limit} bytes"));
    }
    Ok(body)
}

/// Extracts a `.zip`, `.tar`, `.tar.gz`, `.tar.xz` or `.tar.bz2` archive
/// into `target`, refusing an entry or a link that would land outside it,
/// and a symbolic link that points up further than `upward_links` allows.
pub fn extract_archive_confined(
    archive: &Path,
    target: &Path,
    upward_links: UpwardLinks,
    cancellation: &CancellationToken,
) -> Result<(), PythonInstallError> {
    extract_archive(archive, target, upward_links, cancellation).map_err(classify_install_error)
}
