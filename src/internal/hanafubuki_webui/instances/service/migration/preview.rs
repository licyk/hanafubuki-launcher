use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;
use crate::internal::hanafubuki_webui::instances::service::WebUiInstanceService;
use crate::internal::hanafubuki_webui::instances::{
    InstanceMaintenanceIntent, InstanceMaintenanceOperation, InstanceMigrationPlan,
    InstanceMigrationPreservedComponent, InstanceMigrationPreview, InstanceMigrationRequest,
    WebUiInstanceLifecycle, WebUiInstanceSnapshot,
};

impl WebUiInstanceService {
    pub fn preview_migration(
        &self,
        request: &InstanceMigrationRequest,
    ) -> Result<InstanceMigrationPreview, String> {
        let (_, plan) = self.prepare_migration_plan(request)?;
        migration_preview(&plan)
    }

    pub fn begin_migration(
        &self,
        request: &InstanceMigrationRequest,
    ) -> Result<(InstanceMaintenanceOperation, InstanceMigrationPlan), String> {
        let (_, plan) = self.prepare_migration_plan(request)?;
        let entry = self
            .maintenance
            .reserve(InstanceMaintenanceIntent::Migration(plan.clone()))?;
        let operation = self.maintenance.get_public(&entry.operation_id)?;
        Ok((operation, plan))
    }

    fn prepare_migration_plan(
        &self,
        request: &InstanceMigrationRequest,
    ) -> Result<(WebUiInstanceSnapshot, InstanceMigrationPlan), String> {
        let snapshot = self.get(&request.instance_id)?;
        snapshot.install.require_available()?;
        if snapshot.install.instance_identity != request.expected_instance_identity {
            return Err("instance identity changed; reload migration details".to_string());
        }
        if snapshot.instance.config_revision != request.expected_config_revision {
            return Err("instance configuration changed; reload migration details".to_string());
        }
        if !matches!(
            snapshot.install.lifecycle,
            WebUiInstanceLifecycle::Ready { .. }
        ) {
            return Err(format!(
                "instance {} can only migrate while ready",
                request.instance_id
            ));
        }

        if let Some(entry) = self
            .maintenance
            .pending()?
            .into_iter()
            .find(|entry| entry.intent.instance_id() == request.instance_id)
        {
            let InstanceMaintenanceIntent::Migration(plan) = entry.intent else {
                return Err(format!(
                    "instance {} already has unfinished lifecycle maintenance {}",
                    request.instance_id, entry.operation_id
                ));
            };
            if plan.expected_instance_identity != request.expected_instance_identity {
                return Err(
                    "unfinished migration belongs to a different instance identity".to_string(),
                );
            }
            let requested_parent = InstancePathPolicy::canonical_existing_migration_parent(
                "instance migration destination parent",
                &request.destination_parent_dir,
            )?;
            let durable_parent = plan.destination_install_dir.parent().ok_or_else(|| {
                "durable migration destination has no parent directory".to_string()
            })?;
            if !InstancePathPolicy::paths_refer_to_same_location(durable_parent, &requested_parent)
            {
                return Err(format!(
                    "instance {} has an unfinished migration to {}; choose its parent directory to retry",
                    request.instance_id,
                    plan.destination_install_dir.display()
                ));
            }
            return Ok((snapshot, plan));
        }

        let source_component_paths = snapshot
            .instance
            .component_paths
            .clone()
            .ok_or_else(|| "instance component paths are not configured".to_string())?;
        let paths = self.repository.plan_migration_paths(
            &request.instance_id,
            snapshot.install_dir.clone(),
            request.destination_parent_dir.clone(),
            source_component_paths,
        )?;
        let plan = InstanceMigrationPlan {
            instance_id: request.instance_id.clone(),
            expected_instance_identity: request.expected_instance_identity.clone(),
            expected_config_revision: request.expected_config_revision,
            source_install_dir: paths.source_install_dir,
            destination_install_dir: paths.destination_install_dir,
            source_component_paths: paths.source_component_paths,
            destination_component_paths: paths.destination_component_paths,
        };
        Ok((snapshot, plan))
    }
}

fn migration_preview(plan: &InstanceMigrationPlan) -> Result<InstanceMigrationPreview, String> {
    let mut preserved_components = Vec::new();
    for (component, path) in plan.source_component_paths.iter() {
        if !InstancePathPolicy::component_is_in_instance_root(&plan.source_install_dir, path) {
            preserved_components.push(InstanceMigrationPreservedComponent {
                component: InstancePathPolicy::component_label(component),
                path: path.to_path_buf(),
            });
        }
    }
    Ok(InstanceMigrationPreview {
        instance_id: plan.instance_id.clone(),
        source_install_dir: plan.source_install_dir.clone(),
        destination_parent_dir: plan
            .destination_install_dir
            .parent()
            .ok_or_else(|| "migration destination has no parent directory".to_string())?
            .to_path_buf(),
        destination_install_dir: plan.destination_install_dir.clone(),
        source_component_paths: plan.source_component_paths.clone(),
        destination_component_paths: plan.destination_component_paths.clone(),
        preserved_components,
        warnings: vec![
            "If a cross-filesystem copy is required, platform-specific ACLs, extended attributes, and alternate data streams are not preserved."
                .to_string(),
        ],
    })
}
