use std::time::Duration;

use crate::internal::hanafubuki_process::executor::{ExecutorManager, ExecutorStatus};

use crate::process::ProcessCommand;

pub(super) fn output(
    executor: &ExecutorManager,
    program: &str,
    arguments: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    let outcome = ProcessCommand::new(program)
        .args(arguments.iter().copied())
        .run(executor, timeout)
        .map_err(|error| format!("Run {program}: {error}"))?;
    match (outcome.status, outcome.exit_code) {
        (ExecutorStatus::TimedOut, _) => Err(format!("{program} timed out")),
        (_, Some(0)) => Ok(outcome.output),
        (status, exit_code) => Err(format!(
            "{program} exited with status {status:?} and code {exit_code:?}: {}",
            outcome.output.trim()
        )),
    }
}
