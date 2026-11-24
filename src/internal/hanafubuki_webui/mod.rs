//! WebUI installation and durable instance domain.
//!
//! Process lifecycle is available only through the install task port. This
//! crate has no Executor manager or mutation API dependency and no Tauri types.

pub mod discovery;
pub mod install;
pub mod instance_types;
pub mod instances;
pub mod runtime;

#[cfg(test)]
mod test_support;
