use super::{
    GitRuntimeSource, LOG_FILE_MAX_SIZE_MB, LOG_RETENTION_DAYS, LogLevelSetting,
    ModelDownloadSource, ProxyMode, SUPPORTED_LANGUAGES, WebviewNewWindow,
};
use crate::internal::hanafubuki_config::model::presentation::{
    BackgroundFit, BackgroundPosition, MAX_BLUR_RADIUS, MAX_SURFACE_OPACITY, MIN_BLUR_RADIUS,
    MIN_SURFACE_OPACITY, ThemePreference,
};
use crate::internal::hanafubuki_foundation::form_schema::{
    CommitMode, FieldKind, SchemaCondition, SchemaField, SchemaGroup, SchemaOption, SchemaText,
    SchemaValue,
};
use crate::internal::hanafubuki_python_runtime::PythonVersion;
use serde::Serialize;
use std::collections::BTreeMap;

/// The settings pages' fields, by section. Each field's `commit` says when its
/// editor may submit it, following the rules `validate_settings` and
/// `prepare_settings_directories` apply: a discrete choice on change, free
/// text once left, a value whose persistence has a filesystem side effect
/// behind an explicit Save.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingsSchema {
    pub sections: Vec<SchemaGroup>,
}

/// Patch groups for the cross-field rules enforced by `validate_settings`.
///
/// A patch is all-or-nothing, so a field that another field is *required for*
/// cannot be committed alone. Editors submit every dirty member of a group
/// together; the group is declared here so the frontend never restates the
/// validation rule itself.
pub(super) mod validation_group {
    /// `proxyAddress` is required when `proxyMode` is `manual`.
    pub const PROXY: &str = "proxy";
}

fn label(key: &str) -> SchemaText {
    SchemaText::key(format!("settings.fields.{key}.label"))
}

fn help(key: &str) -> SchemaText {
    SchemaText::key(format!("settings.fields.{key}.help"))
}

/// A field labelled `settings.fields.<key>.label`.
fn field(key: &str, kind: FieldKind, commit: CommitMode) -> SchemaField {
    SchemaField::new(key, kind, label(key)).commit(commit)
}

fn switch(key: &str) -> SchemaField {
    field(key, FieldKind::Boolean, CommitMode::Change).help(help(key))
}

fn select(key: &str, options: Vec<SchemaOption>) -> SchemaField {
    field(key, FieldKind::Select, CommitMode::Change).options(options)
}

fn is(key: &str, value: SchemaValue) -> SchemaCondition {
    SchemaCondition::equals(key, value)
}

fn text(value: &str) -> SchemaValue {
    SchemaValue::Text(value.to_string())
}

fn reason(key: &str) -> SchemaText {
    SchemaText::key(format!("settings.disabledReasons.{key}"))
}

/// One option per stored value, labelled `settings.options.<field>.<value>`
/// with the value in camelCase, so a choice list is never restated by hand.
fn choice_options<T: Copy>(
    field: &str,
    all: &[T],
    name: fn(T) -> &'static str,
) -> Vec<SchemaOption> {
    all.iter()
        .map(|choice| {
            let value = name(*choice);
            SchemaOption {
                value: value.to_string(),
                label: SchemaText::key(format!("settings.options.{field}.{}", camel_case(value))),
            }
        })
        .collect()
}

fn camel_case(value: &str) -> String {
    let mut words = value.split('_');
    let mut label = words.next().unwrap_or_default().to_string();
    for word in words {
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            label.extend(first.to_uppercase());
            label.push_str(characters.as_str());
        }
    }
    label
}

/// One option per supported Python version, so the list is never restated.
fn python_version_options() -> Vec<SchemaOption> {
    PythonVersion::ALL
        .into_iter()
        .map(|version| SchemaOption {
            value: version.as_str().to_string(),
            label: SchemaText::key_with(
                "settings.options.pythonVersion",
                BTreeMap::from([("version".to_string(), version.as_str().to_string())]),
            ),
        })
        .collect()
}

fn section(id: &str, fields: Vec<SchemaField>) -> SchemaGroup {
    SchemaGroup {
        id: id.to_string(),
        label: SchemaText::key(format!("settings.sections.{id}")),
        fields,
    }
}

