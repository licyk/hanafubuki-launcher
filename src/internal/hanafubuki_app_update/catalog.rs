use semver::Version;
use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::internal::hanafubuki_app_update::{
    AppArchitecture, AppPlatform, InstalledBundle, UpdateProvider, UpdateRequestContext,
};

pub const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_VERSIONS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCatalog {
    pub provider: UpdateProvider,
    pub versions: Vec<ValidatedCatalogVersion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCatalogVersion {
    pub version: Version,
    pub artifacts: Vec<ValidatedCatalogArtifact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCatalogArtifact {
    pub platform: AppPlatform,
    pub architecture: AppArchitecture,
    pub kind: String,
    pub file_name: String,
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualPackageCandidate {
    pub provider: UpdateProvider,
    pub kind: String,
    pub file_name: String,
    pub size: u64,
    pub url: String,
}

// The catalog comes from the release pipeline, which may add fields, targets
// and kinds before this build knows them: unknown fields are ignored, and
// versions and artifacts are read one at a time so an entry this build cannot
// use is skipped instead of failing the catalog.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalog {
    schema_version: u32,
    product: RawProduct,
    versions: Vec<Value>,
}

#[derive(Deserialize)]
struct RawProduct {
    name: String,
    identifier: String,
}

#[derive(Deserialize)]
struct RawVersion {
    version: String,
    artifacts: Vec<Value>,
}

/// What an artifact is for, read before the rest so another target's or an
/// unknown kind's entry is skipped whatever else it holds.
#[derive(Deserialize)]
struct RawArtifactTarget {
    platform: String,
    arch: String,
    kind: String,
}

#[derive(Deserialize)]
struct RawArtifact {
    kind: String,
    filename: String,
    size: u64,
    url: String,
}

fn bounded(value: &str, label: &str, limit: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > limit {
        return Err(format!(
            "{label} must be non-empty and at most {limit} bytes"
        ));
    }
    if value.chars().any(|character| character == '\0') {
        return Err(format!("{label} contains a forbidden control character"));
    }
    Ok(())
}

fn stable_version(value: &str, label: &str) -> Result<Version, String> {
    bounded(value, label, 64)?;
    let version = Version::parse(value).map_err(|_| format!("{label} is not valid SemVer"))?;
    if !version.pre.is_empty() || !version.build.is_empty() || version.to_string() != value {
        return Err(format!("{label} must be canonical stable SemVer"));
    }
    Ok(version)
}

fn expected_kinds(
    platform: AppPlatform,
    architecture: AppArchitecture,
) -> Option<&'static [&'static str]> {
    match (platform, architecture) {
        (AppPlatform::Windows, AppArchitecture::X86_64) => Some(&["nsis", "msi", "portable-zip"]),
        (AppPlatform::Macos, AppArchitecture::Aarch64 | AppArchitecture::X86_64) => {
            Some(&["dmg", "app-tar-gz"])
        }
        (AppPlatform::Linux, AppArchitecture::Aarch64 | AppArchitecture::X86_64) => {
            Some(&["appimage", "deb", "rpm"])
        }
        _ => None,
    }
}

fn target_names(
    platform: AppPlatform,
    architecture: AppArchitecture,
) -> (&'static str, &'static str) {
    let platform = match platform {
        AppPlatform::Windows => "windows",
        AppPlatform::Macos => "macos",
        AppPlatform::Linux => "linux",
    };
    let architecture = match architecture {
        AppArchitecture::X86_64 => "x86_64",
        AppArchitecture::Aarch64 => "aarch64",
    };
    (platform, architecture)
}

fn expected_filename(
    version: &Version,
    platform: AppPlatform,
    architecture: AppArchitecture,
    kind: &str,
) -> Result<String, String> {
    let suffix = match kind {
        "nsis" => "-setup.exe",
        "msi" => ".msi",
        "dmg" => ".dmg",
        "app-tar-gz" => ".app.tar.gz",
        "appimage" => ".AppImage",
        "deb" => ".deb",
        "rpm" => ".rpm",
        "portable-zip" => "-portable.zip",
        _ => return Err(format!("unsupported catalog artifact kind: {kind}")),
    };
    let (platform, architecture) = target_names(platform, architecture);
    Ok(format!(
        "Hanafubuki-{version}-{platform}-{architecture}{suffix}"
    ))
}

fn validate_download_url(
    provider: UpdateProvider,
    value: &str,
    artifact_path: &str,
) -> Result<(), String> {
    bounded(value, "download URL", MAX_TEXT_BYTES)?;
    let parsed = Url::parse(value).map_err(|_| "download URL is invalid".to_string())?;
    if parsed.scheme() != "https" || parsed.fragment().is_some() {
        return Err("download URL must be HTTPS without a fragment".into());
    }
    let decoded_path = percent_encoding::percent_decode_str(parsed.path())
        .decode_utf8()
        .map_err(|_| "download URL path is not UTF-8".to_string())?;
    let valid = match provider {
        UpdateProvider::Huggingface => {
            parsed.host_str() == Some("huggingface.co")
                && decoded_path.contains("/resolve/")
                && decoded_path.ends_with(&format!("/{artifact_path}"))
                && parsed.query().is_none()
        }
        UpdateProvider::Modelscope => {
            let public = parsed.host_str() == Some("modelscope.cn")
                && decoded_path.contains("/resolve/")
                && decoded_path.ends_with(&format!("/{artifact_path}"))
                && parsed.query().is_none();
            let query = parsed
                .query_pairs()
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>();
            let file_paths = query
                .iter()
                .filter(|(key, _)| key == "FilePath")
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>();
            let revisions = query.iter().filter(|(key, _)| key == "Revision").count();
            let api = parsed.host_str() == Some("www.modelscope.cn")
                && parsed.path().starts_with("/api/v1/models/")
                && parsed.path().ends_with("/repo")
                && file_paths == [artifact_path]
                && revisions == 1
                && query.len() == 2;
            public || api
        }
    };
    if !valid {
        return Err(format!(
            "download URL does not match the {:?} artifact path",
            provider
        ));
    }
    Ok(())
}

fn decode_artifact(
    raw: RawArtifact,
    version: &Version,
    provider: UpdateProvider,
    platform: AppPlatform,
    architecture: AppArchitecture,
) -> Result<ValidatedCatalogArtifact, String> {
    let expected_filename = expected_filename(version, platform, architecture, &raw.kind)?;
    if raw.filename != expected_filename {
        return Err("artifact filename does not match its version and target".into());
    }
    if raw.size == 0 {
        return Err("artifact size must be positive".into());
    }
    let (platform_name, architecture_name) = target_names(platform, architecture);
    let artifact_path = format!(
        "hanafubuki/releases/{version}/{platform_name}/{architecture_name}/{}",
        raw.filename
    );
    validate_download_url(provider, &raw.url, &artifact_path)?;
    Ok(ValidatedCatalogArtifact {
        platform,
        architecture,
        kind: raw.kind,
        file_name: raw.filename,
        size: raw.size,
        url: raw.url,
    })
}

/// Decode a provider's release catalog for this build's own target.
///
/// The document's identity is strict: `schemaVersion` 1, the product, the
/// size bound, and unique versions in descending stable SemVer order across
/// every version entry that can be read. Everything else is selected for the
/// own target: another target's artifacts and a kind this build does not know
/// are skipped unread, an unreadable version entry is skipped, and a version
/// without every kind the own target needs is not offered. The own target's
/// artifacts stay strict (deterministic order, one per kind, the canonical
/// filename, a positive size, the HTTPS host and path pinning), so a broken
/// one fails the catalog with an error naming the target. A target without a
/// kind matrix (Windows on ARM) has no update.
pub fn decode_catalog(
    provider: UpdateProvider,
    bytes: &[u8],
    expected_product: &str,
    expected_identifier: &str,
    expected_platform: AppPlatform,
    expected_architecture: AppArchitecture,
) -> Result<ValidatedCatalog, String> {
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err(format!(
            "catalog exceeds the {MAX_CATALOG_BYTES}-byte limit"
        ));
    }
    let raw: RawCatalog = serde_json::from_slice(bytes)
        .map_err(|error| format!("catalog JSON is invalid: {error}"))?;
    if raw.schema_version != 1 {
        return Err("catalog schemaVersion must be 1".into());
    }
    bounded(&raw.product.name, "product name", 128)?;
    bounded(&raw.product.identifier, "product identifier", 256)?;
    if raw.product.name != expected_product || raw.product.identifier != expected_identifier {
        return Err("catalog product identity does not match the application".into());
    }
    let mut versions = Vec::new();
    let Some(expected_kinds) = expected_kinds(expected_platform, expected_architecture) else {
        log::info!(
            "update catalog has no package kinds for this target provider={provider:?} \
             platform={expected_platform:?} architecture={expected_architecture:?}"
        );
        return Ok(ValidatedCatalog { provider, versions });
    };
    if raw.versions.len() > MAX_VERSIONS {
        log::warn!(
            "update catalog lists {} versions; only the newest {MAX_VERSIONS} are read provider={provider:?}",
            raw.versions.len()
        );
    }
    let mut previous: Option<Version> = None;
    for item in raw.versions.into_iter().take(MAX_VERSIONS) {
        let Some((version, artifacts)) = RawVersion::deserialize(&item).ok().and_then(|item| {
            let version = stable_version(&item.version, "catalog version").ok()?;
            Some((version, item.artifacts))
        }) else {
            log::info!("skipped an unreadable update catalog version provider={provider:?}");
            continue;
        };
        if let Some(previous) = &previous {
            if *previous == version {
                return Err("catalog versions must be unique".into());
            }
            if *previous < version {
                return Err("catalog versions must use descending stable SemVer order".into());
            }
        }
        previous = Some(version.clone());
        let target = (expected_platform, expected_architecture);
        match target_artifacts(artifacts, &version, provider, target, expected_kinds)? {
            Some(artifacts) => versions.push(ValidatedCatalogVersion { version, artifacts }),
            None => log::info!(
                "update catalog version {version} lacks a package for this target provider={provider:?}"
            ),
        }
    }
    Ok(ValidatedCatalog { provider, versions })
}

