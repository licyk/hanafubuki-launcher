//! The steps that remove or replace what is already in their target: an
//! `unpack` (a download's `unpackTo` too), which replaces each entry of the
//! same name, or the whole target with `replace`, and a `git-clone`, which
//! moves its clone into place the same way. None of them may target an
//! external component, which the instance does not own, and a replacing
//! one may not take another component with it. A step that only writes
//! files (`download`'s `to`, `write-file`, `git-pull`) may write in an
//! external component; a `run` step's effects are its program's.

use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;
use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_foundation::paths::PathKey;

use crate::internal::hanafubuki_webui::install::steps::{
    ComponentPath, DownloadTarget, PlannedStep,
};

pub(super) fn check(step: &PlannedStep, components: &InstanceComponentPaths) -> Result<(), String> {
    let (kind, target, replace) = match step {
        PlannedStep::Download {
            target: DownloadTarget::UnpackInto { into, replace, .. },
            ..
        } => ("download", into, *replace),
        PlannedStep::Unpack { into, replace, .. } => ("unpack", into, *replace),
        PlannedStep::GitClone { into, .. } => ("git-clone", into, false),
        _ => return Ok(()),
    };
    if target.external {
        return Err(format!(
            "`{kind}` would replace files in {}, inside the external {} folder, which the instance does not own",
            target.path().display(),
            InstancePathPolicy::component_label(&target.component)
        ));
    }
    if replace {
        refuse_taking_a_component(kind, target, components)?;
    }
    Ok(())
}

/// A replaced folder goes whole: it must hold no other component.
fn refuse_taking_a_component(
    kind: &str,
    target: &ComponentPath,
    components: &InstanceComponentPaths,
) -> Result<(), String> {
    let replaced = PathKey::of(&target.path());
    let taken = components
        .iter()
        .find(|(name, path)| *name != target.component && PathKey::of(path).starts_with(&replaced));
    match taken {
        Some((name, path)) => Err(format!(
            "`{kind}` with `replace` would replace {}, which holds the {} folder {}",
            target.path().display(),
            InstancePathPolicy::component_label(name),
            path.display()
        )),
        None => Ok(()),
    }
}
