//! A declarative instance type as it is written (TOML or JSON). Decoded
//! tolerantly: a field this application does not know is ignored, so a
//! descriptor written for a newer one still loads when it uses nothing new.
//! What a field holds is checked once, by `validation`.

use std::collections::BTreeMap;

use crate::internal::hanafubuki_foundation::form_schema::SchemaCondition;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A declarative instance type's descriptor.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TypeDocument {
    /// Type name within the contributing extension namespace; the full id is <extension-id>:<name>.
    pub name: String,
    /// `declarative` (the default), or `backend` for a type its extension's
    /// backend plans the install and launch of.
    #[serde(default)]
    pub provider: Option<String>,
    /// Name shown to users, literal/localized or a message key in this extension namespace.
    pub display_name: TextDocument,
    /// The prefix for generated instance names; blank means unspecified.
    #[serde(default)]
    pub default_name: Option<String>,
    #[serde(default)]
    /// Localized description; omission supplies an empty localized text.
    pub description: Option<TextDocument>,
    /// A file of the extension's package: the type's preview image.
    #[serde(default)]
    pub preview: Option<String>,
    /// Declared installation components. core is required; other names must be unique. Names use lowercase ASCII letters, digits and hyphens.
    pub components: Vec<ComponentDocument>,
    #[serde(default)]
    /// Runtime composition for this type. Omission selects kind none.
    pub runtime: RuntimeDocument,
    #[serde(default)]
    /// Environment changes applied to run steps, servers and terminals.
    pub environment: EnvironmentDocument,
    #[serde(default)]
    /// Install form and ordered installation steps; omitted lists are empty.
    pub install: InstallDocument,
    #[serde(default)]
    /// Optional update steps. Absence means no declarative update workflow; backend providers cannot declare this table.
    pub update: Option<UpdateDocument>,
    #[serde(default)]
    /// Server role declarations, including commands, readiness and dependency order.
    pub servers: Vec<ServerDocument>,
    #[serde(default)]
    /// Exposed operations and optional built-in management adoption.
    pub management: ManagementDocument,
    #[serde(default)]
    /// Signatures used to recognize existing installations.
    pub discovery: Vec<DiscoveryDocument>,
    /// An instance only comes from discovery attaching an installation.
    #[serde(default)]
    /// When true, creation occurs only through discovery and at least one discovery signature is required.
    pub attach_only: bool,
}

/// Text in every locale, a message key of the type's namespace
/// (`{ key = "ext.<namespace>.<key>" }`), or text by locale
/// (`{ en = "…", zh = "…" }`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(in crate::internal::hanafubuki_webui::instance_types) enum TextDocument {
    Plain(String),
    Key {
        key: String,
        #[serde(default)]
        params: BTreeMap<String, String>,
    },
    Localized(BTreeMap<String, String>),
}

/// A component: its name, or a table with its name, the label it is shown
/// with, and whether the user may point it at a folder of their own
/// (`external = "allowed"`) holding which program (`executable`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum ComponentDocument {
    Name(String),
    Table {
        name: String,
        #[serde(default)]
        label: Option<TextDocument>,
        #[serde(default)]
        external: Option<String>,
        #[serde(default)]
        executable: Option<ExecutableDocument>,
    },
}

/// An external component's program, relative to its folder, by platform.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct ExecutableDocument {
    #[serde(default)]
    /// Executable path relative to the external component on Unix platforms.
    pub unix: Option<String>,
    #[serde(default)]
    /// Executable path relative to the external component on Windows.
    pub windows: Option<String>,
}

