//! Transactional runtime handles for the reference in-process adapters.
//!
//! The handles are cloneable so one clone can serve as an [`Observer`] while
//! another is passed mutably as the [`TransactionalActuator`]. Both clones
//! share exactly one protected adapter state; no telemetry shadow copy is
//! introduced.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use elastic_adapters::{AdapterError, ConcurrencyPermits, RamBudget};
use elastic_core::resource::ObservationSignalId;
use elastic_eir::{EirResource, PlanningContext};

use crate::{
    Actuation, CommitRecord, ConcurrencyPermitsObserver, InvariantCheck, Observation,
    ObservationSource, Observer, Plan, RamBudgetObserver, RollbackRecord, RuntimeError,
    TransactionalActuator, ValidatedPlan, VerificationResult,
};

fn lock_error(component: &str) -> RuntimeError {
    RuntimeError::actuation(format!(
        "{component} shared adapter state lock was poisoned"
    ))
}

fn adapter_state_accessible_signal() -> ObservationSignalId {
    ObservationSignalId::custom("adapter-state-accessible")
        .expect("adapter-state-accessible is a valid observation signal")
}

fn poisoned_observation(
    source: ObservationSource,
    component: &str,
) -> (PlanningContext, Vec<Observation>) {
    (
        PlanningContext::new(),
        vec![Observation::unsupported_from_source(
            source,
            adapter_state_accessible_signal(),
            Instant::now(),
            format!("{component} shared adapter state lock was poisoned"),
        )],
    )
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
        .map(|invariant| {
            InvariantCheck::new(
                invariant,
                true,
                Some("adapter action-time precondition passed".into()),
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedRam {
    previous: u64,
    target: u64,
}

#[derive(Debug)]
struct RamState {
    resource: RamBudget,
    prepared: Option<PreparedRam>,
}

/// Cloneable transactional handle around a real [`RamBudget`].
#[derive(Clone, Debug)]
pub struct TransactionalRam {
    state: Arc<Mutex<RamState>>,
    source: ObservationSource,
}

impl TransactionalRam {
    /// Construct a shared RAM adapter from explicit operator configuration.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`AdapterError`] when the RAM declaration is
    /// invalid.
    pub fn new(
        id: &str,
        host_total: u64,
        min: u64,
        max: u64,
        initial: u64,
        max_step: Option<u64>,
    ) -> Result<Self, AdapterError> {
        let resource = RamBudget::new(id, host_total, min, max, initial, max_step)?;
        let source = ObservationSource::Resource(resource.spec().resource_id().clone());
        Ok(Self {
            state: Arc::new(Mutex::new(RamState {
                resource,
                prepared: None,
            })),
            source,
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, RamState>, RuntimeError> {
        self.state.lock().map_err(|_| lock_error("RAM"))
    }

    /// Clone the resource's normalized EIR node.
    ///
    /// # Errors
    ///
    /// Returns an actuation error if the shared state lock is poisoned.
    pub fn ir(&self) -> Result<EirResource, RuntimeError> {
        Ok(self.lock()?.resource.ir().clone())
    }

    /// Current committed bytes.
    ///
    /// # Errors
    ///
    /// Returns an actuation error if the shared state lock is poisoned.
    pub fn committed(&self) -> Result<u64, RuntimeError> {
        Ok(self.lock()?.resource.committed())
    }

    /// Record bytes protected by the `PreserveContents` invariant.
    ///
    /// # Errors
    ///
    /// Returns a validation error if protected usage would exceed commitment
    /// or the shared state cannot be locked.
    pub fn record_use(&self, bytes: u64) -> Result<(), RuntimeError> {
        self.lock()?
            .resource
            .record_use(bytes)
            .map_err(|error| RuntimeError::validation(error.to_string()))
    }

    /// Release protected bytes.
    ///
    /// # Errors
    ///
    /// Returns a validation error if the release is invalid or the shared
    /// state cannot be locked.
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
            Err(_) => poisoned_observation(self.source.clone(), "RAM"),
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
        if state.prepared.is_some() {
            return Err(RuntimeError::actuation(
                "RAM adapter already has a prepared transaction",
            ));
        }
        state
            .resource
            .validate_resize(target)
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        state.prepared = Some(PreparedRam {
            previous: state.resource.committed(),
            target,
        });
        Ok(Actuation::new(plan.clone(), Some(target), self.name()))
    }

    fn actuate(&mut self, actuation: &Actuation) -> Result<(), RuntimeError> {
        let target = actuation
            .target
            .ok_or_else(|| RuntimeError::actuation("RAM actuation has no target"))?;
        let mut state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::actuation("RAM actuation was not prepared transactionally")
        })?;
        if prepared.target != target {
            return Err(RuntimeError::actuation(format!(
                "RAM actuation target {target} does not match prepared target {}",
                prepared.target
            )));
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
        let state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::verification("RAM adapter has no prepared transaction to verify")
        })?;
        if prepared.target != target {
            return Err(RuntimeError::verification(format!(
                "RAM verification target {target} does not match prepared target {}",
                prepared.target
            )));
        }
        let current = state.resource.committed();
        if current == target {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: format!("RAM target {target} was not reached; current={current}"),
            })
        }
    }

    fn commit(&mut self, actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        let target = actuation
            .target
            .ok_or_else(|| RuntimeError::commit("RAM actuation has no target"))?;
        let mut state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::commit("RAM adapter has no prepared transaction to commit")
        })?;
        if prepared.target != target {
            return Err(RuntimeError::commit(format!(
                "RAM commit target {target} does not match prepared target {}",
                prepared.target
            )));
        }
        if state.resource.committed() != target {
            return Err(RuntimeError::commit(format!(
                "RAM target {target} is not the current committed state"
            )));
        }
        state.prepared = None;
        Ok(CommitRecord::new(
            self.name(),
            "verified RAM resize committed",
        ))
    }

    fn rollback(
        &mut self,
        actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        let target = actuation
            .target
            .ok_or_else(|| RuntimeError::rollback("RAM actuation has no target"))?;
        let mut state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::rollback("RAM adapter has no prepared transaction to roll back")
        })?;
        if prepared.target != target {
            return Err(RuntimeError::rollback(format!(
                "RAM rollback target {target} does not match prepared target {}",
                prepared.target
            )));
        }
        state
            .resource
            .apply(prepared.previous)
            .map_err(|error| RuntimeError::rollback(error.to_string()))?;
        let restored = state.resource.committed() == prepared.previous;
        if restored {
            state.prepared = None;
        }
        Ok(RollbackRecord::new(
            self.name(),
            format!("restored RAM commitment to {}", prepared.previous),
            restored,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedConcurrency {
    previous: usize,
    target: usize,
}

#[derive(Debug)]
struct ConcurrencyState {
    resource: ConcurrencyPermits,
    prepared: Option<PreparedConcurrency>,
}

/// Cloneable transactional handle around [`ConcurrencyPermits`].
#[derive(Clone, Debug)]
pub struct TransactionalConcurrency {
    state: Arc<Mutex<ConcurrencyState>>,
    source: ObservationSource,
}

impl TransactionalConcurrency {
    /// Construct a shared concurrency adapter.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`AdapterError`] for invalid width bounds.
    pub fn new(id: &str, max_width: usize, initial_width: usize) -> Result<Self, AdapterError> {
        let resource = ConcurrencyPermits::new(id, max_width, initial_width)?;
        let source = ObservationSource::Resource(resource.spec().resource_id().clone());
        Ok(Self {
            state: Arc::new(Mutex::new(ConcurrencyState {
                resource,
                prepared: None,
            })),
            source,
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, ConcurrencyState>, RuntimeError> {
        self.state.lock().map_err(|_| lock_error("concurrency"))
    }

    /// Clone the resource's normalized EIR node.
    ///
    /// # Errors
    ///
    /// Returns an actuation error if the shared state lock is poisoned.
    pub fn ir(&self) -> Result<EirResource, RuntimeError> {
        Ok(self.lock()?.resource.ir().clone())
    }

    /// Current licensed width.
    ///
    /// # Errors
    ///
    /// Returns an actuation error if the shared state lock is poisoned.
    pub fn width(&self) -> Result<usize, RuntimeError> {
        Ok(self.lock()?.resource.width())
    }

    /// Acquire one active permit.
    ///
    /// # Errors
    ///
    /// Returns a validation error when the licensed width is exhausted.
    pub fn acquire(&self) -> Result<(), RuntimeError> {
        self.lock()?
            .resource
            .acquire()
            .map_err(|error| RuntimeError::validation(error.to_string()))
    }

    /// Release one active permit.
    ///
    /// # Errors
    ///
    /// Returns a validation error when no permit is active.
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
            Err(_) => poisoned_observation(self.source.clone(), "concurrency"),
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
        if state.prepared.is_some() {
            return Err(RuntimeError::actuation(
                "concurrency adapter already has a prepared transaction",
            ));
        }
        state
            .resource
            .validate_resize(target)
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        state.prepared = Some(PreparedConcurrency {
            previous: state.resource.width(),
            target,
        });
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
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::actuation("concurrency actuation was not prepared transactionally")
        })?;
        if prepared.target != target {
            return Err(RuntimeError::actuation(format!(
                "concurrency actuation target {target} does not match prepared target {}",
                prepared.target
            )));
        }
        state
            .resource
            .apply(target)
            .map_err(|error| RuntimeError::actuation(error.to_string()))?;
        Ok(())
    }

    fn verify(&self, actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        let target =
            usize::try_from(actuation.target.ok_or_else(|| {
                RuntimeError::verification("concurrency actuation has no target")
            })?)
            .map_err(|_| RuntimeError::verification("concurrency target does not fit usize"))?;
        let state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::verification("concurrency adapter has no prepared transaction to verify")
        })?;
        if prepared.target != target {
            return Err(RuntimeError::verification(format!(
                "concurrency verification target {target} does not match prepared target {}",
                prepared.target
            )));
        }
        let current = state.resource.width();
        if current == target {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: format!("concurrency target {target} was not reached; current={current}"),
            })
        }
    }

    fn commit(&mut self, actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        let target = usize::try_from(
            actuation
                .target
                .ok_or_else(|| RuntimeError::commit("concurrency actuation has no target"))?,
        )
        .map_err(|_| RuntimeError::commit("concurrency target does not fit usize"))?;
        let mut state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::commit("concurrency adapter has no prepared transaction to commit")
        })?;
        if prepared.target != target {
            return Err(RuntimeError::commit(format!(
                "concurrency commit target {target} does not match prepared target {}",
                prepared.target
            )));
        }
        if state.resource.width() != target {
            return Err(RuntimeError::commit(format!(
                "concurrency target {target} is not the current licensed width"
            )));
        }
        state.prepared = None;
        Ok(CommitRecord::new(
            self.name(),
            "verified concurrency resize committed",
        ))
    }

    fn rollback(
        &mut self,
        actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        let target = usize::try_from(
            actuation
                .target
                .ok_or_else(|| RuntimeError::rollback("concurrency actuation has no target"))?,
        )
        .map_err(|_| RuntimeError::rollback("concurrency target does not fit usize"))?;
        let mut state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::rollback("concurrency adapter has no prepared transaction to roll back")
        })?;
        if prepared.target != target {
            return Err(RuntimeError::rollback(format!(
                "concurrency rollback target {target} does not match prepared target {}",
                prepared.target
            )));
        }
        state
            .resource
            .apply(prepared.previous)
            .map_err(|error| RuntimeError::rollback(error.to_string()))?;
        let restored = state.resource.width() == prepared.previous;
        if restored {
            state.prepared = None;
        }
        Ok(RollbackRecord::new(
            self.name(),
            format!("restored concurrency width to {}", prepared.previous),
            restored,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{plan::plan_with_context, plan::validate_with_checks};
    use elastic_adapters::HeadroomPlanner;
    use elastic_core::{resource::DimensionId, TransitionMechanism};
    use elastic_eir::{PlanOutcome, TransitionCandidate, TransitionPlanner};

    struct ConcurrencyTargetPlanner(usize);

    impl TransitionPlanner for ConcurrencyTargetPlanner {
        fn propose_transition(&self, resource: &EirResource) -> PlanOutcome {
            let Some(admitted) = resource.transitions().iter().find(|admitted| {
                admitted.transition().mechanism() == TransitionMechanism::Reinterpret
                    && admitted.transition().dimension() == &DimensionId::CONCURRENCY
                    && admitted.capability_grounded()
            }) else {
                return PlanOutcome::Unsupported;
            };
            PlanOutcome::Candidate(
                TransitionCandidate::from_admitted(admitted).with_magnitude(self.0 as u64),
            )
        }
    }

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
    fn transactional_ram_rejects_actuation_that_differs_from_prepared_target() {
        let adapter = TransactionalRam::new("ram", 4096, 512, 4096, 1024, Some(2048)).unwrap();
        let mut actuator = adapter.clone();
        let resource = adapter.ir().unwrap();
        let planner = HeadroomPlanner::new(0.5, 0.0).unwrap();
        let (context, _) = adapter.observe();
        let plan = plan_with_context(&planner, &resource, &context);
        let checks = actuator.validate(&plan).unwrap();
        let validated = validate_with_checks(plan, checks);
        let actuation = actuator.prepare(&validated).unwrap();
        let forged_target = actuation.target.unwrap().saturating_add(1);
        let forged = Actuation::new(validated, Some(forged_target), actuator.name());

        assert!(actuator.actuate(&forged).is_err());
        assert_eq!(adapter.committed().unwrap(), 1024);
        actuator
            .rollback(
                &actuation,
                &VerificationResult::Inconclusive {
                    detail: "forged actuation rejected".into(),
                },
            )
            .unwrap();
    }

    #[test]
    fn poisoned_ram_observer_reports_unsupported_evidence() {
        let adapter = TransactionalRam::new("ram", 4096, 512, 4096, 1024, None).unwrap();
        let poison = adapter.clone();
        let joined = std::thread::spawn(move || {
            let _guard = poison.state.lock().unwrap();
            panic!("poison RAM state for observer test");
        })
        .join();
        assert!(joined.is_err());

        let (context, observations) = adapter.observe();
        assert!(context.iter().next().is_none());
        assert_eq!(observations.len(), 1);
        assert!(observations[0].is_unsupported());
        assert_eq!(observations[0].source, adapter.source);
        assert!(observations[0]
            .unsupported_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("poisoned")));
    }

    #[test]
    fn transactional_concurrency_rolls_back_to_previous_width() {
        let adapter = TransactionalConcurrency::new("workers", 8, 4).unwrap();
        let mut actuator = adapter.clone();
        let resource = adapter.ir().unwrap();
        let plan = plan_with_context(
            &ConcurrencyTargetPlanner(2),
            &resource,
            &PlanningContext::new(),
        );
        let checks = actuator.validate(&plan).unwrap();
        let validated = validate_with_checks(plan, checks);
        let actuation = actuator.prepare(&validated).unwrap();
        actuator.actuate(&actuation).unwrap();
        assert_eq!(adapter.width().unwrap(), 2);
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

    #[test]
    fn transactional_concurrency_rejects_actuation_that_differs_from_prepared_target() {
        let adapter = TransactionalConcurrency::new("workers", 8, 4).unwrap();
        let mut actuator = adapter.clone();
        let resource = adapter.ir().unwrap();
        let plan = plan_with_context(
            &ConcurrencyTargetPlanner(2),
            &resource,
            &PlanningContext::new(),
        );
        let checks = actuator.validate(&plan).unwrap();
        let validated = validate_with_checks(plan, checks);
        let actuation = actuator.prepare(&validated).unwrap();
        let forged = Actuation::new(validated, Some(3), actuator.name());

        assert!(actuator.actuate(&forged).is_err());
        assert_eq!(adapter.width().unwrap(), 4);
        actuator
            .rollback(
                &actuation,
                &VerificationResult::Inconclusive {
                    detail: "forged actuation rejected".into(),
                },
            )
            .unwrap();
    }
}
