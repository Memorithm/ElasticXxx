//! Runtime clock abstraction used only for control-loop cadence.

use std::time::Duration;

/// Sleeping boundary for periodic controllers.
///
/// Production uses [`SystemClock`]. Tests can inject a deterministic clock
/// that advances logical time without sleeping the test process.
pub trait RuntimeClock: Send + Sync {
    fn sleep(&self, duration: Duration);
}

/// Standard blocking clock for the synchronous runtime.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl RuntimeClock for SystemClock {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
