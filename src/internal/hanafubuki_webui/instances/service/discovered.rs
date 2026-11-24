use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;
use crate::internal::hanafubuki_config::repository::instances::{
    InstanceComponentPaths, InstanceConfig, LaunchArgumentOrigin,
};
use serde_json::json;

use super::super::identity::{generate_instance_id, now_millis, resolve_instance_name};
use super::super::{
    InstanceSpec, WebUiInstanceInstallSource, WebUiInstanceLifecycle, WebUiInstanceRecord,
    WebUiInstanceSnapshot,
};
use super::{InstancePythonValidator, WebUiInstanceService};
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;

use crate::internal::hanafubuki_webui::install::{WebUiInstallSpec, default_launch_arguments};
use crate::internal::hanafubuki_webui::instance_types::BuiltinProvider;

#[derive(Clone, Debug)]
pub struct PrepareDiscoveredWebUiRequest {
    pub family: WebUiFamily,
    pub instance_path: PathBuf,
    pub core_path: PathBuf,
    pub python_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PrepareDiscoveredCoreRequest {
    pub family: WebUiFamily,
    pub core_path: PathBuf,
}

#[derive(Clone, Debug)]
struct PreparedDiscoveredBase {
    instance_id: String,
    instance_identity: String,
    name: String,
    install_dir: PathBuf,
    reserve_install_dir: bool,
    core_path: PathBuf,
    spec: WebUiInstallSpec,
}

#[derive(Clone, Copy)]
enum DiscoveredCommitMode {
    Ready,
    Incomplete,
}

#[derive(Clone, Debug)]
pub struct PreparedDiscoveredWebUi {
    base: PreparedDiscoveredBase,
    python_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PreparedDiscoveredCore {
    base: PreparedDiscoveredBase,
}

impl PreparedDiscoveredWebUi {
    pub fn instance_id(&self) -> &str {
        &self.base.instance_id
    }

    pub fn core_path(&self) -> &Path {
        &self.base.core_path
    }

    pub fn python_dir(&self) -> &Path {
        &self.python_dir
    }

    pub fn install_dir(&self) -> &Path {
        &self.base.install_dir
    }
}

impl PreparedDiscoveredCore {
    pub fn instance_id(&self) -> &str {
        &self.base.instance_id
    }

    pub fn core_path(&self) -> &Path {
        &self.base.core_path
    }

    pub fn install_dir(&self) -> &Path {
        &self.base.install_dir
    }
}

impl WebUiInstanceService {
    pub fn prepare_discovered(
        &self,
        request: PrepareDiscoveredWebUiRequest,
    ) -> Result<PreparedDiscoveredWebUi, String> {
        let install_dir = InstancePathPolicy::canonical_existing_directory(
            "discovered instance root",
            &request.instance_path,
        )?;
        let core_path = InstancePathPolicy::canonical_existing_directory(
            "discovered WebUI Core",
            &request.core_path,
        )?;
        let python_dir = InstancePathPolicy::canonical_existing_directory(
            "discovered WebUI Python",
            &request.python_dir,
        )?;
        let component_paths =
            InstanceComponentPaths::python_and_core(python_dir.clone(), core_path.clone());
        InstancePathPolicy::validate_component_layout(&install_dir, &component_paths)?;
        let base = self.prepare_discovered_base(request.family, core_path, Some(install_dir))?;
        Ok(PreparedDiscoveredWebUi { base, python_dir })
    }

    pub fn prepare_discovered_core(
        &self,
        request: PrepareDiscoveredCoreRequest,
    ) -> Result<PreparedDiscoveredCore, String> {
        let core_path = InstancePathPolicy::canonical_existing_directory(
            "discovered WebUI Core",
            &request.core_path,
        )?;
        let base = self.prepare_discovered_base(request.family, core_path, None)?;
        InstancePathPolicy::validate_component_layout(
            &base.install_dir,
            &InstanceComponentPaths::python_and_core(
                base.install_dir.join("python"),
                base.core_path.clone(),
            ),
        )?;
        Ok(PreparedDiscoveredCore { base })
    }

