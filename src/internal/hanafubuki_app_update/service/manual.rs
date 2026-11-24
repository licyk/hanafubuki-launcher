//! The manual-download fallback built from the release catalogs.

use super::settle::grace_expired;
use super::{AppUpdateService, diagnostic_detail, provider_label, stable_version};
use crate::internal::hanafubuki_app_update::{
    AppUpdateCheckResult, AppUpdateInstallResult, AppUpdateOperationPhase,
    AppUpdateOperationStatus, CatalogFetchRequest, ManualUpdateReason, UpdateProvider,
    UpdateRequestContext, ValidatedCatalog, decode_catalog, reconcile_catalogs,
    select_manual_packages,
};

impl AppUpdateService {
    pub(super) async fn manual_result(
        &self,
        activity_id: &str,
        context: &UpdateRequestContext,
        warnings: Vec<String>,
        unreachable: &[UpdateProvider],
        reason: ManualUpdateReason,
    ) -> AppUpdateCheckResult {
        match self
            .manual_fallback(activity_id, context, warnings, unreachable, reason)
            .await
        {
            ManualFallback::UpToDate(warnings) => {
                self.registry.clear_availability();
                let _ = self.registry.finish(
                    activity_id,
                    AppUpdateOperationPhase::Completed,
                    AppUpdateOperationStatus::Completed,
                    None,
                    warnings,
                );
                AppUpdateCheckResult::UpToDate {
                    activity_id: activity_id.to_string(),
                    current_version: context.current_version.clone(),
                }
            }
            ManualFallback::Offer(offer, warnings, reason) => {
                let _ = self.registry.finish(
                    activity_id,
                    AppUpdateOperationPhase::ReadyToInstall,
                    AppUpdateOperationStatus::Completed,
                    None,
                    warnings,
                );
                AppUpdateCheckResult::ManualDownloadRequired {
                    activity_id: activity_id.to_string(),
                    offer,
                    reason,
                }
            }
            ManualFallback::Unavailable(error, warnings) => {
                self.registry.clear_availability();
                self.finish_failed(activity_id, &error, warnings);
                AppUpdateCheckResult::Unavailable {
                    activity_id: activity_id.to_string(),
                    current_version: context.current_version.clone(),
                    error,
                }
            }
        }
    }

    pub(super) async fn manual_install_result(
        &self,
        activity_id: &str,
        context: &UpdateRequestContext,
        warnings: Vec<String>,
        reason: ManualUpdateReason,
    ) -> AppUpdateInstallResult {
        match self
            .manual_fallback(activity_id, context, warnings, &[], reason)
            .await
        {
            ManualFallback::Offer(offer, warnings, reason) => {
                let _ = self.registry.finish(
                    activity_id,
                    AppUpdateOperationPhase::ReadyToInstall,
                    AppUpdateOperationStatus::Completed,
                    None,
                    warnings,
                );
                AppUpdateInstallResult::ManualDownloadRequired {
                    activity_id: activity_id.to_string(),
                    offer,
                    reason,
                }
            }
            ManualFallback::UpToDate(warnings) => {
                let error =
                    "automatic installation failed and no newer manual release is available"
                        .to_string();
                self.registry.clear_availability();
                self.finish_failed(activity_id, &error, warnings);
                AppUpdateInstallResult::Unavailable {
                    activity_id: activity_id.to_string(),
                    error,
                }
            }
            ManualFallback::Unavailable(error, warnings) => {
                self.registry.clear_availability();
                self.finish_failed(activity_id, &error, warnings);
                AppUpdateInstallResult::Unavailable {
                    activity_id: activity_id.to_string(),
                    error,
                }
            }
        }
    }

    /// Catalog sources in `unreachable` already failed to answer during this
    /// operation and are skipped rather than waited on a second time.
    async fn manual_fallback(
        &self,
        activity_id: &str,
        context: &UpdateRequestContext,
        mut warnings: Vec<String>,
        unreachable: &[UpdateProvider],
        reason: ManualUpdateReason,
    ) -> ManualFallback {
        let _ = self
            .registry
            .set_phase(activity_id, AppUpdateOperationPhase::CheckingCatalogSources);
        let [left_request, right_request] = self.sources.clone().map(|source| {
            (!unreachable.contains(&source.provider)).then(|| CatalogFetchRequest {
                source,
                proxy: context.proxy.clone(),
            })
        });
        let (left, right) = match (left_request, right_request) {
            (Some(left), Some(right)) => {
                let (left, right) = self
                    .settle_both(
                        self.catalogs.fetch(left),
                        self.catalogs.fetch(right),
                        grace_expired,
                    )
                    .await;
                (Some(left), Some(right))
            }
            (Some(left), None) => (Some(self.catalogs.fetch(left).await), None),
            (None, Some(right)) => (None, Some(self.catalogs.fetch(right).await)),
            (None, None) => (None, None),
        };
        let left =
            left.map(|result| decode_fetched_catalog(self.sources[0].provider, result, context));
        let right =
            right.map(|result| decode_fetched_catalog(self.sources[1].provider, result, context));
        let catalog_errors = [left.as_ref(), right.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(|result| result.as_ref().err())
            .cloned()
            .collect::<Vec<_>>();
        warnings.extend(catalog_errors.iter().cloned());
        let selected =
            match reconcile_catalogs(left.and_then(Result::ok), right.and_then(Result::ok)) {
                Ok(Some(catalog)) => catalog,
                Ok(None) if catalog_errors.is_empty() => {
                    return ManualFallback::Unavailable(
                        "no update source could be reached".into(),
                        warnings,
                    );
                }
                Ok(None) => {
                    return ManualFallback::Unavailable(
                        format!(
                            "manual update fallback could not use either catalog source: {}",
                            catalog_errors.join("; ")
                        ),
                        warnings,
                    );
                }
                Err(error) => return ManualFallback::Unavailable(error, warnings),
            };
        let current = match stable_version(&context.current_version) {
            Ok(version) => version,
            Err(error) => return ManualFallback::Unavailable(error, warnings),
        };
        let Some(latest) = selected.versions.first() else {
            return ManualFallback::UpToDate(warnings);
        };
        if latest.version <= current {
            return ManualFallback::UpToDate(warnings);
        }
        let candidates = match select_manual_packages(&selected, context) {
            Ok(candidates) => candidates,
            Err(error) => return ManualFallback::Unavailable(error, warnings),
        };
        match self.registry.retain_manual_with_reason(
            &context.current_version,
            &latest.version.to_string(),
            candidates,
            reason,
        ) {
            Ok(offer) => ManualFallback::Offer(offer, warnings, reason),
            Err(error) => ManualFallback::Unavailable(error, warnings),
        }
    }
}

enum ManualFallback {
    UpToDate(Vec<String>),
    Offer(
        crate::internal::hanafubuki_app_update::ManualUpdateOffer,
        Vec<String>,
        ManualUpdateReason,
    ),
    Unavailable(String, Vec<String>),
}

fn decode_fetched_catalog(
    provider: UpdateProvider,
    result: Result<Vec<u8>, String>,
    context: &UpdateRequestContext,
) -> Result<ValidatedCatalog, String> {
    match result {
        Ok(bytes) => decode_catalog(
            provider,
            &bytes,
            &context.product_name,
            &context.identifier,
            context.platform,
            context.architecture,
        )
        .map_err(|error| {
            format!(
                "{} update catalog was invalid: {}",
                provider_label(provider),
                diagnostic_detail(&error)
            )
        }),
        Err(error) => Err(format!(
            "{} update catalog request failed: {}",
            provider_label(provider),
            diagnostic_detail(&error)
        )),
    }
}
