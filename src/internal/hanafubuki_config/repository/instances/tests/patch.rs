use super::*;

#[test]
fn patch_sets_null_mirror_overrides() {
    let paths = test_paths("patch-null-mirror");
    let patched = apply_instance_patch(
        &paths,
        sample_instance(),
        "demo",
        json!({ "settingsOverrides": { "customGithubMirror": null, "customHuggingfaceMirror": null } }),
    )
    .unwrap();
    assert_eq!(
        patched.settings_overrides.get("customGithubMirror"),
        Some(&Value::Null)
    );
    assert_eq!(
        patched.settings_overrides.get("customHuggingfaceMirror"),
        Some(&Value::Null)
    );
}

#[test]
fn patch_merges_overrides_without_dropping_existing() {
    let paths = test_paths("patch-merge-override");
    let mut existing = sample_instance();
    existing.settings_overrides = json!({ "enableUv": false });
    let patched = apply_instance_patch(
        &paths,
        existing,
        "demo",
        json!({ "settingsOverrides": { "customGithubMirror": null } }),
    )
    .unwrap();
    // Existing override preserved, new nullable override added.
    assert_eq!(
        patched.settings_overrides.get("enableUv"),
        Some(&json!(false))
    );
    assert_eq!(
        patched.settings_overrides.get("customGithubMirror"),
        Some(&Value::Null)
    );
}

#[test]
fn patch_clears_optional_install_dir_with_null() {
    let paths = test_paths("patch-clear-install");
    let mut existing = sample_instance();
    existing.install_dir = Some(std::env::temp_dir().join("hanafubuki-instance"));
    let patched =
        apply_instance_patch(&paths, existing, "demo", json!({ "installDir": null })).unwrap();
    assert_eq!(patched.install_dir, None);
}

#[test]
fn patch_deletes_env_var_with_null() {
    let paths = test_paths("patch-del-env");
    let mut existing = sample_instance();
    existing
        .env_vars
        .insert("KEEP".to_string(), "1".to_string());
    existing
        .env_vars
        .insert("DROP".to_string(), "2".to_string());
    let patched = apply_instance_patch(
        &paths,
        existing,
        "demo",
        json!({ "envVars": { "DROP": null } }),
    )
    .unwrap();
    assert!(patched.env_vars.contains_key("KEEP"));
    assert!(!patched.env_vars.contains_key("DROP"));
}

/// The instance configuration is where an environment entry enters; the
/// processes it later reaches do not check it again.
#[test]
fn patch_rejects_unrepresentable_env_vars() {
    let paths = test_paths("patch-bad-env");
    for (name, value) in [("BAD=KEY", "v"), ("BAD\0KEY", "v"), ("KEY", "bad\0value")] {
        let error = apply_instance_patch(
            &paths,
            sample_instance(),
            "demo",
            json!({ "envVars": { name: value } }),
        )
        .unwrap_err();
        assert!(error.contains("invalid instance environment"), "{error}");
    }
}

#[test]
fn patch_rejects_name_in_favor_of_the_typed_operation() {
    let paths = test_paths("patch-reject-name");
    let error = apply_instance_patch(
        &paths,
        sample_instance(),
        "demo",
        json!({ "name": "Renamed" }),
    )
    .unwrap_err();
    assert!(error.contains("name"));
    assert!(error.contains("dedicated operations"));
}
