mod catalog;
mod fields;
mod form;
mod selection;
mod spec;

pub use catalog::{
    PytorchOptionSource, WebUiFamilyInstallOptionCatalog, WebUiInstallContract,
    WebUiInstallOptionCatalog, WebUiInstallRequest, WebUiInstallSpecContract,
    WebUiInstallTypeCatalog,
};
pub use form::{
    CATALOG_BRANCHES, CATALOG_PYTORCH, INSTALL_GROUP, PYTHON_ENVIRONMENT_GROUP,
    builtin_install_form,
};
pub use selection::{BranchSelection, InvokeAiPytorchFamily, PytorchInstallSelection};
pub(crate) use spec::FamilyInstallSpec;
pub use spec::{WebUiInstallSelection, WebUiInstallSpec};
