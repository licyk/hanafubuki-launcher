//! A corpus of damaged settings documents, one family of damage per domain:
//! every field that can still be used survives, the damaged one takes its
//! default and is reported, and an invalid original is kept in quarantine.

use std::fs;

use serde_json::{Map, Value, json};

use crate::internal::hanafubuki_config::model::presentation::{BackgroundFit, ThemePreference};
use crate::internal::hanafubuki_config::repository::settings::{
    AppSettings, GitRuntimeSource, ModelDownloadSource, ProxyMode, WebviewNewWindow,
    get_global_settings_inner, settings_domain_document,
};
use crate::internal::hanafubuki_config::{AppPaths, ConfigState, SettingsDomain};

use super::{ConfigPersistenceHealth, ConfigRecoveryAttention, bootstrap_config, paths};

type Document = Map<String, Value>;

/// One damage applied to one domain's document.
struct Case {
    domain: SettingsDomain,
    label: &'static str,
    /// Damage the valid customized document.
    damage: fn(&mut Document),
    /// Turn the customized document into what the repair must hold, given the
    /// domain's default document.
    expect: fn(&mut Document, &Document),
    /// The pointer the report names.
    pointer: &'static str,
    attention: ConfigRecoveryAttention,
}

fn default_field(field: &'static str) -> impl Fn(&mut Document, &Document) {
    move |document, defaults| {
        document.insert(field.to_string(), defaults[field].clone());
    }
}

/// Settings where every domain holds values other than its defaults, so a
/// field that survives is visibly the user's.
fn customized(paths: &AppPaths) -> AppSettings {
    let mut settings = AppSettings::defaults(paths);
    settings.language = "en".to_string();
    settings.theme = ThemePreference::Dark;
    settings.background_image_fit = BackgroundFit::Contain;
    settings.background_surface_opacity = 73;
    settings.artifact_cache_dir = paths.app_data_dir.join("custom-cache");
    settings.default_webui_install_dir = paths.app_data_dir.join("custom-instances");
    settings.proxy_mode = ProxyMode::Manual;
    settings.proxy_address = Some("http://127.0.0.1:7890".to_string());
    settings.use_github_mirror = false;
    settings.custom_github_mirror = Some("https://github.example.com".to_string());
    settings.model_download_source = ModelDownloadSource::Huggingface;
    settings.git_runtime_source = GitRuntimeSource::Micromamba;
    settings.enable_uv = false;
    settings.use_cuda_malloc = false;
    settings.log_retention_days = 42;
    settings.log_level =
        crate::internal::hanafubuki_config::repository::settings::LogLevelSetting::Debug;
    settings.app_diagnostics_include_all_instances = false;
    settings.auto_check_app_updates = false;
    settings.auto_create_instance_snapshots = false;
    settings.auto_check_extension_updates = false;
    settings.webview_new_window = WebviewNewWindow::Browser;
    settings.webview_shared_cache = false;
    settings.disabled_modules = ["diagnostics".to_string()].into();
    settings.system_webui_scan_prompted = true;
    settings
}

fn unknown_field(document: &mut Document) {
    document.insert("fromTheFuture".to_string(), json!({"kept": false}));
}

fn unchanged(_: &mut Document, _: &Document) {}

