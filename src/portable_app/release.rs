use std::io::Read;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::internal::hanafubuki_app_update::{
    AppArchitecture, AppPlatform, MAX_CATALOG_BYTES, UpdateProvider, UpdateSource,
    ValidatedCatalog, ValidatedCatalogArtifact, decode_catalog, hanafubuki_update_sources,
    reconcile_catalogs,
};
use crate::internal::hanafubuki_foundation::user_agent::BROWSER_USER_AGENT;
use semver::Version;

use crate::network::{self, NetworkContext};
use crate::platform::TargetSpec;

const PRODUCT_NAME: &str = "Hanafubuki";
const PRODUCT_IDENTIFIER: &str = "org.licyk.hanafubuki";

#[derive(Debug)]
pub enum RemoteError {
    Offline(String),
    Fatal(String),
}

impl RemoteError {
    pub(crate) fn request(context: &str, error: reqwest::Error) -> Self {
        let message = format!("{context}: {error}");
        if error.is_connect() || error.is_timeout() {
            Self::Offline(message)
        } else {
            Self::Fatal(message)
        }
    }
}

impl From<String> for RemoteError {
    fn from(error: String) -> Self {
        Self::Fatal(error)
    }
}

pub struct Release {
    pub version: Version,
    pub(crate) artifact: ValidatedCatalogArtifact,
}

/// The latest-release lookup, started early so it runs while the launcher
/// checks for its own update and prepares the workspace.
pub struct ReleaseLookup(Result<JoinHandle<Result<Release, RemoteError>>, String>);

impl ReleaseLookup {
    pub fn start(network: NetworkContext, target: TargetSpec) -> Self {
        let handle = thread::Builder::new()
            .name("launcher-release".into())
            .spawn(move || latest(&network, &target))
            .map_err(|error| format!("Start release lookup: {error}"));
        Self(handle)
    }

    pub fn wait(self) -> Result<Release, RemoteError> {
        self.0
            .map_err(RemoteError::Fatal)?
            .join()
            .unwrap_or_else(|_| {
                Err(RemoteError::Fatal(
                    "Release lookup stopped unexpectedly".into(),
                ))
            })
    }
}

fn latest(network: &NetworkContext, target: &TargetSpec) -> Result<Release, RemoteError> {
    let client = network.client(BROWSER_USER_AGENT)?;
    let mut modelscope = None;
    let mut huggingface = None;
    let mut offline_diagnostics = Vec::new();
    let mut fatal_diagnostics = Vec::new();

    let sources = hanafubuki_update_sources().to_vec();
    let (platform, architecture) = (target.platform, target.architecture);
    let results = network::fetch_sources(sources.clone(), move |source: UpdateSource| {
        log::debug!(
            target: "launcher::release",
            "requesting release catalog provider={:?}",
            source.provider
        );
        fetch_catalog(&client, &source, platform, architecture)
    });
    for (source, result) in sources.into_iter().zip(results) {
        let Some(result) = result else {
            let error = format!(
                "no answer within {} s of another source",
                network::SOURCE_GRACE.as_secs()
            );
            log::warn!(
                target: "launcher::release",
                "release catalog skipped provider={:?}: {error}",
                source.provider
            );
            offline_diagnostics.push(format!("{:?}: {error}", source.provider));
            continue;
        };
        match result {
            Ok(catalog) => {
                log::info!(
                    target: "launcher::release",
                    "release catalog accepted provider={:?} versions={}",
                    source.provider,
                    catalog.versions.len()
                );
                match source.provider {
                    UpdateProvider::Modelscope => modelscope = Some(catalog),
                    UpdateProvider::Huggingface => huggingface = Some(catalog),
                }
            }
            Err(RemoteError::Offline(error)) => {
                log::warn!(
                    target: "launcher::release",
                    "release catalog unavailable provider={:?} error={error}",
                    source.provider
                );
                offline_diagnostics.push(format!("{:?}: {error}", source.provider));
            }
            Err(RemoteError::Fatal(error)) => {
                log::warn!(
                    target: "launcher::release",
                    "release catalog rejected provider={:?} error={error}",
                    source.provider
                );
                fatal_diagnostics.push(format!("{:?}: {error}", source.provider));
            }
        }
    }

    if modelscope.is_none() && huggingface.is_none() {
        if fatal_diagnostics.is_empty() && !offline_diagnostics.is_empty() {
            return Err(RemoteError::Offline(offline_diagnostics.join("; ")));
        }
        offline_diagnostics.extend(fatal_diagnostics);
        return Err(RemoteError::Fatal(format!(
            "No release source is available. {}",
            offline_diagnostics.join("; ")
        )));
    }

    let catalog = reconcile_catalogs(modelscope, huggingface)
        .map_err(|error| RemoteError::Fatal(format!("Release catalogs conflict: {error}")))?
        .ok_or_else(|| RemoteError::Fatal("No release source is available.".into()))?;
    let version = catalog
        .versions
        .first()
        .ok_or_else(|| RemoteError::Fatal("Release catalog is empty".into()))?;
    let artifact = version
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == target.package_kind)
        .cloned()
        .ok_or_else(|| {
            RemoteError::Fatal("Release catalog does not contain the portable package".into())
        })?;
    Ok(Release {
        version: version.version.clone(),
        artifact,
    })
}

fn fetch_catalog(
    client: &reqwest::blocking::Client,
    source: &UpdateSource,
    platform: AppPlatform,
    architecture: AppArchitecture,
) -> Result<ValidatedCatalog, RemoteError> {
    let response = client
        .get(&source.catalog_url)
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|error| RemoteError::request("Release catalog request failed", error))?
        .error_for_status()
        .map_err(|error| RemoteError::Fatal(format!("Release catalog request failed: {error}")))?;
    let mut bytes = Vec::new();
    response
        .take((MAX_CATALOG_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| RemoteError::Offline(format!("Read release catalog: {error}")))?;
    decode_catalog(
        source.provider,
        &bytes,
        PRODUCT_NAME,
        PRODUCT_IDENTIFIER,
        platform,
        architecture,
    )
    .map_err(RemoteError::Fatal)
}
