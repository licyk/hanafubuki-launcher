use std::collections::BTreeMap;

use crate::internal::hanafubuki_foundation::json_normalization::{
    JsonField, JsonNormalizationEventKind, JsonShape, OPAQUE_JSON_SHAPE, json_pointer_child,
    normalize_current_json,
};
use crate::internal::hanafubuki_instance_contracts::launch_arguments_within_bounds;
use serde_json::{Map, Value};

use crate::internal::hanafubuki_config::AppPaths;
use crate::internal::hanafubuki_config::repository::settings::{self, AppSettings};

use super::{InstanceConfig, InstanceQuickPath, LaunchArgumentOrigin, validation};

const COMPONENT_PATH_FIELDS: &[JsonField] = &[
    JsonField::new("path", &OPAQUE_JSON_SHAPE),
    JsonField::new("external", &OPAQUE_JSON_SHAPE),
];
const COMPONENT_PATH_JSON_SHAPE: JsonShape = JsonShape::Object(COMPONENT_PATH_FIELDS);
pub const INSTANCE_COMPONENT_PATHS_CURRENT_JSON_SHAPE: JsonShape =
    JsonShape::MapValues(&COMPONENT_PATH_JSON_SHAPE);
const NULLABLE_COMPONENT_PATHS_JSON_SHAPE: JsonShape =
    JsonShape::Nullable(&INSTANCE_COMPONENT_PATHS_CURRENT_JSON_SHAPE);
const QUICK_PATH_FIELDS: &[JsonField] = &[
    JsonField::new("name", &OPAQUE_JSON_SHAPE),
    JsonField::new("path", &OPAQUE_JSON_SHAPE),
];
const QUICK_PATH_JSON_SHAPE: JsonShape = JsonShape::Object(QUICK_PATH_FIELDS);
const QUICK_PATHS_JSON_SHAPE: JsonShape = JsonShape::Array(&QUICK_PATH_JSON_SHAPE);
const INSTANCE_CONFIG_FIELDS: &[JsonField] = &[
    JsonField::new("id", &OPAQUE_JSON_SHAPE),
    JsonField::new("name", &OPAQUE_JSON_SHAPE),
    JsonField::new("configRevision", &OPAQUE_JSON_SHAPE),
    JsonField::new("installDir", &OPAQUE_JSON_SHAPE),
    JsonField::new("componentPaths", &NULLABLE_COMPONENT_PATHS_JSON_SHAPE),
    JsonField::new("quickPaths", &QUICK_PATHS_JSON_SHAPE),
    JsonField::new("envVars", &OPAQUE_JSON_SHAPE),
    JsonField::new("launchArgs", &OPAQUE_JSON_SHAPE),
    JsonField::new("launchArgumentOrigin", &OPAQUE_JSON_SHAPE),
    JsonField::new("settingsOverrides", &OPAQUE_JSON_SHAPE),
];
const INSTANCE_CONFIG_JSON_SHAPE: JsonShape = JsonShape::Object(INSTANCE_CONFIG_FIELDS);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistedInstanceIssueKind {
    MissingField,
    UnsupportedField,
    /// One field or list entry fell back to its default.
    InvalidField,
    InvalidSection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PersistedInstanceIssue {
    pub kind: PersistedInstanceIssueKind,
    pub json_pointer: String,
    pub message: String,
}

pub(crate) struct DecodedPersistedInstanceConfig {
    pub config: InstanceConfig,
    pub changed: bool,
    pub issues: Vec<PersistedInstanceIssue>,
}

/// Decode one on-disk instance configuration into the current contract.
///
/// Unsupported fields are discarded, and an invalid list entry, override, or
/// launch-argument origin falls back to its default on its own. Identity and
/// path values are never renamed, synthesized or guessed: a record whose id
/// does not match its directory, or whose paths fail the path policy, is an
/// error the caller turns into a disabled instance.
pub(crate) struct PersistedInstanceConfigDecoder<'a> {
    paths: &'a AppPaths,
}

impl<'a> PersistedInstanceConfigDecoder<'a> {
    pub fn new(paths: &'a AppPaths) -> Self {
        Self { paths }
    }

    pub fn decode(
        &self,
        instance_id: &str,
        value: Value,
    ) -> Result<DecodedPersistedInstanceConfig, String> {
        let original = value.clone();
        let mut normalized = value;
        let mut issues = Vec::new();
        normalize_current_json(&mut normalized, &INSTANCE_CONFIG_JSON_SHAPE, |event| {
            if event.kind == JsonNormalizationEventKind::FieldRemoved {
                issues.push(unsupported_field(
                    &event.json_pointer,
                    "unsupported instance field was removed",
                ));
            }
        });
        let mut object = normalized
            .as_object()
            .cloned()
            .ok_or_else(|| "instance config root is not an object".to_string())?;

        validate_component_paths_shape(&object)?;
        normalize_quick_paths(&mut object, &mut issues);
        normalize_env_vars(&mut object, &mut issues);
        normalize_settings_overrides(self.paths, &mut object, &mut issues)?;
        normalize_launch_args(&mut object, &mut issues);
        normalize_launch_argument_origin(&mut object, &mut issues);

        let mut config: InstanceConfig = serde_json::from_value(Value::Object(object))
            .map_err(|error| format!("instance schema is not safely recoverable: {error}"))?;
        if config.id != instance_id {
            return Err("instance id does not match its record directory".to_string());
        }
        validation::validate_instance_config(self.paths, &mut config)
            .map_err(|error| format!("instance validation failed: {error}"))?;

        let canonical = serde_json::to_value(&config)
            .map_err(|error| format!("serialize instance: {error}"))?;
        Ok(DecodedPersistedInstanceConfig {
            changed: canonical != original,
            config,
            issues,
        })
    }
}

