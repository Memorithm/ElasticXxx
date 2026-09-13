//! Fail-closed Boolean prechecks for semantic invariants.
//!
//! This layer can reject or defer a candidate before trusted validation, but a
//! successful precheck never authorizes actuation and never marks a plan as
//! validated. [`crate::validate_with_checks`] remains the authoritative runtime
//! invariant gate and adapters still revalidate immediately before effects.

use crate::plan::invariant_applies_to_candidate;
use crate::{FactFreshnessError, FactSnapshot, Plan};
use elastic_core::resource::{Invariant, LogicalResourceId};
use elastic_core::{
    FreshnessSnapshot, GuardFactSource, InvariantPredicateBinding, PredicateKey, TruthValue,
    MAX_REGISTERED_PREDICATES,
};
use std::collections::BTreeMap;
use std::fmt;

/// Aggregate result of an invariant Boolean precheck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvariantPrecheckStatus {
    /// The plan contains no transition candidate, so no invariant precheck is
    /// applicable.
    NoCandidate,
    /// The plan contains a candidate that is not declared by its resource.
    InvalidCandidate,
    /// Every applicable invariant has an explicitly bound predicate that is
    /// currently `True`. This permits only continued trusted validation.
    Passed,
    /// At least one applicable invariant predicate is explicitly `False`.
    Rejected,
    /// No applicable predicate is false, but at least one is missing or
    /// `Unknown`.
    InsufficientEvidence,
}

/// One invariant and the Boolean fact used only for its early precheck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantPrecheckEntry {
    invariant: Invariant,
    predicate: Option<PredicateKey>,
    truth: TruthValue,
}

impl InvariantPrecheckEntry {
    /// Semantic invariant being prechecked.
    #[must_use]
    pub const fn invariant(&self) -> &Invariant {
        &self.invariant
    }

    /// Stable predicate bound to the invariant, or `None` when no binding was
    /// supplied and the result is therefore `Unknown`.
    #[must_use]
    pub const fn predicate(&self) -> Option<&PredicateKey> {
        self.predicate.as_ref()
    }

    /// Three-valued fact observed for this invariant precheck.
    #[must_use]
    pub const fn truth(&self) -> TruthValue {
        self.truth
    }
}

/// Deterministic report for the invariants applicable to one planned candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantPrecheckReport {
    status: InvariantPrecheckStatus,
    entries: Vec<InvariantPrecheckEntry>,
}

impl InvariantPrecheckReport {
    /// Aggregate precheck disposition.
    #[must_use]
    pub const fn status(&self) -> InvariantPrecheckStatus {
        self.status
    }

    /// Applicable invariant entries in the resource's canonical invariant
    /// order.
    #[must_use]
    pub fn entries(&self) -> &[InvariantPrecheckEntry] {
        &self.entries
    }

    /// Whether trusted validation may continue.
    ///
    /// `true` does not mean the plan is validated; it only means the Boolean
    /// precheck found no reason to stop before [`crate::validate_with_checks`].
    #[must_use]
    pub const fn may_continue_to_trusted_validation(&self) -> bool {
        matches!(self.status, InvariantPrecheckStatus::Passed)
    }
}

/// Construction/provenance failures that prevent a trustworthy invariant
/// precheck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvariantPrecheckError {
    /// Too many invariant bindings were supplied for the current compact
    /// Boolean predicate capacity.
    TooManyBindings {
        /// Maximum supported bindings.
        max: usize,
        /// Requested bindings.
        actual: usize,
    },
    /// The same semantic invariant was bound more than once.
    DuplicateBinding {
        /// Duplicated invariant.
        invariant: Invariant,
    },
    /// Resource-specific invariant facts must name the resource generation they
    /// were derived from.
    MissingResourceBinding,
    /// The fact snapshot belongs to another logical resource.
    ResourceBindingMismatch {
        /// Resource carried by the fact snapshot.
        snapshot: LogicalResourceId,
        /// Resource whose plan is being prechecked.
        requested: LogicalResourceId,
    },
    /// The fact snapshot no longer matches the trusted epoch/generation state.
    StaleFacts(FactFreshnessError),
}

impl fmt::Display for InvariantPrecheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyBindings { max, actual } => write!(
                f,
                "invariant precheck requested {actual} bindings; maximum is {max}"
            ),
            Self::DuplicateBinding { invariant } => {
                write!(f, "invariant {invariant} has more than one Boolean binding")
            }
            Self::MissingResourceBinding => {
                f.write_str("invariant Boolean precheck requires a resource-bound fact snapshot")
            }
            Self::ResourceBindingMismatch {
                snapshot,
                requested,
            } => write!(
                f,
                "invariant fact snapshot is bound to resource {} but plan uses resource {}",
                snapshot.as_str(),
                requested.as_str()
            ),
            Self::StaleFacts(error) => write!(f, "stale invariant fact snapshot: {error}"),
        }
    }
}

