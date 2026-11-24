//! The shape every runtime readiness service has (`docs/rules/modules.md`,
//! Runtime readiness).
//!
//! A readiness service prepares one runtime the application depends on (the
//! application's Python runtime, micromamba, Git, the proxy, the App
//! backend) on workers of its own and publishes what it is doing as a
//! snapshot: [`Readiness`] is what its module needs from it (its snapshot,
//! starting its background preparation when the module mounts, stopping its
//! workers when the module stops), and [`ReadinessEvents`] is where it
//! publishes every snapshot it moves to. The events are an input of its
//! construction, never set afterwards. How it retries is its own.

use std::time::Instant;

use crate::internal::hanafubuki_foundation::background_worker::WorkerShutdownDiagnostic;

/// A runtime readiness service as its module runs it.
pub trait Readiness: Send + Sync + 'static {
    /// What it publishes and its status command returns.
    type Snapshot: Clone + Send + Sync + 'static;

    /// The state as of now.
    fn snapshot(&self) -> Self::Snapshot;

    /// Starts preparing in the background unless it is preparing or ready
    /// already; the module's mount calls it.
    fn start_background(&self);

    /// Stops its workers by `deadline`, reporting each one that did not stop
    /// cleanly; the module's stop calls it.
    fn shutdown_workers(&self, deadline: Instant) -> Vec<WorkerShutdownDiagnostic>;
}

/// Where a readiness service publishes every snapshot it moves to, on the
/// thread that moved it. It must not block and must not call back into the
/// service under the service's own lock (it is called outside it).
pub trait ReadinessEvents<S>: Send + Sync {
    fn emit(&self, snapshot: &S);
}

/// Events nobody hears: for a service whose snapshots only its own callers
/// read.
pub struct NoReadinessEvents;

impl<S> ReadinessEvents<S> for NoReadinessEvents {
    fn emit(&self, _snapshot: &S) {}
}
