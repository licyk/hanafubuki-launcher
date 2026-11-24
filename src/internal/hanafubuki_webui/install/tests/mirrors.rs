use super::*;

#[test]
fn manual_mirror_settings_emit_only_explicit_choices() {
    let settings = ResolvedInstallSettings {
        automatic_mirror: Some(false),
        pypi_mirror: Some(PypiMirrorMode::Disabled),
        github_mirror: Some(GithubMirrorSetting {
            enabled: Some(false),
            custom: None,
        }),
        model_download_source: Some(ModelDownloadSource::Modelscope),
        use_uv: Some(false),
    };
    let plan = build(
        fooocus_spec(),
        &settings,
        &WebUiInstallOptionCatalog::default(),
    )
    .unwrap();
    assert!(plan.cli_args().ends_with(&strings(&[
        "--no-pypi-mirror",
        "--no-uv",
        "--no-github-mirror",
        "--model-resource",
        "modelscope",
        "--no-auto-mirror",
    ])));
}

#[test]
fn speech_families_never_receive_a_model_source_their_installers_lack() {
    let settings = ResolvedInstallSettings {
        automatic_mirror: Some(false),
        pypi_mirror: Some(PypiMirrorMode::Disabled),
        github_mirror: None,
        model_download_source: Some(ModelDownloadSource::Huggingface),
        use_uv: None,
    };
    let cases = [
        (
            WebUiInstallSpec::RvcNext(RvcNextInstallSpec {
                pytorch: Some(pytorch("cu121")),
                python_version: None,
            }),
            "rvc-next-webui",
            "--rvc-next-webui-path",
        ),
        (
            WebUiInstallSpec::MizuotoTts(MizuotoTtsInstallSpec {
                pytorch: Some(pytorch("cu121")),
                python_version: None,
            }),
            "mizuoto-tts-webui",
            "--mizuoto-tts-webui-path",
        ),
    ];
    for (spec, command, path_flag) in cases {
        let plan = build(spec, &settings, &populated_catalog()).unwrap();
        assert_eq!(
            plan.cli_args(),
            [
                command.to_string(),
                "install".to_string(),
                path_flag.to_string(),
                path_arg(),
                "--pytorch-mirror-type".to_string(),
                "cu121".to_string(),
                "--no-pypi-mirror".to_string(),
                "--no-auto-mirror".to_string(),
            ]
        );
    }
}

#[test]
fn pypi_mirror_omission_and_builtin_mode_emit_no_substitute_argument() {
    for pypi_mirror in [None, Some(PypiMirrorMode::BuiltIn)] {
        let settings = ResolvedInstallSettings {
            automatic_mirror: Some(false),
            pypi_mirror,
            ..ResolvedInstallSettings::default()
        };
        let plan = build(
            comfyui_spec(),
            &settings,
            &WebUiInstallOptionCatalog::default(),
        )
        .unwrap();
        assert!(
            !plan
                .cli_args()
                .iter()
                .any(|argument| argument.contains("pypi"))
        );
        assert!(plan.cli_args().ends_with(&strings(&["--no-auto-mirror"])));
    }
}

#[test]
fn automatic_mirror_forwards_manual_mirrors_and_preserves_uv() {
    for automatic_mirror in [None, Some(true)] {
        let settings = ResolvedInstallSettings {
            automatic_mirror,
            pypi_mirror: Some(PypiMirrorMode::Disabled),
            github_mirror: Some(GithubMirrorSetting {
                enabled: Some(false),
                custom: None,
            }),
            model_download_source: Some(ModelDownloadSource::Huggingface),
            use_uv: Some(false),
        };
        let plan = build(
            invokeai_spec(),
            &settings,
            &WebUiInstallOptionCatalog::default(),
        )
        .unwrap();
        assert!(plan.cli_args().contains(&"--no-uv".into()));
        assert!(plan.cli_args().contains(&"--no-pypi-mirror".into()));
        assert!(plan.cli_args().contains(&"--no-github-mirror".into()));
        assert!(
            plan.cli_args()
                .windows(2)
                .any(|pair| pair == ["--model-resource", "huggingface"])
        );
        assert!(!plan.cli_args().contains(&"--no-auto-mirror".into()));
    }
}

#[test]
fn github_mirror_enablement_and_custom_url_are_independent() {
    for automatic_mirror in [Some(false), Some(true)] {
        for (enabled, custom, expects_disabled, expects_custom) in [
            (true, None, false, false),
            (true, Some("https://mirror.example"), false, true),
            (false, None, true, false),
            (false, Some("https://mirror.example"), true, true),
        ] {
            let settings = ResolvedInstallSettings {
                automatic_mirror,
                github_mirror: Some(GithubMirrorSetting {
                    enabled: Some(enabled),
                    custom: custom.map(str::to_string),
                }),
                ..ResolvedInstallSettings::default()
            };
            let plan = build(
                stable_spec(),
                &settings,
                &WebUiInstallOptionCatalog::default(),
            )
            .unwrap();
            let args = plan.cli_args();
            assert_eq!(
                args.contains(&"--no-github-mirror".into()),
                expects_disabled
            );
            assert_eq!(
                args.windows(2)
                    .any(|pair| pair == ["--custom-github-mirror", "https://mirror.example"]),
                expects_custom
            );
        }
    }
}

#[test]
fn empty_custom_github_mirror_value_is_rejected() {
    let empty = ResolvedInstallSettings {
        github_mirror: Some(GithubMirrorSetting {
            enabled: None,
            custom: Some(" ".to_string()),
        }),
        ..ResolvedInstallSettings::default()
    };
    assert!(matches!(
        build(stable_spec(), &empty, &WebUiInstallOptionCatalog::default(),),
        Err(WebUiInstallBuildError::InvalidGlobalSetting { .. })
    ));
}
