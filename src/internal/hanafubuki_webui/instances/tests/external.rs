//! A component that is a folder of the user's own: chosen through the path
//! update, checked there, recorded, and left alone when the instance is
//! deleted.

use std::path::Path;

use super::declarative::setup_type;
use super::*;

const NAMESPACE_TYPE: &str = "test.fixtures:own-node";

const OWN_NODE: &str = r#"
name = "own-node"
displayName = "Own Node.js"
components = [
  "core",
  { name = "node", external = "allowed", executable = { unix = "bin/node", windows = "node.exe" } },
]

[[install.steps]]
kind = "write-file"
path = "{component.core}/installed.txt"
content = "yes"
"#;

fn create(service: &WebUiInstanceService) -> WebUiInstanceSnapshot {
    service
        .create(CreateWebUiInstanceRequest {
            name: None,
            install_dir: None,
            component_paths: Default::default(),
            type_id: crate::internal::hanafubuki_instance_contracts::InstanceTypeId::parse(
                NAMESPACE_TYPE,
            )
            .unwrap(),
            spec: Default::default(),
            preset: None,
        })
        .unwrap()
}

fn with_node(
    snapshot: &WebUiInstanceSnapshot,
    node: &Path,
    external: bool,
) -> InstanceComponentPaths {
    let mut paths = snapshot.instance.component_paths.clone().unwrap();
    match external {
        true => paths.set_external("node", node),
        false => paths.set("node", node),
    }
    paths
}

fn update(
    service: &WebUiInstanceService,
    snapshot: &WebUiInstanceSnapshot,
    component_paths: InstanceComponentPaths,
) -> Result<WebUiInstanceSnapshot, String> {
    service.update_component_paths(&UpdateWebUiInstanceComponentPathsRequest {
        instance_id: snapshot.instance.id.clone(),
        expected_config_revision: snapshot.instance.config_revision,
        component_paths,
    })
}

fn program(node: &Path) -> std::path::PathBuf {
    if cfg!(windows) {
        node.join("node.exe")
    } else {
        node.join("bin").join("node")
    }
}

#[test]
fn an_external_folder_must_hold_its_program_and_is_recorded_as_external() {
    let (root, service, _) = setup_type("external-node", OWN_NODE, vec![]);
    let created = create(&service);
    let node = root.join("users-node");
    std::fs::create_dir_all(&node).unwrap();

    let error = update(&service, &created, with_node(&created, &node, true)).unwrap_err();
    assert!(error.contains("holds no"), "{error}");

    std::fs::create_dir_all(program(&node).parent().unwrap()).unwrap();
    std::fs::write(program(&node), b"node").unwrap();
    let updated = update(&service, &created, with_node(&created, &node, true)).unwrap();
    // Saved and read back from the configuration.
    let reread = service.get(&created.instance.id).unwrap();
    for snapshot in [&updated, &reread] {
        let paths = snapshot.instance.component_paths.as_ref().unwrap();
        assert!(paths.is_external("node"));
        assert_eq!(paths.get("node").unwrap(), node);
    }
    // What plans and backends read is the resolved configuration.
    let location = service.location(&created.instance.id).unwrap();
    assert!(location.components.is_external("node"));
}

#[test]
fn only_a_component_its_type_allows_may_be_external() {
    let (root, service, _) = setup_type("external-core", OWN_NODE, vec![]);
    let created = create(&service);
    let core = root.join("users-core");
    std::fs::create_dir_all(&core).unwrap();
    let mut paths = created.instance.component_paths.clone().unwrap();
    paths.set_external("core", &core);

    let error = update(&service, &created, paths).unwrap_err();
    assert!(
        error.contains("does not let its Core component be"),
        "{error}"
    );
}

#[test]
fn deleting_the_instance_keeps_its_external_folder() {
    let (root, service, _) = setup_type("external-delete", OWN_NODE, vec![]);
    let created = create(&service);
    let node = root.join("users-node");
    std::fs::create_dir_all(program(&node).parent().unwrap()).unwrap();
    std::fs::write(program(&node), b"node").unwrap();
    let updated = update(&service, &created, with_node(&created, &node, true)).unwrap();
    std::fs::create_dir_all(updated.instance.component_paths.as_ref().unwrap().core()).unwrap();

    // The confirmation names what goes: the root, not the user's folder.
    assert_eq!(
        updated.deleted_directories,
        Some(vec![updated.install_dir.clone()])
    );
    service.delete_instance_files(&updated).unwrap();

    assert!(!updated.install_dir.exists());
    assert!(program(&node).is_file());
}

#[test]
fn an_owned_component_outside_the_root_is_deleted_with_the_instance() {
    let (root, service, _) = setup_type("owned-delete", OWN_NODE, vec![]);
    let created = create(&service);
    let node = root.join("owned-node");
    std::fs::create_dir_all(&node).unwrap();
    let updated = update(&service, &created, with_node(&created, &node, false)).unwrap();

    assert_eq!(
        updated.deleted_directories,
        Some(vec![node.clone(), updated.install_dir.clone()])
    );
    service.delete_instance_files(&updated).unwrap();
    assert!(!node.exists());
}
