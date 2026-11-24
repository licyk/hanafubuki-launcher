use std::collections::BTreeSet;
use std::path::Path;

use super::super::{
    UpdateWebUiInstanceComponentPathsRequest, WebUiInstanceLifecycle, WebUiInstanceSnapshot,
};
use super::WebUiInstanceService;
use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;
use crate::internal::hanafubuki_foundation::paths::{PathKey, ResolvedPath};
use crate::internal::hanafubuki_instance_contracts::InstanceTypeDescriptor;

impl WebUiInstanceService {
    pub fn update_component_paths(
        &self,
        request: &UpdateWebUiInstanceComponentPathsRequest,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let record =
            self.recover_interrupted_install(self.repository.load_record(&request.instance_id)?)?;
        let provider = self.repository.types().require(&record.type_id)?;
        let declared = &provider.descriptor().components;
        let named: BTreeSet<&str> = request.component_paths.names().collect();
        if named != declared.iter().map(String::as_str).collect() {
            return Err(format!(
                "componentPaths must name exactly the components of type `{}`: {}",
                record.type_id,
                declared.join(", ")
            ));
        }
        let snapshot = self.get(&request.instance_id)?;
        let current = snapshot.instance.component_paths.as_ref();
        for (name, path) in request.component_paths.external() {
            // A folder already recorded as external is not checked again:
            // an attached installation keeps its layout.
            let kept = current.is_some_and(|current| {
                current.is_external(name)
                    && current
                        .get(name)
                        .is_some_and(|recorded| PathKey::of(recorded) == PathKey::of(path))
            });
            if !kept {
                check_external(provider.descriptor(), name, path)?;
            }
        }
        if matches!(
            record.lifecycle,
            WebUiInstanceLifecycle::Installing { .. } | WebUiInstanceLifecycle::Deleting
        ) {
            return Err(format!(
                "instance {} paths cannot change while lifecycle is {:?}",
                request.instance_id, record.lifecycle
            ));
        }
        if matches!(
            record.lifecycle,
            WebUiInstanceLifecycle::Created | WebUiInstanceLifecycle::InstallFailed { .. }
        ) {
            InstancePathPolicy::validate_installation_layout(
                &snapshot.install_dir,
                &request.component_paths,
                false,
                false,
            )?;
        }
        self.repository.update_component_paths(request)?;
        self.get(&request.instance_id)
    }
}

/// An external component is one its type allows to be, an existing folder
/// holding the program the type names for it. Checked here, where the
/// user chooses it.
fn check_external(
    descriptor: &InstanceTypeDescriptor,
    name: &str,
    path: &Path,
) -> Result<(), String> {
    let label = InstancePathPolicy::component_label(name);
    let component = descriptor.external_components.get(name).ok_or_else(|| {
        format!(
            "instance type `{}` does not let its {label} component be a folder of your own",
            descriptor.id
        )
    })?;
    let folder = ResolvedPath::resolve(path)
        .map_err(|error| format!("{label} folder {}: {error}", path.display()))?;
    if !folder.system().is_dir() {
        return Err(format!(
            "{label} folder {} is not a directory",
            folder.native_text()
        ));
    }
    if let Some(program) = &component.executable {
        let program = program
            .split('/')
            .fold(folder.system().to_path_buf(), |path, name| path.join(name));
        if !program.is_file() {
            return Err(format!(
                "{label} folder {} holds no {}",
                folder.native_text(),
                component.executable.as_deref().unwrap_or_default()
            ));
        }
    }
    Ok(())
}