/// The WebView section's fields. GPU acceleration picks how WebKitGTK paints,
/// so it is offered only where WebKitGTK is the engine.
fn webview_fields() -> Vec<SchemaField> {
    let mut fields = vec![
        switch("webviewPreservePageState"),
        switch("webviewSharedCache"),
        select(
            "webviewNewWindow",
            choice_options(
                "webviewNewWindow",
                WebviewNewWindow::ALL,
                WebviewNewWindow::as_str,
            ),
        )
        .help(help("webviewNewWindow")),
    ];
    if cfg!(target_os = "linux") {
        fields.push(switch("webviewGpuAcceleration"));
    }
    fields
}

fn appearance_fields() -> Vec<SchemaField> {
    let background_off = is("backgroundImageEnabled", SchemaValue::Boolean(false));
    vec![
        select(
            "theme",
            choice_options("theme", ThemePreference::ALL, ThemePreference::as_str),
        ),
        switch("backgroundImageEnabled"),
        switch("backgroundRandomEnabled")
            .disabled_when(background_off.clone(), reason("backgroundDisabled")),
        // Surface opacity and frosted glass style the interface itself, so
        // they carry no `disabledWhen`: they apply over the wallpaper when
        // one is set and over the window colour when none is.
        field(
            "backgroundSurfaceOpacity",
            FieldKind::Number,
            CommitMode::Blur,
        )
        .bounds(MIN_SURFACE_OPACITY, MAX_SURFACE_OPACITY)
        .help(help("backgroundSurfaceOpacity")),
        field("backgroundBlurRadius", FieldKind::Number, CommitMode::Blur)
            .bounds(MIN_BLUR_RADIUS, MAX_BLUR_RADIUS)
            .help(help("backgroundBlurRadius")),
        select(
            "backgroundImageFit",
            choice_options(
                "backgroundImageFit",
                BackgroundFit::ALL,
                BackgroundFit::as_str,
            ),
        )
        .help(help("backgroundImageFit"))
        .disabled_when(background_off.clone(), reason("backgroundDisabled")),
        select(
            "backgroundImagePosition",
            choice_options(
                "backgroundImagePosition",
                BackgroundPosition::ALL,
                BackgroundPosition::as_str,
            ),
        )
        .help(help("backgroundImagePosition"))
        .disabled_when(
            SchemaCondition::Any {
                any: vec![background_off, is("backgroundImageFit", text("stretch"))],
            },
            reason("backgroundPositionUnused"),
        ),
    ]
}

fn mirror_fields() -> Vec<SchemaField> {
    let automatic = || is("autoMirror", SchemaValue::Boolean(true));
    let manual_only = |key: &str| {
        field(key, FieldKind::Boolean, CommitMode::Change)
            .disabled_when(automatic(), reason("automaticMirror"))
    };
    let custom = |key: &str, toggle: &str, why: &str| {
        field(key, FieldKind::Text, CommitMode::Blur)
            .nullable()
            .disabled_when(
                SchemaCondition::Any {
                    any: vec![automatic(), is(toggle, SchemaValue::Boolean(false))],
                },
                reason(why),
            )
    };
    vec![
        switch("autoMirror"),
        manual_only("useMicromambaMirror"),
        manual_only("usePypiMirror"),
        manual_only("useGithubMirror"),
        custom(
            "customGithubMirror",
            "useGithubMirror",
            "manualGithubMirror",
        ),
        manual_only("useHuggingfaceMirror"),
        custom(
            "customHuggingfaceMirror",
            "useHuggingfaceMirror",
            "manualHuggingfaceMirror",
        ),
        select(
            "modelDownloadSource",
            choice_options(
                "modelDownloadSource",
                ModelDownloadSource::ALL,
                ModelDownloadSource::as_str,
            ),
        )
        .disabled_when(automatic(), reason("automaticMirror")),
    ]
}

