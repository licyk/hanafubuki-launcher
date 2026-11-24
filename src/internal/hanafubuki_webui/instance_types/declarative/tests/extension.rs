//! What a type knows of its extension and gives its processes: its preview
//! and component labels, its `[environment]`, the `terminal` operation,
//! `strip`, `{extension.root}` and `{host.python}`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_foundation::form_schema::SchemaText;
use crate::internal::hanafubuki_instance_contracts::ServerRole;

use super::super::{DeclarativeProvider, ExtensionContext, HostPython, HostPythonPaths};
use super::{issues, sha};
use crate::internal::hanafubuki_webui::install::steps::{
    DownloadTarget, PlannedStep, ProcessEnvironment, StepOperation,
};
use crate::internal::hanafubuki_webui::instance_types::{
    BuiltinProvider, InstanceLocation, InstanceProvider,
};
use crate::internal::hanafubuki_webui::instances::InstanceSpec;

const NAMESPACE: &str = "com.example.node";

const NODE_TOOL: &str = r#"
name = "node-tool"
displayName = "Node tool"
preview = "images/preview.png"
components = ["core", { name = "node", label = { en = "Node.js", zh = "Node.js 运行时" } }]

[environment]
path = ["{component.node}", "{component.node}/bin"]
vars = { NPM_CONFIG_UPDATE_NOTIFIER = "false", TOOL_HOME = "{component.core}" }

[management]
operations = ["update", "terminal"]

[[install.steps]]
kind = "download"
url = "http://127.0.0.1:1/node.tar.gz"
sha256 = "SHA"
unpackTo = "{component.node}"
strip = 1

[[install.steps]]
kind = "run"
command = ["{host.python}", "{extension.root}/scripts/setup.py", "--into", "{component.core}"]
env = { NPM_CONFIG_UPDATE_NOTIFIER = "true" }

[[install.steps]]
kind = "run"
command = ["{component.node}/bin/npm", "install"]

[[install.steps]]
kind = "unpack"
archive = "{component.core}/extra.zip"
into = "{component.core}/extra"
strip = 2

[[servers]]
role = "ui"
command = ["{host.python}", "{extension.root}/launch.py", "--port", "{port}"]
env = { TOOL_HOME = "{component.core}/ui" }
readiness = { tcp = true }
webview = true
"#;

fn node_tool() -> String {
    NODE_TOOL.replace("SHA", &sha())
}

/// A managed Python at fixed paths, counting how often it was asked.
struct Runtime {
    paths: Result<HostPythonPaths, String>,
    asked: AtomicUsize,
}

impl HostPython for Runtime {
    fn resolve(&self, _abandon: &dyn Fn() -> bool) -> Result<HostPythonPaths, String> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.paths.clone()
    }
}

fn base() -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\x" } else { "/x" })
}

fn host_paths() -> HostPythonPaths {
    HostPythonPaths {
        python: base().join("runtime").join("python"),
        launcher_code: base().join("runtime").join("launcher-code"),
    }
}

fn extended(runtime: &Arc<Runtime>) -> DeclarativeProvider {
    DeclarativeProvider::from_toml(NAMESPACE, &node_tool())
        .unwrap()
        .with_extension(
            ExtensionContext {
                root: base().join("package"),
                host_python: runtime.clone(),
            },
            Some("hfext://com.example.node/images/preview.png".to_string()),
        )
}

fn runtime(paths: Result<HostPythonPaths, String>) -> Arc<Runtime> {
    Arc::new(Runtime {
        paths,
        asked: AtomicUsize::new(0),
    })
}

fn location() -> InstanceLocation {
    let root = base().join("instances").join("one");
    InstanceLocation {
        instance_id: "one".to_string(),
        components: InstanceComponentPaths::defaults(&root, ["core", "node"]),
        instance_root: root,
    }
}

