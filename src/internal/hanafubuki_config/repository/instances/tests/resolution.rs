use super::*;

#[test]
fn resolve_layers_instance_over_global() {
    let paths = test_paths("resolve");
    let mut global = AppSettings::defaults(&paths);
    global.enable_uv = true;
    global.use_pypi_mirror = true;

    let mut instance = sample_instance();
    instance.settings_overrides = json!({
        "enableUv": false,
        "usePypiMirror": false
    });

    let settings = resolve_effective_settings(&paths, &global, &instance).unwrap();
    assert!(!settings.enable_uv, "instance override should win");
    assert!(!settings.use_pypi_mirror);
}

#[test]
fn null_custom_mirror_overrides_resolve_to_none() {
    let paths = test_paths("null-mirror");
    let mut global = AppSettings::defaults(&paths);
    global.custom_github_mirror = Some("https://gh.example".to_string());
    global.custom_huggingface_mirror = Some("https://hf.example".to_string());
    let mut instance = sample_instance();
    instance.settings_overrides =
        json!({ "customGithubMirror": null, "customHuggingfaceMirror": null });

    let settings = resolve_effective_settings(&paths, &global, &instance).unwrap();
    assert_eq!(settings.custom_github_mirror, None);
    assert_eq!(settings.custom_huggingface_mirror, None);
}

#[test]
fn instance_proxy_policy_overrides_are_rejected() {
    let paths = test_paths("reject-proxy-policy-override");
    for field in ["proxy", "proxyMode", "proxyAddress"] {
        let error = apply_instance_patch(
            &paths,
            sample_instance(),
            "demo",
            json!({ "settingsOverrides": { (field): null } }),
        )
        .unwrap_err();
        assert!(error.contains("global-only") || error.contains("unsupported"));
    }
}
