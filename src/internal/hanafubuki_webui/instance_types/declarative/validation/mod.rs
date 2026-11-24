//! A decoded type document checked once, at registration, into the
//! provider that runs it: every placeholder named is declared, every path
//! starts in a declared component, every role is valid and the roles' start
//! order has no cycle. Each problem is reported with where it is. A type its
//! backend provides declares no steps and no commands: its backend's plans
//! are checked as they come, with the same checks (`super::backend`).

mod components;
mod discovery;
mod management;
mod steps;

use std::collections::BTreeMap;
use std::path::{Component, Path};

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_foundation::form_schema::{FieldKind, SchemaField, SchemaText};
use crate::internal::hanafubuki_instance_contracts::{
    InstanceRuntime, InstanceTypeDescriptor, InstanceTypeId, InstanceTypeKind, ServerPort,
    ServerReadiness, ServerRestart, ServerRole, ServerRoleDescriptor, ServerStart,
    server_start_order,
};
use crate::internal::hanafubuki_python_runtime::PythonVersion;
use serde_json::Value;

use super::document::{
    PortDocument, ReadinessDocument, RestartDocument, RuntimeDocument, ServerDocument,
    TextDocument, TypeDocument,
};
use super::parameters::{self, text};
use super::placeholders::Scope;
use super::{DeclarativeProvider, DescriptorIssue, ServerTemplate};
pub use management::is_standard_operation;
pub(super) use steps::{argv, environment, steps};

#[derive(Default)]
pub(super) struct Issues(pub(super) Vec<DescriptorIssue>);

impl Issues {
    pub(super) fn add(&mut self, location: impl Into<String>, message: impl Into<String>) {
        self.0.push(DescriptorIssue {
            location: location.into(),
            message: message.into(),
        });
    }

    /// `result`'s value, or `None` with its error recorded at `location`.
    fn take<T>(&mut self, location: impl Into<String>, result: Result<T, String>) -> Option<T> {
        result.map_err(|message| self.add(location, message)).ok()
    }
}

pub(super) fn validate(
    namespace: &str,
    document: TypeDocument,
) -> Result<DeclarativeProvider, Vec<DescriptorIssue>> {
    let mut issues = Issues::default();
    let id = issues.take(
        "name",
        InstanceTypeId::parse(&format!("{namespace}:{}", document.name)),
    );
    let backend = provided_by_backend(&mut issues, &document);
    let components::Declared {
        names: components,
        labels: component_labels,
        external: external_components,
    } = components::declared(&mut issues, namespace, document.components);
    let preview = document
        .preview
        .and_then(|path| issues.take("preview", package_path(path)));
    let (runtime, python_version) = runtime(&mut issues, document.runtime, &components);
    let parameters = parameters::fields(&mut issues.0, namespace, document.install.parameters);
    let names: Vec<String> = parameters.iter().map(|field| field.key.clone()).collect();
    let scope = Scope {
        components: &components,
        parameters: &names,
        port: false,
    };
    let install = steps(
        &mut issues,
        "install.steps",
        document.install.steps,
        &scope,
        &runtime,
    );
    let update = document
        .update
        .map(|update| steps(&mut issues, "update.steps", update.steps, &scope, &runtime));
    let environment = steps::type_environment(&mut issues, document.environment, &scope);
    let management = management::declared(
        &mut issues,
        &document.management,
        management::Type {
            backend,
            runtime: &runtime,
            core_may_be_external: external_components.contains_key(InstanceComponentPaths::CORE),
            control: document
                .servers
                .iter()
                .any(|server| server.role == ServerRole::CONTROL),
        },
    );
    let server_scope = Scope {
        port: true,
        ..scope
    };
    let (servers, templates) = servers(
        &mut issues,
        document.servers,
        &server_scope,
        &parameters,
        (backend, &runtime),
    );
    if let Err(error) = server_start_order(&servers) {
        issues.add("servers", error);
    }
    if document.attach_only && document.discovery.is_empty() {
        issues.add("attachOnly", super::documentation::ATTACH_DISCOVERY.message);
    }
    let discovery = discovery::signatures(
        &mut issues,
        document.discovery,
        id.as_ref(),
        &components,
        &external_components,
        &parameters,
    );
    let mut scoped = |location: &str, document: TextDocument| {
        issues.take(location, text(document).scoped_to(namespace))
    };
    let label = scoped("displayName", document.display_name);
    let description = match document.description {
        Some(description) => scoped("description", description),
        None => Some(SchemaText::localized(BTreeMap::new())),
    };
    match (id, label, description) {
        (Some(id), Some(label), Some(description)) if issues.0.is_empty() => {
            Ok(DeclarativeProvider {
                descriptor: InstanceTypeDescriptor {
                    id,
                    kind: InstanceTypeKind::Declarative,
                    label,
                    default_name: document
                        .default_name
                        .map(|name| name.trim().to_string())
                        .filter(|name| !name.is_empty()),
                    description,
                    preview: None,
                    preview_url: None,
                    components,
                    component_labels,
                    external_components,
                    runtime,
                    servers,
                    own_operations: document
                        .management
                        .operations
                        .iter()
                        .filter(|operation| !is_standard_operation(operation))
                        .cloned()
                        .collect(),
                    operations: document.management.operations,
                    checks_updates: management.checks_updates,
                    adopts: management.adopts,
                    attach_only: document.attach_only,
                },
                preview,
                parameters,
                install,
                update,
                offers_update: management.offers_update,
                servers: templates,
                environment,
                python_version,
                provided_by_backend: backend,
                discovery,
                backend: None,
                extension: None,
            })
        }
        _ => Err(issues.0),
    }
}

