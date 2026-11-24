use super::*;
use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;

fn plan(root: &Path) -> InstanceMigrationPlan {
    let source = root.join("source");
    let destination = root.join("destination");
    InstanceMigrationPlan {
        instance_id: "one".to_string(),
        expected_instance_identity: "identity-one".to_string(),
        expected_config_revision: 1,
        source_install_dir: source.clone(),
        destination_install_dir: destination.clone(),
        source_component_paths: InstanceComponentPaths::defaults(
            &source,
            InstanceComponentPaths::PYTHON_AND_CORE,
        ),
        destination_component_paths: InstanceComponentPaths::defaults(
            &destination,
            InstanceComponentPaths::PYTHON_AND_CORE,
        ),
    }
}

fn temporary_root(label: &str) -> PathBuf {
    let root = crate::internal::hanafubuki_webui::test_support::temp_root().join(format!(
        "hanafubuki-instance-migration-{label}-{}",
        crate::internal::hanafubuki_webui::instances::identity::now_millis()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

mod platform;
mod preview;
mod transfer;
