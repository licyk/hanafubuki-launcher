use super::*;

pub fn get_resolved_instance_config_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
) -> Result<ResolvedInstanceConfig, String> {
    validate_instance_id(instance_id)?;
    let global = settings::get_global_settings_inner(paths, state)?;
    let instance = {
        let _guard = state.instances_lock.lock();
        load_instance(paths, state, instance_id)?
    };
    let settings = resolve_effective_settings(paths, &global, &instance)?;
    let install_dir = instance
        .install_dir
        .clone()
        .unwrap_or_else(|| global.default_webui_install_dir.join(&instance.id));
    let component_paths = instance.component_paths.clone().unwrap_or_else(|| {
        InstanceComponentPaths::defaults(&install_dir, InstanceComponentPaths::PYTHON_AND_CORE)
    });

    log::debug!("get_resolved_instance_config_inner resolved id={instance_id}");
    Ok(ResolvedInstanceConfig {
        id: instance.id,
        name: instance.name,
        install_dir,
        component_paths,
        quick_paths: instance.quick_paths,
        env_vars: instance.env_vars,
        launch_args: instance.launch_args,
        launch_argument_origin: instance.launch_argument_origin,
        settings,
    })
}

/// Produce the effective [`AppSettings`] for an instance by layering
/// defaults -> global -> instance overrides.
pub fn resolve_effective_settings(
    paths: &AppPaths,
    global: &AppSettings,
    instance: &InstanceConfig,
) -> Result<AppSettings, String> {
    // Keep-null merge: nullable settings fields (custom mirrors) may be set
    // to `null`, and every key must remain present so the final value still
    // deserializes into the typed `AppSettings`.
    let mut value = serde_json::to_value(AppSettings::defaults(paths))
        .map_err(|err| format!("serialize defaults: {err}"))?;
    value = merge::merge_json_keep_null(
        value,
        serde_json::to_value(global).map_err(|err| format!("serialize global settings: {err}"))?,
    );
    if let Some(object) = instance.settings_overrides.as_object() {
        value = merge::merge_json_keep_null(value, Value::Object(object.clone()));
    }
    serde_json::from_value(value).map_err(|err| format!("resolve effective settings: {err}"))
}
