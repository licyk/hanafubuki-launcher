use super::*;
use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;
use crate::internal::hanafubuki_webui::instance_types::{DeclarativeProvider, InstanceProvider};
use serde_json::json;

#[test]
fn declarative_default_names_are_normalized_and_exposed_in_json_and_toml() {
    for configured in [None, Some(""), Some("  \t"), Some("  My workspace  ")] {
        let mut document = json!({
            "name": "named", "displayName": "Display label", "components": ["core"],
            "futureField": true
        });
        if let Some(name) = configured {
            document["defaultName"] = json!(name);
        }
        let expected = configured.map(str::trim).filter(|name| !name.is_empty());
        for provider in [
            DeclarativeProvider::from_json("test", &document.to_string()).unwrap(),
            DeclarativeProvider::from_toml("test", &toml::to_string(&document).unwrap()).unwrap(),
        ] {
            assert_eq!(provider.descriptor().default_name.as_deref(), expected);
            let serialized = serde_json::to_value(provider.descriptor()).unwrap();
            match expected {
                Some(name) => assert_eq!(serialized["defaultName"], name),
                None => assert!(serialized.get("defaultName").is_none()),
            }
        }
    }
}

#[test]
fn create_and_import_use_default_name_unless_the_request_names_the_instance() {
    for (configured, label, expected) in [
        (Some("  Workspace  "), "\"Display label\"", "Workspace"),
        (
            Some("Workspace"),
            "{ key = \"ext.test.fixtures.label\" }",
            "Workspace",
        ),
        (None, "\"Display label\"", "Display label"),
        (Some("   "), "\"Display label\"", "Display label"),
        (
            None,
            "{ key = \"ext.test.fixtures.label\" }",
            "test.fixtures:named",
        ),
    ] {
        let default = configured
            .map(|value| format!("defaultName = {value:?}\n"))
            .unwrap_or_default();
        let document =
            format!("name = \"named\"\ndisplayName = {label}\n{default}components = [\"core\"]\n");
        let (root, service, _) = declarative::setup_type("default-name", &document, vec![]);
        let type_id = InstanceTypeId::parse("test.fixtures:named").unwrap();
        for (index, name) in [None, Some(" \t "), Some("  Personal name  ")]
            .into_iter()
            .enumerate()
        {
            let created = service
                .create(CreateWebUiInstanceRequest {
                    name: name.map(str::to_string),
                    type_id: type_id.clone(),
                    install_dir: None,
                    component_paths: Default::default(),
                    spec: Default::default(),
                    preset: None,
                })
                .unwrap();
            let install_dir = root.join(format!("import-{index}"));
            std::fs::create_dir_all(install_dir.join("core")).unwrap();
            let imported = service
                .import_existing(
                    ImportWebUiInstanceRequest {
                        name: name.map(str::to_string),
                        type_id: type_id.clone(),
                        install_dir,
                        component_paths: Default::default(),
                        spec: Default::default(),
                    },
                    &AcceptInstancePython,
                )
                .unwrap();
            for snapshot in [created, imported] {
                let persisted = service.get(&snapshot.instance.id).unwrap();
                assert_eq!(persisted.instance.name, snapshot.instance.name);
                if index == 2 {
                    assert_eq!(snapshot.instance.name, "Personal name");
                } else {
                    let suffix = snapshot
                        .instance
                        .name
                        .strip_prefix(&format!("{expected} "))
                        .unwrap();
                    assert_eq!(suffix.len(), 6);
                    assert!(suffix.bytes().all(|byte| byte.is_ascii_hexdigit()));
                }
            }
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
