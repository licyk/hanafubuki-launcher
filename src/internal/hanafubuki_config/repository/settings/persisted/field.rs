//! Field-by-field reading of one persisted settings object.
//!
//! Every field decodes on its own: a field that is missing or invalid takes
//! its default and is reported, and every other field keeps what the user
//! configured. Nothing here decides what a default is or which fields belong
//! together; each domain decoder does.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::internal::hanafubuki_foundation::json_normalization::json_pointer_child;

use super::{PersistedSettingsIssue, PersistedSettingsIssueKind};

/// What one field held.
pub(super) enum FieldRead<T> {
    Valid(T),
    Missing,
    Invalid,
}

/// Reads the fields of one JSON object, recording what it had to default.
pub(super) struct FieldReader<'a> {
    object: &'a Map<String, Value>,
    pointer: String,
    issues: &'a mut Vec<PersistedSettingsIssue>,
}

impl<'a> FieldReader<'a> {
    pub fn new(
        object: &'a Map<String, Value>,
        issues: &'a mut Vec<PersistedSettingsIssue>,
    ) -> Self {
        Self::nested(object, "", issues)
    }

    /// A reader for an object nested at `pointer`, reporting under it.
    pub fn nested(
        object: &'a Map<String, Value>,
        pointer: &str,
        issues: &'a mut Vec<PersistedSettingsIssue>,
    ) -> Self {
        Self {
            object,
            pointer: pointer.to_string(),
            issues,
        }
    }

    pub fn pointer(&self, field: &str) -> String {
        json_pointer_child(&self.pointer, field)
    }

    pub fn raw(&self, field: &str) -> Option<&'a Value> {
        self.object.get(field)
    }

    /// Decode and check a field without reporting anything.
    pub fn read_quiet<T: DeserializeOwned>(
        &self,
        field: &str,
        check: impl FnOnce(T) -> Result<T, String>,
    ) -> FieldRead<T> {
        let Some(value) = self.object.get(field) else {
            return FieldRead::Missing;
        };
        match serde_json::from_value::<T>(value.clone())
            .map_err(|_| ())
            .and_then(|value| check(value).map_err(|_| ()))
        {
            Ok(value) => FieldRead::Valid(value),
            Err(()) => FieldRead::Invalid,
        }
    }

    /// A field whose type is its whole contract.
    pub fn read<T: DeserializeOwned>(&mut self, field: &str, default: T) -> T {
        self.read_checked(field, default, Ok)
    }

    /// A field that must also pass `check`, which may normalize it.
    pub fn read_checked<T: DeserializeOwned>(
        &mut self,
        field: &str,
        default: T,
        check: impl FnOnce(T) -> Result<T, String>,
    ) -> T {
        match self.read_quiet(field, check) {
            FieldRead::Valid(value) => value,
            FieldRead::Missing => {
                self.missing(field);
                default
            }
            FieldRead::Invalid => {
                self.invalid(field, PersistedSettingsIssueKind::InvalidField, None);
                default
            }
        }
    }

    pub fn missing(&mut self, field: &str) {
        let json_pointer = self.pointer(field);
        self.issues.push(PersistedSettingsIssue {
            kind: PersistedSettingsIssueKind::MissingField,
            json_pointer,
            message: "missing field was populated from backend defaults".to_string(),
        });
    }

    /// Report that `field` fell back to its default. `message` replaces the
    /// generic one when the fallback has a consequence the user should read.
    pub fn invalid(
        &mut self,
        field: &str,
        kind: PersistedSettingsIssueKind,
        message: Option<&str>,
    ) {
        let json_pointer = self.pointer(field);
        self.issues.push(PersistedSettingsIssue {
            kind,
            json_pointer,
            message: message
                .unwrap_or("invalid field was replaced with its backend default")
                .to_string(),
        });
    }

    pub fn issues(&mut self) -> &mut Vec<PersistedSettingsIssue> {
        self.issues
    }
}

/// `check` for a value that must lie in `range`.
pub(super) fn within<T: PartialOrd>(
    range: std::ops::RangeInclusive<T>,
) -> impl FnOnce(T) -> Result<T, String> {
    move |value| {
        if range.contains(&value) {
            Ok(value)
        } else {
            Err("value is outside its supported range".to_string())
        }
    }
}
