use super::*;
use crate::internal::hanafubuki_instance_contracts::launch_arguments_within_bounds;

/// Quick paths are a small, hand-curated list; these bounds keep one instance
/// config from growing without limit and keep every name renderable as a label.
pub(crate) const MAX_INSTANCE_QUICK_PATHS: usize = 32;
pub(crate) const MAX_INSTANCE_QUICK_PATH_NAME_CHARS: usize = 64;

/// The quick-path bounds, sent with the list to the editor so it disables
/// adding and limits a name without restating them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceQuickPathLimits {
    pub max_entries: usize,
    pub max_name_chars: usize,
}

impl InstanceQuickPathLimits {
    pub const CURRENT: Self = Self {
        max_entries: MAX_INSTANCE_QUICK_PATHS,
        max_name_chars: MAX_INSTANCE_QUICK_PATH_NAME_CHARS,
    };
}

pub(crate) fn validate_instance_config(
    paths: &AppPaths,
    config: &mut InstanceConfig,
) -> Result<(), String> {
    validate_instance_id(&config.id)?;
    if config.name.trim().is_empty() {
        return Err("instance name must not be empty".to_string());
    }
    if let Some(dir) = &config.install_dir {
        let normalized = normalize_path(dir.clone())?;
        validate_not_dangerous(&normalized)?;
        config.install_dir = Some(normalized);
    }
    if let Some(component_paths) = &config.component_paths {
        let component_paths = component_paths.try_map(|_, path| {
            let path = normalize_path(path.to_path_buf())?;
            validate_not_dangerous(&path)?;
            Ok(path)
        })?;
        if let Some(install_dir) = &config.install_dir {
            InstancePathPolicy::validate_component_layout(install_dir, &component_paths)?;
        } else {
            InstancePathPolicy::validate_component_separation(&component_paths)?;
        }
        config.component_paths = Some(component_paths);
    }
    validate_instance_quick_paths(&mut config.quick_paths)?;
    validate_instance_env_vars(&config.env_vars)?;
    if config.config_revision == 0 {
        return Err("instance config revision must be positive".to_string());
    }
    if !launch_arguments_within_bounds(&config.launch_args) {
        return Err("instance launchArgs exceed the canonical string-list limits".to_string());
    }
    if config.launch_argument_origin == LaunchArgumentOrigin::PendingDefault
        && !config.launch_args.is_empty()
    {
        return Err("pending default launch arguments must be empty".to_string());
    }

    // settingsOverrides must be an object (or null, treated as empty) and pass
    // per-field settings validation.
    match &config.settings_overrides {
        Value::Null => config.settings_overrides = Value::Object(Map::new()),
        Value::Object(object) => settings::validate_settings_overrides(paths, object)?,
        _ => return Err("settingsOverrides must be an object".to_string()),
    }
    Ok(())
}

/// Validate an instance id used as a file name. Only ASCII letters, digits,
/// `-`, and `_` are allowed, so the id cannot escape the instances directory.
pub fn validate_instance_id(instance_id: &str) -> Result<(), String> {
    if instance_id.is_empty() {
        return Err("instance id must not be empty".to_string());
    }
    if instance_id.len() > 128 {
        return Err("instance id is too long".to_string());
    }
    if !instance_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(format!(
            "instance id has unsupported characters: {instance_id}"
        ));
    }
    Ok(())
}

/// Normalize and bound the user's quick-access list.
///
/// A name is a free label the user writes on an entry, not its address: the
/// open command resolves an entry by its position in this list, so a name may
/// be left empty and two entries may carry the same one. Only the bounds that
/// keep a name renderable are enforced. Paths get the same lexical
/// normalization and root rejection as every other managed location.
pub(crate) fn validate_instance_quick_paths(
    quick_paths: &mut [InstanceQuickPath],
) -> Result<(), String> {
    if quick_paths.len() > MAX_INSTANCE_QUICK_PATHS {
        return Err(format!(
            "instance quickPaths exceed the limit of {MAX_INSTANCE_QUICK_PATHS} entries"
        ));
    }
    for quick_path in quick_paths.iter_mut() {
        let name = quick_path.name.trim().to_string();
        if name.chars().count() > MAX_INSTANCE_QUICK_PATH_NAME_CHARS {
            return Err(format!(
                "instance quick path name is longer than {MAX_INSTANCE_QUICK_PATH_NAME_CHARS} characters"
            ));
        }
        if name.chars().any(char::is_control) {
            return Err("instance quick path name must not contain control characters".to_string());
        }
        quick_path.name = name;
        quick_path.path = normalize_path(quick_path.path.clone())?;
        validate_not_dangerous(&quick_path.path)?;
    }
    Ok(())
}

pub(crate) fn validate_instance_env_vars(
    env_vars: &BTreeMap<String, String>,
) -> Result<(), String> {
    validate_native_environment(
        env_vars
            .iter()
            .map(|(key, value)| (key.as_str(), Some(value.as_str()))),
    )
    .map_err(|error| format!("invalid instance environment: {error}"))
}
