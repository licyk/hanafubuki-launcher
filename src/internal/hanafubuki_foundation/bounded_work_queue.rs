use std::collections::VecDeque;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use parking_lot::{Condvar, Mutex};

use crate::internal::hanafubuki_foundation::background_worker::spawn_contained_thread;
use crate::internal::hanafubuki_foundation::module_faults::{FaultOwner, report_module_fault};
use crate::internal::hanafubuki_foundation::panic::catch_panic;

type WorkHandler<T> = Arc<dyn Fn(T) -> Result<(), String> + Send + Sync>;

struct QueueState<T> {
    pending: VecDeque<(T, usize)>,
    pending_weight: usize,
    accepting: bool,
    finished: bool,
    diagnostic: Option<String>,
}

/// Bounded FIFO worker for short ordered mutations such as terminal input.
///
/// The queue owns scheduling and shutdown only. Receipts, generations,
/// authorization and the meaning of each item remain with its caller.
///
/// Its worker thread is contained: a handler that panics stops the queue as
/// a failing handler does (later items are refused, shutdown returns the
/// panic as the diagnostic) and is reported as a fault of the queue's owner.
pub struct BoundedWorkQueue<T> {
    name: String,
    maximum_item_weight: usize,
    maximum_pending_weight: usize,
    shared: Arc<(Mutex<QueueState<T>>, Condvar)>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl<T: Send + 'static> BoundedWorkQueue<T> {
    pub fn start(
        owner: FaultOwner,
        name: impl Into<String>,
        maximum_item_weight: usize,
        maximum_pending_weight: usize,
        handler: WorkHandler<T>,
    ) -> Result<Self, String> {
        let name = name.into();
        if name.trim().is_empty()
            || maximum_item_weight == 0
            || maximum_pending_weight < maximum_item_weight
        {
            return Err("bounded work queue requires a name and valid positive limits".to_string());
        }
        let shared = Arc::new((
            Mutex::new(QueueState {
                pending: VecDeque::new(),
                pending_weight: 0,
                accepting: true,
                finished: false,
                diagnostic: None,
            }),
            Condvar::new(),
        ));
        let worker_shared = shared.clone();
        let thread_name = name.clone();
        let queue_log_name = thread_name.clone();
        let join = spawn_contained_thread(owner.clone(), thread_name.clone(), move || loop {
            let item = {
                let (state, changed) = &*worker_shared;
                let mut state = state.lock();
                while state.pending.is_empty() && state.accepting {
                    changed.wait(&mut state);
                }
                let Some((item, weight)) = state.pending.pop_front() else {
                    state.finished = true;
                    changed.notify_all();
                    return;
                };
                state.pending_weight = state.pending_weight.saturating_sub(weight);
                item
            };
            let handled = catch_panic(|| handler(item)).unwrap_or_else(|message| {
                report_module_fault(owner.clone(), &queue_log_name, &message);
                Err(format!("handler panicked: {message}"))
            });
            if let Err(error) = handled {
                log::warn!("bounded work queue {queue_log_name} handler failed and stopped the queue: {error}");
                let (state, changed) = &*worker_shared;
                let mut state = state.lock();
                state.accepting = false;
                state.pending.clear();
                state.pending_weight = 0;
                state.diagnostic = Some(error);
                state.finished = true;
                changed.notify_all();
                return;
            }
        })
        .map_err(|error| format!("spawn bounded work queue {thread_name}: {error}"))?;
        Ok(Self {
            name,
            maximum_item_weight,
            maximum_pending_weight,
            shared,
            join: Mutex::new(Some(join)),
        })
    }

    pub fn enqueue(&self, item: T, weight: usize) -> Result<(), String> {
        if weight == 0 || weight > self.maximum_item_weight {
            return Err(format!(
                "{} item weight must be between 1 and {}",
                self.name, self.maximum_item_weight
            ));
        }
        let (state, changed) = &*self.shared;
        let mut state = state.lock();
        if let Some(error) = &state.diagnostic {
            return Err(format!("{} stopped: {error}", self.name));
        }
        if !state.accepting {
            return Err(format!("{} is shutting down", self.name));
        }
        if state.pending_weight.saturating_add(weight) > self.maximum_pending_weight {
            return Err(format!(
                "{} backpressure: pending weight limit {} exceeded",
                self.name, self.maximum_pending_weight
            ));
        }
        state.pending_weight += weight;
        state.pending.push_back((item, weight));
        changed.notify_one();
        Ok(())
    }

    pub fn shutdown(&self, deadline: Instant) -> Result<Option<String>, String> {
        let (state, changed) = &*self.shared;
        let mut state = state.lock();
        state.accepting = false;
        changed.notify_all();
        while !state.finished {
            if changed.wait_until(&mut state, deadline).timed_out() && !state.finished {
                return Err(format!(
                    "{} did not stop before shutdown deadline",
                    self.name
                ));
            }
        }
        let diagnostic = state.diagnostic.clone();
        drop(state);
        if let Some(join) = self.join.lock().take() {
            join.join()
                .map_err(|_| format!("{} panicked during shutdown", self.name))?;
        }
        Ok(diagnostic)
    }
}

#[cfg(test)]
mod tests;
