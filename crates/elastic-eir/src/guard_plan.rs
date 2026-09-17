//! Fail-closed transition eligibility and candidate pruning over guarded EIR.
//!
//! Guard evaluation is a pre-planning filter. It can reject an admitted
//! transition or report insufficient evidence, but it cannot manufacture a
//! transition or bypass capability grounding. Numeric objective ranking remains
//! a later concern for candidates that survive this filter.

use crate::resource::AdmittedTransition;
use crate::{EirGuard, EirGuardedResource, Fingerprint, TransitionCandidate};
use elastic_core::resource::DimensionId;
use elastic_core::{
    FactSet, GuardFactSource, GuardScope, LogicError, TransitionMechanism, TruthValue,
};

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

/// One transition eliminated by an explicit false guard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedTransition {
    candidate: TransitionCandidate,
    failed_scope: GuardScope,
}

impl RejectedTransition {
    /// Candidate removed from later numeric planning.
    #[must_use]
    pub const fn candidate(&self) -> &TransitionCandidate {
        &self.candidate
    }

    /// First deterministic scope that evaluated to `False`.
    #[must_use]
    pub const fn failed_scope(&self) -> &GuardScope {
        &self.failed_scope
    }
}

/// One transition retained as unknown rather than incorrectly accepted or
/// rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownTransition {
    candidate: TransitionCandidate,
    unknown_scopes: Vec<GuardScope>,
    capability_grounded: bool,
}

impl UnknownTransition {
    /// Candidate for which current evidence is insufficient.
    #[must_use]
    pub const fn candidate(&self) -> &TransitionCandidate {
        &self.candidate
    }

    /// Applicable guard scopes whose result is `Unknown`.
    #[must_use]
    pub fn unknown_scopes(&self) -> &[GuardScope] {
        &self.unknown_scopes
    }

    /// Whether the declaration has a matching required capability path.
    #[must_use]
    pub const fn capability_grounded(&self) -> bool {
        self.capability_grounded
    }
}

/// Deterministic partition of the resource's declared transition set.
///
/// Each declared admission is classified exactly once into `eligible`,
/// `rejected`, or `unknown`. Entries preserve the canonical order of
/// [`crate::EirResource::transitions`] within each partition. This report is
/// deliberately not an optimizer: callers rank only `eligible` candidates.
///
/// Reports produced by pruning retain their complete guarded-EIR source. A
/// default report has no source binding and cannot construct a planning view.
/// This is structural identity, not authentication or runtime freshness.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransitionPruningReport {
    source: Option<EirGuardedResource>,
    eligible: Vec<TransitionCandidate>,
    rejected: Vec<RejectedTransition>,
    unknown: Vec<UnknownTransition>,
}

impl TransitionPruningReport {
    /// Whether this report was derived for exactly this guarded declaration.
    /// Includes resource identity, contents and guard policy, not just transition
    /// pairs or a non-cryptographic fingerprint. Freshness is checked separately
    /// by the runtime before deriving the report.
    #[must_use]
    pub fn is_for_resource(&self, resource: &EirGuardedResource) -> bool {
        self.source
            .as_ref()
            .is_some_and(|source| source == resource)
    }

    /// Candidates permitted to reach later numeric ranking.
    #[must_use]
    pub fn eligible(&self) -> &[TransitionCandidate] {
        &self.eligible
    }

    /// Candidates removed by explicit false guards.
    #[must_use]
    pub fn rejected(&self) -> &[RejectedTransition] {
        &self.rejected
    }

    /// Candidates blocked by missing evidence or missing capability grounding.
    #[must_use]
    pub fn unknown(&self) -> &[UnknownTransition] {
        &self.unknown
    }

    /// Number of declared admissions classified by this report.
    #[must_use]
    pub fn total_classified(&self) -> usize {
        self.eligible.len() + self.rejected.len() + self.unknown.len()
    }

    /// Whether an eligible transition matches this mechanism/dimension pair.
    #[must_use]
    pub fn contains_eligible(
        &self,
        mechanism: TransitionMechanism,
        dimension: &elastic_core::resource::DimensionId,
    ) -> bool {
        self.eligible.iter().any(|candidate| {
            candidate.mechanism() == mechanism && candidate.dimension() == dimension
        })
    }

    /// First eligible candidate in canonical admission order.
    #[must_use]
    pub fn first_eligible(&self) -> Option<&TransitionCandidate> {
        self.eligible.first()
    }