impl std::error::Error for InvariantPrecheckError {}

impl From<FactFreshnessError> for InvariantPrecheckError {
    fn from(value: FactFreshnessError) -> Self {
        Self::StaleFacts(value)
    }
}

/// Evaluate Boolean facts for exactly the invariants applicable to the planned
/// candidate.
///
/// Missing bindings and missing facts become `Unknown`. Any explicit `False`
/// rejects the precheck. Only all-`True` applicable facts produce `Passed`, and
/// even that status grants no validation authority.
///
/// # Errors
///
/// Returns a typed error for duplicate/oversized bindings, missing or mismatched
/// resource provenance, or stale fact epochs/generations.
pub fn precheck_plan_invariants(
    plan: &Plan,
    bindings: &[InvariantPredicateBinding],
    facts: &FactSnapshot,
    freshness: &FreshnessSnapshot,
) -> Result<InvariantPrecheckReport, InvariantPrecheckError> {
    let Some(candidate) = plan.candidate() else {
        return Ok(InvariantPrecheckReport {
            status: InvariantPrecheckStatus::NoCandidate,
            entries: Vec::new(),
        });
    };

    if !candidate.is_declared_in(&plan.resource) {
        return Ok(InvariantPrecheckReport {
            status: InvariantPrecheckStatus::InvalidCandidate,
            entries: Vec::new(),
        });
    }

    if bindings.len() > MAX_REGISTERED_PREDICATES {
        return Err(InvariantPrecheckError::TooManyBindings {
            max: MAX_REGISTERED_PREDICATES,
            actual: bindings.len(),
        });
    }

    let Some(resource_binding) = facts.resource_binding() else {
        return Err(InvariantPrecheckError::MissingResourceBinding);
    };
    if resource_binding.resource() != plan.resource.identity() {
        return Err(InvariantPrecheckError::ResourceBindingMismatch {
            snapshot: resource_binding.resource().clone(),
            requested: plan.resource.identity().clone(),
        });
    }
    facts.validate_freshness(freshness)?;

    let mut by_invariant = BTreeMap::new();
    for binding in bindings {
        if by_invariant
            .insert(binding.invariant().clone(), binding.predicate().clone())
            .is_some()
        {
            return Err(InvariantPrecheckError::DuplicateBinding {
                invariant: binding.invariant().clone(),
            });
        }
    }

    let mut entries = Vec::new();
    for invariant in plan
        .resource
        .invariants()
        .iter()
        .filter(|invariant| invariant_applies_to_candidate(invariant, candidate))
    {
        match by_invariant.get(invariant) {
            Some(predicate) => entries.push(InvariantPrecheckEntry {
                invariant: invariant.clone(),
                predicate: Some(predicate.clone()),
                truth: facts.truth(predicate),
            }),
            None => entries.push(InvariantPrecheckEntry {
                invariant: invariant.clone(),
                predicate: None,
                truth: TruthValue::Unknown,
            }),
        }
    }

    let status = if entries.iter().any(|entry| entry.truth == TruthValue::False) {
        InvariantPrecheckStatus::Rejected
    } else if entries
        .iter()
        .any(|entry| entry.truth == TruthValue::Unknown)
    {
        InvariantPrecheckStatus::InsufficientEvidence
    } else {
        InvariantPrecheckStatus::Passed
    };

    Ok(InvariantPrecheckReport { status, entries })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        plan_with_context, validate_with_checks, CapabilityPredicate, FactResourceBinding,
        FactSourceId, ObservationSnapshot, PredicateEvaluationInput,
    };
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, InvariantKind, ResourceClassId,
        ResourceSpec,
    };
    use elastic_core::{ObservationEpoch, PlannerEpoch, ResourceGeneration, TransitionMechanism};
    use elastic_eir::{lower, FirstGroundedPlanner, PlanningContext};
    use std::time::Instant;

    fn fixture(
        fact: Option<bool>,
    ) -> (
        Plan,
        InvariantPredicateBinding,
        FactSnapshot,
        FreshnessSnapshot,
    ) {
        let resource_id = LogicalResourceId::new("invariant-precheck").unwrap();
        let invariant = Invariant::new(InvariantKind::PreserveContents);
        let spec = ResourceSpec::builder(ResourceClassId::CAPACITY_RESOURCE, resource_id.clone())
            .allow(DimensionId::CAPACITY)
            .preserve(invariant.clone())
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
        let document = lower(&spec).unwrap();
        let resource = document.resources()[0].clone();
        let plan = plan_with_context(&FirstGroundedPlanner, &resource, &PlanningContext::new());

        let predicate = PredicateKey::new("elastic.invariant", "contents-preserved").unwrap();
        let binding = InvariantPredicateBinding::new(invariant, predicate.clone());
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, Vec::new());
        let empty_context = PlanningContext::new();
        let input = PredicateEvaluationInput::new(&empty_context, &observations, now);
        let evaluator = CapabilityPredicate::new(predicate, fact);
        let facts = FactSnapshot::derive(
            FactSourceId::new("runtime:invariant-precheck-test").unwrap(),
            ObservationEpoch::new(12),
            Some(FactResourceBinding::new(
                resource_id.clone(),
                ResourceGeneration::new(4),
            )),
            &input,
            &[&evaluator],
        )
        .unwrap();
        let freshness = FreshnessSnapshot::new(PlannerEpoch::new(2), ObservationEpoch::new(12))
            .with_resource_generation(resource_id, ResourceGeneration::new(4));
        (plan, binding, facts, freshness)
    }

    #[test]
    fn all_true_precheck_still_does_not_validate_without_trusted_checks() {
        let (plan, binding, facts, freshness) = fixture(Some(true));
        let report = precheck_plan_invariants(&plan, &[binding], &facts, &freshness).unwrap();
        assert_eq!(report.status(), InvariantPrecheckStatus::Passed);
        assert!(report.may_continue_to_trusted_validation());

        let validated = validate_with_checks(plan, Vec::new());
        assert!(!validated.validated);
    }

    #[test]
    fn false_fact_rejects_and_unknown_fact_defers() {
        let (false_plan, false_binding, false_facts, false_freshness) = fixture(Some(false));
        let rejected = precheck_plan_invariants(
            &false_plan,
            &[false_binding],
            &false_facts,
            &false_freshness,
        )
        .unwrap();
        assert_eq!(rejected.status(), InvariantPrecheckStatus::Rejected);

        let (unknown_plan, unknown_binding, unknown_facts, unknown_freshness) = fixture(None);
        let unknown = precheck_plan_invariants(
            &unknown_plan,
            &[unknown_binding],
            &unknown_facts,
            &unknown_freshness,
        )
        .unwrap();
        assert_eq!(
            unknown.status(),
            InvariantPrecheckStatus::InsufficientEvidence
        );
    }

    #[test]
    fn missing_binding_is_unknown_not_implicitly_true() {
        let (plan, _binding, facts, freshness) = fixture(Some(true));
        let report = precheck_plan_invariants(&plan, &[], &facts, &freshness).unwrap();
        assert_eq!(
            report.status(),
            InvariantPrecheckStatus::InsufficientEvidence
        );
        assert_eq!(report.entries().len(), 1);
        assert_eq!(report.entries()[0].predicate(), None);
        assert_eq!(report.entries()[0].truth(), TruthValue::Unknown);
    }

    #[test]
    fn duplicate_binding_is_rejected() {
        let (plan, binding, facts, freshness) = fixture(Some(true));
        let error = precheck_plan_invariants(
            &plan,
            &[binding.clone(), binding.clone()],
            &facts,
            &freshness,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            InvariantPrecheckError::DuplicateBinding { .. }
        ));
    }

    #[test]
    fn dimension_scoped_invariant_uses_same_applicability_as_trusted_validation() {
        let resource_id = LogicalResourceId::new("invariant-scope").unwrap();
        let global = Invariant::new(InvariantKind::PreserveContents);
        let residency =
            Invariant::new(InvariantKind::PreserveIdentity).along(DimensionId::RESIDENCY);
        let spec = ResourceSpec::builder(ResourceClassId::CAPACITY_RESOURCE, resource_id.clone())
            .allow(DimensionId::CAPACITY)
            .allow(DimensionId::RESIDENCY)
            .preserve(global.clone())
            .preserve(residency)
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
        let document = lower(&spec).unwrap();
        let resource = document.resources()[0].clone();
        let plan = plan_with_context(&FirstGroundedPlanner, &resource, &PlanningContext::new());
        let predicate = PredicateKey::new("elastic.invariant", "scope-global").unwrap();
        let binding = InvariantPredicateBinding::new(global.clone(), predicate.clone());
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, Vec::new());
        let empty_context = PlanningContext::new();
        let input = PredicateEvaluationInput::new(&empty_context, &observations, now);
        let evaluator = CapabilityPredicate::new(predicate, Some(true));
        let facts = FactSnapshot::derive(
            FactSourceId::new("runtime:invariant-scope-test").unwrap(),
            ObservationEpoch::new(3),
            Some(FactResourceBinding::new(
                resource_id.clone(),
                ResourceGeneration::new(6),
            )),
            &input,
            &[&evaluator],
        )
        .unwrap();
        let freshness = FreshnessSnapshot::new(PlannerEpoch::new(1), ObservationEpoch::new(3))
            .with_resource_generation(resource_id, ResourceGeneration::new(6));

        let report = precheck_plan_invariants(&plan, &[binding], &facts, &freshness).unwrap();
        assert_eq!(report.status(), InvariantPrecheckStatus::Passed);
        assert_eq!(report.entries().len(), 1);

        let validated =
            validate_with_checks(plan, vec![crate::InvariantCheck::new(global, true, None)]);
        assert!(validated.validated);
    }
}
