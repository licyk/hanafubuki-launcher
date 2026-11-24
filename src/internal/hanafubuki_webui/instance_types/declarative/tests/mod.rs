use std::path::PathBuf;
use std::sync::Arc;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_instance_contracts::{
    InstanceRuntime, InstanceTypeKind, ServerPort, ServerReadiness, ServerRestart, ServerRole,
    ServerStart, server_start_order,
};
use serde_json::{Map, Value, json};

use crate::internal::hanafubuki_foundation::form_schema::{SchemaText, SchemaValue};
use crate::internal::hanafubuki_foundation::module_faults::ContributionOwner;

use super::{DeclarativeProvider, DescriptorIssue};
use crate::internal::hanafubuki_webui::install::steps::{
    DownloadTarget, PlannedStep, StepOperation,
};
use crate::internal::hanafubuki_webui::instance_types::{
    InstanceLocation, InstanceProvider, InstanceTypeRegistry,
};
use crate::internal::hanafubuki_webui::instances::InstanceSpec;

pub(crate) mod fixtures {
    /// A type with no Python: an archive with its two servers, `api` and the
    /// `ui` the webview shows, which waits for it.
    pub(crate) const TWO_SERVERS: &str = r#"
name = "two-servers"
displayName = { en = "Two servers", zh = "两个服务" }
components = ["core", "data"]
runtime = { kind = "none" }
futureField = "ignored by this build"

[[install.parameters]]
name = "archiveUrl"
kind = "text"
required = true

[[install.parameters]]
name = "sha256"
kind = "text"
required = true

[[install.parameters]]
name = "uiPort"
kind = "number"
default = 18080

[[install.parameters]]
name = "flavour"
kind = "select"
options = ["plain", "fancy"]
default = "plain"

[[install.steps]]
kind = "download"
url = "{param.archiveUrl}"
sha256 = "{param.sha256}"
unpackTo = "{component.core}"

[[install.steps]]
kind = "write-file"
path = "{component.data}/settings.txt"
content = "flavour={param.flavour} instance={instance.id}"

[[update.steps]]
kind = "write-file"
path = "{component.data}/updated.txt"
content = "updated"

[management]
operations = ["update"]

[[servers]]
role = "api"
command = ["{component.core}/server", "--port", "{port}", "--name", "api"]
env = { SERVER_DATA = "{component.data}" }
readiness = { http = "/health", timeoutSeconds = 30 }
restart = "on-failure"

[[servers]]
role = "ui"
command = ["{component.core}/server", "--port", "{port}", "--name", "ui"]
port = { param = "uiPort" }
readiness = { tcp = true }
after = ["api"]
webview = true
"#;

    pub(crate) const NAMESPACE: &str = "test.fixtures";
}

