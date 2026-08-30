//! Transactional runtime handles for the reference in-process adapters.
//!
//! The handles are cloneable so one clone can serve as an [`Observer`] while
//! another is passed mutably as the [`TransactionalActuator`]. Both clones
//! share exactly one protected adapter state; no telemetry shadow copy is
//! introduced.

use std::sync::{Arc, Mutex, MutexGuard};

use elastic_adapters::{
    AdapterError, ConcurrencyPermits, RamBudget,
};
use elastic_eir::EirResource;

use crate::{
    Actuation, CommitRecord, ConcurrencyPermitsObserver, InvariantCheck, Observation,
    Observer, Plan, RamBudgetObserver, RollbackRecord, RuntimeError, TransactionalActuator,
    ValidatedPlan, VerificationResult,
};
use elastic_eir::PlanningContext;

fn lock_error(component: &str) -> RuntimeError {
    RuntimeError::actuation(format!("{component} shared adapter state lock was poisoned"))
}

fn candidate_target(plan: &Plan) -> Result<u64, RuntimeError> {
    plan.candidate()
        .and_then(|candidate| candidate.magnitude())
        .ok_or_else(|| RuntimeError::validation("candidate does not contain a target magnitude"))
}

fn successful_checks(plan: &Plan) -> Vec<InvariantCheck> {
    plan.resource
        .invariants()
        .iter()
        .cloned()
        .map(|invariant| InvariantCheck::new(invariant, true, Some("adapter precondition passed".into())))
        .collect()
}

#[derive(Debug)]
struct RamState {
    resource: RamBudget,
    rollback_target: Option<u64>,
}

/// Cloneable transactional handle around a real [`RamBudget`].
#[derive(Clone, Debug)]
pub struct TransactionalRam {
    state: Arc<Mutex<RamState>>,
}

impl TransactionalRam {
    /// Construct a shared RAM adapter from explicit operator configuration.
    pub fn new(
        id: &str,
        host_total: u64,
        min: u64,
        max: u64,
        initial: u64,
        max_step: Option<u64>,
    ) -> Result<Self, AdapterError> {
        Ok(Self {
            state: Arc::new(Mutex::new(RamState {
                resource: RamBudget::new(id, host_total, min, max, initial, max_step)?,
                rollback_target: None,
            })),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, RamState>, RuntimeError> {
        self.state.lock().map_err(|_| lock_error("RAM"))
    }

    pub fn ir(&self) -> Result<EirResource, RuntimeError> {
        Ok(self.lock()?.resource.ir().clone())
    }

    pub fn committed(&self) -> Result<u64, RuntimeError> {
        Ok(self.lock()?.resource.committed())
    }

    pub fn record_use(&self, bytes: u64) -> Result<(), RuntimeError> {
        self.lock()?
            .resource
            .record_use(bytes)
            .map_err(|error| RuntimeError::validation(error.to_string()))
    }

    pub fn release_use(&self, bytes: u64) -> Result<(), RuntimeError> {
        self.lock()?
            .resource
            .release_use(bytes)
            .map_err(|error| RuntimeError::validation(error.to_string()))
    }
}

impl Observer for TransactionalRam {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        match self.state.lock() {
            Ok(state) => RamBudgetObserver::new(&state.resource).observe(),
            Err(_) => (PlanningContext::new(), Vec::new()),
        }
    }
}

impl TransactionalActuator for TransactionalRam {
    fn name(&self) -> &str {
        "transactional-ram"
    }

    fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        let target = candidate_target(plan)?;
        let state = self.lock()?;
        if plan.resource.identity() != state.resource.ir().identity() {
            return Err(RuntimeError::validation(
                "RAM actuator received a plan for a different resource",
            ));
        }
        state
            .resource
            .validate_resize(target)
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        Ok(successful_checks(plan))
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        if !plan.validated {
            return Err(RuntimeError::validation(
                "RAM actuation requires a validated plan",
            ));
        }
        let target = candidate_target(&plan.plan)?;
        let mut state = self.lock()?;
        state
            .resource
            .validate_resize(target)
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        if state.rollback_target.is_some() {
            return Err(RuntimeError::actuation(
                "RAM adapter already has a prepared transaction",
            ));
        }
        state.rollback_target = Some(state.resource.committed());
        Ok(Actuation::new(plan.clone(), Some(target), self.name()))
    }

    fn actuate(&mut self, actuation: &Actuation) -> Result<(), RuntimeError> {
        let target = actuation
            .target
            .ok_or_else(|| RuntimeError::actuation("RAM actuation has no target"))?;
        let mut state = self.lock()?;
        if state.rollback_target.is_none() {
            return Err(RuntimeError::actuation(
                "RAM actuation was not prepared transactionally",
            ));
        }
        state
            .resource
            .apply(target)
            .map_err(|error| RuntimeError::actuation(error.to_string()))?;
        Ok(())
    }

    fn verify(&self, actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        let target = actuation
            .target
            .ok_or_else(|| RuntimeError::verification("RAM actuation has no target"))?;
        let current = self.lock()?.resource.committed();
        if current == target {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: format!("RAM target {target} was not reached; current={current}"),
            })
        }
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        let mut state = self.lock()?;
        if state.rollback_target.take().is_none() {
            return Err(RuntimeError::commit(
                "RAM adapter has no prepared transaction to commit",
            ));
        }
        Ok(CommitRecord::new(
            self.name(),
            "verified RAM resize committed",
        ))
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        let mut state = self.lock()?;
        let previous = state.rollback_target.take().ok_or_else(|| {
            RuntimeError::rollback("RAM adapter has no prepared transaction to roll back")
        })?;
        state
            .resource
            .apply(previous)
            .map_err(|error| RuntimeError::rollback(error.to_string()))?;
        let restored = state.resource.committed() == previous;
        Ok(RollbackRecord::new(
            self.name(),
            format!("restored RAM commitment to {previous}"),
            restored,
        ))
    }
}

