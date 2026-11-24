//! The update check: query both signed sources, then prepare the newest offer.

use std::sync::Arc;

use semver::Version;

use super::settle::grace_expired;
use super::{
    AppUpdateService, UnsettledActivity, diagnostic_detail, provider_label, stable_version,
    supports_signed_update,
};
use crate::internal::hanafubuki_app_update::{
    AppUpdateCheckResult, AppUpdateOperationPhase, AppUpdateOperationStatus, AppUpdateProgressSink,
    AppUpdateRegistry, BeginOperation, DownloadProgress, ManualUpdateReason, SignedCheckOutcome,
    SignedCheckRequest, SignedPrepareOutcome, SignedPrepareRequest, SourceError, UpdateProvider,
    UpdateRequestContext,
};

impl AppUpdateService {
    pub async fn check(&self, context: UpdateRequestContext) -> AppUpdateCheckResult {
        if context.portable_mode {
            self.registry.clear_availability();
            return AppUpdateCheckResult::PortableModeUnsupported {
                current_version: context.current_version,
            };
        }
        let initial_phase = initial_check_phase(context.installed_bundle);
        let activity_id = match self.registry.begin(initial_phase) {
            Ok(BeginOperation::Started(activity_id)) => activity_id,
            Ok(BeginOperation::AlreadyRunning(activity_id)) => {
                return AppUpdateCheckResult::AlreadyRunning { activity_id };
            }
            Err(error) => {
                return AppUpdateCheckResult::Unavailable {
                    activity_id: "app-update:unavailable".into(),
                    current_version: context.current_version,
                    error,
                };
            }
        };
        let _unsettled = UnsettledActivity::new(&self.registry, &activity_id);
        let current = match stable_version(&context.current_version) {
            Ok(version) => version,
            Err(error) => {
                self.finish_failed(&activity_id, &error, Vec::new());
                return AppUpdateCheckResult::Unavailable {
                    activity_id,
                    current_version: context.current_version,
                    error,
                };
            }
        };

        if !supports_signed_update(context.installed_bundle) {
            return self
                .manual_result(
                    &activity_id,
                    &context,
                    Vec::new(),
                    &[],
                    ManualUpdateReason::InstalledBundleUnsupported,
                )
                .await;
        }

        let left_request = SignedCheckRequest {
            source: self.sources[0].clone(),
            context: context.clone(),
        };
        let right_request = SignedCheckRequest {
            source: self.sources[1].clone(),
            context: context.clone(),
        };
        let (left, right) = self
            .settle_both(
                self.signed.check(left_request),
                self.signed.check(right_request),
                || SourceError::Unreachable(grace_expired()),
            )
            .await;
        let unreachable = [
            (self.sources[0].provider, &left),
            (self.sources[1].provider, &right),
        ]
        .into_iter()
        .filter(|(_, result)| matches!(result, Err(SourceError::Unreachable(_))))
        .map(|(provider, _)| provider)
        .collect::<Vec<_>>();
        let attempts = [
            normalize_signed_check(self.sources[0].provider, left, &current),
            normalize_signed_check(self.sources[1].provider, right, &current),
        ];
        let mut warnings = attempts
            .iter()
            .filter_map(|attempt| attempt.as_ref().err().cloned())
            .collect::<Vec<_>>();
        let mut updates = attempts
            .iter()
            .filter_map(|attempt| match attempt {
                Ok(SignedCheckOutcome::Update(metadata)) => Some(metadata.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        updates.sort_by(|left, right| {
            stable_version(&right.version)
                .unwrap()
                .cmp(&stable_version(&left.version).unwrap())
                .then_with(|| provider_rank(left.provider).cmp(&provider_rank(right.provider)))
        });

        if let Some(selected) = updates.first().cloned() {
            let selected_sources = updates
                .iter()
                .filter(|metadata| metadata.version == selected.version)
                .filter_map(|metadata| {
                    self.sources
                        .iter()
                        .find(|source| source.provider == metadata.provider)
                        .cloned()
                })
                .collect::<Vec<_>>();
            let _ = self
                .registry
                .set_phase(&activity_id, AppUpdateOperationPhase::Downloading);
            let progress: Arc<dyn AppUpdateProgressSink> = Arc::new(RegistryProgressSink {
                registry: self.registry.clone(),
                activity_id: activity_id.clone(),
            });
            for source in selected_sources {
                let request = SignedPrepareRequest {
                    source: source.clone(),
                    context: context.clone(),
                    expected_version: selected.version.clone(),
                };
                match self.signed.prepare(request, progress.clone()).await {
                    Ok(SignedPrepareOutcome::Prepared(prepared))
                        if prepared.metadata.provider == source.provider
                            && prepared.metadata.version == selected.version =>
                    {
                        match self.registry.retain_prepared(
                            &context.current_version,
                            prepared.metadata,
                            context.clone(),
                            prepared.installer,
                        ) {
                            Ok(offer) => {
                                let _ = self.registry.finish(
                                    &activity_id,
                                    AppUpdateOperationPhase::ReadyToInstall,
                                    AppUpdateOperationStatus::Completed,
                                    None,
                                    warnings,
                                );
                                return AppUpdateCheckResult::ReadyToInstall { activity_id, offer };
                            }
                            Err(error) => {
                                self.finish_failed(&activity_id, &error, warnings);
                                return AppUpdateCheckResult::Unavailable {
                                    activity_id,
                                    current_version: context.current_version,
                                    error,
                                };
                            }
                        }
                    }
                    Ok(SignedPrepareOutcome::Prepared(_)) => warnings.push(format!(
                        "{} prepared mismatched update metadata",
                        provider_label(source.provider)
                    )),
                    Ok(SignedPrepareOutcome::VersionChanged(metadata)) => warnings.push(format!(
                        "{} changed the offered version from {} to {} during preparation",
                        provider_label(source.provider),
                        selected.version,
                        metadata.version
                    )),
                    Err(error) => warnings.push(format!(
                        "{} signed update preparation failed: {}",
                        provider_label(source.provider),
                        diagnostic_detail(&error)
                    )),
                }
            }
            return self
                .manual_result(
                    &activity_id,
                    &context,
                    warnings,
                    &unreachable,
                    ManualUpdateReason::PreparationFailed,
                )
                .await;
        }

        // An unreachable source's catalog lives on the same host, so asking it
        // again cannot settle the check; every source that answered decides.
        let answered_up_to_date = attempts
            .iter()
            .any(|attempt| matches!(attempt, Ok(SignedCheckOutcome::NoUpdate)))
            && attempts.iter().zip(&self.sources).all(|(attempt, source)| {
                matches!(attempt, Ok(SignedCheckOutcome::NoUpdate))
                    || unreachable.contains(&source.provider)
            });
        if answered_up_to_date {
            self.registry.clear_availability();
            let _ = self.registry.finish(
                &activity_id,
                AppUpdateOperationPhase::Completed,
                AppUpdateOperationStatus::Completed,
                None,
                warnings,
            );
            return AppUpdateCheckResult::UpToDate {
                activity_id,
                current_version: context.current_version,
            };
        }

        self.manual_result(
            &activity_id,
            &context,
            warnings,
            &unreachable,
            ManualUpdateReason::SignedUpdateUnavailable,
        )
        .await
    }
}

struct RegistryProgressSink {
    registry: Arc<AppUpdateRegistry>,
    activity_id: String,
}

impl AppUpdateProgressSink for RegistryProgressSink {
    fn progress(&self, progress: DownloadProgress) {
        let _ = self.registry.progress(&self.activity_id, progress);
    }
}

fn provider_rank(provider: UpdateProvider) -> u8 {
    match provider {
        UpdateProvider::Modelscope => 0,
        UpdateProvider::Huggingface => 1,
    }
}

fn normalize_signed_check(
    provider: UpdateProvider,
    result: Result<SignedCheckOutcome, SourceError>,
    current: &Version,
) -> Result<SignedCheckOutcome, String> {
    match result {
        Ok(SignedCheckOutcome::NoUpdate) => Ok(SignedCheckOutcome::NoUpdate),
        Ok(SignedCheckOutcome::Update(metadata)) => {
            if metadata.provider != provider {
                return Err(format!(
                    "{:?} returned mismatched source metadata",
                    provider
                ));
            }
            if metadata
                .notes
                .as_ref()
                .is_some_and(|notes| notes.len() > 16 * 1024)
                || metadata
                    .published_at
                    .as_ref()
                    .is_some_and(|value| value.len() > 64)
            {
                return Err(format!("{:?} returned oversized update metadata", provider));
            }
            let version = stable_version(&metadata.version)
                .map_err(|_| format!("{:?} returned an invalid stable version", provider))?;
            if version <= *current {
                return Err(format!("{:?} returned an equal or older version", provider));
            }
            Ok(SignedCheckOutcome::Update(metadata))
        }
        Err(error) => Err(format!(
            "{} signed update check failed: {}",
            provider_label(provider),
            diagnostic_detail(error.message())
        )),
    }
}

/// The phase a check starts in: the signed sources when this bundle can
/// install a signed update, the catalog otherwise.
fn initial_check_phase(
    bundle: crate::internal::hanafubuki_app_update::InstalledBundle,
) -> AppUpdateOperationPhase {
    if supports_signed_update(bundle) {
        AppUpdateOperationPhase::CheckingSignedSources
    } else {
        AppUpdateOperationPhase::CheckingCatalogSources
    }
}
