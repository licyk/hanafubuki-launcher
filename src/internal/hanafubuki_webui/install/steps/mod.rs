//! A declarative instance type's install and update steps, resolved for one
//! instance, and the steps that touch only files. The install operation runs
//! them in order (`orchestrator::steps`); the steps that run a process
//! (`run`, `git-clone`, `git-pull`) are Instance tasks it starts through
//! its backend.

mod environment;
pub(crate) mod files;
mod model;

pub use environment::ProcessEnvironment;
pub use model::{
    ComponentPath, DownloadTarget, InstallStepKind, InstallStepProgress, PlannedStep,
    StepEnvironment, StepOperation, StepPlan,
};

#[cfg(test)]
mod tests;
