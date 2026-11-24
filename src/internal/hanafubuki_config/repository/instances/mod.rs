//! Per-instance configuration: typed structs, validation, persistence, and
//! layered resolution.
//!
//! Instance configuration is stored at
//! `data/instances/<id>/config.json`, beside the same instance identity's
//! lifecycle metadata. It is kept separate from global user settings (one
//! `config/<domain>.json` file per settings domain) and runtime state
//! (`state/`).
//!
//! The resolved configuration for an instance is produced here in Rust by
//! layering, from lowest to highest precedence:
//!
//! 1. Built-in defaults ([`AppSettings::defaults`]).
//! 2. Global user settings.
//! 3. Instance `settingsOverrides`.
//!
//! Layered settings resolution uses [`merge::merge_json_keep_null`] (recursive
//! object merge; arrays replaced wholesale). Keep-null means a `null` overlay is
//! a real value: nullable `AppSettings` fields (the custom mirror fields) can
//! be explicitly overridden to `null`, and every key stays present so the merged
//! JSON still deserializes into the typed `AppSettings`.
//!
//! Remove-null semantics ([`merge::merge_json_remove_null`]) are used only for
//! selected edit behavior in `update_instance_config_patch`, where a `null`
//! deletes a key (clearing an optional field or removing an `envVars` entry).
//! The `settingsOverrides` part of a patch is exempt from that and uses keep-null
//! so it can persist explicit `null` override values.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::internal::hanafubuki_foundation::environment::validate_native_environment;
use crate::internal::hanafubuki_foundation::paths::{normalize_path, validate_not_dangerous};
use serde_json::{Map, Value};

use crate::internal::hanafubuki_config::merge::{self};
use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;

use super::safe_file;
use super::settings::{self, AppSettings};
use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;

mod models;
mod mutation;
mod patch;
mod persisted;
mod query;
mod resolution;
mod store;
pub(crate) mod validation;

pub use models::{
    InstanceComponentPath, InstanceComponentPaths, InstanceConfig, InstanceQuickPath,
    LaunchArgumentOrigin, ResolvedInstanceConfig,
};
pub use mutation::{
    create_instance_config_inner, require_instance_config_revision_inner,
    synchronize_builtin_instance_launch_args_inner, update_instance_component_paths_inner,
    update_instance_config_patch_inner, update_instance_launch_args_inner,
    update_instance_location_inner, update_instance_name_inner, update_instance_quick_paths_inner,
};
pub use persisted::INSTANCE_COMPONENT_PATHS_CURRENT_JSON_SHAPE;
pub(crate) use persisted::{
    PersistedInstanceConfigDecoder, PersistedInstanceIssue, PersistedInstanceIssueKind,
};
pub use query::{get_instance_config_inner, list_instance_configs_inner};
pub use resolution::{get_resolved_instance_config_inner, resolve_effective_settings};
pub use validation::{InstanceQuickPathLimits, validate_instance_id};

use models::initial_config_revision;
use patch::apply_instance_patch;
use store::{load_instance, read_instance_configs_unlocked};
use validation::validate_instance_config;

#[cfg(test)]
mod tests;
