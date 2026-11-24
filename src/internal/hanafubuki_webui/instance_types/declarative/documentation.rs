use crate::internal::hanafubuki_foundation::config_documentation::{ConfigRule, RuleStage};

pub(super) const BACKEND_STEPS: ConfigRule = ConfigRule {
    id: "instance_types-declarative-backend-steps",
    fields: &["install.steps"],
    condition: "provider is backend",
    requirement: "For provider backend, omit install steps and the update table; plans come from the backend.",
    message: "a type its backend provides gets its steps from its backend",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};

pub(super) const ATTACH_DISCOVERY: ConfigRule = ConfigRule {
    id: "instance_types-declarative-attach-discovery",
    fields: &["attachOnly"],
    condition: "attachOnly is true",
    requirement: "Supply at least one discovery signature when attachOnly is true.",
    message: "a type attached only by discovery needs a `[[discovery]]` signature",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};

pub(super) const DIGEST: ConfigRule = ConfigRule {
    id: "instance_types-declarative-digest",
    fields: &["install.steps[].sha256"],
    condition: "download digest contains no parameter placeholder",
    requirement: "Provide 64 hexadecimal SHA-256 digits, or a declared parameter template.",
    message: "a SHA-256 is 64 hexadecimal digits",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/steps.rs",
};

pub(super) const STRIP: ConfigRule = ConfigRule {
    id: "instance_types-declarative-strip",
    fields: &["install.steps[].strip"],
    condition: "download uses to with strip greater than zero",
    requirement: "Use unpackTo when strip is greater than zero.",
    message: "`strip` drops path components of an archive's entries; it needs `unpackTo`",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/steps.rs",
};

pub(super) const REPLACE: ConfigRule = ConfigRule {
    id: "instance_types-declarative-replace",
    fields: &["install.steps[].replace"],
    condition: "download uses to with replace true",
    requirement: "Use unpackTo when replace is true.",
    message: "`replace` replaces the folder an archive unpacks into; it needs `unpackTo`",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/steps.rs",
};

pub(super) const DOWNLOAD_TARGET: ConfigRule = ConfigRule {
    id: "instance_types-declarative-download-target",
    fields: &["install.steps"],
    condition: "download step",
    requirement: "Choose exactly one of to and unpackTo for each download.",
    message: "a download names exactly one of `to` and `unpackTo`",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/steps.rs",
};

#[cfg(feature = "schema")]
pub(super) const RULES: &[ConfigRule] = &[
    READINESS_REQUIRED,
    READINESS_MODE,
    ADOPTS_RUNTIME,
    ADOPTS_CORE,
    UPDATE_PROVIDER,
    CONTROL_RUNTIME,
    CORE,
    EXTERNAL_PROGRAM,
    EXTERNAL_PYTHON,
    MARKERS,
    PYTHON_ENV,
    CONTROL_CONFIG,
    SERVER_COMMAND,
    BACKEND_COMMAND,
    PORT,
    UPDATE_CHECK,
    PORT_SCOPE,
    BACKEND_STEPS,
    ATTACH_DISCOVERY,
    DIGEST,
    STRIP,
    REPLACE,
    DOWNLOAD_TARGET,
];

pub(super) const READINESS_TIMEOUT_SECONDS: u64 = 120;

pub(super) const CORE: ConfigRule = ConfigRule {
    id: "type-core",
    fields: &["components"],
    condition: "All instance types",
    requirement: "Declare the core component.",
    message: "a type declares the `core` component",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/components.rs",
};

pub(super) const EXTERNAL_PROGRAM: ConfigRule = ConfigRule {
    id: "type-external-program",
    fields: &["components"],
    condition: "An executable is declared",
    requirement: "Only an external = allowed component can declare an executable.",
    message: "a program is named only for a component with `external = \"allowed\"`",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/components.rs",
};

pub(super) const EXTERNAL_PYTHON: ConfigRule = ConfigRule {
    id: "type-external-python",
    fields: &["components"],
    condition: "An external component is declared",
    requirement: "The managed python component cannot be external.",
    message: "the `python` component is the managed environment and cannot be external",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/components.rs",
};

pub(super) const MARKERS: ConfigRule = ConfigRule {
    id: "type-markers",
    fields: &["discovery[].markers"],
    condition: "A discovery signature is declared",
    requirement: "Provide at least one discovery marker.",
    message: "a signature names at least one marker",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/discovery.rs",
};

