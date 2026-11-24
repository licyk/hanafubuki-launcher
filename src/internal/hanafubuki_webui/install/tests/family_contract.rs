use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use serde_json::Value;

use crate::internal::hanafubuki_webui::instance_types::InstanceTypeRegistry;

use super::*;

#[test]
fn every_family_declares_exactly_its_spec_fields() {
    let expected: [(WebUiFamily, &[&str]); 8] = [
        (
            WebUiFamily::StableDiffusionWebui,
            &[
                "pytorch",
                "pythonVersion",
                "branch",
                "predownloadModels",
                "predownloadExtensions",
            ],
        ),
        (
            WebUiFamily::Comfyui,
            &[
                "pytorch",
                "pythonVersion",
                "predownloadModels",
                "predownloadExtensions",
            ],
        ),
        (
            WebUiFamily::Invokeai,
            &["pytorch", "pythonVersion", "predownloadModels"],
        ),
        (
            WebUiFamily::Fooocus,
            &["pytorch", "pythonVersion", "branch", "predownloadModels"],
        ),
        (
            WebUiFamily::SdTrainer,
            &["pytorch", "pythonVersion", "branch", "predownloadModels"],
        ),
        (
            WebUiFamily::QwenTts,
            &["pytorch", "pythonVersion", "predownloadModels"],
        ),
        (WebUiFamily::RvcNext, &["pytorch", "pythonVersion"]),
        (WebUiFamily::MizuotoTts, &["pytorch", "pythonVersion"]),
    ];
    for (family, fields) in expected {
        assert_eq!(
            WebUiInstallSpec::accepted_fields(family),
            fields,
            "{family:?}"
        );
        assert_eq!(
            WebUiInstallSpec::takes_branch(family),
            fields.contains(&"branch")
        );
    }
}

#[test]
fn only_invokeai_reads_pytorch_options_from_device_categories() {
    for family in WebUiFamily::ALL {
        let expected = if family == WebUiFamily::Invokeai {
            PytorchOptionSource::DeviceCategories
        } else {
            PytorchOptionSource::SharedBuilds
        };
        assert_eq!(WebUiInstallSpec::pytorch_option_source(family), expected);
    }
}

#[test]
fn default_and_snapshot_specs_belong_to_the_requested_family() {
    for family in WebUiFamily::ALL {
        let default = WebUiInstallSpec::default_for(family);
        assert_eq!(default.family(), family);
        assert_eq!(default.python_version(), None);
        let snapshot = WebUiInstallSpec::snapshot_install(family, PythonVersion::Python313);
        assert_eq!(snapshot.family(), family);
        assert_eq!(snapshot.python_version(), Some(PythonVersion::Python313));
        assert_eq!(snapshot.pytorch_selection_name(), None);
        assert_eq!(snapshot.branch_name(), None);
    }
}

#[test]
fn persisted_specs_keep_every_declared_field_and_drop_the_rest() {
    for family in WebUiFamily::ALL {
        let value = json!({
            "pytorch": if family == WebUiFamily::Invokeai { "cuda" } else { "cu128" },
            "pythonVersion": "3.12",
            "branch": "sd_webui_main",
            "predownloadModels": false,
            "predownloadExtensions": false,
            "pytorchFamily": "cuda",
            "legacyOption": true,
        });
        let spec =
            WebUiInstallSpec::from_persisted_object(family, value.as_object().unwrap().clone())
                .unwrap();
        let mut keys: Vec<String> = spec.to_object().keys().cloned().collect();
        keys.sort_unstable();
        let mut expected = WebUiInstallSpec::accepted_fields(family).to_vec();
        expected.sort_unstable();
        assert_eq!(keys, expected, "{family:?}");
        assert_eq!(spec.family(), family);
        assert_eq!(spec.python_version(), Some(PythonVersion::Python312));
    }
}

