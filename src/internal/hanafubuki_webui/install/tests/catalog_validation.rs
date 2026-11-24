use super::*;

#[test]
fn present_pytorch_selections_require_the_correct_family_catalog() {
    let stable = WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
        pytorch: Some(pytorch("cu121")),
        python_version: None,
        branch: None,
        predownload_models: None,
        predownload_extensions: None,
    });
    let comfy = WebUiInstallSpec::Comfyui(ComfyuiInstallSpec {
        pytorch: Some(pytorch("cu121")),
        python_version: None,
        predownload_models: None,
        predownload_extensions: None,
    });
    let catalog = catalog_with(&[(WebUiFamily::StableDiffusionWebui, &["cu121"], &[])]);
    assert!(build(stable, &ResolvedInstallSettings::default(), &catalog).is_ok());
    assert!(matches!(
        build(comfy, &ResolvedInstallSettings::default(), &catalog),
        Err(WebUiInstallBuildError::PytorchSelectionNotInCatalog {
            webui: "ComfyUI",
            ..
        })
    ));
}

#[test]
fn every_present_selection_requires_catalog_validation() {
    let specs = [
        WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
            pytorch: Some(pytorch("cu121")),
            python_version: None,
            branch: None,
            predownload_models: None,
            predownload_extensions: None,
        }),
        WebUiInstallSpec::Comfyui(ComfyuiInstallSpec {
            pytorch: Some(pytorch("cu121")),
            python_version: None,
            predownload_models: None,
            predownload_extensions: None,
        }),
        WebUiInstallSpec::Fooocus(FooocusInstallSpec {
            pytorch: Some(pytorch("cu121")),
            python_version: None,
            branch: None,
            predownload_models: None,
        }),
        WebUiInstallSpec::SdTrainer(SdTrainerInstallSpec {
            pytorch: Some(pytorch("cu121")),
            python_version: None,
            branch: None,
            predownload_models: None,
        }),
        WebUiInstallSpec::QwenTts(QwenTtsInstallSpec {
            pytorch: Some(pytorch("cu121")),
            python_version: None,
            predownload_models: None,
        }),
        WebUiInstallSpec::RvcNext(RvcNextInstallSpec {
            pytorch: Some(pytorch("cu121")),
            python_version: None,
        }),
        WebUiInstallSpec::MizuotoTts(MizuotoTtsInstallSpec {
            pytorch: Some(pytorch("cu121")),
            python_version: None,
        }),
    ];
    for spec in specs {
        assert!(matches!(
            build(
                spec,
                &ResolvedInstallSettings::default(),
                &WebUiInstallOptionCatalog::default(),
            ),
            Err(WebUiInstallBuildError::PytorchSelectionNotInCatalog { .. })
        ));
    }

    let invoke = WebUiInstallSpec::Invokeai(InvokeaiInstallSpec {
        pytorch: Some(invoke_family("cuda")),
        python_version: None,
        predownload_models: None,
    });
    assert!(matches!(
        build(
            invoke,
            &ResolvedInstallSettings::default(),
            &WebUiInstallOptionCatalog::default(),
        ),
        Err(WebUiInstallBuildError::PytorchSelectionNotInCatalog { webui: "InvokeAI", selection })
            if selection == "cuda"
    ));
    assert!(matches!(
        InvokeAiPytorchFamily::try_from("cu121".to_string()),
        Err(WebUiInstallBuildError::InvalidPytorchSelectionGranularity { .. })
    ));
}

#[test]
fn branches_require_family_specific_supported_catalog_entries() {
    let stable = WebUiInstallSpec::StableDiffusionWebui(StableDiffusionWebuiInstallSpec {
        pytorch: None,
        python_version: None,
        branch: Some(branch("sd_webui_forge")),
        predownload_models: None,
        predownload_extensions: None,
    });
    let wrong_family_catalog = catalog_with(&[(WebUiFamily::Fooocus, &[], &["sd_webui_forge"])]);
    assert!(matches!(
        build(
            stable,
            &ResolvedInstallSettings::default(),
            &wrong_family_catalog,
        ),
        Err(WebUiInstallBuildError::BranchNotInCatalog {
            webui: "Stable Diffusion WebUI",
            ..
        })
    ));

    let invalid = WebUiInstallSpec::Fooocus(FooocusInstallSpec {
        pytorch: None,
        python_version: None,
        branch: Some(branch("sd_webui_forge")),
        predownload_models: None,
    });
    let catalog = catalog_with(&[(WebUiFamily::Fooocus, &[], &["sd_webui_forge"])]);
    assert!(matches!(
        build(invalid, &ResolvedInstallSettings::default(), &catalog),
        Err(WebUiInstallBuildError::InvalidBranch {
            webui: "Fooocus",
            ..
        })
    ));
}
