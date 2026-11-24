use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;
use crate::internal::hanafubuki_config::repository::instances::{
    InstanceComponentPaths, InstanceConfig, LaunchArgumentOrigin,
};
use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;
use serde_json::json;

use super::super::identity::{generate_instance_id, now_millis, resolve_instance_name};
use super::super::{
    ImportWebUiInstanceRequest, InstanceSpec, WebUiInstanceInstallSource, WebUiInstanceLifecycle,
    WebUiInstanceRecord, WebUiInstanceSnapshot,
};
use super::WebUiInstanceService;
use crate::internal::hanafubuki_webui::install::default_launch_arguments;

/// Application-supplied validation port for importing an existing instance.
pub trait InstancePythonValidator: Send + Sync {
    fn validate(&self, instance_id: &str, python_dir: &Path) -> Result<(), String>;
}

/// An import checked and given its instance id, which a caller holds the
/// instance's start permit for before it commits the record.
#[derive(Clone, Debug)]
pub struct PreparedImport {
    instance_id: String,
    name: String,
    install_dir: PathBuf,
    component_paths: InstanceComponentPaths,
    type_id: InstanceTypeId,
    spec: InstanceSpec,
}

impl PreparedImport {
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

impl WebUiInstanceService {
    pub fn import_existing(
        &self,
        request: ImportWebUiInstanceRequest,
        python: &dyn InstancePythonValidator,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let prepared = self.prepare_import(request)?;
        self.commit_import(prepared, python)
    }

    /// Checks the request (its type's `validate_spec`, the directories it
    /// names, which must exist) and allocates the instance's id. A type
    /// only discovery attaches is refused: its signature says which
    /// components are external, which a hand import does not know.
    pub fn prepare_import(
        &self,
        request: ImportWebUiInstanceRequest,
    ) -> Result<PreparedImport, String> {
        let provider = self.repository.types().require(&request.type_id)?;
        super::create::refuse_attach_only(provider.descriptor(), "import")?;
        self.prepare_import_with_external(request, &BTreeSet::new())
    }

    /// `prepare_import` of an installation whose `external` components the
    /// instance records as folders it does not own: a discovery signature's,
    /// each one its type declares `external = "allowed"`.
    pub fn prepare_import_with_external(
        &self,
        request: ImportWebUiInstanceRequest,
        external: &BTreeSet<String>,
    ) -> Result<PreparedImport, String> {
        let spec = self
            .repository
            .types()
            .require(&request.type_id)?
            .validate_spec(request.spec)?;
        let name = resolve_instance_name(
            request.name.as_deref(),
            &self.default_name(&request.type_id, &spec),
        )?;
        let install_dir = InstancePathPolicy::canonical_existing_directory(
            "instance root",
            &request.install_dir,
        )?;
        let component_paths =
            self.component_paths(&request.type_id, &install_dir, request.component_paths)?;
        let component_paths = component_paths.try_map(|name, path| {
            let label = match name {
                InstanceComponentPaths::CORE => "WebUI core".to_string(),
                other => InstancePathPolicy::component_label(other),
            };
            InstancePathPolicy::canonical_existing_directory(&label, path)
        })?;
        let component_paths = mark_external(component_paths, external, &request.type_id, self)?;
        for _ in 0..100 {
            let instance_id = generate_instance_id(&name);
            if self.repository.record_or_config_exists(&instance_id) {
                continue;
            }
            return Ok(PreparedImport {
                instance_id,
                name,
                install_dir,
                component_paths,
                type_id: request.type_id,
                spec,
            });
        }
        Err("failed to allocate a unique imported instance id".to_string())
    }

    /// Validates the import's Python, when its type has one, and writes the
    /// ready instance's record.
    pub fn commit_import(
        &self,
        prepared: PreparedImport,
        python: &dyn InstancePythonValidator,
    ) -> Result<WebUiInstanceSnapshot, String> {
        if let Some(python_dir) = prepared.component_paths.python() {
            python.validate(&prepared.instance_id, python_dir)?;
        }
        self.import_existing_with_id(prepared)
    }

    fn import_existing_with_id(
        &self,
        PreparedImport {
            instance_id,
            name,
            install_dir,
            component_paths,
            type_id,
            spec,
        }: PreparedImport,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let (launch_args, launch_argument_origin) = match &spec {
            InstanceSpec::Builtin(spec) => (
                default_launch_arguments(spec),
                LaunchArgumentOrigin::BuiltIn,
            ),
            _ => (Vec::new(), LaunchArgumentOrigin::User),
        };
        let now = now_millis();
        let record = WebUiInstanceRecord {
            instance_id: instance_id.clone(),
            instance_identity: self.repository.new_instance_identity()?,
            type_id,
            spec,
            install_source: WebUiInstanceInstallSource::Standard,
            lifecycle: WebUiInstanceLifecycle::Ready {
                operation_id: format!("import-{now}"),
            },
            created_at: now,
            updated_at: now,
            preset_id: None,
            preset_launch_arguments: Vec::new(),
        };
        self.repository.create_instance(
            record,
            InstanceConfig {
                id: instance_id.clone(),
                name,
                config_revision: 1,
                install_dir: Some(install_dir),
                component_paths: Some(component_paths),
                quick_paths: Vec::new(),
                env_vars: BTreeMap::new(),
                launch_args,
                launch_argument_origin,
                settings_overrides: json!({}),
            },
            None,
        )?;
        self.get(&instance_id)
    }
}

fn mark_external(
    mut component_paths: InstanceComponentPaths,
    external: &BTreeSet<String>,
    type_id: &InstanceTypeId,
    service: &WebUiInstanceService,
) -> Result<InstanceComponentPaths, String> {
    let provider = service.repository.types().require(type_id)?;
    for name in external {
        if !provider.descriptor().external_components.contains_key(name) {
            return Err(format!(
                "instance type `{type_id}` does not let its `{name}` component be external"
            ));
        }
        let path = component_paths.require(name)?.to_path_buf();
        component_paths.set_external(name, path);
    }
    Ok(component_paths)
}