pub fn get_global_settings_schema() -> SettingsSchema {
    log::debug!("get_global_settings_schema requested");
    SettingsSchema {
        sections: vec![
            section(
                "language",
                vec![select(
                    "language",
                    choice_options("language", SUPPORTED_LANGUAGES, |language| language),
                )],
            ),
            // Appearance is rendered by a dedicated panel, not the generic
            // schema-field renderer: the background image needs a preview, a
            // native file dialog, and a clear action that no generic control can
            // express. The option lists and the opacity bounds still come from
            // here, so Rust remains the single authority for what is accepted.
            section("appearance", appearance_fields()),
            // These paths are created on disk by `prepare_settings_directories`
            // when they are persisted, so an intermediate value would leave a
            // stray directory behind. They stay behind an explicit Save.
            section(
                "paths",
                [
                    "artifactCacheDir",
                    "defaultWebuiInstallDir",
                    "webviewDownloadDir",
                ]
                .into_iter()
                .map(|key| {
                    let mut path = field(key, FieldKind::Path, CommitMode::Manual);
                    path.path_kind = Some(Default::default());
                    path
                })
                .collect(),
            ),
            section(
                "network",
                vec![
                    select(
                        "proxyMode",
                        choice_options("proxyMode", ProxyMode::ALL, ProxyMode::as_str),
                    )
                    .patch_group(validation_group::PROXY)
                    .help(help("proxyMode")),
                    field("proxyAddress", FieldKind::Text, CommitMode::Blur)
                        .nullable()
                        .patch_group(validation_group::PROXY)
                        .help(help("proxyAddress"))
                        .disabled_when(
                            SchemaCondition::Any {
                                any: vec![
                                    is("proxyMode", text("system")),
                                    is("proxyMode", text("disabled")),
                                ],
                            },
                            SchemaText::key("settings.fields.proxyAddress.manualOnly"),
                        ),
                ],
            ),
            section("mirrors", mirror_fields()),
            // Runtime owns application interpreter and Git initialization
            // choices. Toggles that only tell Python how to manage packages
            // remain in `environmentTools`.
            section(
                "runtime",
                vec![
                    select("appPythonVersion", python_version_options())
                        .help(help("appPythonVersion")),
                    select(
                        "gitRuntimeSource",
                        choice_options(
                            "gitRuntimeSource",
                            GitRuntimeSource::ALL,
                            GitRuntimeSource::as_str,
                        ),
                    )
                    .help(help("gitRuntimeSource")),
                ],
            ),
            section(
                "environmentTools",
                vec![
                    switch("enableUv"),
                    switch("enableEnvironmentCheck"),
                    switch("useCudaMalloc"),
                ],
            ),
            section(
                "logs",
                vec![
                    switch("logFilePersistenceEnabled"),
                    field("logFileMaxSizeMb", FieldKind::Number, CommitMode::Blur)
                        .bounds(*LOG_FILE_MAX_SIZE_MB.start(), *LOG_FILE_MAX_SIZE_MB.end())
                        .help(help("logFileMaxSizeMb"))
                        .disabled_when(
                            is("logFilePersistenceEnabled", SchemaValue::Boolean(false)),
                            reason("logFilePersistenceDisabled"),
                        ),
                    field("logRetentionDays", FieldKind::Number, CommitMode::Blur)
                        .bounds(*LOG_RETENTION_DAYS.start(), *LOG_RETENTION_DAYS.end())
                        .help(help("logRetentionDays")),
                    select(
                        "logLevel",
                        choice_options("logLevel", LogLevelSetting::ALL, LogLevelSetting::as_str),
                    ),
                ],
            ),
            // The embedded WebUI's own browser. Its data store and whether a
            // page survives leaving its workspace are settings; the size of
            // that store, the engine version, and clearing it are operations
            // the WebView panel asks Rust for directly.
            section("webview", webview_fields()),
            section(
                "automation",
                vec![
                    switch("autoCheckAppUpdates"),
                    switch("autoCheckInstanceUpdates"),
                    switch("autoCreateInstanceSnapshots"),
                    switch("autoCheckExtensionUpdates"),
                ],
            ),
            section(
                "diagnostics",
                vec![switch("appDiagnosticsIncludeAllInstances")],
            ),
        ],
    }
}
