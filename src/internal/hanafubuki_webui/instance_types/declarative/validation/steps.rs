//! The checks of what a type's processes run: its steps, its commands and
//! their variables, and its `[environment]`. A backend's plans are checked
//! with these too (`super::super::backend`).

use std::collections::BTreeMap;
use std::time::Duration;

use crate::internal::hanafubuki_instance_contracts::InstanceRuntime;

use super::super::document::{EnvironmentDocument, StepDocument};
use super::super::placeholders::{PathEntryTemplate, PathTemplate, Scope, Template};
use super::super::{EnvironmentTemplate, StepTemplate, TargetTemplate};
use super::Issues;

/// Each step of a list, checked at `<location>[<index>]`.
pub(in super::super) fn steps(
    issues: &mut Issues,
    location: &str,
    declared: Vec<StepDocument>,
    scope: &Scope<'_>,
    runtime: &InstanceRuntime,
) -> Vec<StepTemplate> {
    let mut steps = Vec::new();
    for (index, step) in declared.into_iter().enumerate() {
        let location = format!("{location}[{index}]");
        if let Some(step) = step_template(issues, &location, step, scope, runtime) {
            steps.push(step);
        }
    }
    steps
}

fn step_template(
    issues: &mut Issues,
    location: &str,
    step: StepDocument,
    scope: &Scope<'_>,
    runtime: &InstanceRuntime,
) -> Option<StepTemplate> {
    Some(match step {
        StepDocument::Download {
            url,
            sha256,
            to,
            unpack_to,
            strip,
            replace,
        } => {
            let url = template(issues, location, "url", &url, scope);
            let digest = template(issues, location, "sha256", &sha256, scope);
            if digest.as_ref().is_some_and(|digest| !digest.names_param()) && !is_sha256(&sha256) {
                issues.add(
                    format!("{location}.sha256"),
                    super::super::documentation::DIGEST.message,
                );
            }
            let target = match (to, unpack_to) {
                (Some(_), None) if strip > 0 => {
                    issues.add(
                        format!("{location}.strip"),
                        super::super::documentation::STRIP.message,
                    );
                    None
                }
                (Some(_), None) if replace => {
                    issues.add(
                        format!("{location}.replace"),
                        super::super::documentation::REPLACE.message,
                    );
                    None
                }
                (Some(to), None) => {
                    path(issues, location, "to", &to, scope).map(TargetTemplate::File)
                }
                (None, Some(into)) => {
                    path(issues, location, "unpackTo", &into, scope).map(|into| {
                        TargetTemplate::UnpackInto {
                            into,
                            strip,
                            replace,
                        }
                    })
                }
                _ => {
                    issues.add(
                        location,
                        super::super::documentation::DOWNLOAD_TARGET.message,
                    );
                    None
                }
            };
            StepTemplate::Download {
                url: url?,
                sha256: digest?,
                target: target?,
            }
        }
        StepDocument::Unpack {
            archive,
            into,
            strip,
            replace,
        } => {
            let archive = path(issues, location, "archive", &archive, scope);
            let into = path(issues, location, "into", &into, scope);
            StepTemplate::Unpack {
                archive: archive?,
                into: into?,
                strip,
                replace,
            }
        }
        StepDocument::GitClone { url, into, branch } => {
            let url = template(issues, location, "url", &url, scope);
            let branch = branch.map(|branch| template(issues, location, "branch", &branch, scope));
            let into = path(issues, location, "into", &into, scope);
            StepTemplate::GitClone {
                url: url?,
                into: into?,
                branch: branch.map(|branch| branch.ok_or(())).transpose().ok()?,
            }
        }
        StepDocument::GitPull { dir } => StepTemplate::GitPull {
            repository: path(issues, location, "dir", &dir, scope)?,
        },
        StepDocument::Run { command, cwd, env } => {
            let command = argv(issues, &format!("{location}.command"), &command, scope);
            let env = environment(issues, &format!("{location}.env"), env, scope);
            let cwd = path(
                issues,
                location,
                "cwd",
                cwd.as_deref().unwrap_or("{component.core}"),
                scope,
            );
            StepTemplate::Run {
                command: command?,
                cwd: cwd?,
                env: env?,
            }
        }
        StepDocument::PythonEnv {} if runtime.is_python() => StepTemplate::PythonEnv,
        StepDocument::PythonEnv {} => {
            issues.add(location, super::super::documentation::PYTHON_ENV.message);
            return None;
        }
        StepDocument::WriteFile {
            path: file,
            content,
        } => {
            let content = template(issues, location, "content", &content, scope);
            let file = path(issues, location, "path", &file, scope);
            StepTemplate::WriteFile {
                path: file?,
                content: content?,
            }
        }
        StepDocument::Probe {
            url,
            timeout_seconds,
        } => StepTemplate::Probe {
            url: template(issues, location, "url", &url, scope)?,
            timeout: Duration::from_secs(timeout_seconds.max(1)),
        },
    })
}