fn provider() -> DeclarativeProvider {
    DeclarativeProvider::from_toml(fixtures::NAMESPACE, fixtures::TWO_SERVERS).unwrap()
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

fn issues(text: &str) -> Vec<DescriptorIssue> {
    match DeclarativeProvider::from_toml(fixtures::NAMESPACE, text) {
        Ok(_) => panic!("the descriptor is valid"),
        Err(issues) => issues.0,
    }
}

/// A descriptor of the fixture with `replace` swapped for `with`.
fn variant(replace: &str, with: &str) -> String {
    assert!(fixtures::TWO_SERVERS.contains(replace), "{replace}");
    fixtures::TWO_SERVERS.replacen(replace, with, 1)
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

fn spec(provider: &DeclarativeProvider, values: Value) -> InstanceSpec {
    provider.validate_spec(object(values)).unwrap()
}

fn sha() -> String {
    "ab".repeat(32)
}

#[test]
fn a_descriptor_declares_a_python_free_type_and_its_servers() {
    let provider = provider();
    let descriptor = provider.descriptor();
    assert_eq!(descriptor.id.as_str(), "test.fixtures:two-servers");
    assert_eq!(descriptor.kind, InstanceTypeKind::Declarative);
    assert_eq!(descriptor.components, ["core", "data"]);
    assert_eq!(descriptor.runtime, InstanceRuntime::None);
    let roles: Vec<&str> = server_start_order(&descriptor.servers)
        .unwrap()
        .into_iter()
        .map(|role| role.role.as_str())
        .collect();
    assert_eq!(roles, ["api", "ui"]);
    let api = &descriptor.servers[0];
    assert!(
        matches!(&api.start, ServerStart::Command { argv, .. } if argv[0] == "{component.core}/server")
    );
    assert_eq!(api.port, ServerPort::Allocate);
    assert_eq!(
        api.readiness,
        ServerReadiness::Http {
            path: "/health".to_string(),
            timeout_seconds: 30
        }
    );
    assert_eq!(api.restart, ServerRestart::OnFailure);
    let ui = &descriptor.servers[1];
    assert_eq!(
        ui.port,
        ServerPort::Config {
            key: "uiPort".to_string()
        }
    );
    assert_eq!(ui.after, [ServerRole::parse("api").unwrap()]);
    assert!(ui.webview && ui.required && !api.webview);
    assert!(provider.offers_update());
    let registry = InstanceTypeRegistry::with_builtin_types();
    registry
        .register(
            ContributionOwner::Extension("test".into()),
            Arc::new(provider),
        )
        .unwrap();
}

#[test]
fn a_declared_role_may_be_ready_on_the_address_it_opens() {
    let text = variant(
        "readiness = { tcp = true }",
        "readiness = { captured = true, timeoutSeconds = 45 }",
    );
    let provider = DeclarativeProvider::from_toml(fixtures::NAMESPACE, &text).unwrap();
    assert_eq!(
        provider.descriptor().servers[1].readiness,
        ServerReadiness::Captured {
            timeout_seconds: 45
        }
    );
}

#[test]
fn the_same_descriptor_reads_from_json() {
    let provider = DeclarativeProvider::from_json(
        "test.json",
        r#"{
            "name": "json-type",
            "displayName": "JSON type",
            "components": ["core"],
            "servers": [{
                "role": "ui",
                "command": ["{component.core}/server", "{port}"],
                "readiness": { "tcp": true },
                "webview": true
            }]
        }"#,
    )
    .unwrap();
    assert_eq!(provider.descriptor().id.as_str(), "test.json:json-type");
    assert_eq!(provider.descriptor().runtime, InstanceRuntime::None);
    let Err(error) = DeclarativeProvider::from_json("test.json", "{ \"name\": 3 }") else {
        panic!("a name is text");
    };
    assert_eq!(error.0[0].location, "line 1, column 11");
}

#[test]
fn registration_errors_say_where_they_are() {
    let cases = [
        (
            variant(
                "url = \"{param.archiveUrl}\"",
                "url = \"{param.releaseUrl}\"",
            ),
            "install.steps[0].url",
            "`{param.releaseUrl}` names `releaseUrl`, which is not a declared install parameter",
        ),
        (
            variant(
                "command = [\"{component.core}/server\", \"--port\", \"{port}\", \"--name\", \"api\"]",
                "command = [\"{component.models}/server\"]",
            ),
            "servers[0].command[0]",
            "`{component.models}` names `models`, which is not a declared component",
        ),
        (
            variant(
                "command = [\"{component.core}/server\", \"--port\", \"{port}\", \"--name\", \"api\"]",
                "command = []",
            ),
            "servers[0].command",
            "a command names its program first",
        ),
        (
            variant(
                "restart = \"on-failure\"",
                "restart = \"on-failure\"\nafter = [\"ui\"]",
            ),
            "servers",
            "server roles api, ui wait for each other in a cycle",
        ),
        (
            variant("after = [\"api\"]", "after = [\"worker\"]"),
            "servers",
            "server role `ui` starts after `worker`, which is not declared",
        ),
        (
            variant("content = \"updated\"", "content = \"{port}\""),
            "update.steps[0].content",
            "`{port}` is known only to a server's command",
        ),
        (
            variant(
                "path = \"{component.data}/settings.txt\"",
                "path = \"/etc/settings.txt\"",
            ),
            "install.steps[1].path",
            "path `/etc/settings.txt` does not start with a `{component.<name>}`",
        ),
        (
            variant("operations = [\"update\"]", "operations = [\"versions\"]"),
            "management.operations[0]",
            "`versions` is a built-in type's management, which a type offers by adopting one \
             (`[management] adopts = \"builtin:<name>\"`)",
        ),
        (
            variant(
                "port = { param = \"uiPort\" }",
                "port = { param = \"flavour\" }",
            ),
            "servers[1].port.param",
            "`flavour` is not a declared number parameter",
        ),
        (
            variant(
                "readiness = { tcp = true }",
                "readiness = { tcp = true, captured = true }",
            ),
            "servers[1].readiness",
            "readiness is one of `http = \"/path\"`, `tcp = true`, `stdout = \"<regex>\"` and `captured = true`",
        ),
        (
            variant(
                "readiness = { http = \"/health\", timeoutSeconds = 30 }",
                "readiness = { captured = true }",
            )
            .replace(
                "readiness = { tcp = true }",
                "readiness = { captured = true }",
            ),
            "servers",
            "server roles `api` and `ui` are both captured; an instance has one captured role",
        ),
        (
            variant(
                "runtime = { kind = \"none\" }",
                "runtime = { kind = \"python\", version = \"3.11\" }",
            ),
            "runtime",
            "a Python runtime needs the `python` component",
        ),
    ];
    for (text, location, message) in cases {
        let found = issues(&text);
        assert!(
            found
                .iter()
                .any(|issue| issue.location == location && issue.message == message),
            "{location}: {message}\n{found:?}"
        );
    }
}

#[test]
fn a_descriptor_that_does_not_decode_says_where() {
    let found = issues(
        "name = \"x\"\ndisplayName = \"X\"\ncomponents = [\"core\"]\n[[install.steps]]\nkind = \"teleport\"\n",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].location, "line 5, column 8");
    assert!(found[0].message.contains("teleport"), "{found:?}");
    let unclosed = issues(&variant(
        "content = \"updated\"",
        "content = \"{param.flavour\"",
    ));
    assert_eq!(unclosed[0].location, "update.steps[0].content");
    assert_eq!(unclosed[0].message, "`{param.flavour` is not closed");
}

