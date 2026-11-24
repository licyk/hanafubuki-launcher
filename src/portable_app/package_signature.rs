//! The detached signature of the application package the launcher installs.
//!
//! The release pipeline publishes `<package>.sig`, signed with the updater
//! key, next to each package the launcher can install, so the release catalog
//! needs no field for it: the signature's URL is derived from the package's.
//! The package is verified while it downloads and nothing is extracted from
//! it before the check passes.

use std::io::Read;
use std::time::Duration;

use crate::internal::hanafubuki_app_update::ValidatedCatalogArtifact;
use minisign_verify::StreamVerifier;
use reqwest::{StatusCode, Url};
use semver::Version;

use crate::signature::{ReleaseSignature, UpdaterKey};

use super::release::RemoteError;

/// The first release that publishes a signature for its portable ZIP. Earlier
/// releases signed only the updater kinds (the AppImage and the macOS
/// `.app.tar.gz`, which the launcher installs on Linux and macOS), so a
/// portable ZIP before this version without a signature is checked by size
/// only; from it on, a missing signature refuses the package.
pub(crate) const SIGNED_PORTABLE_ZIP_SINCE: Version = Version::new(0, 3, 29);

const MAX_SIGNATURE_BYTES: usize = 64 * 1024;

pub(crate) fn signature_required(kind: &str, version: &Version) -> bool {
    kind != "portable-zip" || *version >= SIGNED_PORTABLE_ZIP_SINCE
}

/// `<package URL>.sig` in either URL form the catalog allows: the file path
/// in the URL path (Hugging Face, ModelScope `resolve`), or in the
/// `FilePath` query parameter (the ModelScope API).
pub(crate) fn signature_url(artifact: &ValidatedCatalogArtifact) -> Result<String, String> {
    let mut url =
        Url::parse(&artifact.url).map_err(|_| "The package URL is invalid".to_string())?;
    let suffix = format!("/{}", artifact.file_name);
    let file_paths = url
        .query_pairs()
        .filter(|(key, _)| key == "FilePath")
        .map(|(_, value)| value.into_owned())
        .collect::<Vec<_>>();
    match file_paths.as_slice() {
        [] if url.query().is_none() && url.path().ends_with(&suffix) => {
            let path = format!("{}.sig", url.path());
            url.set_path(&path);
        }
        [file_path] if file_path.ends_with(&suffix) => {
            let pairs = url
                .query_pairs()
                .map(|(key, value)| {
                    let value = if key == "FilePath" {
                        format!("{value}.sig")
                    } else {
                        value.into_owned()
                    };
                    (key.into_owned(), value)
                })
                .collect::<Vec<_>>();
            url.query_pairs_mut().clear().extend_pairs(pairs);
        }
        _ => {
            return Err(format!(
                "Cannot derive the signature URL of {}",
                artifact.file_name
            ));
        }
    }
    Ok(url.into())
}

/// The package's signature, or `None` when it is missing and
/// [`signature_required`] allows that. Only an HTTP 404 counts as missing.
pub(crate) fn fetch(
    client: &reqwest::blocking::Client,
    artifact: &ValidatedCatalogArtifact,
    version: &Version,
) -> Result<Option<ReleaseSignature>, RemoteError> {
    let url = signature_url(artifact)?;
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|error| RemoteError::request("Download the package signature", error))?;
    if response.status() == StatusCode::NOT_FOUND {
        return missing(&artifact.kind, version, &url);
    }
    let response = response
        .error_for_status()
        .map_err(|error| RemoteError::Fatal(format!("Download the package signature: {error}")))?;
    let mut bytes = Vec::new();
    response
        .take((MAX_SIGNATURE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| RemoteError::Offline(format!("Read the package signature: {error}")))?;
    if bytes.len() > MAX_SIGNATURE_BYTES {
        return Err(RemoteError::Fatal(
            "The package signature exceeds the size limit".into(),
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| RemoteError::Fatal("The package signature is not UTF-8 text".into()))?;
    ReleaseSignature::decode(&text)
        .map(Some)
        .map_err(RemoteError::Fatal)
}

pub(crate) fn missing(
    kind: &str,
    version: &Version,
    url: &str,
) -> Result<Option<ReleaseSignature>, RemoteError> {
    if signature_required(kind, version) {
        return Err(RemoteError::Fatal(format!(
            "The Hanafubuki {version} package has no published signature: {url}"
        )));
    }
    log::warn!(
        target: "launcher::download",
        "release {version} predates signed {kind} packages; checking the package by size only signature_url={url}"
    );
    Ok(None)
}

/// Checks the package bytes as they are downloaded.
pub(crate) enum PackageCheck<'a> {
    Signed(StreamVerifier<'a>),
    SizeOnly,
}

impl<'a> PackageCheck<'a> {
    pub(crate) fn new(
        key: &'a UpdaterKey,
        signature: Option<&'a ReleaseSignature>,
    ) -> Result<Self, String> {
        match signature {
            Some(signature) => key.verifier(signature).map(Self::Signed),
            None => Ok(Self::SizeOnly),
        }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        if let Self::Signed(verifier) = self {
            verifier.update(bytes);
        }
    }

    /// Whether the package was verified by signature; an error when the
    /// signature does not match the bytes.
    pub(crate) fn finish(self) -> Result<bool, String> {
        match self {
            Self::Signed(mut verifier) => verifier
                .finalize()
                .map(|()| true)
                .map_err(|error| format!("Verify the package signature: {error}")),
            Self::SizeOnly => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests;
