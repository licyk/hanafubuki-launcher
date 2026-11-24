use serde_json::{Map, Value};

use crate::internal::hanafubuki_foundation::json_normalization::{
    JsonField, JsonNormalizationEventKind, JsonShape, OPAQUE_JSON_SHAPE, normalize_current_json,
};

use super::{AppPaths, AppSettings, validation::validate_settings};
use crate::internal::hanafubuki_config::model::settings_domain::SettingsDomain;

mod appearance;
mod automation;
mod diagnostics;
mod environment_tools;
mod field;
mod language;
mod logging;
mod mirrors;
mod modules;
mod names;
mod network;
mod onboarding;
mod paths;
mod runtime;
mod webview;

macro_rules! opaque_settings_shape {
    ($fields:ident, $shape:ident, [$($name:literal),+ $(,)?]) => {
        const $fields: &[JsonField] = &[$(JsonField::new($name, &OPAQUE_JSON_SHAPE)),+];
        const $shape: JsonShape = JsonShape::Object($fields);
    };
}

opaque_settings_shape!(LANGUAGE_JSON_FIELDS, LANGUAGE_JSON_SHAPE, ["language"]);
opaque_settings_shape!(
    APPEARANCE_JSON_FIELDS,
    APPEARANCE_JSON_SHAPE,
    [
        "theme",
        "themeColor",
        "backgroundImageEnabled",
        "backgroundRandomEnabled",
        "backgroundSurfaceOpacity",
        "backgroundBlurRadius",
        "backgroundImageFit",
        "backgroundImagePosition",
    ]
);
opaque_settings_shape!(
    PATHS_JSON_FIELDS,
    PATHS_JSON_SHAPE,
    [
        "artifactCacheDir",
        "defaultWebuiInstallDir",
        "webviewDownloadDir"
    ]
);
opaque_settings_shape!(
    NETWORK_JSON_FIELDS,
    NETWORK_JSON_SHAPE,
    ["proxyMode", "proxyAddress"]
);
opaque_settings_shape!(
    MIRRORS_JSON_FIELDS,
    MIRRORS_JSON_SHAPE,
    [
        "autoMirror",
        "useMicromambaMirror",
        "usePypiMirror",
        "useGithubMirror",
        "customGithubMirror",
        "useHuggingfaceMirror",
        "customHuggingfaceMirror",
        "modelDownloadSource",
    ]
);
opaque_settings_shape!(
    RUNTIME_JSON_FIELDS,
    RUNTIME_JSON_SHAPE,
    ["appPythonVersion", "gitRuntimeSource"]
);
opaque_settings_shape!(
    ENVIRONMENT_TOOLS_JSON_FIELDS,
    ENVIRONMENT_TOOLS_JSON_SHAPE,
    ["enableUv", "enableEnvironmentCheck", "useCudaMalloc"]
);
opaque_settings_shape!(
    LOGGING_JSON_FIELDS,
    LOGGING_JSON_SHAPE,
    [
        "logRetentionDays",
        "logLevel",
        "logFilePersistenceEnabled",
        "logFileMaxSizeMb",
    ]
);
opaque_settings_shape!(
    DIAGNOSTICS_JSON_FIELDS,
    DIAGNOSTICS_JSON_SHAPE,
    ["appDiagnosticsIncludeAllInstances"]
);
opaque_settings_shape!(
    AUTOMATION_JSON_FIELDS,
    AUTOMATION_JSON_SHAPE,
    [
        "autoCheckAppUpdates",
        "autoCheckInstanceUpdates",
        "autoCreateInstanceSnapshots",
        "autoCheckExtensionUpdates"
    ]
);
opaque_settings_shape!(
    WEBVIEW_JSON_FIELDS,
    WEBVIEW_JSON_SHAPE,
    [
        "webviewSharedCache",
        "webviewPreservePageState",
        "webviewNewWindow",
        "webviewGpuAcceleration"
    ]
);
opaque_settings_shape!(
    MODULES_JSON_FIELDS,
    MODULES_JSON_SHAPE,
    ["disabledModules", "modules"]
);
opaque_settings_shape!(
    ONBOARDING_JSON_FIELDS,
    ONBOARDING_JSON_SHAPE,
    ["systemWebuiScanPrompted"]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistedSettingsIssueKind {
    MissingField,
    UnsupportedField,
    /// One field (or one entry of a list) fell back to its default.
    InvalidField,
    /// As `InvalidField`, but the fallback changes where traffic goes or what
    /// runs, so the user must see it.
    InvalidPolicyField,
    /// The document as a whole could not be used and every field took its
    /// default.
    InvalidSection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PersistedSettingsIssue {
    pub kind: PersistedSettingsIssueKind,
    pub json_pointer: String,
    pub message: String,
}

pub(crate) struct DecodedPersistedSettings {
    /// Exactly the document this domain persists, repaired and canonical. This
    /// is what a caller merges and what it writes back.
    pub document: Map<String, Value>,
    pub changed: bool,
    pub issues: Vec<PersistedSettingsIssue>,
}

pub(crate) struct PersistedSettingsDecoder<'a> {
    paths: &'a AppPaths,
}

impl<'a> PersistedSettingsDecoder<'a> {
    pub fn new(paths: &'a AppPaths) -> Self {
        Self { paths }
    }

    /// Decode one persisted settings document.
    ///
    /// Only `domain`'s fields are read, each on its own: a missing or invalid
    /// field takes its default and is reported, and the rest are kept. A field
    /// that does not belong to this domain is reported as unsupported and
    /// dropped, which is what keeps a document from silently owning a field
    /// another document is the authority for. Any JSON value decodes; the
    /// error is reserved for failing to project the result.
    pub fn decode(
        &self,
        domain: SettingsDomain,
        value: Value,
    ) -> Result<DecodedPersistedSettings, String> {
        let original = value;
        let mut normalized = original.clone();
        let mut issues = Vec::new();
        normalize_current_json(&mut normalized, domain_json_shape(domain), |event| {
            if event.kind == JsonNormalizationEventKind::FieldRemoved {
                issues.push(PersistedSettingsIssue {
                    kind: PersistedSettingsIssueKind::UnsupportedField,
                    json_pointer: event.json_pointer,
                    message: "unsupported settings field was removed".to_string(),
                });
            }
        });
        let defaults = AppSettings::defaults(self.paths);
        let mut settings = defaults.clone();
        match normalized.as_object() {
            Some(object) => decode_domain(domain, object, &defaults, &mut settings, &mut issues),
            None => issues.push(invalid_section(format!(
                "{} must contain an object; backend defaults were used",
                domain.file_name()
            ))),
        }

        // Each domain decoder ends self-consistent, so this only guards
        // against a decoder bug: the domain then falls back as a whole rather
        // than handing back settings the contract rejects.
        if validate_settings(&settings).is_err() {
            settings = defaults;
            issues.push(invalid_section(format!(
                "decoded {} did not satisfy the settings contract; backend defaults were used",
                domain.file_name()
            )));
        }
        let document = document_for(domain, &settings)?;
        Ok(DecodedPersistedSettings {
            changed: Value::Object(document.clone()) != original,
            document,
            issues,
        })
    }
}

fn invalid_section(message: String) -> PersistedSettingsIssue {
    PersistedSettingsIssue {
        kind: PersistedSettingsIssueKind::InvalidSection,
        json_pointer: String::new(),
        message,
    }
}

fn decode_domain(
    domain: SettingsDomain,
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    match domain {
        SettingsDomain::Language => language::decode(object, defaults, settings, issues),
        SettingsDomain::Appearance => appearance::decode(object, defaults, settings, issues),
        SettingsDomain::Paths => paths::decode(object, defaults, settings, issues),
        SettingsDomain::Network => network::decode(object, defaults, settings, issues),
        SettingsDomain::Mirrors => mirrors::decode(object, defaults, settings, issues),
        SettingsDomain::Runtime => runtime::decode(object, defaults, settings, issues),
        SettingsDomain::EnvironmentTools => {
            environment_tools::decode(object, defaults, settings, issues)
        }
        SettingsDomain::Logging => logging::decode(object, defaults, settings, issues),
        SettingsDomain::Diagnostics => diagnostics::decode(object, defaults, settings, issues),
        SettingsDomain::Automation => automation::decode(object, defaults, settings, issues),
        SettingsDomain::Webview => webview::decode(object, defaults, settings, issues),
        SettingsDomain::Modules => modules::decode(object, defaults, settings, issues),
        SettingsDomain::Onboarding => onboarding::decode(object, defaults, settings, issues),
    }
}

/// The settings fields one domain persists, in document order.
pub fn domain_fields(domain: SettingsDomain) -> &'static [&'static str] {
    match domain {
        SettingsDomain::Language => language::FIELDS,
        SettingsDomain::Appearance => appearance::FIELDS,
        SettingsDomain::Paths => paths::FIELDS,
        SettingsDomain::Network => network::FIELDS,
        SettingsDomain::Mirrors => mirrors::FIELDS,
        SettingsDomain::Runtime => runtime::FIELDS,
        SettingsDomain::EnvironmentTools => environment_tools::FIELDS,
        SettingsDomain::Logging => logging::FIELDS,
        SettingsDomain::Diagnostics => diagnostics::FIELDS,
        SettingsDomain::Automation => automation::FIELDS,
        SettingsDomain::Webview => webview::FIELDS,
        SettingsDomain::Modules => modules::FIELDS,
        SettingsDomain::Onboarding => onboarding::FIELDS,
    }
}

