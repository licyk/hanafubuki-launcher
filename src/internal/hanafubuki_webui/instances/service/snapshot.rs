use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use serde::Deserialize;

use super::super::identity::{generate_instance_id, resolve_instance_name};
use super::super::{
    CreateWebUiInstanceFromSnapshotRequest, InstanceSpec, WebUiInstanceInstallSource,
    WebUiInstanceSnapshot,
};
use super::WebUiInstanceService;
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;

use crate::internal::hanafubuki_python_runtime::PythonVersion;
use crate::internal::hanafubuki_webui::install::WebUiInstallSpec;
use crate::internal::hanafubuki_webui::instance_types::BuiltinProvider;

pub(in crate::internal::hanafubuki_webui::instances) const MAX_SNAPSHOT_BYTES: u64 =
    64 * 1024 * 1024;

impl WebUiInstanceService {
    pub fn create_from_snapshot(
        &self,
        request: CreateWebUiInstanceFromSnapshotRequest,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let expected_family =
            BuiltinProvider::family_of(&request.expected_type).ok_or_else(|| {
                format!(
                    "type `{}` does not restore from a snapshot",
                    request.expected_type
                )
            })?;
        let name = resolve_instance_name(request.name.as_deref(), expected_family.product_name())?;
        let source = ResolvedPath::resolve(&request.snapshot_path).map_err(|error| {
            format!("open snapshot {}: {error}", request.snapshot_path.display())
        })?;
        let source_metadata = source
            .system()
            .metadata()
            .map_err(|error| format!("read snapshot {} metadata: {error}", source.native_text()))?;
        if !source_metadata.is_file() {
            return Err(format!(
                "snapshot path is not a file: {}",
                source.native_text()
            ));
        }
        if source_metadata.len() > MAX_SNAPSHOT_BYTES {
            return Err("snapshot file exceeds the 64 MiB installation limit".to_string());
        }
        let bytes = std::fs::read(source.system())
            .map_err(|error| format!("read snapshot {}: {error}", source.native_text()))?;
        // Retain the post-read check for a file that grows after metadata was
        // inspected. The normal oversized path is rejected before allocation.
        if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
            return Err("snapshot file exceeds the 64 MiB installation limit".to_string());
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("snapshot file is not valid JSON: {error}"))?;
        let header: SnapshotInstallHeader = serde_json::from_value(value.clone())
            .map_err(|error| format!("snapshot does not contain install metadata: {error}"))?;
        if header.schema_version != 1 {
            return Err(format!(
                "unsupported snapshot schema version: {}",
                header.schema_version
            ));
        }
        if !header.python.implementation.eq_ignore_ascii_case("cpython") {
            return Err(format!(
                "snapshot Python implementation is unsupported: {}",
                header.python.implementation
            ));
        }
        let family = snapshot_webui_family(&header.webui.r#type)?;
        if family != expected_family {
            return Err(format!(
                "snapshot WebUI type {} does not match the selected family",
                header.webui.r#type
            ));
        }
        validate_snapshot_platform(&header.python.platform)?;
        let python_version = snapshot_python_version(&header.python.version)?;
        let spec = WebUiInstallSpec::snapshot_install(family, python_version);

        for _ in 0..100 {
            let instance_id = generate_instance_id(&name);
            if self.repository.record_or_config_exists(&instance_id) {
                continue;
            }
            let managed_snapshot = self.repository.managed_snapshot_path(&instance_id);
            return self.create_with_id(
                instance_id,
                name,
                request.install_dir,
                request.component_paths.clone(),
                request.expected_type.clone(),
                InstanceSpec::Builtin(spec),
                WebUiInstanceInstallSource::Snapshot {
                    snapshot_path: managed_snapshot,
                },
                Some(&value),
                None,
            );
        }
        Err("failed to allocate a unique snapshot instance id".to_string())
    }
}

#[derive(Deserialize)]
struct SnapshotInstallHeader {
    schema_version: u32,
    webui: SnapshotWebUiHeader,
    python: SnapshotPythonHeader,
}

#[derive(Deserialize)]
struct SnapshotWebUiHeader {
    r#type: String,
}

#[derive(Deserialize)]
struct SnapshotPythonHeader {
    version: String,
    implementation: String,
    platform: String,
}

fn snapshot_webui_family(value: &str) -> Result<WebUiFamily, String> {
    WebUiFamily::from_api_namespace(value)
        .ok_or_else(|| format!("unsupported snapshot WebUI type: {value}"))
}

fn snapshot_python_version(value: &str) -> Result<PythonVersion, String> {
    let mut parts = value.trim().split('.');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next().unwrap_or_default();
    if major.parse::<u32>().is_err() || minor.parse::<u32>().is_err() {
        return Err(format!("invalid snapshot Python version: {value}"));
    }
    PythonVersion::try_from(format!("{major}.{minor}").as_str()).map_err(|error| error.to_string())
}

fn validate_snapshot_platform(platform: &str) -> Result<(), String> {
    let compatible = match std::env::consts::OS {
        "windows" => platform.starts_with("win"),
        "macos" => platform == "darwin",
        "linux" => platform.starts_with("linux"),
        _ => true,
    };
    compatible.then_some(()).ok_or_else(|| {
        format!(
            "snapshot Python platform {platform} is incompatible with {}",
            std::env::consts::OS
        )
    })
}