/// What the type's `run` steps, servers and terminal get: `unset` names
/// removed from the inherited environment (a trailing `*` is a prefix),
/// `path` entries (path templates) before `PATH`, in order, and `vars`
/// under the instance's own variables.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct EnvironmentDocument {
    #[serde(default)]
    /// Inherited environment variable names to remove; a trailing * removes a prefix.
    pub unset: Vec<String>,
    #[serde(default)]
    /// Path templates prepended to PATH in declaration order.
    pub path: Vec<String>,
    #[serde(default)]
    /// Environment variable values with declared placeholders.
    pub vars: BTreeMap<String, String>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(super) enum RuntimeDocument {
    #[default]
    /// No managed runtime.
    None,
    /// A managed Python environment with the requested supported version.
    Python { version: String },
    /// A system executable by program name.
    SystemBinary { name: String },
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct InstallDocument {
    #[serde(default)]
    /// Install-form fields; names, defaults, options and conditions are checked by the shared form authority.
    pub parameters: Vec<ParameterDocument>,
    #[serde(default)]
    /// Ordered operations. Templates reference only declared components/parameters and permitted runtime values.
    pub steps: Vec<StepDocument>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
pub(super) struct UpdateDocument {
    #[serde(default)]
    pub steps: Vec<StepDocument>,
}

/// An install parameter: a form-schema field (`docs/rules/instance-types.md`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ParameterDocument {
    pub name: String,
    pub kind: ParameterKindDocument,
    #[serde(default)]
    /// Optional field label; omission uses the parameter name.
    pub label: Option<TextDocument>,
    #[serde(default)]
    /// Optional help text for the install field.
    pub help: Option<TextDocument>,
    #[serde(default)]
    /// Whether the field must be provided; conditional requirements may additionally apply.
    pub required: bool,
    #[serde(default)]
    /// Optional form default, validated against this field kind and options; omission supplies no explicit default.
    pub default: Option<Value>,
    #[serde(default)]
    /// Select/multiselect options; only declared values may be used.
    pub options: Vec<OptionDocument>,
    #[serde(default)]
    /// Optional numeric lower bound.
    pub min: Option<serde_json::Number>,
    #[serde(default)]
    /// Optional numeric upper bound.
    pub max: Option<serde_json::Number>,
    #[serde(default)]
    /// Optional numeric increment.
    pub step: Option<serde_json::Number>,
    /// A path parameter's: `directory` (the default) or `file`.
    #[serde(default)]
    pub path: Option<PathKindDocument>,
    #[serde(default)]
    /// Marks a field as advanced in the install form.
    pub advanced: bool,
    #[serde(default)]
    /// Condition under which this field is additionally required.
    pub required_when: Option<SchemaCondition>,
    #[serde(default)]
    /// Condition under which this field is disabled.
    pub disabled_when: Option<SchemaCondition>,
    #[serde(default)]
    /// Explanation shown while the field is disabled.
    pub disabled_reason: Option<TextDocument>,
    #[serde(default)]
    /// Condition controlling field visibility.
    pub visible_when: Option<SchemaCondition>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ParameterKindDocument {
    Text,
    Number,
    Boolean,
    Select,
    Multiselect,
    Path,
    Secret,
    Slider,
    Textarea,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum PathKindDocument {
    Directory,
    File,
}

/// A select's option: its value, which is also its label, or a value with
/// its own label.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum OptionDocument {
    Value(String),
    Labelled { value: String, label: TextDocument },
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub(super) enum StepDocument {
    /// Download a verified artifact to a file or unpack it into a component path.
    Download {
        /// URL template using declared placeholders.
        url: String,
        /// Expected SHA-256 digest, literal hexadecimal or an install-parameter template.
        sha256: String,
        #[serde(default)]
        /// File target path beginning with a declared component placeholder; exclusive with unpackTo.
        to: Option<String>,
        #[serde(default)]
        /// Archive destination beginning with a declared component placeholder; exclusive with to.
        unpack_to: Option<String>,
        /// With `unpackTo`: the leading path components dropped from every
        /// entry.
        #[serde(default)]
        strip: u32,
        /// With `unpackTo`: the target is replaced whole, atomically.
        #[serde(default)]
        replace: bool,
    },
    /// Unpack an existing archive into a component path.
    Unpack {
        /// Existing archive path template inside a declared component.
        archive: String,
        /// Destination path template inside a declared component.
        into: String,
        #[serde(default)]
        strip: u32,
        /// `into` is replaced whole, atomically, instead of merged into.
        #[serde(default)]
        replace: bool,
    },
    /// Clone a repository into a component path, optionally selecting a branch.
    GitClone {
        /// URL template using declared placeholders.
        url: String,
        /// Destination path template inside a declared component.
        into: String,
        #[serde(default)]
        /// Optional repository branch template; omission uses the repository default.
        branch: Option<String>,
    },
    /// Update the repository at a component path.
    GitPull {
        /// Repository path template inside a declared component.
        dir: String,
    },
    /// Run an argv vector directly, with optional cwd and environment templates.
    Run {
        /// Nonempty argv vector of templates; no shell parsing is performed.
        command: Vec<String>,
        #[serde(default)]
        /// Optional working-directory path template inside a declared component.
        cwd: Option<String>,
        #[serde(default)]
        /// Process environment overrides; keys and values are validated by the shared template checks.
        env: BTreeMap<String, String>,
    },
    /// Prepare the Python environment declared by runtime; requires a Python runtime.
    PythonEnv {},
    /// Write template-expanded text to a component path.
    WriteFile {
        /// Output path template inside a declared component.
        path: String,
        /// Text template written to the output path.
        content: String,
    },
    /// Wait for an HTTP endpoint with a bounded timeout.
    Probe {
        /// URL template using declared placeholders.
        url: String,
        #[serde(default = "default_probe_timeout")]
        /// Probe timeout in seconds; omission uses the reader default.
        timeout_seconds: u64,
    },
}

fn default_probe_timeout() -> u64 {
    60
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ServerDocument {
    /// Unique server role. References in after must name declared roles and form an acyclic start order.
    pub role: String,
    /// A declarative type's; a type its backend provides has none.
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    /// Environment overrides for this server command.
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    /// Port allocation, fixed value or install-parameter reference; omission allocates a port.
    pub port: PortDocument,
    /// Required for every non-control role. The control role uses its own probes and rejects this field.
    #[serde(default)]
    pub readiness: Option<ReadinessDocument>,
    #[serde(default)]
    /// Roles that must start before this role.
    pub after: Vec<String>,
    #[serde(default)]
    /// Restart policy; non-control roles default to never. The control role fixes on-failure and rejects this field.
    pub restart: Option<RestartDocument>,
    #[serde(default)]
    /// Whether this server supplies an embedded WebUI entry.
    pub webview: bool,
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_required() -> bool {
    true
}

/// `{ allocate = true }`, `{ fixed = 8080 }` or `{ param = "port" }`; none
/// of them allocates.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct PortDocument {
    #[serde(default)]
    /// true requests a host-allocated port; false is rejected.
    pub allocate: Option<bool>,
    #[serde(default)]
    /// Fixed port number; mutually exclusive with an allocated or parameter port.
    pub fixed: Option<u16>,
    #[serde(default)]
    /// Name of a declared install parameter supplying the port.
    pub param: Option<String>,
}

/// `{ http = "/health" }`, `{ tcp = true }`, `{ stdout = "<regex>" }` or
/// `{ captured = true }`, each with an optional `timeoutSeconds`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReadinessDocument {
    #[serde(default)]
    /// HTTP readiness path, mutually exclusive with other readiness modes.
    pub http: Option<String>,
    #[serde(default)]
    /// true selects TCP readiness.
    pub tcp: Option<bool>,
    #[serde(default)]
    /// Regular expression used to detect readiness in process output.
    pub stdout: Option<String>,
    #[serde(default)]
    /// true selects a URL captured from process output.
    pub captured: Option<bool>,
    #[serde(default)]
    /// Optional readiness deadline in seconds; omission uses the server-role default.
    pub timeout_seconds: Option<u64>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RestartDocument {
    #[default]
    /// Do not restart this server automatically.
    Never,
    /// Restart when the server fails.
    OnFailure,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct ManagementDocument {
    /// The built-in type whose management its instances take
    /// (`builtin:comfyui`), for a type on Python with the `control` role:
    /// it may then declare `versions`, `extensions`, `models` and
    /// `environment`.
    #[serde(default)]
    pub adopts: Option<String>,
    #[serde(default)]
    /// Management operation names permitted for this type and runtime.
    pub operations: Vec<String>,
}

/// How an existing installation of the type is recognized: paths that must
/// all exist below a directory, the instance root below it, components
/// below the root, and install parameter values.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, Serialize)]
pub(super) struct DiscoveryDocument {
    /// Nonempty paths that must all exist under a candidate installation.
    pub markers: Vec<String>,
    #[serde(default = "default_discovery_root")]
    /// Instance-root path relative to the discovery candidate; omission uses .
    pub root: String,
    #[serde(default)]
    pub components: BTreeMap<String, String>,
    #[serde(default)]
    /// Install parameter values recorded for a discovered installation.
    pub values: serde_json::Map<String, Value>,
    /// The components an attached installation records as external.
    #[serde(default)]
    pub external: Vec<String>,
}

fn default_discovery_root() -> String {
    ".".to_string()
}

/// The JSON Schema of a descriptor as this application reads it: what it
/// requires, and every field it knows (another is ignored). Its steps are
/// also what a backend's install plan answers.
#[cfg(feature = "schema")]
pub fn descriptor_schema() -> schemars::Schema {
    let mut schema = schemars::schema_for!(TypeDocument);
    crate::internal::hanafubuki_foundation::config_documentation::annotate(
        &mut schema,
        super::documentation::RULES,
    );
    schema.ensure_object()["properties"]["provider"]["x-hanafubuki-effective-default"] = serde_json::json!({"description":"Omission selects the declarative provider", "value":super::documentation::DEFAULT_PROVIDER});
    schema.ensure_object()["properties"]["defaultName"]["x-hanafubuki-effective-default"] = serde_json::json!({"description":"Leading/trailing whitespace is trimmed; an empty result is unspecified. The instance naming authority supplies its fallback."});
    schema.ensure_object()["$defs"]["ReadinessDocument"]["properties"]["timeoutSeconds"]["x-hanafubuki-effective-default"] = serde_json::json!({"description":"Readiness timeout when omitted, in seconds", "value":super::documentation::READINESS_TIMEOUT_SECONDS});
    schema.ensure_object()["$defs"]["ServerDocument"]["properties"]["port"]["x-hanafubuki-effective-default"] =
        serde_json::json!({"description":"Omission allocates a port", "value":{"allocate":true}});
    schema
}
