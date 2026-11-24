use super::*;
use crate::internal::hanafubuki_config::SettingsDomain;

#[test]
fn the_system_scan_offer_starts_unanswered() {
    let paths = test_paths("onboarding-defaults");

    assert!(!AppSettings::defaults(&paths).system_webui_scan_prompted);
}

#[test]
fn answering_the_system_scan_offer_persists_in_its_own_document() {
    let paths = test_paths("onboarding-answer");
    let state = ConfigState::default();

    let updated = update_global_setting_inner(
        &paths,
        &state,
        "systemWebuiScanPrompted".to_string(),
        json!(true),
    )
    .unwrap();
    assert!(updated.system_webui_scan_prompted);

    // A fresh state proves the answer came back off disk rather than out of the
    // in-memory snapshot the write published.
    let reloaded = get_global_settings_inner(&paths, &ConfigState::default()).unwrap();
    assert!(reloaded.system_webui_scan_prompted);

    let document = paths.settings_file(SettingsDomain::Onboarding);
    assert!(document.is_file());
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&document).unwrap()).unwrap(),
        json!({"systemWebuiScanPrompted": true})
    );

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn the_answer_is_a_boolean_and_nothing_else() {
    let paths = test_paths("onboarding-invalid");
    let state = ConfigState::default();

    for invalid in [json!("true"), json!(1), Value::Null] {
        assert!(
            update_global_setting_inner(
                &paths,
                &state,
                "systemWebuiScanPrompted".to_string(),
                invalid,
            )
            .is_err()
        );
    }

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
