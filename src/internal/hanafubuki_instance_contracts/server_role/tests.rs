use serde_json::json;

use super::{
    ServerPort, ServerReadiness, ServerRestart, ServerRole, ServerRoleDescriptor, ServerStart,
    server_start_order,
};

fn role(name: &str, after: &[&str]) -> ServerRoleDescriptor {
    ServerRoleDescriptor {
        role: ServerRole::parse(name).unwrap(),
        start: ServerStart::LaunchWorkflow,
        port: ServerPort::Allocate,
        readiness: ServerReadiness::Tcp { timeout_seconds: 5 },
        after: after
            .iter()
            .map(|name| ServerRole::parse(name).unwrap())
            .collect(),
        restart: ServerRestart::Never,
        webview: false,
        required: true,
    }
}

fn order(roles: &[ServerRoleDescriptor]) -> Vec<&str> {
    server_start_order(roles)
        .unwrap()
        .into_iter()
        .map(|role| role.role.as_str())
        .collect()
}

#[test]
fn role_names_are_of_the_role_alphabet() {
    for valid in ["ui", "control", "api-2"] {
        assert_eq!(ServerRole::parse(valid).unwrap().as_str(), valid);
    }
    for invalid in ["", "UI", "a b", "a:b", &"x".repeat(65)] {
        assert!(ServerRole::parse(invalid).is_err(), "{invalid}");
    }
    assert!(serde_json::from_value::<ServerRole>(json!("Ui")).is_err());
}

#[test]
fn roles_start_after_the_roles_they_wait_for() {
    let roles = [
        role("ui", &["api", "worker"]),
        role("worker", &[]),
        role("api", &["worker"]),
    ];
    assert_eq!(order(&roles), ["worker", "api", "ui"]);
    let independent = [role("b", &[]), role("a", &[])];
    assert_eq!(order(&independent), ["b", "a"]);
}

#[test]
fn a_cycle_in_the_start_order_is_an_invalid_descriptor() {
    let roles = [role("ui", &["api"]), role("api", &["ui"]), role("log", &[])];
    assert_eq!(
        server_start_order(&roles).unwrap_err(),
        "server roles ui, api wait for each other in a cycle"
    );
    let own = [role("ui", &["ui"])];
    assert!(server_start_order(&own).is_err());
}

#[test]
fn an_undeclared_or_duplicate_role_is_an_invalid_descriptor() {
    assert_eq!(
        server_start_order(&[role("ui", &["api"])]).unwrap_err(),
        "server role `ui` starts after `api`, which is not declared"
    );
    assert_eq!(
        server_start_order(&[role("ui", &[]), role("ui", &[])]).unwrap_err(),
        "server role `ui` is declared twice"
    );
}

#[test]
fn a_readiness_the_start_cannot_provide_is_an_invalid_descriptor() {
    let mut captured = role("api", &[]);
    captured.start = ServerStart::ControlServer;
    captured.readiness = ServerReadiness::Captured { timeout_seconds: 5 };
    assert!(server_start_order(&[captured]).is_err());
    let mut control = role("api", &[]);
    control.readiness = ServerReadiness::Control;
    assert!(server_start_order(&[control]).is_err());
    let mut command = role("api", &[]);
    command.start = ServerStart::Command {
        argv: vec!["{component.core}/server".to_string()],
        env: Default::default(),
    };
    assert!(server_start_order(std::slice::from_ref(&command)).is_ok());
    command.readiness = ServerReadiness::Tcp { timeout_seconds: 5 };
    command.readiness = ServerReadiness::Tcp { timeout_seconds: 5 };
    command.start = ServerStart::Command {
        argv: vec![" ".to_string()],
        env: Default::default(),
    };
    assert_eq!(
        server_start_order(&[command]).unwrap_err(),
        "server role `api`: its command names no program"
    );
}

#[test]
fn a_declared_or_backend_role_may_be_captured_once_per_type() {
    let captured = |name: &str, start: ServerStart| {
        let mut role = role(name, &[]);
        role.start = start;
        role.readiness = ServerReadiness::Captured {
            timeout_seconds: 30,
        };
        role
    };
    let command = ServerStart::Command {
        argv: vec!["{component.core}/server".to_string()],
        env: Default::default(),
    };
    for start in [
        ServerStart::LaunchWorkflow,
        command.clone(),
        ServerStart::Backend,
    ] {
        assert!(server_start_order(&[captured("web", start)]).is_ok());
    }
    assert_eq!(
        server_start_order(&[
            captured("web", command),
            captured("admin", ServerStart::Backend)
        ])
        .unwrap_err(),
        "server roles `web` and `admin` are both captured; an instance has one captured role"
    );
    let decoded: ServerReadiness = serde_json::from_value(json!({ "kind": "captured" })).unwrap();
    assert_eq!(
        decoded,
        ServerReadiness::Captured {
            timeout_seconds: 120
        }
    );
}

#[test]
fn a_declared_role_decodes_with_its_defaults_and_unknown_fields() {
    let decoded: ServerRoleDescriptor = serde_json::from_value(json!({
        "role": "ui",
        "start": { "kind": "launchWorkflow" },
        "port": { "kind": "fixed", "port": 8080 },
        "readiness": { "kind": "http", "path": "/health" },
        "restart": "on-failure",
        "webview": true,
        "description": "a field this build does not know",
    }))
    .unwrap();
    assert_eq!(
        decoded.readiness,
        ServerReadiness::Http {
            path: "/health".to_string(),
            timeout_seconds: 120,
        }
    );
    assert_eq!(decoded.port, ServerPort::Fixed { port: 8080 });
    assert_eq!(decoded.restart, ServerRestart::OnFailure);
    assert!(decoded.after.is_empty());
    assert!(decoded.webview && decoded.required);
}

#[test]
fn an_unknown_readiness_kind_is_refused() {
    let error = serde_json::from_value::<ServerRoleDescriptor>(json!({
        "role": "ui",
        "start": { "kind": "launchWorkflow" },
        "port": { "kind": "allocate" },
        "readiness": { "kind": "telepathy" },
        "restart": "never",
    }))
    .unwrap_err();
    assert!(error.to_string().contains("telepathy"), "{error}");
}
