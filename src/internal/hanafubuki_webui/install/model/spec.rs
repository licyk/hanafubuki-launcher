use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::super::comfyui::ComfyuiInstallSpec;
use super::super::error::WebUiInstallBuildError;
use super::super::fooocus::FooocusInstallSpec;
use super::super::invokeai::InvokeaiInstallSpec;
use super::super::mizuoto_tts::MizuotoTtsInstallSpec;
use super::super::qwen_tts::QwenTtsInstallSpec;
use super::super::rvc_next::RvcNextInstallSpec;
use super::super::sd_trainer::SdTrainerInstallSpec;
use super::super::settings::ResolvedInstallSettings;
use super::super::stable_diffusion_webui::StableDiffusionWebuiInstallSpec;
use super::fields::declared_fields;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

use super::{BranchSelection, PytorchOptionSource, WebUiFamilyInstallOptionCatalog};

/// One family's install spec. Each family's module implements it, so what a
/// family installs, accepts, and defaults to lives with that family and
/// [`WebUiInstallSpec`] only dispatches to it.
pub(crate) trait FamilyInstallSpec: Default + DeserializeOwned {
    const DEFAULT_PYTHON_VERSION: PythonVersion;
    /// Where the App API lists the PyTorch selections the installer takes.
    const PYTORCH_OPTIONS: PytorchOptionSource = PytorchOptionSource::SharedBuilds;
    /// Whether the Core directory must exist before runtime preparation.
    const REQUIRES_PREEXISTING_CORE_DIRECTORY: bool = true;

    fn python_version(&self) -> Option<PythonVersion>;
    fn set_python_version(&mut self, version: PythonVersion);
    /// The configured PyTorch selection, in the family's own vocabulary.
    fn pytorch_selection(&self) -> Option<&str>;
    fn branch(&self) -> Option<&BranchSelection> {
        None
    }
    fn set_branch(&mut self, _branch: BranchSelection) -> Result<(), String> {
        Err("this WebUI family has no persisted branch preset".to_string())
    }
    /// Check the spec's selections against the family's current catalog.
    fn validate(
        &self,
        options: &WebUiFamilyInstallOptionCatalog,
    ) -> Result<(), WebUiInstallBuildError>;
    /// `python -m sd_webui_all_in_one <family> install ...` arguments.
    fn install_args(
        &self,
        settings: &ResolvedInstallSettings,
        target_path: &str,
    ) -> Result<Vec<String>, WebUiInstallBuildError>;
    /// The launch arguments a new instance starts with.
    fn default_launch_arguments(&self) -> &'static [&'static str];
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebUiInstallSpec {
    StableDiffusionWebui(StableDiffusionWebuiInstallSpec),
    Comfyui(ComfyuiInstallSpec),
    Invokeai(InvokeaiInstallSpec),
    Fooocus(FooocusInstallSpec),
    SdTrainer(SdTrainerInstallSpec),
    QwenTts(QwenTtsInstallSpec),
    RvcNext(RvcNextInstallSpec),
    MizuotoTts(MizuotoTtsInstallSpec),
}

/// Run `$body` with `$spec` bound to the family-specific spec.
macro_rules! with_spec {
    ($value:expr, $spec:ident => $body:expr) => {
        match $value {
            WebUiInstallSpec::StableDiffusionWebui($spec) => $body,
            WebUiInstallSpec::Comfyui($spec) => $body,
            WebUiInstallSpec::Invokeai($spec) => $body,
            WebUiInstallSpec::Fooocus($spec) => $body,
            WebUiInstallSpec::SdTrainer($spec) => $body,
            WebUiInstallSpec::QwenTts($spec) => $body,
            WebUiInstallSpec::RvcNext($spec) => $body,
            WebUiInstallSpec::MizuotoTts($spec) => $body,
        }
    };
}