fn cases() -> Vec<Case> {
    use ConfigRecoveryAttention::{Error, Warning};
    use SettingsDomain::*;
    let mut cases = Vec::new();
    // An unknown field is dropped from every domain and nothing else changes.
    for &domain in SettingsDomain::ALL {
        cases.push(Case {
            domain,
            label: "unknown field",
            damage: unknown_field,
            expect: unchanged,
            pointer: "/fromTheFuture",
            attention: Warning,
        });
    }
    macro_rules! case {
        ($domain:expr, $label:literal, $pointer:literal, $attention:expr, $damage:expr, $expect:expr) => {
            cases.push(Case {
                domain: $domain,
                label: $label,
                damage: $damage,
                expect: $expect,
                pointer: $pointer,
                attention: $attention,
            })
        };
    }
    case!(
        Language,
        "wrong type",
        "/language",
        Warning,
        |d| {
            d.insert("language".into(), json!(5));
        },
        |d, x| default_field("language")(d, x)
    );
    case!(
        Language,
        "unknown value",
        "/language",
        Warning,
        |d| {
            d.insert("language".into(), json!("klingon"));
        },
        |d, x| default_field("language")(d, x)
    );
    case!(
        Appearance,
        "out of range",
        "/backgroundSurfaceOpacity",
        Warning,
        |d| {
            d.insert("backgroundSurfaceOpacity".into(), json!(250));
        },
        |d, x| default_field("backgroundSurfaceOpacity")(d, x)
    );
    case!(
        Appearance,
        "unknown enum value",
        "/theme",
        Warning,
        |d| {
            d.insert("theme".into(), json!("neon"));
        },
        |d, x| default_field("theme")(d, x)
    );
    case!(
        Appearance,
        "wrong type",
        "/backgroundImageFit",
        Warning,
        |d| {
            d.insert("backgroundImageFit".into(), json!(["cover"]));
        },
        |d, x| default_field("backgroundImageFit")(d, x)
    );
    case!(
        Paths,
        "path policy",
        "/artifactCacheDir",
        Warning,
        |d| {
            d.insert("artifactCacheDir".into(), json!("/"));
        },
        |d, x| default_field("artifactCacheDir")(d, x)
    );
    case!(
        Paths,
        "wrong type",
        "/defaultWebuiInstallDir",
        Warning,
        |d| {
            d.insert("defaultWebuiInstallDir".into(), json!(7));
        },
        |d, x| default_field("defaultWebuiInstallDir")(d, x)
    );
    // A manual proxy without a usable address cannot stay manual; with a
    // usable address an unreadable mode keeps routing through it.
    case!(
        Network,
        "invalid proxy address",
        "/proxyMode",
        Error,
        |d| {
            d.insert("proxyAddress".into(), json!("http://no-port.example"));
        },
        |d, x| {
            default_field("proxyAddress")(d, x);
            default_field("proxyMode")(d, x);
        }
    );
    case!(
        Network,
        "missing proxy mode beside a usable address",
        "/proxyMode",
        Error,
        |d| {
            d.remove("proxyMode");
        },
        unchanged
    );
    case!(
        Network,
        "unknown proxy mode",
        "/proxyMode",
        Error,
        |d| {
            d.insert("proxyMode".into(), json!("pac"));
        },
        unchanged
    );
    case!(
        Mirrors,
        "invalid URL",
        "/customGithubMirror",
        Warning,
        |d| {
            d.insert("customGithubMirror".into(), json!("not a url"));
        },
        |d, x| default_field("customGithubMirror")(d, x)
    );
    case!(
        Mirrors,
        "unknown enum value",
        "/modelDownloadSource",
        Warning,
        |d| {
            d.insert("modelDownloadSource".into(), json!("carrier-pigeon"));
        },
        |d, x| default_field("modelDownloadSource")(d, x)
    );
    case!(
        Runtime,
        "unknown enum value",
        "/appPythonVersion",
        Warning,
        |d| {
            d.insert("appPythonVersion".into(), json!("9.99"));
        },
        |d, x| default_field("appPythonVersion")(d, x)
    );
    case!(
        EnvironmentTools,
        "wrong type",
        "/enableUv",
        Warning,
        |d| {
            d.insert("enableUv".into(), json!("no"));
        },
        |d, x| default_field("enableUv")(d, x)
    );
    case!(
        Logging,
        "out of range",
        "/logRetentionDays",
        Warning,
        |d| {
            d.insert("logRetentionDays".into(), json!(0));
        },
        |d, x| default_field("logRetentionDays")(d, x)
    );
    case!(
        Logging,
        "unknown value",
        "/logLevel",
        Warning,
        |d| {
            d.insert("logLevel".into(), json!("verbose"));
        },
        |d, x| default_field("logLevel")(d, x)
    );
    case!(
        Diagnostics,
        "wrong type",
        "/appDiagnosticsIncludeAllInstances",
        Warning,
        |d| {
            d.insert("appDiagnosticsIncludeAllInstances".into(), json!(1));
        },
        |d, x| default_field("appDiagnosticsIncludeAllInstances")(d, x)
    );
    case!(
        Automation,
        "wrong type",
        "/autoCheckAppUpdates",
        Warning,
        |d| {
            d.insert("autoCheckAppUpdates".into(), json!(null));
        },
        |d, x| default_field("autoCheckAppUpdates")(d, x)
    );
    case!(
        Automation,
        "wrong type",
        "/autoCheckExtensionUpdates",
        Warning,
        |d| {
            d.insert("autoCheckExtensionUpdates".into(), json!("yes"));
        },
        |d, x| default_field("autoCheckExtensionUpdates")(d, x)
    );
    case!(
        Webview,
        "unknown enum value",
        "/webviewNewWindow",
        Warning,
        |d| {
            d.insert("webviewNewWindow".into(), json!("tab"));
        },
        |d, x| default_field("webviewNewWindow")(d, x)
    );
    case!(
        Onboarding,
        "wrong type",
        "/systemWebuiScanPrompted",
        Warning,
        |d| {
            d.insert("systemWebuiScanPrompted".into(), json!("yes"));
        },
        |d, x| default_field("systemWebuiScanPrompted")(d, x)
    );
    case!(
        Modules,
        "bad disabled module name",
        "/disabledModules/1",
        Warning,
        |d| {
            d.insert("disabledModules".into(), json!(["diagnostics", "\tproxy"]));
        },
        unchanged
    );
    case!(
        Modules,
        "unreadable list",
        "/disabledModules",
        Error,
        |d| {
            d.insert("disabledModules".into(), json!("diagnostics"));
        },
        |d, _| {
            d.insert("disabledModules".into(), json!([]));
        }
    );
    cases
}