fn template(
    issues: &mut Issues,
    location: &str,
    field: &str,
    text: &str,
    scope: &Scope<'_>,
) -> Option<Template> {
    issues.take(format!("{location}.{field}"), Template::parse(text, scope))
}

fn path(
    issues: &mut Issues,
    location: &str,
    field: &str,
    text: &str,
    scope: &Scope<'_>,
) -> Option<PathTemplate> {
    issues.take(
        format!("{location}.{field}"),
        PathTemplate::parse(text, scope),
    )
}

/// A command: a program, then its arguments, each one argument.
pub(in super::super) fn argv(
    issues: &mut Issues,
    location: &str,
    argv: &[String],
    scope: &Scope<'_>,
) -> Option<Vec<Template>> {
    if argv.first().is_none_or(|program| program.trim().is_empty()) {
        issues.add(location, "a command names its program first");
        return None;
    }
    let parsed: Vec<Option<Template>> = argv
        .iter()
        .enumerate()
        .map(|(index, arg)| {
            issues.take(format!("{location}[{index}]"), Template::parse(arg, scope))
        })
        .collect();
    parsed.into_iter().collect()
}

pub(in super::super) fn environment(
    issues: &mut Issues,
    location: &str,
    env: BTreeMap<String, String>,
    scope: &Scope<'_>,
) -> Option<BTreeMap<String, Template>> {
    let mut parsed = BTreeMap::new();
    let mut valid = true;
    for (name, value) in env {
        if name.is_empty() || name.contains(['=', '\0']) {
            issues.add(location, format!("`{name}` is not a variable name"));
            valid = false;
            continue;
        }
        match issues.take(format!("{location}.{name}"), Template::parse(&value, scope)) {
            Some(value) => {
                parsed.insert(name, value);
            }
            None => valid = false,
        }
    }
    valid.then_some(parsed)
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The type's `[environment]`: names to unset (a trailing `*` makes a
/// prefix), `PATH` entries that start in a component or the instance root,
/// and variables that do not know a port.
pub(super) fn type_environment(
    issues: &mut Issues,
    environment: EnvironmentDocument,
    scope: &Scope<'_>,
) -> EnvironmentTemplate {
    for (index, name) in environment.unset.iter().enumerate() {
        let exact = name.strip_suffix('*').unwrap_or(name);
        if exact.is_empty() || exact.contains(['=', '\0', '*']) {
            issues.add(
                format!("environment.unset[{index}]"),
                format!("`{name}` is not a variable name, or a prefix of one ending in `*`"),
            );
        }
    }
    let path = environment
        .path
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            issues.take(
                format!("environment.path[{index}]"),
                PathEntryTemplate::parse(entry, scope),
            )
        })
        .collect();
    let vars =
        self::environment(issues, "environment.vars", environment.vars, scope).unwrap_or_default();
    EnvironmentTemplate {
        unset: environment.unset,
        path,
        vars,
    }
}
