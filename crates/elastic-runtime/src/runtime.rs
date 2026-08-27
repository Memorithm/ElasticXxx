//! Runtime orchestrator.

use crate::actuation::Actuation;
use crate::commit::{CommitRecord, RollbackRecord};
use crate::config::RuntimeConfig;
use crate::error::RuntimeError;
use crate::events::{RuntimeEvent, RuntimeEventKind};
use crate::observation::{ObservationSnapshot, Observer};
use crate::plan::{Plan, ValidatedPlan};
use crate::verification::VerificationResult;
use elastic_eir::{EirResource, TransitionPlanner};

/// High-level runtime orchestrator.
#[derive(Clone, Debug)]
pub struct Runtime {
    config: RuntimeConfig,
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Run one cycle: observe → plan → validate → actuate → verify → commit/rollback
    pub fn cycle<P: TransitionPlanner, O: Observer>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
    ) -> Result<CycleResult, RuntimeError> {
        let events = vec![RuntimeEvent::new(RuntimeEventKind::CycleStarted, "cycle started")];
        // Planning
        let plan = crate::control_loop::run_planning_cycle(planner, resource, observer)?;
        let validated = crate::control_loop::validate_plan(plan.clone());
        let actuation = if validated.validated {
            Some(Actuation::new(validated.clone(), None, "default"))
        } else {
            None
        };
        // Dummy verification
        let verification = Some(VerificationResult::Pass);
        let commit = Some(CommitRecord::new("transition", "cycle complete"));
        Ok(CycleResult {
            observations: vec![],
            plan: Some(validated),
            actuation,
            verification,
            commit,
            rollback: None,
            events,
        })
    }
}

/// Result of a runtime cycle.
#[derive(Clone, Debug)]
pub struct CycleResult {
    pub observations: Vec<ObservationSnapshot>,
    pub plan: Option<ValidatedPlan>,
    pub actuation: Option<Actuation>,
    pub verification: Option<VerificationResult>,
    pub commit: Option<CommitRecord>,
    pub rollback: Option<RollbackRecord>,
    pub events: Vec<RuntimeEvent>,
}
