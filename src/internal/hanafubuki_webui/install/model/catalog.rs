use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::internal::hanafubuki_foundation::form_schema::SchemaGroup;
use crate::internal::hanafubuki_instance_contracts::{InstanceTypeId, WebUiFamily};
use serde::Serialize;

use super::WebUiInstallSpec;
use crate::internal::hanafubuki_webui::instance_types::{BuiltinProvider, InstanceTypeRegistry};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebUiInstallRequest {
    pub target_path: PathBuf,
    pub spec: WebUiInstallSpec,
}

/// What a family's install spec takes. Static, so a form can lay out a
/// family's inputs without the App API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiInstallSpecContract {
    /// The spec fields the family accepts besides `type`, so a client sends
    /// only those instead of keeping its own list per family.
    pub fields: &'static [&'static str],
    /// What the family's `pytorch` selection names.
    pub pytorch_source: PytorchOptionSource,
}

impl WebUiInstallSpecContract {
    pub fn of(family: WebUiFamily) -> Self {
        Self {
            fields: WebUiInstallSpec::accepted_fields(family),
            pytorch_source: WebUiInstallSpec::pytorch_option_source(family),
        }
    }
}

/// The install form of every registered type, by type id: what the
/// creation and reinstall forms lay out, without the App API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiInstallContract {
    pub types: BTreeMap<InstanceTypeId, Vec<SchemaGroup>>,
}

impl WebUiInstallContract {
    pub fn of(registry: &InstanceTypeRegistry) -> Self {
        Self {
            types: registry
                .descriptors()
                .into_iter()
                .filter_map(|descriptor| {
                    let provider = registry.get(&descriptor.id)?;
                    Some((descriptor.id, provider.install_form()))
                })
                .collect(),
        }
    }
}

/// One family's install options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiFamilyInstallOptionCatalog {
    #[serde(flatten)]
    pub contract: WebUiInstallSpecContract,
    /// The PyTorch selections the family's installer accepts.
    pub pytorch: BTreeSet<String>,
    /// The branch presets, for a family whose spec takes a branch.
    pub branches: BTreeSet<String>,
}

impl WebUiFamilyInstallOptionCatalog {
    pub fn new(family: WebUiFamily, pytorch: BTreeSet<String>, branches: BTreeSet<String>) -> Self {
        Self {
            contract: WebUiInstallSpecContract::of(family),
            pytorch,
            branches,
        }
    }
}

/// Each family's current install options, keyed by family: the Agent's
/// install catalog names families as its selections do.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiInstallOptionCatalog {
    pub families: BTreeMap<WebUiFamily, WebUiFamilyInstallOptionCatalog>,
}

/// The same options keyed by type id, as the frontend reads them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiInstallTypeCatalog {
    pub types: BTreeMap<InstanceTypeId, WebUiFamilyInstallOptionCatalog>,
}

impl WebUiInstallOptionCatalog {
    pub fn by_type(self) -> WebUiInstallTypeCatalog {
        WebUiInstallTypeCatalog {
            types: self
                .families
                .into_iter()
                .map(|(family, options)| (BuiltinProvider::type_id(family), options))
                .collect(),
        }
    }

    /// A family's options; one the catalog does not list offers none.
    pub fn family(&self, family: WebUiFamily) -> &WebUiFamilyInstallOptionCatalog {
        static NONE: WebUiFamilyInstallOptionCatalog = WebUiFamilyInstallOptionCatalog {
            contract: WebUiInstallSpecContract {
                fields: &[],
                pytorch_source: PytorchOptionSource::SharedBuilds,
            },
            pytorch: BTreeSet::new(),
            branches: BTreeSet::new(),
        };
        self.families.get(&family).unwrap_or(&NONE)
    }
}

/// Where the App API lists the PyTorch selections a family's installer takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PytorchOptionSource {
    /// The shared PyTorch build library (`pytorch.library`).
    SharedBuilds,
    /// The family's own device categories (`<family>.pytorch.catalog`).
    DeviceCategories,
}
