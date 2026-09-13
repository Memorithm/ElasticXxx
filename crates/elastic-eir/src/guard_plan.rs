//! Fail-closed transition eligibility over guarded EIR.
//!
//! Guard evaluation is a pre-planning filter. It can reject an admitted
//! transition or report insufficient evidence, but it cannot manufacture a
//! transition or bypass capability grounding. Numeric objective ranking remains
//! a later concern for candidates that survive this filter.

use crate::resource::AdmittedTransition;
use crate::{EirGuard, EirGuardedResource, TransitionCandidate};
use elastic_core::{FactSet, GuardFactSource, GuardScope, LogicError, TruthValue};

/// Result of applying all Boolean guards relevant to one admitted transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardedTransitionOutcome {
    /// The transition is declared, capability-grounded, and every applicable
    /// guard evaluated to `True`.
    Eligible(TransitionCandidate),
    /// At least one applicable guard evaluated to `False`.
    Rejected {
        /// The already-declared transition candidate that was rejected.
        candidate: TransitionCandidate,
        /// First failed scope in deterministic guard order.
        failed_scope: GuardScope,
    },
    /// The transition is declared but cannot yet be justified because at least
    /// one applicable guard is `Unknown` or capability grounding is absent.
    InsufficientEvidence {
        /// The already-declared transition candidate under consideration.
        candidate: TransitionCandidate,
        /// Applicable guard scopes that evaluated to `Unknown`.
        unknown_scopes: Vec<GuardScope>,
        /// Whether the underlying admission is capability-grounded.
        capability_grounded: bool,
    },
    /// The supplied admission is not a member of this guarded resource.
    NotDeclared,
}

impl GuardedTransitionOutcome {
    /// Whether this outcome carries a candidate eligible for later numeric
    /// ranking. This never returns true for rejected, unknown, ungrounded, or
    /// foreign admissions.
    #[must_use]
    pub fn is_eligible(&self) -> bool {
        matches!(self, Self::Eligible(_))
    }
}

/// Evaluate resource-, dimension-, and transition-scoped guards for one
/// admission.
///
/// Composition is conjunctive and deterministic: resource guards apply first,
/// followed by matching dimension and transition guards according to canonical
/// scope order. `False` rejects immediately. `Unknown` is accumulated and
/// yields [`GuardedTransitionOutcome::InsufficientEvidence`] unless a later
/// applicable guard is explicitly `False`. Only all-`True` evidence plus
/// capability grounding yields `Eligible`.
///
/// # Errors
///
/// Returns [`LogicError`] only if the internally validated compact predicate
/// table cannot be materialized into a [`FactSet`].
pub fn evaluate_transition_guards(
    resource: &EirGuardedResource,
    admitted: &AdmittedTransition,
    source: &impl GuardFactSource,
) -> Result<GuardedTransitionOutcome, LogicError> {
    let declared = resource
        .resource()
        .transitions()
        .iter()
        .any(|candidate| candidate == admitted);
    if !declared {
        return Ok(GuardedTransitionOutcome::NotDeclared);
    }

    let candidate = TransitionCandidate::from_admitted(admitted);
    let mechanism = admitted.transition().mechanism();
    let dimension = admitted.transition().dimension();
    let mut unknown_scopes = Vec::new();

    for guard in resource
        .guards()
        .iter()
        .filter(|guard| guard.scope().applies_to(mechanism, dimension))
    {
        match evaluate_eir_guard(guard, source)? {
            TruthValue::True => {}
            TruthValue::False => {
                return Ok(GuardedTransitionOutcome::Rejected {
                    candidate,
                    failed_scope: guard.scope().clone(),
                });
            }
            TruthValue::Unknown => unknown_scopes.push(guard.scope().clone()),
        }
    }

    if admitted.capability_grounded() && unknown_scopes.is_empty() {
        Ok(GuardedTransitionOutcome::Eligible(candidate))
    } else {
        Ok(GuardedTransitionOutcome::InsufficientEvidence {
            candidate,
            unknown_scopes,
            capability_grounded: admitted.capability_grounded(),
        })
    }
}

