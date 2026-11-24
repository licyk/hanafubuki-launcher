//! Backend-owned configuration models, validation, and persistence.
//!
//! Tauri path resolution and application side effects stay in the application
//! crate. Callers provide a project-owned [`model::paths::AppPaths`] value.

pub mod merge;
pub mod model;
pub mod policy;
pub mod recovery;
pub mod repository;
mod state;

pub use model::paths::{AppPaths, BootstrapPaths, ManagedRuntimeLayoutKind, ManagedRuntimePaths};
pub use model::settings_domain::SettingsDomain;
pub use state::ConfigState;

#[cfg(test)]
mod test_support;
