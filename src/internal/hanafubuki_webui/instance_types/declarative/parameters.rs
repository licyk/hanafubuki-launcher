//! A declarative type's install parameters: the form-schema fields its
//! descriptor writes, and the values an instance gives them (its spec),
//! read through the schema's one value reader.

use crate::internal::hanafubuki_foundation::form_schema::{
    FieldKind, PathKind, SchemaField, SchemaOption, SchemaText, SchemaValue, SchemaValues,
    check_fields, read_values,
};
use serde_json::{Map, Value};

use super::DescriptorIssue;
use super::document::{
    OptionDocument, ParameterDocument, ParameterKindDocument, PathKindDocument, TextDocument,
};

/// The values of one instance's install parameters, every declared field
/// with a value or a default present.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclarativeSpec(SchemaValues);

impl DeclarativeSpec {
    /// Reads `object` against `fields`. A field the type does not declare is
    /// refused when `strict` (a request) and dropped otherwise (a persisted
    /// spec, which outlives a type that stopped taking an option).
    pub(super) fn read(
        fields: &[SchemaField],
        object: Map<String, Value>,
        strict: bool,
    ) -> Result<Self, String> {
        read_values(fields, object, strict)
            .map(Self)
            .map_err(|error| format!("install parameters: {error}"))
    }

    pub fn to_object(&self) -> Map<String, Value> {
        self.0
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    serde_json::to_value(value).unwrap_or(Value::Null),
                )
            })
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<&SchemaValue> {
        self.0.get(name)
    }

    /// A parameter's value as a placeholder renders it; none when unset.
    pub(super) fn text(&self, name: &str) -> Option<String> {
        self.0.get(name).map(SchemaValue::text)
    }
}

/// The install form a descriptor writes, its texts confined to `namespace`
/// and checked as a form from outside the application; each problem is
/// added to `issues` at the parameter it is about.
pub(super) fn fields(
    issues: &mut Vec<DescriptorIssue>,
    namespace: &str,
    declared: Vec<ParameterDocument>,
) -> Vec<SchemaField> {
    let add = |issues: &mut Vec<DescriptorIssue>, location: String, message: String| {
        issues.push(DescriptorIssue { location, message })
    };
    let mut fields = Vec::new();
    for (index, mut document) in declared.into_iter().enumerate() {
        let default = document.default.take();
        let written = parameter(document);
        // A field whose text is refused is kept as written, so the steps
        // naming it are checked all the same.
        let mut field = written
            .clone()
            .scoped_to(namespace)
            .unwrap_or_else(|error| {
                add(issues, format!("install.parameters[{index}]"), error);
                written
            });
        if let Some(default) = default {
            match field.read(&default) {
                Ok(default) => field.default = Some(default),
                Err(error) => add(
                    issues,
                    format!("install.parameters[{index}].default"),
                    error,
                ),
            }
        }
        fields.push(field);
    }
    for issue in check_fields(&fields) {
        add(
            issues,
            format!("install.parameters[{}]", issue.field),
            issue.message,
        );
    }
    fields
}

fn parameter(document: ParameterDocument) -> SchemaField {
    let kind = match document.kind {
        ParameterKindDocument::Text => FieldKind::Text,
        ParameterKindDocument::Number => FieldKind::Number,
        ParameterKindDocument::Boolean => FieldKind::Boolean,
        ParameterKindDocument::Select => FieldKind::Select,
        ParameterKindDocument::Multiselect => FieldKind::Multiselect,
        ParameterKindDocument::Path => FieldKind::Path,
        ParameterKindDocument::Secret => FieldKind::Secret,
        ParameterKindDocument::Slider => FieldKind::Slider,
        ParameterKindDocument::Textarea => FieldKind::Textarea,
    };
    let label = document
        .label
        .map(text)
        .unwrap_or_else(|| SchemaText::literal(document.name.clone()));
    let mut field = SchemaField::new(document.name, kind, label);
    field.help = document.help.map(text);
    field.required = document.required;
    field.options = document
        .options
        .into_iter()
        .map(|option| match option {
            OptionDocument::Value(value) => SchemaOption {
                label: SchemaText::literal(value.clone()),
                value,
            },
            OptionDocument::Labelled { value, label } => SchemaOption {
                value,
                label: text(label),
            },
        })
        .collect();
    field.min = document.min;
    field.max = document.max;
    field.step = document.step;
    if kind == FieldKind::Path {
        field.path_kind = Some(match document.path {
            Some(PathKindDocument::File) => PathKind::File,
            Some(PathKindDocument::Directory) | None => PathKind::Directory,
        });
    }
    field.advanced = document.advanced;
    field.required_when = document.required_when;
    field.disabled_when = document.disabled_when;
    field.disabled_reason = document.disabled_reason.map(text);
    field.visible_when = document.visible_when;
    field
}

/// Plain text is the same in every locale; a map gives it by locale. A key
/// is confined to the type's namespace afterwards (`scoped_texts`).
pub(in crate::internal::hanafubuki_webui::instance_types) fn text(
    text: TextDocument,
) -> SchemaText {
    match text {
        TextDocument::Plain(text) => SchemaText::literal(text),
        TextDocument::Key { key, params } => SchemaText::key_with(key, params),
        TextDocument::Localized(texts) => SchemaText::localized(texts),
    }
}
