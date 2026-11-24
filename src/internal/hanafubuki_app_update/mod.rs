//! Tauri-free contracts for checking and installing Hanafubuki application updates.
//!
//! The domain owns source selection, version/catalog validation, offers, and operation state.
//! Concrete Tauri updater, HTTP, proxy, opener, IPC, and restart adapters live in the root crate.

mod catalog;
mod model;
mod ports;
mod registry;
mod service;
mod sources;

pub use catalog::{
    MAX_CATALOG_BYTES, ManualPackageCandidate, ValidatedCatalog, ValidatedCatalogArtifact,
    ValidatedCatalogVersion, decode_catalog, reconcile_catalogs, select_manual_packages,
};
pub use model::{
    AppArchitecture, AppPlatform, AppUpdateAvailabilitySnapshot, AppUpdateCheckResult,
    AppUpdateInstallResult, AppUpdateOperationPhase, AppUpdateOperationSnapshot,
    AppUpdateOperationStatus, CatalogFetchRequest, DownloadProgress, InstalledBundle,
    ManualPackage, ManualUpdateOffer, ManualUpdateReason, SignedCheckOutcome, SignedCheckRequest,
    SignedPrepareRequest, SignedUpdateMetadata, SignedUpdateOffer, SourceAttemptDiagnostic,
    SourceError, UpdateProvider, UpdateRequestContext, UpdateSource,
};
pub use ports::{
    AppUpdateFuture, AppUpdateIdentityPort, AppUpdateProgressSink, AppUpdateTimerPort,
    PreparedSignedUpdate, PreparedUpdateInstaller, ReleaseCatalogPort, SignedPrepareOutcome,
    SignedUpdatePort,
};
pub use registry::{
    AppUpdateRegistry, BeginOperation, ClaimPreparedInstall, PreparedInstallClaim,
    PreparedOfferCapability,
};
pub use service::{AppUpdateService, SOURCE_GRACE, supports_signed_update};
pub use sources::hanafubuki_update_sources;
