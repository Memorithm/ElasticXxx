//! Commit and rollback records.

use std::time::Instant;

/// Record of a successful commit.
#[derive(Clone, Debug, PartialEq)]
pub struct CommitRecord {
    pub transition: String,
    pub timestamp: Instant,
    pub rationale: String,
}

impl CommitRecord {
    pub fn new(transition: impl Into<String>, rationale: impl Into<String>) -> Self {
        Self { transition: transition.into(), timestamp: Instant::now(), rationale: rationale.into() }
    }
}

/// Record of a rollback.
#[derive(Clone, Debug, PartialEq)]
pub struct RollbackRecord {
    pub transition: String,
    pub timestamp: Instant,
    pub rationale: String,
    pub invariants_restored: bool,
}

impl RollbackRecord {
    pub fn new(transition: impl Into<String>, rationale: impl Into<String>, invariants_restored: bool) -> Self {
        Self { transition: transition.into(), timestamp: Instant::now(), rationale: rationale.into(), invariants_restored }
    }
}
