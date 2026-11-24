use super::*;

pub fn create_instance_config_inner(
    paths: &AppPaths,
    state: &ConfigState,
    mut config: InstanceConfig,
) -> Result<InstanceConfig, String> {
    state.ensure_persistence_writable()?;
    let _guard = state.instances_lock.lock();
    config.config_revision = initial_config_revision();
    validate_instance_config(paths, &mut config)?;
    validate_component_path_conflicts(paths, state, &config)?;
    let file = paths.instance_config_file(&config.id);
    if file.exists() {
        return Err(format!("instance already exists: {}", config.id));
    }
    safe_file::write_json_atomic(&file, &config)?;
    state.clear_instance_issue(&config.id);
    log::info!("create_instance_config_inner persisted id={}", config.id);
    Ok(config)
}

pub fn update_instance_config_patch_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
    patch: Value,
) -> Result<InstanceConfig, String> {
    validate_instance_id(instance_id)?;
    state.ensure_persistence_writable()?;
    if !patch.is_object() {
        return Err("instance config patch must be an object".to_string());
    }
    let _guard = state.instances_lock.lock();
    let existing = load_instance(paths, state, instance_id)?;
    let mut config = apply_instance_patch(paths, existing, instance_id, patch)?;
    validate_component_path_conflicts(paths, state, &config)?;
    config.config_revision = config
        .config_revision
        .checked_add(1)
        .ok_or_else(|| "instance config revision exhausted".to_string())?;
    safe_file::write_json_atomic(&paths.instance_config_file(instance_id), &config)?;
    log::info!("update_instance_config_patch_inner persisted id={instance_id}");
    Ok(config)
}

pub fn update_instance_component_paths_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
    expected_revision: u64,
    component_paths: InstanceComponentPaths,
) -> Result<InstanceConfig, String> {
    validate_instance_id(instance_id)?;
    state.ensure_persistence_writable()?;
    let _guard = state.instances_lock.lock();
    let mut config = load_instance(paths, state, instance_id)?;
    if config.config_revision != expected_revision {
        return Err(format!(
            "instance config revision is stale (observed {expected_revision}, current {})",
            config.config_revision
        ));
    }
    config.component_paths = Some(component_paths);
    config.config_revision = config
        .config_revision
        .checked_add(1)
        .ok_or_else(|| "instance config revision exhausted".to_string())?;
    validate_instance_config(paths, &mut config)?;
    validate_component_path_conflicts(paths, state, &config)?;
    safe_file::write_json_atomic(&paths.instance_config_file(instance_id), &config)?;
    Ok(config)
}

/// Persist only the user's quick-access list under an observed revision.
///
/// Quick paths take part in no layout policy, so this deliberately performs no
/// component-path conflict check; ordinary instance validation still bounds and
/// normalizes every entry.
pub fn update_instance_quick_paths_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
    expected_revision: u64,
    quick_paths: Vec<InstanceQuickPath>,
) -> Result<InstanceConfig, String> {
    validate_instance_id(instance_id)?;
    state.ensure_persistence_writable()?;
    let _guard = state.instances_lock.lock();
    let mut config = load_instance(paths, state, instance_id)?;
    if config.config_revision != expected_revision {
        return Err(format!(
            "instance config revision is stale (observed {expected_revision}, current {})",
            config.config_revision
        ));
    }
    config.quick_paths = quick_paths;
    config.config_revision = config
        .config_revision
        .checked_add(1)
        .ok_or_else(|| "instance config revision exhausted".to_string())?;
    validate_instance_config(paths, &mut config)?;
    safe_file::write_json_atomic(&paths.instance_config_file(instance_id), &config)?;
    Ok(config)
}

pub fn update_instance_name_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
    expected_revision: u64,
    name: String,
) -> Result<InstanceConfig, String> {
    validate_instance_id(instance_id)?;
    state.ensure_persistence_writable()?;
    let _guard = state.instances_lock.lock();
    let mut config = load_instance(paths, state, instance_id)?;
    if config.config_revision != expected_revision {
        return Err(format!(
            "instance config revision is stale (observed {expected_revision}, current {})",
            config.config_revision
        ));
    }
    if config.name == name {
        return Ok(config);
    }
    config.name = name;
    config.config_revision = config
        .config_revision
        .checked_add(1)
        .ok_or_else(|| "instance config revision exhausted".to_string())?;
    validate_instance_config(paths, &mut config)?;
    safe_file::write_json_atomic(&paths.instance_config_file(instance_id), &config)?;
    Ok(config)
}

