//! A built-in family's install form in the form schema: the fields its
//! spec accepts, as the creation and reinstall forms show them.

use crate::internal::hanafubuki_foundation::form_schema::{
    FieldKind, SchemaField, SchemaGroup, SchemaOption, SchemaText,
};
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

use super::{PytorchOptionSource, WebUiInstallSpec};

/// The option lists a built-in form's selects take from the install
/// catalog (`get_webui_install_catalog`), by the catalog's field name.
pub const CATALOG_BRANCHES: &str = "branches";
pub const CATALOG_PYTORCH: &str = "pytorch";

/// The group a new Python environment alone receives; a reinstall shows it
/// under the switch that rebuilds the environment.
pub const PYTHON_ENVIRONMENT_GROUP: &str = "pythonEnvironment";
/// The group every install applies.
pub const INSTALL_GROUP: &str = "install";

fn configuration(key: &str) -> SchemaText {
    SchemaText::key(format!("instances.create.configuration.{key}"))
}

fn group(id: &str, fields: Vec<SchemaField>) -> SchemaGroup {
    SchemaGroup {
        id: id.to_string(),
        label: SchemaText::key(format!("instances.install.sections.{id}")),
        fields,
    }
}

/// The family's install form: its Python environment, then what every
/// install applies, each holding only the fields the family's spec accepts.
/// Every field is optional, its unset entry the family's own default.
pub fn builtin_install_form(family: WebUiFamily) -> Vec<SchemaGroup> {
    let accepted = WebUiInstallSpec::accepted_fields(family);
    let takes = |key: &str| accepted.contains(&key);
    let mut environment = Vec::new();
    if takes("pythonVersion") {
        environment.push(
            SchemaField::new("pythonVersion", FieldKind::Select, configuration("python"))
                .options(
                    PythonVersion::ALL
                        .into_iter()
                        .map(|version| SchemaOption {
                            value: version.as_str().to_string(),
                            label: SchemaText::literal(version.as_str()),
                        })
                        .collect(),
                )
                .unset(configuration("familyDefault"))
                .advanced(),
        );
    }
    if takes("pytorch") {
        let label = match WebUiInstallSpec::pytorch_option_source(family) {
            PytorchOptionSource::SharedBuilds => "pytorchBuild",
            PytorchOptionSource::DeviceCategories => "deviceFamily",
        };
        let mut pytorch = SchemaField::new("pytorch", FieldKind::Select, configuration(label))
            .unset(configuration("automatic"))
            .advanced();
        pytorch.options_source = Some(CATALOG_PYTORCH.to_string());
        environment.push(pytorch);
    }
    let mut install = Vec::new();
    if takes("branch") {
        let mut branch = SchemaField::new("branch", FieldKind::Select, configuration("branch"))
            .unset(configuration("defaultBranch"));
        branch.options_source = Some(CATALOG_BRANCHES.to_string());
        install.push(branch);
    }
    for key in ["predownloadModels", "predownloadExtensions"] {
        if takes(key) {
            install.push(
                SchemaField::new(key, FieldKind::Boolean, configuration(key))
                    .unset(configuration("installerDefault"))
                    .advanced(),
            );
        }
    }
    vec![
        group(PYTHON_ENVIRONMENT_GROUP, environment),
        group(INSTALL_GROUP, install),
    ]
}
