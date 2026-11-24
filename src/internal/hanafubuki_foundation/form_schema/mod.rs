//! A form's fields as the application describes them: one schema for the
//! settings pages and for every instance type's install form.
//!
//! Rust declares the fields and validates the values once, where they enter
//! (`read_values`); the frontend renders the fields, evaluates the same
//! conditions to show them, and reports what Rust answered. The types here
//! are what the frontend reads (`src/types/schema.ts` mirrors them).

mod check;
mod values;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

pub use check::{SchemaIssue, check_fields};
pub use values::{SchemaValues, patched_values, read_values, stored_values};

/// Text a form shows: a message key of the frontend's locale tables, or
/// text by locale. Decoded where a form comes from outside the application
/// (an extension's settings), whose keys [`SchemaText::scoped_to`] confines.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SchemaText {
    Key {
        key: String,
        /// Named interpolation values for the message.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        params: BTreeMap<String, String>,
    },
    /// By locale (`en`, `zh`, …). A locale it lacks shows the English, and
    /// text with no English shows the first locale it has.
    Literal { text: BTreeMap<String, String> },
}

impl SchemaText {
    pub fn key(key: impl Into<String>) -> Self {
        Self::Key {
            key: key.into(),
            params: BTreeMap::new(),
        }
    }

    pub fn key_with(key: impl Into<String>, params: BTreeMap<String, String>) -> Self {
        Self::Key {
            key: key.into(),
            params,
        }
    }

    /// The same text in every locale, such as a product name.
    pub fn literal(text: impl Into<String>) -> Self {
        Self::Literal {
            text: BTreeMap::from([("en".to_string(), text.into())]),
        }
    }

    /// Text by locale; an empty map is no text.
    pub fn localized(text: BTreeMap<String, String>) -> Self {
        Self::Literal { text }
    }

    /// The text of an extension whose message namespace is `namespace`: a key
    /// it writes `ext.<namespace>.<key>` becomes the key the frontend merged
    /// its strings under (`ext['<namespace>'].<key>`), and any other key, an
    /// application key or another extension's, is refused. Literal text
    /// stays as it is.
    pub fn scoped_to(self, namespace: &str) -> Result<Self, String> {
        match self {
            Self::Key { key, params } => {
                let own = key
                    .strip_prefix("ext.")
                    .and_then(|rest| rest.strip_prefix(namespace))
                    .and_then(|rest| rest.strip_prefix('.'))
                    .filter(|rest| !rest.is_empty());
                match own {
                    Some(rest) => Ok(Self::Key {
                        key: format!("ext['{namespace}'].{rest}"),
                        params,
                    }),
                    None => Err(format!("message key `{key}` is outside `ext.{namespace}.`")),
                }
            }
            literal @ Self::Literal { .. } => Ok(literal),
        }
    }

    /// A literal's English, or its first locale's text without one; none
    /// for a message key, which only the frontend's tables resolve.
    pub fn literal_text(&self) -> Option<&str> {
        match self {
            Self::Key { .. } => None,
            Self::Literal { text } => text
                .get("en")
                .or_else(|| text.values().next())
                .map(String::as_str),
        }
    }
}

/// What a field edits, and so which control shows it.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    /// Within `min`/`max`, on `step` from `min` when given.
    Number,
    Boolean,
    /// One of `options`.
    Select,
    /// Any of `options`, as a list.
    Multiselect,
    /// A directory or a file (`pathKind`).
    Path,
    /// Text the control does not show.
    Secret,
    /// A number between `min` and `max`, which a slider needs.
    Slider,
    /// Text of several lines.
    Textarea,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathKind {
    #[default]
    Directory,
    File,
}

/// When a settings editor may submit a field. Rust owns this because it owns
/// validation: only a value that cannot be partial commits without a Save.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitMode {
    /// A discrete value; committing on change never sees a partial one.
    Change,
    /// Free text or a number; once the field is left or Enter is pressed.
    Blur,
    /// Behind an explicit Save: persisting it has a side effect an
    /// intermediate value would make visible.
    Manual,
}

/// A value a field holds.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SchemaValue {
    Boolean(bool),
    Number(Number),
    Text(String),
    List(Vec<String>),
}

impl SchemaValue {
    /// The value as a placeholder renders it: a list joined with commas.
    pub fn text(&self) -> String {
        match self {
            Self::Boolean(value) => value.to_string(),
            Self::Number(value) => value.to_string(),
            Self::Text(value) => value.clone(),
            Self::List(values) => values.join(","),
        }
    }
}

/// When a condition holds, over the form's current values: a field equal to
/// a value, or any or all of several conditions. Decoded where a form comes
/// from outside the application (a descriptor), and checked there.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SchemaCondition {
    Equals { key: String, equals: SchemaValue },
    Any { any: Vec<SchemaCondition> },
    All { all: Vec<SchemaCondition> },
}

