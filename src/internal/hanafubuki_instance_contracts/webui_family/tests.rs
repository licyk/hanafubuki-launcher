use super::WebUiFamily;

#[test]
fn api_namespaces_round_trip_for_every_family() {
    for family in WebUiFamily::ALL {
        assert_eq!(
            WebUiFamily::from_api_namespace(family.api_namespace()),
            Some(family)
        );
    }
    assert_eq!(WebUiFamily::from_api_namespace("sd-webui"), None);
}

#[test]
fn serde_uses_camel_case_family_ids() {
    let ids: Vec<String> = WebUiFamily::ALL
        .into_iter()
        .map(|family| {
            serde_json::to_value(family)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        ids,
        [
            "stableDiffusionWebui",
            "comfyui",
            "invokeai",
            "fooocus",
            "sdTrainer",
            "qwenTts",
            "rvcNext",
            "mizuotoTts"
        ]
    );
}

#[test]
fn every_family_names_its_api_and_cli_contract() {
    assert_eq!(
        WebUiFamily::ALL.map(|family| (
            family.api_path_parameter(),
            family.cli_command(),
            family.cli_path_flag()
        )),
        [
            ("sd_webui_path", "sd-webui", "--sd-webui-path"),
            ("comfyui_path", "comfyui", "--comfyui-path"),
            ("invokeai_path", "invokeai", "--invokeai-path"),
            ("fooocus_path", "fooocus", "--fooocus-path"),
            ("sd_trainer_path", "sd-trainer", "--sd-trainer-path"),
            (
                "qwen_tts_webui_path",
                "qwen-tts-webui",
                "--qwen-tts-webui-path"
            ),
            (
                "rvc_next_webui_path",
                "rvc-next-webui",
                "--rvc-next-webui-path"
            ),
            (
                "mizuoto_tts_webui_path",
                "mizuoto-tts-webui",
                "--mizuoto-tts-webui-path"
            ),
        ]
    );
    assert_eq!(
        WebUiFamily::Comfyui.api_method("launch.prepare"),
        "comfyui.launch.prepare"
    );
}
