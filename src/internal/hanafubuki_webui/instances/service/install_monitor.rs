use std::thread;
use std::time::{Duration, Instant};

use crate::internal::hanafubuki_foundation::background_worker::{
    WorkerShutdownDiagnostic, spawn_contained_thread,
};

use super::super::identity::now_millis;
use super::super::repository::LifecycleTransitionPolicy;
use super::super::{WebUiInstanceLifecycle, WebUiInstanceRecord};
use super::WebUiInstanceService;
use crate::internal::hanafubuki_webui::install::{
    INSTALL_MODULE, InstanceInstallCleanupStatus, InstanceInstallLogOutput,
    InstanceInstallOperation, InstanceInstallPendingCleanup, InstanceInstallPhase,
};

const MONITOR_INTERVAL: Duration = Duration::from_millis(50);
const LIFECYCLE_PERSIST_ATTEMPTS: usize = 3;
const INTERRUPTED_INSTALL_ERROR: &str = "installation was interrupted before completion; retry installation to continue using retained files";

impl WebUiInstanceService {
    /// Install task sessions remain process-owned. Durable lifecycle state is
    /// converted to the retryable install contract when its task disappeared.
    pub(super) fn recover_interrupted_install(
        &self,
        record: WebUiInstanceRecord,
    ) -> Result<WebUiInstanceRecord, String> {
        let WebUiInstanceLifecycle::Installing { operation_id } = &record.lifecycle else {
            return Ok(record);
        };
        if self.installs.get(operation_id).is_ok() {
            return Ok(record);
        }
        self.transition(
            &record.instance_id,
            WebUiInstanceLifecycle::InstallFailed {
                operation_id: operation_id.clone(),
                phase: InstanceInstallPhase::Failed,
                exit_code: None,
                error: Some(INTERRUPTED_INSTALL_ERROR.to_string()),
                cleanup_status: InstanceInstallCleanupStatus::Released,
                cleanup_error: None,
            },
        )
    }

    pub fn get_install(&self, operation_id: &str) -> Result<InstanceInstallOperation, String> {
        let operation = self.installs.get(operation_id)?;
        if operation.phase.is_terminal() {
            if let Err(error) = self.persist_terminal(&operation) {
                log::debug!("persist terminal install state for {operation_id} failed: {error}");
            }
        }
        Ok(operation)
    }

    pub fn list_installs(&self) -> Vec<InstanceInstallOperation> {
        self.installs.list()
    }

    pub fn cancel_install(&self, operation_id: &str) -> Result<InstanceInstallOperation, String> {
        self.installs.cancel(operation_id)
    }

    /// Ends every install run at the application's quit as a Cancel ends it
    /// ([`InstanceInstallService::shutdown`](crate::internal::hanafubuki_webui::install::InstanceInstallService::shutdown))
    /// and records each run that ended without waiting for its monitor, so
    /// the record says `InstallFailed` (`Cancelled`), as after a Cancel. A
    /// run still going at `deadline` is a diagnostic; its record stays
    /// `Installing`, which the next start recovers as an interrupted install.
    pub fn shutdown_installs(&self, deadline: Instant) -> Vec<WorkerShutdownDiagnostic> {
        let mut diagnostics = Vec::new();
        for operation in self.installs.shutdown(deadline) {
            if !operation.phase.is_terminal() {
                diagnostics.push(WorkerShutdownDiagnostic {
                    worker: operation.operation_id,
                    message: format!(
                        "the install did not end within the shutdown deadline ({:?})",
                        operation.phase
                    ),
                });
            } else if let Err(error) = self.persist_terminal(&operation) {
                log::warn!(
                    "persist install {} ended at shutdown failed: {error}",
                    operation.operation_id
                );
            }
        }
        diagnostics
    }

    pub fn read_install_output(
        &self,
        operation_id: &str,
        from_offset: u64,
        max_bytes: Option<usize>,
    ) -> Result<InstanceInstallLogOutput, String> {
        self.installs
            .read_output(operation_id, from_offset, max_bytes)
    }

    pub fn retry_install_cleanup(&self, operation_id: &str) -> Result<(), String> {
        self.installs.retry_operation_cleanup(operation_id)
    }

    pub fn list_pending_cleanups(&self) -> Vec<InstanceInstallPendingCleanup> {
        self.installs.list_pending_cleanups()
    }

    pub(super) fn spawn_monitor(&self, instance_id: String, operation_id: String) {
        let service = self.clone();
        let name = format!("install-monitor-{operation_id}");
        spawn_contained_thread(INSTALL_MODULE, name, move || loop {
            match service.installs.get(&operation_id) {
                Ok(operation) if operation.phase.is_terminal() => {
                    if let Err(error) = service.persist_terminal_with_retry(&operation) {
                        log::error!(
                            "persist terminal install lifecycle failed instance_id={instance_id} operation_id={operation_id}: {error}"
                        );
                    }
                    return;
                }
                Ok(_) => thread::sleep(MONITOR_INTERVAL),
                Err(error) => {
                    log::error!(
                        "install lifecycle monitor lost operation instance_id={instance_id} operation_id={operation_id}: {error}"
                    );
                    return;
                }
            }
        })
        .expect("spawn install monitor thread");
    }

    pub(super) fn persist_terminal(
        &self,
        operation: &InstanceInstallOperation,
    ) -> Result<(), String> {
        let lifecycle = match operation.phase {
            InstanceInstallPhase::Completed => {
                let (record, completed_now) = self.repository.complete_install_with_defaults(
                    &operation.instance_id,
                    operation.operation_id.clone(),
                    now_millis(),
                )?;
                if completed_now {
                    self.install_completions
                        .completed(&record.instance_id, &record.instance_identity);
                }
                return Ok(());
            }
            InstanceInstallPhase::Failed | InstanceInstallPhase::Cancelled => {
                WebUiInstanceLifecycle::InstallFailed {
                    operation_id: operation.operation_id.clone(),
                    phase: operation.phase,
                    exit_code: operation.exit_code,
                    error: operation.error.clone().or_else(|| {
                        (operation.phase == InstanceInstallPhase::Cancelled)
                            .then(|| "installation cancelled".to_string())
                    }),
                    cleanup_status: operation.cleanup_status,
                    cleanup_error: operation.cleanup_error.clone(),
                }
            }
            _ => return Ok(()),
        };
        self.transition(&operation.instance_id, lifecycle)
            .map(|_| ())
    }

    pub(super) fn persist_terminal_with_retry(
        &self,
        operation: &InstanceInstallOperation,
    ) -> Result<(), String> {
        let mut last_error = None;
        for attempt in 0..LIFECYCLE_PERSIST_ATTEMPTS {
            match self.persist_terminal(operation) {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
            if attempt + 1 < LIFECYCLE_PERSIST_ATTEMPTS {
                thread::sleep(Duration::from_millis(25 * (attempt as u64 + 1)));
            }
        }
        Err(last_error.unwrap_or_else(|| "lifecycle persistence failed".to_string()))
    }

    pub(in crate::internal::hanafubuki_webui::instances) fn transition(
        &self,
        instance_id: &str,
        lifecycle: WebUiInstanceLifecycle,
    ) -> Result<WebUiInstanceRecord, String> {
        self.repository.transition_lifecycle(
            instance_id,
            lifecycle,
            now_millis(),
            LifecycleTransitionPolicy::Standard,
        )
    }
}
