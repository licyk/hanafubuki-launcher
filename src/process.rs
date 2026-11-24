use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::internal::hanafubuki_process::executor::manager::{
    RunOutcome, dispose_executor, run_and_wait, start_independent_executor,
};
use crate::internal::hanafubuki_process::executor::{
    EnvironmentInheritance, ExecutorInfo, ExecutorIoMode, ExecutorManager, ExecutorStartRequest,
};

const OUTPUT_BYTES_CAP: usize = 256 * 1024;
const WAIT_GRACE: Duration = Duration::from_secs(2);

pub(crate) struct ProcessCommand {
    argv: Vec<String>,
    cwd: Option<PathBuf>,
    env: HashMap<String, Option<String>>,
}

impl ProcessCommand {
    pub(crate) fn new(program: impl Into<String>) -> Self {
        Self {
            argv: vec![program.into()],
            cwd: None,
            env: HashMap::new(),
        }
    }

    pub(crate) fn arg(mut self, argument: impl Into<String>) -> Self {
        self.argv.push(argument.into());
        self
    }

    #[cfg(unix)]
    pub(crate) fn args(mut self, arguments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.argv.extend(arguments.into_iter().map(Into::into));
        self
    }

    pub(crate) fn current_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.cwd = Some(directory.into());
        self
    }

    pub(crate) fn env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(name.into(), Some(value.into()));
        self
    }

    pub(crate) fn run(
        &self,
        executor: &ExecutorManager,
        timeout: Duration,
    ) -> Result<RunOutcome, String> {
        run_and_wait(
            executor,
            self.request(true, Some(duration_millis(timeout)?)),
            timeout.saturating_add(WAIT_GRACE),
        )
    }

    pub(crate) fn start_independent(
        &self,
        executor: &ExecutorManager,
    ) -> Result<ExecutorInfo, String> {
        start_independent_executor(executor, self.request(false, None))
    }

    fn request(&self, stream_output: bool, timeout_ms: Option<u64>) -> ExecutorStartRequest {
        ExecutorStartRequest {
            executor_id: None,
            command: self.argv.clone(),
            cwd: self.cwd.clone(),
            environment_inheritance: EnvironmentInheritance::Inherit,
            env: self.env.clone(),
            io_mode: ExecutorIoMode::Pipe,
            stream_stdin: false,
            stream_output,
            cols: None,
            rows: None,
            timeout_ms,
            output_bytes_cap: stream_output.then_some(OUTPUT_BYTES_CAP),
        }
    }
}

pub(crate) fn stop(executor: &ExecutorManager, executor_id: &str) -> Result<(), String> {
    dispose_executor(executor, executor_id)
}

fn duration_millis(duration: Duration) -> Result<u64, String> {
    u64::try_from(duration.as_millis()).map_err(|_| "process timeout is too large".to_string())
}

#[cfg(test)]
mod tests;