#[test]
fn a_spec_is_checked_against_the_install_parameters() {
    let provider = provider();
    let InstanceSpec::Declarative(values) = spec(
        &provider,
        json!({ "archiveUrl": "http://x/a.zip", "sha256": sha() }),
    ) else {
        panic!("a declarative type reads its parameters");
    };
    assert_eq!(
        values.get("uiPort"),
        Some(&SchemaValue::Number(18080.into()))
    );
    assert_eq!(
        values.get("flavour"),
        Some(&SchemaValue::Text("plain".to_string()))
    );
    let refuse = |values: Value| provider.validate_spec(object(values)).unwrap_err();
    assert_eq!(
        refuse(json!({ "sha256": sha() })),
        "install parameters: `archiveUrl` is required"
    );
    assert_eq!(
        refuse(json!({ "archiveUrl": "u", "sha256": sha(), "flavour": "spicy" })),
        "install parameters: `flavour` takes one of plain, fancy"
    );
    assert_eq!(
        refuse(json!({ "archiveUrl": "u", "sha256": sha(), "uiPort": "80" })),
        "install parameters: `uiPort` takes a number"
    );
    assert_eq!(
        refuse(json!({ "archiveUrl": "u", "sha256": sha(), "model": "m" })),
        "install parameters: undeclared fields: model"
    );
    // A persisted spec outlives a parameter the type no longer takes.
    let read = provider
        .read_spec(object(
            json!({ "archiveUrl": "u", "sha256": sha(), "model": "m" }),
        ))
        .unwrap();
    assert_eq!(
        read,
        spec(&provider, json!({ "archiveUrl": "u", "sha256": sha() }))
    );
}