    fn prepare_discovered_base(
        &self,
        family: WebUiFamily,
        core_path: PathBuf,
        requested_install_dir: Option<PathBuf>,
    ) -> Result<PreparedDiscoveredBase, String> {
        let name = resolve_instance_name(None, family.product_name())?;
        let spec = WebUiInstallSpec::default_for(family);
        for _ in 0..100 {
            let instance_id = generate_instance_id(&name);
            if self.repository.record_or_config_exists(&instance_id) {
                continue;
            }
            let (install_dir, reserve_install_dir) = match requested_install_dir.as_ref() {
                Some(path) => (path.clone(), false),
                None => {
                    let path = self.resolve_and_validate_install_dir(&instance_id, None)?;
                    if path.exists() {
                        continue;
                    }
                    (path, true)
                }
            };
            return Ok(PreparedDiscoveredBase {
                instance_id,
                instance_identity: self.repository.new_instance_identity()?,
                name,
                install_dir,
                reserve_install_dir,
                core_path,
                spec,
            });
        }
        Err("failed to allocate a unique discovered instance id".to_string())
    }

    pub fn commit_discovered(
        &self,
        prepared: PreparedDiscoveredWebUi,
        python: &dyn InstancePythonValidator,
    ) -> Result<WebUiInstanceSnapshot, String> {
        python.validate(&prepared.base.instance_id, &prepared.python_dir)?;
        self.commit_discovered_record(
            prepared.base,
            prepared.python_dir,
            DiscoveredCommitMode::Ready,
        )
    }

    pub fn commit_discovered_core(
        &self,
        prepared: PreparedDiscoveredCore,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let python_dir = prepared.base.install_dir.join("python");
        self.commit_discovered_record(prepared.base, python_dir, DiscoveredCommitMode::Incomplete)
    }

    fn commit_discovered_record(
        &self,
        prepared: PreparedDiscoveredBase,
        python_dir: PathBuf,
        mode: DiscoveredCommitMode,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let now = now_millis();
        let (install_source, lifecycle, launch_args, launch_argument_origin) = match mode {
            DiscoveredCommitMode::Ready => (
                WebUiInstanceInstallSource::Standard,
                WebUiInstanceLifecycle::Ready {
                    operation_id: format!("discovery-import-{now}"),
                },
                default_launch_arguments(&prepared.spec),
                LaunchArgumentOrigin::BuiltIn,
            ),
            DiscoveredCommitMode::Incomplete => (
                WebUiInstanceInstallSource::DiscoveredCore,
                WebUiInstanceLifecycle::Created,
                Vec::new(),
                LaunchArgumentOrigin::PendingDefault,
            ),
        };
        if prepared.reserve_install_dir {
            let parent = prepared
                .install_dir
                .parent()
                .ok_or_else(|| "discovered managed instance root has no parent".to_string())?;
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "create discovered instance parent {}: {error}",
                    parent.display()
                )
            })?;
            std::fs::create_dir(&prepared.install_dir).map_err(|error| {
                format!(
                    "reserve discovered instance root {}: {error}",
                    prepared.install_dir.display()
                )
            })?;
        }

        let result = self.repository.create_instance(
            WebUiInstanceRecord {
                instance_id: prepared.instance_id.clone(),
                instance_identity: prepared.instance_identity,
                type_id: BuiltinProvider::type_id(prepared.spec.family()),
                spec: InstanceSpec::Builtin(prepared.spec),
                install_source,
                lifecycle,
                created_at: now,
                updated_at: now,
                preset_id: None,
                preset_launch_arguments: Vec::new(),
            },
            InstanceConfig {
                id: prepared.instance_id.clone(),
                name: prepared.name,
                config_revision: 1,
                install_dir: Some(prepared.install_dir.clone()),
                component_paths: Some(InstanceComponentPaths::python_and_core(
                    python_dir,
                    prepared.core_path,
                )),
                quick_paths: Vec::new(),
                env_vars: BTreeMap::new(),
                launch_args,
                launch_argument_origin,
                settings_overrides: json!({}),
            },
            None,
        );
        if let Err(error) = result {
            if !prepared.reserve_install_dir {
                return Err(error);
            }
            let cleanup = std::fs::remove_dir(&prepared.install_dir);
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}; remove reserved discovered instance root {}: {cleanup_error}",
                    prepared.install_dir.display()
                )),
            };
        }
        self.get(&prepared.instance_id)
    }
}
