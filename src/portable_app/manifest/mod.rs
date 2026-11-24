use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::Read;

use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::FileProgress;

mod verify;

pub use verify::{remove_extras, repair, verify};

const REPAIR_TEMP_PREFIX: &str = ".hanafubuki-repair-";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreManifest {
    pub version: String,
    pub files: Vec<CoreManifestFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreManifestFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegrityStatus {
    Intact,
    ExtrasOnly,
    RepairRequired,
}

pub fn load(path: &ResolvedPath) -> Result<Option<CoreManifest>, String> {
    match fs::symlink_metadata(path.system()) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            log::warn!(
                target: "launcher::manifest",
                "manifest ignored because it is not a regular file path={}",
                path.native_text()
            );
            Ok(None)
        }
        Ok(_) => {
            let value = match safe_file::read_json::<Value>(path.system()) {
                Ok(Some(value)) => value,
                Ok(None) => {
                    log::info!(
                        target: "launcher::manifest",
                        "manifest is absent path={}",
                        path.native_text()
                    );
                    return Ok(None);
                }
                Err(error) => {
                    log::warn!(
                        target: "launcher::manifest",
                        "manifest could not be read and will be rebuilt path={} error={error}",
                        path.native_text()
                    );
                    return Ok(None);
                }
            };
            let Some((manifest, changed)) = decode_manifest(value) else {
                log::warn!(
                    target: "launcher::manifest",
                    "manifest is invalid and will be rebuilt path={}",
                    path.native_text()
                );
                return Ok(None);
            };
            if changed {
                log::info!(
                    target: "launcher::manifest",
                    "rewriting canonical manifest version={} files={} path={}",
                    manifest.version,
                    manifest.files.len(),
                    path.native_text()
                );
                store(&manifest, path)?;
            }
            log::info!(
                target: "launcher::manifest",
                "manifest loaded version={} files={} path={}",
                manifest.version,
                manifest.files.len(),
                path.native_text()
            );
            Ok(Some(manifest))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            log::info!(
                target: "launcher::manifest",
                "manifest is absent path={}",
                path.native_text()
            );
            Ok(None)
        }
        Err(error) => Err(format!("Inspect core manifest: {error}")),
    }
}

