//! A preset as it is written: an extension's TOML or JSON file, or an entry
//! of the user's presets file. Decoded tolerantly (a field this version
//! does not know is ignored); what it holds is checked once here, apart
//! from its base type's form, which is checked whenever it is listed
//! (`super::check_against`), since types come and go.

use crate::internal::hanafubuki_foundation::form_schema::SchemaText;
use crate::internal::hanafubuki_instance_contracts::{
    InstanceTypeId, LaunchArgumentIssue, MAX_LAUNCH_ARGUMENTS, launch_argument_issue,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{InstancePreset, LaunchArgument, LaunchValue, PresetName};
use crate::internal::hanafubuki_webui::instance_types::DescriptorIssue;
use crate::internal::hanafubuki_webui::instance_types::declarative::{
    TextDocument, line_and_column, text,
};

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresetDocument {
    /// An extension's file names its preset; a user entry is named by its
    /// key.
    #[serde(default)]
    id: Option<String>,
    /// Registered instance type id (<namespace>:<name>), never another preset. An unavailable base keeps this preset listed but unusable.
    base: String,
    /// Display label, literal/localized or a message key belonging to this extension.
    label: TextDocument,
    #[serde(default)]
    /// Optional localized description.
    description: Option<TextDocument>,
    #[serde(default)]
    /// Partial defaults for the base type install form. Catalog validation rejects unknown fields, invalid values and secret fields; missing required fields remain for the user.
    install: Map<String, Value>,
    #[serde(default)]
    /// Launch options applied at creation; the base type must expose launch-arguments when options are supplied.
    launch: LaunchDocument,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Default, Deserialize, Serialize)]
struct LaunchDocument {
    #[serde(default)]
    /// Named options with flag, numeric or text values. Option names start with a dash and contain no whitespace; values are single process arguments.
    arguments: Vec<LaunchArgument>,
}

impl InstancePreset {
    /// An extension's preset from a TOML file, its texts confined to the
    /// extension's messages (`namespace`).
    pub fn from_toml(namespace: &str, text: &str) -> Result<Self, Vec<DescriptorIssue>> {
        let document: PresetDocument = toml::from_str(text).map_err(|error| {
            let location = error
                .span()
                .map(|span| line_and_column(text, span.start))
                .unwrap_or_else(|| "preset".to_string());
            vec![issue(location, error.message())]
        })?;
        extension_preset(namespace, document)
    }

    /// An extension's preset from a JSON file.
    pub fn from_json(namespace: &str, text: &str) -> Result<Self, Vec<DescriptorIssue>> {
        let document: PresetDocument = serde_json::from_str(text).map_err(|error| {
            vec![issue(
                format!("line {}, column {}", error.line(), error.column()),
                error.to_string(),
            )]
        })?;
        extension_preset(namespace, document)
    }

    /// One entry of the user's presets file, named by its key.
    pub(super) fn from_user_entry(key: &str, entry: &Value) -> Result<Self, String> {
        let document = PresetDocument::deserialize(entry).map_err(|error| error.to_string())?;
        let name = PresetName::parse(key)?;
        read(name, document, None).map_err(|issues| {
            issues
                .into_iter()
                .map(|issue| format!("{}: {}", issue.location, issue.message))
                .collect::<Vec<_>>()
                .join("; ")
        })
    }
}

fn extension_preset(
    namespace: &str,
    document: PresetDocument,
) -> Result<InstancePreset, Vec<DescriptorIssue>> {
    let name = match document.id.as_deref() {
        Some(id) => PresetName::parse(id).map_err(|error| vec![issue("id", error)])?,
        None => return Err(vec![issue("id", EXTENSION_ID.message)]),
    };
    read(name, document, Some(namespace))
}

