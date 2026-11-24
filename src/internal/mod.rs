//! The code the launcher used to import as workspace crates, now modules of this crate.
//!
//! Each module keeps the module tree its crate had, re-rooted at its own name, so the only
//! change the port made is the path prefix: `hanafubuki_foundation::paths` is
//! `crate::internal::hanafubuki_foundation::paths`. Item paths inside a module tree are
//! unchanged as well, because each was rewritten from the crate root it was written against.
//!
//! Dependencies point down this list; each module only reaches the ones before it, as the
//! crates did.

pub mod hanafubuki_app_update;
pub mod hanafubuki_config;
pub mod hanafubuki_foundation;
pub mod hanafubuki_fs_transfer;
pub mod hanafubuki_instance_contracts;
pub mod hanafubuki_process;
pub mod hanafubuki_python_runtime;
pub mod hanafubuki_runtime_storage;
pub mod hanafubuki_task;
pub mod hanafubuki_webui;