#[test]
fn a_damaged_field_is_defaulted_alone_reported_and_its_original_quarantined() {
    for case in cases() {
        let label = format!("{:?} {}", case.domain, case.label);
        let paths = paths("settings-corpus");
        let settings = customized(&paths);
        let valid = settings_domain_document(case.domain, &settings).unwrap();
        let defaults =
            settings_domain_document(case.domain, &AppSettings::defaults(&paths)).unwrap();
        let mut damaged = valid.clone();
        (case.damage)(&mut damaged);
        let mut expected = valid.clone();
        (case.expect)(&mut expected, &defaults);
        let file = paths.settings_file(case.domain);
        fs::write(&file, serde_json::to_vec_pretty(&damaged).unwrap()).unwrap();
        let state = ConfigState::default();

        bootstrap_config(&paths, &state);

        let repaired: Document = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(repaired, expected, "{label}: repaired document");
        let published =
            settings_domain_document(case.domain, &state.settings_snapshot().unwrap()).unwrap();
        assert_eq!(published, expected, "{label}: published settings");
        let report = state
            .recovery_reports()
            .into_iter()
            .find(|report| {
                report.file_kind
                    == crate::internal::hanafubuki_config::recovery::ConfigFileKind::GlobalSettings {
                        domain: case.domain,
                    }
            })
            .unwrap_or_else(|| panic!("{label}: no report"));
        assert!(
            report
                .actions
                .iter()
                .any(|action| action.json_pointer.as_deref() == Some(case.pointer)),
            "{label}: {:?}",
            report.actions
        );
        assert_eq!(report.attention, case.attention, "{label}: attention");
        let quarantined = report
            .quarantine_relative_path
            .as_ref()
            .map(|relative| paths.app_data_dir.join(relative))
            .unwrap_or_else(|| panic!("{label}: original not quarantined"));
        let original: Document = serde_json::from_slice(&fs::read(&quarantined).unwrap()).unwrap();
        assert_eq!(original, damaged, "{label}: quarantined original");
        assert_eq!(state.persistence_health(), ConfigPersistenceHealth::Healthy);
        let _ = fs::remove_dir_all(&paths.app_data_dir);
    }
}

/// A field added after the document was written is ordinary: it is defaulted
/// and reported for information, and the document is rewritten in place (its
/// previous version stays as the backup) rather than quarantined.
#[test]
fn a_missing_field_is_defaulted_and_reported_without_quarantine() {
    // Network has no field whose absence is neutral: either one decides where
    // traffic goes, so the corpus covers it as a policy repair.
    for &domain in SettingsDomain::ALL {
        if domain == SettingsDomain::Network {
            continue;
        }
        let paths = paths("settings-missing-field");
        let settings = customized(&paths);
        let mut document = settings_domain_document(domain, &settings).unwrap();
        let defaults = settings_domain_document(domain, &AppSettings::defaults(&paths)).unwrap();
        let field = document.keys().next().unwrap().clone();
        document.remove(&field);
        fs::write(
            paths.settings_file(domain),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
        let state = ConfigState::default();

        bootstrap_config(&paths, &state);

        let published =
            settings_domain_document(domain, &state.settings_snapshot().unwrap()).unwrap();
        for (key, value) in &document {
            assert_eq!(&published[key], value, "{domain:?}: {key} survived");
        }
        assert_eq!(published[&field], defaults[&field], "{domain:?}: {field}");
        let report = state.recovery_reports().pop().unwrap();
        assert!(
            report
                .actions
                .iter()
                .any(|action| action.json_pointer.as_deref() == Some(&format!("/{field}")))
        );
        assert!(report.quarantine_relative_path.is_none(), "{domain:?}");
        let _ = fs::remove_dir_all(&paths.app_data_dir);
    }
}

#[test]
fn a_duplicate_key_is_handled_the_same_at_bootstrap_and_at_runtime() {
    let raw = r#"{"language":"en","language":"zh"}"#;
    let at_bootstrap = paths("settings-duplicate-bootstrap");
    let at_runtime = paths("settings-duplicate-runtime");
    for paths in [&at_bootstrap, &at_runtime] {
        fs::write(paths.settings_file(SettingsDomain::Language), raw).unwrap();
    }
    let bootstrap_state = ConfigState::default();
    bootstrap_config(&at_bootstrap, &bootstrap_state);
    let runtime_state = ConfigState::default();
    // Nothing published yet: the runtime read goes through the same reader.
    let runtime_settings = get_global_settings_inner(&at_runtime, &runtime_state).unwrap();

    for (paths, state, settings) in [
        (
            &at_bootstrap,
            &bootstrap_state,
            bootstrap_state.settings_snapshot().unwrap(),
        ),
        (&at_runtime, &runtime_state, runtime_settings),
    ] {
        assert_eq!(settings.language, "auto", "neither duplicate value wins");
        let report = state.recovery_reports().pop().unwrap();
        assert!(report.quarantine_relative_path.is_some());
        let repaired: Value = serde_json::from_slice(
            &fs::read(paths.settings_file(SettingsDomain::Language)).unwrap(),
        )
        .unwrap();
        assert_eq!(repaired, json!({"language": "auto"}));
        let _ = fs::remove_dir_all(&paths.app_data_dir);
    }
}
