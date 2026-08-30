//! Cooperative cancellation for bounded runtime loops.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Cloneable cancellation signal shared with a running controller.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_cancellation_state() {
        let token = CancellationToken::new();
        let peer = token.clone();

        assert!(!peer.is_cancelled());
        token.cancel();
        assert!(peer.is_cancelled());
    }
}
