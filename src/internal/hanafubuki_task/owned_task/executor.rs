use parking_lot::Mutex;

use crate::internal::hanafubuki_process::executor::manager::{
    ExecutorManager, confirm_reserved_executor_termination, dispose_reserved_executor,
    get_reserved_executor_info, read_reserved_executor_output, reserved_executor_exists,
    resize_reserved_executor_pty, terminate_reserved_executor,
};
use crate::internal::hanafubuki_process::executor::{ExecutorInfo, ExecutorOutput};
use crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt;

use super::OwnedTaskRegistry;

/// Stateless, borrowed bridge from an owned Task registry to receipt-gated
/// Executor operations. It never stores a receipt, owns a lock, or knows about
/// App/Instance DTOs and process indexes.
pub(crate) struct OwnedTaskExecutorAdapter<'a, Scope, Metadata> {
    executor: &'a ExecutorManager,
    registry: &'a Mutex<OwnedTaskRegistry<Scope, Metadata>>,
    task_kind: &'static str,
}

impl<'a, Scope, Metadata> OwnedTaskExecutorAdapter<'a, Scope, Metadata> {
    pub fn new(
        executor: &'a ExecutorManager,
        registry: &'a Mutex<OwnedTaskRegistry<Scope, Metadata>>,
        task_kind: &'static str,
    ) -> Self {
        Self {
            executor,
            registry,
            task_kind,
        }
    }

    pub fn exists(&self, receipt: &ReservedProcessReceipt) -> bool {
        let recorded = {
            let registry = self.registry.lock();
            registry.process_matches(receipt)
        };
        recorded || reserved_executor_exists(self.executor, receipt.process())
    }

    fn require_available(&self, receipt: &ReservedProcessReceipt) -> Result<(), String> {
        if self.exists(receipt) {
            Ok(())
        } else {
            Err(format!(
                "reserved {} receipt is stale: {}",
                self.task_kind,
                receipt.task_id()
            ))
        }
    }

    pub fn info(&self, receipt: &ReservedProcessReceipt) -> Result<ExecutorInfo, String> {
        self.require_available(receipt)?;
        get_reserved_executor_info(self.executor, receipt.process())
    }

    pub fn read_output(
        &self,
        receipt: &ReservedProcessReceipt,
        from_offset: Option<u64>,
        max_bytes: Option<usize>,
    ) -> Result<ExecutorOutput, String> {
        self.require_available(receipt)?;
        read_reserved_executor_output(self.executor, receipt.process(), from_offset, max_bytes)
    }

    pub fn resize(
        &self,
        receipt: &ReservedProcessReceipt,
        cols: u16,
        rows: u16,
    ) -> Result<(), String> {
        self.require_available(receipt)?;
        resize_reserved_executor_pty(self.executor, receipt.process(), cols, rows)
    }

    pub fn terminate(&self, receipt: &ReservedProcessReceipt) -> Result<(), String> {
        self.require_available(receipt)?;
        terminate_reserved_executor(self.executor, receipt.process())
    }

    pub fn confirm_termination(&self, receipt: &ReservedProcessReceipt) -> Result<(), String> {
        self.require_available(receipt)?;
        confirm_reserved_executor_termination(self.executor, receipt.process())
    }

    pub fn dispose_matching(&self, receipt: &ReservedProcessReceipt) -> Result<(), String> {
        self.require_available(receipt)?;
        dispose_reserved_executor(self.executor, receipt.process())?;
        self.registry.lock().remove_matching(receipt);
        Ok(())
    }
}
