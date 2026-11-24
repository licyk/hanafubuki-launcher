//! Instance presets: named defaults over one instance type, applied when an
//! instance is created. An instance's identity stays its base type id; it
//! records the preset it was made from for display only, so a preset that
//! changes or goes away changes nothing about it (`docs/rules/instance-types.md`).
//!
//! An extension's presets register while its row is active
//! ([`ContributedPresets`], held by the module host beside the type
//! registry); the user's are a file ([`user`]). The [`PresetCatalog`] lists
//! both by base type and checks each against its base type's install form
//! as the registry holds it then: a preset whose base type is not
//! registered, or that sets what the form does not take, is listed
//! unusable with the reason, never removed.

mod document;
mod documentation;
mod launch;
mod user;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::internal::hanafubuki_foundation::form_schema::{FieldKind, SchemaField, SchemaText};
use crate::internal::hanafubuki_foundation::module_faults::ContributionOwner;
use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

pub(crate) use document::check_argument;
#[cfg(feature = "schema")]
pub use document::preset_schema;
pub use launch::merge_launch_arguments;
pub use user::BrokenPreset;

use super::{InstanceProvider, InstanceTypeRegistry};

const PRESET_ID_MAX_BYTES: usize = 64;

/// The operation a type that takes launch arguments declares.
const LAUNCH_ARGUMENTS_OPERATION: &str = "launch-arguments";

/// A preset's name among its owner's: `[a-z0-9-]`, at most 64 bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresetName(String);

impl PresetName {
    pub fn parse(value: &str) -> Result<Self, String> {
        let valid = !value.is_empty()
            && value.len() <= PRESET_ID_MAX_BYTES
            && value
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'));
        if valid {
            Ok(Self(value.to_string()))
        } else {
            Err(format!("preset id `{value}` is not [a-z0-9-]"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Who a preset is: an extension's (by its id) or the user's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum PresetOwner {
    User,
    Extension(String),
}

impl PresetOwner {
    const USER: &'static str = "user";
}

/// A preset's full id, `<owner>:<name>`: `user:<name>` for the user's,
/// `<extension-id>:<name>` for an extension's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetId {
    pub owner: PresetOwner,
    pub name: PresetName,
}

impl PresetId {
    pub fn parse(value: &str) -> Result<Self, String> {
        let (owner, name) = value
            .split_once(':')
            .ok_or_else(|| format!("preset `{value}` is not `<owner>:<id>`"))?;
        let owner = match owner {
            PresetOwner::USER => PresetOwner::User,
            owner if !owner.is_empty() => PresetOwner::Extension(owner.to_string()),
            _ => return Err(format!("preset `{value}` is not `<owner>:<id>`")),
        };
        Ok(Self {
            owner,
            name: PresetName::parse(name)?,
        })
    }
}

impl fmt::Display for PresetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let owner = match &self.owner {
            PresetOwner::User => PresetOwner::USER,
            PresetOwner::Extension(id) => id,
        };
        write!(formatter, "{owner}:{}", self.name.as_str())
    }
}

impl<'de> Deserialize<'de> for PresetId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl Serialize for PresetId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// One launch argument a preset sets, by name.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchArgument {
    pub name: String,
    pub value: LaunchValue,
}

/// `true` passes the option alone and `false` leaves it out; a number or
/// text follows it as its value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum LaunchValue {
    Flag(bool),
    Number(Number),
    Text(String),
}

/// A preset as it was read: its structure checked, not yet against its
/// base type.
#[derive(Clone, Debug, PartialEq)]
pub struct InstancePreset {
    pub name: PresetName,
    pub base: InstanceTypeId,
    pub label: SchemaText,
    pub description: Option<SchemaText>,
    /// Install-form values: defaults over the base type's.
    pub install: Map<String, Value>,
    pub launch: Vec<LaunchArgument>,
}

/// A preset as the catalog lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetEntry {
    pub id: PresetId,
    pub owner: PresetOwner,
    pub base: InstanceTypeId,
    pub label: SchemaText,
    pub description: Option<SchemaText>,
    pub launch_arguments: Vec<LaunchArgument>,
    /// The base type's install form as the preset fills it: every field,
    /// the preset's value over the field's default, `null` for none.
    /// Empty for a preset that cannot be used.
    pub values: Map<String, Value>,
    /// Why it cannot be used now: its base type is not registered, or it
    /// sets what the type's form does not take.
    pub unavailable: Option<String>,
}

/// A user's entry that does not read, kept until it is deleted.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokenPresetEntry {
    /// `user:<key>`, which deletes it.
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetListing {
    pub presets: Vec<PresetEntry>,
    pub broken: Vec<BrokenPresetEntry>,
}

/// A usable preset, for creating an instance of its base type.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPreset {
    pub id: PresetId,
    pub install: Map<String, Value>,
    pub launch: Vec<LaunchArgument>,
}

