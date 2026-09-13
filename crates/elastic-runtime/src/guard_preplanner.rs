//! Runtime bridge from provenance-bound fact snapshots to Boolean EIR pruning.
//!
//! This module is intentionally a preplanner rather than an optimizer. It
//! rejects stale fact snapshots before guard evaluation, then delegates the
//! actual partitioning to the single EIR guard semantics. Numeric planners may
//! consume only the returned `eligible` partition.

use crate::{FactFreshnessError, FactSnapshot};
use elastic_core::resource::LogicalResourceId;
use elastic_core::{FreshnessSnapshot, LogicError};
use elastic_eir::{prune_transition_candidates, EirGuardedResource, TransitionPruningReport};
use std::fmt;

/// Failures that prevent Boolean candidate pruning from producing a trustworthy
/// eligibility report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardPreplannerError {
    /// Runtime facts no longer match the trusted observation/resource state.
    StaleFacts(FactFreshnessError),
    /// Facts are provenance-bound to a different logical resource.
    ResourceBindingMismatch {
        /// Resource identity bound to the fact snapshot.
        snapshot: LogicalResourceId,
        /// Resource identity requested for pruning.
        requested: LogicalResourceId,
    },
    /// Guard evaluation failed inside the bounded Boolean representation.
    Logic(LogicError),
}

impl fmt::Display for GuardPreplannerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleFacts(error) => write!(f, "stale Boolean fact snapshot: {error}"),
            Self::ResourceBindingMismatch {
                snapshot,
                requested,
            } => write!(
                f,
                "Boolean fact snapshot is bound to resource {} but pruning requested resource {}",
                snapshot.as_str(),
                requested.as_str()
            ),
            Self::Logic(error) => write!(f, "Boolean guard evaluation failed: {error}"),
        }
    }
}

impl std::error::Error for GuardPreplannerError {}

impl From<FactFreshnessError> for GuardPreplannerError {
    fn from(value: FactFreshnessError) -> Self {
        Self::StaleFacts(value)
    }
}

impl From<LogicError> for GuardPreplannerError {
    fn from(value: LogicError) -> Self {
        Self::Logic(value)
    }
}

/// Stateless runtime preplanner for Boolean eligibility.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BooleanGuardPreplanner;

impl BooleanGuardPreplanner {
    /// Validate fact provenance/freshness, then partition the resource's
    /// declared transitions into eligible, rejected, and unknown sets.
    ///
    /// No numeric objective or planner is executed by this method.
    ///
    /// # Errors
    ///
    /// Returns [`GuardPreplannerError::ResourceBindingMismatch`] before guard
    /// evaluation when facts are bound to a different logical resource.
    /// Returns [`GuardPreplannerError::StaleFacts`] when the fact snapshot is
    /// stale. Logic errors are propagated fail closed.
    pub fn prune(
        &self,
        resource: &EirGuardedResource,
        facts: &FactSnapshot,
        freshness: &FreshnessSnapshot,
    ) -> Result<TransitionPruningReport, GuardPreplannerError> {
        if let Some(binding) = facts.resource_binding() {
            let requested = resource.resource().identity();
            if binding.resource() != requested {
                return Err(GuardPreplannerError::ResourceBindingMismatch {
                    snapshot: binding.resource().clone(),
                    requested: requested.clone(),
                });
            }
        }
        facts.validate_freshness(freshness)?;
        Ok(prune_transition_candidates(resource, facts)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilityPredicate, FactResourceBinding, FactSourceId, ObservationSnapshot,
        PredicateEvaluationInput,
    };
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::{
        BoolExpr, BooleanGuard, GuardScope, GuardedResourceSpec, ObservationEpoch, PlannerEpoch,
        PredicateKey, PredicateRegistry, ResourceGeneration, TransitionMechanism,
    };
    use elastic_eir::{lower_guarded, PlanningContext};
    use std::time::Instant;

    fn fixture() -> (
        EirGuardedResource,
        FactSnapshot,
        FreshnessSnapshot,
        LogicalResourceId,
    ) {
        let resource_id = LogicalResourceId::new("runtime-preplanner").unwrap();
        let spec = ResourceSpec::builder(ResourceClassId::CAPACITY_RESOURCE, resource_id.clone())
            .allow(DimensionId::CAPACITY)
            .admit(AdmissibleTransition::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
            ))
            .require_capability(CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
            ))
            .build()
            .unwrap();

