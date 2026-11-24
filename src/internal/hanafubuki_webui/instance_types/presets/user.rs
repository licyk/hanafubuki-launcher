//! The user's presets (`<config>/instance-presets.json`), written by Rust
//! only: `presets` by name, each an entry in a preset's shape, and `broken`
//! by key. Read tolerantly, as the extension store's `state.json` is: an
//! entry whose key is not a name or which does not decode is moved to
//! `broken` with the reason and what it held, after the file is
//! quarantined (`.recovery/`), and the file is written again; the others
//! load. A document that is not one is quarantined whole. A broken entry
//! stays until it is deleted by its key.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_foundation::persistence::quarantine_file;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::InstancePreset;

/// An entry the file could not use: why, and what it held.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokenPreset {
    pub reason: String,
    #[serde(default)]
    pub entry: Value,
}

#[derive(Debug, Default, Serialize)]
struct UserFile {
    /// Every entry here decodes.
    presets: BTreeMap<String, Value>,
    broken: BTreeMap<String, BrokenPreset>,
}

/// The file, and the lock every read-modify-write of it takes.
pub(super) struct UserPresets {
    path: PathBuf,
    lock: Mutex<()>,
}

/// What the file holds now.
pub(super) struct UserPresetsRead {
    pub presets: Vec<InstancePreset>,
    pub broken: BTreeMap<String, BrokenPreset>,
}

impl UserPresets {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub(super) fn read(&self) -> Result<UserPresetsRead, String> {
        let _guard = self.lock.lock();
        let file = UserFile::read(&self.path)?;
        Ok(UserPresetsRead {
            presets: file.decoded(),
            broken: file.broken,
        })
    }

    /// Adds `entry` under a new name from `stem`, and returns the preset it
    /// reads as; `entry` was checked by its caller.
    pub(super) fn add(&self, stem: &str, entry: Value) -> Result<InstancePreset, String> {
        let _guard = self.lock.lock();
        let mut file = UserFile::read(&self.path)?;
        let name = unique_name(stem, |name| {
            file.presets.contains_key(name) || file.broken.contains_key(name)
        });
        let preset = InstancePreset::from_user_entry(&name, &entry)?;
        file.presets.insert(name, entry);
        file.write(&self.path)?;
        Ok(preset)
    }

    /// Removes the preset or the broken entry named `key`.
    pub(super) fn remove(&self, key: &str) -> Result<(), String> {
        let _guard = self.lock.lock();
        let mut file = UserFile::read(&self.path)?;
        let removed = file.presets.remove(key).is_some() || file.broken.remove(key).is_some();
        if !removed {
            return Err(format!("you have no preset `{key}`"));
        }
        file.write(&self.path)
    }
}

impl UserFile {
    /// The file at `path`; an entry or a document it cannot use is
    /// quarantined and the file rewritten without it.
    fn read(path: &Path) -> Result<Self, String> {
        let raw = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("read {}: {error}", path.display())),
        };
        let (file, repaired) = Self::decode(&raw);
        if repaired {
            let quarantined = quarantine_file(path)?;
            log::warn!(
                "instance presets {} held entries it cannot use; the original is kept at {:?}",
                path.display(),
                quarantined
            );
            file.write(path)?;
        }
        Ok(file)
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        safe_file::write_json_atomic(path, self)
    }

    /// The file and whether reading it dropped something.
    fn decode(raw: &str) -> (Self, bool) {
        let Ok(Value::Object(document)) = serde_json::from_str::<Value>(raw) else {
            return (Self::default(), true);
        };
        let section = |key: &str| match document.get(key) {
            Some(Value::Object(entries)) => (entries.clone(), false),
            None => (Map::new(), false),
            Some(_) => (Map::new(), true),
        };
        let (presets, bad_presets) = section("presets");
        let (broken, bad_broken) = section("broken");
        let mut repaired = bad_presets || bad_broken;
        let mut file = Self::default();
        for (key, entry) in presets {
            match InstancePreset::from_user_entry(&key, &entry) {
                Ok(_) => {
                    file.presets.insert(key, entry);
                }
                Err(reason) => {
                    repaired = true;
                    file.broken.insert(key, BrokenPreset { reason, entry });
                }
            }
        }
        for (key, entry) in broken {
            match BrokenPreset::deserialize(&entry) {
                Ok(record) => {
                    file.broken.insert(key, record);
                }
                Err(_) => repaired = true,
            }
        }
        (file, repaired)
    }

    fn decoded(&self) -> Vec<InstancePreset> {
        self.presets
            .iter()
            .filter_map(|(key, entry)| InstancePreset::from_user_entry(key, entry).ok())
            .collect()
    }
}

/// A preset name from the label the user gave it: its ASCII letters and
/// digits, the rest as `-`, or `preset` when nothing is left, with `-2`,
/// `-3`, … until it is not `taken`.
fn unique_name(stem: &str, taken: impl Fn(&str) -> bool) -> String {
    let slug = stem
        .chars()
        .flat_map(char::to_lowercase)
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug: String = if slug.is_empty() {
        "preset".to_string()
    } else {
        slug
    }
    .chars()
    .take(48)
    .collect();
    let slug = slug.trim_end_matches('-').to_string();
    (1..)
        .map(|index| match index {
            1 => slug.clone(),
            index => format!("{slug}-{index}"),
        })
        .find(|name| !taken(name))
        .unwrap_or(slug)
}
