mod check;
mod manual;
mod settle;

use std::sync::Arc;
use std::time::Duration;

use semver::Version;

use crate::internal::hanafubuki_app_update::{
    AppUpdateInstallResult, AppUpdateOperationPhase, AppUpdateOperationStatus, AppUpdateRegistry,
    AppUpdateTimerPort, BeginOperation, ClaimPreparedInstall, InstalledBundle, ManualUpdateReason,
    PreparedInstallClaim, ReleaseCatalogPort, SignedUpdatePort, UpdateProvider,
    UpdateRequestContext, UpdateSource,
};

/// Once one source has answered, how long the other may still take before the
/// operation continues without it. A release published only to the slower
/// source is then found by a later check.
pub const SOURCE_GRACE: Duration = Duration::from_secs(3);

pub struct AppUpdateService {
    signed: Arc<dyn SignedUpdatePort>,
    catalogs: Arc<dyn ReleaseCatalogPort>,
    timer: Arc<dyn AppUpdateTimerPort>,
    registry: Arc<AppUpdateRegistry>,
    sources: [UpdateSource; 2],
}

impl AppUpdateService {
    pub fn new(
        signed: Arc<dyn SignedUpdatePort>,
        catalogs: Arc<dyn ReleaseCatalogPort>,
        timer: Arc<dyn AppUpdateTimerPort>,
        registry: Arc<AppUpdateRegistry>,
        sources: [UpdateSource; 2],
    ) -> Result<Self, String> {
        if sources[0].provider != UpdateProvider::Modelscope
            || sources[1].provider != UpdateProvider::Huggingface
        {
            return Err("app update sources must be ModelScope then Hugging Face".into());
        }
        for source in &sources {
            for (label, value) in [
                ("latest", source.latest_url.as_str()),
                ("catalog", source.catalog_url.as_str()),
            ] {
                let parsed =
                    url::Url::parse(value).map_err(|_| format!("{label} source URL is invalid"))?;
                if parsed.scheme() != "https" || parsed.fragment().is_some() {
                    return Err(format!(
                        "{label} source URL must be HTTPS without a fragment"
                    ));
                }
            }
        }
        Ok(Self {
            signed,
            catalogs,
            timer,
            registry,
            sources,
        })
    }

    pub fn registry(&self) -> &Arc<AppUpdateRegistry> {
        &self.registry
    }

    pub fn claim_install(&self, offer_id: Option<&str>) -> Result<ClaimPreparedInstall, String> {
        self.registry.claim_prepared_install(offer_id)
    }

    pub async fn install_claim(&self, claim: PreparedInstallClaim) -> AppUpdateInstallResult {
        let activity_id = claim.activity_id;
        let _unsettled = UnsettledActivity::new(&self.registry, &activity_id);
        let capability = claim.capability;
        match capability.installer.install().await {
            Ok(()) => {
                let version = capability.offer.version;
                let _ = self.registry.finish(
                    &activity_id,
                    AppUpdateOperationPhase::Completed,
                    AppUpdateOperationStatus::Completed,
                    None,
                    Vec::new(),
                );
                self.registry.clear_availability();
                AppUpdateInstallResult::Installed {
                    activity_id,
                    version,
                }
            }
            Err(error) => {
                let warnings = vec![format!(
                    "signed update installation failed: {}",
                    diagnostic_detail(&error)
                )];
                self.manual_install_result(
                    &activity_id,
                    &capability.context,
                    warnings,
                    ManualUpdateReason::InstallationFailed,
                )
                .await
            }
        }
    }

    pub async fn install(&self, offer_id: &str) -> AppUpdateInstallResult {
        match self.claim_install(Some(offer_id)) {
            Ok(ClaimPreparedInstall::Started(claim)) => self.install_claim(claim).await,
            Ok(ClaimPreparedInstall::AlreadyRunning(activity_id)) => {
                AppUpdateInstallResult::AlreadyRunning { activity_id }
            }
            Err(error) => AppUpdateInstallResult::Unavailable {
                activity_id: "app-update:unavailable".into(),
                error,
            },
        }
    }

    /// Recover an updater handoff whose next process is not running the exact
    /// target version. This path deliberately skips signed preparation and
    /// resolves only a retained manual package offer: the automatic installer
    /// has already had its one attempt.
    pub async fn recover_installation_failure(
        &self,
        context: UpdateRequestContext,
        attempted_version: &str,
    ) -> AppUpdateInstallResult {
        let activity_id = match self
            .registry
            .begin(AppUpdateOperationPhase::CheckingCatalogSources)
        {
            Ok(BeginOperation::Started(activity_id)) => activity_id,
            Ok(BeginOperation::AlreadyRunning(activity_id)) => {
                return AppUpdateInstallResult::AlreadyRunning { activity_id };
            }
            Err(error) => {
                return AppUpdateInstallResult::Unavailable {
                    activity_id: "app-update:unavailable".into(),
                    error,
                };
            }
        };
        let _unsettled = UnsettledActivity::new(&self.registry, &activity_id);
        let warning = format!(
            "automatic update to {attempted_version} did not complete; the next launch is still running {}",
            context.current_version
        );
        if context.portable_mode {
            self.registry.clear_availability();
            self.finish_failed(&activity_id, &warning, Vec::new());
            return AppUpdateInstallResult::Unavailable {
                activity_id,
                error: warning,
            };
        }
        self.manual_install_result(
            &activity_id,
            &context,
            vec![warning],
            ManualUpdateReason::InstallationFailed,
        )
        .await
    }

    fn finish_failed(&self, activity_id: &str, error: &str, warnings: Vec<String>) {
        let _ = self.registry.finish(
            activity_id,
            AppUpdateOperationPhase::Failed,
            AppUpdateOperationStatus::Failed,
            Some(error.to_string()),
            warnings,
        );
    }
}

/// Fails a started activity whose operation ends before it settled: its
/// future was dropped or it panicked. Without it the activity would stay
/// active and every later operation would answer `AlreadyRunning`. A settled
/// activity is left as it was.
struct UnsettledActivity<'a> {
    registry: &'a AppUpdateRegistry,
    activity_id: String,
}

impl<'a> UnsettledActivity<'a> {
    fn new(registry: &'a AppUpdateRegistry, activity_id: &str) -> Self {
        Self {
            registry,
            activity_id: activity_id.to_string(),
        }
    }
}

impl Drop for UnsettledActivity<'_> {
    fn drop(&mut self) {
        self.registry.fail_unsettled(&self.activity_id);
    }
}

pub const fn supports_signed_update(bundle: InstalledBundle) -> bool {
    matches!(
        bundle,
        InstalledBundle::Nsis | InstalledBundle::AppImage | InstalledBundle::MacosApp
    )
}

fn stable_version(value: &str) -> Result<Version, String> {
    let version = Version::parse(value).map_err(|_| "application version is not valid SemVer")?;
    if !version.pre.is_empty() || !version.build.is_empty() || version.to_string() != value {
        return Err("application version must be canonical stable SemVer".into());
    }
    Ok(version)
}

fn provider_label(provider: UpdateProvider) -> &'static str {
    match provider {
        UpdateProvider::Modelscope => "ModelScope",
        UpdateProvider::Huggingface => "Hugging Face",
    }
}

fn diagnostic_detail(value: &str) -> String {
    const MAX_DETAIL_CHARS: usize = 384;
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MAX_DETAIL_CHARS)
        .collect()
}
