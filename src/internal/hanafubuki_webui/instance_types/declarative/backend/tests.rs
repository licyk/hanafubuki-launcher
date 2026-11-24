use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_instance_contracts::{InstanceTypeId, ServerRole, ServerStart};
use parking_lot::Mutex;
use serde_json::{Map, Value, json};

use super::super::DeclarativeProvider;
use super::super::{ExtensionContext, HostPython, HostPythonPaths};
use super::{BackendCommand, TypeBackend, UpdateCheck};
use crate::internal::hanafubuki_webui::install::steps::{PlannedStep, StepOperation};
use crate::internal::hanafubuki_webui::instance_types::{InstanceLocation, InstanceProvider};

const PROVIDED: &str = r#"
name = "planned"
provider = "backend"
displayName = "Planned"
components = ["core", "data"]

[[install.parameters]]
name = "name"
kind = "text"
default = "notes"

[management]
operations = ["update", "clear-cache"]

[[servers]]
role = "ui"
env = { BASE = "{component.data}" }
readiness = { tcp = true }
webview = true
"#;

/// A backend that answers with what a test sets, and records what it was
/// asked.
#[derive(Default)]
struct Planner {
    issues: BTreeMap<String, String>,
    steps: Vec<Value>,
    command: BackendCommand,
    asked: Mutex<Vec<String>>,
}

impl TypeBackend for Planner {
    fn validate_spec(
        &self,
        _: &InstanceTypeId,
        values: &Map<String, Value>,
        _: Option<&InstanceLocation>,
    ) -> Result<BTreeMap<String, String>, String> {
        self.asked
            .lock()
            .push(format!("validate {}", Value::Object(values.clone())));
        Ok(self.issues.clone())
    }

    fn install_plan(
        &self,
        _: &InstanceTypeId,
        operation: StepOperation,
        _: &Map<String, Value>,
        instance: &InstanceLocation,
    ) -> Result<Vec<Value>, String> {
        self.asked
            .lock()
            .push(format!("plan {operation:?} {}", instance.instance_id));
        Ok(self.steps.clone())
    }

    fn launch_plan(
        &self,
        _: &InstanceTypeId,
        role: &ServerRole,
        port: u16,
        _: &Map<String, Value>,
        _: &InstanceLocation,
    ) -> Result<BackendCommand, String> {
        self.asked.lock().push(format!("launch {role} {port}"));
        Ok(self.command.clone())
    }

    fn check_update(
        &self,
        type_id: &InstanceTypeId,
        values: &Map<String, Value>,
        instance: &InstanceLocation,
    ) -> Result<UpdateCheck, String> {
        self.asked.lock().push(format!(
            "check {type_id} {} {}",
            Value::Object(values.clone()),
            instance.instance_id
        ));
        Ok(UpdateCheck {
            current: Some("1".to_string()),
            latest: Some("2".to_string()),
            has_update: true,
            notes: None,
        })
    }
}

fn provided(planner: Planner) -> (DeclarativeProvider, Arc<Planner>) {
    let planner = Arc::new(planner);
    let provider = DeclarativeProvider::from_toml("test.backend", PROVIDED)
        .unwrap()
        .with_backend(planner.clone());
    (provider, planner)
}

fn location() -> InstanceLocation {
    let root = PathBuf::from(if cfg!(windows) {
        r"C:\instances\one"
    } else {
        "/instances/one"
    });
    InstanceLocation {
        instance_id: "one".to_string(),
        components: InstanceComponentPaths::defaults(&root, ["core", "data"]),
        instance_root: root,
    }
}

fn issues(text: &str) -> Vec<(String, String)> {
    match DeclarativeProvider::from_toml("test.backend", text) {
        Ok(_) => panic!("the descriptor is valid"),
        Err(issues) => issues
            .0
            .into_iter()
            .map(|issue| (issue.location, issue.message))
            .collect(),
    }
}