#[derive(Debug)]
struct ConcurrencyState {
    resource: ConcurrencyPermits,
    rollback_target: Option<usize>,
}

/// Cloneable transactional handle around [`ConcurrencyPermits`].
#[derive(Clone, Debug)]
pub struct TransactionalConcurrency {
    state: Arc<Mutex<ConcurrencyState>>,
}

impl TransactionalConcurrency {
    pub fn new(id: &str, max_width: usize, initial_width: usize) -> Result<Self, AdapterError> {
        Ok(Self {
            state: Arc::new(Mutex::new(ConcurrencyState {
                resource: ConcurrencyPermits::new(id, max_width, initial_width)?,
                rollback_target: None,
            })),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, ConcurrencyState>, RuntimeError> {
        self.state.lock().map_err(|_| lock_error("concurrency"))
    }

    pub fn ir(&self) -> Result<EirResource, RuntimeError> {
        Ok(self.lock()?.resource.ir().clone())
    }

    pub fn width(&self) -> Result<usize, RuntimeError> {
        Ok(self.lock()?.resource.width())
    }

    pub fn acquire(&self) -> Result<(), RuntimeError> {
        self.lock()?
            .resource
            .acquire()
            .map_err(|error| RuntimeError::validation(error.to_string()))
    }

    pub fn release(&self) -> Result<(), RuntimeError> {
        self.lock()?
            .resource
            .release()
            .map_err(|error| RuntimeError::validation(error.to_string()))
    }
}

impl Observer for TransactionalConcurrency {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        match self.state.lock() {
            Ok(state) => ConcurrencyPermitsObserver::new(&state.resource).observe(),
            Err(_) => (PlanningContext::new(), Vec::new()),
        }
    }
}

impl TransactionalActuator for TransactionalConcurrency {
    fn name(&self) -> &str {
        "transactional-concurrency"
    }

    fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        let target = usize::try_from(candidate_target(plan)?)
            .map_err(|_| RuntimeError::validation("concurrency target does not fit usize"))?;
        let state = self.lock()?;
        if plan.resource.identity() != state.resource.ir().identity() {
            return Err(RuntimeError::validation(
                "concurrency actuator received a plan for a different resource",
            ));
        }
        state
            .resource
            .validate_resize(target)
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        Ok(successful_checks(plan))
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        if !plan.validated {
            return Err(RuntimeError::validation(
                "concurrency actuation requires a validated plan",
            ));
        }
        let target = usize::try_from(candidate_target(&plan.plan)?)
            .map_err(|_| RuntimeError::validation("concurrency target does not fit usize"))?;
        let mut state = self.lock()?;
        state
            .resource
            .validate_resize(target)
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        if state.rollback_target.is_some() {
            return Err(RuntimeError::actuation(
                "concurrency adapter already has a prepared transaction",
            ));
        }
        state.rollback_target = Some(state.resource.width());
        Ok(Actuation::new(
            plan.clone(),
            Some(target as u64),
            self.name(),
        ))
    }

    fn actuate(&mut self, actuation: &Actuation) -> Result<(), RuntimeError> {
        let target = usize::try_from(
            actuation
                .target
                .ok_or_else(|| RuntimeError::actuation("concurrency actuation has no target"))?,
        )
        .map_err(|_| RuntimeError::actuation("concurrency target does not fit usize"))?;
        let mut state = self.lock()?;
        if state.rollback_target.is_none() {
            return Err(RuntimeError::actuation(
                "concurrency actuation was not prepared transactionally",
            ));
        }
        state
            .resource
            .apply(target)
            .map_err(|error| RuntimeError::actuation(error.to_string()))?;
        Ok(())
    }

    fn verify(&self, actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        let target = usize::try_from(
            actuation
                .target
                .ok_or_else(|| RuntimeError::verification("concurrency actuation has no target"))?,
        )
        .map_err(|_| RuntimeError::verification("concurrency target does not fit usize"))?;
        let current = self.lock()?.resource.width();
        if current == target {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: format!("concurrency target {target} was not reached; current={current}"),
            })
        }
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        let mut state = self.lock()?;
        if state.rollback_target.take().is_none() {
            return Err(RuntimeError::commit(
                "concurrency adapter has no prepared transaction to commit",
            ));
        }
        Ok(CommitRecord::new(
            self.name(),
            "verified concurrency resize committed",
        ))
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        let mut state = self.lock()?;
        let previous = state.rollback_target.take().ok_or_else(|| {
            RuntimeError::rollback("concurrency adapter has no prepared transaction to roll back")
        })?;
        state
            .resource
            .apply(previous)
            .map_err(|error| RuntimeError::rollback(error.to_string()))?;
        let restored = state.resource.width() == previous;
        Ok(RollbackRecord::new(
            self.name(),
            format!("restored concurrency width to {previous}"),
            restored,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{plan::plan_with_context, plan::validate_with_checks};
    use elastic_adapters::{HeadroomPlanner, ThresholdPlanner};

    #[test]
    fn transactional_ram_observer_and_actuator_share_one_state() {
        let adapter = TransactionalRam::new("ram", 4096, 512, 4096, 1024, Some(2048)).unwrap();
        let observer = adapter.clone();
        let mut actuator = adapter.clone();
        let resource = adapter.ir().unwrap();
        let planner = HeadroomPlanner::new(0.5, 0.0).unwrap();
        let (context, _) = observer.observe();
        let plan = plan_with_context(&planner, &resource, &context);
        let checks = actuator.validate(&plan).unwrap();
        let validated = validate_with_checks(plan, checks);
        let actuation = actuator.prepare(&validated).unwrap();
        actuator.actuate(&actuation).unwrap();
        assert!(actuator.verify(&actuation).unwrap().is_pass());
        actuator.commit(&actuation).unwrap();
        assert_eq!(adapter.committed().unwrap(), actuation.target.unwrap());
    }

    #[test]
    fn transactional_concurrency_rolls_back_to_previous_width() {
        let adapter = TransactionalConcurrency::new("workers", 8, 4).unwrap();
        let observer = adapter.clone();
        let mut actuator = adapter.clone();
        let resource = adapter.ir().unwrap();
        let planner = ThresholdPlanner::new(0.0, 0.0, 0.5).unwrap();
        let (context, _) = observer.observe();
        let plan = plan_with_context(&planner, &resource, &context);
        let checks = actuator.validate(&plan).unwrap();
        let validated = validate_with_checks(plan, checks);
        let actuation = actuator.prepare(&validated).unwrap();
        actuator.actuate(&actuation).unwrap();
        actuator
            .rollback(
                &actuation,
                &VerificationResult::Fail {
                    detail: "injected".into(),
                },
            )
            .unwrap();
        assert_eq!(adapter.width().unwrap(), 4);
    }
}
