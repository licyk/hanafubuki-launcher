use super::*;

#[test]
fn explicit_optional_values_build_exact_ordered_argv() {
    let spec = WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
        pytorch: Some(pytorch("cu121")),
        python_version: Some(PythonVersion::Python313),
        branch: Some(branch("sd_webui_forge")),
        predownload_models: Some(false),
        predownload_extensions: Some(false),
    });
    let settings = ResolvedInstallSettings {
        automatic_mirror: Some(false),
        pypi_mirror: Some(PypiMirrorMode::Disabled),
        github_mirror: Some(GithubMirrorSetting {
            enabled: None,
            custom: Some("https://github.example/custom".to_string()),
        }),
        model_download_source: Some(ModelDownloadSource::Huggingface),
        use_uv: Some(false),
    };
    let plan = build(spec, &settings, &populated_catalog()).unwrap();
    let mut expected = strings(&["sd-webui", "install", "--sd-webui-path"]);
    expected.push(path_arg());
    expected.extend(strings(&[
        "--pytorch-mirror-type",
        "cu121",
        "--no-pypi-mirror",
        "--no-uv",
        "--custom-github-mirror",
        "https://github.example/custom",
        "--install-branch",
        "sd_webui_forge",
        "--no-pre-download-extension",
        "--no-pre-download-model",
        "--model-resource",
        "huggingface",
        "--no-auto-mirror",
    ]));
    assert_eq!(plan.cli_args(), expected);
    assert_eq!(plan.python_version(), PythonVersion::Python313);
}

#[test]
fn explicit_true_predownload_and_enabled_settings_emit_no_negative_flags() {
    let spec = WebUiInstallSpec::Comfyui(ComfyuiInstallSpec {
        pytorch: None,
        python_version: None,
        predownload_models: Some(true),
        predownload_extensions: Some(true),
    });
    let settings = ResolvedInstallSettings {
        automatic_mirror: Some(true),
        pypi_mirror: Some(PypiMirrorMode::BuiltIn),
        github_mirror: Some(GithubMirrorSetting {
            enabled: Some(true),
            custom: None,
        }),
        model_download_source: None,
        use_uv: Some(true),
    };
    let plan = build(spec, &settings, &WebUiInstallOptionCatalog::default()).unwrap();
    assert_eq!(plan.cli_args().len(), 4);
}

#[test]
fn explicit_false_predownload_emits_only_family_supported_negative_flags() {
    let stable = WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
        pytorch: None,
        python_version: None,
        branch: None,
        predownload_models: Some(false),
        predownload_extensions: Some(false),
    });
    let qwen = WebUiInstallSpec::QwenTts(QwenTtsInstallSpec {
        pytorch: None,
        python_version: None,
        predownload_models: Some(false),
    });
    let stable_args = build(
        stable,
        &ResolvedInstallSettings::default(),
        &WebUiInstallOptionCatalog::default(),
    )
    .unwrap();
    assert!(stable_args.cli_args().ends_with(&strings(&[
        "--no-pre-download-extension",
        "--no-pre-download-model"
    ])));
    let qwen_args = build(
        qwen,
        &ResolvedInstallSettings::default(),
        &WebUiInstallOptionCatalog::default(),
    )
    .unwrap();
    assert!(
        qwen_args
            .cli_args()
            .ends_with(&strings(&["--no-pre-download-model"]))
    );
    assert!(
        !qwen_args
            .cli_args()
            .iter()
            .any(|arg| arg.contains("extension"))
    );
}
