//! A type that adopts a built-in type's management (`[management]
//! adopts`): on Python with the `control` role and a `core` of its own, it
//! may declare the sections the management API serves for it.

use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;

use super::super::DeclarativeProvider;
use crate::internal::hanafubuki_webui::instance_types::InstanceProvider;

const NAMESPACE: &str = "test.adopts";

/// A fork of ComfyUI installed from a repository, managed as ComfyUI is.
const ADOPTING: &str = r#"
name = "comfyui-fork"
displayName = "ComfyUI fork"
components = ["core", "python"]
runtime = { kind = "python", version = "3.11" }

[[install.parameters]]
name = "repository"
kind = "text"
required = true

[[install.steps]]
kind = "git-clone"
url = "{param.repository}"
into = "{component.core}"

[[install.steps]]
kind = "python-env"

[[update.steps]]
kind = "git-pull"
dir = "{component.core}"

[management]
adopts = "builtin:comfyui"
operations = ["update", "terminal", "versions", "extensions", "models", "environment"]

[[servers]]
role = "control"

[[servers]]
role = "ui"
command = ["{component.python}/bin/python", "{component.core}/main.py", "--port", "{port}"]
readiness = { http = "/", timeoutSeconds = 300 }
after = ["control"]
webview = true
"#;

fn issues(text: &str) -> Vec<(String, String)> {
    match DeclarativeProvider::from_toml(NAMESPACE, text) {
        Ok(_) => panic!("the descriptor is valid"),
        Err(issues) => issues
            .0
            .into_iter()
            .map(|issue| (issue.location, issue.message))
            .collect(),
    }
}

const NEEDS_THE_API: &str = "a built-in type's management is served by the management API: \
                             adopting it needs `runtime = { kind = \"python\" }` and the \
                             `control` role";

#[test]
fn an_adopting_type_declares_the_sections_and_names_the_built_in_type() {
    let provider = DeclarativeProvider::from_toml(NAMESPACE, ADOPTING).unwrap();
    let descriptor = provider.descriptor();

    assert_eq!(
        descriptor.adopts,
        Some(InstanceTypeId::parse("builtin:comfyui").unwrap())
    );
    assert_eq!(
        descriptor.operations,
        [
            "update",
            "terminal",
            "versions",
            "extensions",
            "models",
            "environment"
        ]
    );
    // The sections are the application's: none is an operation of its own,
    // and its update check stays its backend's, which it has none of.
    assert!(descriptor.own_operations.is_empty());
    assert!(!descriptor.checks_updates);
    assert_eq!(
        serde_json::to_value(descriptor).unwrap()["adopts"],
        "builtin:comfyui"
    );
}

#[test]
fn a_type_that_adopts_nothing_serializes_no_adoption() {
    let text = ADOPTING
        .replace("adopts = \"builtin:comfyui\"\n", "")
        .replace(
            ", \"versions\", \"extensions\", \"models\", \"environment\"",
            "",
        );
    let provider = DeclarativeProvider::from_toml(NAMESPACE, &text).unwrap();
    assert_eq!(provider.descriptor().adopts, None);
    assert!(
        serde_json::to_value(provider.descriptor())
            .unwrap()
            .get("adopts")
            .is_none()
    );
}

#[test]
fn a_type_its_backend_provides_adopts_as_a_declarative_one_does() {
    let text = r#"
name = "comfyui-fork"
provider = "backend"
displayName = "ComfyUI fork"
components = ["core", "python"]
runtime = { kind = "python", version = "3.11" }

[management]
adopts = "builtin:sd-webui"
operations = ["update", "update-check", "versions", "environment", "rebuild-index"]

[[servers]]
role = "control"

[[servers]]
role = "ui"
readiness = { http = "/" }
after = ["control"]
"#;
    let provider = DeclarativeProvider::from_toml(NAMESPACE, text).unwrap();
    let descriptor = provider.descriptor();
    assert_eq!(
        descriptor.adopts,
        Some(InstanceTypeId::parse("builtin:sd-webui").unwrap())
    );
    assert_eq!(descriptor.own_operations, ["rebuild-index"]);
    assert!(descriptor.checks_updates);
}

#[test]
fn the_sections_need_an_adopted_type() {
    let text = ADOPTING.replace("adopts = \"builtin:comfyui\"\n", "");
    let expected: Vec<(String, String)> = ["versions", "extensions", "models", "environment"]
        .into_iter()
        .enumerate()
        .map(|(index, operation)| {
            (
                format!("management.operations[{}]", index + 2),
                format!(
                    "`{operation}` is a built-in type's management, which a type offers by \
                     adopting one (`[management] adopts = \"builtin:<name>\"`)"
                ),
            )
        })
        .collect();
    assert_eq!(issues(&text), expected);
}

#[test]
fn what_stays_a_built_in_types_is_refused_with_an_adopted_type_too() {
    for closed in ["launch-arguments", "snapshots", "hotpatch", "diagnostics"] {
        let text = ADOPTING.replace("\"environment\"]", &format!("\"{closed}\"]"));
        assert_eq!(
            issues(&text),
            [(
                "management.operations[5]".to_string(),
                format!(
                    "`{closed}` is the application's own management, which an extension's type \
                     cannot declare"
                )
            )],
            "{closed}"
        );
    }
}

#[test]
fn only_a_built_in_type_is_adopted() {
    for other in ["comfyui", "builtin:nothing", "test.adopts:comfyui-fork"] {
        let text = ADOPTING.replace("builtin:comfyui", other);
        assert_eq!(
            issues(&text),
            [(
                "management.adopts".to_string(),
                format!(
                    "`{other}` is not a built-in type (builtin:sd-webui, builtin:comfyui, \
                     builtin:invokeai, builtin:fooocus, builtin:sd-trainer, \
                     builtin:qwen-tts-webui, builtin:rvc-next-webui, builtin:mizuoto-tts-webui)"
                )
            )],
            "{other}"
        );
    }
}

#[test]
fn adopting_needs_python_and_the_management_api() {
    let without_control = ADOPTING
        .replace("[[servers]]\nrole = \"control\"\n\n", "")
        .replace("after = [\"control\"]\n", "");
    assert_eq!(
        issues(&without_control),
        [("management.adopts".to_string(), NEEDS_THE_API.to_string())]
    );

    let without_python = r#"
name = "tool"
displayName = "Tool"
components = ["core"]

[management]
adopts = "builtin:comfyui"
operations = ["versions"]

[[servers]]
role = "ui"
command = ["{component.core}/tool", "--port", "{port}"]
readiness = { tcp = true }
"#;
    assert_eq!(
        issues(without_python),
        [("management.adopts".to_string(), NEEDS_THE_API.to_string())]
    );
}

#[test]
fn a_type_whose_core_may_be_the_users_folder_cannot_adopt() {
    let text = ADOPTING.replace(
        "components = [\"core\", \"python\"]",
        "components = [{ name = \"core\", external = \"allowed\" }, \"python\"]",
    );
    assert_eq!(
        issues(&text),
        [(
            "management.adopts".to_string(),
            "a built-in type's management changes what `core` holds: a type whose `core` may be \
             an external folder cannot adopt it"
                .to_string()
        )]
    );
}
