use super::{InstanceConfig, validate_instance_id};
use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use crate::internal::hanafubuki_config::recovery::{
    InstanceConfigRead, read_instance_config, record_reports,
};

/// Read an instance config through the one instance reader and **revalidate**
/// it. A manually edited, corrupted, or outdated file is repaired field by
/// field (the original kept in quarantine) or, when its identity or paths
/// cannot be trusted, quarantined and the instance made unavailable. Every
/// read path (`get`, `resolve`, `list`, and the base of every update) goes
/// through here, so each enforces the same rules as create/update.
pub(super) fn load_instance(
    paths: &AppPaths,
    state: &ConfigState,
    instance_id: &str,
) -> Result<InstanceConfig, String> {
    validate_instance_id(instance_id)?;
    // An unavailable record is reported once; reading it again would only
    // report it again.
    if let Some(issue) = state.instance_issue(instance_id) {
        return Err(unavailable(instance_id, &issue));
    }
    let writable = state.ensure_persistence_writable().is_ok();
    let (read, report) = read_instance_config(paths, instance_id, writable);
    record_reports(paths, state, report.into_iter().collect());
    match read {
        InstanceConfigRead::Loaded(config) => Ok(config),
        InstanceConfigRead::Missing => Err(format!("instance not found: {instance_id}")),
        InstanceConfigRead::Unavailable(issue) => {
            let error = unavailable(instance_id, &issue);
            state.record_instance_issue(issue);
            Err(error)
        }
    }
}

fn unavailable(
    instance_id: &str,
    issue: &crate::internal::hanafubuki_config::recovery::InstanceConfigIssue,
) -> String {
    format!(
        "instance {instance_id} is unavailable after configuration recovery {}: {}",
        issue.recovery_id, issue.reason
    )
}

/// Every instance config that can be used, by id. One that cannot is reported
/// through the reader and skipped, so it never hides the others.
pub(super) fn read_instance_configs_unlocked(
    paths: &AppPaths,
    state: &ConfigState,
) -> Result<Vec<InstanceConfig>, String> {
    let entries = match std::fs::read_dir(&paths.instance_records_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "read {}: {error}",
                paths.instance_records_dir.display()
            ));
        }
    };
    let mut configs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            continue;
        }
        let path = entry.path();
        let Some(instance_id) = path.file_name().and_then(|name| name.to_str()) else {
            log::warn!(
                "skipping instance record directory with a non-UTF-8 name: {}",
                path.display()
            );
            continue;
        };
        if validate_instance_id(instance_id).is_err() || !path.join("config.json").is_file() {
            continue;
        }
        match load_instance(paths, state, instance_id) {
            Ok(config) => configs.push(config),
            Err(error) => log::warn!("skipping instance config id={instance_id}: {error}"),
        }
    }
    configs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(configs)
}
