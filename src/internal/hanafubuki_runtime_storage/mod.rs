//! Durable relocation of the application-owned Python, Micromamba, Conda and
//! launcher-code storage. This crate is Tauri-free: callers supply resolved
//! bootstrap/protected paths and run scheduled work on the next start, before
//! resolving application paths.

mod model;
mod operations;
mod repository;
mod service;
mod transfer;

pub use model::{
    ManagedRuntimeStorageSnapshot, MigrationComponentTarget, MigrationPhase, RuntimeStorageConfig,
    RuntimeStorageConfigAttention, RuntimeStorageConfigProblem, RuntimeStorageJournal,
    RuntimeStorageMigrationPlan,
};
pub use operations::ManagedRuntimeStorageOperations;
pub use service::ManagedRuntimeStorageService;