fn evaluate_eir_guard(
    guard: &EirGuard,
    source: &impl GuardFactSource,
) -> Result<TruthValue, LogicError> {
    let mut facts = FactSet::new();
    for predicate in guard.predicates() {
        facts.set(predicate.id(), source.truth(predicate.key()))?;
    }
    guard.expression().evaluate(&facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower_guarded;
    use elastic_core::resource::{
        AdmissibleTransition, DimensionId, LogicalResourceId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::{
        BoolExpr, BooleanGuard, GuardedResourceSpec, PredicateKey, PredicateRegistry,
        TransitionMechanism,
    };
    use std::collections::BTreeMap;

    fn fixture() -> (EirGuardedResource, Vec<PredicateKey>) {
        let resource = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("guard-plan").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .allow(DimensionId::RESIDENCY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .require_capability(elastic_core::resource::CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap();

        let keys = vec![
            PredicateKey::new("elastic.guard", "resource-ok").unwrap(),
            PredicateKey::new("elastic.guard", "capacity-ok").unwrap(),
            PredicateKey::new("elastic.guard", "transition-ok").unwrap(),
            PredicateKey::new("elastic.guard", "residency-ok").unwrap(),
        ];
        let registry = PredicateRegistry::from_keys(keys.clone()).unwrap();
        let resource_ok = registry.id(&keys[0]).unwrap();
        let capacity_ok = registry.id(&keys[1]).unwrap();
        let transition_ok = registry.id(&keys[2]).unwrap();
        let residency_ok = registry.id(&keys[3]).unwrap();
        let guards = vec![
            BooleanGuard::when(
                GuardScope::Resource,
                registry.clone(),
                BoolExpr::atom(resource_ok),
            )
            .unwrap(),
            BooleanGuard::requires(
                GuardScope::Dimension(DimensionId::CAPACITY),
                registry.clone(),
                capacity_ok,
            )
            .unwrap(),
            BooleanGuard::requires(
                GuardScope::Transition {
                    mechanism: TransitionMechanism::Reinterpret,
                    dimension: DimensionId::CAPACITY,
                },
                registry.clone(),
                transition_ok,
            )
            .unwrap(),
            BooleanGuard::requires(
                GuardScope::Dimension(DimensionId::RESIDENCY),
                registry,
                residency_ok,
            )
            .unwrap(),
        ];
        let guarded = GuardedResourceSpec::new(resource, guards).unwrap();
        (lower_guarded(&guarded).unwrap(), keys)
    }

    fn transition(resource: &EirGuardedResource) -> &AdmittedTransition {
        &resource.resource().transitions()[0]
    }

    #[test]
    fn all_applicable_true_yields_eligible_and_ignores_other_dimension() {
        let (resource, keys) = fixture();
        let facts = BTreeMap::from([
            (keys[0].clone(), TruthValue::True),
            (keys[1].clone(), TruthValue::True),
            (keys[2].clone(), TruthValue::True),
        ]);
        let outcome = evaluate_transition_guards(&resource, transition(&resource), &facts).unwrap();
        assert!(outcome.is_eligible());
    }

    #[test]
    fn false_rejects_and_unknown_fails_closed() {
        let (resource, keys) = fixture();
        let false_facts = BTreeMap::from([
            (keys[0].clone(), TruthValue::True),
            (keys[1].clone(), TruthValue::False),
            (keys[2].clone(), TruthValue::True),
        ]);
        assert!(matches!(
            evaluate_transition_guards(&resource, transition(&resource), &false_facts).unwrap(),
            GuardedTransitionOutcome::Rejected { .. }
        ));

        let unknown_facts = BTreeMap::from([
            (keys[0].clone(), TruthValue::True),
            (keys[1].clone(), TruthValue::True),
        ]);
        assert!(matches!(
            evaluate_transition_guards(&resource, transition(&resource), &unknown_facts).unwrap(),
            GuardedTransitionOutcome::InsufficientEvidence {
                capability_grounded: true,
                ..
            }
        ));
    }

    #[test]
    fn foreign_admission_is_never_made_eligible() {
        let (resource, keys) = fixture();
        let foreign_resource = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("foreign").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Recompute,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap();
        let foreign = crate::lower(&foreign_resource).unwrap();
        let foreign_admission = &foreign.resources()[0].transitions()[0];
        let facts = BTreeMap::from([
            (keys[0].clone(), TruthValue::True),
            (keys[1].clone(), TruthValue::True),
            (keys[2].clone(), TruthValue::True),
        ]);
        assert_eq!(
            evaluate_transition_guards(&resource, foreign_admission, &facts).unwrap(),
            GuardedTransitionOutcome::NotDeclared
        );
    }
}