fn domain_json_shape(domain: SettingsDomain) -> &'static JsonShape {
    match domain {
        SettingsDomain::Language => &LANGUAGE_JSON_SHAPE,
        SettingsDomain::Appearance => &APPEARANCE_JSON_SHAPE,
        SettingsDomain::Paths => &PATHS_JSON_SHAPE,
        SettingsDomain::Network => &NETWORK_JSON_SHAPE,
        SettingsDomain::Mirrors => &MIRRORS_JSON_SHAPE,
        SettingsDomain::Runtime => &RUNTIME_JSON_SHAPE,
        SettingsDomain::EnvironmentTools => &ENVIRONMENT_TOOLS_JSON_SHAPE,
        SettingsDomain::Logging => &LOGGING_JSON_SHAPE,
        SettingsDomain::Diagnostics => &DIAGNOSTICS_JSON_SHAPE,
        SettingsDomain::Automation => &AUTOMATION_JSON_SHAPE,
        SettingsDomain::Webview => &WEBVIEW_JSON_SHAPE,
        SettingsDomain::Modules => &MODULES_JSON_SHAPE,
        SettingsDomain::Onboarding => &ONBOARDING_JSON_SHAPE,
    }
}

/// Project one domain's fields out of complete settings, producing exactly the
/// document that domain persists.
pub fn document_for(
    domain: SettingsDomain,
    settings: &AppSettings,
) -> Result<Map<String, Value>, String> {
    project(domain, &settings_object(settings)?)
}

