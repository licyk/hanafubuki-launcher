use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_foundation::form_schema::{SchemaGroup, SchemaText};
use crate::internal::hanafubuki_instance_contracts::{
    InstanceRuntime, InstanceTypeDescriptor, InstanceTypeId, InstanceTypeKind, ServerPort,
    ServerReadiness, ServerRestart, ServerRole, ServerRoleDescriptor, ServerStart, WebUiFamily,
};
use serde_json::{Map, Value};

use crate::internal::hanafubuki_webui::install::{WebUiInstallSpec, builtin_install_form};
use crate::internal::hanafubuki_webui::instances::InstanceSpec;

use super::InstanceProvider;

/// One built-in WebUI family as an instance type. Its spec, install,
/// launch, management and discovery are that family's code, reached by the
/// family this provider maps the type id to.
pub struct BuiltinProvider {
    family: WebUiFamily,
    descriptor: InstanceTypeDescriptor,
}

const NAMESPACE: &str = "builtin";

/// What every built-in family's management offers, over its control
/// server; whether one instance supports each is its capabilities' answer.
/// Its diagnostics are none of them: every type's instances have those.
pub const BUILTIN_OPERATIONS: [&str; 9] = [
    "update",
    "terminal",
    "versions",
    "extensions",
    "launch-arguments",
    "snapshots",
    "environment",
    "models",
    "hotpatch",
];

impl BuiltinProvider {
    pub fn new(family: WebUiFamily) -> Self {
        Self {
            family,
            descriptor: InstanceTypeDescriptor {
                id: Self::type_id(family),
                kind: InstanceTypeKind::Builtin,
                label: SchemaText::literal(label(family)),
                default_name: None,
                description: SchemaText::key(format!(
                    "instances.create.family.descriptions.{}",
                    family_key(family)
                )),
                preview: Some(family.cli_command().to_string()),
                preview_url: None,
                components: vec![
                    InstanceComponentPaths::CORE.to_string(),
                    InstanceComponentPaths::PYTHON.to_string(),
                ],
                component_labels: Default::default(),
                external_components: Default::default(),
                runtime: InstanceRuntime::Python { version: None },
                servers: builtin_servers(),
                operations: BUILTIN_OPERATIONS.map(str::to_string).to_vec(),
                own_operations: Vec::new(),
                checks_updates: true,
                adopts: None,
                attach_only: false,
            },
        }
    }

    pub fn all() -> impl Iterator<Item = Self> {
        WebUiFamily::ALL.into_iter().map(Self::new)
    }

    /// `builtin:<the family's CLI command>`. Persisted in every record, so a
    /// family keeps its id for good.
    pub fn type_id(family: WebUiFamily) -> InstanceTypeId {
        match InstanceTypeId::parse(&format!("{NAMESPACE}:{}", family.cli_command())) {
            Ok(id) => id,
            Err(error) => unreachable!("a built-in type id is valid: {error}"),
        }
    }

    /// The built-in family a type id names, if it names one.
    pub fn family_of(id: &InstanceTypeId) -> Option<WebUiFamily> {
        WebUiFamily::ALL
            .into_iter()
            .find(|family| Self::type_id(*family) == *id)
    }
}

impl InstanceProvider for BuiltinProvider {
    fn descriptor(&self) -> &InstanceTypeDescriptor {
        &self.descriptor
    }

    fn install_form(&self) -> Vec<SchemaGroup> {
        builtin_install_form(self.family)
    }

    fn validate_spec(&self, spec: Map<String, Value>) -> Result<InstanceSpec, String> {
        WebUiInstallSpec::from_object(self.family, spec)
            .map(InstanceSpec::Builtin)
            .map_err(|error| format!("{} install spec: {error}", self.family.product_name()))
    }

    fn read_spec(&self, spec: Map<String, Value>) -> Result<InstanceSpec, String> {
        WebUiInstallSpec::from_persisted_object(self.family, spec)
            .map(InstanceSpec::Builtin)
            .map_err(|error| format!("{} install spec: {error}", self.family.product_name()))
    }
}

/// What every built-in type runs: the control server on the instance's
/// Python, which the launch workflow starts before the WebUI and manages it
/// through, and the WebUI, ready when the hotpatcher captures the address it
/// opens and never restarted on its own.
fn builtin_servers() -> Vec<ServerRoleDescriptor> {
    vec![
        ServerRoleDescriptor {
            role: ServerRole::control(),
            start: ServerStart::ControlServer,
            port: ServerPort::Allocate,
            readiness: ServerReadiness::Control,
            after: Vec::new(),
            restart: ServerRestart::OnFailure,
            webview: false,
            required: true,
        },
        ServerRoleDescriptor {
            role: ServerRole::ui(),
            start: ServerStart::LaunchWorkflow,
            port: ServerPort::Config {
                key: "launchArguments".to_string(),
            },
            // The launch workflow keeps its own deadline.
            readiness: ServerReadiness::Captured {
                timeout_seconds: 120,
            },
            after: vec![ServerRole::control()],
            restart: ServerRestart::Never,
            webview: true,
            required: true,
        },
    ]
}

/// The name the creation wizard shows. Not `product_name`, which names new
/// instances and is persisted with them.
fn label(family: WebUiFamily) -> &'static str {
    match family {
        WebUiFamily::StableDiffusionWebui => "Stable Diffusion WebUI",
        WebUiFamily::Comfyui => "ComfyUI",
        WebUiFamily::Invokeai => "InvokeAI",
        WebUiFamily::Fooocus => "Fooocus",
        WebUiFamily::SdTrainer => "SD Trainer",
        WebUiFamily::QwenTts => "Qwen TTS WebUI",
        WebUiFamily::RvcNext => "RVC Next WebUI",
        WebUiFamily::MizuotoTts => "Mizuoto TTS WebUI",
    }
}

/// The family's serde name, which keys its messages in the frontend.
fn family_key(family: WebUiFamily) -> String {
    match serde_json::to_value(family) {
        Ok(Value::String(key)) => key,
        _ => unreachable!("a WebUI family serializes as a string"),
    }
}