/// Whether the type's backend provides it (`provider = "backend"`), which
/// then declares no steps of its own.
fn provided_by_backend(issues: &mut Issues, document: &TypeDocument) -> bool {
    let backend = match document.provider.as_deref() {
        None | Some(super::documentation::DEFAULT_PROVIDER) => false,
        Some("backend") => true,
        Some(other) => {
            issues.add(
                "provider",
                format!("`{other}` is not a provider this version knows (declarative, backend)"),
            );
            false
        }
    };
    if backend {
        let planned = super::documentation::BACKEND_STEPS.message;
        if !document.install.steps.is_empty() {
            issues.add("install.steps", planned);
        }
        if document.update.is_some() {
            issues.add("update", planned);
        }
    }
    backend
}

/// A file's path in the extension's package: relative, `/`-separated, and
/// inside it. Whether the package has it is the extension store's check.
fn package_path(path: String) -> Result<String, String> {
    let inside = !path.is_empty()
        && !path.contains('\\')
        && Path::new(&path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if inside {
        Ok(path)
    } else {
        Err(format!("`{path}` is not a path inside the package"))
    }
}

fn runtime(
    issues: &mut Issues,
    runtime: RuntimeDocument,
    components: &[String],
) -> (InstanceRuntime, Option<PythonVersion>) {
    match runtime {
        RuntimeDocument::None => (InstanceRuntime::None, None),
        RuntimeDocument::SystemBinary { name } => {
            if name.trim().is_empty() || name.contains(['/', '\\']) {
                issues.add("runtime.name", format!("`{name}` is not a program name"));
            }
            (InstanceRuntime::SystemBinary { name }, None)
        }
        RuntimeDocument::Python { version } => {
            if !components
                .iter()
                .any(|name| name == InstanceComponentPaths::PYTHON)
            {
                issues.add("runtime", "a Python runtime needs the `python` component");
            }
            let parsed = issues.take(
                "runtime.version",
                serde_json::from_value::<PythonVersion>(Value::String(version.clone())).map_err(
                    |_| format!("Python {version} is not a version the application installs"),
                ),
            );
            (
                InstanceRuntime::Python {
                    version: Some(version),
                },
                parsed,
            )
        }
    }
}

/// The type's roles, and the template of each it runs a command for: every
/// role but `control`, which the supervisor starts as a built-in type's.
fn servers(
    issues: &mut Issues,
    declared: Vec<ServerDocument>,
    scope: &Scope<'_>,
    parameters: &[SchemaField],
    (backend, runtime): (bool, &InstanceRuntime),
) -> (Vec<ServerRoleDescriptor>, Vec<(ServerRole, ServerTemplate)>) {
    let mut servers = Vec::new();
    let mut templates = Vec::new();
    for (index, server) in declared.into_iter().enumerate() {
        let location = format!("servers[{index}]");
        if server.role == ServerRole::CONTROL {
            servers.extend(control_role(issues, &location, server, runtime));
        } else if let Some((descriptor, template)) =
            server_role(issues, &location, server, scope, parameters, backend)
        {
            templates.push((descriptor.role.clone(), template));
            servers.push(descriptor);
        }
    }
    (servers, templates)
}

/// The system's management API, as every built-in type declares it: the
/// control server on the instance's Python, its own probes, restarted on
/// failure. A type says only when it starts and whether it is required.
fn control_role(
    issues: &mut Issues,
    location: &str,
    server: ServerDocument,
    runtime: &InstanceRuntime,
) -> Option<ServerRoleDescriptor> {
    if !runtime.is_python() {
        issues.add(
            format!("{location}.role"),
            super::documentation::CONTROL_RUNTIME.message,
        );
    }
    let PortDocument {
        allocate,
        fixed,
        param,
    } = &server.port;
    let configured = server.command.is_some()
        || !server.env.is_empty()
        || allocate.is_some()
        || fixed.is_some()
        || param.is_some()
        || server.readiness.is_some()
        || server.restart.is_some()
        || server.webview;
    if configured {
        issues.add(location, super::documentation::CONTROL_CONFIG.message);
    }
    let after = after_roles(issues, location, &server.after);
    Some(ServerRoleDescriptor {
        role: ServerRole::control(),
        start: ServerStart::ControlServer,
        port: ServerPort::Allocate,
        readiness: ServerReadiness::Control,
        after: after?,
        restart: ServerRestart::OnFailure,
        webview: false,
        required: server.required,
    })
}

fn after_roles(issues: &mut Issues, location: &str, after: &[String]) -> Option<Vec<ServerRole>> {
    let parsed: Vec<Option<ServerRole>> = after
        .iter()
        .enumerate()
        .map(|(index, name)| {
            issues.take(
                format!("{location}.after[{index}]"),
                ServerRole::parse(name),
            )
        })
        .collect();
    parsed.into_iter().collect()
}

fn server_role(
    issues: &mut Issues,
    location: &str,
    server: ServerDocument,
    scope: &Scope<'_>,
    parameters: &[SchemaField],
    backend: bool,
) -> Option<(ServerRoleDescriptor, ServerTemplate)> {
    let role = issues.take(format!("{location}.role"), ServerRole::parse(&server.role));
    let after = after_roles(issues, location, &server.after);
    let (start, argv) = match (backend, server.command) {
        (false, Some(command)) => (
            ServerStart::Command {
                argv: command.clone(),
                env: server.env.clone(),
            },
            argv(issues, &format!("{location}.command"), &command, scope),
        ),
        (true, None) => (ServerStart::Backend, Some(Vec::new())),
        (false, None) => {
            issues.add(
                format!("{location}.command"),
                super::documentation::SERVER_COMMAND.message,
            );
            return None;
        }
        (true, Some(_)) => {
            issues.add(
                format!("{location}.command"),
                super::documentation::BACKEND_COMMAND.message,
            );
            return None;
        }
    };
    let env = environment(
        issues,
        &format!("{location}.env"),
        server.env.clone(),
        scope,
    );
    let (port, port_param) = port(issues, location, server.port, parameters)?;
    let Some(readiness) = server.readiness else {
        issues.add(
            format!("{location}.readiness"),
            super::documentation::READINESS_REQUIRED.message,
        );
        return None;
    };
    let readiness = self::readiness(issues, location, readiness)?;
    let descriptor = ServerRoleDescriptor {
        role: role?,
        start,
        port,
        readiness,
        after: after?,
        restart: match server.restart.unwrap_or_default() {
            RestartDocument::Never => ServerRestart::Never,
            RestartDocument::OnFailure => ServerRestart::OnFailure,
        },
        webview: server.webview,
        required: server.required,
    };
    Some((
        descriptor,
        ServerTemplate {
            argv: argv?,
            env: env?,
            port_param,
        },
    ))
}

fn port(
    issues: &mut Issues,
    location: &str,
    port: PortDocument,
    parameters: &[SchemaField],
) -> Option<(ServerPort, Option<String>)> {
    match (port.allocate, port.fixed, port.param) {
        (None | Some(true), None, None) => Some((ServerPort::Allocate, None)),
        (None, Some(fixed), None) if fixed > 0 => Some((ServerPort::Fixed { port: fixed }, None)),
        (None, None, Some(name)) => {
            let declared = parameters
                .iter()
                .any(|field| field.key == name && field.kind == FieldKind::Number);
            if !declared {
                issues.add(
                    format!("{location}.port.param"),
                    format!("`{name}` is not a declared number parameter"),
                );
                return None;
            }
            Some((ServerPort::Config { key: name.clone() }, Some(name)))
        }
        _ => {
            issues.add(
                format!("{location}.port"),
                super::documentation::PORT.message,
            );
            None
        }
    }
}

fn readiness(
    issues: &mut Issues,
    location: &str,
    readiness: ReadinessDocument,
) -> Option<ServerReadiness> {
    let timeout_seconds = readiness
        .timeout_seconds
        .unwrap_or(super::documentation::READINESS_TIMEOUT_SECONDS);
    let location = format!("{location}.readiness");
    let captured = readiness.captured == Some(true);
    match (readiness.http, readiness.tcp, readiness.stdout) {
        (None, None | Some(false), None) if captured => {
            Some(ServerReadiness::Captured { timeout_seconds })
        }
        _ if captured => {
            issues.add(location, super::documentation::READINESS_MODE.message);
            None
        }
        (Some(path), None | Some(false), None) => {
            if !path.starts_with('/') || path.chars().any(char::is_whitespace) {
                issues.add(location, format!("`{path}` is not an absolute URL path"));
                return None;
            }
            Some(ServerReadiness::Http {
                path,
                timeout_seconds,
            })
        }
        (None, Some(true), None) => Some(ServerReadiness::Tcp { timeout_seconds }),
        (None, None | Some(false), Some(pattern)) => {
            if let Err(error) = regex::Regex::new(&pattern) {
                issues.add(
                    location,
                    format!("`{pattern}` is not a regular expression: {error}"),
                );
                return None;
            }
            Some(ServerReadiness::Stdout {
                pattern,
                timeout_seconds,
            })
        }
        _ => {
            issues.add(location, super::documentation::READINESS_MODE.message);
            None
        }
    }
}
