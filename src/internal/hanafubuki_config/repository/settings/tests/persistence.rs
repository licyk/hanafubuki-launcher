use super::*;

use crate::internal::hanafubuki_config::model::settings_domain::SettingsDomain;

#[test]
fn load_settings_writes_defaults_without_materializing_their_directories() {
    let paths = test_paths("defaults");

    let settings = load_settings(&paths).unwrap();

    assert_eq!(settings.language, "auto");
    assert_eq!(settings.theme, ThemePreference::System);
    assert_eq!(settings.proxy_mode, ProxyMode::System);
    assert_eq!(settings.proxy_address, None);
    assert_eq!(settings.app_python_version, PythonVersion::Python315);
    assert_eq!(settings.git_runtime_source, GitRuntimeSource::System);
    let value = serde_json::to_value(&settings).unwrap();
    assert_eq!(value["proxyMode"], "system");
    assert_eq!(value["proxyAddress"], Value::Null);
    assert!(value.get("proxy").is_none());
    // Every domain gets its own document, so no single file can take the whole
    // configuration down with it.
    for &domain in SettingsDomain::ALL {
        assert!(
            paths.settings_file(domain).is_file(),
            "{} was not created",
            domain.file_name()
        );
    }
    assert!(!settings.artifact_cache_dir.exists());
    assert!(!settings.default_webui_install_dir.exists());
    assert!(!settings.webview_download_dir.exists());

    prepare_settings_directories(&settings).unwrap();

    assert!(settings.artifact_cache_dir.is_dir());
    assert!(settings.default_webui_install_dir.is_dir());
    assert!(settings.webview_download_dir.is_dir());

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn loading_persisted_paths_does_not_recreate_their_directories() {
    let paths = test_paths("load-paths-without-materialization");
    let configured_root = paths.app_data_dir.with_extension("configured-paths");
    let cache = configured_root.join("cache");
    let instances = configured_root.join("instances");
    let downloads = configured_root.join("downloads");
    let state = ConfigState::default();
    let settings = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({
            "artifactCacheDir": cache,
            "defaultWebuiInstallDir": instances,
            "webviewDownloadDir": downloads,
        }),
    )
    .unwrap();
    assert!(configured_root.is_dir());
    std::fs::remove_dir_all(&configured_root).unwrap();

    let loaded = load_settings(&paths).unwrap();

    assert_eq!(loaded.artifact_cache_dir, settings.artifact_cache_dir);
    assert_eq!(
        loaded.default_webui_install_dir,
        settings.default_webui_install_dir
    );
    assert_eq!(loaded.webview_download_dir, settings.webview_download_dir);
    assert!(!configured_root.exists());

    prepare_settings_directories(&loaded).unwrap();
    assert!(loaded.artifact_cache_dir.is_dir());
    assert!(loaded.default_webui_install_dir.is_dir());
    assert!(loaded.webview_download_dir.is_dir());

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
    let _ = std::fs::remove_dir_all(configured_root);
}