/// The own target's artifacts of one version, or `None` when a kind the
/// target needs is missing.
fn target_artifacts(
    raw: Vec<Value>,
    version: &Version,
    provider: UpdateProvider,
    (platform, architecture): (AppPlatform, AppArchitecture),
    expected_kinds: &[&str],
) -> Result<Option<Vec<ValidatedCatalogArtifact>>, String> {
    let (platform_name, architecture_name) = target_names(platform, architecture);
    let broken = |error: String| {
        format!("catalog {version} {platform_name}/{architecture_name} artifact: {error}")
    };
    let mut artifacts: Vec<ValidatedCatalogArtifact> = Vec::with_capacity(expected_kinds.len());
    for item in raw {
        let Ok(target) = RawArtifactTarget::deserialize(&item) else {
            continue;
        };
        if target.platform != platform_name || target.arch != architecture_name {
            continue;
        }
        if !expected_kinds.contains(&target.kind.as_str()) {
            log::debug!(
                "skipped update catalog artifact of an unknown kind version={version} kind={}",
                target.kind
            );
            continue;
        }
        let artifact =
            RawArtifact::deserialize(&item).map_err(|error| broken(error.to_string()))?;
        if let Some(last) = artifacts.last() {
            if (last.kind.as_str(), last.file_name.as_str())
                >= (artifact.kind.as_str(), artifact.filename.as_str())
            {
                return Err(broken(if last.kind == artifact.kind {
                    "identities must be unique".into()
                } else {
                    "artifacts must use deterministic ordering".into()
                }));
            }
        }
        artifacts.push(
            decode_artifact(artifact, version, provider, platform, architecture).map_err(broken)?,
        );
    }
    let complete = expected_kinds
        .iter()
        .all(|kind| artifacts.iter().any(|artifact| artifact.kind == *kind));
    Ok(complete.then_some(artifacts))
}

