use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;

use super::TASK_POLL_INTERVAL;
use super::cleanup::schedule_pending_cleanup;
use super::completion::{fail_and_schedule_cleanup, read_all_diagnostics, record_core_task};
use super::model::{CoreInstallTaskRequest, InstanceInstallCoreTask, InstanceInstallTaskState};
use super::registry::{PendingCleanupPurpose, ServiceInner};

/// How one core task of an install operation ended.
pub(super) enum CoreTaskOutcome {
    /// It exited 0; the task is still registered.
    Succeeded(InstanceInstallCoreTask),
    /// It ended after this operation asked it to.
    Cancelled { exit_code: Option<i32> },
    Failed {
        exit_code: Option<i32>,
        error: Option<String>,
    },
    /// The operation is already settled: its task's cleanup is owed and
    /// scheduled, or the operation lost its ownership.
    Settled,
}

/// Starts the task, records it on the operation, and waits for it to end,
/// terminating it when the operation is cancelled.
pub(super) fn run_core_task(
    inner: &Arc<ServiceInner>,
    operation_id: &str,
    ownership: u64,
    request: CoreInstallTaskRequest,
    cancellation: &CancellationToken,
) -> CoreTaskOutcome {
    let task = request.task.clone();
    let core = match inner.backend.start_core_task(request) {
        Ok(core) => core,
        Err(error) => {
            match error.created.map(|core| *core) {
                // The task started and could not be cleaned up: keep it, then
                // fail with its cleanup still owed.
                Some(core) if record_core_task(inner, operation_id, ownership, &core) => {
                    fail_and_schedule_cleanup(inner, operation_id, ownership, core, error.message);
                }
                Some(core) => {
                    schedule_pending_cleanup(inner.clone(), core, PendingCleanupPurpose::Evicted);
                }
                None => {
                    return CoreTaskOutcome::Failed {
                        exit_code: None,
                        error: Some(error.message),
                    };
                }
            }
            return CoreTaskOutcome::Settled;
        }
    };
    if !record_core_task(inner, operation_id, ownership, &core) {
        schedule_pending_cleanup(inner.clone(), core, PendingCleanupPurpose::Evicted);
        return CoreTaskOutcome::Settled;
    }

    let mut termination_requested = false;
    let mut cancellation_deadline = None;
    if cancellation.is_cancelled() {
        if let Err(error) = inner.backend.terminate_core_task(&core) {
            fail_and_schedule_cleanup(inner, operation_id, ownership, core.clone(), error);
            return CoreTaskOutcome::Settled;
        }
        termination_requested = true;
        cancellation_deadline = Some(Instant::now() + inner.cancellation_confirm_timeout);
    }
    loop {
        match inner.backend.get_core_task(&core) {
            Ok(InstanceInstallTaskState::Running) => {
                if termination_requested {
                    if cancellation_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        fail_and_schedule_cleanup(
                            inner,
                            operation_id,
                            ownership,
                            core.clone(),
                            "timed out confirming cancelled Instance Task termination".to_string(),
                        );
                        return CoreTaskOutcome::Settled;
                    }
                } else if cancellation.is_cancelled() {
                    if let Err(error) = inner.backend.terminate_core_task(&core) {
                        fail_and_schedule_cleanup(
                            inner,
                            operation_id,
                            ownership,
                            core.clone(),
                            error,
                        );
                        return CoreTaskOutcome::Settled;
                    }
                    termination_requested = true;
                    cancellation_deadline =
                        Some(Instant::now() + inner.cancellation_confirm_timeout);
                }
                thread::sleep(TASK_POLL_INTERVAL);
            }
            Ok(InstanceInstallTaskState::Exited { exit_code: 0 }) if !termination_requested => {
                return CoreTaskOutcome::Succeeded(core);
            }
            Ok(InstanceInstallTaskState::Exited { exit_code }) => {
                return if termination_requested {
                    CoreTaskOutcome::Cancelled {
                        exit_code: Some(exit_code),
                    }
                } else {
                    CoreTaskOutcome::Failed {
                        exit_code: Some(exit_code),
                        error: read_all_diagnostics(inner, &task),
                    }
                };
            }
            Ok(InstanceInstallTaskState::Cancelled) if termination_requested => {
                return CoreTaskOutcome::Cancelled { exit_code: None };
            }
            Ok(InstanceInstallTaskState::Failed { exit_code, .. }) if termination_requested => {
                return CoreTaskOutcome::Cancelled { exit_code };
            }
            Ok(InstanceInstallTaskState::Cancelled) => {
                return CoreTaskOutcome::Failed {
                    exit_code: None,
                    error: Some(
                        "core task was cancelled outside this install operation".to_string(),
                    ),
                };
            }
            Ok(InstanceInstallTaskState::Failed { exit_code, error }) => {
                let error = match read_all_diagnostics(inner, &task) {
                    Some(output) if !output.is_empty() => format!("{error}\n{output}"),
                    _ => error,
                };
                return CoreTaskOutcome::Failed {
                    exit_code,
                    error: Some(error),
                };
            }
            Err(error) => {
                fail_and_schedule_cleanup(inner, operation_id, ownership, core.clone(), error);
                return CoreTaskOutcome::Settled;
            }
        }
    }
}
