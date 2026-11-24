use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::internal::hanafubuki_foundation::module_faults::recorded_module_faults;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[test]
fn items_are_handled_in_order_and_shutdown_drains_them() {
    let (sender, received) = mpsc::channel();
    let queue = BoundedWorkQueue::start(
        FaultOwner::module("queue-test-order"),
        "queue-test-order",
        4,
        16,
        Arc::new(move |item: u8| {
            sender.send(item).map_err(|error| error.to_string())?;
            Ok(())
        }),
    )
    .unwrap();
    for item in 1..=3 {
        queue.enqueue(item, 1).unwrap();
    }
    assert_eq!(queue.shutdown(deadline()), Ok(None));
    assert_eq!(received.try_iter().collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(
        queue.enqueue(4, 1),
        Err("queue-test-order is shutting down".to_string())
    );
}

#[test]
fn a_panicking_handler_stops_the_queue_and_is_a_fault_of_its_owner() {
    let owner = FaultOwner::module("queue-test-panic");
    let (handled, wait_handled) = mpsc::channel();
    let queue = BoundedWorkQueue::start(
        owner.clone(),
        "queue-test-panic",
        4,
        16,
        Arc::new(move |item: u8| {
            let _ = handled.send(());
            if item == 1 {
                panic!("handler panicked on {item}");
            }
            Ok(())
        }),
    )
    .unwrap();
    queue.enqueue(1, 1).unwrap();
    wait_handled.recv_timeout(Duration::from_secs(5)).unwrap();
    let started = Instant::now();
    let diagnostic = queue.shutdown(deadline()).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "shutdown returned promptly"
    );
    assert_eq!(
        diagnostic.as_deref(),
        Some("handler panicked: handler panicked on 1")
    );
    assert_eq!(
        queue.enqueue(2, 1),
        Err("queue-test-panic stopped: handler panicked: handler panicked on 1".to_string())
    );
    assert!(recorded_module_faults().recent.iter().any(|fault| {
        fault.owner == owner
            && fault.component == "queue-test-panic"
            && fault.message == "handler panicked on 1"
    }));
}

#[test]
fn backpressure_refuses_what_exceeds_the_pending_weight() {
    let (release, wait_release) = mpsc::channel::<()>();
    let wait_release = Mutex::new(wait_release);
    let (started, wait_started) = mpsc::channel();
    let queue = BoundedWorkQueue::start(
        FaultOwner::module("queue-test-weight"),
        "queue-test-weight",
        4,
        4,
        Arc::new(move |_: u8| {
            let _ = started.send(());
            let _ = wait_release.lock().recv();
            Ok(())
        }),
    )
    .unwrap();
    // The worker holds the first item, so only the rest count as pending.
    queue.enqueue(0, 1).unwrap();
    wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
    queue.enqueue(1, 4).unwrap();
    assert!(
        queue
            .enqueue(2, 1)
            .unwrap_err()
            .contains("backpressure: pending weight limit 4 exceeded")
    );
    drop(release);
    assert_eq!(queue.shutdown(deadline()), Ok(None));
}
