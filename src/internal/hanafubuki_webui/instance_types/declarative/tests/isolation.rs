//! What a type declares to isolate its instances: components that may be a
//! folder of the user's own, the inherited variables it unsets, `PATH`
//! entries in the instance root, and replacing unpacks; and the steps an
//! external component refuses.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use serde_json::json;

use super::super::DeclarativeProvider;
use super::{issues, object};
use crate::internal::hanafubuki_webui::install::steps::{
    DownloadTarget, PlannedStep, ProcessEnvironment, StepOperation,
};
use crate::internal::hanafubuki_webui::instance_types::{InstanceLocation, InstanceProvider};

const NAMESPACE: &str = "test.isolation";

const NODE_TYPE: &str = r#"
name = "node-app"
displayName = "Node app"
components = [
  "core",
  { name = "node", external = "allowed", executable = { unix = "bin/node", windows = "node.exe" } },
  { name = "data", external = "allowed" },
]

[environment]
unset = ["npm_config_*", "NODE_OPTIONS"]
path = ["{component.node}/bin", "{instance.root}/tools"]
vars = { npm_config_cache = "{instance.root}/.npm-cache" }

[[install.steps]]
kind = "download"
url = "https://example.invalid/node.tar.xz"
sha256 = "abababababababababababababababababababababababababababababababab"
unpackTo = "{component.node}"
strip = 1
replace = true

[[install.steps]]
kind = "write-file"
path = "{component.node}/etc/npmrc"
content = "prefix={component.core}"

[[discovery]]
markers = ["bin/node"]
components = { core = ".", node = "bin" }
external = ["node"]
"#;

fn provider() -> DeclarativeProvider {
    DeclarativeProvider::from_toml(NAMESPACE, NODE_TYPE).unwrap()
}

fn location(external_node: bool) -> InstanceLocation {
    let root = PathBuf::from(if cfg!(windows) {
        r"C:\instances\one"
    } else {
        "/instances/one"
    });
    let mut components = InstanceComponentPaths::defaults(&root, ["core", "node", "data"]);
    if external_node {
        components.set_external(
            "node",
            PathBuf::from(if cfg!(windows) {
                r"C:\Program Files\nodejs"
            } else {
                "/opt/node"
            }),
        );
    }
    InstanceLocation {
        instance_id: "one".to_string(),
        instance_root: root,
        components,
    }
}

#[test]
fn a_component_may_be_external_with_this_platforms_program() {
    let provider = provider();
    let external = &provider.descriptor().external_components;
    assert_eq!(
        external["node"].executable.as_deref(),
        Some(if cfg!(windows) {
            "node.exe"
        } else {
            "bin/node"
        })
    );
    assert_eq!(external["data"].executable, None);
    assert!(!external.contains_key("core"));
    assert_eq!(
        serde_json::to_value(provider.descriptor()).unwrap()["externalComponents"]["data"],
        json!({})
    );
    let signature = &provider.discovery_signatures()[0];
    assert_eq!(signature.external.iter().collect::<Vec<_>>(), ["node"]);
}