fn immutable_equal(left: &ValidatedCatalogVersion, right: &ValidatedCatalogVersion) -> bool {
    left.version == right.version
        && left.artifacts.len() == right.artifacts.len()
        && left
            .artifacts
            .iter()
            .zip(&right.artifacts)
            .all(|(left, right)| {
                left.platform == right.platform
                    && left.architecture == right.architecture
                    && left.kind == right.kind
                    && left.file_name == right.file_name
                    && left.size == right.size
            })
}

pub fn reconcile_catalogs(
    left: Option<ValidatedCatalog>,
    right: Option<ValidatedCatalog>,
) -> Result<Option<ValidatedCatalog>, String> {
    match (left, right) {
        (None, None) => Ok(None),
        (Some(catalog), None) | (None, Some(catalog)) => Ok(Some(catalog)),
        (Some(left), Some(right)) => {
            let left_latest = left.versions.first();
            let right_latest = right.versions.first();
            match (left_latest, right_latest) {
                (None, None) => Ok(Some(left)),
                (Some(_), None) => Ok(Some(left)),
                (None, Some(_)) => Ok(Some(right)),
                (Some(left_version), Some(right_version)) => {
                    if left_version.version == right_version.version
                        && !immutable_equal(left_version, right_version)
                    {
                        return Err(
                            "catalog sources conflict on immutable latest-version metadata".into(),
                        );
                    }
                    if right_version.version > left_version.version {
                        Ok(Some(right))
                    } else {
                        Ok(Some(left))
                    }
                }
            }
        }
    }
}

pub fn select_manual_packages(
    catalog: &ValidatedCatalog,
    context: &UpdateRequestContext,
) -> Result<Vec<ManualPackageCandidate>, String> {
    let latest = catalog
        .versions
        .first()
        .ok_or("catalog does not contain a release")?;
    let preferences: &[&str] = match context.installed_bundle {
        InstalledBundle::Nsis => &["nsis", "msi", "portable-zip"],
        InstalledBundle::Msi => &["msi", "nsis", "portable-zip"],
        InstalledBundle::AppImage => &["appimage", "deb", "rpm"],
        InstalledBundle::Deb => &["deb", "appimage", "rpm"],
        InstalledBundle::Rpm => &["rpm", "appimage", "deb"],
        InstalledBundle::MacosApp => &["dmg"],
        InstalledBundle::Unknown => match context.platform {
            AppPlatform::Windows => &["portable-zip", "nsis", "msi"],
            AppPlatform::Linux => &["appimage", "deb", "rpm"],
            AppPlatform::Macos => &["dmg"],
        },
    };
    let mut selected = Vec::new();
    for kind in preferences {
        if let Some(artifact) = latest.artifacts.iter().find(|artifact| {
            artifact.platform == context.platform
                && artifact.architecture == context.architecture
                && artifact.kind == *kind
        }) {
            selected.push(ManualPackageCandidate {
                provider: catalog.provider,
                kind: artifact.kind.clone(),
                file_name: artifact.file_name.clone(),
                size: artifact.size,
                url: artifact.url.clone(),
            });
        }
    }
    if selected.is_empty() {
        return Err("catalog has no manual package for the installed target".into());
    }
    Ok(selected)
}