#[test]
fn a_type_its_backend_provides_declares_no_steps_or_commands_but_may_offer_its_own_operations() {
    let provider = DeclarativeProvider::from_toml("test.backend", PROVIDED).unwrap();
    assert!(provider.provided_by_backend());
    assert!(provider.offers_update());
    let descriptor = provider.descriptor();
    assert_eq!(descriptor.operations, ["update", "clear-cache"]);
    // Its own are those its backend runs: none of the standard ones.
    assert_eq!(descriptor.own_operations, ["clear-cache"]);
    assert_eq!(descriptor.servers[0].start, ServerStart::Backend);

    let with_steps = format!(
        "{PROVIDED}\n[[install.steps]]\nkind = \"write-file\"\npath = \"{{component.core}}/a\"\ncontent = \"a\"\n"
    );
    assert_eq!(
        issues(&with_steps),
        [(
            "install.steps".to_string(),
            "a type its backend provides gets its steps from its backend".to_string()
        )]
    );
    let with_command = PROVIDED.replace("env = { BASE", "command = [\"server\"]\nenv = { BASE");
    assert_eq!(
        issues(&with_command),
        [(
            "servers[0].command".to_string(),
            "a type its backend provides gets its commands from its backend".to_string()
        )]
    );
    // Every type's diagnostics stay the application's, though no type declares them.
    let standard = PROVIDED.replace("\"clear-cache\"", "\"hotpatch\", \"diagnostics\", \"Bad\"");
    assert_eq!(
        issues(&standard),
        [
            (
                "management.operations[1]".to_string(),
                "`hotpatch` is the application's own management, which an extension's type cannot declare"
                    .to_string()
            ),
            (
                "management.operations[2]".to_string(),
                "`diagnostics` is the application's own management, which an extension's type cannot declare"
                    .to_string()
            ),
            (
                "management.operations[3]".to_string(),
                "operation `Bad` is not a name of [a-z0-9-]".to_string()
            ),
        ]
    );
    let unknown = PROVIDED.replace("provider = \"backend\"", "provider = \"plugin\"");
    assert_eq!(
        issues(&unknown)[0],
        (
            "provider".to_string(),
            "`plugin` is not a provider this version knows (declarative, backend)".to_string()
        )
    );
    // A declarative type still offers no operation of its own.
    let declarative = PROVIDED
        .replace("provider = \"backend\"\n", "")
        .replace("env = { BASE", "command = [\"server\"]\nenv = { BASE");
    assert_eq!(
        issues(&declarative)[0].0,
        "management.operations[1]".to_string()
    );
}

#[test]
fn update_check_is_a_standard_operation_only_a_type_its_backend_provides_offers() {
    let provider = DeclarativeProvider::from_toml("test.backend", PROVIDED).unwrap();
    assert!(!provider.descriptor().checks_updates);

    let checked = PROVIDED.replace("\"clear-cache\"", "\"clear-cache\", \"update-check\"");
    let provider = DeclarativeProvider::from_toml("test.backend", &checked).unwrap();
    let descriptor = provider.descriptor();
    assert!(descriptor.checks_updates);
    assert_eq!(
        descriptor.operations,
        ["update", "clear-cache", "update-check"]
    );
    // The application runs it, never `provider.manage.update-check`.
    assert_eq!(descriptor.own_operations, ["clear-cache"]);

    let declarative = PROVIDED
        .replace("provider = \"backend\"\n", "")
        .replace("env = { BASE", "command = [\"server\"]\nenv = { BASE")
        .replace("\"clear-cache\"", "\"update-check\"");
    assert_eq!(
        issues(&declarative),
        [(
            "management.operations[1]".to_string(),
            "`update-check` is answered by the type's backend: only a type its backend provides \
             (`provider = \"backend\"`) can offer it"
                .to_string()
        )]
    );
}

#[test]
fn update_check_needs_update_so_a_type_announces_only_an_update_it_can_apply() {
    let unapplied = PROVIDED.replace(
        "[\"update\", \"clear-cache\"]",
        "[\"clear-cache\", \"update-check\"]",
    );
    assert_eq!(
        issues(&unapplied),
        [(
            "management.operations[1]".to_string(),
            "`update-check` needs `update`: a type announces only an update it can apply"
                .to_string()
        )]
    );
}