fn validate_component_paths_shape(instance: &Map<String, Value>) -> Result<(), String> {
    let Some(value) = instance.get("componentPaths") else {
        return Ok(());
    };
    if value.is_null() {
        return Ok(());
    }
    let object = value
        .as_object()
        .ok_or_else(|| "componentPaths must be an object or null".to_string())?;

    if !object.contains_key("core") {
        return Err("/componentPaths/core is required".to_string());
    }
    for (component, path) in object {
        let pointer = format!("/componentPaths/{component}");
        let path = path
            .as_object()
            .ok_or_else(|| format!("{pointer} must be an object"))?;
        if !path.contains_key("path") {
            return Err(format!("{pointer}/path is required"));
        }
    }
    Ok(())
}

/// A missing list is simply the current empty default. An unusable entry is
/// dropped alone; the others keep their order.
fn normalize_quick_paths(
    instance: &mut Map<String, Value>,
    issues: &mut Vec<PersistedInstanceIssue>,
) {
    let Some(value) = instance.get("quickPaths") else {
        return;
    };
    let Some(items) = value.as_array() else {
        instance.insert("quickPaths".to_string(), Value::Array(Vec::new()));
        issues.push(invalid_section(
            "/quickPaths",
            "invalid quickPaths were reset to the current empty default",
        ));
        return;
    };
    let mut kept = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let entry = serde_json::from_value::<InstanceQuickPath>(item.clone())
            .ok()
            .and_then(|mut entry| {
                validation::validate_instance_quick_paths(std::slice::from_mut(&mut entry))
                    .ok()
                    .map(|()| entry)
            });
        match entry {
            Some(entry) if kept.len() < validation::MAX_INSTANCE_QUICK_PATHS => {
                kept.push(serde_json::to_value(entry).unwrap_or(Value::Null));
            }
            _ => issues.push(invalid_field(
                &json_pointer_child("/quickPaths", &index.to_string()),
                "an unusable quick path was removed",
            )),
        }
    }
    instance.insert("quickPaths".to_string(), Value::Array(kept));
}

/// An environment variable that could not be passed to a process safely is
/// dropped alone; the injection checks themselves never relax.
fn normalize_env_vars(instance: &mut Map<String, Value>, issues: &mut Vec<PersistedInstanceIssue>) {
    let Some(value) = instance.get("envVars") else {
        instance.insert("envVars".to_string(), Value::Object(Map::new()));
        issues.push(missing_field(
            "/envVars",
            "missing envVars received the current empty default",
        ));
        return;
    };
    let Some(entries) = value.as_object() else {
        instance.insert("envVars".to_string(), Value::Object(Map::new()));
        issues.push(invalid_section(
            "/envVars",
            "invalid envVars were reset to the current empty default",
        ));
        return;
    };
    let mut kept = Map::new();
    for (key, value) in entries {
        let valid = value.as_str().is_some_and(|text| {
            validation::validate_instance_env_vars(&BTreeMap::from([(
                key.clone(),
                text.to_string(),
            )]))
            .is_ok()
        });
        if valid {
            kept.insert(key.clone(), value.clone());
        } else {
            issues.push(invalid_field(
                &json_pointer_child("/envVars", key),
                "an environment variable that cannot be passed safely was removed",
            ));
        }
    }
    instance.insert("envVars".to_string(), Value::Object(kept));
}

