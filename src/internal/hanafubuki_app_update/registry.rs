use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::internal::hanafubuki_app_update::{
    AppUpdateAvailabilitySnapshot, AppUpdateIdentityPort, AppUpdateOperationPhase,
    AppUpdateOperationSnapshot, AppUpdateOperationStatus, DownloadProgress, ManualPackage,
    ManualPackageCandidate, ManualUpdateOffer, ManualUpdateReason, PreparedUpdateInstaller,
    SignedUpdateMetadata, SignedUpdateOffer, UpdateRequestContext,
};

const MAX_TERMINAL_OPERATIONS: usize = 32;
const MAX_DIAGNOSTIC_BYTES: usize = 512;
/// The error of an activity whose operation ended before it settled.
const UNSETTLED_ACTIVITY_ERROR: &str = "the app update operation ended before it settled";

#[derive(Clone)]
pub struct PreparedOfferCapability {
    pub offer: SignedUpdateOffer,
    pub context: UpdateRequestContext,
    pub installer: Arc<dyn PreparedUpdateInstaller>,
}

#[derive(Clone, Debug)]
struct ManualOfferCapability {
    offer: ManualUpdateOffer,
    urls: HashMap<String, String>,
}

#[derive(Clone)]
enum RetainedOffer {
    Prepared(PreparedOfferCapability),
    Manual(ManualOfferCapability),
}

struct RegistryState {
    active_activity_id: Option<String>,
    operations: HashMap<String, AppUpdateOperationSnapshot>,
    operation_order: VecDeque<String>,
    offer: Option<RetainedOffer>,
    availability: AppUpdateAvailabilitySnapshot,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            active_activity_id: None,
            operations: HashMap::new(),
            operation_order: VecDeque::new(),
            offer: None,
            availability: AppUpdateAvailabilitySnapshot::None,
        }
    }
}

pub enum BeginOperation {
    Started(String),
    AlreadyRunning(String),
}

pub struct PreparedInstallClaim {
    pub activity_id: String,
    pub capability: PreparedOfferCapability,
}

pub enum ClaimPreparedInstall {
    Started(PreparedInstallClaim),
    AlreadyRunning(String),
}

pub struct AppUpdateRegistry {
    identity: Arc<dyn AppUpdateIdentityPort>,
    state: Mutex<RegistryState>,
}

impl AppUpdateRegistry {
    pub fn new(identity: Arc<dyn AppUpdateIdentityPort>) -> Self {
        Self {
            identity,
            state: Mutex::new(RegistryState::default()),
        }
    }

    fn next_id(&self, label: &str) -> Result<String, String> {
        let id = self.identity.next_id()?;
        if id.is_empty() || id.len() > 128 || id.chars().any(char::is_whitespace) {
            return Err(format!(
                "{label} identity generator returned an invalid value"
            ));
        }
        Ok(id)
    }

    pub fn begin(&self, phase: AppUpdateOperationPhase) -> Result<BeginOperation, String> {
        let mut state = self.state.lock();
        if let Some(activity_id) = &state.active_activity_id {
            return Ok(BeginOperation::AlreadyRunning(activity_id.clone()));
        }
        let activity_id = format!("app-update:{}", self.next_id("activity")?);
        let now = self.identity.now_millis();
        state.operations.insert(
            activity_id.clone(),
            AppUpdateOperationSnapshot {
                activity_id: activity_id.clone(),
                phase,
                status: AppUpdateOperationStatus::Running,
                created_at: now,
                updated_at: now,
                completed_bytes: 0,
                total_bytes: None,
                error: None,
                warnings: Vec::new(),
            },
        );
        state.active_activity_id = Some(activity_id.clone());
        Ok(BeginOperation::Started(activity_id))
    }

    pub fn record_handoff_completion(&self, attempted_at: u128) -> Result<String, String> {
        let activity_id = format!("app-update:{}", self.next_id("activity")?);
        let snapshot = AppUpdateOperationSnapshot {
            activity_id: activity_id.clone(),
            phase: AppUpdateOperationPhase::Installed,
            status: AppUpdateOperationStatus::Completed,
            created_at: attempted_at,
            updated_at: self.identity.now_millis(),
            completed_bytes: 0,
            total_bytes: None,
            error: None,
            warnings: Vec::new(),
        };
        let mut state = self.state.lock();
        state.operations.insert(activity_id.clone(), snapshot);
        state.operation_order.push_back(activity_id.clone());
        while state.operation_order.len() > MAX_TERMINAL_OPERATIONS {
            if let Some(expired) = state.operation_order.pop_front() {
                state.operations.remove(&expired);
            }
        }
        Ok(activity_id)
    }

