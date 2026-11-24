use super::*;

#[test]
fn independent_mirror_defaults_serialize_exactly() {
    let paths = test_paths("mirror-defaults");
    let settings = AppSettings::defaults(&paths);
    let value = serde_json::to_value(&settings).unwrap();
    assert_eq!(value["autoMirror"], true);
    assert_eq!(value["useMicromambaMirror"], true);
    assert_eq!(value["usePypiMirror"], true);
    assert_eq!(value["useGithubMirror"], true);
    assert_eq!(value["customGithubMirror"], Value::Null);
    assert_eq!(value["useHuggingfaceMirror"], true);
    assert_eq!(value["customHuggingfaceMirror"], Value::Null);
    assert_eq!(value["modelDownloadSource"], "modelscope");
    for removed in [
        "pypiMirror",
        "githubMirror",
        "huggingfaceMirror",
        "modelMirror",
    ] {
        assert!(value.get(removed).is_none());
        assert!(
            apply_setting_patch(&mut AppSettings::defaults(&paths), removed, Value::Null).is_err()
        );
    }
    let roundtrip: AppSettings = serde_json::from_value(value).unwrap();
    assert!(
        roundtrip.use_micromamba_mirror
            && roundtrip.use_pypi_mirror
            && roundtrip.use_github_mirror
            && roundtrip.use_huggingface_mirror
    );
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn mirror_schema_exposes_a_dedicated_micromamba_switch() {
    let schema = serde_json::to_value(get_global_settings_schema()).unwrap();
    let fields = schema["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|section| section["id"] == "mirrors")
        .unwrap()["fields"]
        .as_array()
        .unwrap();
    let micromamba = fields
        .iter()
        .find(|field| field["key"] == "useMicromambaMirror")
        .expect("micromamba mirror switch must be exposed");
    assert_eq!(
        micromamba["disabledWhen"],
        json!({"key": "autoMirror", "equals": true})
    );
    assert!(fields.iter().any(|field| field["key"] == "usePypiMirror"));
}

#[test]
fn custom_mirror_urls_are_validated_without_mutating_disabled_values() {
    let paths = test_paths("mirror-urls");
    let mut settings = AppSettings::defaults(&paths);
    apply_setting_patch(&mut settings, "useGithubMirror", json!(false)).unwrap();
    apply_setting_patch(
        &mut settings,
        "customGithubMirror",
        json!("https://github.example/base"),
    )
    .unwrap();
    assert!(!settings.use_github_mirror);
    assert_eq!(
        settings.custom_github_mirror.as_deref(),
        Some("https://github.example/base")
    );
    for invalid in [json!(" "), json!("ftp://example.com"), json!("not a url")] {
        let error =
            apply_setting_patch(&mut settings, "customHuggingfaceMirror", invalid).unwrap_err();
        assert!(error.contains("customHuggingfaceMirror"));
    }
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn model_download_source_is_closed() {
    let paths = test_paths("model-source");
    let mut settings = AppSettings::defaults(&paths);

    apply_setting_patch(&mut settings, "modelDownloadSource", json!("huggingface")).unwrap();
    assert_eq!(
        settings.model_download_source,
        ModelDownloadSource::Huggingface
    );
    let error =
        apply_setting_patch(&mut settings, "modelDownloadSource", json!("other")).unwrap_err();
    assert!(error.contains("modelDownloadSource"));

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
