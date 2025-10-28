use std::sync::atomic::Ordering;
use std::time::Duration;

use harmoniq_engine::sched::executor::{self, RtPool};

#[test]
fn idle_does_not_spin_workers() {
    executor::test_metrics::reset();
    let pool = RtPool::new(4, 1, &[0]);

    // Allow the worker to enter its idle loop and settle into the sleep path.
    std::thread::sleep(Duration::from_millis(10));

    let yields = executor::test_metrics::YIELD_COUNT.load(Ordering::Relaxed);
    let sleeps = executor::test_metrics::SLEEP_COUNT.load(Ordering::Relaxed);

    // The hybrid backoff should eventually yield and sleep rather than busy spin.
    assert!(yields > 0, "worker never yielded while idle");
    assert!(sleeps > 0, "worker never slept while idle");

    drop(pool);
}
