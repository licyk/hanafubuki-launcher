//! The system's management API on an extension's type: the `control` role,
//! which only a type on Python may declare, beside the type's own roles.

use crate::internal::hanafubuki_instance_contracts::{
    ServerPort, ServerReadiness, ServerRestart, ServerRole, ServerStart, server_start_order,
};

use super::super::DeclarativeProvider;
use crate::internal::hanafubuki_webui::instance_types::InstanceProvider;

const NAMESPACE: &str = "test.control";

/// A Python type with the management API and its own HTTP API after it.
const PYTHON_API: &str = r#"
name = "python-api"
displayName = "Python API"
components = ["core", "python"]
runtime = { kind = "python", version = "3.11" }

[[install.steps]]
kind = "python-env"

[[servers]]
role = "control"

[[servers]]
role = "api"
command = ["{component.python}/bin/python", "-m", "http.server", "{port}"]
readiness = { http = "/", timeoutSeconds = 30 }
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

#[test]
fn a_python_type_declares_the_management_api_and_its_own_roles_after_it() {
    let provider = DeclarativeProvider::from_toml(NAMESPACE, PYTHON_API).unwrap();
    let servers = &provider.descriptor().servers;
    let control = &servers[0];
    assert_eq!(control.role, ServerRole::control());
    assert_eq!(control.start, ServerStart::ControlServer);
    assert_eq!(control.port, ServerPort::Allocate);
    assert_eq!(control.readiness, ServerReadiness::Control);
    assert_eq!(control.restart, ServerRestart::OnFailure);
    assert!(control.required && !control.webview && control.after.is_empty());
    assert!(matches!(servers[1].start, ServerStart::Command { .. }));
    assert_eq!(servers[1].after, [ServerRole::control()]);

    let order: Vec<&str> = server_start_order(servers)
        .unwrap()
        .into_iter()
        .map(|server| server.role.as_str())
        .collect();
    assert_eq!(order, ["control", "api"]);
}

/// The same type, its commands planned by its extension's backend.
const BACKEND_API: &str = r#"
name = "python-api"
provider = "backend"
displayName = "Python API"
components = ["core", "python"]
runtime = { kind = "python", version = "3.11" }

[[servers]]
role = "control"

[[servers]]
role = "api"
readiness = { http = "/" }
after = ["control"]
"#;

#[test]
fn a_type_its_backend_provides_may_declare_it_and_its_backend_plans_the_rest() {
    let provider = DeclarativeProvider::from_toml(NAMESPACE, BACKEND_API).unwrap();
    let starts: Vec<&ServerStart> = provider
        .descriptor()
        .servers
        .iter()
        .map(|server| &server.start)
        .collect();
    assert_eq!(starts, [&ServerStart::ControlServer, &ServerStart::Backend]);
}

#[test]
fn a_type_without_python_cannot_declare_it_and_is_told_why() {
    let text = PYTHON_API
        .replace(
            "components = [\"core\", \"python\"]",
            "components = [\"core\"]",
        )
        .replace(
            "runtime = { kind = \"python\", version = \"3.11\" }",
            "runtime = { kind = \"none\" }",
        )
        .replace("[[install.steps]]\nkind = \"python-env\"\n", "")
        .replace("{component.python}/bin/python", "{component.core}/server");
    assert_eq!(
        issues(&text),
        [(
            "servers[0].role".to_string(),
            "the `control` role is the management API, which runs on the instance's Python: \
             it needs `runtime = { kind = \"python\" }`"
                .to_string()
        )]
    );
}

#[test]
fn the_management_api_takes_no_command_port_or_readiness_of_its_own() {
    for configured in [
        "command = [\"{component.core}/server\"]",
        "port = { fixed = 8080 }",
        "readiness = { tcp = true }",
        "restart = \"never\"",
        "env = { A = \"b\" }",
    ] {
        let text = PYTHON_API.replacen(
            "role = \"control\"\n",
            &format!("role = \"control\"\n{configured}\n"),
            1,
        );
        assert_eq!(
            issues(&text),
            [(
                "servers[0]".to_string(),
                "the `control` role is the system's management API: it takes only `after` and `required`"
                    .to_string()
            )],
            "{configured}"
        );
    }
}

#[test]
fn every_other_role_still_declares_its_readiness() {
    let text = PYTHON_API.replace("readiness = { http = \"/\", timeoutSeconds = 30 }\n", "");
    assert_eq!(
        issues(&text),
        [(
            "servers[1].readiness".to_string(),
            "a server declares its readiness".to_string()
        )]
    );
}

#[test]
fn the_management_api_alone_gives_no_built_in_management_operations() {
    // The sections are a built-in type's, taken by adopting one
    // (`adopts.rs`): the role alone names no family to serve them as.
    let text = PYTHON_API.replace(
        "[[servers]]\nrole = \"control\"",
        "[management]\noperations = [\"update\", \"environment\"]\n\n[[servers]]\nrole = \"control\"",
    );
    assert_eq!(
        issues(&text),
        [(
            "management.operations[1]".to_string(),
            "`environment` is a built-in type's management, which a type offers by adopting one \
             (`[management] adopts = \"builtin:<name>\"`)"
                .to_string()
        )]
    );
    let backend = BACKEND_API.replace(
        "[[servers]]\nrole = \"control\"",
        "[management]\noperations = [\"snapshots\"]\n\n[[servers]]\nrole = \"control\"",
    );
    assert_eq!(
        issues(&backend),
        [(
            "management.operations[0]".to_string(),
            "`snapshots` is the application's own management, which an extension's type cannot declare"
                .to_string()
        )]
    );
}
