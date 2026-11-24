use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::json;

use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use crate::internal::hanafubuki_python_runtime::PythonVersion;

use super::*;

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn pytorch(value: &str) -> PytorchInstallSelection {
    PytorchInstallSelection::try_from(value.to_string()).unwrap()
}

fn invoke_family(value: &str) -> InvokeAiPytorchFamily {
    InvokeAiPytorchFamily::try_from(value.to_string()).unwrap()
}

fn branch(value: &str) -> BranchSelection {
    BranchSelection::try_from(value.to_string()).unwrap()
}

fn target_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\WebUI\target")
    } else {
        PathBuf::from("/tmp/webui-target")
    }
}

fn path_arg() -> String {
    target_path().to_str().unwrap().to_string()
}

fn request(spec: WebUiInstallSpec) -> WebUiInstallRequest {
    WebUiInstallRequest {
        target_path: target_path(),
        spec,
    }
}

fn stable_spec() -> WebUiInstallSpec {
    WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
        pytorch: None,
        python_version: None,
        branch: None,
        predownload_models: None,
        predownload_extensions: None,
    })
}

fn comfyui_spec() -> WebUiInstallSpec {
    WebUiInstallSpec::Comfyui(ComfyuiInstallSpec {
        pytorch: None,
        python_version: None,
        predownload_models: None,
        predownload_extensions: None,
    })
}

fn invokeai_spec() -> WebUiInstallSpec {
    WebUiInstallSpec::Invokeai(InvokeaiInstallSpec {
        pytorch: None,
        python_version: None,
        predownload_models: None,
    })
}

fn fooocus_spec() -> WebUiInstallSpec {
    WebUiInstallSpec::Fooocus(FooocusInstallSpec {
        pytorch: None,
        python_version: None,
        branch: None,
        predownload_models: None,
    })
}

fn sd_trainer_spec() -> WebUiInstallSpec {
    WebUiInstallSpec::SdTrainer(SdTrainerInstallSpec {
        pytorch: None,
        python_version: None,
        branch: None,
        predownload_models: None,
    })
}

fn qwen_tts_spec() -> WebUiInstallSpec {
    WebUiInstallSpec::QwenTts(QwenTtsInstallSpec {
        pytorch: None,
        python_version: None,
        predownload_models: None,
    })
}

fn rvc_next_spec() -> WebUiInstallSpec {
    WebUiInstallSpec::RvcNext(RvcNextInstallSpec {
        pytorch: None,
        python_version: None,
    })
}

fn mizuoto_tts_spec() -> WebUiInstallSpec {
    WebUiInstallSpec::MizuotoTts(MizuotoTtsInstallSpec {
        pytorch: None,
        python_version: None,
    })
}

fn catalog_with(entries: &[(WebUiFamily, &[&str], &[&str])]) -> WebUiInstallOptionCatalog {
    WebUiInstallOptionCatalog {
        families: entries
            .iter()
            .map(|(family, pytorch, branches)| {
                (
                    *family,
                    WebUiFamilyInstallOptionCatalog::new(*family, set(pytorch), set(branches)),
                )
            })
            .collect(),
    }
}

fn populated_catalog() -> WebUiInstallOptionCatalog {
    catalog_with(&[
        (
            WebUiFamily::StableDiffusionWebui,
            &["cu121"],
            &["sd_webui_forge"],
        ),
        (WebUiFamily::Comfyui, &["cu121"], &[]),
        (WebUiFamily::Invokeai, &["cuda"], &[]),
        (WebUiFamily::Fooocus, &["cu121"], &["fooocus_main"]),
        (WebUiFamily::SdTrainer, &["cu121"], &["sd_trainer_main"]),
        (WebUiFamily::QwenTts, &["cu121"], &[]),
        (WebUiFamily::RvcNext, &["cu121"], &[]),
        (WebUiFamily::MizuotoTts, &["cu121"], &[]),
    ])
}

fn build(
    spec: WebUiInstallSpec,
    settings: &ResolvedInstallSettings,
    catalog: &WebUiInstallOptionCatalog,
) -> Result<WebUiInstallPlan, WebUiInstallBuildError> {
    build_webui_install_plan(request(spec), settings, catalog)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

mod catalog_validation;
mod explicit_options;
mod family_contract;
mod minimum_args;
mod mirrors;
mod serde_contract;
