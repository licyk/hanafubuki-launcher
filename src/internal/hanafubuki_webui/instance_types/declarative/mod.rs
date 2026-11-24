//! A declarative instance type: a descriptor the application interprets.
//!
//! The descriptor (TOML or JSON, `docs/rules/instance-types.md`) declares
//! the type's components, runtime, install parameters, install and update
//! steps from a closed vocabulary, its servers' commands, and the
//! environment its processes get. It is decoded tolerantly and checked
//! once, when the provider is built; resolving it for one instance only
//! substitutes placeholders. An active extension's
//! descriptors are its providers; a type its backend provides
//! (`provider = "backend"`) asks the backend for its plans (`backend`).

mod backend;
mod destructive;
mod document;
mod documentation;
mod extension;
mod parameters;
mod placeholders;
mod validation;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::internal::hanafubuki_foundation::form_schema::{
    SchemaField, SchemaGroup, SchemaText, SchemaValue,
};
use crate::internal::hanafubuki_instance_contracts::{InstanceTypeDescriptor, ServerRole};
use crate::internal::hanafubuki_python_runtime::PythonVersion;
use serde_json::{Map, Value};

use crate::internal::hanafubuki_webui::discovery::DiscoverySignature;
use crate::internal::hanafubuki_webui::install::INSTALL_GROUP;
use crate::internal::hanafubuki_webui::install::steps::{
    DownloadTarget, PlannedStep, ProcessEnvironment, StepOperation, StepPlan,
};
use crate::internal::hanafubuki_webui::instances::InstanceSpec;

pub use backend::{BackendCommand, TypeBackend, UpdateCheck};
pub(in crate::internal::hanafubuki_webui::instance_types) use document::TextDocument;
#[cfg(feature = "schema")]
pub use document::descriptor_schema;
pub use extension::{ExtensionContext, HostPython, HostPythonPaths};
pub use parameters::DeclarativeSpec;
pub(in crate::internal::hanafubuki_webui::instance_types) use parameters::text;
use placeholders::{PathEntryTemplate, PathTemplate, Template, Values};
pub use validation::is_standard_operation;

use super::{InstanceLocation, InstanceProvider, ServerCommand};

/// One problem with a descriptor, and where it is: a field path such as
/// `servers[1].command[0]`, or a line and column for one that does not
/// decode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorIssue {
    pub location: String,
    pub message: String,
}

/// Why a descriptor is not a type the application can run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorIssues(pub Vec<DescriptorIssue>);

impl fmt::Display for DescriptorIssues {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let issues: Vec<String> = self
            .0
            .iter()
            .map(|issue| format!("{}: {}", issue.location, issue.message))
            .collect();
        write!(
            formatter,
            "invalid instance type descriptor: {}",
            issues.join("; ")
        )
    }
}

pub struct DeclarativeProvider {
    descriptor: InstanceTypeDescriptor,
    /// The preview image's path in the extension's package.
    preview: Option<String>,
    parameters: Vec<SchemaField>,
    install: Vec<StepTemplate>,
    update: Option<Vec<StepTemplate>>,
    offers_update: bool,
    servers: Vec<(ServerRole, ServerTemplate)>,
    environment: EnvironmentTemplate,
    python_version: Option<PythonVersion>,
    provided_by_backend: bool,
    discovery: Vec<DiscoverySignature>,
    /// The extension's backend, for a type it provides.
    backend: Option<Arc<dyn TypeBackend>>,
    /// The extension that registered the type.
    extension: Option<ExtensionContext>,
}

/// The type's `[environment]`.
struct EnvironmentTemplate {
    unset: Vec<String>,
    path: Vec<PathEntryTemplate>,
    vars: BTreeMap<String, Template>,
}

#[derive(Clone, Debug)]
enum StepTemplate {
    Download {
        url: Template,
        sha256: Template,
        target: TargetTemplate,
    },
    Unpack {
        archive: PathTemplate,
        into: PathTemplate,
        strip: u32,
        replace: bool,
    },
    GitClone {
        url: Template,
        into: PathTemplate,
        branch: Option<Template>,
    },
    GitPull {
        repository: PathTemplate,
    },
    Run {
        command: Vec<Template>,
        cwd: PathTemplate,
        env: BTreeMap<String, Template>,
    },
    PythonEnv,
    WriteFile {
        path: PathTemplate,
        content: Template,
    },
    Probe {
        url: Template,
        timeout: Duration,
    },
}

#[derive(Clone, Debug)]
enum TargetTemplate {
    File(PathTemplate),
    UnpackInto {
        into: PathTemplate,
        strip: u32,
        replace: bool,
    },
}