    /// Structural identity of this source-bound pruning result.
    ///
    /// Default/unbound reports return `None`. The fingerprint absorbs the full
    /// original guarded-resource identity plus each classified candidate and
    /// its guard evidence in deterministic partition order. Built-in and custom
    /// dimensions are explicitly discriminated even when they share canonical
    /// text. This is non-cryptographic diagnostic identity, not authentication
    /// and not runtime freshness evidence.
    #[must_use]
    pub fn fingerprint(&self) -> Option<Fingerprint> {
        let source = self.source.as_ref()?;
        let mut fingerprint = Fingerprint::EMPTY
            .text("transition-pruning-report")
            .number(1)
            .number(source.fingerprint().bits());
        fingerprint = fingerprint.number(self.eligible.len() as u64);
        for candidate in &self.eligible {
            fingerprint = fingerprint_candidate(fingerprint.text("eligible"), candidate);
        }
        fingerprint = fingerprint.number(self.rejected.len() as u64);
        for rejected in &self.rejected {
            fingerprint = fingerprint_candidate(fingerprint.text("rejected"), rejected.candidate());
            fingerprint = fingerprint_guard_scope(fingerprint, rejected.failed_scope());
        }
        fingerprint = fingerprint.number(self.unknown.len() as u64);
        for unknown in &self.unknown {
            fingerprint = fingerprint_candidate(fingerprint.text("unknown"), unknown.candidate());
            fingerprint = fingerprint.number(u64::from(unknown.capability_grounded()));
            fingerprint = fingerprint.number(unknown.unknown_scopes().len() as u64);
            for scope in unknown.unknown_scopes() {
                fingerprint = fingerprint_guard_scope(fingerprint, scope);
            }
        }
        Some(fingerprint)
    }
}

fn fingerprint_candidate(
    mut fingerprint: Fingerprint,
    candidate: &TransitionCandidate,
) -> Fingerprint {
    fingerprint = fingerprint
        .text(mechanism_text(candidate.mechanism()))
        .text(dimension_kind(candidate.dimension()))
        .text(candidate.dimension().as_str())
        .number(u64::from(candidate.capability_grounded()));
    match candidate.magnitude() {
        Some(magnitude) => fingerprint.text("magnitude").number(magnitude),
        None => fingerprint.text("no-magnitude"),
    }
}

fn fingerprint_guard_scope(mut fingerprint: Fingerprint, scope: &GuardScope) -> Fingerprint {
    match scope {
        GuardScope::Resource => fingerprint.text("scope-resource"),
        GuardScope::Dimension(dimension) => fingerprint
            .text("scope-dimension")
            .text(dimension_kind(dimension))
            .text(dimension.as_str()),
        GuardScope::Transition {
            mechanism,
            dimension,
        } => {
            fingerprint = fingerprint
                .text("scope-transition")
                .text(mechanism_text(*mechanism))
                .text(dimension_kind(dimension));
            fingerprint.text(dimension.as_str())
        }
    }
}

const fn dimension_kind(dimension: &DimensionId) -> &'static str {
    if dimension.builtin_part().is_some() {
        "builtin"
    } else {
        "custom"
    }
}