/// Run `$body` with the type `$spec` naming the family's spec type and
/// `$variant` its `WebUiInstallSpec` constructor.
macro_rules! with_family {
    ($family:expr, $spec:ident, $variant:ident => $body:expr) => {
        match $family {
            WebUiFamily::StableDiffusionWebui => {
                type $spec = StableDiffusionWebuiInstallSpec;
                let $variant = WebUiInstallSpec::StableDiffusionWebui;
                $body
            }
            WebUiFamily::Comfyui => {
                type $spec = ComfyuiInstallSpec;
                let $variant = WebUiInstallSpec::Comfyui;
                $body
            }
            WebUiFamily::Invokeai => {
                type $spec = InvokeaiInstallSpec;
                let $variant = WebUiInstallSpec::Invokeai;
                $body
            }
            WebUiFamily::Fooocus => {
                type $spec = FooocusInstallSpec;
                let $variant = WebUiInstallSpec::Fooocus;
                $body
            }
            WebUiFamily::SdTrainer => {
                type $spec = SdTrainerInstallSpec;
                let $variant = WebUiInstallSpec::SdTrainer;
                $body
            }
            WebUiFamily::QwenTts => {
                type $spec = QwenTtsInstallSpec;
                let $variant = WebUiInstallSpec::QwenTts;
                $body
            }
            WebUiFamily::RvcNext => {
                type $spec = RvcNextInstallSpec;
                let $variant = WebUiInstallSpec::RvcNext;
                $body
            }
            WebUiFamily::MizuotoTts => {
                type $spec = MizuotoTtsInstallSpec;
                let $variant = WebUiInstallSpec::MizuotoTts;
                $body
            }
        }
    };
}

/// Loose install selections, as the Agent supplies them. The family's spec
/// decides which it accepts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebUiInstallSelection {
    pub pytorch: Option<String>,
    pub python_version: Option<String>,
    pub branch: Option<String>,
    pub predownload_models: Option<bool>,
    pub predownload_extensions: Option<bool>,
}

impl WebUiInstallSpec {
    pub fn family(&self) -> WebUiFamily {
        match self {
            Self::StableDiffusionWebui(_) => WebUiFamily::StableDiffusionWebui,
            Self::Comfyui(_) => WebUiFamily::Comfyui,
            Self::Invokeai(_) => WebUiFamily::Invokeai,
            Self::Fooocus(_) => WebUiFamily::Fooocus,
            Self::SdTrainer(_) => WebUiFamily::SdTrainer,
            Self::QwenTts(_) => WebUiFamily::QwenTts,
            Self::RvcNext(_) => WebUiFamily::RvcNext,
            Self::MizuotoTts(_) => WebUiFamily::MizuotoTts,
        }
    }

    /// The family's spec with every selection left to its default.
    pub fn default_for(family: WebUiFamily) -> Self {
        with_family!(family, Spec, variant => variant(Spec::default()))
    }

    /// The spec a snapshot-restored instance records: only its Python version
    /// is known, because the snapshot itself restores the rest.
    pub fn snapshot_install(family: WebUiFamily, python_version: PythonVersion) -> Self {
        with_family!(family, Spec, variant => {
            let mut spec = Spec::default();
            spec.set_python_version(python_version);
            variant(spec)
        })
    }

