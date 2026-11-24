use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::internal::hanafubuki_config::repository::instances::{
    InstanceComponentPaths, InstanceConfig, InstanceQuickPath,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;

use crate::internal::hanafubuki_webui::install::{
    InstanceInstallCleanupStatus, InstanceInstallPhase, WebUiInstallSpec,
};
use crate::internal::hanafubuki_webui::instance_types::presets::{
    LaunchArgument, PresetId, merge_launch_arguments,
};
use crate::internal::hanafubuki_webui::instance_types::{DeclarativeSpec, unavailable_reason};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateWebUiInstanceRequest {
    /// Optional: the backend generates a name when this is absent or blank.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub install_dir: Option<PathBuf>,
    /// Where a declared component goes instead of its default below the
    /// install directory; a component the type does not declare is refused.
    #[serde(default)]
    pub component_paths: BTreeMap<String, PathBuf>,
    #[serde(rename = "type")]
    pub type_id: InstanceTypeId,
    /// Validated by the type's provider; final as it is, even when it was
    /// filled from a preset.
    pub spec: Map<String, Value>,
    /// The preset the spec was filled from, a usable one over the type: the
    /// record names it for display, and its launch arguments become the
    /// instance's.
    #[serde(default)]
    pub preset: Option<PresetId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ImportWebUiInstanceRequest {
    /// Optional: the backend generates a name when this is absent or blank.
    #[serde(default)]
    pub name: Option<String>,
    pub install_dir: PathBuf,
    /// Where a declared component is instead of its default below the
    /// install directory; a component the type does not declare is refused.
    #[serde(default)]
    pub component_paths: BTreeMap<String, PathBuf>,
    #[serde(rename = "type")]
    pub type_id: InstanceTypeId,
    /// Validated by the type's provider.
    pub spec: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateWebUiInstanceFromSnapshotRequest {
    /// Optional: the backend generates a name when this is absent or blank.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub install_dir: Option<PathBuf>,
    /// Where a declared component goes instead of its default below the
    /// install directory; a component the type does not declare is refused.
    #[serde(default)]
    pub component_paths: BTreeMap<String, PathBuf>,
    pub snapshot_path: PathBuf,
    pub expected_type: InstanceTypeId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WebUiInstanceInstallSource {
    Standard,
    DiscoveredCore,
    Snapshot { snapshot_path: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WebUiInstanceLifecycle {
    Created,
    Installing {
        operation_id: String,
    },
    Ready {
        operation_id: String,
    },
    InstallFailed {
        operation_id: String,
        phase: InstanceInstallPhase,
        exit_code: Option<i32>,
        error: Option<String>,
        cleanup_status: InstanceInstallCleanupStatus,
        cleanup_error: Option<String>,
    },
    Deleting,
}

/// An instance's spec, as its type's provider read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstanceSpec {
    Builtin(WebUiInstallSpec),
    /// The values of a declarative type's install parameters.
    Declarative(DeclarativeSpec),
    /// No provider is registered for the record's type: the spec is kept as
    /// it was read, and the instance refuses every operation but deleting
    /// its record.
    Unavailable(Map<String, Value>),
}

/// `lifecycle.json`. Written as `type` (the type id) beside a `spec` object
/// only the type's provider reads; the repository decodes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiInstanceRecord {
    pub instance_id: String,
    pub instance_identity: String,
    #[serde(rename = "type")]
    pub type_id: InstanceTypeId,
    #[serde(serialize_with = "serialize_spec")]
    pub spec: InstanceSpec,
    pub install_source: WebUiInstanceInstallSource,
    pub lifecycle: WebUiInstanceLifecycle,
    pub created_at: u128,
    pub updated_at: u128,
    /// The preset it was made from (`<owner>:<id>`), for display only:
    /// nothing reads it to operate the instance, so a preset that changes
    /// or goes away changes nothing about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    /// The launch arguments of that preset, as they were when the instance
    /// was made: kept, not read from the preset again, so they stay over
    /// the built-in defaults each time those are set
    /// ([`Self::default_launch_arguments`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preset_launch_arguments: Vec<LaunchArgument>,
}

impl WebUiInstanceRecord {
    /// The built-in WebUI install spec, or why the instance has none: its
    /// type is declarative (it has no family and no Python workflow), or no
    /// provider is registered for it.
    pub fn install_spec(&self) -> Result<&WebUiInstallSpec, String> {
        match &self.spec {
            InstanceSpec::Builtin(spec) => Ok(spec),
            InstanceSpec::Declarative(_) => Err(not_builtin(&self.type_id)),
            InstanceSpec::Unavailable(_) => Err(unavailable_reason(&self.type_id)),
        }
    }

    pub fn install_spec_mut(&mut self) -> Result<&mut WebUiInstallSpec, String> {
        match &mut self.spec {
            InstanceSpec::Builtin(spec) => Ok(spec),
            InstanceSpec::Declarative(_) => Err(not_builtin(&self.type_id)),
            InstanceSpec::Unavailable(_) => Err(unavailable_reason(&self.type_id)),
        }
    }

    /// A built-in type's default launch arguments for the spec, with the
    /// preset's it was made from over them: what the instance starts with,
    /// and what the install's completion and a branch switch set again
    /// unless the user edited them.
    pub(crate) fn default_launch_arguments(&self) -> Result<Vec<String>, String> {
        Ok(merge_launch_arguments(
            &crate::internal::hanafubuki_webui::install::default_launch_arguments(
                self.install_spec()?,
            ),
            &self.preset_launch_arguments,
        ))
    }

    /// Refuses an instance whose type no provider is registered for.
    pub fn require_available(&self) -> Result<(), String> {
        match &self.spec {
            InstanceSpec::Unavailable(_) => Err(unavailable_reason(&self.type_id)),
            _ => Ok(()),
        }
    }

    /// Whether the instance's type is declarative: installed by its steps and
    /// started as its declared servers, with no family workflow.
    pub fn is_declarative(&self) -> bool {
        matches!(self.spec, InstanceSpec::Declarative(_))
    }

    pub fn availability(&self) -> InstanceTypeAvailability {
        match &self.spec {
            InstanceSpec::Builtin(_) | InstanceSpec::Declarative(_) => {
                InstanceTypeAvailability::Available
            }
            InstanceSpec::Unavailable(_) => InstanceTypeAvailability::Unavailable {
                reason: unavailable_reason(&self.type_id),
            },
        }
    }
}

/// Why an operation of the built-in WebUI workflow refuses an instance of a
/// declarative type.
pub(crate) fn not_builtin(type_id: &InstanceTypeId) -> String {
    format!("type `{type_id}` is not a built-in WebUI type")
}

fn serialize_spec<S: serde::Serializer>(
    spec: &InstanceSpec,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match spec {
        InstanceSpec::Builtin(spec) => spec.to_object().serialize(serializer),
        InstanceSpec::Declarative(spec) => spec.to_object().serialize(serializer),
        InstanceSpec::Unavailable(spec) => spec.serialize(serializer),
    }
}

/// Whether the application has a provider for an instance's type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InstanceTypeAvailability {
    Available,
    Unavailable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WebUiInstanceInstallAction {
    Available,
    Unavailable { reason: String },
}

impl Default for WebUiInstanceInstallAction {
    fn default() -> Self {
        Self::Unavailable {
            reason: "installation is unavailable".to_string(),
        }
    }
}

/// Every installer entry point a snapshot reports on.
///
/// They are resolved together because they share one layout check, and they are
/// carried together so a caller cannot build a snapshot that answers for the
/// reinstall button while leaving its destructive switches unanswered.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WebUiInstanceInstallActions {
    pub install: WebUiInstanceInstallAction,
    pub reinstall: WebUiInstanceInstallAction,
    pub force_recreate_python: WebUiInstanceInstallAction,
    pub force_recreate_core: WebUiInstanceInstallAction,
    pub reinstall_spec: WebUiInstanceInstallAction,
}

impl WebUiInstanceInstallActions {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let action = WebUiInstanceInstallAction::Unavailable {
            reason: reason.into(),
        };
        Self {
            install: action.clone(),
            reinstall: action.clone(),
            force_recreate_python: action.clone(),
            force_recreate_core: action.clone(),
            reinstall_spec: action,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiInstanceSnapshot {
    pub instance: InstanceConfig,
    pub install: WebUiInstanceRecord,
    /// An instance whose type is unavailable is listed with the reason, and
    /// shows its paths, but refuses every operation except deleting its
    /// record.
    pub availability: InstanceTypeAvailability,
    pub install_dir: PathBuf,
    pub install_action: WebUiInstanceInstallAction,
    pub reinstall_action: WebUiInstanceInstallAction,
    /// Whether a reinstall may delete each component first. A component that
    /// cannot be deleted on its own, such as a core equal to the instance root,
    /// is reported here instead of failing after the user confirms.
    pub force_recreate_python_action: WebUiInstanceInstallAction,
    pub force_recreate_core_action: WebUiInstanceInstallAction,
    /// Whether a reinstall may replace the recorded install parameters. An
    /// instance restored from a snapshot reinstalls from that snapshot, which
    /// carries its own parameters.
    pub reinstall_spec_action: WebUiInstanceInstallAction,
    /// Whether the instance's Python directory already holds a runtime. The
    /// installer keeps an existing runtime and the PyTorch installed in it, so
    /// only then does a different Python version or PyTorch build need the
    /// Python environment recreated to take effect.
    pub python_environment_established: bool,
    /// What deleting the instance with its files removes
    /// (`plan_instance_deletion`), for the confirmation to name; `None` when
    /// the plan is refused, which the deletion itself then reports.
    pub deleted_directories: Option<Vec<PathBuf>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateWebUiInstanceComponentPathsRequest {
    pub instance_id: String,
    pub expected_config_revision: u64,
    pub component_paths: InstanceComponentPaths,
}

/// Replace the instance's quick-access list wholesale under an observed
/// revision. There is no per-entry mutation: the editor owns the whole list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateWebUiInstanceQuickPathsRequest {
    pub instance_id: String,
    pub expected_config_revision: u64,
    pub quick_paths: Vec<InstanceQuickPath>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RenameWebUiInstanceRequest {
    pub instance_id: String,
    pub expected_instance_identity: String,
    pub expected_config_revision: u64,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReinstallWebUiInstanceRequest {
    pub instance_id: String,
    pub expected_instance_identity: String,
    pub expected_config_revision: u64,
    #[serde(default)]
    pub force_recreate_python: bool,
    #[serde(default)]
    pub force_recreate_core: bool,
    /// Install parameters for this reinstall, persisted for every later install
    /// once it starts. Absent replays the recorded spec. A different Python
    /// version or PyTorch build requires `force_recreate_python`, because only
    /// a new Python environment receives them. Validated by the instance's
    /// type.
    #[serde(default)]
    pub spec: Option<Map<String, Value>>,
}
