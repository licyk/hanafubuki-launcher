//! The application modules the user turned off (`disabledModules`) and
//! each module's own settings (`modules`).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::names::DisabledNames;

/// At most this many names in `disabledModules`.
pub const MAX_DISABLED_MODULES: usize = 64;
/// The longest module name `disabledModules` holds.
pub const MAX_MODULE_NAME_BYTES: usize = 128;

/// `disabledModules`: the application's module host leaves the modules it
/// names unmounted; a module that cannot be turned off ignores it.
pub(crate) const DISABLED_MODULES: DisabledNames = DisabledNames {
    key: "disabledModules",
    item: "module",
    max_names: MAX_DISABLED_MODULES,
    max_name_bytes: MAX_MODULE_NAME_BYTES,
};

/// `modules`: each module's settings object, by module name.
pub type ModuleSettings = BTreeMap<String, Map<String, Value>>;

/// Bounds `modules` as `disabledModules` is bounded: at most
/// [`MAX_DISABLED_MODULES`] entries, each named as a module can be. What an
/// entry holds is its module's to decode.
pub(crate) fn validate_module_settings(settings: &ModuleSettings) -> Result<(), String> {
    if settings.len() > MAX_DISABLED_MODULES {
        return Err(format!(
            "modules holds the settings of at most {MAX_DISABLED_MODULES} modules"
        ));
    }
    if settings
        .keys()
        .any(|name| !DISABLED_MODULES.is_valid_name(name))
    {
        return Err(format!(
            "modules names must contain 1 to {MAX_MODULE_NAME_BYTES} bytes without surrounding whitespace or control characters"
        ));
    }
    Ok(())
}

/// The `modules` a settings patch carries: an object of settings objects.
pub(crate) fn parse_module_settings(value: &Value) -> Result<ModuleSettings, String> {
    let settings: ModuleSettings = serde_json::from_value(value.clone())
        .map_err(|_| "modules must be an object of settings objects".to_string())?;
    validate_module_settings(&settings)?;
    Ok(settings)
}
