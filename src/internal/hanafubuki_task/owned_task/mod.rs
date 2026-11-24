//! Shared Task identity, metadata, receipt, and Executor routing mechanisms.

mod executor;
mod model;
mod registry;

pub(crate) use executor::OwnedTaskExecutorAdapter;
pub(crate) use model::OwnedTaskRecord;
pub(crate) use registry::OwnedTaskRegistry;