#[test]
fn a_plan_resolves_the_placeholders_for_one_instance() {
    let provider = provider();
    let spec = spec(
        &provider,
        json!({ "archiveUrl": "http://127.0.0.1:1/site.zip", "sha256": sha().to_uppercase(), "flavour": "fancy" }),
    );
    let location = location();
    let plan = provider
        .step_plan(StepOperation::Install, &spec, &location)
        .unwrap();
    assert_eq!(plan.python_dir, None);
    let [
        PlannedStep::Download {
            url,
            sha256,
            target,
        },
        PlannedStep::WriteFile { path, content },
    ] = plan.steps.as_slice()
    else {
        panic!("{:?}", plan.steps);
    };
    assert_eq!(url, "http://127.0.0.1:1/site.zip");
    assert_eq!(sha256, &sha());
    let DownloadTarget::UnpackInto {
        into,
        strip: 0,
        replace: false,
    } = target
    else {
        panic!("{target:?}");
    };
    assert_eq!(into.path(), location.components.core());
    assert_eq!(
        path.path(),
        location
            .components
            .get("data")
            .unwrap()
            .join("settings.txt")
    );
    assert_eq!(content, "flavour=fancy instance=one");

    let update = provider
        .step_plan(StepOperation::Update, &spec, &location)
        .unwrap();
    assert!(
        matches!(update.steps.as_slice(), [PlannedStep::WriteFile { content, .. }] if content == "updated")
    );
}

#[test]
fn a_value_that_would_leave_its_component_is_refused() {
    let text = variant(
        "path = \"{component.data}/settings.txt\"",
        "path = \"{component.data}/{param.flavour}\"",
    )
    .replace(
        "options = [\"plain\", \"fancy\"]",
        "options = [\"plain\", \"../../escape\"]",
    );
    let provider = DeclarativeProvider::from_toml(fixtures::NAMESPACE, &text).unwrap();
    let spec = spec(
        &provider,
        json!({ "archiveUrl": "u", "sha256": sha(), "flavour": "../../escape" }),
    );
    let error = provider
        .step_plan(StepOperation::Install, &spec, &location())
        .unwrap_err();
    assert_eq!(
        error,
        "step 2: `/../../escape` leaves its component; a path stays inside it"
    );
    let digest = provider
        .step_plan(
            StepOperation::Install,
            &self::spec(
                &provider,
                json!({ "archiveUrl": "u", "sha256": "not-a-digest" }),
            ),
            &location(),
        )
        .unwrap_err();
    assert_eq!(
        digest,
        "step 1: `not-a-digest` is not a SHA-256 of 64 hexadecimal digits"
    );
}

#[test]
fn a_server_command_and_port_are_resolved_for_one_start() {
    let provider = provider();
    let spec = spec(
        &provider,
        json!({ "archiveUrl": "u", "sha256": sha(), "uiPort": 18181 }),
    );
    let location = location();
    let api = ServerRole::parse("api").unwrap();
    let ui = ServerRole::parse("ui").unwrap();
    assert_eq!(provider.server_port(&api, &spec).unwrap(), None);
    assert_eq!(provider.server_port(&ui, &spec).unwrap(), Some(18181));
    let command = provider
        .server_command(&api, &spec, &location, 4321, &|| false)
        .unwrap();
    let core = location.components.core().to_str().unwrap();
    assert_eq!(
        command.argv,
        [
            format!("{core}/server"),
            "--port".into(),
            "4321".into(),
            "--name".into(),
            "api".into()
        ]
    );
    assert_eq!(
        command.env.vars.get("SERVER_DATA").map(String::as_str),
        location
            .components
            .get("data")
            .and_then(|path| path.to_str())
    );
    assert_eq!(command.cwd, location.components.core());
    assert!(
        provider
            .server_command(
                &ServerRole::parse("worker").unwrap(),
                &spec,
                &location,
                1,
                &|| false
            )
            .is_err()
    );
}

