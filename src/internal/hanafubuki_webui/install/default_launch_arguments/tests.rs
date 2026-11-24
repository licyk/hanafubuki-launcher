use super::*;
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use crate::internal::hanafubuki_webui::install::{
    BranchSelection, ComfyuiInstallSpec, FooocusInstallSpec, SdTrainerInstallSpec,
    StableDiffusionWebuiInstallSpec,
};

const SD_WEBUI_MAIN: &[&str] = &[
    "--theme",
    "dark",
    "--autolaunch",
    "--api",
    "--skip-load-model-at-start",
    "--skip-python-version-check",
    "--skip-version-check",
    "--no-download-sd-model",
];
const SD_WEBUI_FORGE: &[&str] = &[
    "--theme",
    "dark",
    "--autolaunch",
    "--api",
    "--skip-python-version-check",
    "--skip-version-check",
    "--no-download-sd-model",
];
const SD_WEBUI_FORGE_CLASSIC: &[&str] = &[
    "--theme",
    "dark",
    "--autolaunch",
    "--api",
    "--skip-python-version-check",
    "--skip-version-check",
];
const SD_WEBUI_AMDGPU: &[&str] = &[
    "--theme",
    "dark",
    "--autolaunch",
    "--api",
    "--skip-torch-cuda-test",
    "--backend",
    "directml",
    "--skip-python-version-check",
    "--skip-version-check",
    "--no-download-sd-model",
];
const SD_NEXT: &[&str] = &["--autolaunch", "--use-cuda"];
const FOOOCUS: &[&str] = &[
    "--language",
    "zh",
    "--preset",
    "sd_webui_all_in_one",
    "--disable-offload-from-vram",
    "--disable-analytics",
    "--always-download-new-model",
];
const SD_TRAINER: &[&str] = &["--skip-prepare-onnxruntime"];
const KOHYA_GUI: &[&str] = &["--inbrowser", "--language", "zh-CN", "--noverify"];

fn owned(arguments: &[&str]) -> Vec<String> {
    arguments
        .iter()
        .map(|argument| argument.to_string())
        .collect()
}

fn branch(value: &str) -> Option<BranchSelection> {
    Some(BranchSelection::try_from(value.to_string()).unwrap())
}

fn stable(value: &str) -> WebUiInstallSpec {
    WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
        branch: branch(value),
        ..Default::default()
    })
}

fn trainer(value: &str) -> WebUiInstallSpec {
    WebUiInstallSpec::SdTrainer(SdTrainerInstallSpec {
        branch: branch(value),
        ..Default::default()
    })
}

#[test]
fn stable_diffusion_branch_table_and_fallback_are_exact() {
    let cases = [
        ("sd_webui_forge_neo", SD_WEBUI_FORGE_CLASSIC),
        ("sd_webui_forge_classic", SD_WEBUI_FORGE_CLASSIC),
        ("sd_webui_main", SD_WEBUI_MAIN),
        ("sd_webui_dev", SD_WEBUI_MAIN),
        ("sd_webui_forge", SD_WEBUI_FORGE),
        ("sd_webui_reforge_main", SD_WEBUI_FORGE),
        ("sd_webui_reforge_dev", SD_WEBUI_FORGE),
        ("sd_webui_amdgpu", SD_WEBUI_AMDGPU),
        ("sd_next_main", SD_NEXT),
        ("sd_next_dev", SD_NEXT),
        ("unknown", SD_WEBUI_FORGE_CLASSIC),
    ];
    for (preset, expected) in cases {
        assert_eq!(default_launch_arguments(&stable(preset)), owned(expected));
    }
    assert_eq!(
        default_launch_arguments(&WebUiInstallSpec::default_for(
            WebUiFamily::StableDiffusionWebui
        )),
        owned(SD_WEBUI_FORGE_CLASSIC)
    );
}

#[test]
fn fixed_family_defaults_and_empty_families_are_exact() {
    assert_eq!(
        default_launch_arguments(&WebUiInstallSpec::Comfyui(ComfyuiInstallSpec::default())),
        owned(&[
            "--auto-launch",
            "--preview-method",
            "auto",
            "--disable-cuda-malloc",
            "--enable-manager",
        ])
    );
    assert_eq!(
        default_launch_arguments(&WebUiInstallSpec::default_for(WebUiFamily::QwenTts)),
        owned(&["--api"])
    );
    for family in [
        WebUiFamily::Invokeai,
        WebUiFamily::RvcNext,
        WebUiFamily::MizuotoTts,
    ] {
        assert!(default_launch_arguments(&WebUiInstallSpec::default_for(family)).is_empty());
    }
}

#[test]
fn fooocus_branch_table_and_fallback_are_exact() {
    for preset in [
        "fooocus_main",
        "ruined_fooocus_main",
        "fooocus_mre_main",
        "unknown",
    ] {
        let spec = WebUiInstallSpec::Fooocus(FooocusInstallSpec {
            branch: branch(preset),
            ..Default::default()
        });
        assert_eq!(default_launch_arguments(&spec), owned(FOOOCUS));
    }
}

#[test]
fn sd_trainer_branch_table_and_fallback_are_exact() {
    for (preset, expected) in [
        ("sd_trainer_main", SD_TRAINER),
        ("sd_trainer_next_main", SD_TRAINER),
        ("kohya_gui_main", KOHYA_GUI),
        ("unknown", SD_TRAINER),
    ] {
        assert_eq!(default_launch_arguments(&trainer(preset)), owned(expected));
    }
}

#[test]
fn every_builtin_token_satisfies_instance_config_bounds() {
    let specs = [
        stable("sd_webui_main"),
        stable("sd_webui_amdgpu"),
        stable("sd_next_main"),
        trainer("sd_trainer_main"),
        trainer("kohya_gui_main"),
    ];
    for arguments in specs.iter().map(default_launch_arguments) {
        assert!(arguments.len() <= 256);
        assert!(arguments.iter().all(|argument| !argument.is_empty()
            && argument.len() <= 4096
            && !argument.contains('\0')));
    }
}
