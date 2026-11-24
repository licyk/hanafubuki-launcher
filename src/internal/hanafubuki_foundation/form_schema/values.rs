use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{FieldKind, SchemaField, SchemaValue};

/// A form's values by field key.
pub type SchemaValues = BTreeMap<String, SchemaValue>;

/// Reads a form's values from `object`, once, where they enter.
///
/// A field takes its value or its default; `null`, empty text and an empty
/// list are no value. A key the
/// form does not declare is refused when `strict` (a request) and dropped
/// otherwise (a persisted value, which outlives a form that stopped taking
/// it). With every value read, a field whose `visibleWhen` does not hold
/// loses its value, and a field that is required (`required`, or
/// `requiredWhen` holding) and shown must have one.
pub fn read_values(
    fields: &[SchemaField],
    mut object: Map<String, Value>,
    strict: bool,
) -> Result<SchemaValues, String> {
    if strict {
        let undeclared: Vec<&str> = object
            .keys()
            .filter(|key| !fields.iter().any(|field| &field.key == *key))
            .map(String::as_str)
            .collect();
        if !undeclared.is_empty() {
            return Err(format!("undeclared fields: {}", undeclared.join(", ")));
        }
    }
    let mut values = SchemaValues::new();
    for field in fields {
        let value = match object.remove(&field.key) {
            None | Some(Value::Null) => None,
            Some(value) => Some(field.read(&value)?),
        }
        .or_else(|| field.default.clone())
        .filter(|value| !is_empty(value));
        if let Some(value) = value {
            values.insert(field.key.clone(), value);
        }
    }
    let read = values.clone();
    for field in fields {
        if field
            .visible_when
            .as_ref()
            .is_some_and(|condition| !condition.matches(&read))
        {
            values.remove(&field.key);
            continue;
        }
        let required = field.required
            || field
                .required_when
                .as_ref()
                .is_some_and(|condition| condition.matches(&read));
        if required && !values.contains_key(&field.key) {
            return Err(format!("`{}` is required", field.key));
        }
    }
    Ok(values)
}

/// The values a persisted `object` holds that its fields take, as JSON: a
/// value its field does not take, and a key the form does not declare, is
/// left out, so the form shows that field's default. The stored settings of
/// an extension or a module, read against its form.
pub fn stored_values(fields: &[SchemaField], object: &Map<String, Value>) -> Map<String, Value> {
    fields
        .iter()
        .filter_map(|field| {
            let value = field.read(object.get(&field.key)?).ok()?;
            Some((field.key.clone(), json(&value)))
        })
        .collect()
}

/// `patch` (a field's value, or `null` to unset it) over the `stored`
/// values, read as a request: a key the form does not declare is refused,
/// and the whole form must read ([`read_values`], strict). Returns what it
/// read, as JSON, which is what is written.
pub fn patched_values(
    fields: &[SchemaField],
    mut stored: Map<String, Value>,
    patch: Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    for (key, value) in patch {
        if !fields.iter().any(|field| field.key == key) {
            return Err(format!("undeclared fields: {key}"));
        }
        match value {
            Value::Null => stored.remove(&key),
            value => stored.insert(key, value),
        };
    }
    Ok(read_values(fields, stored, true)?
        .into_iter()
        .map(|(key, value)| (key, json(&value)))
        .collect())
}

fn json(value: &SchemaValue) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn is_empty(value: &SchemaValue) -> bool {
    match value {
        SchemaValue::Text(text) => text.is_empty(),
        SchemaValue::List(list) => list.is_empty(),
        SchemaValue::Boolean(_) | SchemaValue::Number(_) => false,
    }
}

impl SchemaField {
    /// `value` as this field's kind takes it.
    pub fn read(&self, value: &Value) -> Result<SchemaValue, String> {
        let read = match (self.kind, value) {
            (
                FieldKind::Text | FieldKind::Secret | FieldKind::Textarea | FieldKind::Path,
                Value::String(text),
            ) => Some(SchemaValue::Text(text.clone())),
            (FieldKind::Number | FieldKind::Slider, Value::Number(number)) => {
                return self.read_number(number);
            }
            (FieldKind::Boolean, Value::Bool(value)) => Some(SchemaValue::Boolean(*value)),
            (FieldKind::Select, Value::String(text)) if self.offers(text) => {
                Some(SchemaValue::Text(text.clone()))
            }
            (FieldKind::Multiselect, Value::Array(items)) => items
                .iter()
                .map(|item| item.as_str().filter(|text| self.offers(text)))
                .map(|text| text.map(str::to_string))
                .collect::<Option<Vec<_>>>()
                .map(SchemaValue::List),
            _ => None,
        };
        read.ok_or_else(|| format!("`{}` takes {}", self.key, self.expected()))
    }

    fn read_number(&self, number: &serde_json::Number) -> Result<SchemaValue, String> {
        let value = number.as_f64().unwrap_or(f64::NAN);
        let (min, max, step) = self.number_bounds();
        let below = min.is_some_and(|min| value < min);
        let above = max.is_some_and(|max| value > max);
        let off_step = step.is_some_and(|step| {
            let steps = (value - min.unwrap_or(0.0)) / step;
            (steps - steps.round()).abs() > 1e-9
        });
        if value.is_nan() || below || above || off_step {
            return Err(format!("`{}` takes {}", self.key, self.expected()));
        }
        Ok(SchemaValue::Number(number.clone()))
    }

    /// `min`, `max` and `step` as numbers to compare with.
    pub(super) fn number_bounds(&self) -> (Option<f64>, Option<f64>, Option<f64>) {
        let read = |bound: &Option<serde_json::Number>| bound.as_ref().and_then(|n| n.as_f64());
        (read(&self.min), read(&self.max), read(&self.step))
    }

    /// Whether a select offers `value`: one of its options, or anything when
    /// its host lists them at run time.
    fn offers(&self, value: &str) -> bool {
        self.options_source.is_some() || self.options.iter().any(|option| option.value == value)
    }

    fn expected(&self) -> String {
        let options = || {
            self.options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self.kind {
            FieldKind::Text | FieldKind::Secret | FieldKind::Textarea => "text".to_string(),
            FieldKind::Path => "a path".to_string(),
            FieldKind::Boolean => "true or false".to_string(),
            FieldKind::Select => format!("one of {}", options()),
            FieldKind::Multiselect => format!("a list of {}", options()),
            FieldKind::Number | FieldKind::Slider => {
                let mut expected = "a number".to_string();
                match (&self.min, &self.max) {
                    (Some(min), Some(max)) => expected.push_str(&format!(" from {min} to {max}")),
                    (Some(min), None) => expected.push_str(&format!(" of at least {min}")),
                    (None, Some(max)) => expected.push_str(&format!(" of at most {max}")),
                    (None, None) => {}
                }
                if let Some(step) = &self.step {
                    expected.push_str(&format!(" in steps of {step}"));
                }
                expected
            }
        }
    }
}
