//! What a declarative type adds to the environment of a process it runs (a
//! `run` step, a server, its terminal): its `[environment]` and, for a
//! program that is `{host.python}`, the launcher code's import path. The
//! consumer composes it in the order python-runtime.md sets: the variables
//! and `PATH` entries are caller overlays, before the managed composition;
//! the launcher `PYTHONPATH` comes last.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::PathBuf;

use crate::internal::hanafubuki_process::environment::native_environment_name_matches;
use crate::internal::hanafubuki_task::environment::merge_prepended_path_environment;

use crate::internal::hanafubuki_webui::runtime::normalize_pythonpath_environment;

/// The variables a `{host.python}` program runs with, over every other.
const HOST_PYTHON_VARS: [(&str, &str); 3] = [
    ("PYTHONUTF8", "1"),
    ("PYTHONUNBUFFERED", "1"),
    ("PYTHONNOUSERSITE", "1"),
];
/// The launcher code's `sitecustomize.py` starts the hotpatcher only when a
/// variable of this prefix is set; a `{host.python}` program never has one.
const HOTPATCHER_PREFIX: &str = "SD_WEBUI_ALL_IN_ONE_HOTPATCHER_";

/// One process's part of a declarative type's environment, resolved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessEnvironment {
    /// The type's `[environment].unset`: inherited variables the process
    /// does not get, each a name or a prefix ending in `*`.
    pub unset: Vec<String>,
    /// The type's `[environment].vars` under the process's own variables
    /// (a step's or a role's `env`, a backend plan's); the instance's
    /// variables go over these.
    pub vars: BTreeMap<String, String>,
    /// The type's `[environment].path`, before `PATH` in order.
    pub path: Vec<PathBuf>,
    /// The launcher code, when the program is `{host.python}`.
    pub launcher_code: Option<PathBuf>,
}

impl ProcessEnvironment {
    /// `compose`'s variables, and a removal (`None`) for each inherited
    /// variable its type unsets (`unset_inherited`).
    pub fn compose_over_inherited(
        &self,
        instance: &BTreeMap<String, String>,
        process_path: Option<&OsStr>,
        inherited: impl IntoIterator<Item = String>,
        managed: impl FnOnce(BTreeMap<String, String>) -> Result<BTreeMap<String, String>, String>,
    ) -> Result<BTreeMap<String, Option<String>>, String> {
        let composed = self.compose(instance, process_path, managed)?;
        let removed = self.unset_inherited(&composed, inherited);
        let mut env: BTreeMap<String, Option<String>> = composed
            .into_iter()
            .map(|(name, value)| (name, Some(value)))
            .collect();
        env.extend(removed.into_iter().map(|name| (name, None)));
        Ok(env)
    }

    /// The `inherited` variables `unset` names, exactly or by a prefix
    /// ending in `*`, that `composed` does not set: the type's own
    /// variables apply after its `unset`. A name without `*` is removed
    /// whether or not it is inherited, so no overlay applied before the
    /// caller's brings it back.
    pub fn unset_inherited(
        &self,
        composed: &BTreeMap<String, String>,
        inherited: impl IntoIterator<Item = String>,
    ) -> Vec<String> {
        let exact = self
            .unset
            .iter()
            .filter(|pattern| !pattern.ends_with('*'))
            .cloned();
        let mut removed: Vec<String> = Vec::new();
        for name in inherited
            .into_iter()
            .filter(|name| {
                self.unset
                    .iter()
                    .any(|pattern| native_environment_name_matches(name, pattern))
            })
            .chain(exact)
        {
            let set = composed
                .keys()
                .any(|set| native_environment_name_matches(set, &name));
            if !set && !removed.contains(&name) {
                removed.push(name);
            }
        }
        removed
    }

    /// The process's environment in python-runtime.md's order: its
    /// variables under `instance`'s, the type's `PATH` entries before the
    /// result's `PATH` (or `process_path` when it has none), then `managed`
    /// (the managed Python composition of a type that has one), then for a
    /// `{host.python}` program the launcher code first on `PYTHONPATH`, the
    /// Python variables, and no hotpatcher variable.
    pub fn compose(
        &self,
        instance: &BTreeMap<String, String>,
        process_path: Option<&OsStr>,
        managed: impl FnOnce(BTreeMap<String, String>) -> Result<BTreeMap<String, String>, String>,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut env = self.vars.clone();
        env.extend(instance.clone());
        if !self.path.is_empty() {
            env = merge_prepended_path_environment(env, self.path.iter().cloned(), process_path)?;
        }
        let mut env = managed(env)?;
        let Some(launcher_code) = &self.launcher_code else {
            return Ok(env);
        };
        env.retain(|name, _| !name.to_ascii_uppercase().starts_with(HOTPATCHER_PREFIX));
        for (name, value) in HOST_PYTHON_VARS {
            env.retain(|known, _| !known.eq_ignore_ascii_case(name));
            env.insert(name.to_string(), value.to_string());
        }
        normalize_pythonpath_environment(env.into_iter().collect(), launcher_code)
            .map(|env| env.into_iter().collect())
    }
}