#[test]
fn its_backend_checks_for_an_update_with_the_values_only_when_the_type_declares_it() {
    let (provider, planner) = provided(Planner::default());
    let spec = provider.read_spec(Map::new()).unwrap();
    assert_eq!(
        provider.check_update(&spec, &location()).unwrap_err(),
        "instance type `test.backend:planned` has no update check"
    );
    assert!(planner.asked.lock().is_empty());

    let checked = PROVIDED.replace("\"clear-cache\"", "\"update-check\"");
    let planner = Arc::new(Planner::default());
    let provider = DeclarativeProvider::from_toml("test.backend", &checked)
        .unwrap()
        .with_backend(planner.clone());
    let spec = provider
        .read_spec(json!({ "name": "kept" }).as_object().unwrap().clone())
        .unwrap();
    let check = provider.check_update(&spec, &location()).unwrap();
    assert_eq!(
        check,
        UpdateCheck {
            current: Some("1".to_string()),
            latest: Some("2".to_string()),
            has_update: true,
            notes: None,
        }
    );
    assert_eq!(
        planner.asked.lock().clone(),
        [r#"check test.backend:planned {"name":"kept"} one"#]
    );

    let unattached = DeclarativeProvider::from_toml("test.backend", &checked).unwrap();
    assert!(
        unattached
            .check_update(&spec, &location())
            .unwrap_err()
            .starts_with("extension backend unavailable")
    );
}

#[test]
fn without_its_backend_every_plan_fails_with_why_and_a_record_still_reads() {
    let provider = DeclarativeProvider::from_toml("test.backend", PROVIDED).unwrap();
    let error = provider.validate_spec(Map::new()).unwrap_err();
    assert_eq!(
        error,
        "extension backend unavailable: no backend provides type `test.backend:planned`"
    );
    let spec = provider.read_spec(Map::new()).unwrap();
    assert!(
        provider
            .step_plan(StepOperation::Install, &spec, &location())
            .unwrap_err()
            .starts_with("extension backend unavailable")
    );
}

#[test]
fn its_backend_checks_the_values_and_plans_steps_the_host_resolves() {
    let (provider, planner) = provided(Planner {
        issues: BTreeMap::from([("name".to_string(), "taken".to_string())]),
        ..Planner::default()
    });
    assert_eq!(
        provider
            .validate_spec(json!({ "name": "a" }).as_object().unwrap().clone())
            .unwrap_err(),
        "install parameters: `name`: taken"
    );
    // The host's own checks come first: an undeclared value never reaches it.
    assert!(
        provider
            .validate_spec(json!({ "other": 1 }).as_object().unwrap().clone())
            .is_err()
    );
    assert_eq!(planner.asked.lock().len(), 1);

    let (provider, planner) = provided(Planner {
        steps: vec![
            json!({ "kind": "write-file", "path": "{component.data}/a.txt", "content": "{param.name} {instance.id}", "later": true }),
            json!({ "kind": "run", "command": ["tool", "{component.core}"] }),
        ],
        ..Planner::default()
    });
    let spec = provider.read_spec(Map::new()).unwrap();
    let plan = provider
        .step_plan(StepOperation::Reinstall, &spec, &location())
        .unwrap();
    assert_eq!(planner.asked.lock().clone(), ["plan Reinstall one"]);
    let PlannedStep::WriteFile { path, content } = &plan.steps[0] else {
        panic!("{:?}", plan.steps[0]);
    };
    assert_eq!(path.component, "data");
    assert_eq!(content, "notes one");
    assert!(matches!(&plan.steps[1], PlannedStep::Run { command, .. } if command[0] == "tool"));
}

#[test]
fn a_plan_that_names_a_path_outside_its_components_or_an_unknown_step_is_refused() {
    for (step, expected) in [
        (
            json!({ "kind": "write-file", "path": "/etc/passwd", "content": "x" }),
            "its backend's install plan is refused: steps[0].path: path `/etc/passwd` does not start with a `{component.<name>}`",
        ),
        (
            json!({ "kind": "write-file", "path": "{component.cache}/a", "content": "x" }),
            "its backend's install plan is refused: steps[0].path:",
        ),
        (
            json!({ "kind": "format-disk" }),
            "its backend's install plan is refused: steps[0]: unknown variant `format-disk`",
        ),
        (
            json!({ "kind": "write-file", "path": "{component.core}/../../x", "content": "x" }),
            "its backend's install plan: steps[0]:",
        ),
    ] {
        let (provider, _) = provided(Planner {
            steps: vec![step],
            ..Planner::default()
        });
        let spec = provider.read_spec(Map::new()).unwrap();
        let error = provider
            .step_plan(StepOperation::Install, &spec, &location())
            .unwrap_err();
        assert!(error.starts_with(expected), "{error}");
    }
}

#[test]
fn its_backend_plans_a_role_command_over_the_declared_environment() {
    let (provider, planner) = provided(Planner {
        command: BackendCommand {
            argv: vec![
                "{component.core}/server".into(),
                "--port".into(),
                "{port}".into(),
            ],
            env: BTreeMap::from([("NAME".to_string(), "{param.name}".to_string())]),
        },
        ..Planner::default()
    });
    let spec = provider.read_spec(Map::new()).unwrap();
    let ui = ServerRole::parse("ui").unwrap();
    let command = provider
        .server_command(&ui, &spec, &location(), 4321, &|| false)
        .unwrap();
    assert_eq!(planner.asked.lock().clone(), ["launch ui 4321"]);
    assert_eq!(command.argv[1..], ["--port", "4321"]);
    assert!(command.argv[0].ends_with("server"));
    assert_eq!(command.env.vars["NAME"], "notes");
    assert!(command.env.vars["BASE"].ends_with("data"));

    let (provider, _) = provided(Planner {
        command: BackendCommand {
            argv: vec!["{secret}".into()],
            env: BTreeMap::new(),
        },
        ..Planner::default()
    });
    assert!(
        provider
            .server_command(&ui, &spec, &location(), 1, &|| false)
            .unwrap_err()
            .starts_with("its backend's launch plan is refused: argv[0]:")
    );
}

/// The managed Python at fixed paths.
struct FixedPython;

impl HostPython for FixedPython {
    fn resolve(&self, _abandon: &dyn Fn() -> bool) -> Result<HostPythonPaths, String> {
        Ok(HostPythonPaths {
            python: PathBuf::from("python"),
            launcher_code: PathBuf::from("launcher-code"),
        })
    }
}

#[test]
fn a_plan_may_strip_an_archive_and_run_the_host_python() {
    let planner = Arc::new(Planner {
        steps: vec![
            json!({ "kind": "unpack", "archive": "{component.data}/a.zip", "into": "{component.core}", "strip": 1 }),
            json!({ "kind": "run", "command": ["{host.python}", "{extension.root}/setup.py"] }),
        ],
        command: BackendCommand {
            argv: vec!["{host.python}".into(), "-m".into(), "server".into()],
            env: BTreeMap::new(),
        },
        ..Planner::default()
    });
    let provider = DeclarativeProvider::from_toml("test.backend", PROVIDED)
        .unwrap()
        .with_backend(planner)
        .with_extension(
            ExtensionContext {
                root: PathBuf::from("package"),
                host_python: Arc::new(FixedPython),
            },
            None,
        );
    let spec = provider.read_spec(Map::new()).unwrap();
    let plan = provider
        .step_plan(StepOperation::Install, &spec, &location())
        .unwrap();
    assert!(matches!(
        &plan.steps[0],
        PlannedStep::Unpack { strip: 1, .. }
    ));
    let PlannedStep::Run { command, env, .. } = &plan.steps[1] else {
        panic!("{:?}", plan.steps[1]);
    };
    assert_eq!(command[0], "python");
    assert!(command[1].ends_with("setup.py") && command[1].starts_with("package"));
    assert_eq!(env.launcher_code, Some(PathBuf::from("launcher-code")));

    let ui = ServerRole::parse("ui").unwrap();
    let command = provider
        .server_command(&ui, &spec, &location(), 4321, &|| false)
        .unwrap();
    assert_eq!(command.argv, ["python", "-m", "server"]);
    assert_eq!(
        command.env.launcher_code,
        Some(PathBuf::from("launcher-code"))
    );
}
