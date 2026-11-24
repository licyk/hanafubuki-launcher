//! A type its extension's backend provides (`provider = "backend"`): the
//! descriptor holds what is static (components, runtime, parameters,
//! servers, operations), and the backend checks an instance's values and
//! plans its install and its servers' commands. A plan comes back as the
//! descriptor's own step and command documents, placeholders and all, and
//! is checked and resolved exactly as a descriptor's are: the backend names
//! components, never a path, and a step that would leave its component is
//! refused.

use std::collections::BTreeMap;

use crate::internal::hanafubuki_instance_contracts::{InstanceTypeId, ServerRole};
use serde_json::{Map, Value};

use super::document::StepDocument;
use super::placeholders::{Scope, Values};
use super::validation::{self, Issues};
use super::{DeclarativeProvider, InstanceLocation, render_all, render_env};
use crate::internal::hanafubuki_webui::install::steps::{
    ProcessEnvironment, StepOperation, StepPlan,
};
use crate::internal::hanafubuki_webui::instance_types::registry::no_update_check;
use crate::internal::hanafubuki_webui::instances::InstanceSpec;

/// The backend of an extension, as the types it provides reach it. The
/// root implements it over the extension's running backend; each call is
/// bounded there, and fails with why when the backend is not running.
pub trait TypeBackend: Send + Sync {
    /// The problems with `values` the backend finds, by parameter; the
    /// instance is there when it exists (a reinstall with new values).
    fn validate_spec(
        &self,
        type_id: &InstanceTypeId,
        values: &Map<String, Value>,
        instance: Option<&InstanceLocation>,
    ) -> Result<BTreeMap<String, String>, String>;

    /// The steps of `operation` for the instance, as step documents.
    fn install_plan(
        &self,
        type_id: &InstanceTypeId,
        operation: StepOperation,
        values: &Map<String, Value>,
        instance: &InstanceLocation,
    ) -> Result<Vec<Value>, String>;

    /// The command `role` runs for the instance, listening on `port`.
    fn launch_plan(
        &self,
        type_id: &InstanceTypeId,
        role: &ServerRole,
        port: u16,
        values: &Map<String, Value>,
        instance: &InstanceLocation,
    ) -> Result<BackendCommand, String>;

    /// Whether the type's `update` would change what the instance has
    /// installed, as the backend finds it. Read-only: the instance may be
    /// running.
    fn check_update(
        &self,
        type_id: &InstanceTypeId,
        values: &Map<String, Value>,
        instance: &InstanceLocation,
    ) -> Result<UpdateCheck, String>;

    /// Whether the backend runs now. An extension's types register before
    /// its backend starts, so the daily check waits for this rather than
    /// record the gap as that day's failure.
    fn running(&self) -> bool {
        true
    }
}

/// A role's command as a backend plans it: placeholders allowed, as in a
/// declarative type's `command` and `env`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendCommand {
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// A backend's answer to an update check of one instance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateCheck {
    /// The version installed, as the user reads it.
    pub current: Option<String>,
    /// The version an update would install, as the user reads it.
    pub latest: Option<String>,
    /// Running the type's `update` would change what is installed.
    pub has_update: bool,
    pub notes: Option<String>,
}

impl DeclarativeProvider {
    /// What the type's backend answers on whether an update would change
    /// the instance; only a type that declares `update-check` asks it.
    pub(super) fn backend_update_check(
        &self,
        spec: &InstanceSpec,
        location: &InstanceLocation,
    ) -> Result<UpdateCheck, String> {
        if !self.descriptor.checks_updates {
            return Err(no_update_check(&self.descriptor.id));
        }
        let Some(backend) = self.backend()? else {
            return Err(no_update_check(&self.descriptor.id));
        };
        backend.check_update(&self.descriptor.id, &self.spec(spec)?.to_object(), location)
    }

    /// The backend's install plan, checked and resolved for `location`.
    pub(super) fn backend_steps(
        &self,
        planned: Vec<Value>,
        values: &Values<'_>,
        python_dir: Option<std::path::PathBuf>,
    ) -> Result<StepPlan, String> {
        let mut issues = Issues::default();
        let documents: Vec<StepDocument> = planned
            .into_iter()
            .enumerate()
            .filter_map(|(index, step)| {
                serde_json::from_value(step)
                    .map_err(|error| issues.add(format!("steps[{index}]"), error.to_string()))
                    .ok()
            })
            .collect();
        let names = self.parameter_names();
        let scope = Scope {
            components: &self.descriptor.components,
            parameters: &names,
            port: false,
        };
        let templates = validation::steps(
            &mut issues,
            "steps",
            documents,
            &scope,
            &self.descriptor.runtime,
        );
        planned_issues("install plan", issues)?;
        let steps = templates
            .iter()
            .enumerate()
            .map(|(index, step)| {
                self.plan_step(step, values)
                    .map_err(|error| format!("its backend's install plan: steps[{index}]: {error}"))
            })
            .collect::<Result<_, _>>()?;
        Ok(StepPlan { steps, python_dir })
    }

    /// The backend's command for a role, checked and resolved over the
    /// role's declared environment.
    pub(super) fn backend_command(
        &self,
        planned: BackendCommand,
        values: &Values<'_>,
        declared_env: BTreeMap<String, String>,
    ) -> Result<(Vec<String>, ProcessEnvironment), String> {
        let mut issues = Issues::default();
        let names = self.parameter_names();
        let scope = Scope {
            components: &self.descriptor.components,
            parameters: &names,
            port: true,
        };
        let argv = validation::argv(&mut issues, "argv", &planned.argv, &scope);
        let env = validation::environment(&mut issues, "env", planned.env, &scope);
        planned_issues("launch plan", issues)?;
        let (Some(argv), Some(env)) = (argv, env) else {
            return Err("its backend's launch plan does not read".to_string());
        };
        let mut rendered = declared_env;
        rendered.extend(render_env(&env, values)?);
        let env = self.process_environment(rendered, argv.first(), values)?;
        Ok((render_all(&argv, values)?, env))
    }

    fn parameter_names(&self) -> Vec<String> {
        self.parameters
            .iter()
            .map(|field| field.key.clone())
            .collect()
    }
}

fn planned_issues(plan: &str, issues: Issues) -> Result<(), String> {
    if issues.0.is_empty() {
        return Ok(());
    }
    let issues: Vec<String> = issues
        .0
        .iter()
        .map(|issue| format!("{}: {}", issue.location, issue.message))
        .collect();
    Err(format!(
        "its backend's {plan} is refused: {}",
        issues.join("; ")
    ))
}

#[cfg(test)]
mod tests;
