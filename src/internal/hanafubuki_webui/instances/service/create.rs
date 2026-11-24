use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_config::repository::instances::{
    InstanceComponentPaths, InstanceConfig, LaunchArgumentOrigin,
};
use crate::internal::hanafubuki_foundation::paths::{normalize_path, validate_not_dangerous};
use crate::internal::hanafubuki_instance_contracts::{InstanceTypeDescriptor, InstanceTypeId};
use serde_json::json;

use crate::internal::hanafubuki_webui::instance_types::presets::ResolvedPreset;

use super::super::identity::{generate_instance_id, now_millis, resolve_instance_name};
use super::super::{
    CreateWebUiInstanceRequest, InstanceSpec, WebUiInstanceInstallSource, WebUiInstanceLifecycle,
    WebUiInstanceRecord, WebUiInstanceSnapshot,
};
use super::WebUiInstanceService;

impl WebUiInstanceService {
    pub fn create(
        &self,
        request: CreateWebUiInstanceRequest,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let provider = self.repository.types().require(&request.type_id)?;
        refuse_attach_only(provider.descriptor(), "create")?;
        let spec = provider.validate_spec(request.spec)?;
        let preset = request
            .preset
            .map(|id| self.repository.presets().resolve(&id, &request.type_id))
            .transpose()?;
        let name = resolve_instance_name(
            request.name.as_deref(),
            &self.default_name(&request.type_id, &spec),
        )?;
        for _ in 0..100 {
            let instance_id = generate_instance_id(&name);
            if self.repository.record_or_config_exists(&instance_id) {
                continue;
            }
            return self.create_with_id(
                instance_id,
                name,
                request.install_dir,
                request.component_paths,
                request.type_id,
                spec,
                WebUiInstanceInstallSource::Standard,
                None,
                preset.as_ref(),
            );
        }
        Err("failed to allocate a unique instance id".to_string())
    }

    /// [`Self::create`] for a caller that names only what it sets (the
    /// management tools): the request's spec goes over its preset's install
    /// values, the caller's value of a field winning.
    pub fn create_over_preset(
        &self,
        mut request: CreateWebUiInstanceRequest,
    ) -> Result<WebUiInstanceSnapshot, String> {
        if let Some(id) = &request.preset {
            let mut spec = self
                .repository
                .presets()
                .resolve(id, &request.type_id)?
                .install;
            spec.extend(std::mem::take(&mut request.spec));
            request.spec = spec;
        }
        self.create(request)
    }

    pub(in crate::internal::hanafubuki_webui::instances) fn create_with_id(
        &self,
        instance_id: String,
        name: String,
        install_dir: Option<PathBuf>,
        requested_paths: BTreeMap<String, PathBuf>,
        type_id: InstanceTypeId,
        spec: InstanceSpec,
        install_source: WebUiInstanceInstallSource,
        snapshot_payload: Option<&serde_json::Value>,
        preset: Option<&ResolvedPreset>,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let final_dir =
            self.resolve_and_validate_install_dir(&instance_id, install_dir.as_ref())?;
        let component_paths = self.component_paths(&type_id, &final_dir, requested_paths)?;
        if final_dir.is_file() {
            return Err(format!(
                "instance install path is an existing file: {}",
                final_dir.display()
            ));
        }

        let now = now_millis();
        let record = WebUiInstanceRecord {
            instance_id: instance_id.clone(),
            instance_identity: self.repository.new_instance_identity()?,
            type_id,
            spec,
            install_source,
            lifecycle: WebUiInstanceLifecycle::Created,
            created_at: now,
            updated_at: now,
            preset_id: preset.map(|preset| preset.id.to_string()),
            preset_launch_arguments: preset
                .map(|preset| preset.launch.clone())
                .unwrap_or_default(),
        };
        let (launch_args, launch_argument_origin) = initial_launch_arguments(&record)?;
        self.repository.create_instance(
            record,
            InstanceConfig {
                id: instance_id.clone(),
                name,
                config_revision: 1,
                install_dir: Some(final_dir),
                component_paths: Some(component_paths),
                quick_paths: Vec::new(),
                env_vars: BTreeMap::new(),
                launch_args,
                launch_argument_origin,
                settings_overrides: json!({}),
            },
            snapshot_payload,
        )?;
        self.get(&instance_id)
    }

    /// The type's components, each in its own directory below the root
    /// unless the request places it; a request naming a component the type
    /// does not declare is refused.
    pub(super) fn component_paths(
        &self,
        type_id: &InstanceTypeId,
        install_dir: &Path,
        requested: BTreeMap<String, PathBuf>,
    ) -> Result<InstanceComponentPaths, String> {
        let provider = self.repository.types().require(type_id)?;
        let declared = &provider.descriptor().components;
        let mut paths =
            InstanceComponentPaths::defaults(install_dir, declared.iter().map(String::as_str));
        for (name, path) in requested {
            if !declared.contains(&name) {
                return Err(format!(
                    "instance type `{type_id}` declares no `{name}` component"
                ));
            }
            paths.set(&name, path);
        }
        Ok(paths)
    }

    /// A generated name's prefix: the type's explicit default, otherwise
    /// the built-in family's product name or the type's label.
    pub(super) fn default_name(&self, type_id: &InstanceTypeId, spec: &InstanceSpec) -> String {
        let provider = self.repository.types().get(type_id);
        if let Some(name) = provider
            .as_ref()
            .and_then(|provider| provider.descriptor().default_name.as_ref())
        {
            return name.clone();
        }
        if let InstanceSpec::Builtin(spec) = spec {
            return spec.family().product_name().to_string();
        }
        provider
            .and_then(|provider| {
                provider
                    .descriptor()
                    .label
                    .literal_text()
                    .filter(|text| !text.trim().is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| type_id.to_string())
    }

    pub(super) fn resolve_and_validate_install_dir(
        &self,
        instance_id: &str,
        install_dir: Option<&PathBuf>,
    ) -> Result<PathBuf, String> {
        let path = match install_dir {
            Some(path) => normalize_path(path.clone())?,
            None => self.repository.default_install_dir(instance_id)?,
        };
        validate_not_dangerous(&path)?;
        Ok(path)
    }
}

/// Refuses to `create` or `import` an instance of a type that only
/// discovery attaches (`attachOnly`).
pub(super) fn refuse_attach_only(
    descriptor: &InstanceTypeDescriptor,
    action: &str,
) -> Result<(), String> {
    if !descriptor.attach_only {
        return Ok(());
    }
    Err(format!(
        "instance type `{}` is attached only by discovery: scan the folder of an existing installation instead of trying to {action} one",
        descriptor.id
    ))
}

/// The launch arguments a new instance starts with: a preset's over its
/// built-in type's defaults, as built-in ones, which the install's
/// completion and a branch switch set again over the defaults of the spec
/// then; otherwise none until the install's completion sets the defaults.
fn initial_launch_arguments(
    record: &WebUiInstanceRecord,
) -> Result<(Vec<String>, LaunchArgumentOrigin), String> {
    if record.preset_launch_arguments.is_empty() {
        return Ok((Vec::new(), LaunchArgumentOrigin::PendingDefault));
    }
    Ok((
        record.default_launch_arguments()?,
        LaunchArgumentOrigin::BuiltIn,
    ))
}
