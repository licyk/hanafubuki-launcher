//! Minisign signatures from the release pipeline, checked against
//! `assets/updater-public-key.txt`.
//!
//! A signature is the base64-wrapped Minisign text the Tauri signer writes:
//! inline in the launcher's own `latest.json`, or as the detached `<file>.sig`
//! published next to an application package.

use base64::Engine;
use minisign_verify::{PublicKey, Signature, StreamVerifier};

pub struct UpdaterKey(PublicKey);

impl UpdaterKey {
    /// The key the application's updater trusts, embedded at build time.
    pub fn embedded() -> Result<Self, String> {
        Self::decode(&embedded_public_key()?)
    }

    /// Decodes a base64-wrapped Minisign public key: the base64 of the Minisign
    /// public key document, which is what the application's updater settings hold.
    pub fn decode(encoded: &str) -> Result<Self, String> {
        let text = decode_wrapped(encoded, "updater public key")?;
        PublicKey::decode(&text)
            .map(Self)
            .map_err(|error| format!("Decode updater Minisign public key: {error}"))
    }

    pub fn verify(&self, data: &[u8], signature: &ReleaseSignature) -> Result<(), String> {
        self.0
            .verify(data, &signature.0, true)
            .map_err(|error| format!("Verify Minisign signature: {error}"))
    }

    /// A verifier fed the data in chunks, for a package too large to hold in
    /// memory; it accepts only prehashed signatures, which the Tauri signer
    /// writes.
    pub fn verifier<'a>(
        &'a self,
        signature: &'a ReleaseSignature,
    ) -> Result<StreamVerifier<'a>, String> {
        self.0
            .verify_stream(&signature.0)
            .map_err(|error| format!("Verify Minisign signature: {error}"))
    }
}

pub struct ReleaseSignature(Signature);

impl ReleaseSignature {
    /// Decodes a base64-wrapped Minisign signature; surrounding whitespace,
    /// such as a `.sig` file's trailing newline, is ignored.
    pub fn decode(encoded: &str) -> Result<Self, String> {
        let text = decode_wrapped(encoded.trim(), "signature")?;
        Signature::decode(&text)
            .map(Self)
            .map_err(|error| format!("Decode Minisign signature: {error}"))
    }
}

fn decode_wrapped(encoded: &str, label: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("Decode {label}: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("Decode {label} text: {error}"))
}

fn embedded_public_key() -> Result<String, String> {
    let key = include_str!("../assets/updater-public-key.txt");
    Some(key.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Embedded updater public key is missing".to_string())
}

#[cfg(test)]
mod tests;
