use std::panic::{AssertUnwindSafe, catch_unwind};

use super::*;

#[test]
fn the_cleanup_running_flag_clears_when_the_worker_panics() {
    let running = AtomicBool::new(true);
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _running = CleanupRunning(&running);
        panic!("cleanup panicked");
    }));
    assert!(unwound.is_err());
    assert!(!running.load(Ordering::Acquire));
}
