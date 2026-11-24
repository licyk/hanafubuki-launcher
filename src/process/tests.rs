use std::time::Duration;

use crate::internal::hanafubuki_process::executor::{ExecutorManager, ExecutorStatus};

use super::ProcessCommand;

#[test]
fn command_runs_through_executor_and_captures_output() {
    let executor = ExecutorManager::default();
    #[cfg(windows)]
    let command = ProcessCommand::new("cmd.exe")
        .arg("/D")
        .arg("/S")
        .arg("/C")
        .arg("echo executor");
    #[cfg(unix)]
    let command = ProcessCommand::new("sh").arg("-c").arg("printf executor");

    let outcome = command.run(&executor, Duration::from_secs(2)).unwrap();

    assert_eq!(outcome.status, ExecutorStatus::Exited);
    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.output.contains("executor"));
}
