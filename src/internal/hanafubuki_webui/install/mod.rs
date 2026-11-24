mod comfyui;
mod common;
mod default_launch_arguments;
mod error;
mod fooocus;
mod invokeai;
mod mizuoto_tts;
mod model;
mod orchestrator;
mod planner;
mod qwen_tts;
mod rvc_next;
mod sd_trainer;
mod settings;
mod snapshot_restore;
mod stable_diffusion_webui;
pub mod steps;
mod validation;

pub use comfyui::ComfyuiInstallSpec;
pub use default_launch_arguments::default_launch_arguments;
pub use error::WebUiInstallBuildError;
pub use fooocus::FooocusInstallSpec;
pub use invokeai::InvokeaiInstallSpec;
pub use mizuoto_tts::MizuotoTtsInstallSpec;
pub use model::{
    BranchSelection, CATALOG_BRANCHES, CATALOG_PYTORCH, INSTALL_GROUP, InvokeAiPytorchFamily,
    PYTHON_ENVIRONMENT_GROUP, PytorchInstallSelection, PytorchOptionSource,
    WebUiFamilyInstallOptionCatalog, WebUiInstallContract, WebUiInstallOptionCatalog,
    WebUiInstallRequest, WebUiInstallSelection, WebUiInstallSpec, WebUiInstallSpecContract,
    WebUiInstallTypeCatalog, builtin_install_form,
};
pub(crate) use orchestrator::INSTALL_MODULE;
pub use orchestrator::{
    CoreInstallTaskRequest, DEFAULT_TERMINAL_OPERATION_RETENTION, InstanceInstallBackend,
    InstanceInstallCleanupStatus, InstanceInstallCoreStartError, InstanceInstallCoreTask,
    InstanceInstallLogOutput, InstanceInstallOperation, InstanceInstallOutputChunk,
    InstanceInstallOutputStream, InstanceInstallPendingCleanup, InstanceInstallPendingCleanupKind,
    InstanceInstallPendingCleanupStatus, InstanceInstallPhase, InstanceInstallPreparation,
    InstanceInstallService, InstanceInstallStageError, InstanceInstallTaskCleanupResult,
    InstanceInstallTaskRef, InstanceInstallTaskState, MAX_PENDING_EVICTION_CLEANUPS,
    MAX_PENDING_OWNERSHIP_CLEANUPS, StartInstanceInstallRequest, StepInstallPreparation,
};
pub use planner::{WebUiInstallPlan, build_webui_install_plan};
pub use qwen_tts::QwenTtsInstallSpec;
pub use rvc_next::RvcNextInstallSpec;
pub use sd_trainer::SdTrainerInstallSpec;
pub use settings::{
    GithubMirrorSetting, ModelDownloadSource, PypiMirrorMode, ResolvedInstallSettings,
};
pub use snapshot_restore::{WebUiSnapshotRestorePlan, build_webui_snapshot_restore_plan};
pub use stable_diffusion_webui::StableDiffusionWebuiInstallSpec;
pub use validation::ValidatedWebUiInstallRequest;

pub fn build_webui_install_args(
    request: &ValidatedWebUiInstallRequest,
    settings: &ResolvedInstallSettings,
) -> Result<Vec<String>, WebUiInstallBuildError> {
    let args = request
        .spec()
        .install_args(settings, request.target_path_arg())?;
    common::validate_final_args(&args)?;
    Ok(args)
}

#[cfg(test)]
mod tests;
