//! The checks of a type's components: names of `[a-z0-9-]`, unique, `core`
//! among them, their labels, and which of them may be a folder of the
//! user's own (`external = "allowed"`) holding which program.

use std::collections::BTreeMap;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_foundation::form_schema::SchemaText;
use crate::internal::hanafubuki_instance_contracts::ExternalComponent;

use super::super::document::{ComponentDocument, ExecutableDocument};
use super::super::parameters::text;
use super::Issues;
use crate::internal::hanafubuki_webui::discovery::RelativePath;

/// The components a type declares, checked.
pub(super) struct Declared {
    /// `core` first.
    pub names: Vec<String>,
    pub labels: BTreeMap<String, SchemaText>,
    pub external: BTreeMap<String, ExternalComponent>,
}

pub(super) fn declared(
    issues: &mut Issues,
    namespace: &str,
    declared: Vec<ComponentDocument>,
) -> Declared {
    let mut labels = BTreeMap::new();
    let mut external = BTreeMap::new();
    let declared: Vec<String> = declared
        .into_iter()
        .enumerate()
        .map(|(index, component)| match component {
            ComponentDocument::Name(name) => name,
            ComponentDocument::Table {
                name,
                label,
                external: allowed,
                executable,
            } => {
                let location = format!("components[{index}]");
                if let Some(label) = label.and_then(|label| {
                    issues.take(
                        format!("{location}.label"),
                        text(label).scoped_to(namespace),
                    )
                }) {
                    labels.insert(name.clone(), label);
                }
                if let Some(component) =
                    external_component(issues, &location, &name, allowed, executable)
                {
                    external.insert(name.clone(), component);
                }
                name
            }
        })
        .collect();
    let mut names = vec![InstanceComponentPaths::CORE.to_string()];
    for (index, name) in declared.iter().enumerate() {
        let location = format!("components[{index}]");
        let valid = !name.is_empty()
            && name
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'));
        if !valid {
            issues.add(
                location,
                format!("component `{name}` is not a name of [a-z0-9-]"),
            );
        } else if name == InstanceComponentPaths::CORE {
            continue;
        } else if names.contains(name) {
            issues.add(location, format!("component `{name}` is declared twice"));
        } else {
            names.push(name.clone());
        }
    }
    if !declared
        .iter()
        .any(|name| name == InstanceComponentPaths::CORE)
    {
        issues.add("components", super::super::documentation::CORE.message);
    }
    Declared {
        names,
        labels,
        external,
    }
}

/// `external = "allowed"` and its program, this platform's of the two
/// `executable` names. The managed Python environment is never external.
fn external_component(
    issues: &mut Issues,
    location: &str,
    name: &str,
    allowed: Option<String>,
    executable: Option<ExecutableDocument>,
) -> Option<ExternalComponent> {
    match allowed.as_deref() {
        None => {
            if executable.is_some() {
                issues.add(
                    format!("{location}.executable"),
                    super::super::documentation::EXTERNAL_PROGRAM.message,
                );
            }
            return None;
        }
        Some("allowed") => {}
        Some(other) => {
            issues.add(
                format!("{location}.external"),
                format!("`{other}` is not what `external` takes (`allowed`)"),
            );
            return None;
        }
    }
    if name == InstanceComponentPaths::PYTHON {
        issues.add(
            format!("{location}.external"),
            super::super::documentation::EXTERNAL_PYTHON.message,
        );
        return None;
    }
    let ExecutableDocument { unix, windows } = executable.unwrap_or_default();
    for (platform, program) in [("unix", &unix), ("windows", &windows)] {
        if let Some(program) = program {
            let checked = RelativePath::parse(program).and_then(|path| match path.is_empty() {
                true => Err(format!("`{program}` names no program")),
                false => Ok(()),
            });
            issues.take(format!("{location}.executable.{platform}"), checked)?;
        }
    }
    Some(ExternalComponent {
        executable: if cfg!(windows) { windows } else { unix },
    })
}