const fn mechanism_text(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
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

/// Classify every declared transition before any numeric objective ranking.
///
/// The output is a partition of the existing admitted set, bound to the full
/// guarded declaration used here. It can only shrink the set reaching later
/// planning and never constructs candidates from external transition pairs.
///
/// # Errors
///
/// Returns [`LogicError`] if one internally validated predicate table cannot be
/// materialized into the compact fact representation.
pub fn prune_transition_candidates(
    resource: &EirGuardedResource,
    source: &impl GuardFactSource,
) -> Result<TransitionPruningReport, LogicError> {
    let mut report = TransitionPruningReport {
        source: Some(resource.clone()),
        ..TransitionPruningReport::default()
    };
    for admitted in resource.resource().transitions() {
        match evaluate_transition_guards(resource, admitted, source)? {
            GuardedTransitionOutcome::Eligible(candidate) => report.eligible.push(candidate),
            GuardedTransitionOutcome::Rejected {
                candidate,
                failed_scope,
            } => report.rejected.push(RejectedTransition {
                candidate,
                failed_scope,
            }),
            GuardedTransitionOutcome::InsufficientEvidence {
                candidate,
                unknown_scopes,
                capability_grounded,
            } => report.unknown.push(UnknownTransition {
                candidate,
                unknown_scopes,
                capability_grounded,
            }),
            GuardedTransitionOutcome::NotDeclared => {
                unreachable!("resource-owned admission must be declared in the same resource")
            }
        }
    }
    Ok(report)
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
    use crate::{lower_guarded, FirstGroundedPlanner, PlanOutcome, TransitionPlanner};
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId,
        ResourceClassId, ResourceSpec,
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
        .require_capability(CapabilityRequirement::new(
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

    #[test]
    fn pruning_partitions_declared_set_without_widening_it() {
        let (resource, keys) = fixture();
        let facts = BTreeMap::from([
            (keys[0].clone(), TruthValue::True),
            (keys[1].clone(), TruthValue::False),
            (keys[2].clone(), TruthValue::True),
        ]);
        let report = prune_transition_candidates(&resource, &facts).unwrap();
        assert_eq!(
            report.total_classified(),
            resource.resource().transitions().len()
        );
        assert!(report.eligible().is_empty());
        assert_eq!(report.rejected().len(), 1);
        assert!(report.unknown().is_empty());
        assert!(!report.contains_eligible(TransitionMechanism::Reinterpret, &DimensionId::CAPACITY));
    }

    #[test]
    fn missing_fact_is_reported_as_unknown_not_rejected() {
        let (resource, keys) = fixture();
        let facts = BTreeMap::from([(keys[0].clone(), TruthValue::True)]);
        let report = prune_transition_candidates(&resource, &facts).unwrap();
        assert!(report.eligible().is_empty());
        assert!(report.rejected().is_empty());
        assert_eq!(report.unknown().len(), 1);
        assert_eq!(report.unknown()[0].unknown_scopes().len(), 2);
    }

    #[test]
    fn pruning_report_fingerprint_is_source_bound_and_classification_sensitive() {
        let (resource, keys) = fixture();
        let true_facts = BTreeMap::from([
            (keys[0].clone(), TruthValue::True),
            (keys[1].clone(), TruthValue::True),
            (keys[2].clone(), TruthValue::True),
        ]);
        let false_facts = BTreeMap::from([
            (keys[0].clone(), TruthValue::True),
            (keys[1].clone(), TruthValue::False),
            (keys[2].clone(), TruthValue::True),
        ]);
        let first = prune_transition_candidates(&resource, &true_facts).unwrap();
        let second = prune_transition_candidates(&resource, &true_facts).unwrap();
        let rejected = prune_transition_candidates(&resource, &false_facts).unwrap();

        assert_eq!(first.fingerprint(), second.fingerprint());
        assert_ne!(first.fingerprint(), rejected.fingerprint());
        assert!(first.fingerprint().is_some());
        assert_eq!(TransitionPruningReport::default().fingerprint(), None);
    }

    #[test]
    fn pruning_report_fingerprint_distinguishes_builtin_from_same_text_custom_dimension() {
        fn lowered(dimension: DimensionId, id: &str) -> EirGuardedResource {
            let spec = ResourceSpec::builder(
                ResourceClassId::CAPACITY_RESOURCE,
                LogicalResourceId::new(id).unwrap(),
            )
            .allow(dimension.clone())
            .admit(AdmissibleTransition::new(
                TransitionMechanism::Reinterpret,
                dimension.clone(),
            ))
            .require_capability(CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                dimension,
            ))
            .build()
            .unwrap();
            lower_guarded(&GuardedResourceSpec::new(spec, Vec::new()).unwrap()).unwrap()
        }

        let builtin = lowered(DimensionId::CAPACITY, "same-text-dimension");
        let custom = lowered(
            DimensionId::custom("capacity").unwrap(),
            "same-text-dimension",
        );
        let builtin_report = prune_transition_candidates(&builtin, &BTreeMap::new()).unwrap();
        let custom_report = prune_transition_candidates(&custom, &BTreeMap::new()).unwrap();

        assert_ne!(builtin_report.fingerprint(), custom_report.fingerprint());
    }

    #[test]
    fn empty_guard_set_preserves_first_grounded_candidate() {
        let resource = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("tautology-differential").unwrap(),
        )
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
        let guarded = GuardedResourceSpec::new(resource, Vec::new()).unwrap();
        let lowered = lower_guarded(&guarded).unwrap();
        let report = prune_transition_candidates(&lowered, &BTreeMap::new()).unwrap();
        let reference = FirstGroundedPlanner.propose_transition(lowered.resource());
        let PlanOutcome::Candidate(reference_candidate) = reference else {
            panic!("grounded fixture must produce a reference candidate")
        };
        assert_eq!(report.first_eligible(), Some(&reference_candidate));
    }
}
