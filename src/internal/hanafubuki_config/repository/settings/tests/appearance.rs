use super::*;

#[test]
fn appearance_defaults_serialize_exactly() {
    let paths = test_paths("appearance-defaults");
    let settings = AppSettings::defaults(&paths);
    let value = serde_json::to_value(&settings).unwrap();

    assert_eq!(value["theme"], "system");
    // The background is off by default: an unreleased default must not turn
    // surfaces translucent before the user has picked an image.
    assert_eq!(value["backgroundImageEnabled"], false);
    assert_eq!(value["backgroundSurfaceOpacity"], DEFAULT_SURFACE_OPACITY);
    assert_eq!(value["backgroundImageFit"], "cover");
    assert_eq!(value["backgroundImagePosition"], "center");
    // The asset itself is backend-owned metadata, never a settings field.
    for absent in [
        "backgroundImagePath",
        "backgroundImageRevision",
        "background",
    ] {
        assert!(value.get(absent).is_none());
    }

    let roundtrip: AppSettings = serde_json::from_value(value).unwrap();
    assert_eq!(roundtrip.theme, ThemePreference::System);
    assert_eq!(roundtrip.background_image_fit, BackgroundFit::Cover);
    assert_eq!(
        roundtrip.background_image_position,
        BackgroundPosition::Center
    );

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn appearance_enums_are_closed() {
    let paths = test_paths("appearance-enums");
    let mut settings = AppSettings::defaults(&paths);

    apply_setting_patch(&mut settings, "theme", json!("light")).unwrap();
    assert_eq!(settings.theme, ThemePreference::Light);
    apply_setting_patch(&mut settings, "backgroundImageFit", json!("contain")).unwrap();
    assert_eq!(settings.background_image_fit, BackgroundFit::Contain);
    apply_setting_patch(
        &mut settings,
        "backgroundImagePosition",
        json!("bottomRight"),
    )
    .unwrap();
    assert_eq!(
        settings.background_image_position,
        BackgroundPosition::BottomRight
    );

    for (key, value) in [
        ("theme", json!("solarized")),
        ("theme", json!(3)),
        ("backgroundImageFit", json!("tile")),
        ("backgroundImagePosition", json!("middle")),
        ("backgroundImageEnabled", json!("yes")),
    ] {
        assert!(
            apply_setting_patch(&mut settings, key, value.clone()).is_err(),
            "{key}={value} should have been rejected"
        );
    }

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn surface_opacity_is_bounded_so_the_interface_stays_readable() {
    let paths = test_paths("appearance-opacity");
    let mut settings = AppSettings::defaults(&paths);

    for value in [MIN_SURFACE_OPACITY, 70, MAX_SURFACE_OPACITY] {
        apply_setting_patch(&mut settings, "backgroundSurfaceOpacity", json!(value)).unwrap();
        assert_eq!(settings.background_surface_opacity, value);
    }

    // Below the floor the interface could become unreadable over an
    // arbitrary image, so it is rejected rather than silently clamped.
    for value in [
        json!(0),
        json!(MIN_SURFACE_OPACITY - 1),
        json!(101),
        json!(-5),
        json!("80"),
    ] {
        let error = apply_setting_patch(&mut settings, "backgroundSurfaceOpacity", value.clone())
            .unwrap_err();
        assert!(error.contains("backgroundSurfaceOpacity"), "{error}");
    }
    // The last valid value survives every rejection.
    assert_eq!(settings.background_surface_opacity, MAX_SURFACE_OPACITY);

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn blur_radius_is_bounded_and_zero_disables_the_effect() {
    let paths = test_paths("appearance-blur");
    let mut settings = AppSettings::defaults(&paths);

    for value in [MIN_BLUR_RADIUS, 8, MAX_BLUR_RADIUS] {
        apply_setting_patch(&mut settings, "backgroundBlurRadius", json!(value)).unwrap();
        assert_eq!(settings.background_blur_radius, value);
    }

    for value in [
        json!(u64::from(MAX_BLUR_RADIUS) + 1),
        json!(-1),
        json!("12"),
        json!(2.5),
    ] {
        let error =
            apply_setting_patch(&mut settings, "backgroundBlurRadius", value.clone()).unwrap_err();
        assert!(error.contains("backgroundBlurRadius"), "{error}");
    }
    // The last valid value survives every rejection.
    assert_eq!(settings.background_blur_radius, MAX_BLUR_RADIUS);

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn appearance_schema_matches_the_persisted_contract() {
    let schema = serde_json::to_value(get_global_settings_schema()).unwrap();
    let sections = schema["sections"].as_array().unwrap();
    let appearance = sections
        .iter()
        .find(|section| section["id"] == "appearance")
        .expect("appearance section must be exposed");

    let fields = appearance["fields"].as_array().unwrap();
    let keys: Vec<&str> = fields
        .iter()
        .map(|field| field["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "theme",
            "backgroundImageEnabled",
            "backgroundRandomEnabled",
            "backgroundSurfaceOpacity",
            "backgroundBlurRadius",
            "backgroundImageFit",
            "backgroundImagePosition",
        ]
    );

    // Every schema key must be a key the patch path actually accepts.
    let paths = test_paths("appearance-schema");
    for key in &keys {
        let mut scratch = AppSettings::defaults(&paths);
        assert!(
            apply_setting_patch(&mut scratch, key, Value::Null).is_err(),
            "{key} must validate its type"
        );
        assert!(
            !apply_setting_patch(&mut scratch, key, Value::Null)
                .unwrap_err()
                .contains("unsupported settings key"),
            "{key} is in the schema but not in the patch path"
        );
    }

    // Appearance is the only section that offers `theme`. Any other section
    // carrying it would mean two panels editing one field.
    for section in sections {
        if section["id"] == "appearance" {
            continue;
        }
        let section_keys: Vec<&str> = section["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| field["key"].as_str().unwrap())
            .collect();
        assert!(
            !section_keys.contains(&"theme"),
            "{} must not also offer theme",
            section["id"]
        );
    }

    // The slider bounds are Rust's, so the UI cannot offer a rejected value.
    let opacity = fields
        .iter()
        .find(|field| field["key"] == "backgroundSurfaceOpacity")
        .expect("the opacity field must be exposed");
    assert_eq!(opacity["min"], MIN_SURFACE_OPACITY);
    assert_eq!(opacity["max"], MAX_SURFACE_OPACITY);
    let blur = fields
        .iter()
        .find(|field| field["key"] == "backgroundBlurRadius")
        .expect("the blur field must be exposed");
    assert_eq!(blur["min"], MIN_BLUR_RADIUS);
    assert_eq!(blur["max"], MAX_BLUR_RADIUS);

    // Surface opacity and frosted glass style the interface itself and apply
    // over the window colour with no wallpaper at all, so neither may be gated
    // on the background image the way the geometry fields are.
    for field in [opacity, blur] {
        assert!(
            field["disabledWhen"].is_null(),
            "{} must stay adjustable without a wallpaper",
            field["key"]
        );
    }
    for key in [
        "backgroundRandomEnabled",
        "backgroundImageFit",
        "backgroundImagePosition",
    ] {
        let field = fields
            .iter()
            .find(|field| field["key"] == key)
            .expect("the field must be exposed");
        assert!(
            field["disabledWhen"]
                .to_string()
                .contains("backgroundImageEnabled"),
            "{key} describes the image and needs one"
        );
    }

    // Every option the schema offers must parse back into its enum, so the
    // UI can never present a choice Rust would reject.
    for field in fields {
        let Some(options) = field["options"].as_array() else {
            continue;
        };
        let key = field["key"].as_str().unwrap();
        for option in options {
            let value = option["value"].as_str().unwrap();
            let parsed = match key {
                "theme" => ThemePreference::try_from(value).map(|_| ()),
                "backgroundImageFit" => BackgroundFit::try_from(value).map(|_| ()),
                "backgroundImagePosition" => BackgroundPosition::try_from(value).map(|_| ()),
                other => panic!("unhandled appearance select: {other}"),
            };
            assert!(parsed.is_ok(), "{key} offers unparsable value {value}");
            // Presentation identifiers, never English labels.
            assert_eq!(option["label"]["kind"], "key");
        }
    }

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn reset_appearance_restores_defaults_without_touching_other_settings() {
    let paths = test_paths("appearance-reset");
    let state = ConfigState::default();

    update_global_settings_patch_inner(
        &paths,
        &state,
        json!({
            "theme": "dark",
            "backgroundImageEnabled": true,
            "backgroundRandomEnabled": true,
            "backgroundSurfaceOpacity": 45,
            "backgroundImageFit": "stretch",
            "backgroundImagePosition": "topLeft",
            "language": "en",
            "logLevel": "debug"
        }),
    )
    .unwrap();

    let reset = reset_appearance_settings_inner(&paths, &state).unwrap();

    assert_eq!(reset.theme, ThemePreference::System);
    assert!(!reset.background_image_enabled);
    assert!(!reset.background_random_enabled);
    assert_eq!(reset.background_surface_opacity, DEFAULT_SURFACE_OPACITY);
    assert_eq!(reset.background_image_fit, BackgroundFit::Cover);
    assert_eq!(reset.background_image_position, BackgroundPosition::Center);
    // Reset is scoped to Appearance; unrelated settings are untouched.
    assert_eq!(reset.language, "en");
    assert_eq!(reset.log_level, LogLevelSetting::Debug);
    // ...and it is durable, not just in-memory.
    assert_eq!(
        get_global_settings_inner(&paths, &state).unwrap().theme,
        ThemePreference::System
    );

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn an_invalid_appearance_value_rejects_the_whole_patch() {
    let paths = test_paths("appearance-atomic");
    let state = ConfigState::default();
    let initial = get_global_settings_inner(&paths, &state).unwrap();

    // Valid theme, invalid opacity: neither may be written.
    assert!(
        update_global_settings_patch_inner(
            &paths,
            &state,
            json!({"theme": "dark", "backgroundSurfaceOpacity": 5}),
        )
        .is_err()
    );

    let current = get_global_settings_inner(&paths, &state).unwrap();
    assert_eq!(current.theme, initial.theme);
    assert_eq!(
        current.background_surface_opacity,
        initial.background_surface_opacity
    );

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn the_theme_colour_defaults_to_cherry_blossom_and_normalises_to_lowercase_hex() {
    let paths = test_paths("appearance-theme-colour");
    let mut settings = AppSettings::defaults(&paths);
    assert_eq!(settings.theme_color, DEFAULT_THEME_COLOR);

    // Case and surrounding space are presentation noise, not a different
    // colour: the picker round-trips this value and compares it against the
    // presets, so one colour must have exactly one spelling.
    apply_setting_patch(&mut settings, "themeColor", json!("  #4C8DF6 ")).unwrap();
    assert_eq!(settings.theme_color, "#4c8df6");

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn an_unusable_theme_colour_is_refused_and_the_last_valid_one_survives() {
    let paths = test_paths("appearance-theme-colour-invalid");
    let mut settings = AppSettings::defaults(&paths);
    apply_setting_patch(&mut settings, "themeColor", json!("#4c8df6")).unwrap();

    for value in [
        json!("4c8df6"),    // no leading '#'
        json!("#4c8"),      // shorthand is not expanded, it is refused
        json!("#4c8df6ff"), // alpha would not survive the round trip
        json!("#gggggg"),   // not hex
        json!("rebeccapurple"),
        json!(""),
        json!(16_763_904),
        json!(null),
    ] {
        let error = apply_setting_patch(&mut settings, "themeColor", value.clone()).unwrap_err();
        assert!(error.contains("themeColor"), "{value}: {error}");
    }
    assert_eq!(settings.theme_color, "#4c8df6");

    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn resetting_appearance_restores_the_default_theme_colour() {
    let paths = test_paths("appearance-theme-colour-reset");
    let mut settings = AppSettings::defaults(&paths);
    apply_setting_patch(&mut settings, "themeColor", json!("#123456")).unwrap();

    settings.reset_appearance();

    assert_eq!(settings.theme_color, DEFAULT_THEME_COLOR);
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