impl SchemaCondition {
    pub fn equals(key: impl Into<String>, value: SchemaValue) -> Self {
        Self::Equals {
            key: key.into(),
            equals: value,
        }
    }

    pub fn matches(&self, values: &SchemaValues) -> bool {
        match self {
            Self::Equals { key, equals } => values.get(key) == Some(equals),
            Self::Any { any } => any.iter().any(|condition| condition.matches(values)),
            Self::All { all } => all.iter().all(|condition| condition.matches(values)),
        }
    }

    /// The fields the condition reads.
    fn keys<'a>(&'a self, keys: &mut Vec<&'a str>) {
        match self {
            Self::Equals { key, .. } => keys.push(key),
            Self::Any { any: conditions } | Self::All { all: conditions } => {
                for condition in conditions {
                    condition.keys(keys);
                }
            }
        }
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaOption {
    pub value: String,
    pub label: SchemaText,
}

/// One field of a form. It decodes from what it serializes to, tolerantly
/// (an unknown field is ignored), for a form written outside the
/// application; [`check_fields`] then checks it once.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaField {
    pub key: String,
    pub kind: FieldKind,
    pub label: SchemaText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<SchemaText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SchemaOption>,
    /// The name of an option list the form's host fetches at run time (an
    /// install catalog's `branches`), offered after `options`. Rust checks
    /// the value against the same list when it uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_kind: Option<PathKind>,
    /// No value is a value the user can choose: a select or a boolean offers
    /// it as an entry (`unsetLabel`), text as a blank.
    #[serde(default, skip_serializing_if = "is_false")]
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unset_label: Option<SchemaText>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<SchemaValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_when: Option<SchemaCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_when: Option<SchemaCondition>,
    /// While the condition does not hold the field is hidden and its value
    /// is not submitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_when: Option<SchemaCondition>,
    /// Shown while `disabledWhen` holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<SchemaText>,
    /// It has a working default, so a creation form keeps it out of the way.
    #[serde(default, skip_serializing_if = "is_false")]
    pub advanced: bool,
    /// Settings only: when its editor submits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitMode>,
    /// Settings only: the fields Rust validates together, which an editor
    /// submits together rather than restating the rule that binds them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_group: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl SchemaField {
    /// [`SchemaText::scoped_to`] over every text the field shows.
    pub fn scoped_to(mut self, namespace: &str) -> Result<Self, String> {
        let scope = |text: SchemaText| text.scoped_to(namespace);
        self.label = scope(self.label)?;
        self.help = self.help.map(scope).transpose()?;
        self.unset_label = self.unset_label.map(scope).transpose()?;
        self.disabled_reason = self.disabled_reason.map(scope).transpose()?;
        for option in &mut self.options {
            option.label = scope(option.label.clone())?;
        }
        Ok(self)
    }

    pub fn new(key: impl Into<String>, kind: FieldKind, label: SchemaText) -> Self {
        Self {
            key: key.into(),
            kind,
            label,
            help: None,
            options: Vec::new(),
            options_source: None,
            min: None,
            max: None,
            step: None,
            path_kind: None,
            nullable: false,
            unset_label: None,
            required: false,
            default: None,
            required_when: None,
            disabled_when: None,
            visible_when: None,
            disabled_reason: None,
            advanced: false,
            commit: None,
            patch_group: None,
        }
    }

    pub fn help(mut self, help: SchemaText) -> Self {
        self.help = Some(help);
        self
    }

    pub fn commit(mut self, commit: CommitMode) -> Self {
        self.commit = Some(commit);
        self
    }

    pub fn options(mut self, options: Vec<SchemaOption>) -> Self {
        self.options = options;
        self
    }

    pub fn bounds(mut self, min: impl Into<Number>, max: impl Into<Number>) -> Self {
        self.min = Some(min.into());
        self.max = Some(max.into());
        self
    }

    /// No value is selectable, and `unset` names it.
    pub fn unset(mut self, unset: SchemaText) -> Self {
        self.nullable = true;
        self.unset_label = Some(unset);
        self
    }

    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    pub fn disabled_when(mut self, condition: SchemaCondition, reason: SchemaText) -> Self {
        self.disabled_when = Some(condition);
        self.disabled_reason = Some(reason);
        self
    }

    pub fn patch_group(mut self, group: impl Into<String>) -> Self {
        self.patch_group = Some(group.into());
        self
    }

    pub fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }
}

/// Fields under one heading: a settings page's section, or a part of an
/// install form the form lays out on its own (its Python environment).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SchemaGroup {
    pub id: String,
    pub label: SchemaText,
    pub fields: Vec<SchemaField>,
}

#[cfg(test)]
mod tests;
