use serde_json::{Map, Value};

use super::field::FieldReader;
use super::names::{DisabledList, decode_disabled_names};
use super::{AppSettings, PersistedSettingsIssue, PersistedSettingsIssueKind};
use crate::internal::hanafubuki_config::repository::settings::modules::{
    DISABLED_MODULES, MAX_DISABLED_MODULES, ModuleSettings,
};

/// Which application modules run, and their own settings.
pub(super) const FIELDS: &[&str] = &["disabledModules", "modules"];

pub(super) fn decode(
    object: &Map<String, Value>,
    _defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.disabled_modules = decode_disabled_names(
        &mut fields,
        &DISABLED_MODULES,
        DisabledList {
            list: "module",
            item: "module",
        },
    );
    settings.modules = decode_module_settings(&mut fields);
}

/// One module's entry that cannot be one is dropped alone, and that module
/// runs on its defaults.
fn decode_module_settings(fields: &mut FieldReader<'_>) -> ModuleSettings {
    let key = "modules";
    let pointer = fields.pointer(key);
    let Some(raw) = fields.raw(key) else {
        fields.missing(key);
        return ModuleSettings::new();
    };
    let Some(entries) = raw.as_object() else {
        fields.invalid(
            key,
            PersistedSettingsIssueKind::InvalidField,
            Some("the module settings are unreadable; every module runs on its defaults"),
        );
        return ModuleSettings::new();
    };
    let mut kept = ModuleSettings::new();
    let mut nested = FieldReader::nested(entries, &pointer, fields.issues());
    for (name, value) in entries {
        match value.as_object() {
            Some(entry)
                if DISABLED_MODULES.is_valid_name(name) && kept.len() < MAX_DISABLED_MODULES =>
            {
                kept.insert(name.clone(), entry.clone());
            }
            _ => nested.invalid(
                name,
                PersistedSettingsIssueKind::InvalidField,
                Some(
                    "a module's settings that cannot be read were dropped; it runs on its defaults",
                ),
            ),
        }
    }
    kept
}
