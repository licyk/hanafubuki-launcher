use super::InstanceConfig;
use super::store::{load_instance, read_instance_configs_unlocked};
use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;

pub fn list_instance_configs_inner(
    paths: &AppPaths,
    state: &ConfigState,
) -> Result<Vec<InstanceConfig>, String> {
    let _guard = state.instances_lock.lock();
    let configs = read_instance_configs_unlocked(paths, state)?;
    log::debug!("list_instance_configs_inner found count={}", configs.len());
    Ok(configs)
}

pub fn get_instance_config_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
) -> Result<InstanceConfig, String> {
    let _guard = state.instances_lock.lock();
    load_instance(paths, state, instance_id)
}