/// "Save as preset": the creation form's values over a type.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveInstancePresetRequest {
    #[serde(rename = "type")]
    pub base: InstanceTypeId,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The form's values; the type's form checks each.
    pub values: Map<String, Value>,
    /// The launch arguments of the preset the form was filled from.
    #[serde(default)]
    pub launch_arguments: Vec<LaunchArgument>,
}

/// The presets the extensions' rows registered, each by its owner.
#[derive(Default)]
pub struct ContributedPresets {
    presets: RwLock<Vec<(ContributionOwner, InstancePreset)>>,
}

impl ContributedPresets {
    /// `owner`'s presets, replacing those it registered before; two of the
    /// same name are refused.
    pub fn register(
        &self,
        owner: ContributionOwner,
        presets: Vec<InstancePreset>,
    ) -> Result<(), String> {
        for (index, preset) in presets.iter().enumerate() {
            if presets[..index]
                .iter()
                .any(|known| known.name == preset.name)
            {
                return Err(format!(
                    "preset `{}` is contributed twice",
                    preset.name.as_str()
                ));
            }
        }
        let mut registered = self.presets.write();
        registered.retain(|(known, _)| *known != owner);
        registered.extend(presets.into_iter().map(|preset| (owner.clone(), preset)));
        Ok(())
    }

    /// Removes what `owner` registered.
    pub fn unregister(&self, owner: &ContributionOwner) {
        self.presets.write().retain(|(known, _)| known != owner);
    }

    /// The full ids of `owner`'s presets, in registration order.
    pub fn owned_by(&self, owner: &ContributionOwner) -> Vec<String> {
        self.presets
            .read()
            .iter()
            .filter(|(known, _)| known == owner)
            .filter_map(|(owner, preset)| Some(extension_id(owner, preset)?.to_string()))
            .collect()
    }

    fn snapshot(&self) -> Vec<(PresetId, InstancePreset)> {
        self.presets
            .read()
            .iter()
            .filter_map(|(owner, preset)| Some((extension_id(owner, preset)?, preset.clone())))
            .collect()
    }
}

fn extension_id(owner: &ContributionOwner, preset: &InstancePreset) -> Option<PresetId> {
    match owner {
        ContributionOwner::Extension(id) => Some(PresetId {
            owner: PresetOwner::Extension(id.clone()),
            name: preset.name.clone(),
        }),
        ContributionOwner::Core | ContributionOwner::Module(_) => None,
    }
}

/// Every preset, the extensions' and the user's, by base type (Core,
/// beside the type registry).
pub struct PresetCatalog {
    types: Arc<InstanceTypeRegistry>,
    contributed: Arc<ContributedPresets>,
    user: user::UserPresets,
}

impl PresetCatalog {
    /// The catalog over `types`, `contributed` and the user's presets at
    /// `user_file`.
    pub fn new(
        types: Arc<InstanceTypeRegistry>,
        contributed: Arc<ContributedPresets>,
        user_file: PathBuf,
    ) -> Self {
        Self {
            types,
            contributed,
            user: user::UserPresets::new(user_file),
        }
    }

    /// The presets over `base`, or every one, the extensions' first, each
    /// checked against its base type now; and the user's entries that do
    /// not read.
    pub fn list(&self, base: Option<&InstanceTypeId>) -> Result<PresetListing, String> {
        let user = self.user.read()?;
        let presets = self
            .contributed
            .snapshot()
            .into_iter()
            .chain(user.presets.into_iter().map(|preset| {
                let id = PresetId {
                    owner: PresetOwner::User,
                    name: preset.name.clone(),
                };
                (id, preset)
            }))
            .filter(|(_, preset)| base.is_none_or(|base| preset.base == *base))
            .map(|(id, preset)| self.entry(id, preset))
            .collect();
        let broken = user
            .broken
            .into_iter()
            .map(|(key, broken)| BrokenPresetEntry {
                id: format!("{}:{key}", PresetOwner::USER),
                reason: broken.reason,
            })
            .collect();
        Ok(PresetListing { presets, broken })
    }

    /// The preset `id`, usable over `base`.
    pub fn resolve(&self, id: &PresetId, base: &InstanceTypeId) -> Result<ResolvedPreset, String> {
        let preset = self
            .find(id)?
            .ok_or_else(|| format!("there is no preset `{id}`"))?;
        if preset.base != *base {
            return Err(format!(
                "preset `{id}` is over `{}`, not `{base}`",
                preset.base
            ));
        }
        let provider = self.types.require(&preset.base)?;
        check_against(&preset, provider.as_ref())
            .map_err(|reason| format!("preset `{id}` cannot be used: {reason}"))?;
        Ok(ResolvedPreset {
            id: id.clone(),
            install: preset.install,
            launch: preset.launch,
        })
    }