    pub fn set_phase(
        &self,
        activity_id: &str,
        phase: AppUpdateOperationPhase,
    ) -> Result<(), String> {
        let mut state = self.state.lock();
        let operation = state
            .operations
            .get_mut(activity_id)
            .ok_or_else(|| format!("unknown app update activity: {activity_id}"))?;
        operation.phase = phase;
        operation.updated_at = self.identity.now_millis();
        Ok(())
    }

    pub fn progress(&self, activity_id: &str, progress: DownloadProgress) -> Result<(), String> {
        let mut state = self.state.lock();
        let operation = state
            .operations
            .get_mut(activity_id)
            .ok_or_else(|| format!("unknown app update activity: {activity_id}"))?;
        if progress.completed_bytes < operation.completed_bytes {
            return Err("app update progress must be monotonic".into());
        }
        if let Some(total) = progress.total_bytes {
            if total < progress.completed_bytes {
                return Err("app update progress exceeds total bytes".into());
            }
            if operation
                .total_bytes
                .is_some_and(|previous| total < previous)
            {
                return Err("app update total bytes must not decrease".into());
            }
        }
        operation.completed_bytes = progress.completed_bytes;
        operation.total_bytes = progress.total_bytes.or(operation.total_bytes);
        operation.updated_at = self.identity.now_millis();
        Ok(())
    }

    pub fn finish(
        &self,
        activity_id: &str,
        phase: AppUpdateOperationPhase,
        status: AppUpdateOperationStatus,
        error: Option<String>,
        warnings: Vec<String>,
    ) -> Result<(), String> {
        let mut state = self.state.lock();
        if state.active_activity_id.as_deref() != Some(activity_id) {
            return Err(format!("app update activity is not active: {activity_id}"));
        }
        self.settle_active(&mut state, activity_id, phase, status, error, warnings)
    }

    fn settle_active(
        &self,
        state: &mut RegistryState,
        activity_id: &str,
        phase: AppUpdateOperationPhase,
        status: AppUpdateOperationStatus,
        error: Option<String>,
        warnings: Vec<String>,
    ) -> Result<(), String> {
        let now = self.identity.now_millis();
        let operation = state
            .operations
            .get_mut(activity_id)
            .ok_or_else(|| format!("unknown app update activity: {activity_id}"))?;
        operation.phase = phase;
        operation.status = status;
        operation.error = error.map(|value| bounded_diagnostic(&value));
        operation.warnings = warnings
            .into_iter()
            .map(|value| bounded_diagnostic(&value))
            .take(8)
            .collect();
        operation.updated_at = now;
        state.active_activity_id = None;
        state.operation_order.push_back(activity_id.to_string());
        while state.operation_order.len() > MAX_TERMINAL_OPERATIONS {
            if let Some(expired) = state.operation_order.pop_front() {
                state.operations.remove(&expired);
            }
        }
        Ok(())
    }

    pub fn snapshot(&self, activity_id: &str) -> Option<AppUpdateOperationSnapshot> {
        self.state.lock().operations.get(activity_id).cloned()
    }

