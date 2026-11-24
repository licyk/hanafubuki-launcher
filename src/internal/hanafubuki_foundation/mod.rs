//! Domain-neutral primitives shared by backend crates.
//!
//! This crate deliberately contains no Tauri types, domain phase enums,
//! registries, filesystem repositories, or application orchestration.

pub mod background_worker;
pub mod blocking;
pub mod bounded_work_queue;
pub mod cancellation;
pub mod concurrency;
pub mod config_documentation;
pub mod environment;
pub mod form_schema;
pub mod json_normalization;
pub mod module_faults;
pub mod panic;
pub mod paths;
pub mod persistence;
pub mod readiness;
pub mod resolved;
pub mod terminal_text;
pub mod url_policy;
pub mod user_agent;
