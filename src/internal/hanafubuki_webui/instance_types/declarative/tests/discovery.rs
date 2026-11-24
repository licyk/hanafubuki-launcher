//! A type's discovery signatures: checked at registration, served by the
//! provider and the registry.

use std::sync::Arc;

use crate::internal::hanafubuki_foundation::module_faults::ContributionOwner;

use super::super::{DeclarativeProvider, DescriptorIssue};
use crate::internal::hanafubuki_webui::discovery::RelativePath;
use crate::internal::hanafubuki_webui::instance_types::{InstanceProvider, InstanceTypeRegistry};

const NAMESPACE: &str = "com.example.dsh";

const DSH: &str = r#"
name = "dsh"
displayName = "DeepSeek Harness"
components = ["core", "node"]

[[install.parameters]]
name = "version"
kind = "text"
required = true

[[install.parameters]]
name = "nodeVersion"
kind = "text"
default = "24"

[[install.steps]]
kind = "write-file"
path = "{component.core}/version.txt"
content = "{param.version}"

[[discovery]]
markers = ["node_modules/@deepseek-ai/dsh/package.json"]
components = { core = ".", node = "node" }
values = { version = "latest" }

[[discovery]]
markers = ["dsh/package.json", "./bin/dsh"]
root = "app"
values = { version = "1.0.0", nodeVersion = "22" }
"#;

fn issues(text: &str) -> Vec<DescriptorIssue> {
    match DeclarativeProvider::from_toml(NAMESPACE, text) {
        Ok(_) => panic!("the descriptor is valid"),
        Err(issues) => issues.0,
    }
}

fn with_signature(signature: &str) -> String {
    format!("{}\n[[discovery]]\n{signature}\n", DSH)
}

fn locations(text: &str) -> Vec<(String, String)> {
    issues(text)
        .into_iter()
        .map(|issue| (issue.location, issue.message))
        .collect()
}

#[test]
fn a_type_declares_its_signatures_in_order() {
    let provider = DeclarativeProvider::from_toml(NAMESPACE, DSH).unwrap();
    let signatures = provider.discovery_signatures();
    assert_eq!(signatures.len(), 2);
    let first = &signatures[0];
    assert_eq!(first.type_id.as_str(), "com.example.dsh:dsh");
    assert_eq!(
        first.markers,
        vec![RelativePath::parse("node_modules/@deepseek-ai/dsh/package.json").unwrap()]
    );
    assert!(first.root.is_empty());
    assert!(first.components["core"].is_empty());
    assert_eq!(first.components["node"].components(), ["node"]);
    assert_eq!(first.values["version"], "latest");
    let second = &signatures[1];
    assert_eq!(second.markers[1].components(), ["bin", "dsh"]);
    assert_eq!(second.root.components(), ["app"]);
    assert!(second.components.is_empty());
}

#[test]
fn the_registry_serves_every_registered_signature_until_its_type_leaves() {
    let registry = InstanceTypeRegistry::with_builtin_types();
    assert!(registry.discovery_signatures().is_empty());
    let provider = DeclarativeProvider::from_toml(NAMESPACE, DSH).unwrap();
    let id = provider.descriptor().id.clone();
    registry
        .register(
            ContributionOwner::Extension(NAMESPACE.to_string()),
            Arc::new(provider),
        )
        .unwrap();
    assert_eq!(registry.discovery_signatures().len(), 2);
    registry.unregister(&[id]);
    assert!(registry.discovery_signatures().is_empty());
}

#[test]
fn markers_root_and_component_paths_stay_below_the_matched_directory() {
    let found = locations(&with_signature(
        r#"markers = ["../outside", "/etc/passwd", ".", "a\\b"]
root = "../.."
components = { core = "/abs", node = "nested/../up" }
values = { version = "1" }"#,
    ));
    let at = |location: &str| {
        found
            .iter()
            .any(|(found, _)| found == location)
            .then_some(())
            .unwrap_or_else(|| panic!("no issue at {location}: {found:?}"))
    };
    at("discovery[2].markers[0]");
    at("discovery[2].markers[1]");
    at("discovery[2].markers[2]");
    at("discovery[2].markers[3]");
    at("discovery[2].root");
    at("discovery[2].components.core");
    at("discovery[2].components.node");
    assert!(
        !found
            .iter()
            .any(|(location, _)| location.starts_with("discovery[0]")
                || location.starts_with("discovery[1]")),
        "{found:?}"
    );
}

#[test]
fn a_signature_names_markers_and_only_declared_components() {
    assert_eq!(
        locations(&with_signature(
            r#"markers = []
values = { version = "1" }"#
        )),
        vec![(
            "discovery[2].markers".to_string(),
            "a signature names at least one marker".to_string()
        )]
    );
    assert_eq!(
        locations(&with_signature(
            r#"markers = ["dsh"]
components = { python = "python" }
values = { version = "1" }"#
        )),
        vec![(
            "discovery[2].components.python".to_string(),
            "`python` is not a declared component".to_string()
        )]
    );
}

#[test]
fn a_signature_s_values_are_read_against_the_install_form() {
    let missing = locations(&with_signature(r#"markers = ["dsh"]"#));
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].0, "discovery[2].values");
    assert!(missing[0].1.contains("version"), "{missing:?}");

    let undeclared = locations(&with_signature(
        r#"markers = ["dsh"]
values = { version = "1", channel = "beta" }"#,
    ));
    assert_eq!(undeclared.len(), 1, "{undeclared:?}");
    assert_eq!(undeclared[0].0, "discovery[2].values");
    assert!(undeclared[0].1.contains("channel"), "{undeclared:?}");

    let wrong_kind = locations(&with_signature(
        r#"markers = ["dsh"]
values = { version = 3 }"#,
    ));
    assert_eq!(wrong_kind[0].0, "discovery[2].values", "{wrong_kind:?}");
}
