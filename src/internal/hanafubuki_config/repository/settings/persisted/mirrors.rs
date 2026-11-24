use serde_json::{Map, Value};

use super::field::FieldReader;
use super::{AppSettings, PersistedSettingsIssue};
use crate::internal::hanafubuki_config::repository::settings::validation::normalize_http_url;

pub(super) const FIELDS: &[&str] = &[
    "autoMirror",
    "useMicromambaMirror",
    "usePypiMirror",
    "useGithubMirror",
    "customGithubMirror",
    "useHuggingfaceMirror",
    "customHuggingfaceMirror",
    "modelDownloadSource",
];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.auto_mirror = fields.read("autoMirror", defaults.auto_mirror);
    settings.use_micromamba_mirror =
        fields.read("useMicromambaMirror", defaults.use_micromamba_mirror);
    settings.use_pypi_mirror = fields.read("usePypiMirror", defaults.use_pypi_mirror);
    settings.use_github_mirror = fields.read("useGithubMirror", defaults.use_github_mirror);
    settings.custom_github_mirror = fields.read_checked(
        "customGithubMirror",
        defaults.custom_github_mirror.clone(),
        |value: Option<String>| value.as_deref().map(normalize_http_url).transpose(),
    );
    settings.use_huggingface_mirror =
        fields.read("useHuggingfaceMirror", defaults.use_huggingface_mirror);
    settings.custom_huggingface_mirror = fields.read_checked(
        "customHuggingfaceMirror",
        defaults.custom_huggingface_mirror.clone(),
        |value: Option<String>| value.as_deref().map(normalize_http_url).transpose(),
    );
    settings.model_download_source =
        fields.read("modelDownloadSource", defaults.model_download_source);
}