/// One persisted settings document, paired with the domain that owns it.
pub(crate) type DomainDocument = (SettingsDomain, Map<String, Value>);

/// Project every domain's document out of complete settings in one pass.
///
/// Callers that need more than one document use this rather than
/// [`document_for`] per domain: settings are a large struct, and serializing
/// them once instead of ten times is the difference between a load or a write
/// costing one round trip and costing a dozen.
pub(crate) fn documents_for(settings: &AppSettings) -> Result<Vec<DomainDocument>, String> {
    let object = settings_object(settings)?;
    SettingsDomain::ALL
        .iter()
        .map(|&domain| Ok((domain, project(domain, &object)?)))
        .collect()
}

/// Assemble complete settings from per-domain documents laid over the defaults.
///
/// Fields are collected into one object and deserialized once at the end, so a
/// ten-document load costs a single round trip instead of one per document.
pub(crate) fn settings_from_documents(
    defaults: &AppSettings,
    documents: impl IntoIterator<Item = Map<String, Value>>,
) -> Result<AppSettings, String> {
    let mut object = settings_object(defaults)?;
    for document in documents {
        for (field, value) in document {
            object.insert(field, value);
        }
    }
    serde_json::from_value(Value::Object(object))
        .map_err(|_| "merged settings are invalid".to_string())
}

fn settings_object(settings: &AppSettings) -> Result<Map<String, Value>, String> {
    match serde_json::to_value(settings).map_err(|_| "serialize settings".to_string())? {
        Value::Object(object) => Ok(object),
        _ => Err("settings did not serialize as an object".to_string()),
    }
}

fn project(
    domain: SettingsDomain,
    object: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let mut document = Map::new();
    for field in domain_fields(domain) {
        let value = object
            .get(*field)
            .cloned()
            .ok_or_else(|| format!("settings do not expose {field}"))?;
        document.insert((*field).to_string(), value);
    }
    Ok(document)
}

#[cfg(test)]
mod tests;
