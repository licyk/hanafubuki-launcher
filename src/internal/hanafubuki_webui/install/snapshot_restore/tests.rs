use super::*;
use crate::internal::hanafubuki_webui::install::{GithubMirrorSetting, PypiMirrorMode};

#[test]
fn restore_argv_matches_external_cli_contract_without_install_only_flags() {
    let snapshot_path = std::env::temp_dir()
        .join("snapshots")
        .join("environment.json");
    let core_path = std::env::temp_dir()
        .join("instances")
        .join("demo")
        .join("core");
    let settings = ResolvedInstallSettings {
        automatic_mirror: Some(false),
        pypi_mirror: Some(PypiMirrorMode::Disabled),
        github_mirror: Some(GithubMirrorSetting {
            enabled: Some(false),
            custom: Some("https://github.example".into()),
        }),
        model_download_source: None,
        use_uv: Some(false),
    };
    let plan = build_webui_snapshot_restore_plan(
        WebUiFamily::Comfyui,
        PythonVersion::Python311,
        snapshot_path.clone(),
        core_path.clone(),
        &settings,
    )
    .unwrap();
    assert_eq!(&plan.cli_args()[..2], ["comfyui", "restore"]);
    assert_eq!(plan.cli_args()[2], snapshot_path.to_string_lossy());
    assert_eq!(plan.cli_args()[3], "--comfyui-path");
    assert_eq!(plan.cli_args()[4], core_path.to_string_lossy());
    for required in [
        "--prune-packages",
        "--prune-extensions",
        "--force-git-reset",
        "--no-uv",
        "--no-pypi-mirror",
        "--no-github-mirror",
        "--no-auto-mirror",
    ] {
        assert!(plan.cli_args().iter().any(|arg| arg == required));
    }
    assert!(!plan.cli_args().iter().any(|arg| arg == "--model-resource"));
}