#[test]
fn what_external_takes_is_checked_where_it_is() {
    let found = issues(&NODE_TYPE.replace(
        r#"{ name = "data", external = "allowed" }"#,
        r#"{ name = "data", external = "always", executable = { unix = "../x" } }"#,
    ));
    assert_eq!(found[0].location, "components[2].external", "{found:?}");
    let found =
        issues(&NODE_TYPE.replace(r#"windows = "node.exe""#, r#"windows = "C:\\node.exe""#));
    assert_eq!(
        found[0].location, "components[1].executable.windows",
        "{found:?}"
    );
    let found = issues(&NODE_TYPE.replace(r#"external = ["node"]"#, r#"external = ["core"]"#));
    assert_eq!(found[0].location, "discovery[0].external[0]", "{found:?}");
    let found = issues(&NODE_TYPE.replace(
        r#"{ name = "data", external = "allowed" }"#,
        r#"{ name = "data", executable = { unix = "x" } }"#,
    ));
    assert_eq!(found[0].location, "components[2].executable", "{found:?}");
    let found = issues(&NODE_TYPE.replace(
        r#"unset = ["npm_config_*", "NODE_OPTIONS"]"#,
        r#"unset = ["*npm", ""]"#,
    ));
    let locations: Vec<&str> = found.iter().map(|issue| issue.location.as_str()).collect();
    assert_eq!(locations, ["environment.unset[0]", "environment.unset[1]"]);
    let found = issues(&NODE_TYPE.replace(
        "unpackTo = \"{component.node}\"\nstrip = 1",
        "to = \"{component.node}/n.tar.xz\"",
    ));
    assert!(
        found
            .iter()
            .any(|issue| issue.location == "install.steps[0].replace"),
        "{found:?}"
    );
}

#[test]
fn the_environment_unsets_and_names_the_instance_root() {
    let provider = provider();
    let location = location(false);
    let spec = provider.validate_spec(object(json!({}))).unwrap();
    let environment = provider.terminal_environment(&spec, &location).unwrap();
    assert_eq!(environment.unset, ["npm_config_*", "NODE_OPTIONS"]);
    assert_eq!(
        environment.path,
        [
            location.components.get("node").unwrap().join("bin"),
            location.instance_root.join("tools"),
        ]
    );
    assert_eq!(
        PathBuf::from(&environment.vars["npm_config_cache"]),
        location.instance_root.join(".npm-cache")
    );
}

#[test]
fn unset_removes_inherited_names_and_prefixes_the_type_does_not_set() {
    let environment = ProcessEnvironment {
        unset: vec!["npm_config_*".to_string(), "NODE_OPTIONS".to_string()],
        vars: BTreeMap::from([("npm_config_cache".to_string(), "/i/cache".to_string())]),
        ..ProcessEnvironment::default()
    };
    let inherited = [
        "npm_config_registry",
        "npm_config_cache",
        "HOME",
        "NPM_CONFIG_GLOBAL",
    ]
    .map(str::to_string);

    let env = environment
        .compose_over_inherited(&BTreeMap::new(), None, inherited, Ok)
        .unwrap();

    // The type's own variable applies after its `unset`.
    assert_eq!(env["npm_config_cache"].as_deref(), Some("/i/cache"));
    assert_eq!(env["npm_config_registry"], None);
    assert_eq!(env["NODE_OPTIONS"], None);
    assert!(!env.contains_key("HOME"));
    // Names compare as the system compares them.
    assert_eq!(env.contains_key("NPM_CONFIG_GLOBAL"), cfg!(windows));
}

#[test]
fn a_replacing_unpack_into_an_external_component_is_refused_with_where() {
    let provider = provider();
    let spec = provider.validate_spec(object(json!({}))).unwrap();

    let owned = provider
        .step_plan(StepOperation::Install, &spec, &location(false))
        .unwrap();
    let PlannedStep::Download {
        target: DownloadTarget::UnpackInto { replace: true, .. },
        ..
    } = &owned.steps[0]
    else {
        panic!("{:?}", owned.steps[0]);
    };

    let error = provider
        .step_plan(StepOperation::Install, &spec, &location(true))
        .unwrap_err();
    assert!(
        error.starts_with("step 1: `download` would replace files in"),
        "{error}"
    );
    assert!(error.contains("inside the external node folder"), "{error}");
}

#[test]
fn a_step_that_only_writes_may_write_in_an_external_component() {
    let provider = DeclarativeProvider::from_toml(
        NAMESPACE,
        &NODE_TYPE.replacen("[[install.steps]]", "[[ignored.steps]]", 1),
    )
    .unwrap();
    let spec = provider.validate_spec(object(json!({}))).unwrap();
    let plan = provider
        .step_plan(StepOperation::Install, &spec, &location(true))
        .unwrap();
    let PlannedStep::WriteFile { path, .. } = &plan.steps[0] else {
        panic!("{:?}", plan.steps);
    };
    assert!(path.external);
}

#[test]
fn a_replaced_folder_may_not_hold_another_component() {
    let provider = DeclarativeProvider::from_toml(
        NAMESPACE,
        &NODE_TYPE.replace(
            "unpackTo = \"{component.node}\"",
            "unpackTo = \"{component.core}\"",
        ),
    )
    .unwrap();
    let spec = provider.validate_spec(object(json!({}))).unwrap();
    let mut location = location(false);
    // A root-as-core layout: `node` lies inside `core`.
    location.components = InstanceComponentPaths::new(location.instance_root.clone())
        .with("node", location.instance_root.join("node"))
        .with("data", location.instance_root.join("data"));
    let error = provider
        .step_plan(StepOperation::Install, &spec, &location)
        .unwrap_err();
    assert!(error.contains("which holds the"), "{error}");
}