/// The preset `document` describes, every problem at its place. An
/// extension's texts are confined to its namespace, as its types' are.
fn read(
    name: PresetName,
    document: PresetDocument,
    namespace: Option<&str>,
) -> Result<InstancePreset, Vec<DescriptorIssue>> {
    let mut issues = Vec::new();
    let base = InstanceTypeId::parse(&document.base)
        .map_err(|error| issues.push(issue("base", error)))
        .ok();
    let mut scoped = |location: &str, written: TextDocument| {
        let read = text(written);
        match namespace {
            Some(namespace) => read
                .scoped_to(namespace)
                .map_err(|error| issues.push(issue(location, error)))
                .ok(),
            None => Some(read),
        }
    };
    let label = scoped("label", document.label);
    let description = document
        .description
        .map(|description| scoped("description", description));
    if document.launch.arguments.len() > MAX_LAUNCH_ARGUMENTS {
        issues.push(issue(
            "launch.arguments",
            format!("at most {MAX_LAUNCH_ARGUMENTS} arguments"),
        ));
    }
    for (index, argument) in document.launch.arguments.iter().enumerate() {
        if let Err(error) = check_argument(argument) {
            issues.push(issue(format!("launch.arguments[{index}]"), error));
        }
    }
    match (base, label, description) {
        (Some(base), Some(label), description) if issues.is_empty() => Ok(InstancePreset {
            name,
            base,
            label,
            description: description.flatten(),
            install: document.install,
            launch: document.launch.arguments,
        }),
        _ => Err(issues),
    }
}

/// A launch argument's name is an option (`-x`, `--name`) and its value, as
/// text, one process argument.
pub(crate) fn check_argument(argument: &LaunchArgument) -> Result<(), String> {
    let name = &argument.name;
    if !name.starts_with('-') || name.chars().any(char::is_whitespace) {
        return Err(format!("`{name}` is not an option name"));
    }
    let value = match &argument.value {
        LaunchValue::Text(text) => Some(text.as_str()),
        LaunchValue::Flag(_) | LaunchValue::Number(_) => None,
    };
    for part in std::iter::once(name.as_str()).chain(value) {
        let problem = match launch_argument_issue(part) {
            None => continue,
            Some(LaunchArgumentIssue::Empty) => "is empty",
            Some(LaunchArgumentIssue::TooLong) => "exceeds the byte limit",
            Some(LaunchArgumentIssue::Nul) => "holds a NUL",
        };
        return Err(format!("`{name}`: its value {problem}"));
    }
    Ok(())
}

/// A label the user gives a preset: plain text.
pub(super) fn user_label(label: &str) -> Result<SchemaText, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("a preset needs a name".to_string());
    }
    if label.chars().count() > 80 {
        return Err("a preset's name is at most 80 characters".to_string());
    }
    Ok(SchemaText::literal(label))
}

fn issue(location: impl Into<String>, message: impl Into<String>) -> DescriptorIssue {
    DescriptorIssue {
        location: location.into(),
        message: message.into(),
    }
}

const EXTENSION_ID: crate::internal::hanafubuki_foundation::config_documentation::ConfigRule =
    crate::internal::hanafubuki_foundation::config_documentation::ConfigRule {
        id: "preset-extension-id",
        fields: &["id"],
        condition: "Preset is contributed by an extension (not a user-store entry)",
        requirement: "Supply id in extension preset files; user-store entries obtain their id from the store key.",
        message: "a preset names its `id`",
        stage: crate::internal::hanafubuki_foundation::config_documentation::RuleStage::Validation,
        source: "src-tauri/crates/hanafubuki-webui/src/instance_types/presets/document.rs",
    };

/// Schema of an extension preset, using the same document as its TOML/JSON reader.
#[cfg(feature = "schema")]
pub fn preset_schema() -> schemars::Schema {
    let mut schema = schemars::schema_for!(PresetDocument);
    schema.ensure_object()["properties"]["id"]["maxLength"] =
        serde_json::json!(super::PRESET_ID_MAX_BYTES);
    schema.ensure_object()["properties"]["id"]["type"] = serde_json::json!("string");
    schema.ensure_object()["properties"]["id"]
        .as_object_mut()
        .expect("id schema")
        .remove("default");
    schema.ensure_object()["required"]
        .as_array_mut()
        .expect("preset is an object")
        .push(serde_json::json!("id"));
    schema.ensure_object()["properties"]["launch"]["x-hanafubuki-effective-default"] = serde_json::json!({"description":"No launch overrides when omitted", "value":{"arguments":[]}});
    if let Some(arguments) = schema
        .ensure_object()
        .get_mut("$defs")
        .and_then(|defs| defs.pointer_mut("/LaunchDocument/properties/arguments"))
    {
        arguments["maxItems"] = serde_json::json!(MAX_LAUNCH_ARGUMENTS);
    }
    crate::internal::hanafubuki_foundation::config_documentation::annotate(
        &mut schema,
        &[
            EXTENSION_ID,
            super::documentation::SECRET,
            super::documentation::LAUNCH,
        ],
    );
    schema
}