/// A descriptor whose install form uses every field kind, labels by locale
/// and conditions.
const FORM: &str = r#"
name = "form"
displayName = "Form"
components = ["core", "models"]

[[install.parameters]]
name = "source"
kind = "select"
label = { en = "Source", zh = "来源" }
options = ["release", { value = "local", label = { en = "Local file", zh = "本地文件" } }]
default = "release"

[[install.parameters]]
name = "archive"
kind = "path"
path = "file"
visibleWhen = { key = "source", equals = "local" }
requiredWhen = { key = "source", equals = "local" }

[[install.parameters]]
name = "token"
kind = "secret"
advanced = true

[[install.parameters]]
name = "threads"
kind = "slider"
min = 1
max = 16
step = 1
default = 4

[[install.parameters]]
name = "features"
kind = "multiselect"
options = ["cuda", "vulkan"]

[[install.parameters]]
name = "notes"
kind = "textarea"
help = "Written beside the model."
disabledWhen = { key = "source", equals = "local" }
disabledReason = "Only for a release."
"#;

#[test]
fn a_declarative_install_form_is_served_and_its_values_validated() {
    let provider = DeclarativeProvider::from_toml(fixtures::NAMESPACE, FORM).unwrap();
    let form = serde_json::to_value(provider.install_form()).unwrap();
    assert_eq!(form[0]["id"], "install");
    let fields = form[0]["fields"].as_array().unwrap();
    assert_eq!(
        fields[0],
        json!({
            "key": "source",
            "kind": "select",
            "label": { "kind": "literal", "text": { "en": "Source", "zh": "来源" } },
            "options": [
                { "value": "release", "label": { "kind": "literal", "text": { "en": "release" } } },
                { "value": "local", "label": { "kind": "literal", "text": { "en": "Local file", "zh": "本地文件" } } }
            ],
            "default": "release"
        })
    );
    assert_eq!(fields[1]["pathKind"], "file");
    assert_eq!(
        fields[1]["visibleWhen"],
        json!({ "key": "source", "equals": "local" })
    );
    assert_eq!(fields[2]["kind"], "secret");
    assert_eq!(fields[2]["advanced"], true);
    assert_eq!(fields[3]["min"], 1);
    assert_eq!(
        fields[5]["disabledReason"]["text"]["en"],
        "Only for a release."
    );

    let refuse = |values: Value| provider.validate_spec(object(values)).unwrap_err();
    assert_eq!(
        refuse(json!({ "source": "local" })),
        "install parameters: `archive` is required"
    );
    assert_eq!(
        refuse(json!({ "threads": 32 })),
        "install parameters: `threads` takes a number from 1 to 16 in steps of 1"
    );
    assert_eq!(
        refuse(json!({ "features": ["metal"] })),
        "install parameters: `features` takes a list of cuda, vulkan"
    );
    let InstanceSpec::Declarative(values) = spec(
        &provider,
        json!({ "archive": "/tmp/model.zip", "features": ["vulkan"], "token": "s3cret" }),
    ) else {
        panic!("a declarative type reads its parameters");
    };
    assert_eq!(
        values.get("archive"),
        None,
        "hidden while the source is a release"
    );
    assert_eq!(values.get("threads"), Some(&SchemaValue::Number(4.into())));
    assert_eq!(
        values.to_object(),
        object(json!({
            "source": "release",
            "threads": 4,
            "features": ["vulkan"],
            "token": "s3cret"
        }))
    );
}

