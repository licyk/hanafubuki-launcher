//! Process execution and ownership infrastructure.
//!
//! The crate owns native environment validation, process-tree supervision,
//! Executor sessions, and unforgeable ownership capabilities. It contains no
//! Tauri command adapters or application-domain state machines.

pub mod da_filter;
pub mod environment;
pub mod executor;
pub mod ownership;
pub mod process_groups;
pub mod terminal_size;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod tree;
