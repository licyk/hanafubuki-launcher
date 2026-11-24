use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::internal::hanafubuki_app_update::{
    CatalogFetchRequest, DownloadProgress, SignedCheckOutcome, SignedCheckRequest,
    SignedPrepareRequest, SignedUpdateMetadata, SourceError,
};

pub type AppUpdateFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait AppUpdateProgressSink: Send + Sync {
    fn progress(&self, progress: DownloadProgress);
}

pub trait PreparedUpdateInstaller: Send + Sync {
    fn install<'a>(&'a self) -> AppUpdateFuture<'a, Result<(), String>>;
}

pub struct PreparedSignedUpdate {
    pub metadata: SignedUpdateMetadata,
    pub installer: Arc<dyn PreparedUpdateInstaller>,
}

pub enum SignedPrepareOutcome {
    Prepared(PreparedSignedUpdate),
    VersionChanged(SignedUpdateMetadata),
}

pub trait SignedUpdatePort: Send + Sync {
    fn check<'a>(
        &'a self,
        request: SignedCheckRequest,
    ) -> AppUpdateFuture<'a, Result<SignedCheckOutcome, SourceError>>;

    fn prepare<'a>(
        &'a self,
        request: SignedPrepareRequest,
        progress: Arc<dyn AppUpdateProgressSink>,
    ) -> AppUpdateFuture<'a, Result<SignedPrepareOutcome, String>>;
}

pub trait ReleaseCatalogPort: Send + Sync {
    fn fetch<'a>(
        &'a self,
        request: CatalogFetchRequest,
    ) -> AppUpdateFuture<'a, Result<Vec<u8>, String>>;
}

pub trait AppUpdateIdentityPort: Send + Sync {
    fn next_id(&self) -> Result<String, String>;
    fn now_millis(&self) -> u128;
}

/// Bounds how long the service waits for a slower source once another answered.
pub trait AppUpdateTimerPort: Send + Sync {
    fn sleep<'a>(&'a self, duration: Duration) -> AppUpdateFuture<'a, ()>;
}