pub(super) const PYTHON_ENV: ConfigRule = ConfigRule {
    id: "type-python-env",
    fields: &["install.steps"],
    condition: "kind is python-env",
    requirement: "Declare a Python runtime before using python-env.",
    message: "`python-env` prepares a Python runtime, which the type does not declare",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/steps.rs",
};

pub(super) const CONTROL_CONFIG: ConfigRule = ConfigRule {
    id: "type-control-config",
    fields: &["servers"],
    condition: "role is control",
    requirement: "For role control, configure only after and required in addition to role.",
    message: "the `control` role is the system's management API: it takes only `after` and `required`",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};

pub(super) const SERVER_COMMAND: ConfigRule = ConfigRule {
    id: "type-server-command",
    fields: &["servers[].command"],
    condition: "Non-control server with declarative provider",
    requirement: "Supply command for each non-control declarative server.",
    message: "a server declares its command",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};

pub(super) const BACKEND_COMMAND: ConfigRule = ConfigRule {
    id: "type-backend-command",
    fields: &["servers[].command"],
    condition: "provider is backend",
    requirement: "Omit server command for provider backend; commands come from backend plans.",
    message: "a type its backend provides gets its commands from its backend",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};

pub(super) const PORT: ConfigRule = ConfigRule {
    id: "type-port",
    fields: &["servers[].port"],
    condition: "A server port is configured",
    requirement: "Select exactly one of allocate = true, a positive fixed port, or a declared numeric parameter; omission allocates a port.",
    message: "a port is one of `allocate = true`, `fixed = <port>` and `param = \"<name>\"`",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};

pub(super) const UPDATE_CHECK: ConfigRule = ConfigRule {
    id: "type-update-check",
    fields: &["management.operations"],
    condition: "update-check is offered",
    requirement: "Declare update alongside update-check.",
    message: "`update-check` needs `update`: a type announces only an update it can apply",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/management.rs",
};

pub(super) const PORT_SCOPE: ConfigRule = ConfigRule {
    id: "type-port-scope",
    fields: &["install.steps"],
    condition: "Templates outside a server command",
    requirement: "Use {port} only in server commands.",
    message: "`{port}` is known only to a server's command",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/placeholders.rs",
};

pub(super) const ADOPTS_RUNTIME: ConfigRule = ConfigRule {
    id: "type-adopts-runtime",
    fields: &["management.adopts"],
    condition: "Built-in management is adopted",
    requirement: "Declare a Python runtime and the control server role when adopting built-in management.",
    message: "a built-in type's management is served by the management API: adopting it needs \
             `runtime = { kind = \"python\" }` and the `control` role",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/management.rs",
};

pub(super) const ADOPTS_CORE: ConfigRule = ConfigRule {
    id: "type-adopts-core",
    fields: &["management.adopts"],
    condition: "Built-in management is adopted",
    requirement: "The core component must remain managed rather than allowing an external directory.",
    message: "a built-in type's management changes what `core` holds: a type whose `core` may be \
             an external folder cannot adopt it",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/management.rs",
};

pub(super) const UPDATE_PROVIDER: ConfigRule = ConfigRule {
    id: "type-update-provider",
    fields: &["management.operations"],
    condition: "update-check is offered",
    requirement: "Only backend-provided types may offer update-check.",
    message: "`update-check` is answered by the type's backend: only a type its backend \
                     provides (`provider = \"backend\"`) can offer it",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/management.rs",
};

pub(super) const CONTROL_RUNTIME: ConfigRule = ConfigRule {
    id: "type-control-runtime",
    fields: &["servers"],
    condition: "role is control",
    requirement: "Declare a Python runtime for the control role.",
    message: "the `control` role is the management API, which runs on the instance's Python: \
             it needs `runtime = { kind = \"python\" }`",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};

pub(super) const DEFAULT_PROVIDER: &str = "declarative";

pub(super) const READINESS_REQUIRED: ConfigRule = ConfigRule {
    id: "type-readiness-required",
    fields: &["servers[].readiness"],
    condition: "Server role is not control",
    requirement: "Declare readiness for every non-control server.",
    message: "a server declares its readiness",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};
pub(super) const READINESS_MODE: ConfigRule = ConfigRule {
    id: "type-readiness-mode",
    fields: &["servers[].readiness"],
    condition: "A readiness table is supplied",
    requirement: "Select exactly one HTTP path, true TCP flag, stdout regular expression or true captured flag. HTTP paths begin with / and contain no whitespace; stdout patterns must compile.",
    message: "readiness is one of `http = \"/path\"`, `tcp = true`, `stdout = \"<regex>\"` and `captured = true`",
    stage: RuleStage::Validation,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/declarative/validation/mod.rs",
};
