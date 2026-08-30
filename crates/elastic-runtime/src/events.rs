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
        Self {
            kind,
            timestamp: Instant::now(),
            details: details.into(),
        }
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
    ControlLoopStarted,
    ControlLoopStopped,
    CancellationObserved,
    ErrorEncountered,
}

/// Streaming boundary for runtime events.
pub trait RuntimeEventSink {
    fn emit(&mut self, event: &RuntimeEvent);
}

/// Event sink that intentionally discards streamed events.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopEventSink;

impl RuntimeEventSink for NoopEventSink {
    fn emit(&mut self, _event: &RuntimeEvent) {}
}

impl RuntimeEventSink for Vec<RuntimeEvent> {
    fn emit(&mut self, event: &RuntimeEvent) {
        self.push(event.clone());
    }
}