#[test]
fn a_descriptor_with_an_unusable_install_form_says_which_parameter() {
    let issues = issues(
        r#"
name = "bad-form"
displayName = "Bad"
components = ["core"]

[[install.parameters]]
name = "level"
kind = "slider"

[[install.parameters]]
name = "mode"
kind = "select"
options = ["a"]
default = "b"
visibleWhen = { key = "missing", equals = true }
"#,
    );
    assert_eq!(
        issues,
        vec![
            DescriptorIssue {
                location: "install.parameters[1].default".to_string(),
                message: "`mode` takes one of a".to_string(),
            },
            DescriptorIssue {
                location: "install.parameters[0]".to_string(),
                message: "a slider has a min and a max".to_string(),
            },
            DescriptorIssue {
                location: "install.parameters[1]".to_string(),
                message: "a condition reads `missing`, which is not a field".to_string(),
            },
        ]
    );
}

#[test]
fn a_descriptor_names_message_keys_of_its_own_namespace_only() {
    let namespace = fixtures::NAMESPACE;
    let keyed = DeclarativeProvider::from_toml(
        namespace,
        &format!(
            r#"
name = "keyed"
displayName = {{ key = "ext.{namespace}.types.keyed" }}
components = ["core"]

[[install.parameters]]
name = "mode"
kind = "select"
label = {{ key = "ext.{namespace}.mode" }}
options = [{{ value = "a", label = {{ key = "ext.{namespace}.modes.a" }} }}]
"#
        ),
    )
    .unwrap();
    assert_eq!(
        keyed.descriptor().label,
        SchemaText::key(format!("ext['{namespace}'].types.keyed"))
    );
    assert_eq!(
        keyed.parameters()[0].options[0].label,
        SchemaText::key(format!("ext['{namespace}'].modes.a"))
    );

    let issues = issues(
        r#"
name = "foreign"
displayName = { key = "instances.title" }
components = ["core"]

[[install.parameters]]
name = "mode"
kind = "text"
help = { key = "ext.someone.else.help" }
"#,
    );
    assert_eq!(
        issues,
        vec![
            DescriptorIssue {
                location: "install.parameters[0]".to_string(),
                message: format!(
                    "message key `ext.someone.else.help` is outside `ext.{namespace}.`"
                ),
            },
            DescriptorIssue {
                location: "displayName".to_string(),
                message: format!("message key `instances.title` is outside `ext.{namespace}.`"),
            },
        ]
    );
}

#[test]
fn an_extension_registers_its_types_together_and_removes_only_its_own() {
    let registry = InstanceTypeRegistry::with_builtin_types();
    let builtin = registry.descriptors().len();
    let second = variant("name = \"two-servers\"", "name = \"other\"");
    let duplicate = DeclarativeProvider::from_toml(fixtures::NAMESPACE, fixtures::TWO_SERVERS);
    let extension = ContributionOwner::Extension(fixtures::NAMESPACE.to_string());
    let refused = registry.register_all(
        extension.clone(),
        [
            Arc::new(provider()) as Arc<dyn InstanceProvider>,
            Arc::new(duplicate.unwrap()),
        ],
    );
    assert_eq!(
        refused,
        Err(format!(
            "instance type `{}:two-servers` is already registered",
            fixtures::NAMESPACE
        ))
    );
    assert_eq!(registry.descriptors().len(), builtin);

    registry
        .register_all(
            extension.clone(),
            [
                Arc::new(provider()) as Arc<dyn InstanceProvider>,
                Arc::new(DeclarativeProvider::from_toml(fixtures::NAMESPACE, &second).unwrap()),
            ],
        )
        .unwrap();
    assert_eq!(registry.descriptors().len(), builtin + 2);
    // Each type records its provider's owner: the core's built-in ones,
    // the extension's own.
    let own: Vec<String> = registry
        .owned_by(&extension)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        own,
        [
            format!("{}:two-servers", fixtures::NAMESPACE),
            format!("{}:other", fixtures::NAMESPACE)
        ]
    );
    assert_eq!(registry.owned_by(&ContributionOwner::Core).len(), builtin);
    let ids: Vec<_> = registry
        .descriptors()
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect();
    registry.unregister(&ids);
    assert_eq!(registry.descriptors().len(), builtin);
    assert!(registry.owned_by(&extension).is_empty());
}

mod adopts;
mod control;
mod discovery;
mod extension;
mod isolation;
