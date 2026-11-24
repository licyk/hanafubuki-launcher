use super::InstanceConfig;
use super::validation::validate_instance_config;
use crate::internal::hanafubuki_config::merge;
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use serde_json::{Map, Value};

/// Apply a patch to an existing [`InstanceConfig`] with field-aware null
/// semantics, returning the validated result (without writing it).
///
/// - Normal instance fields (`installDir`, `envVars`) use remove-null, so
///   `installDir: null` clears the optional field and `envVars.KEY: null` deletes
///   that variable.
/// - `settingsOverrides` uses keep-null, so nullable `AppSettings` fields
///   (custom mirrors) can be persisted as an explicit `null` override even when
///   global settings provide a value.
pub(super) fn apply_instance_patch(
    paths: &AppPaths,
    existing: InstanceConfig,
    instance_id: &str,
    patch: Value,
) -> Result<InstanceConfig, String> {
    let mut patch_object = match patch {
        Value::Object(object) => object,
        _ => return Err("instance config patch must be an object".to_string()),
    };
    if patch_object.contains_key("name")
        || patch_object.contains_key("configRevision")
        || patch_object.contains_key("launchArgs")
        || patch_object.contains_key("launchArgumentOrigin")
        || patch_object.contains_key("componentPaths")
    {
        return Err(
            "name, configRevision, launchArgs, launchArgumentOrigin, and componentPaths are Rust-owned; use their dedicated operations"
                .to_string(),
        );
    }
    // Pull the settingsOverrides patch out so it is not subjected to the
    // remove-null merge applied to the rest of the instance fields.
    let overrides_patch = patch_object.remove("settingsOverrides");

    let base = serde_json::to_value(&existing)
        .map_err(|err| format!("serialize instance config: {err}"))?;
    let merged = merge::merge_json_remove_null(base, Value::Object(patch_object));
    let mut config: InstanceConfig = serde_json::from_value(merged)
        .map_err(|err| format!("invalid instance config after patch: {err}"))?;

    if let Some(overrides_patch) = overrides_patch {
        config.settings_overrides =
            apply_settings_overrides_patch(config.settings_overrides, overrides_patch);
    }

    // The id is the storage key and must not be changed by a patch.
    config.id = instance_id.to_string();
    validate_instance_config(paths, &mut config)?;
    Ok(config)
}

/// Merge a `settingsOverrides` patch onto the existing overrides using keep-null
/// semantics, so nullable `AppSettings` fields can be overridden to an explicit
/// `null`. A non-object patch (e.g. `null` to clear everything, or an invalid
/// type) is passed through unchanged for `validate_instance_config` to coerce or
/// reject.
fn apply_settings_overrides_patch(existing: Value, patch: Value) -> Value {
    match patch {
        Value::Object(patch_object) => {
            let base = if existing.is_object() {
                existing
            } else {
                Value::Object(Map::new())
            };
            merge::merge_json_keep_null(base, Value::Object(patch_object))
        }
        other => other,
    }
}
