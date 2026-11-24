use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, json};

use super::*;
use crate::internal::hanafubuki_config::repository::settings::{ModelDownloadSource, ProxyMode};

fn test_paths(name: &str) -> AppPaths {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    AppPaths::from_app_data_dir(std::env::temp_dir().join(format!(
        "hanafubuki-persisted-settings-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )))
    .unwrap()
}

/// Project the settings into every domain document, decode each one back, and
/// assert the union reconstructs exactly what went in.
///
/// Splitting one file into ten is only safe if no field is dropped between
/// them, so this checks the round trip through the real document boundary
/// rather than through a whole-settings blob.
fn assert_lossless(paths: &AppPaths, settings: AppSettings) {
    let expected = serde_json::to_value(&settings).unwrap();
    let mut documents = Vec::new();
    for &domain in SettingsDomain::ALL {
        let document = Value::Object(document_for(domain, &settings).unwrap());
        let decoded = PersistedSettingsDecoder::new(paths)
            .decode(domain, document)
            .unwrap();
        assert!(!decoded.changed, "{domain:?} document was rewritten");
        assert!(
            decoded.issues.is_empty(),
            "{domain:?} reported {:?}",
            decoded.issues
        );
        documents.push(decoded.document);
    }
    let merged = settings_from_documents(&AppSettings::defaults(paths), documents).unwrap();
    assert_eq!(serde_json::to_value(merged).unwrap(), expected);
}

#[test]
fn every_settings_field_belongs_to_exactly_one_domain() {
    let paths = test_paths("field-coverage");
    let value = serde_json::to_value(AppSettings::defaults(&paths)).unwrap();
    let all_fields: BTreeSet<String> = value.as_object().unwrap().keys().cloned().collect();

    let mut owned: BTreeSet<String> = BTreeSet::new();
    for &domain in SettingsDomain::ALL {
        for field in domain_fields(domain) {
            assert!(
                owned.insert((*field).to_string()),
                "{field} is claimed by more than one settings document"
            );
        }
    }

    assert_eq!(
        all_fields, owned,
        "every persisted settings field must be owned by exactly one document"
    );
}

#[test]
fn every_settings_domain_shape_matches_its_persisted_fields() {
    for &domain in SettingsDomain::ALL {
        let JsonShape::Object(fields) = domain_json_shape(domain) else {
            panic!("{domain:?} settings shape must be an object");
        };
        let names = fields.iter().map(|field| field.name).collect::<Vec<_>>();
        assert_eq!(names, domain_fields(domain), "{domain:?} shape drifted");
    }
}

#[test]
fn a_field_from_another_domain_is_dropped_rather_than_applied() {
    let paths = test_paths("cross-domain");
    // `theme` belongs to Appearance. Offering it inside the Language document
    // must not let that document become a second authority for it.
    let document = serde_json::json!({"language": "en", "theme": "dark"});
    let decoded = PersistedSettingsDecoder::new(&paths)
        .decode(SettingsDomain::Language, document)
        .unwrap();

    assert!(decoded.changed, "the stray field must be rewritten away");
    assert!(decoded.issues.iter().any(|issue| {
        issue.kind == PersistedSettingsIssueKind::UnsupportedField && issue.json_pointer == "/theme"
    }));
    assert_eq!(
        Value::Object(decoded.document),
        serde_json::json!({"language": "en"}),
        "the repaired document must hold this domain's fields and nothing else"
    );
}

#[test]
fn automatic_language_preference_round_trips_without_repair() {
    let paths = test_paths("automatic-language");
    let document = serde_json::json!({"language": "auto"});
    let decoded = PersistedSettingsDecoder::new(&paths)
        .decode(SettingsDomain::Language, document.clone())
        .unwrap();

    assert!(!decoded.changed);
    assert!(decoded.issues.is_empty());
    assert_eq!(Value::Object(decoded.document), document);
}

#[test]
fn valid_appearance_section_round_trips_without_loss() {
    let paths = test_paths("appearance");
    let mut settings = AppSettings::defaults(&paths);
    settings.background_image_enabled = true;
    settings.background_random_enabled = true;
    settings.background_surface_opacity = 73;
    settings.background_blur_radius = 21;
    assert_lossless(&paths, settings);
}

#[test]
fn valid_path_section_round_trips_without_loss() {
    let paths = test_paths("paths");
    let mut settings = AppSettings::defaults(&paths);
    settings.artifact_cache_dir = paths.app_data_dir.join("custom-artifacts");
    settings.default_webui_install_dir = paths.app_data_dir.join("custom-instances");
    assert_lossless(&paths, settings);
}

#[test]
fn valid_network_and_mirror_sections_round_trip_without_loss() {
    let paths = test_paths("network");
    let mut settings = AppSettings::defaults(&paths);
    settings.proxy_mode = ProxyMode::Manual;
    settings.proxy_address = Some("http://127.0.0.1:7890".to_string());
    settings.auto_mirror = false;
    settings.use_micromamba_mirror = false;
    settings.use_pypi_mirror = false;
    settings.use_github_mirror = true;
    settings.custom_github_mirror = Some("https://github.example.com".to_string());
    settings.use_huggingface_mirror = true;
    settings.custom_huggingface_mirror = Some("https://hf.example.com".to_string());
    settings.model_download_source = ModelDownloadSource::Huggingface;
    assert_lossless(&paths, settings);
}

/// A small deterministic generator, so the property below needs no dependency.
struct Generator(u64);

impl Generator {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    fn key(&mut self, domain: SettingsDomain) -> String {
        const NESTED: &[&str] = &[
            "providers",
            "modelPolicy",
            "revision",
            "defaultProviderId",
            "id",
            "displayName",
            "enabled",
            "protocol",
            "baseUrl",
            "apiToken",
            "requestHeaders",
            "name",
            "value",
            "model",
            "turnBudget",
            "autoCompactThresholdPercent",
            "maxSourceAgeDays",
            "extractionModel",
            "inputCapabilities",
            "imageInput",
            "unknown",
        ];
        if self.below(3) == 0 {
            (*self.pick(NESTED)).to_string()
        } else {
            (*self.pick(domain_fields(domain))).to_string()
        }
    }

    fn value(&mut self, domain: SettingsDomain, depth: usize) -> Value {
        const STRINGS: &[&str] = &[
            "",
            " ",
            "auto",
            "en",
            "zh",
            "manual",
            "system",
            "disabled",
            "searxng",
            "dark",
            "#A1B2C3",
            "info",
            "3.11",
            "responses",
            "chat_completions",
            "/",
            "relative",
            "http://127.0.0.1:8080",
            "https://user:pw@example.com",
            "https://example.com/v1",
            "socks5://proxy:1080",
            "x\u{0}y",
            "web",
            " padded ",
            "primary",
        ];
        let kinds = if depth == 0 { 6 } else { 8 };
        match self.below(kinds) {
            0 => Value::Null,
            1 => Value::Bool(self.below(2) == 0),
            2 => json!(*self.pick(&[0_i64, 1, 7, 60, 80, 100, 255, 256, 3650, -1, i64::MAX])),
            3 => json!(*self.pick(&[0.5_f64, 1e300, -2.5])),
            4 | 5 => Value::String((*self.pick(STRINGS)).to_string()),
            6 => Value::Array(
                (0..self.below(4))
                    .map(|_| self.value(domain, depth - 1))
                    .collect(),
            ),
            _ => Value::Object(self.object(domain, depth - 1)),
        }
    }

    fn object(&mut self, domain: SettingsDomain, depth: usize) -> Map<String, Value> {
        (0..self.below(8))
            .map(|_| (self.key(domain), self.value(domain, depth)))
            .collect()
    }
}

/// Any JSON object decodes as any settings domain without panicking, into
/// settings the whole contract accepts, and decoding the repair again changes
/// nothing.
#[test]
fn decoding_arbitrary_objects_always_yields_valid_settings() {
    let paths = test_paths("property");
    let defaults = AppSettings::defaults(&paths);
    let decoder = PersistedSettingsDecoder::new(&paths);
    let mut generator = Generator(0x9e37_79b9_7f4a_7c15);
    for round in 0..400 {
        for &domain in SettingsDomain::ALL {
            let value = Value::Object(generator.object(domain, 3));
            let decoded = decoder
                .decode(domain, value.clone())
                .unwrap_or_else(|error| panic!("round {round} {domain:?} {value}: {error}"));
            let merged = settings_from_documents(&defaults, [decoded.document.clone()])
                .unwrap_or_else(|error| panic!("round {round} {domain:?} {value}: {error}"));
            assert!(
                validate_settings(&merged).is_ok(),
                "round {round} {domain:?} {value}"
            );
            let again = decoder
                .decode(domain, Value::Object(decoded.document.clone()))
                .unwrap();
            assert!(
                !again.changed && again.issues.is_empty(),
                "round {round} {domain:?} {value}: repair is not stable: {:?}",
                again.issues
            );
        }
    }
    // A root that is not an object decodes to the domain's defaults.
    for value in [Value::Null, json!([1, 2]), json!("text"), json!(3)] {
        for &domain in SettingsDomain::ALL {
            let decoded = decoder.decode(domain, value.clone()).unwrap();
            assert_eq!(decoded.document, document_for(domain, &defaults).unwrap());
            assert!(!decoded.issues.is_empty());
        }
    }
}

/// Whether the persisted reader keeps `value` for `key` rather than repairing
/// it to the default.
fn persisted_reader_keeps(paths: &AppPaths, key: &str, value: &Value) -> bool {
    let settings = AppSettings::defaults(paths);
    let domain = *SettingsDomain::ALL
        .iter()
        .find(|domain| domain_fields(**domain).contains(&key))
        .unwrap();
    let mut document = document_for(domain, &settings).unwrap();
    document.insert(key.to_string(), value.clone());
    let decoded = PersistedSettingsDecoder::new(paths)
        .decode(domain, Value::Object(document))
        .unwrap();
    decoded.document.get(key) == Some(value)
}

fn patch_accepts(paths: &AppPaths, key: &str, value: &Value) -> bool {
    let mut settings = AppSettings::defaults(paths);
    crate::internal::hanafubuki_config::repository::settings::apply_setting_patch(
        &mut settings,
        key,
        value.clone(),
    )
    .is_ok()
}

fn schema_field(key: &str) -> Value {
    serde_json::to_value(
        crate::internal::hanafubuki_config::repository::settings::get_global_settings_schema(),
    )
    .unwrap()["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|section| section["fields"].as_array().unwrap().clone())
        .find(|field| field["key"] == key)
        .unwrap()
}

/// One rule per field: what the schema offers, the patch accepts and the
/// persisted reader keeps are the same set, for the choice lists the old
/// copies spelled out by hand and the bounds they restated.
#[test]
fn the_schema_the_patch_and_the_persisted_reader_agree_on_each_rule() {
    let paths = test_paths("one-rule");
    for key in [
        "language",
        "logLevel",
        "theme",
        "backgroundImageFit",
        "backgroundImagePosition",
        "proxyMode",
        "modelDownloadSource",
        "gitRuntimeSource",
        "webviewNewWindow",
    ] {
        let options = schema_field(key)["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| option["value"].clone())
            .collect::<Vec<_>>();
        assert!(!options.is_empty(), "{key}");
        for option in &options {
            assert!(patch_accepts(&paths, key, option), "{key} {option}");
            // `manual` needs an address the default settings do not have; the
            // reader repairs such a pair as a whole.
            if !matches!((key, option.as_str()), ("proxyMode", Some("manual"))) {
                assert!(
                    persisted_reader_keeps(&paths, key, option),
                    "{key} {option}"
                );
            }
        }
        for unsupported in [json!("unsupported"), json!(""), json!(1)] {
            assert!(!patch_accepts(&paths, key, &unsupported), "{key}");
            assert!(!persisted_reader_keeps(&paths, key, &unsupported), "{key}");
        }
    }
    for key in ["logRetentionDays", "logFileMaxSizeMb"] {
        let field = schema_field(key);
        let (min, max) = (
            field["min"].as_u64().unwrap(),
            field["max"].as_u64().unwrap(),
        );
        for (value, supported) in [(min - 1, false), (min, true), (max, true), (max + 1, false)] {
            let value = json!(value);
            assert_eq!(
                patch_accepts(&paths, key, &value),
                supported,
                "{key} {value}"
            );
            assert_eq!(
                persisted_reader_keeps(&paths, key, &value),
                supported,
                "{key} {value}"
            );
        }
    }
}

/// The serde spelling of each choice is the one `setting_choice!` declares.
#[test]
fn every_settings_choice_is_stored_under_its_declared_name() {
    fn check<T>(all: &[T], name: fn(T) -> &'static str)
    where
        T: Copy + std::fmt::Debug + PartialEq + serde::Serialize + serde::de::DeserializeOwned,
        for<'a> T: TryFrom<&'a str, Error = String>,
    {
        for choice in all {
            let stored = serde_json::to_value(choice).unwrap();
            assert_eq!(stored, json!(name(*choice)), "{choice:?}");
            assert_eq!(serde_json::from_value::<T>(stored).unwrap(), *choice);
            assert_eq!(T::try_from(name(*choice)).unwrap(), *choice);
        }
    }
    use crate::internal::hanafubuki_config::model::presentation::{
        BackgroundFit, BackgroundPosition, ThemePreference,
    };
    use crate::internal::hanafubuki_config::repository::settings::{
        GitRuntimeSource, LogLevelSetting, WebviewNewWindow,
    };
    check(ThemePreference::ALL, ThemePreference::as_str);
    check(BackgroundFit::ALL, BackgroundFit::as_str);
    check(BackgroundPosition::ALL, BackgroundPosition::as_str);
    check(ProxyMode::ALL, ProxyMode::as_str);
    check(ModelDownloadSource::ALL, ModelDownloadSource::as_str);
    check(GitRuntimeSource::ALL, GitRuntimeSource::as_str);
    check(WebviewNewWindow::ALL, WebviewNewWindow::as_str);
    check(LogLevelSetting::ALL, LogLevelSetting::as_str);
}

/// The mirror URL rules the patch and the persisted reader used to restate:
/// both now call the same normalizer.
#[test]
fn mirror_urls_follow_one_rule_on_both_paths() {
    let paths = test_paths("one-url-rule");
    for (key, value, supported) in [
        ("customGithubMirror", "https://mirror.example/gh/", true),
        (
            "customGithubMirror",
            "https://user:secret@mirror.example/?q#f",
            true,
        ),
        ("customGithubMirror", "ftp://mirror.example", false),
        ("customGithubMirror", "not a url", false),
        ("customHuggingfaceMirror", "http://127.0.0.1:8080", true),
        ("customHuggingfaceMirror", "file:///tmp", false),
    ] {
        let value = json!(value);
        assert_eq!(
            patch_accepts(&paths, key, &value),
            supported,
            "{key} {value}"
        );
        assert_eq!(
            persisted_reader_keeps(&paths, key, &value),
            supported,
            "{key} {value}"
        );
    }
}