    pub fn snapshots(&self) -> Vec<AppUpdateOperationSnapshot> {
        let state = self.state.lock();
        let mut values = state.operations.values().cloned().collect::<Vec<_>>();
        values.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| left.activity_id.cmp(&right.activity_id))
        });
        values
    }

    pub fn retain_prepared(
        &self,
        current_version: &str,
        metadata: SignedUpdateMetadata,
        context: UpdateRequestContext,
        installer: Arc<dyn PreparedUpdateInstaller>,
    ) -> Result<SignedUpdateOffer, String> {
        let offer = SignedUpdateOffer {
            offer_id: format!("signed-offer:{}", self.next_id("offer")?),
            current_version: current_version.to_string(),
            version: metadata.version.clone(),
            provider: metadata.provider,
            published_at: metadata.published_at,
            notes: metadata.notes,
        };
        let mut state = self.state.lock();
        state.offer = Some(RetainedOffer::Prepared(PreparedOfferCapability {
            offer: offer.clone(),
            context,
            installer,
        }));
        state.availability = AppUpdateAvailabilitySnapshot::ReadyToInstall {
            offer: offer.clone(),
        };
        Ok(offer)
    }

    pub fn claim_prepared_install(
        &self,
        offer_id: Option<&str>,
    ) -> Result<ClaimPreparedInstall, String> {
        let mut state = self.state.lock();
        if let Some(activity_id) = &state.active_activity_id {
            return Ok(ClaimPreparedInstall::AlreadyRunning(activity_id.clone()));
        }
        let claimable = matches!(
            &state.offer,
            Some(RetainedOffer::Prepared(capability))
                if offer_id.is_none_or(|expected| capability.offer.offer_id == expected)
        );
        if !claimable {
            return Err("prepared update is unknown, stale, or already consumed".into());
        }
        // Identified before the offer is taken: a failing identity leaves the
        // offer retained for the next attempt.
        let activity_id = format!("app-update:{}", self.next_id("activity")?);
        let now = self.identity.now_millis();
        match state.offer.take() {
            Some(RetainedOffer::Prepared(capability)) => {
                state.operations.insert(
                    activity_id.clone(),
                    AppUpdateOperationSnapshot {
                        activity_id: activity_id.clone(),
                        phase: AppUpdateOperationPhase::Installing,
                        status: AppUpdateOperationStatus::Running,
                        created_at: now,
                        updated_at: now,
                        completed_bytes: 0,
                        total_bytes: None,
                        error: None,
                        warnings: Vec::new(),
                    },
                );
                state.active_activity_id = Some(activity_id.clone());
                state.availability = AppUpdateAvailabilitySnapshot::Installing {
                    version: capability.offer.version.clone(),
                };
                Ok(ClaimPreparedInstall::Started(PreparedInstallClaim {
                    activity_id,
                    capability,
                }))
            }
            retained => {
                state.offer = retained;
                Err("prepared update is unknown, stale, or already consumed".into())
            }
        }
    }

    /// Fail `activity_id` if it is still the active activity: the operation
    /// that started it ended (its future was dropped, or it panicked) before
    /// it settled. An install that ended this way gives its consumed offer's
    /// `Installing` availability up. A settled activity is left as it was.
    pub fn fail_unsettled(&self, activity_id: &str) {
        let mut state = self.state.lock();
        if state.active_activity_id.as_deref() != Some(activity_id) {
            return;
        }
        log::warn!("app update activity {activity_id} ended before it settled");
        if matches!(
            state.availability,
            AppUpdateAvailabilitySnapshot::Installing { .. }
        ) {
            state.availability = AppUpdateAvailabilitySnapshot::None;
        }
        let _ = self.settle_active(
            &mut state,
            activity_id,
            AppUpdateOperationPhase::Failed,
            AppUpdateOperationStatus::Failed,
            Some(UNSETTLED_ACTIVITY_ERROR.to_string()),
            Vec::new(),
        );
    }

    pub fn availability(&self) -> AppUpdateAvailabilitySnapshot {
        self.state.lock().availability.clone()
    }

    pub fn clear_availability(&self) {
        let mut state = self.state.lock();
        state.offer = None;
        state.availability = AppUpdateAvailabilitySnapshot::None;
    }

    pub fn retain_manual(
        &self,
        current_version: &str,
        version: &str,
        candidates: Vec<ManualPackageCandidate>,
    ) -> Result<ManualUpdateOffer, String> {
        self.retain_manual_with_reason(
            current_version,
            version,
            candidates,
            ManualUpdateReason::SignedUpdateUnavailable,
        )
    }

    pub fn retain_manual_with_reason(
        &self,
        current_version: &str,
        version: &str,
        candidates: Vec<ManualPackageCandidate>,
        reason: ManualUpdateReason,
    ) -> Result<ManualUpdateOffer, String> {
        let offer_id = format!("manual-offer:{}", self.next_id("offer")?);
        let mut urls = HashMap::new();
        let mut packages = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let package_id = format!("manual-package:{}", self.next_id("package")?);
            urls.insert(package_id.clone(), candidate.url);
            packages.push(ManualPackage {
                package_id,
                provider: candidate.provider,
                kind: candidate.kind,
                file_name: candidate.file_name,
                size: candidate.size,
            });
        }
        let first = packages
            .first()
            .ok_or("manual update offer requires at least one package")?;
        let offer = ManualUpdateOffer {
            offer_id,
            current_version: current_version.to_string(),
            version: version.to_string(),
            provider: first.provider,
            recommended_package_id: first.package_id.clone(),
            packages,
        };
        let mut state = self.state.lock();
        state.offer = Some(RetainedOffer::Manual(ManualOfferCapability {
            offer: offer.clone(),
            urls,
        }));
        state.availability = AppUpdateAvailabilitySnapshot::ManualDownloadRequired {
            offer: offer.clone(),
            reason,
        };
        Ok(offer)
    }

    pub fn resolve_manual_url(&self, offer_id: &str, package_id: &str) -> Result<String, String> {
        let state = self.state.lock();
        match state.offer.as_ref() {
            Some(RetainedOffer::Manual(capability)) if capability.offer.offer_id == offer_id => {
                capability
                    .urls
                    .get(package_id)
                    .cloned()
                    .ok_or_else(|| "manual package identity is unknown for this offer".into())
            }
            _ => Err("manual update offer is unknown or stale".into()),
        }
    }
}

fn bounded_diagnostic(value: &str) -> String {
    value.chars().take(MAX_DIAGNOSTIC_BYTES).collect()
}
