//! A type only discovery attaches (`attachOnly = true`): it is refused by
//! name when created or imported by hand, and imported with its signature's
//! external components as discovery imports it.

use std::collections::{BTreeMap, BTreeSet};

use super::declarative::setup_type;
use super::*;

const TYPE: &str = "test.fixtures:prefix";

const PREFIX: &str = r#"
name = "prefix"
displayName = "A prefix of the user's"
attachOnly = true
components = [{ name = "core", external = "allowed" }]

[[discovery]]
markers = ["bin/tool"]
components = { core = "." }
external = ["core"]
"#;

fn type_id() -> crate::internal::hanafubuki_instance_contracts::InstanceTypeId {
    crate::internal::hanafubuki_instance_contracts::InstanceTypeId::parse(TYPE).unwrap()
}

fn import_request(root: &Path) -> ImportWebUiInstanceRequest {
    ImportWebUiInstanceRequest {
        name: None,
        install_dir: root.to_path_buf(),
        component_paths: BTreeMap::from([("core".to_string(), root.to_path_buf())]),
        type_id: type_id(),
        spec: Default::default(),
    }
}

#[test]
fn an_attach_only_type_is_neither_created_nor_imported_by_hand() {
    let (root, service, _) = setup_type("attach-only-refused", PREFIX, vec![]);
    let descriptor = serde_json::to_value(
        service
            .repository
            .types()
            .require(&type_id())
            .unwrap()
            .descriptor(),
    )
    .unwrap();
    assert_eq!(descriptor["attachOnly"], true, "{descriptor}");

    let error = service
        .create(CreateWebUiInstanceRequest {
            name: None,
            install_dir: None,
            component_paths: Default::default(),
            type_id: type_id(),
            spec: Default::default(),
            preset: None,
        })
        .unwrap_err();
    assert!(error.contains("attached only by discovery"), "{error}");
    assert!(error.contains(TYPE), "{error}");

    let prefix = root.join("prefix");
    std::fs::create_dir_all(prefix.join("bin")).unwrap();
    let error = service
        .import_existing(import_request(&prefix), &AcceptInstancePython)
        .unwrap_err();
    assert!(error.contains("attached only by discovery"), "{error}");
    assert!(service.list().unwrap_or_default().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn discovery_imports_an_attach_only_type_with_its_external_components() {
    let (root, service, _) = setup_type("attach-only-attached", PREFIX, vec![]);
    let prefix = root.join("prefix");
    std::fs::create_dir_all(prefix.join("bin")).unwrap();

    let prepared = service
        .prepare_import_with_external(
            import_request(&prefix),
            &BTreeSet::from(["core".to_string()]),
        )
        .unwrap();
    let attached = service
        .commit_import(prepared, &AcceptInstancePython)
        .unwrap();
    assert!(matches!(
        attached.install.lifecycle,
        WebUiInstanceLifecycle::Ready { .. }
    ));
    let paths = attached.instance.component_paths.unwrap();
    assert!(paths.is_external("core"), "{paths:?}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_attach_only_type_without_a_discovery_signature_is_refused() {
    let issues = crate::internal::hanafubuki_webui::instance_types::DeclarativeProvider::from_toml(
        "test.fixtures",
        "name = \"prefix\"\ndisplayName = \"x\"\nattachOnly = true\ncomponents = [\"core\"]\n",
    )
    .err()
    .expect("refused");
    assert!(
        issues
            .0
            .iter()
            .any(|issue| issue.location == "attachOnly" && issue.message.contains("[[discovery]]")),
        "{issues}"
    );
}
