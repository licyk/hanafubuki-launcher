//! Decoding of `lifecycle.json`.
//!
//! A record names its type (`type`) beside a `spec` object only that type's
//! provider reads. A record written before type ids carries the built-in
//! family as `spec.type` instead and reads as that family's type; the next
//! write gives it the current shape. A record whose type has no provider
//! is kept as it is and reads as unavailable.

use crate::internal::hanafubuki_instance_contracts::{InstanceTypeId, WebUiFamily};
use serde::Deserialize;
use serde_json::Value;

use crate::internal::hanafubuki_webui::instance_types::presets::{LaunchArgument, check_argument};
use crate::internal::hanafubuki_webui::instance_types::{BuiltinProvider, InstanceTypeRegistry};
use crate::internal::hanafubuki_webui::instances::{
    InstanceSpec, WebUiInstanceInstallSource, WebUiInstanceLifecycle, WebUiInstanceRecord,
};

pub(super) struct DecodedRecord {
    pub record: WebUiInstanceRecord,
    /// Reading dropped something the record held, so it is rewritten (after
    /// a quarantine copy). Moving a record to the current shape drops
    /// nothing; neither does reading one whose type is unavailable.
    pub repaired: bool,
}

/// Every field of a record besides its type and spec.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordFields {
    instance_id: String,
    instance_identity: String,
    install_source: WebUiInstanceInstallSource,
    lifecycle: WebUiInstanceLifecycle,
    created_at: u128,
    updated_at: u128,
}

const RECORD_FIELDS: [&str; 8] = [
    "instanceId",
    "instanceIdentity",
    "type",
    "spec",
    "installSource",
    "lifecycle",
    "createdAt",
    "updatedAt",
];

/// The record, or why it cannot be used (the caller quarantines it).
pub(super) fn decode_record(
    types: &InstanceTypeRegistry,
    raw: &Value,
) -> Result<DecodedRecord, String> {
    let mut current = raw
        .as_object()
        .cloned()
        .ok_or_else(|| "its root is not an object".to_string())?;
    let mut spec = match current.get("spec") {
        Some(Value::Object(spec)) => spec.clone(),
        _ => return Err("its spec is not an object".to_string()),
    };
    let type_id = match current.get("type") {
        Some(value) => InstanceTypeId::deserialize(value)
            .map_err(|error| format!("its type is invalid: {error}"))?,
        None => legacy_type_id(spec.remove("type"))?,
    };
    current.insert("type".to_string(), Value::String(type_id.to_string()));
    current.insert("spec".to_string(), Value::Object(spec.clone()));

    let mut known = current.clone();
    known.retain(|key, _| RECORD_FIELDS.contains(&key.as_str()));
    let fields = RecordFields::deserialize(Value::Object(known))
        .map_err(|error| format!("current fields are invalid: {error}"))?;
    let (spec, available) = match types.get(&type_id) {
        Some(provider) => (
            provider
                .read_spec(spec)
                .map_err(|error| format!("its spec is invalid: {error}"))?,
            true,
        ),
        None => (InstanceSpec::Unavailable(spec), false),
    };
    let record = WebUiInstanceRecord {
        instance_id: fields.instance_id,
        instance_identity: fields.instance_identity,
        type_id,
        spec,
        install_source: fields.install_source,
        lifecycle: fields.lifecycle,
        created_at: fields.created_at,
        updated_at: fields.updated_at,
        // Display only: a value that is not text is dropped.
        preset_id: current
            .get("presetId")
            .and_then(Value::as_str)
            .map(str::to_string),
        preset_launch_arguments: preset_launch_arguments(current.get("presetLaunchArguments")),
    };
    let repaired = available
        && serde_json::to_value(&record)
            .map_err(|error| format!("serialize WebUI instance lifecycle: {error}"))?
            != Value::Object(current);
    Ok(DecodedRecord { record, repaired })
}

/// The launch arguments of the preset the instance was made from; a value
/// that does not read as a preset's arguments drops them all, so the
/// instance's launch arguments follow the defaults alone.
fn preset_launch_arguments(value: Option<&Value>) -> Vec<LaunchArgument> {
    value
        .and_then(|value| Vec::<LaunchArgument>::deserialize(value).ok())
        .filter(|arguments| {
            arguments
                .iter()
                .all(|argument| check_argument(argument).is_ok())
        })
        .unwrap_or_default()
}

fn legacy_type_id(family: Option<Value>) -> Result<InstanceTypeId, String> {
    family
        .and_then(|family| WebUiFamily::deserialize(family).ok())
        .map(BuiltinProvider::type_id)
        .ok_or_else(|| "it names neither a type nor a known WebUI family".to_string())
}