#[test]
fn selections_build_the_family_spec_and_reject_undeclared_options() {
    let stable = WebUiInstallSpec::from_selection(
        WebUiFamily::StableDiffusionWebui,
        WebUiInstallSelection {
            pytorch: Some("cu128".to_string()),
            python_version: Some("3.11".to_string()),
            branch: Some("sd_webui_forge".to_string()),
            predownload_models: Some(false),
            predownload_extensions: Some(true),
        },
    )
    .unwrap();
    assert_eq!(stable.pytorch_selection_name().as_deref(), Some("cu128"));
    assert_eq!(stable.python_version(), Some(PythonVersion::Python311));
    assert_eq!(stable.branch_name().as_deref(), Some("sd_webui_forge"));

    let invoke = WebUiInstallSpec::from_selection(
        WebUiFamily::Invokeai,
        WebUiInstallSelection {
            pytorch: Some("cuda".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(invoke.pytorch_selection_name().as_deref(), Some("cuda"));
    assert!(
        WebUiInstallSpec::from_selection(
            WebUiFamily::Invokeai,
            WebUiInstallSelection {
                pytorch: Some("cu128".to_string()),
                ..Default::default()
            },
        )
        .is_err()
    );

    for (family, selection) in [
        (
            WebUiFamily::Comfyui,
            WebUiInstallSelection {
                branch: Some("main".to_string()),
                ..Default::default()
            },
        ),
        (
            WebUiFamily::QwenTts,
            WebUiInstallSelection {
                predownload_extensions: Some(false),
                ..Default::default()
            },
        ),
        (
            WebUiFamily::RvcNext,
            WebUiInstallSelection {
                predownload_models: Some(false),
                ..Default::default()
            },
        ),
        (
            WebUiFamily::MizuotoTts,
            WebUiInstallSelection {
                predownload_models: Some(false),
                ..Default::default()
            },
        ),
    ] {
        let error = WebUiInstallSpec::from_selection(family, selection).unwrap_err();
        assert!(error.contains("unknown field"), "{family:?}: {error}");
    }
}

#[test]
fn each_built_in_form_holds_exactly_the_fields_its_spec_accepts() {
    for family in WebUiFamily::ALL {
        let form = builtin_install_form(family);
        let mut keys: Vec<&str> = form
            .iter()
            .flat_map(|group| group.fields.iter().map(|field| field.key.as_str()))
            .collect();
        keys.sort_unstable();
        let mut accepted = WebUiInstallSpec::accepted_fields(family).to_vec();
        accepted.sort_unstable();
        assert_eq!(keys, accepted, "{family:?}");
        for field in form.iter().flat_map(|group| &group.fields) {
            assert!(
                field.nullable && field.unset_label.is_some(),
                "{family:?} {}: every built-in field leaves the family default selectable",
                field.key
            );
        }
    }
}

#[test]
fn the_install_contract_serves_every_registered_types_form() {
    let registry = InstanceTypeRegistry::with_builtin_types();
    let contract = serde_json::to_value(WebUiInstallContract::of(&registry)).unwrap();
    assert_eq!(contract["types"].as_object().unwrap().len(), 8);
    let key = |key: &str| json!({ "kind": "key", "key": format!("instances.create.configuration.{key}") });
    let python_versions: Vec<Value> = PythonVersion::ALL
        .iter()
        .map(|version| {
            json!({ "value": version.as_str(), "label": { "kind": "literal", "text": { "en": version.as_str() } } })
        })
        .collect();
    assert_eq!(
        contract["types"]["builtin:invokeai"],
        json!([
            {
                "id": "pythonEnvironment",
                "label": { "kind": "key", "key": "instances.install.sections.pythonEnvironment" },
                "fields": [
                    {
                        "key": "pythonVersion", "kind": "select", "label": key("python"),
                        "options": python_versions, "nullable": true,
                        "unsetLabel": key("familyDefault"), "advanced": true
                    },
                    {
                        "key": "pytorch", "kind": "select", "label": key("deviceFamily"),
                        "optionsSource": "pytorch", "nullable": true,
                        "unsetLabel": key("automatic"), "advanced": true
                    }
                ]
            },
            {
                "id": "install",
                "label": { "kind": "key", "key": "instances.install.sections.install" },
                "fields": [
                    {
                        "key": "predownloadModels", "kind": "boolean", "label": key("predownloadModels"),
                        "nullable": true, "unsetLabel": key("installerDefault"), "advanced": true
                    }
                ]
            }
        ])
    );
    let stable = &contract["types"]["builtin:sd-webui"];
    assert_eq!(stable[0]["fields"][1]["label"], key("pytorchBuild"));
    assert_eq!(
        stable[1]["fields"][0],
        json!({
            "key": "branch", "kind": "select", "label": key("branch"),
            "optionsSource": "branches", "nullable": true, "unsetLabel": key("defaultBranch")
        }),
        "the branch is the one field a fresh install shows up front"
    );
}
