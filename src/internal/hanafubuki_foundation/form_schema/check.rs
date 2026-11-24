use super::{FieldKind, SchemaField};

/// What is wrong with the field at `field` (its index) of a form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaIssue {
    pub field: usize,
    pub message: String,
}

/// Checks a form whose fields come from outside the application (a type
/// descriptor), once, before it is used: keys of `[A-Za-z0-9_-]`, declared
/// once; a select's options; bounds a number can lie in; defaults the field
/// takes; conditions over declared fields.
pub fn check_fields(fields: &[SchemaField]) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let mut issue = |message: String| {
            issues.push(SchemaIssue {
                field: index,
                message,
            })
        };
        let valid_key = !field.key.is_empty()
            && field
                .key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if !valid_key {
            issue(format!("`{}` is not a name of [A-Za-z0-9_-]", field.key));
        }
        if fields[..index].iter().any(|known| known.key == field.key) {
            issue(format!("`{}` is declared twice", field.key));
        }
        let choices = matches!(field.kind, FieldKind::Select | FieldKind::Multiselect);
        if choices && field.options.is_empty() && field.options_source.is_none() {
            issue("a select lists its options".to_string());
        }
        if field
            .options
            .iter()
            .enumerate()
            .any(|(at, option)| field.options[..at].iter().any(|o| o.value == option.value))
        {
            issue("an option is listed twice".to_string());
        }
        let (min, max, step) = field.number_bounds();
        if let (Some(min), Some(max)) = (min, max) {
            if min > max {
                issue(format!("min {min} is above max {max}"));
            }
        }
        if step.is_some_and(|step| step <= 0.0) {
            issue("step is a positive number".to_string());
        }
        if field.kind == FieldKind::Slider && (field.min.is_none() || field.max.is_none()) {
            issue("a slider has a min and a max".to_string());
        }
        if let Some(default) = &field.default {
            let json = serde_json::to_value(default).unwrap_or_default();
            if let Err(error) = field.read(&json) {
                issue(format!("default: {error}"));
            }
        }
        let mut keys = Vec::new();
        for condition in [
            &field.required_when,
            &field.disabled_when,
            &field.visible_when,
        ]
        .into_iter()
        .flatten()
        {
            condition.keys(&mut keys);
        }
        for key in keys {
            if !fields.iter().any(|known| known.key == key) {
                issue(format!("a condition reads `{key}`, which is not a field"));
            }
        }
    }
    issues
}
