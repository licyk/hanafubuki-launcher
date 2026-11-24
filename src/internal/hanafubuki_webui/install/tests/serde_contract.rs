use super::*;

#[test]
fn serde_keeps_optional_fields_absent_and_rejects_unknown_cross_family_fields() {
    let stable: WebUiInstallSpec = serde_json::from_value(json!({
        "type": "stableDiffusionWebui"
    }))
    .unwrap();
    let WebUiInstallSpec::StableDiffusionWebui(stable) = stable else {
        panic!("wrong tagged variant");
    };
    assert_eq!(stable.pytorch, None);
    assert_eq!(stable.branch, None);
    assert_eq!(stable.predownload_models, None);
    assert_eq!(stable.predownload_extensions, None);

    assert!(
        serde_json::from_value::<WebUiInstallSpec>(json!({
            "type": "comfyui",
            "branch": "sd_webui_main"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<WebUiInstallSpec>(json!({
            "type": "qwenTts",
            "predownloadExtensions": false
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<WebUiInstallSpec>(json!({
            "type": "rvcNext",
            "predownloadModels": false
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<WebUiInstallSpec>(json!({
            "type": "mizuotoTts",
            "predownloadModels": false
        }))
        .is_err()
    );
}

#[test]
fn final_plans_never_contain_launcher_or_shell_arguments() {
    let specs = [
        stable_spec(),
        comfyui_spec(),
        invokeai_spec(),
        fooocus_spec(),
        sd_trainer_spec(),
        qwen_tts_spec(),
        rvc_next_spec(),
        mizuoto_tts_spec(),
    ];
    for spec in specs {
        let plan = build(
            spec,
            &ResolvedInstallSettings::default(),
            &WebUiInstallOptionCatalog::default(),
        )
        .unwrap();
        for forbidden in ["python", "python3", "-m", "sd_webui_all_in_one"] {
            assert!(!plan.cli_args().iter().any(|arg| arg == forbidden));
        }
        assert!(!plan.cli_args().iter().any(|arg| arg.starts_with('"')));
        assert!(!plan.cli_args().iter().any(|arg| arg.starts_with('\'')));
    }
}