pub fn update_instance_location_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
    expected_revision: u64,
    expected_install_dir: &std::path::Path,
    destination_install_dir: PathBuf,
    destination_component_paths: InstanceComponentPaths,
) -> Result<InstanceConfig, String> {
    validate_instance_id(instance_id)?;
    state.ensure_persistence_writable()?;
    let _guard = state.instances_lock.lock();
    let mut config = load_instance(paths, state, instance_id)?;
    if config.config_revision != expected_revision {
        return Err(format!(
            "instance config revision is stale (observed {expected_revision}, current {})",
            config.config_revision
        ));
    }
    let current_install_dir = config
        .install_dir
        .as_ref()
        .ok_or_else(|| "instance install directory is not explicitly configured".to_string())?;
    let current_install_dir = normalize_path(current_install_dir.to_path_buf())?;
    let expected_install_dir = normalize_path(expected_install_dir.to_path_buf())?;
    if !InstancePathPolicy::paths_refer_to_same_location(
        &current_install_dir,
        &expected_install_dir,
    ) {
        return Err(format!(
            "instance install directory changed (expected {}, current {})",
            expected_install_dir.display(),
            current_install_dir.display()
        ));
    }
    config.install_dir = Some(destination_install_dir);
    config.component_paths = Some(destination_component_paths);
    config.config_revision = config
        .config_revision
        .checked_add(1)
        .ok_or_else(|| "instance config revision exhausted".to_string())?;
    validate_instance_config(paths, &mut config)?;
    validate_component_path_conflicts(paths, state, &config)?;
    safe_file::write_json_atomic(&paths.instance_config_file(instance_id), &config)?;
    Ok(config)
}

/// Persist only the canonical launch-argument list under an observed revision.
/// Catalog and instance-identity checks are performed by the launch-argument
/// service before this final atomic compare-and-write boundary.
pub fn update_instance_launch_args_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
    observed_revision: u64,
    launch_args: Vec<String>,
) -> Result<InstanceConfig, String> {
    validate_instance_id(instance_id)?;
    state.ensure_persistence_writable()?;
    let _guard = state.instances_lock.lock();
    let mut config = load_instance(paths, state, instance_id)?;
    if config.config_revision != observed_revision {
        return Err(format!(
            "instance config revision is stale (observed {observed_revision}, current {})",
            config.config_revision
        ));
    }
    config.launch_args = launch_args;
    config.launch_argument_origin = LaunchArgumentOrigin::User;
    config.config_revision = config
        .config_revision
        .checked_add(1)
        .ok_or_else(|| "instance config revision exhausted".to_string())?;
    validate_instance_config(paths, &mut config)?;
    safe_file::write_json_atomic(&paths.instance_config_file(instance_id), &config)?;
    Ok(config)
}

/// Synchronize Rust-owned built-in arguments without overwriting an explicit
/// user save. This avoids a new revision when the persisted value is already
/// identical, making install-completion retries idempotent.
pub fn synchronize_builtin_instance_launch_args_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
    launch_args: Vec<String>,
) -> Result<InstanceConfig, String> {
    validate_instance_id(instance_id)?;
    state.ensure_persistence_writable()?;
    let _guard = state.instances_lock.lock();
    let mut config = load_instance(paths, state, instance_id)?;
    if config.launch_argument_origin == LaunchArgumentOrigin::User {
        return Ok(config);
    }
    if config.launch_argument_origin == LaunchArgumentOrigin::BuiltIn
        && config.launch_args == launch_args
    {
        return Ok(config);
    }
    config.launch_args = launch_args;
    config.launch_argument_origin = LaunchArgumentOrigin::BuiltIn;
    config.config_revision = config
        .config_revision
        .checked_add(1)
        .ok_or_else(|| "instance config revision exhausted".to_string())?;
    validate_instance_config(paths, &mut config)?;
    safe_file::write_json_atomic(&paths.instance_config_file(instance_id), &config)?;
    Ok(config)
}

/// Confirm that an instance still has the revision observed by an application
/// operation. This repository check never executes caller work while holding
/// `instances_lock`; callers that create an external resource must validate
/// before and after creation, then compensate the created resource if the
/// second check is stale.
pub fn require_instance_config_revision_inner(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
    observed_revision: u64,
) -> Result<(), String> {
    validate_instance_id(instance_id)?;
    let _guard = state.instances_lock.lock();
    let config = load_instance(paths, state, instance_id)?;
    if config.config_revision != observed_revision {
        return Err(format!(
            "instance config revision is stale (observed {observed_revision}, current {})",
            config.config_revision
        ));
    }
    Ok(())
}

fn validate_component_path_conflicts(
    paths: &AppPaths,
    state: &ConfigState,
    candidate: &InstanceConfig,
) -> Result<(), String> {
    let (Some(install_dir), Some(component_paths)) = (
        candidate.install_dir.as_ref(),
        candidate.component_paths.as_ref(),
    ) else {
        return Ok(());
    };
    let existing = read_instance_configs_unlocked(paths, state)?;
    InstancePathPolicy::validate_layout(
        &candidate.id,
        install_dir.clone(),
        component_paths.clone(),
        &existing,
    )
    .map(|_| ())
}