/// A directory setting is checked where it enters, so a NUL (which the cache
/// path would carry into every child environment) is refused on write and
/// falls back to the default on read.
#[test]
fn a_directory_setting_with_nul_is_refused_on_write_and_replaced_on_read() {
    let paths = test_paths("nul-directory");
    let defaults = AppSettings::defaults(&paths);
    let with_nul = format!("{}\0cache", paths.app_data_dir.display());

    let mut settings = defaults.clone();
    for key in [
        "artifactCacheDir",
        "defaultWebuiInstallDir",
        "webviewDownloadDir",
    ] {
        let error = apply_setting_patch(&mut settings, key, json!(with_nul)).unwrap_err();
        assert!(error.contains("NUL"), "{error}");
    }
    assert_eq!(settings.artifact_cache_dir, defaults.artifact_cache_dir);

    safe_file::write_json_atomic(
        &paths.settings_file(SettingsDomain::Paths),
        &json!({
            "artifactCacheDir": with_nul,
            "defaultWebuiInstallDir": defaults.default_webui_install_dir,
            "webviewDownloadDir": defaults.webview_download_dir,
        }),
    )
    .unwrap();
    let loaded = load_settings(&paths).unwrap();
    assert_eq!(loaded.artifact_cache_dir, defaults.artifact_cache_dir);

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn persisted_app_python_version_outlives_a_new_default() {
    // Every load writes the runtime document in full, so an installation from
    // before the default changed already records the version it runs on.
    let paths = test_paths("app-python-persisted");
    let runtime = json!({"appPythonVersion": "3.11", "gitRuntimeSource": "system"});
    safe_file::write_json_atomic(&paths.settings_file(SettingsDomain::Runtime), &runtime).unwrap();

    let settings = load_settings(&paths).unwrap();

    assert_eq!(settings.app_python_version, PythonVersion::Python311);
    let raw: Value = safe_file::read_json(&paths.settings_file(SettingsDomain::Runtime))
        .unwrap()
        .unwrap();
    assert_eq!(raw, runtime);

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn each_document_holds_only_its_own_domain() {
    let paths = test_paths("documents");

    load_settings(&paths).unwrap();

    let network: Value = safe_file::read_json(&paths.settings_file(SettingsDomain::Network))
        .unwrap()
        .unwrap();
    assert!(network.get("proxyMode").is_some());
    assert!(
        network.get("autoMirror").is_none(),
        "mirror settings belong to their own document"
    );

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn old_proxy_key_is_removed_during_development_file_normalization() {
    let paths = test_paths("proxy-normalize");
    safe_file::write_json_atomic(
        &paths.settings_file(SettingsDomain::Network),
        &json!({"proxy": "http://legacy:8080"}),
    )
    .unwrap();
    let settings = load_settings(&paths).unwrap();
    assert_eq!(settings.proxy_mode, ProxyMode::System);
    assert_eq!(settings.proxy_address, None);
    let raw: Value = safe_file::read_json(&paths.settings_file(SettingsDomain::Network))
        .unwrap()
        .unwrap();
    assert!(raw.get("proxy").is_none());
    assert_eq!(raw["proxyMode"], "system");
    assert_eq!(raw["proxyAddress"], Value::Null);
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn a_write_touches_only_the_documents_whose_domain_changed() {
    let paths = test_paths("selective-write");
    let state = ConfigState::default();
    load_settings(&paths).unwrap();

    let untouched = paths.settings_file(SettingsDomain::Mirrors);
    let before = std::fs::metadata(&untouched).unwrap().modified().unwrap();
    // The backup sibling is only written when the file itself is rewritten, so
    // its absence is the durable proof the document was left alone.
    let backup = untouched.with_extension("json.bak");
    assert!(!backup.exists());

    update_global_setting_inner(&paths, &state, "language".to_string(), json!("en")).unwrap();

    assert_eq!(
        std::fs::metadata(&untouched).unwrap().modified().unwrap(),
        before
    );
    assert!(
        !backup.exists(),
        "an unrelated domain must not be rewritten by a language change"
    );
    let language: Value = safe_file::read_json(&paths.settings_file(SettingsDomain::Language))
        .unwrap()
        .unwrap();
    assert_eq!(language["language"], "en");

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn apply_setting_patch_accepts_whitelisted_field() {
    let paths = test_paths("patch-ok");
    let mut settings = AppSettings::defaults(&paths);

    apply_setting_patch(&mut settings, "theme", json!("dark")).unwrap();
    apply_setting_patch(&mut settings, "language", json!("auto")).unwrap();

    assert_eq!(settings.theme, ThemePreference::Dark);
    assert_eq!(settings.language, "auto");

    for invalid in [json!("fr"), json!(true), Value::Null] {
        assert!(apply_setting_patch(&mut settings, "language", invalid).is_err());
        assert_eq!(settings.language, "auto");
    }

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn apply_setting_patch_rejects_unknown_field() {
    let paths = test_paths("patch-reject");
    let mut settings = AppSettings::defaults(&paths);

    for key in ["unknown", "cacheDir", "defaultInstallDir"] {
        let err = apply_setting_patch(&mut settings, key, json!("value")).unwrap_err();
        assert!(
            err.contains("unsupported settings key"),
            "{key} unexpectedly reached a compatibility path: {err}"
        );
    }

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