#[derive(Clone, Debug)]
struct ServerTemplate {
    argv: Vec<Template>,
    env: BTreeMap<String, Template>,
    /// The number parameter that chooses the role's port.
    port_param: Option<String>,
}

impl DeclarativeProvider {
    /// A type `<namespace>:<name>` from a TOML descriptor.
    pub fn from_toml(namespace: &str, text: &str) -> Result<Self, DescriptorIssues> {
        let document = toml::from_str(text).map_err(|error| {
            let location = error
                .span()
                .map(|span| line_and_column(text, span.start))
                .unwrap_or_else(|| "descriptor".to_string());
            DescriptorIssues(vec![DescriptorIssue {
                location,
                message: error.message().to_string(),
            }])
        })?;
        validation::validate(namespace, document).map_err(DescriptorIssues)
    }

    /// A type `<namespace>:<name>` from a JSON descriptor.
    pub fn from_json(namespace: &str, text: &str) -> Result<Self, DescriptorIssues> {
        let document = serde_json::from_str(text).map_err(|error| {
            DescriptorIssues(vec![DescriptorIssue {
                location: format!("line {}, column {}", error.line(), error.column()),
                message: error.to_string(),
            }])
        })?;
        validation::validate(namespace, document).map_err(DescriptorIssues)
    }

    /// The fields of the type's install form.
    pub fn parameters(&self) -> &[SchemaField] {
        &self.parameters
    }

    /// Whether its extension's backend provides it (`provider = "backend"`).
    pub fn provided_by_backend(&self) -> bool {
        self.provided_by_backend
    }

    /// The type, its plans asked of `backend` when its backend provides it.
    pub fn with_backend(mut self, backend: Arc<dyn TypeBackend>) -> Self {
        self.backend = Some(backend);
        self
    }

    /// The path in its extension's package of the type's preview image.
    pub fn preview(&self) -> Option<&str> {
        self.preview.as_deref()
    }

    /// The type as `extension` registers it, its preview served at
    /// `preview_url`.
    pub fn with_extension(
        mut self,
        extension: ExtensionContext,
        preview_url: Option<String>,
    ) -> Self {
        self.extension = Some(extension);
        self.descriptor.preview_url = preview_url;
        self
    }

    fn values<'a>(
        &'a self,
        spec: &'a InstanceSpec,
        location: &'a InstanceLocation,
        port: Option<u16>,
        abandon: &'a dyn Fn() -> bool,
    ) -> Result<Values<'a>, String> {
        Ok(Values::new(
            location,
            self.spec(spec)?,
            port,
            self.extension.as_ref(),
            abandon,
        ))
    }

    /// A process's environment: the type's variables under `own`, its
    /// `PATH` entries, and the launcher code when `program` is
    /// `{host.python}`.
    fn process_environment(
        &self,
        own: BTreeMap<String, String>,
        program: Option<&Template>,
        values: &Values<'_>,
    ) -> Result<ProcessEnvironment, String> {
        let mut vars = render_env(&self.environment.vars, values)?;
        vars.extend(own);
        let path = self
            .environment
            .path
            .iter()
            .map(|entry| entry.render(values))
            .collect::<Result<_, _>>()?;
        let launcher_code = match program {
            Some(program) if program.is_host_python() => {
                Some(values.host_python()?.launcher_code.clone())
            }
            _ => None,
        };
        Ok(ProcessEnvironment {
            unset: self.environment.unset.clone(),
            vars,
            path,
            launcher_code,
        })
    }

    /// A request's values, checked by the form and then by the backend of a
    /// type it provides, which sees the instance when it exists.
    fn validate_values(
        &self,
        spec: Map<String, Value>,
        instance: Option<&InstanceLocation>,
    ) -> Result<InstanceSpec, String> {
        let spec = DeclarativeSpec::read(&self.parameters, spec, true)?;
        if let Some(backend) = self.backend()? {
            let issues = backend.validate_spec(&self.descriptor.id, &spec.to_object(), instance)?;
            if !issues.is_empty() {
                let issues: Vec<String> = issues
                    .into_iter()
                    .map(|(name, message)| format!("`{name}`: {message}"))
                    .collect();
                return Err(format!("install parameters: {}", issues.join("; ")));
            }
        }
        Ok(InstanceSpec::Declarative(spec))
    }

    /// The backend a type it provides asks; for a declarative one, `None`.
    fn backend(&self) -> Result<Option<&dyn TypeBackend>, String> {
        match (self.provided_by_backend, &self.backend) {
            (false, _) => Ok(None),
            (true, Some(backend)) => Ok(Some(backend.as_ref())),
            (true, None) => Err(format!(
                "extension backend unavailable: no backend provides type `{}`",
                self.descriptor.id
            )),
        }
    }

    fn spec<'a>(&self, spec: &'a InstanceSpec) -> Result<&'a DeclarativeSpec, String> {
        match spec {
            InstanceSpec::Declarative(spec) => Ok(spec),
            _ => Err(format!(
                "instance type `{}` reads only its own install parameters",
                self.descriptor.id
            )),
        }
    }

    fn server(&self, role: &ServerRole) -> Result<&ServerTemplate, String> {
        self.servers
            .iter()
            .find(|(declared, _)| declared == role)
            .map(|(_, template)| template)
            .ok_or_else(|| {
                format!(
                    "instance type `{}` declares no server role `{role}`",
                    self.descriptor.id
                )
            })
    }
}