    /// The fields the family's spec accepts besides `type`.
    pub fn accepted_fields(family: WebUiFamily) -> &'static [&'static str] {
        with_family!(family, Spec, _variant => declared_fields::<Spec>())
    }

    pub fn pytorch_option_source(family: WebUiFamily) -> PytorchOptionSource {
        with_family!(family, Spec, _variant => Spec::PYTORCH_OPTIONS)
    }

    /// Whether the family's spec takes a branch preset.
    pub fn takes_branch(family: WebUiFamily) -> bool {
        Self::accepted_fields(family).contains(&"branch")
    }

    /// The family's spec from loose selections. A selection the family's spec
    /// does not declare is rejected by that spec rather than dropped.
    pub fn from_selection(
        family: WebUiFamily,
        selection: WebUiInstallSelection,
    ) -> Result<Self, String> {
        let mut object = Map::new();
        for (field, value) in [
            ("pytorch", selection.pytorch.map(Value::String)),
            ("pythonVersion", selection.python_version.map(Value::String)),
            ("branch", selection.branch.map(Value::String)),
            (
                "predownloadModels",
                selection.predownload_models.map(Value::Bool),
            ),
            (
                "predownloadExtensions",
                selection.predownload_extensions.map(Value::Bool),
            ),
        ] {
            if let Some(value) = value {
                object.insert(field.to_string(), value);
            }
        }
        Self::from_object(family, object)
            .map_err(|error| format!("{} install selection: {error}", family.product_name()))
    }

    /// The family's spec from its fields, as a request or a record names the
    /// family beside them. A field the family does not declare is refused.
    pub fn from_object(family: WebUiFamily, object: Map<String, Value>) -> Result<Self, String> {
        let mut tagged = object;
        let tag = serde_json::to_value(family).map_err(|error| error.to_string())?;
        if tagged.insert("type".to_string(), tag).is_some() {
            return Err("unknown field `type`".to_string());
        }
        serde_json::from_value(Value::Object(tagged)).map_err(|error| error.to_string())
    }

    /// A persisted spec: the fields its family no longer declares are dropped
    /// first, so only a declared field with an unusable value fails.
    pub fn from_persisted_object(
        family: WebUiFamily,
        mut object: Map<String, Value>,
    ) -> Result<Self, String> {
        let fields = Self::accepted_fields(family);
        object.retain(|key, _| fields.contains(&key.as_str()));
        Self::from_object(family, object)
    }

    /// The spec's fields, without the family, which the record keeps as the
    /// instance's type.
    pub fn to_object(&self) -> Map<String, Value> {
        let mut object = match serde_json::to_value(self) {
            Ok(Value::Object(object)) => object,
            _ => unreachable!("a WebUI install spec serializes as an object"),
        };
        object.remove("type");
        object
    }

    /// Whether the configured Core directory must already be materialized
    /// before runtime preparation. InvokeAI creates its Core directory during
    /// first launch; all other families require installation to create it.
    pub fn requires_preexisting_core_directory(&self) -> bool {
        with_spec!(self, spec => requires_preexisting_core_directory(spec))
    }

    pub fn python_version(&self) -> Option<PythonVersion> {
        with_spec!(self, spec => spec.python_version())
    }

    pub fn default_python_version(&self) -> PythonVersion {
        with_spec!(self, spec => default_python_version(spec))
    }

    /// Whether `other` asks for a different Python environment than this spec.
    ///
    /// The installer keeps a usable Python runtime and an installed PyTorch, so
    /// the Python version and PyTorch build reach an instance only when its
    /// Python environment is created anew. An unset version is compared as the
    /// family default it resolves to.
    pub fn python_environment_differs(&self, other: &Self) -> bool {
        self.python_version()
            .unwrap_or_else(|| self.default_python_version())
            != other
                .python_version()
                .unwrap_or_else(|| other.default_python_version())
            || self.pytorch_selection_name() != other.pytorch_selection_name()
    }

    /// Configured branch selection, for families that support one.
    pub fn branch_name(&self) -> Option<String> {
        with_spec!(self, spec => spec.branch().map(|branch| branch.as_str().to_string()))
    }

    pub fn set_branch_name(&mut self, value: String) -> Result<(), String> {
        let branch = BranchSelection::try_from(value).map_err(|error| error.to_string())?;
        with_spec!(self, spec => spec.set_branch(branch))
    }

    /// Configured PyTorch build (or InvokeAI device-family) selection.
    pub fn pytorch_selection_name(&self) -> Option<String> {
        with_spec!(self, spec => spec.pytorch_selection().map(str::to_string))
    }

    pub(crate) fn validate(
        &self,
        catalog: &super::WebUiInstallOptionCatalog,
    ) -> Result<(), WebUiInstallBuildError> {
        let options = catalog.family(self.family());
        with_spec!(self, spec => spec.validate(options))
    }

    pub(crate) fn install_args(
        &self,
        settings: &ResolvedInstallSettings,
        target_path: &str,
    ) -> Result<Vec<String>, WebUiInstallBuildError> {
        with_spec!(self, spec => spec.install_args(settings, target_path))
    }

    pub(crate) fn default_launch_arguments(&self) -> &'static [&'static str] {
        with_spec!(self, spec => spec.default_launch_arguments())
    }
}

fn requires_preexisting_core_directory<T: FamilyInstallSpec>(_spec: &T) -> bool {
    T::REQUIRES_PREEXISTING_CORE_DIRECTORY
}

fn default_python_version<T: FamilyInstallSpec>(_spec: &T) -> PythonVersion {
    T::DEFAULT_PYTHON_VERSION
}
