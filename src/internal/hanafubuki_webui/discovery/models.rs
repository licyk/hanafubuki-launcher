use std::collections::{BTreeMap, BTreeSet};

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_instance_contracts::{InstanceTypeId, WebUiFamily};
use serde_json::{Map, Value};

use crate::internal::hanafubuki_webui::instance_types::BuiltinProvider;

pub const MAX_DISCOVERY_WARNINGS: usize = 256;

/// An existing installation the scanner found: a built-in family's layout,
/// or a directory a registered type's signature matched.
#[derive(Clone, Debug, PartialEq)]
pub enum WebUiDiscoveryCandidate {
    Builtin(BuiltinDiscoveryCandidate),
    Declared(DeclaredDiscoveryCandidate),
}

impl WebUiDiscoveryCandidate {
    /// What identifies it: a built-in candidate's core, a declared one's root.
    pub fn path(&self) -> &ResolvedPath {
        match self {
            Self::Builtin(candidate) => &candidate.core_path,
            Self::Declared(candidate) => &candidate.root,
        }
    }

    pub fn type_id(&self) -> InstanceTypeId {
        match self {
            Self::Builtin(candidate) => BuiltinProvider::type_id(candidate.family),
            Self::Declared(candidate) => candidate.type_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltinDiscoveryCandidate {
    pub family: WebUiFamily,
    /// Existing instance root inferred from a complete portable layout.
    /// Missing-Python candidates leave allocation to the persistence adapter.
    pub instance_path: Option<ResolvedPath>,
    pub core_path: ResolvedPath,
    pub python_dir: Option<ResolvedPath>,
}

/// A directory a type's discovery signature matched.
#[derive(Clone, Debug, PartialEq)]
pub struct DeclaredDiscoveryCandidate {
    pub type_id: InstanceTypeId,
    pub root: ResolvedPath,
    /// The components the signature names; the others take their defaults
    /// below the root.
    pub component_paths: BTreeMap<String, ResolvedPath>,
    /// The install parameter values the signature gives.
    pub values: Map<String, Value>,
    /// The components the signature marks external.
    pub external: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebUiDiscoveryProgress {
    pub files: u64,
    pub directories: u64,
    pub webuis: u64,
    pub current_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebUiDiscoveryWarning {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WebUiDiscoveryReport {
    pub candidates: Vec<WebUiDiscoveryCandidate>,
    pub warnings: Vec<WebUiDiscoveryWarning>,
    pub root_failures: Vec<WebUiDiscoveryWarning>,
    pub files: u64,
    pub directories: u64,
    pub cancelled: bool,
    pub warnings_truncated: bool,
}
