use super::*;

#[test]
fn path_only_requests_produce_exact_minimum_argv_for_every_family() {
    let path = path_arg();
    let cases = [
        (stable_spec(), "sd-webui", "--sd-webui-path"),
        (comfyui_spec(), "comfyui", "--comfyui-path"),
        (invokeai_spec(), "invokeai", "--invokeai-path"),
        (fooocus_spec(), "fooocus", "--fooocus-path"),
        (sd_trainer_spec(), "sd-trainer", "--sd-trainer-path"),
        (qwen_tts_spec(), "qwen-tts-webui", "--qwen-tts-webui-path"),
        (rvc_next_spec(), "rvc-next-webui", "--rvc-next-webui-path"),
        (
            mizuoto_tts_spec(),
            "mizuoto-tts-webui",
            "--mizuoto-tts-webui-path",
        ),
    ];
    for (spec, command, path_flag) in cases {
        let plan = build(
            spec,
            &ResolvedInstallSettings::default(),
            &WebUiInstallOptionCatalog::default(),
        )
        .unwrap();
        assert_eq!(
            plan.cli_args(),
            [
                command.to_string(),
                "install".to_string(),
                path_flag.to_string(),
                path.clone(),
            ]
        );
        assert_eq!(plan.target_path(), target_path());
    }
}

#[test]
fn path_only_requests_apply_only_rust_python_runtime_defaults() {
    let cases = [
        (stable_spec(), PythonVersion::Python311),
        (comfyui_spec(), PythonVersion::Python312),
        (invokeai_spec(), PythonVersion::Python312),
        (fooocus_spec(), PythonVersion::Python312),
        (sd_trainer_spec(), PythonVersion::Python311),
        (qwen_tts_spec(), PythonVersion::Python311),
        (rvc_next_spec(), PythonVersion::Python311),
        (mizuoto_tts_spec(), PythonVersion::Python311),
    ];
    for (spec, expected) in cases {
        let plan = build(
            spec,
            &ResolvedInstallSettings::default(),
            &WebUiInstallOptionCatalog::default(),
        )
        .unwrap();
        assert_eq!(plan.python_version(), expected);
        assert!(!plan.cli_args().iter().any(|arg| arg.contains("3.")));
    }
}

#[test]
fn missing_and_relative_target_paths_are_rejected_without_filesystem_access() {
    let empty = WebUiInstallRequest {
        target_path: PathBuf::new(),
        spec: stable_spec(),
    };
    assert_eq!(
        build_webui_install_plan(
            empty,
            &ResolvedInstallSettings::default(),
            &WebUiInstallOptionCatalog::default(),
        )
        .unwrap_err(),
        WebUiInstallBuildError::MissingTargetPath
    );

    let relative = WebUiInstallRequest {
        target_path: PathBuf::from("relative/webui"),
        spec: comfyui_spec(),
    };
    assert!(matches!(
        build_webui_install_plan(
            relative,
            &ResolvedInstallSettings::default(),
            &WebUiInstallOptionCatalog::default(),
        ),
        Err(WebUiInstallBuildError::NonAbsoluteTargetPath(_))
    ));
}

#[cfg(unix)]
#[test]
fn absolute_unix_path_with_spaces_and_non_ascii_is_one_unquoted_argument() {
    let target = PathBuf::from("/tmp/WebUI installs/模型 one");
    let plan = build_webui_install_plan(
        WebUiInstallRequest {
            target_path: target.clone(),
            spec: qwen_tts_spec(),
        },
        &ResolvedInstallSettings::default(),
        &WebUiInstallOptionCatalog::default(),
    )
    .unwrap();
    assert_eq!(plan.cli_args().len(), 4);
    assert_eq!(plan.cli_args()[3], target.to_str().unwrap());
    assert!(!plan.cli_args()[3].contains('"'));
    assert!(!plan.cli_args()[3].contains('\''));
}

#[cfg(windows)]
#[test]
fn absolute_windows_path_with_spaces_is_one_unquoted_argument() {
    let target = PathBuf::from(r"C:\WebUI installs\target one");
    let plan = build_webui_install_plan(
        WebUiInstallRequest {
            target_path: target.clone(),
            spec: stable_spec(),
        },
        &ResolvedInstallSettings::default(),
        &WebUiInstallOptionCatalog::default(),
    )
    .unwrap();
    assert_eq!(plan.cli_args().len(), 4);
    assert_eq!(plan.cli_args()[3], target.to_str().unwrap());
}
