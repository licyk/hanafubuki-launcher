//! A service its holders resolve for every call.
//!
//! A module's service is a new instance on every mount, so what is not built
//! over the module holds a [`Resolved`] and asks it for the service each time
//! it needs one: an error while the module is not available, never a stale
//! instance of an earlier mount.

use std::sync::Arc;

type Resolve<S> = dyn Fn() -> Result<Arc<S>, String> + Send + Sync;

pub struct Resolved<S: ?Sized>(Arc<Resolve<S>>);

impl<S: ?Sized> Resolved<S> {
    pub fn new(resolve: impl Fn() -> Result<Arc<S>, String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(resolve))
    }

    /// Always `service`: for a holder whose service outlives it, and tests.
    pub fn fixed(service: Arc<S>) -> Self
    where
        S: Send + Sync + 'static,
    {
        Self::new(move || Ok(service.clone()))
    }

    /// The service as of now; why it is not available otherwise.
    pub fn get(&self) -> Result<Arc<S>, String> {
        (self.0)()
    }
}

impl<S: ?Sized> Clone for Resolved<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
