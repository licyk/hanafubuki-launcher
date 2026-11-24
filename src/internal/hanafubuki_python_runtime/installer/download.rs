//! Archive download with mirror fallback, checked against the pinned digest.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::user_agent::BROWSER_USER_AGENT;
use reqwest::blocking::Client;
use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION, HeaderMap, HeaderValue, PRAGMA, USER_AGENT,
};
use sha2::{Digest, Sha256};

use super::extract::{copy_with_cancellation, format_copy_error};
use super::{CANCELLED_MARKER, PythonDownload, check_cancelled};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) fn download_client() -> Result<Client, String> {
    download_client_with_proxy(None)
}

/// The client every download uses, through `proxy` when one is given.
pub(super) fn download_client_with_proxy(proxy: Option<&str>) -> Result<Client, String> {
    client_with_timeout(proxy, DOWNLOAD_TIMEOUT)
}

/// The download client with another bound on a whole request.
pub(super) fn client_with_timeout(
    proxy: Option<&str>,
    timeout: Duration,
) -> Result<Client, String> {
    let mut builder = Client::builder()
        .timeout(timeout)
        .default_headers(download_headers());
    if let Some(proxy) = proxy {
        builder = builder.proxy(
            reqwest::Proxy::all(proxy).map_err(|err| format!("configure download proxy: {err}"))?,
        );
    }
    builder
        .build()
        .map_err(|err| format!("create http client: {err}"))
}

/// Downloads the archive from the first mirror whose bytes match
/// `source.sha256`; a mirror that serves other bytes counts as failed and the
/// next one is tried. `archive_path` exists only once a download matched.
pub(super) fn download_with_fallback(
    client: &Client,
    source: PythonDownload,
    archive_path: &Path,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let mut errors = Vec::new();
    for url in source.urls {
        check_cancelled(cancellation)?;
        match download_file(client, url, archive_path, Some(source.sha256), cancellation) {
            Ok(_) => return Ok((*url).to_string()),
            Err(err) => {
                if err == CANCELLED_MARKER {
                    return Err(err);
                }
                log::warn!(
                    "standalone python download failed url={} archive={} error={}",
                    url,
                    archive_path.display(),
                    err
                );
                errors.push(format!("{url}: {err}"));
            }
        }
    }

    Err(format!(
        "download standalone python archive {} failed: {}",
        source.name,
        errors.join("; ")
    ))
}

fn download_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(BROWSER_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
    headers
}

pub(super) fn download_file(
    client: &Client,
    url: &str,
    archive_path: &Path,
    expected_sha256: Option<&str>,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    check_cancelled(cancellation)?;
    let parent = archive_path
        .parent()
        .ok_or_else(|| format!("archive path has no parent: {}", archive_path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("create cache dir {}: {err}", parent.display()))?;

    let temp_path = archive_path.with_extension("download.tmp");
    if temp_path.exists() {
        fs::remove_file(&temp_path)
            .map_err(|err| format!("remove stale temp file {}: {err}", temp_path.display()))?;
    }

    log::info!(
        "download standalone python url={} target={}",
        url,
        archive_path.display()
    );
    let mut response = client
        .get(url)
        .send()
        .map_err(|err| format!("send request: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("http status {status}"));
    }

    let file =
        File::create(&temp_path).map_err(|err| format!("create {}: {err}", temp_path.display()))?;
    let mut output = HashingWriter {
        inner: file,
        hasher: Sha256::new(),
    };
    copy_with_cancellation(&mut response, &mut output, cancellation)
        .map_err(|err| format_copy_error(&temp_path, err))?;
    output
        .flush()
        .map_err(|err| format!("flush {}: {err}", temp_path.display()))?;
    let HashingWriter { inner, hasher } = output;
    drop(inner);

    let actual_sha256 = hex::encode(hasher.finalize());
    if let Some(expected_sha256) = expected_sha256.filter(|expected| *expected != actual_sha256) {
        // Never leave bytes that failed the check where a later step could
        // pick them up.
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "SHA-256 mismatch: expected {expected_sha256}, downloaded {actual_sha256}; the archive was discarded"
        ));
    }

    fs::rename(&temp_path, archive_path).map_err(|err| {
        format!(
            "rename {} -> {}: {err}",
            temp_path.display(),
            archive_path.display()
        )
    })?;
    Ok(actual_sha256)
}

/// Hashes exactly the bytes written to the download file.
struct HashingWriter {
    inner: File,
    hasher: Sha256,
}

impl Write for HashingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
