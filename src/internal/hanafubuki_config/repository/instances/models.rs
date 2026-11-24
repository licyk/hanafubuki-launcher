use crate::internal::hanafubuki_config::repository::settings::AppSettings;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceComponentPath {
    pub path: PathBuf,
    /// A folder the user chose that the instance does not own: nothing the
    /// host does creates, empties, replaces or deletes it. A record without
    /// the field is an owned component.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
}

/// An instance's component directories by name, the components its type
/// declares. `core` is always present; `python` is there for a type with a
/// managed Python environment.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "BTreeMap<String, InstanceComponentPath>")]
pub struct InstanceComponentPaths(BTreeMap<String, InstanceComponentPath>);

/// One user-named quick-access directory for an instance.
///
/// This is presentation-adjacent instance configuration, not a component
/// location: it never participates in launch, install, or path-policy layout
/// and is only resolved when the user asks Rust to open it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceQuickPath {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchArgumentOrigin {
    PendingDefault,
    BuiltIn,
    User,
}

impl InstanceComponentPaths {
    pub const CORE: &'static str = "core";
    pub const PYTHON: &'static str = "python";
    /// The components of an instance whose type is not known here: the
    /// layout every built-in type declares.
    pub const PYTHON_AND_CORE: [&'static str; 2] = [Self::CORE, Self::PYTHON];

    pub fn new(core: impl Into<PathBuf>) -> Self {
        Self(BTreeMap::from([(
            Self::CORE.to_string(),
            InstanceComponentPath {
                path: core.into(),
                external: false,
            },
        )]))
    }

    /// The built-in layout: Python and Core.
    pub fn python_and_core(python: impl Into<PathBuf>, core: impl Into<PathBuf>) -> Self {
        Self::new(core).with(Self::PYTHON, python)
    }

    pub fn with(mut self, name: &str, path: impl Into<PathBuf>) -> Self {
        self.set(name, path);
        self
    }

    /// Each named component in its own directory below the instance root.
    pub fn defaults<'a>(
        instance_path: &Path,
        components: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        components
            .into_iter()
            .fold(Self::new(instance_path.join(Self::CORE)), |paths, name| {
                paths.with(name, instance_path.join(name))
            })
    }

    pub fn core(&self) -> &Path {
        &self.0[Self::CORE].path
    }

    pub fn python(&self) -> Option<&Path> {
        self.get(Self::PYTHON)
    }

    pub fn get(&self, name: &str) -> Option<&Path> {
        self.0.get(name).map(|component| component.path.as_path())
    }

    /// A component the caller's operation cannot run without.
    pub fn require(&self, name: &str) -> Result<&Path, String> {
        self.get(name)
            .ok_or_else(|| format!("the instance has no `{name}` component"))
    }

    /// Places `name` at `path`, an owned component.
    pub fn set(&mut self, name: &str, path: impl Into<PathBuf>) {
        self.0.insert(
            name.to_string(),
            InstanceComponentPath {
                path: path.into(),
                external: false,
            },
        );
    }

    /// Places `name` at `path`, a folder the instance does not own.
    pub fn set_external(&mut self, name: &str, path: impl Into<PathBuf>) {
        self.0.insert(
            name.to_string(),
            InstanceComponentPath {
                path: path.into(),
                external: true,
            },
        );
    }

    pub fn is_external(&self, name: &str) -> bool {
        self.0.get(name).is_some_and(|component| component.external)
    }

    /// The components the instance does not own, by name.
    pub fn external(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.0
            .iter()
            .filter(|(_, component)| component.external)
            .map(|(name, component)| (name.as_str(), component.path.as_path()))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.0
            .iter()
            .map(|(name, component)| (name.as_str(), component.path.as_path()))
    }

    /// Replace every path with `map(name, path)`, stopping at the first error.
    pub fn try_map(
        &self,
        mut map: impl FnMut(&str, &Path) -> Result<PathBuf, String>,
    ) -> Result<Self, String> {
        let mut mapped = self.clone();
        for (name, component) in &mut mapped.0 {
            component.path = map(name, &component.path)?;
        }
        Ok(mapped)
    }
}

impl TryFrom<BTreeMap<String, InstanceComponentPath>> for InstanceComponentPaths {
    type Error = String;

    fn try_from(components: BTreeMap<String, InstanceComponentPath>) -> Result<Self, String> {
        if components.contains_key(Self::CORE) {
            Ok(Self(components))
        } else {
            Err("componentPaths must name a `core` component".to_string())
        }
    }
}

impl Serialize for InstanceComponentPaths {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// Typed, persisted per-instance configuration.
///
/// This is deliberately a **generic** instance resource: it carries no
/// product/WebUI type. It is persisted, never an IPC request, so the persisted
/// reader drops unknown and product-specific fields rather than failing.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    /// Rust-owned optimistic concurrency revision for instance-scoped edits.
    #[serde(default = "initial_config_revision")]
    pub config_revision: u64,
    /// Explicit install directory; when `None`, resolution derives it from the
    /// global default install dir and the instance id.
    #[serde(default)]
    pub install_dir: Option<PathBuf>,
    /// Explicit component locations. Instance creation always persists this;
    /// `None` resolves to the built-in layout below `install_dir`.
    #[serde(default)]
    pub component_paths: Option<InstanceComponentPaths>,
    /// User-named quick-access directories, in the user's own order.
    #[serde(default)]
    pub quick_paths: Vec<InstanceQuickPath>,
    /// Instance-scoped environment variables (validated keys/values).
    #[serde(default)]
    pub env_vars: BTreeMap<String, String>,
    /// Canonical, shell-free launch arguments validated by Rust before save and launch.
    #[serde(default)]
    pub launch_args: Vec<String>,
    /// Rust-owned provenance used to preserve explicit user edits across
    /// reinstall and built-in default refreshes.
    pub launch_argument_origin: LaunchArgumentOrigin,
    /// Partial global-settings overrides for this instance (a `camelCase`
    /// settings object). Validated with the same rules as a global patch.
    #[serde(default)]
    pub settings_overrides: Value,
}

/// The fully resolved, effective configuration for one instance.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInstanceConfig {
    pub id: String,
    pub name: String,
    pub install_dir: PathBuf,
    pub component_paths: InstanceComponentPaths,
    pub quick_paths: Vec<InstanceQuickPath>,
    pub env_vars: BTreeMap<String, String>,
    pub launch_args: Vec<String>,
    pub launch_argument_origin: LaunchArgumentOrigin,
    /// Effective settings after layering defaults/global/instance/runtime.
    pub settings: AppSettings,
}

pub(super) const fn initial_config_revision() -> u64 {
    1
}