fn normalize_settings_overrides(
    paths: &AppPaths,
    instance: &mut Map<String, Value>,
    issues: &mut Vec<PersistedInstanceIssue>,
) -> Result<(), String> {
    let Some(value) = instance.get_mut("settingsOverrides") else {
        instance.insert("settingsOverrides".to_string(), Value::Object(Map::new()));
        issues.push(missing_field(
            "/settingsOverrides",
            "missing settingsOverrides received the current empty default",
        ));
        return Ok(());
    };
    if value.is_null() {
        *value = Value::Object(Map::new());
        issues.push(invalid_section(
            "/settingsOverrides",
            "null settingsOverrides were normalized to the current empty default",
        ));
        return Ok(());
    }
    let Some(overrides) = value.as_object_mut() else {
        *value = Value::Object(Map::new());
        issues.push(invalid_section(
            "/settingsOverrides",
            "invalid settingsOverrides were reset to the current empty default",
        ));
        return Ok(());
    };

    let current = serde_json::to_value(AppSettings::defaults(paths))
        .map_err(|error| format!("serialize current settings schema: {error}"))?;
    let current = current
        .as_object()
        .ok_or_else(|| "current settings schema is not an object".to_string())?;
    let unsupported: Vec<String> = overrides
        .keys()
        .filter(|key| {
            !current.contains_key(*key) || settings::validation::is_instance_override_forbidden(key)
        })
        .cloned()
        .collect();
    for key in unsupported {
        overrides.remove(&key);
        issues.push(unsupported_field(
            &json_pointer_child("/settingsOverrides", &key),
            "unsupported instance settings override was removed",
        ));
    }

    let invalid: Vec<String> = overrides
        .iter()
        .filter(|(key, value)| {
            let single = Map::from_iter([((*key).clone(), (*value).clone())]);
            settings::validate_settings_overrides(paths, &single).is_err()
        })
        .map(|(key, _)| key.clone())
        .collect();
    for key in invalid {
        overrides.remove(&key);
        issues.push(invalid_field(
            &json_pointer_child("/settingsOverrides", &key),
            "an invalid instance settings override was removed",
        ));
    }
    // Each remaining override is valid on its own; one that only fails in
    // combination with another leaves no override to prefer, so all go.
    if settings::validate_settings_overrides(paths, overrides).is_err() {
        *overrides = Map::new();
        issues.push(invalid_section(
            "/settingsOverrides",
            "invalid current settings overrides were reset to the current empty default",
        ));
    }
    Ok(())
}

fn normalize_launch_args(
    instance: &mut Map<String, Value>,
    issues: &mut Vec<PersistedInstanceIssue>,
) {
    if instance.get("launchArgs").is_some_and(valid_launch_args) {
        return;
    }
    let missing = !instance.contains_key("launchArgs");
    instance.insert("launchArgs".to_string(), Value::Array(Vec::new()));
    issues.push(if missing {
        missing_field(
            "/launchArgs",
            "missing launchArgs received the current empty default",
        )
    } else {
        invalid_section(
            "/launchArgs",
            "invalid launchArgs were reset to the current empty default",
        )
    });
}

/// The origin decides whether a built-in refresh may replace the arguments.
/// One this version cannot read falls back to the origin that keeps what is
/// stored: stored arguments are the user's, and an empty list awaits the
/// built-in defaults.
fn normalize_launch_argument_origin(
    instance: &mut Map<String, Value>,
    issues: &mut Vec<PersistedInstanceIssue>,
) {
    let has_args = instance
        .get("launchArgs")
        .and_then(Value::as_array)
        .is_some_and(|args| !args.is_empty());
    let origin = instance
        .get("launchArgumentOrigin")
        .and_then(|value| serde_json::from_value::<LaunchArgumentOrigin>(value.clone()).ok());
    let issue = match origin {
        Some(LaunchArgumentOrigin::PendingDefault) if has_args => invalid_field(
            "/launchArgumentOrigin",
            "pending default launch arguments cannot hold arguments; they are kept as the user's",
        ),
        Some(_) => return,
        None if !instance.contains_key("launchArgumentOrigin") => missing_field(
            "/launchArgumentOrigin",
            "missing launchArgumentOrigin was derived from the stored launch arguments",
        ),
        None => invalid_field(
            "/launchArgumentOrigin",
            "unsupported launchArgumentOrigin was derived from the stored launch arguments",
        ),
    };
    let fallback = if has_args {
        LaunchArgumentOrigin::User
    } else {
        LaunchArgumentOrigin::PendingDefault
    };
    instance.insert(
        "launchArgumentOrigin".to_string(),
        serde_json::to_value(fallback).unwrap_or(Value::Null),
    );
    issues.push(issue);
}

fn valid_launch_args(value: &Value) -> bool {
    value.as_array().is_some_and(|values| {
        values
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()
            .is_some_and(|arguments| launch_arguments_within_bounds(&arguments))
    })
}

fn missing_field(pointer: &str, message: &str) -> PersistedInstanceIssue {
    PersistedInstanceIssue {
        kind: PersistedInstanceIssueKind::MissingField,
        json_pointer: pointer.to_string(),
        message: message.to_string(),
    }
}

fn unsupported_field(pointer: &str, message: &str) -> PersistedInstanceIssue {
    PersistedInstanceIssue {
        kind: PersistedInstanceIssueKind::UnsupportedField,
        json_pointer: pointer.to_string(),
        message: message.to_string(),
    }
}

fn invalid_field(pointer: &str, message: &str) -> PersistedInstanceIssue {
    PersistedInstanceIssue {
        kind: PersistedInstanceIssueKind::InvalidField,
        json_pointer: pointer.to_string(),
        message: message.to_string(),
    }
}

fn invalid_section(pointer: &str, message: &str) -> PersistedInstanceIssue {
    PersistedInstanceIssue {
        kind: PersistedInstanceIssueKind::InvalidSection,
        json_pointer: pointer.to_string(),
        message: message.to_string(),
    }
}
