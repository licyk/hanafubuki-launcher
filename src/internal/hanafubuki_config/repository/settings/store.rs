use super::*;

use crate::internal::hanafubuki_config::repository::settings::persisted::documents_for;

/// Load the current global settings, creating and normalizing the settings file
/// when needed. Tauri-free: callable from other backend modules and tests.
pub fn get_global_settings_inner(
    paths: &AppPaths,
    state: &ConfigState,
) -> Result<AppSettings, String> {
    let _guard = state.settings_lock.lock();
    load_settings_for_state(paths, state)
}

/// Validate and persist a single global-settings field, returning the updated
/// settings. Tauri-free; does not emit any Tauri event so callers control
/// notification.
pub fn update_global_setting_inner(
    paths: &AppPaths,
    state: &ConfigState,
    key: String,
    value: Value,
) -> Result<AppSettings, String> {
    let _guard = state.settings_lock.lock();
    ensure_settings_writable(state)?;
    let previous = load_settings_for_state(paths, state)?;
    let mut settings = previous.clone();
    apply_setting_patch(&mut settings, &key, value)?;
    validate_settings(&settings)?;
    prepare_settings_directories(&settings)?;
    write_changed_documents(paths, &previous, &settings)?;
    state.publish_settings(settings.clone());
    log::info!("update_global_setting_inner persisted key={}", key);
    Ok(settings)
}

/// Validate and persist a structured multi-key patch onto the global settings,
/// returning the updated settings. Every key in the patch is validated before
/// anything is written, so a single invalid field rejects the whole patch (no
/// partial application). Tauri-free; emits no event.
pub fn update_global_settings_patch_inner(
    paths: &AppPaths,
    state: &ConfigState,
    patch: Value,
) -> Result<AppSettings, String> {
    let object = patch
        .as_object()
        .ok_or_else(|| "settings patch must be an object".to_string())?;
    let _guard = state.settings_lock.lock();
    ensure_settings_writable(state)?;
    let previous = load_settings_for_state(paths, state)?;
    // Apply against a working copy so a mid-patch validation error cannot leave a
    // half-updated struct that then gets written.
    let mut working = previous.clone();
    for (key, value) in object {
        apply_setting_patch(&mut working, key, value.clone())?;
    }
    validate_settings(&working)?;
    let settings = working;
    prepare_settings_directories(&settings)?;
    write_changed_documents(paths, &previous, &settings)?;
    state.publish_settings(settings.clone());
    log::info!(
        "update_global_settings_patch_inner persisted keys={:?}",
        object.keys().collect::<Vec<_>>()
    );
    Ok(settings)
}

/// Restore the Appearance section to its backend defaults and persist through
/// the same atomic path as any other settings write. Tauri-free; emits no event.
///
/// This never touches the imported image: Reset is a presentation reset, and a
/// user who wanted the file gone would have used Clear.
pub fn reset_appearance_settings_inner(
    paths: &AppPaths,
    state: &ConfigState,
) -> Result<AppSettings, String> {
    let _guard = state.settings_lock.lock();
    ensure_settings_writable(state)?;
    let previous = load_settings_for_state(paths, state)?;
    let mut settings = previous.clone();
    settings.reset_appearance();
    validate_settings(&settings)?;
    prepare_settings_directories(&settings)?;
    write_changed_documents(paths, &previous, &settings)?;
    state.publish_settings(settings.clone());
    log::info!("reset_appearance_settings_inner persisted appearance defaults");
    Ok(settings)
}

/// Persist every domain whose document differs from what `previous` held.
///
/// A settings write touches one or two domains in practice, so rewriting
/// every domain's file on every commit would put unrelated documents at risk
/// for no reason.
fn write_changed_documents(
    paths: &AppPaths,
    previous: &AppSettings,
    settings: &AppSettings,
) -> Result<(), String> {
    // Both sides are projected in `SettingsDomain::ALL` order, so the pairs line
    // up domain for domain.
    for ((domain, before), (_, after)) in documents_for(previous)?
        .into_iter()
        .zip(documents_for(settings)?)
    {
        if before == after {
            continue;
        }
        safe_file::write_json_atomic(&paths.settings_file(domain), &Value::Object(after))?;
    }
    Ok(())
}

/// The published settings. Before anything is published they are read
/// through the one settings reader, which repairs, quarantines, and reports
/// exactly as bootstrap does.
fn load_settings_for_state(paths: &AppPaths, state: &ConfigState) -> Result<AppSettings, String> {
    if let Some(settings) = state.settings_snapshot() {
        return Ok(settings);
    }
    Ok(crate::internal::hanafubuki_config::recovery::load_settings_into_state(paths, state))
}

fn ensure_settings_writable(state: &ConfigState) -> Result<(), String> {
    state.ensure_persistence_writable()
}

/// Materialize the filesystem directories selected by the final settings.
///
/// Loading and recovery deliberately do not call this function. A caller may
/// need to reconcile persisted absolute paths after their owning portable tree
/// moved, and creating directories before that reconciliation would recreate
/// the stale location.
pub fn prepare_settings_directories(settings: &AppSettings) -> Result<(), String> {
    for path in [
        &settings.artifact_cache_dir,
        &settings.default_webui_install_dir,
        &settings.webview_download_dir,
    ] {
        log::debug!(
            "prepare_settings_directories creating path={}",
            path.display()
        );
        validate_not_dangerous(path)?;
        std::fs::create_dir_all(path).map_err(|err| format!("create {}: {err}", path.display()))?;
    }
    Ok(())
}
