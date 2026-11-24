use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredInstance {
    pub instance_id: String,
    pub instance_identity: String,
    pub name: String,
    pub family: WebUiFamily,
    pub instance_path: Option<String>,
    pub core_path: String,
    pub python_path: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    #[serde(default)]
    pub scan_completed: bool,
    #[serde(default)]
    pub discovered_instances: Vec<DiscoveredInstance>,
}

pub fn load_or_create(path: &ResolvedPath) -> Result<LauncherSettings, String> {
    log::info!(
        target: "launcher::settings",
        "loading launcher settings path={}",
        path.native_text()
    );
    let value = match safe_file::read_json::<Value>(path.system())
        .map_err(|error| format!("Read launcher settings: {error}"))?
    {
        Some(value) => value,
        None => {
            let settings = LauncherSettings::default();
            store(&settings, path)?;
            log::info!(target: "launcher::settings", "default launcher settings created");
            return Ok(settings);
        }
    };
    let (settings, changed) = decode(value)?;
    if changed {
        log::info!(target: "launcher::settings", "canonicalizing launcher settings");
        store(&settings, path)?;
    }
    Ok(settings)
}

pub fn store(settings: &LauncherSettings, path: &ResolvedPath) -> Result<(), String> {
    safe_file::write_json_atomic(path.system(), settings)
        .map_err(|error| format!("Write launcher settings: {error}"))?;
    log::info!(
        target: "launcher::settings",
        "launcher settings stored scan_completed={} discovered_instances={} path={}",
        settings.scan_completed,
        settings.discovered_instances.len(),
        path.native_text()
    );
    Ok(())
}

fn decode(value: Value) -> Result<(LauncherSettings, bool), String> {
    let original = value.clone();
    let object = value
        .as_object()
        .ok_or_else(|| "Launcher settings root must be a JSON object".to_string())?;
    let scan_completed = object
        .get("scanCompleted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut invalid_discovery = object
        .get("scanCompleted")
        .is_some_and(|value| !value.is_boolean());
    let mut discovered_instances = Vec::new();

    match object.get("discoveredInstances") {
        None => {}
        Some(Value::Array(entries)) => {
            for entry in entries {
                let Some(entry_object) = entry.as_object() else {
                    invalid_discovery = true;
                    continue;
                };
                let mut sanitized = serde_json::Map::new();
                for key in [
                    "instanceId",
                    "instanceIdentity",
                    "name",
                    "family",
                    "instancePath",
                    "corePath",
                    "pythonPath",
                ] {
                    if let Some(value) = entry_object.get(key) {
                        sanitized.insert(key.to_string(), value.clone());
                    }
                }
                match serde_json::from_value(Value::Object(sanitized)) {
                    Ok(instance) => discovered_instances.push(instance),
                    Err(_) => invalid_discovery = true,
                }
            }
        }
        Some(_) => invalid_discovery = true,
    }

    let settings = LauncherSettings {
        scan_completed: scan_completed && !invalid_discovery,
        discovered_instances,
    };
    let canonical = serde_json::to_value(&settings)
        .map_err(|error| format!("Encode launcher settings: {error}"))?;
    Ok((settings, canonical != original))
}

#[cfg(test)]
mod tests;
