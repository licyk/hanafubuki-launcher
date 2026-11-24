use std::collections::BTreeMap;

use semver::Version;
use serde::Deserialize;
use serde_json::Value;

const MAX_SIGNATURE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Provider {
    Modelscope,
    Huggingface,
}

#[derive(Clone, Copy)]
pub struct Source {
    pub provider: Provider,
    pub latest_url: &'static str,
}

pub const SOURCES: [Source; 2] = [
    Source {
        provider: Provider::Modelscope,
        latest_url: "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/hanafubuki-launcher/latest.json",
    },
    Source {
        provider: Provider::Huggingface,
        latest_url: "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/hanafubuki-launcher/latest.json",
    },
];

// latest.json comes from the release pipeline, which may add fields (`notes`,
// `pub_date`) and targets before this build knows them: unknown fields are
// ignored and only this build's own target is read.
#[derive(Clone, Debug, Deserialize)]
struct RawCatalog {
    version: String,
    platforms: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawArtifact {
    signature: String,
    url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    pub provider: Provider,
    pub version: Version,
    pub url: String,
    pub signature: String,
}

pub fn decode(source: Source, bytes: &[u8]) -> Result<Candidate, String> {
    let raw: RawCatalog = serde_json::from_slice(bytes)
        .map_err(|error| format!("Decode launcher latest.json: {error}"))?;
    let version = Version::parse(&raw.version)
        .map_err(|error| format!("Launcher version is not SemVer: {error}"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("Launcher version must be stable SemVer".to_string());
    }

    decode_for_target(source, raw, version, current_platform_architecture()?)
}

/// Only the own target is read: another target may be absent, unknown, or
/// of a shape this build does not know, and none of it rejects the document.
/// The own target's artifact is strict, and the package it names is still
/// verified against the Minisign key before anything is installed.
fn decode_for_target(
    source: Source,
    raw: RawCatalog,
    version: Version,
    (platform, architecture): (&str, &str),
) -> Result<Candidate, String> {
    let artifact = raw
        .platforms
        .get(platform)
        .and_then(|architectures| architectures.get(architecture))
        .ok_or_else(|| {
            format!("Launcher latest.json has no artifact for {platform}/{architecture}")
        })?;
    let artifact = RawArtifact::deserialize(artifact).map_err(|error| {
        format!("Launcher latest.json artifact for {platform}/{architecture} is invalid: {error}")
    })?;
    validate_signature(&artifact.signature)?;
    Ok(Candidate {
        provider: source.provider,
        version,
        url: artifact.url,
        signature: artifact.signature,
    })
}

fn current_platform_architecture() -> Result<(&'static str, &'static str), String> {
    let platform = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        os => return Err(format!("Unsupported launcher platform: {os}")),
    };
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        architecture => return Err(format!("Unsupported launcher architecture: {architecture}")),
    };
    Ok((platform, architecture))
}

fn validate_signature(signature: &str) -> Result<(), String> {
    if signature.is_empty()
        || signature.len() > MAX_SIGNATURE_BYTES
        || signature.trim() != signature
        || signature.chars().any(char::is_whitespace)
    {
        return Err("Launcher signature is not a bounded base64 token".to_string());
    }
    Ok(())
}

pub fn reconcile(candidates: Vec<Candidate>) -> Option<Vec<Candidate>> {
    let first = candidates.first()?;
    if candidates.iter().any(|candidate| {
        candidate.version != first.version || candidate.signature != first.signature
    }) {
        return None;
    }
    Some(candidates)
}

#[cfg(test)]
mod tests;
