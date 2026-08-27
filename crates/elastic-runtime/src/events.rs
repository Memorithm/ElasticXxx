//! Runtime events.

use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeEvent {
    pub kind: RuntimeEventKind,
    pub timestamp: Instant,
    pub details: String,
}

impl RuntimeEvent {
    pub fn new(kind: RuntimeEventKind, details: impl Into<String>) -> Self {
        Self { kind, timestamp: Instant::now(), details: details.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeEventKind {
    ObservationCollected,
    ForecastGenerated,
    PlanSelected,
    PlanRejected,
    InvariantChecked,
    PlanValidated,
    ActuationPrepared,
    ActuationApplied,
    VerificationPerformed,
    CommitExecuted,
    RollbackExecuted,
    CycleStarted,
    CycleCompleted,
    ErrorEncountered,
}
