use std::collections::BTreeMap;
use std::fmt;

use crate::internal::hanafubuki_foundation::form_schema::SchemaText;
use serde::{Deserialize, Deserializer, Serialize};

use crate::internal::hanafubuki_instance_contracts::ServerRoleDescriptor;

/// The identity of an instance type: `<namespace>:<name>`, each part of
/// `[a-z0-9.-]`. The built-in types are `builtin:<name>`; an extension's
/// types are `<extension-id>:<name>`. Validated once, where it is parsed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct InstanceTypeId(String);

impl InstanceTypeId {
    pub fn parse(value: &str) -> Result<Self, String> {
        let valid_part = |part: &str| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-'))
        };
        match value.split_once(':') {
            Some((namespace, name)) if valid_part(namespace) && valid_part(name) => {
                Ok(Self(value.to_string()))
            }
            _ => Err(format!(
                "instance type id `{value}` is not `<namespace>:<name>` of [a-z0-9.-]"
            )),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstanceTypeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for InstanceTypeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Where an instance type's behaviour comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceTypeKind {
    /// Compiled into the application.
    Builtin,
    /// A descriptor the application interprets: its install steps, its
    /// server commands and the parameters they take.
    Declarative,
}

/// What an instance type's processes run on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum InstanceRuntime {
    /// Nothing the application prepares: the type's own files run as they
    /// are. No Python environment, control server or hotpatcher.
    None,
    /// A managed Python environment in the instance's `python` component.
    /// `version` is the type's default; a built-in type's spec chooses its
    /// own.
    Python {
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
    /// A program found on the user's `PATH`, such as `node`.
    SystemBinary { name: String },
}

impl InstanceRuntime {
    pub fn is_python(&self) -> bool {
        matches!(self, Self::Python { .. })
    }
}

/// A component that may be a folder the instance does not own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalComponent {
    /// The program the folder holds, relative to it and `/`-separated, on
    /// this platform (`node.exe`, `bin/node`): the Paths panel picks it,
    /// and a chosen folder must hold it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
}

/// What the type registry tells the application and the frontend about one
/// instance type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceTypeDescriptor {
    pub id: InstanceTypeId,
    pub kind: InstanceTypeKind,
    pub label: SchemaText,
    /// The prefix for generated instance names, independent of the display label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_name: Option<String>,
    pub description: SchemaText,
    /// The key of a preview image the frontend bundles, for a built-in type.
    pub preview: Option<String>,
    /// Where an extension's type's preview image is served: the `hfext`
    /// address of a file of its package.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    /// The components an instance of this type has, `core` first. Each is a
    /// directory in the instance's `componentPaths`.
    pub components: Vec<String>,
    /// The labels of the components that declare one, by name; a component
    /// without one is shown by its name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub component_labels: BTreeMap<String, SchemaText>,
    /// The components the user may point at a folder of their own
    /// (`external = "allowed"`), by name: such a folder is never created,
    /// emptied, replaced or deleted by the host.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub external_components: BTreeMap<String, ExternalComponent>,
    pub runtime: InstanceRuntime,
    /// The processes an instance of this type runs, in declaration order;
    /// [`crate::internal::hanafubuki_instance_contracts::server_start_order`] gives the order they start in.
    pub servers: Vec<ServerRoleDescriptor>,
    /// The management operations an instance of this type offers beyond
    /// the lifecycle every type has (create, install, start, stop, remove):
    /// the standard ones it declares (`update`, `terminal`, `versions`,
    /// `extensions`, `models`, …), which decide the management sections the
    /// frontend lists.
    pub operations: Vec<String>,
    /// The operations of its own among them: those that are none of the
    /// standard ones, which its extension's backend runs
    /// (`provider.manage.<name>`). A built-in type has none.
    pub own_operations: Vec<String>,
    /// Whether the application checks an instance of this type for updates
    /// (the daily check and the card's on-demand one): a built-in type
    /// through its versions, a type its extension's backend provides when
    /// it declares the standard operation `update-check`
    /// (`provider.checkUpdate`).
    pub checks_updates: bool,
    /// The built-in type whose management an instance of this type takes
    /// (`[management] adopts`): its control server serves the sections
    /// this type declares (`versions`, `extensions`, `models`,
    /// `environment`) as that built-in type's. None for a built-in type,
    /// whose management is its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adopts: Option<InstanceTypeId>,
    /// Whether an instance of this type only comes from discovery attaching
    /// an existing installation: it is never created, nor imported by hand.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub attach_only: bool,
}

#[cfg(test)]
mod tests;