pub fn scan(version: &Version, root: &ResolvedPath) -> Result<CoreManifest, String> {
    log::info!(
        target: "launcher::manifest",
        "trusted core scan started version={version} root={}",
        root.native_text()
    );
    let mut files = Vec::new();
    let mut relative = Vec::new();
    scan_directory(root, &mut relative, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let manifest = CoreManifest {
        version: version.to_string(),
        files,
    };
    validate(&manifest)?;
    log::info!(
        target: "launcher::manifest",
        "trusted core scan completed version={} files={}",
        manifest.version,
        manifest.files.len()
    );
    Ok(manifest)
}

pub fn store(manifest: &CoreManifest, path: &ResolvedPath) -> Result<(), String> {
    validate(manifest)?;
    safe_file::write_json_atomic(path.system(), manifest)
        .map_err(|error| format!("Write core manifest: {error}"))?;
    log::info!(
        target: "launcher::manifest",
        "manifest stored version={} files={} path={}",
        manifest.version,
        manifest.files.len(),
        path.native_text()
    );
    Ok(())
}

fn decode_manifest(value: Value) -> Option<(CoreManifest, bool)> {
    let original = value.clone();
    let object = value.as_object()?;
    let mut sanitized = serde_json::Map::new();
    sanitized.insert("version".to_string(), object.get("version")?.clone());

    let files = object.get("files")?.as_array()?;
    let mut sanitized_files = Vec::with_capacity(files.len());
    for file in files {
        let file_object = file.as_object()?;
        let mut sanitized_file = serde_json::Map::new();
        sanitized_file.insert("path".to_string(), file_object.get("path")?.clone());
        sanitized_file.insert("sha256".to_string(), file_object.get("sha256")?.clone());
        sanitized_files.push(Value::Object(sanitized_file));
    }
    sanitized.insert("files".to_string(), Value::Array(sanitized_files));

    let manifest: CoreManifest = serde_json::from_value(Value::Object(sanitized)).ok()?;
    validate(&manifest).ok()?;
    let canonical = serde_json::to_value(&manifest).ok()?;
    Some((manifest, canonical != original))
}

fn scan_directory(
    directory: &ResolvedPath,
    relative: &mut Vec<String>,
    files: &mut Vec<CoreManifestFile>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory.system())
        .map_err(|error| format!("Scan trusted core {}: {error}", directory.native_text()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("Read trusted core entry: {error}"))?;
        let file_name = entry.file_name();
        let name = file_name
            .to_str()
            .ok_or("Trusted core path is not valid UTF-8")?;
        if name.starts_with(REPAIR_TEMP_PREFIX) {
            return Err("Trusted core contains a reserved repair file name".into());
        }
        let child = ResolvedPath::resolve_child(directory, &file_name)
            .map_err(|error| error.to_string())?;
        let metadata = fs::symlink_metadata(child.system())
            .map_err(|error| format!("Inspect trusted core entry: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("Trusted core contains a symbolic link: {name}"));
        }
        relative.push(name.to_string());
        if metadata.is_dir() {
            let resolved =
                ResolvedPath::resolve(child.native()).map_err(|error| error.to_string())?;
            scan_directory(&resolved, relative, files)?;
        } else if metadata.is_file() {
            let display_path = relative.join("/");
            log::debug!(
                target: "launcher::manifest",
                "trusted file scanned path={display_path} bytes={}",
                metadata.len()
            );
            files.push(CoreManifestFile {
                path: display_path,
                sha256: hash_file(&child)?,
            });
        } else {
            return Err(format!(
                "Trusted core contains an unsupported entry: {name}"
            ));
        }
        relative.pop();
    }
    Ok(())
}

fn validate(manifest: &CoreManifest) -> Result<(), String> {
    let version = Version::parse(&manifest.version)
        .map_err(|_| "Core manifest version is not valid SemVer")?;
    if !version.pre.is_empty()
        || !version.build.is_empty()
        || version.to_string() != manifest.version
    {
        return Err("Core manifest version must be canonical stable SemVer".into());
    }
    let mut previous: Option<&str> = None;
    for file in &manifest.files {
        components(&file.path)?;
        if previous.is_some_and(|previous| previous >= file.path.as_str()) {
            return Err("Core manifest files must be unique and sorted".into());
        }
        if file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("Core manifest SHA-256 must be lowercase hexadecimal".into());
        }
        previous = Some(&file.path);
    }
    Ok(())
}

fn components(path: &str) -> Result<Vec<&str>, String> {
    if path.is_empty() || path.contains('\\') || path.contains('\0') {
        return Err("Core manifest contains an invalid relative path".into());
    }
    let components = path.split('/').collect::<Vec<_>>();
    if components.iter().any(|component| {
        component.is_empty()
            || *component == "."
            || *component == ".."
            || component.starts_with(REPAIR_TEMP_PREFIX)
    }) {
        return Err("Core manifest contains an unsafe relative path".into());
    }
    Ok(components)
}

fn compose(root: &ResolvedPath, components: &[&str]) -> Result<ResolvedPath, String> {
    let mut current = root.clone();
    for component in components {
        current = ResolvedPath::resolve_child(&current, OsStr::new(component))
            .map_err(|error| error.to_string())?;
    }
    Ok(current)
}

fn hash_file(path: &ResolvedPath) -> Result<String, String> {
    let mut file = File::open(path.system())
        .map_err(|error| format!("Open file for hashing {}: {error}", path.native_text()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Hash file {}: {error}", path.native_text()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(encoded)
}

fn remove_any(path: &ResolvedPath) -> Result<(), String> {
    match fs::symlink_metadata(path.system()) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path.system())
                .map_err(|error| format!("Remove {}: {error}", path.native_text()))
        }
        Ok(_) => fs::remove_file(path.system())
            .map_err(|error| format!("Remove {}: {error}", path.native_text())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Inspect {}: {error}", path.native_text())),
    }
}

#[cfg(test)]
mod tests;
