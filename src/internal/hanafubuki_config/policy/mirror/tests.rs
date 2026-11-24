use super::*;

fn settings() -> AppSettings {
    let root = std::env::temp_dir().join(format!("mirror-policy-{}", std::process::id()));
    let paths = AppPaths::from_app_data_dir(root).unwrap();
    AppSettings::defaults(&paths)
}

#[test]
fn automatic_keeps_complete_configured_values() {
    let mut settings = settings();
    settings.use_github_mirror = false;
    settings.use_micromamba_mirror = false;
    settings.custom_github_mirror = Some("https://github.example".into());
    let policy = ResolvedMirrorPolicy::from_settings(&settings);
    assert!(policy.automatic);
    assert_eq!(
        policy.custom_github.as_deref(),
        Some("https://github.example")
    );
    assert!(!policy.use_github);
    assert!(!policy.use_micromamba);
}

#[test]
fn enablement_and_custom_values_remain_independent() {
    let mut settings = settings();
    settings.auto_mirror = false;
    settings.use_github_mirror = false;
    settings.custom_github_mirror = Some("https://github.example".into());
    settings.use_huggingface_mirror = true;
    settings.custom_huggingface_mirror = Some("https://hf.example".into());
    settings.model_download_source = ModelDownloadSource::Huggingface;
    let policy = ResolvedMirrorPolicy::from_settings(&settings);
    assert!(!policy.use_github);
    assert_eq!(
        policy.custom_github.as_deref(),
        Some("https://github.example")
    );
    assert_eq!(
        policy.custom_huggingface.as_deref(),
        Some("https://hf.example")
    );
    assert_eq!(policy.model_source, ModelDownloadSource::Huggingface);
}