impl InstanceProvider for DeclarativeProvider {
    fn descriptor(&self) -> &InstanceTypeDescriptor {
        &self.descriptor
    }

    fn install_form(&self) -> Vec<SchemaGroup> {
        vec![SchemaGroup {
            id: INSTALL_GROUP.to_string(),
            label: SchemaText::key("instances.install.sections.install"),
            fields: self.parameters.clone(),
        }]
    }

    fn validate_spec(&self, spec: Map<String, Value>) -> Result<InstanceSpec, String> {
        self.validate_values(spec, None)
    }

    fn validate_instance_spec(
        &self,
        spec: Map<String, Value>,
        location: &InstanceLocation,
    ) -> Result<InstanceSpec, String> {
        self.validate_values(spec, Some(location))
    }

    fn read_spec(&self, spec: Map<String, Value>) -> Result<InstanceSpec, String> {
        DeclarativeSpec::read(&self.parameters, spec, false).map(InstanceSpec::Declarative)
    }

    fn step_plan(
        &self,
        operation: StepOperation,
        spec: &InstanceSpec,
        location: &InstanceLocation,
    ) -> Result<StepPlan, String> {
        let values = self.values(spec, location, None, &never)?;
        let python_dir = self
            .python_version
            .map(|_| {
                location
                    .components
                    .require("python")
                    .map(|dir| dir.to_path_buf())
            })
            .transpose()?;
        if let Some(backend) = self.backend()? {
            let planned = backend.install_plan(
                &self.descriptor.id,
                operation,
                &values.spec().to_object(),
                location,
            )?;
            return self.backend_steps(planned, &values, python_dir);
        }
        let templates = match (operation, &self.update) {
            (StepOperation::Update, Some(update)) => update,
            _ => &self.install,
        };
        let steps = templates
            .iter()
            .enumerate()
            .map(|(index, step)| {
                self.plan_step(step, &values)
                    .map_err(|error| format!("step {}: {error}", index + 1))
            })
            .collect::<Result<_, _>>()?;
        Ok(StepPlan { steps, python_dir })
    }

    fn offers_update(&self) -> bool {
        self.offers_update
    }

    fn check_update(
        &self,
        spec: &InstanceSpec,
        location: &InstanceLocation,
    ) -> Result<UpdateCheck, String> {
        self.backend_update_check(spec, location)
    }

    fn update_check_ready(&self) -> bool {
        self.backend()
            .ok()
            .flatten()
            .is_some_and(|backend| backend.running())
    }

    fn server_port(&self, role: &ServerRole, spec: &InstanceSpec) -> Result<Option<u16>, String> {
        let declared = self
            .descriptor
            .servers
            .iter()
            .find(|server| &server.role == role)
            .ok_or_else(|| {
                format!(
                    "instance type `{}` declares no server role `{role}`",
                    self.descriptor.id
                )
            })?;
        match (&declared.port, &self.server(role)?.port_param) {
            (crate::internal::hanafubuki_instance_contracts::ServerPort::Fixed { port }, _) => {
                Ok(Some(*port))
            }
            (_, Some(param)) => match self.spec(spec)?.get(param) {
                Some(SchemaValue::Number(port)) => port
                    .as_u64()
                    .and_then(|port| u16::try_from(port).ok())
                    .filter(|port| *port > 0)
                    .map(Some)
                    .ok_or_else(|| format!("install parameter `{param}` is not a port: {port}")),
                _ => Err(format!("install parameter `{param}` has no port")),
            },
            _ => Ok(None),
        }
    }