fn spec(provider: &DeclarativeProvider) -> InstanceSpec {
    provider.validate_spec(serde_json::Map::new()).unwrap()
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

#[test]
fn a_type_serves_its_preview_url_and_component_labels() {
    let provider = DeclarativeProvider::from_toml(NAMESPACE, &node_tool()).unwrap();
    assert_eq!(provider.preview(), Some("images/preview.png"));
    let descriptor = provider.descriptor();
    assert_eq!(descriptor.components, ["core", "node"]);
    assert_eq!(descriptor.operations, ["update", "terminal"]);
    // `update` and `terminal` are standard: the host runs them, not a backend.
    assert!(descriptor.own_operations.is_empty());
    assert_eq!(
        serde_json::to_value(descriptor).unwrap()["ownOperations"],
        serde_json::json!([])
    );
    assert_eq!(
        descriptor.component_labels,
        BTreeMap::from([(
            "node".to_string(),
            SchemaText::localized(BTreeMap::from([
                ("en".to_string(), "Node.js".to_string()),
                ("zh".to_string(), "Node.js 运行时".to_string()),
            ]))
        )])
    );
    let served = serde_json::to_value(extended(&runtime(Ok(host_paths()))).descriptor()).unwrap();
    assert_eq!(
        served["previewUrl"],
        "hfext://com.example.node/images/preview.png"
    );
    assert_eq!(served["preview"], serde_json::Value::Null);
    assert!(served["componentLabels"]["node"].is_object());
    // A built-in type keeps its bundled key and serves neither.
    let builtin =
        serde_json::to_value(BuiltinProvider::all().next().unwrap().descriptor()).unwrap();
    assert!(builtin["preview"].is_string());
    assert!(builtin.get("previewUrl").is_none());
    assert!(builtin.get("componentLabels").is_none());
}

#[test]
fn the_additions_are_checked_where_they_are() {
    let cases = [
        (
            (
                "preview = \"images/preview.png\"",
                "preview = \"../preview.png\"",
            ),
            "preview",
            "`../preview.png` is not a path inside the package",
        ),
        (
            (
                "label = { en = \"Node.js\"",
                "label = { key = \"ext.other.node\"",
            ),
            "components[1].label",
            "",
        ),
        (
            (
                "path = [\"{component.node}\"",
                "path = [\"{extension.root}/node\"",
            ),
            "environment.path[0]",
            "does not start with a `{component.<name>}`",
        ),
        (
            (
                "TOOL_HOME = \"{component.core}\" }",
                "TOOL_HOME = \"{port}\" }",
            ),
            "environment.vars.TOOL_HOME",
            "`{port}` is known only to a server's command",
        ),
        (
            (
                "unpackTo = \"{component.node}\"",
                "to = \"{component.node}/node.tar.gz\"",
            ),
            "install.steps[0].strip",
            "it needs `unpackTo`",
        ),
        (
            (
                "into = \"{component.core}/extra\"",
                "into = \"{extension.root}/extra\"",
            ),
            "install.steps[3].into",
            "does not start with a `{component.<name>}`",
        ),
    ];
    for ((replace, with), location, message) in cases {
        assert!(NODE_TOOL.contains(replace), "{replace}");
        let found = issues(&node_tool().replacen(replace, with, 1));
        assert!(
            found
                .iter()
                .any(|issue| issue.location == location && issue.message.contains(message)),
            "{location}: {message}\n{found:?}"
        );
    }
}

#[test]
fn steps_and_servers_run_with_the_types_environment_and_the_host_python() {
    let runtime = runtime(Ok(host_paths()));
    let provider = extended(&runtime);
    let location = location();
    let spec = spec(&provider);
    let plan = provider
        .step_plan(StepOperation::Install, &spec, &location)
        .unwrap();
    // The runtime is asked once for the whole plan.
    assert_eq!(runtime.asked.load(Ordering::SeqCst), 1);
    let node = location.components.get("node").unwrap().to_path_buf();
    let core = location.components.core().to_path_buf();
    let type_path = vec![node.clone(), node.join("bin")];
    let [
        PlannedStep::Download { target, .. },
        PlannedStep::Run {
            command: setup,
            env: setup_env,
            ..
        },
        PlannedStep::Run {
            command: npm,
            env: npm_env,
            ..
        },
        PlannedStep::Unpack { into, strip, .. },
    ] = plan.steps.as_slice()
    else {
        panic!("{:?}", plan.steps);
    };
    let DownloadTarget::UnpackInto {
        into: node_into,
        strip: 1,
        replace: false,
    } = target
    else {
        panic!("{target:?}");
    };
    assert_eq!(node_into.path(), node);
    assert_eq!((into.path(), *strip), (core.join("extra"), 2));
    assert_eq!(
        setup,
        &[
            text(&host_paths().python),
            format!("{}/scripts/setup.py", text(&base().join("package"))),
            "--into".to_string(),
            text(&core),
        ]
    );
    assert_eq!(
        setup_env,
        &ProcessEnvironment {
            unset: Vec::new(),
            vars: BTreeMap::from([
                // The step's own variable over the type's.
                ("NPM_CONFIG_UPDATE_NOTIFIER".to_string(), "true".to_string()),
                ("TOOL_HOME".to_string(), text(&core)),
            ]),
            path: type_path.clone(),
            launcher_code: Some(host_paths().launcher_code),
        }
    );
    assert_eq!(npm[0], format!("{}/bin/npm", text(&node)));
    assert_eq!(npm_env.path, type_path);
    assert_eq!(npm_env.vars["NPM_CONFIG_UPDATE_NOTIFIER"], "false");
    assert_eq!(npm_env.launcher_code, None);

    let ui = ServerRole::parse("ui").unwrap();
    let command = provider
        .server_command(&ui, &spec, &location, 4321, &|| false)
        .unwrap();
    assert_eq!(command.argv[0], text(&host_paths().python));
    assert_eq!(command.argv[3], "4321");
    assert_eq!(command.env.vars["TOOL_HOME"], format!("{}/ui", text(&core)));
    assert_eq!(command.env.path, type_path);
    assert_eq!(command.env.launcher_code, Some(host_paths().launcher_code));

    let terminal = provider.terminal_environment(&spec, &location).unwrap();
    assert_eq!(terminal.vars["TOOL_HOME"], text(&core));
    assert_eq!(terminal.path, type_path);
    assert_eq!(terminal.launcher_code, None);
}

#[test]
fn the_host_python_is_resolved_when_used_and_only_for_an_extensions_type() {
    let location = location();
    let unready = extended(&runtime(Err(
        "the application's Python is not ready yet".into()
    )));
    let spec = spec(&unready);
    let error = unready
        .step_plan(StepOperation::Install, &spec, &location)
        .unwrap_err();
    assert!(
        error.contains(
            "`{host.python}` cannot be resolved: the application's Python is not ready yet"
        ),
        "{error}"
    );
    let ui = ServerRole::parse("ui").unwrap();
    assert!(
        unready
            .server_command(&ui, &spec, &location, 1, &|| false)
            .is_err()
    );
    // Its terminal names neither, so it opens while the runtime is not ready.
    assert!(unready.terminal_environment(&spec, &location).is_ok());

    let without = DeclarativeProvider::from_toml(NAMESPACE, &node_tool()).unwrap();
    let error = without
        .step_plan(StepOperation::Install, &spec, &location)
        .unwrap_err();
    assert!(error.contains("the type has no extension"), "{error}");
    assert_eq!(without.descriptor().preview_url, None);
}
