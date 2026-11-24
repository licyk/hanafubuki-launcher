use super::*;
use crate::internal::hanafubuki_config::SettingsDomain;

#[test]
fn webview_page_state_and_shared_cache_are_both_on_by_default() {
    let paths = test_paths("webview-defaults");
    let defaults = AppSettings::defaults(&paths);

    // Preserving the page is what the user expects from leaving a tab and
    // coming back, and one shared store is what the application did before
    // instances could have one each.
    assert!(defaults.webview_preserve_page_state);
    assert!(defaults.webview_shared_cache);
    // The engine popup keeps popup-based sign-in working, and the notice is
    // how the user learns they can take the page to their browser instead.
    assert_eq!(
        defaults.webview_new_window,
        WebviewNewWindow::PopupWithNotice
    );
    // GPU painting is the fast path on most machines; CPU painting is the way
    // out for the drivers and virtual GPUs that freeze or blank the windows.
    assert!(defaults.webview_gpu_acceleration);
    let value = serde_json::to_value(&defaults).unwrap();
    assert_eq!(value["webviewPreservePageState"], json!(true));
    assert_eq!(value["webviewSharedCache"], json!(true));
    assert_eq!(value["webviewNewWindow"], json!("popup_with_notice"));
    assert_eq!(value["webviewGpuAcceleration"], json!(true));
}

#[test]
fn webview_settings_are_validated_and_persisted_in_their_own_document() {
    let paths = test_paths("webview-patch");
    let state = ConfigState::default();

    let updated = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({"webviewPreservePageState": false, "webviewSharedCache": false, "webviewNewWindow": "browser", "webviewGpuAcceleration": false}),
    )
    .unwrap();
    assert!(!updated.webview_gpu_acceleration);
    assert!(!updated.webview_preserve_page_state);
    assert!(!updated.webview_shared_cache);
    assert_eq!(updated.webview_new_window, WebviewNewWindow::Browser);

    for invalid in [json!("false"), json!(0), Value::Null] {
        assert!(
            update_global_settings_patch_inner(
                &paths,
                &state,
                json!({"webviewSharedCache": invalid})
            )
            .is_err()
        );
    }
    for invalid in [json!("true"), json!(1), Value::Null] {
        assert!(
            update_global_settings_patch_inner(
                &paths,
                &state,
                json!({"webviewGpuAcceleration": invalid})
            )
            .is_err()
        );
    }
    for invalid in [json!("tab"), json!(true), Value::Null] {
        assert!(
            update_global_settings_patch_inner(
                &paths,
                &state,
                json!({"webviewNewWindow": invalid})
            )
            .is_err()
        );
    }

    let reloaded = get_global_settings_inner(&paths, &ConfigState::default()).unwrap();
    assert!(!reloaded.webview_preserve_page_state);
    assert!(!reloaded.webview_shared_cache);
    assert_eq!(reloaded.webview_new_window, WebviewNewWindow::Browser);
    assert!(!reloaded.webview_gpu_acceleration);
    assert!(paths.settings_file(SettingsDomain::Webview).is_file());

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn the_webview_schema_section_exposes_the_toggles_and_the_new_window_choice() {
    let section = serde_json::to_value(get_global_settings_schema()).unwrap()["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|section| section["id"] == "webview")
        .cloned()
        .expect("the schema declares a WebView section");
    let keys: Vec<String> = section["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["key"].as_str().unwrap().to_string())
        .collect();
    let mut expected = vec![
        "webviewPreservePageState",
        "webviewSharedCache",
        "webviewNewWindow",
    ];
    // Only WebKitGTK reads it, so only Linux offers it.
    if cfg!(target_os = "linux") {
        expected.push("webviewGpuAcceleration");
    }
    assert_eq!(keys, expected);
    for field in section["fields"].as_array().unwrap() {
        assert_eq!(field["commit"], "change");
        if field["key"] == "webviewNewWindow" {
            assert_eq!(field["kind"], "select");
            let values: Vec<&str> = field["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| option["value"].as_str().unwrap())
                .collect();
            assert_eq!(values, vec!["popup_with_notice", "popup", "browser"]);
        } else {
            assert_eq!(field["kind"], "boolean");
        }
    }
}

#[test]
fn the_startup_read_sees_what_the_settings_load_persisted() {
    let paths = test_paths("webview-startup-read");
    let state = ConfigState::default();

    // Nothing persisted yet: the default, without creating anything.
    assert_eq!(
        read_startup_webview_gpu_acceleration(&paths.bootstrap),
        Ok(DEFAULT_WEBVIEW_GPU_ACCELERATION)
    );
    assert!(!paths.settings_file(SettingsDomain::Webview).exists());

    update_global_settings_patch_inner(&paths, &state, json!({"webviewGpuAcceleration": true}))
        .unwrap();
    assert_eq!(
        read_startup_webview_gpu_acceleration(&paths.bootstrap),
        Ok(true)
    );

    // A document from before the field existed is the default; the settings
    // load, not this read, adds the field.
    let file = paths.settings_file(SettingsDomain::Webview);
    std::fs::write(&file, r#"{"webviewSharedCache": false}"#).unwrap();
    assert_eq!(
        read_startup_webview_gpu_acceleration(&paths.bootstrap),
        Ok(DEFAULT_WEBVIEW_GPU_ACCELERATION)
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"webviewSharedCache": false}"#
    );

    std::fs::write(&file, r#"{"webviewGpuAcceleration": "yes"}"#).unwrap();
    assert!(read_startup_webview_gpu_acceleration(&paths.bootstrap).is_err());

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn the_startup_read_of_a_corrupt_document_writes_nothing() {
    let paths = test_paths("webview-startup-read-only");
    let state = ConfigState::default();
    update_global_settings_patch_inner(&paths, &state, json!({"webviewGpuAcceleration": false}))
        .unwrap();
    // A second write leaves the first document behind as the backup.
    update_global_settings_patch_inner(&paths, &state, json!({"webviewSharedCache": false}))
        .unwrap();
    let file = paths.settings_file(SettingsDomain::Webview);
    std::fs::write(&file, "{ torn").unwrap();
    let listing = |dir: &std::path::Path| {
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    let before = listing(&paths.settings_dir);

    // The backup answers, but nothing is restored or moved aside.
    assert_eq!(
        read_startup_webview_gpu_acceleration(&paths.bootstrap),
        Ok(false)
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ torn");
    assert_eq!(listing(&paths.settings_dir), before);

    std::fs::remove_file(file.with_extension("json.bak")).unwrap();
    assert!(read_startup_webview_gpu_acceleration(&paths.bootstrap).is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ torn");

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