    fn server_command(
        &self,
        role: &ServerRole,
        spec: &InstanceSpec,
        location: &InstanceLocation,
        port: u16,
        abandon: &dyn Fn() -> bool,
    ) -> Result<ServerCommand, String> {
        let server = self.server(role)?;
        let values = self.values(spec, location, Some(port), abandon)?;
        let env = render_env(&server.env, &values)?;
        let (argv, env) = match self.backend()? {
            Some(backend) => {
                let planned = backend.launch_plan(
                    &self.descriptor.id,
                    role,
                    port,
                    &values.spec().to_object(),
                    location,
                )?;
                self.backend_command(planned, &values, env)?
            }
            None => {
                let env = self.process_environment(env, server.argv.first(), &values)?;
                (render_all(&server.argv, &values)?, env)
            }
        };
        Ok(ServerCommand {
            argv,
            env,
            cwd: location.components.core().to_path_buf(),
        })
    }

    fn terminal_environment(
        &self,
        spec: &InstanceSpec,
        location: &InstanceLocation,
    ) -> Result<ProcessEnvironment, String> {
        let values = self.values(spec, location, None, &never)?;
        self.process_environment(BTreeMap::new(), None, &values)
    }

    fn discovery_signatures(&self) -> &[DiscoverySignature] {
        &self.discovery
    }
}

impl DeclarativeProvider {
    /// One step resolved for the instance; one that would remove or replace
    /// what an external component holds is refused (`destructive`).
    fn plan_step(&self, step: &StepTemplate, values: &Values<'_>) -> Result<PlannedStep, String> {
        let planned = self.resolve_step(step, values)?;
        destructive::check(&planned, &values.location().components)?;
        Ok(planned)
    }

    fn resolve_step(
        &self,
        step: &StepTemplate,
        values: &Values<'_>,
    ) -> Result<PlannedStep, String> {
        Ok(match step {
            StepTemplate::Download {
                url,
                sha256,
                target,
            } => PlannedStep::Download {
                url: url.render(values)?,
                sha256: sha256_value(&sha256.render(values)?)?,
                target: match target {
                    TargetTemplate::File(path) => DownloadTarget::File(path.render(values)?),
                    TargetTemplate::UnpackInto {
                        into,
                        strip,
                        replace,
                    } => DownloadTarget::UnpackInto {
                        into: into.render(values)?,
                        strip: *strip,
                        replace: *replace,
                    },
                },
            },
            StepTemplate::Unpack {
                archive,
                into,
                strip,
                replace,
            } => PlannedStep::Unpack {
                archive: archive.render(values)?,
                into: into.render(values)?,
                strip: *strip,
                replace: *replace,
            },
            StepTemplate::GitClone { url, into, branch } => PlannedStep::GitClone {
                url: url.render(values)?,
                into: into.render(values)?,
                branch: branch
                    .as_ref()
                    .map(|branch| branch.render(values))
                    .transpose()?,
            },
            StepTemplate::GitPull { repository } => PlannedStep::GitPull {
                repository: repository.render(values)?,
            },
            StepTemplate::Run { command, cwd, env } => PlannedStep::Run {
                command: render_all(command, values)?,
                cwd: cwd.render(values)?,
                env: self.process_environment(render_env(env, values)?, command.first(), values)?,
            },
            StepTemplate::PythonEnv => PlannedStep::PythonEnv {
                python_dir: values
                    .location()
                    .components
                    .require("python")?
                    .to_path_buf(),
                version: self
                    .python_version
                    .ok_or_else(|| "the type declares no Python version".to_string())?,
            },
            StepTemplate::WriteFile { path, content } => PlannedStep::WriteFile {
                path: path.render(values)?,
                content: content.render(values)?,
            },
            StepTemplate::Probe { url, timeout } => PlannedStep::Probe {
                url: url.render(values)?,
                timeout: *timeout,
            },
        })
    }
}

/// The `abandon` of a plan nothing supersedes.
fn never() -> bool {
    false
}

fn render_all(templates: &[Template], values: &Values<'_>) -> Result<Vec<String>, String> {
    templates
        .iter()
        .map(|template| template.render(values))
        .collect()
}

fn render_env(
    env: &BTreeMap<String, Template>,
    values: &Values<'_>,
) -> Result<BTreeMap<String, String>, String> {
    env.iter()
        .map(|(name, value)| Ok((name.clone(), value.render(values)?)))
        .collect()
}

fn sha256_value(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err(format!(
            "`{value}` is not a SHA-256 of 64 hexadecimal digits"
        ))
    }
}

pub(in crate::internal::hanafubuki_webui::instance_types) fn line_and_column(
    text: &str,
    offset: usize,
) -> String {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, str::len) + 1;
    format!("line {line}, column {column}")
}

#[cfg(test)]
pub(crate) mod tests;