    /// A new preset of the user's from the creation form's values, which
    /// the base type's form checks as a preset's are.
    pub fn save(&self, request: SaveInstancePresetRequest) -> Result<PresetEntry, String> {
        let label = document::user_label(&request.label)?;
        let provider = self.types.require(&request.base)?;
        for argument in &request.launch_arguments {
            check_argument(argument)?;
        }
        // A preset never holds a secret: the form's secret values stay out.
        let secrets: Vec<String> = fields(provider.as_ref())
            .into_iter()
            .filter(|field| field.kind == FieldKind::Secret)
            .map(|field| field.key)
            .collect();
        let mut install = request.values;
        install.retain(|key, _| !secrets.contains(key));
        let draft = InstancePreset {
            name: PresetName("draft".to_string()),
            base: request.base.clone(),
            label,
            description: None,
            install,
            launch: request.launch_arguments,
        };
        check_against(&draft, provider.as_ref())?;
        let description = request
            .description
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        let entry = serde_json::json!({
            "base": draft.base,
            "label": request.label.trim(),
            "description": description,
            "install": draft.install,
            "launch": { "arguments": draft.launch },
        });
        let preset = self.user.add(request.label.trim(), entry)?;
        let id = PresetId {
            owner: PresetOwner::User,
            name: preset.name.clone(),
        };
        Ok(self.entry(id, preset))
    }

    /// Deletes the user's preset (or broken entry) `id`; an extension's
    /// leaves with its extension.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let key = id
            .strip_prefix(PresetOwner::USER)
            .and_then(|rest| rest.strip_prefix(':'))
            .ok_or_else(|| {
                format!("`{id}` is not one of your presets; an extension's leaves with it")
            })?;
        self.user.remove(key)
    }

    fn find(&self, id: &PresetId) -> Result<Option<InstancePreset>, String> {
        Ok(match &id.owner {
            PresetOwner::User => self
                .user
                .read()?
                .presets
                .into_iter()
                .find(|preset| preset.name == id.name),
            PresetOwner::Extension(_) => self
                .contributed
                .snapshot()
                .into_iter()
                .find(|(known, _)| known == id)
                .map(|(_, preset)| preset),
        })
    }

    fn entry(&self, id: PresetId, preset: InstancePreset) -> PresetEntry {
        let checked = self.types.require(&preset.base).and_then(|provider| {
            check_against(&preset, provider.as_ref())?;
            Ok(form_values(&fields(provider.as_ref()), &preset.install))
        });
        let (values, unavailable) = match checked {
            Ok(values) => (values, None),
            Err(reason) => (Map::new(), Some(reason)),
        };
        PresetEntry {
            owner: id.owner.clone(),
            id,
            base: preset.base,
            label: preset.label,
            description: preset.description,
            launch_arguments: preset.launch,
            values,
            unavailable,
        }
    }
}

/// Whether `preset` can be used over `provider`'s type: every install key
/// a field of its form, not a secret, and a value that field takes, and launch arguments
/// only for a type that takes them. A field the preset leaves unset is the
/// user's to fill, so a required one is not asked for here.
fn check_against(preset: &InstancePreset, provider: &dyn InstanceProvider) -> Result<(), String> {
    let fields = fields(provider);
    let mut problems = Vec::new();
    for (key, value) in &preset.install {
        match fields.iter().find(|field| field.key == *key) {
            None => problems.push(format!(
                "`{key}` is not a field of `{}`'s install form",
                preset.base
            )),
            Some(field) if field.kind == FieldKind::Secret => {
                problems.push(format!("`{key}` {}", documentation::SECRET.message))
            }
            Some(_) if value.is_null() => {}
            Some(field) => {
                if let Err(error) = field.read(value) {
                    problems.push(error);
                }
            }
        }
    }
    let takes_arguments = provider
        .descriptor()
        .operations
        .iter()
        .any(|operation| operation == LAUNCH_ARGUMENTS_OPERATION);
    if !preset.launch.is_empty() && !takes_arguments {
        problems.push(format!(
            "type `{}` {}",
            preset.base,
            documentation::LAUNCH.message
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

fn fields(provider: &dyn InstanceProvider) -> Vec<SchemaField> {
    provider
        .install_form()
        .into_iter()
        .flat_map(|group| group.fields)
        .collect()
}

/// Every field's value: the preset's, else the field's default, else
/// `null`.
fn form_values(fields: &[SchemaField], install: &Map<String, Value>) -> Map<String, Value> {
    fields
        .iter()
        .map(|field| {
            let value = install
                .get(&field.key)
                .filter(|value| !value.is_null())
                .cloned()
                .or_else(|| {
                    field
                        .default
                        .as_ref()
                        .and_then(|value| serde_json::to_value(value).ok())
                })
                .unwrap_or(Value::Null);
            (field.key.clone(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests;
