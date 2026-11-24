use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use crate::internal::hanafubuki_config::repository::settings::{
    AppSettings, ModelDownloadSource, get_global_settings_inner,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedMirrorPolicy {
    pub automatic: bool,
    pub use_micromamba: bool,
    pub use_pypi: bool,
    pub use_github: bool,
    pub custom_github: Option<String>,
    pub use_huggingface: bool,
    pub custom_huggingface: Option<String>,
    pub model_source: ModelDownloadSource,
}

impl ResolvedMirrorPolicy {
    pub fn from_settings(settings: &AppSettings) -> Self {
        Self {
            automatic: settings.auto_mirror,
            use_micromamba: settings.use_micromamba_mirror,
            use_pypi: settings.use_pypi_mirror,
            use_github: settings.use_github_mirror,
            custom_github: settings.custom_github_mirror.clone(),
            use_huggingface: settings.use_huggingface_mirror,
            custom_huggingface: settings.custom_huggingface_mirror.clone(),
            model_source: settings.model_download_source,
        }
    }
}

pub fn resolve_global_mirror_policy(
    paths: &AppPaths,
    state: &ConfigState,
) -> Result<ResolvedMirrorPolicy, String> {
    let policy = get_global_settings_inner(paths, state)
        .map(|settings| ResolvedMirrorPolicy::from_settings(&settings))?;
    log::debug!("resolved global mirror policy: {policy:?}");
    Ok(policy)
}

#[cfg(test)]
mod tests;