        let key = PredicateKey::new("elastic.preplanner", "capacity-ok").unwrap();
        let registry = PredicateRegistry::from_keys([key.clone()]).unwrap();
        let predicate_id = registry.id(&key).unwrap();
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CAPACITY,
            },
            registry,
            BoolExpr::atom(predicate_id),
        )
        .unwrap();
        let resource =
            lower_guarded(&GuardedResourceSpec::new(spec, vec![guard]).unwrap()).unwrap();

        let now = Instant::now();
        let context = PlanningContext::new();
        let observations = ObservationSnapshot::new(now, Vec::new());
        let input = PredicateEvaluationInput::new(&context, &observations, now);
        let evaluator = CapabilityPredicate::new(key, Some(true));
        let facts = FactSnapshot::derive(
            FactSourceId::new("runtime:preplanner-test").unwrap(),
            ObservationEpoch::new(4),
            Some(FactResourceBinding::new(
                resource_id.clone(),
                ResourceGeneration::new(9),
            )),
            &input,
            &[&evaluator],
        )
        .unwrap();
        let freshness = FreshnessSnapshot::new(PlannerEpoch::new(2), ObservationEpoch::new(4))
            .with_resource_generation(resource_id.clone(), ResourceGeneration::new(9));
        (resource, facts, freshness, resource_id)
    }

    fn same_guard_different_resource() -> EirGuardedResource {
        let resource_id = LogicalResourceId::new("runtime-preplanner-other").unwrap();
        let spec = ResourceSpec::builder(ResourceClassId::CAPACITY_RESOURCE, resource_id)
            .allow(DimensionId::CAPACITY)
            .admit(AdmissibleTransition::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
            ))
            .require_capability(CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
            ))
            .build()
            .unwrap();
        let key = PredicateKey::new("elastic.preplanner", "capacity-ok").unwrap();
        let registry = PredicateRegistry::from_keys([key.clone()]).unwrap();
        let predicate_id = registry.id(&key).unwrap();
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CAPACITY,
            },
            registry,
            BoolExpr::atom(predicate_id),
        )
        .unwrap();
        lower_guarded(&GuardedResourceSpec::new(spec, vec![guard]).unwrap()).unwrap()
    }

    #[test]
    fn fresh_facts_prune_before_numeric_planning() {
        let (resource, facts, freshness, _) = fixture();
        let report = BooleanGuardPreplanner
            .prune(&resource, &facts, &freshness)
            .unwrap();
        assert_eq!(report.eligible().len(), 1);
        assert!(report.rejected().is_empty());
        assert!(report.unknown().is_empty());
    }

    #[test]
    fn facts_bound_to_another_resource_are_rejected_before_guard_evaluation() {
        let (_resource, facts, freshness, resource_id) = fixture();
        let other = same_guard_different_resource();
        assert!(matches!(
            BooleanGuardPreplanner.prune(&other, &facts, &freshness),
            Err(GuardPreplannerError::ResourceBindingMismatch {
                snapshot,
                requested,
            }) if snapshot == resource_id && requested == *other.resource().identity()
        ));
    }

    #[test]
    fn stale_observation_epoch_is_rejected_before_guard_evaluation() {
        let (resource, facts, _freshness, resource_id) = fixture();
        let stale = FreshnessSnapshot::new(PlannerEpoch::new(2), ObservationEpoch::new(5))
            .with_resource_generation(resource_id, ResourceGeneration::new(9));
        assert!(matches!(
            BooleanGuardPreplanner.prune(&resource, &facts, &stale),
            Err(GuardPreplannerError::StaleFacts(
                FactFreshnessError::ObservationEpochMismatch { .. }
            ))
        ));
    }

    #[test]
    fn changed_resource_generation_is_rejected_before_guard_evaluation() {
        let (resource, facts, _freshness, resource_id) = fixture();
        let stale = FreshnessSnapshot::new(PlannerEpoch::new(2), ObservationEpoch::new(4))
            .with_resource_generation(resource_id, ResourceGeneration::new(10));
        assert!(matches!(
            BooleanGuardPreplanner.prune(&resource, &facts, &stale),
            Err(GuardPreplannerError::StaleFacts(
                FactFreshnessError::ResourceGenerationMismatch { .. }
            ))
        ));
    }
}
