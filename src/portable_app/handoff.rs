use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_process::executor::ExecutorManager;

use crate::process::ProcessCommand;
use crate::workspace::paths::WorkspacePaths;

pub fn start(
    executor: &ExecutorManager,
    paths: &WorkspacePaths,
    executable: &ResolvedPath,
) -> Result<(), String> {
    log::info!(
        target: "launcher::handoff",
        "starting application executable={} working_directory={} data_directory={}",
        executable.native_text(),
        paths.core.native_text(),
        paths.data.native_text()
    );
    let process = ProcessCommand::new(executable.native_text())
        .current_dir(paths.core.native())
        .env("HANAFUBUKI_DATA_DIR", paths.data.native_text())
        .env("HANAFUBUKI_PORTABLE", "1")
        .start_independent(executor)
        .map_err(|error| format!("Start Hanafubuki: {error}"))?;
    log::info!(
        target: "launcher::handoff",
        "application handoff started executor_id={}",
        process.executor_id
    );
    log::logger().flush();
    Ok(())
}
