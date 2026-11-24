//! Sets of names the user turned off: `disabledModules`. Each records the user's choice; the host that runs the
//! named rows decides what a name means, so a name nothing carries is valid.

use std::collections::BTreeSet;

use serde_json::Value;

/// The bounds of one set of disabled names, and how its errors name it.
pub(crate) struct DisabledNames {
    /// The settings key.
    pub key: &'static str,
    /// What one name names, in errors (`plugin`, `module`).
    pub item: &'static str,
    pub max_names: usize,
    pub max_name_bytes: usize,
}

impl DisabledNames {
    /// Bounds the set: at most `max_names` names of 1 to `max_name_bytes`
    /// bytes, without surrounding whitespace or control characters.
    pub(crate) fn validate(&self, names: &BTreeSet<String>) -> Result<(), String> {
        let Self {
            key,
            item,
            max_names,
            max_name_bytes,
        } = self;
        if names.len() > *max_names {
            return Err(format!("{key} holds at most {max_names} {item} names"));
        }
        if names.iter().any(|name| !self.is_valid_name(name)) {
            return Err(format!(
                "{key} names must contain 1 to {max_name_bytes} bytes without surrounding whitespace or control characters"
            ));
        }
        Ok(())
    }

    pub(crate) fn is_valid_name(&self, name: &str) -> bool {
        !name.is_empty()
            && name.len() <= self.max_name_bytes
            && name.trim() == name
            && !name.chars().any(char::is_control)
    }

    /// The set a settings patch carries.
    pub(crate) fn parse(&self, value: &Value) -> Result<BTreeSet<String>, String> {
        let names: BTreeSet<String> = serde_json::from_value(value.clone())
            .map_err(|_| format!("{} must be an array of {} names", self.key, self.item))?;
        self.validate(&names)?;
        Ok(names)
    }
}
