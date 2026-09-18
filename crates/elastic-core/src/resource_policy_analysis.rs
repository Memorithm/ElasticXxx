//! Bounded exact static analysis for resource-bound Boolean policies.
//!
//! This module is diagnostic only. It uses the exact strong-Kleene oracle to
//! identify dead transitions, mutually exclusive transition policies, and
//! guard clauses that are semantically redundant for one admitted transition.
//! It can also compare eligibility guards with explicitly declared invariant
//! precheck predicates. None of these results validates an invariant or grants
//! actuation authority: trusted runtime validation remains mandatory.

use std::collections::BTreeMap;
use std::fmt;

use crate::resource::{AdmissibleTransition, Invariant};
use crate::{
    BoolExpr, ExactKleeneOracle, ExactOracleLimits, GuardScope, GuardedResourceSpec,
    InvariantPredicateBinding, KleeneOracleError, KleenePropertyReport, KleeneSatisfiabilityReport,
    PredicateId, PredicateKey, PredicateRegistry, PredicateRegistryError,
};

/// Maximum admitted transitions analyzed in one resource policy report.
pub const MAX_RESOURCE_POLICY_TRANSITIONS: usize = 64;
/// Maximum invariant-to-predicate bindings accepted by one analysis.
pub const MAX_RESOURCE_POLICY_INVARIANT_BINDINGS: usize = 64;
/// Maximum pair reports produced from [`MAX_RESOURCE_POLICY_TRANSITIONS`].
pub const MAX_RESOURCE_POLICY_PAIRS: usize =
    MAX_RESOURCE_POLICY_TRANSITIONS * (MAX_RESOURCE_POLICY_TRANSITIONS - 1) / 2;

/// Fail-closed errors from bounded resource-policy analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourcePolicyAnalysisError {
    /// The resource exposes more admitted transitions than this bounded pass accepts.
    TooManyTransitions { actual: usize, maximum: usize },
    /// Too many invariant bindings were supplied to this bounded pass.
    TooManyInvariantBindings { actual: usize, maximum: usize },
    /// One invariant was bound to more than one predicate, which is ambiguous here.
    DuplicateInvariantBinding { invariant: Invariant },
    /// Building the stable merged predicate registry failed.
    PredicateRegistry(PredicateRegistryError),
    /// A source expression referenced an ID absent from its declared registry.
    MissingPredicateKey { id: PredicateId },
    /// A stable key expected in a merged target registry was unexpectedly absent.
    MissingMergedPredicateKey { key: PredicateKey },
    /// An exact strong-Kleene query exceeded its declared bounds or failed evaluation.
    Oracle(KleeneOracleError),
}

impl fmt::Display for ResourcePolicyAnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyTransitions { actual, maximum } => write!(
                f,
                "resource policy analysis has {actual} transitions; maximum is {maximum}"
            ),
            Self::TooManyInvariantBindings { actual, maximum } => write!(
                f,
                "resource policy analysis has {actual} invariant bindings; maximum is {maximum}"
            ),
            Self::DuplicateInvariantBinding { invariant } => {
                write!(
                    f,
                    "invariant {invariant} has more than one predicate binding"
                )
            }
            Self::PredicateRegistry(error) => error.fmt(f),
            Self::MissingPredicateKey { id } => write!(
                f,
                "Boolean expression references predicate id {} absent from its registry",
                id.index()
            ),
            Self::MissingMergedPredicateKey { key } => write!(
                f,
                "merged predicate registry unexpectedly omitted stable key {key}"
            ),
            Self::Oracle(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ResourcePolicyAnalysisError {}

impl From<PredicateRegistryError> for ResourcePolicyAnalysisError {
    fn from(value: PredicateRegistryError) -> Self {
        Self::PredicateRegistry(value)
    }
}

impl From<KleeneOracleError> for ResourcePolicyAnalysisError {
    fn from(value: KleeneOracleError) -> Self {
        Self::Oracle(value)
    }
}

/// Diagnostic state of an invariant precheck relative to transition eligibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvariantGuardStatus {
    /// The invariant has no declared Boolean precheck binding.
    Unbound,
    /// The transition is unreachable, so implication is only vacuously true.
    VacuousUnreachable,
    /// Every explicitly eligible assignment also establishes the bound predicate.
    ImpliedForEligibility,
    /// Some explicitly eligible assignment does not establish the bound predicate.
    NotImpliedForEligibility,
}

/// Exact diagnostic for one declared invariant on one admitted transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantGuardDiagnostic {
    invariant: Invariant,
    predicate: Option<PredicateKey>,
    status: InvariantGuardStatus,
    predicates: Option<PredicateRegistry>,
    implication: Option<KleenePropertyReport>,
}

impl InvariantGuardDiagnostic {
    /// Invariant whose precheck relationship was inspected.
    #[must_use]
    pub const fn invariant(&self) -> &Invariant {
        &self.invariant
    }

    /// Stable precheck predicate when one was explicitly declared.
    #[must_use]
    pub const fn predicate(&self) -> Option<&PredicateKey> {
        self.predicate.as_ref()
    }

    /// Diagnostic result. This status never validates the invariant itself.
    #[must_use]
    pub const fn status(&self) -> InvariantGuardStatus {
        self.status
    }

    /// Registry needed to interpret a bound implication counterexample.
    #[must_use]
    pub const fn predicates(&self) -> Option<&PredicateRegistry> {
        self.predicates.as_ref()
    }

    /// Exact implication report when a binding was present.
    #[must_use]
    pub const fn implication(&self) -> Option<KleenePropertyReport> {
        self.implication
    }
}

/// Exact policy analysis for one admitted transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionPolicyAnalysis {
    transition: AdmissibleTransition,
    predicates: PredicateRegistry,
    effective_guard: BoolExpr,
    satisfiability: KleeneSatisfiabilityReport,
    redundant_scopes: Vec<GuardScope>,
    invariant_diagnostics: Vec<InvariantGuardDiagnostic>,
}

impl TransitionPolicyAnalysis {
    /// Admitted transition being analyzed.
    #[must_use]
    pub const fn transition(&self) -> &AdmissibleTransition {
        &self.transition
    }

    /// Stable registry of the effective transition guard.
    #[must_use]
    pub const fn predicates(&self) -> &PredicateRegistry {
        &self.predicates
    }

    /// Conjunction of every resource/dimension/transition guard that applies.
    #[must_use]
    pub const fn effective_guard(&self) -> &BoolExpr {
        &self.effective_guard
    }

    /// Exact three-valued satisfiability result.
    #[must_use]
    pub const fn satisfiability(&self) -> KleeneSatisfiabilityReport {
        self.satisfiability
    }

    /// Whether some assignment makes this transition explicitly eligible.
    #[must_use]
    pub const fn is_reachable(&self) -> bool {
        self.satisfiability.is_satisfiable()
    }

    /// Guard scopes whose removal preserves the exact strong-Kleene result.
    #[must_use]
    pub fn redundant_scopes(&self) -> &[GuardScope] {
        &self.redundant_scopes
    }

    /// Eligibility-vs-invariant-precheck diagnostics for applicable invariants.
    #[must_use]
    pub fn invariant_diagnostics(&self) -> &[InvariantGuardDiagnostic] {
        &self.invariant_diagnostics
    }
}

/// Exact mutual-exclusion result for one canonical transition pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionPairAnalysis {
    left: AdmissibleTransition,
    right: AdmissibleTransition,
    predicates: PredicateRegistry,
    mutual_exclusion: KleenePropertyReport,
}

impl TransitionPairAnalysis {
    /// First transition in canonical resource order.
    #[must_use]
    pub const fn left(&self) -> &AdmissibleTransition {
        &self.left
    }

    /// Second transition in canonical resource order.
    #[must_use]
    pub const fn right(&self) -> &AdmissibleTransition {
        &self.right
    }

    /// Stable registry needed to interpret any exact counterexample.
    #[must_use]
    pub const fn predicates(&self) -> &PredicateRegistry {
        &self.predicates
    }

    /// Exact strong-Kleene exclusion report.
    #[must_use]
    pub const fn mutual_exclusion(&self) -> KleenePropertyReport {
        self.mutual_exclusion
    }

    /// Whether no assignment can make both transitions explicitly eligible.
    #[must_use]
    pub const fn are_mutually_exclusive(&self) -> bool {
        self.mutual_exclusion.holds()
    }
}

/// Complete bounded diagnostic report for one guarded resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcePolicyAnalysis {
    transitions: Vec<TransitionPolicyAnalysis>,
    pairs: Vec<TransitionPairAnalysis>,
}

impl ResourcePolicyAnalysis {
    /// Per-transition reports in canonical admitted-transition order.
    #[must_use]
    pub fn transitions(&self) -> &[TransitionPolicyAnalysis] {
        &self.transitions
    }

    /// Pairwise exclusion reports in deterministic `(i, j)` order.
    #[must_use]
    pub fn pairs(&self) -> &[TransitionPairAnalysis] {
        &self.pairs
    }

    /// Iterate only transitions proven unreachable under the declared guards.
    pub fn unreachable_transitions(&self) -> impl Iterator<Item = &TransitionPolicyAnalysis> {
        self.transitions
            .iter()
            .filter(|report| !report.is_reachable())
    }

    /// Iterate only transition pairs proven mutually exclusive.
    pub fn mutually_exclusive_pairs(&self) -> impl Iterator<Item = &TransitionPairAnalysis> {
        self.pairs
            .iter()
            .filter(|report| report.are_mutually_exclusive())
    }
}

/// Analyze one resource's guard policy exactly over bounded strong-Kleene domains.
///
/// Invariant bindings are diagnostics only. Even `ImpliedForEligibility` means
/// only that the Boolean eligibility condition establishes the declared precheck
/// predicate; trusted validation must still check the invariant before actuation.
///
/// # Errors
///
/// Fails closed on structural bounds, ambiguous invariant bindings, malformed
/// predicate mappings, or any exact-oracle resource/evaluation failure.
pub fn analyze_resource_policy(
    spec: &GuardedResourceSpec,
    invariant_bindings: &[InvariantPredicateBinding],
    limits: ExactOracleLimits,
) -> Result<ResourcePolicyAnalysis, ResourcePolicyAnalysisError> {
    let transitions = spec.resource().admissible_transitions();
    if transitions.len() > MAX_RESOURCE_POLICY_TRANSITIONS {
        return Err(ResourcePolicyAnalysisError::TooManyTransitions {
            actual: transitions.len(),
            maximum: MAX_RESOURCE_POLICY_TRANSITIONS,
        });
    }
    if invariant_bindings.len() > MAX_RESOURCE_POLICY_INVARIANT_BINDINGS {
        return Err(ResourcePolicyAnalysisError::TooManyInvariantBindings {
            actual: invariant_bindings.len(),
            maximum: MAX_RESOURCE_POLICY_INVARIANT_BINDINGS,
        });
    }

    let bindings = canonical_bindings(invariant_bindings)?;
    let oracle = ExactKleeneOracle::new(limits);
    let mut transition_reports = Vec::with_capacity(transitions.len());

    for transition in transitions {
        let applicable = spec
            .guards()
            .iter()
            .filter(|guard| {
                guard
                    .scope()
                    .applies_to(transition.mechanism(), transition.dimension())
            })
            .collect::<Vec<_>>();
        let merged = merge_guards(&applicable)?;
        let satisfiability = oracle.satisfiability(&merged.expression)?;

        let mut redundant_scopes = Vec::new();
        for (index, guard) in applicable.iter().enumerate() {
            if oracle
                .conjunction_clause_redundancy(&merged.clauses, index)?
                .holds()
            {
                redundant_scopes.push(guard.scope().clone());
            }
        }

        let invariant_diagnostics = spec
            .resource()
            .invariants()
            .iter()
            .filter(|invariant| invariant_applies(invariant, transition))
            .map(|invariant| {
                analyze_invariant(
                    invariant,
                    bindings.get(invariant),
                    &merged,
                    satisfiability.is_unsatisfiable(),
                    oracle,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        transition_reports.push(TransitionPolicyAnalysis {
            transition: transition.clone(),
            predicates: merged.predicates,
            effective_guard: merged.expression,
            satisfiability,
            redundant_scopes,
            invariant_diagnostics,
        });
    }

    let pair_count = transitions
        .len()
        .saturating_mul(transitions.len().saturating_sub(1))
        / 2;
    debug_assert!(pair_count <= MAX_RESOURCE_POLICY_PAIRS);
    let mut pairs = Vec::with_capacity(pair_count);
    for left in 0..transition_reports.len() {
        for right in (left + 1)..transition_reports.len() {
            let merged = merge_expressions(
                &transition_reports[left].predicates,
                &transition_reports[left].effective_guard,
                &transition_reports[right].predicates,
                &transition_reports[right].effective_guard,
            )?;
            let mutual_exclusion = oracle.mutual_exclusion(&merged.left, &merged.right)?;
            pairs.push(TransitionPairAnalysis {
                left: transition_reports[left].transition.clone(),
                right: transition_reports[right].transition.clone(),
                predicates: merged.predicates,
                mutual_exclusion,
            });
        }
    }

    Ok(ResourcePolicyAnalysis {
        transitions: transition_reports,
        pairs,
    })
}

fn canonical_bindings(
    bindings: &[InvariantPredicateBinding],
) -> Result<BTreeMap<Invariant, PredicateKey>, ResourcePolicyAnalysisError> {
    let mut result = BTreeMap::new();
    for binding in bindings {
        if result
            .insert(binding.invariant().clone(), binding.predicate().clone())
            .is_some()
        {
            return Err(ResourcePolicyAnalysisError::DuplicateInvariantBinding {
                invariant: binding.invariant().clone(),
            });
        }
    }
    Ok(result)
}

fn invariant_applies(invariant: &Invariant, transition: &AdmissibleTransition) -> bool {
    invariant
        .scope()
        .is_none_or(|dimension| dimension == transition.dimension())
}

struct MergedGuard {
    predicates: PredicateRegistry,
    expression: BoolExpr,
    clauses: Vec<BoolExpr>,
}

fn merge_guards(
    guards: &[&crate::BooleanGuard],
) -> Result<MergedGuard, ResourcePolicyAnalysisError> {
    let predicates = PredicateRegistry::from_keys(
        guards
            .iter()
            .flat_map(|guard| guard.predicates().iter().map(|(_, key)| key.clone())),
    )?;
    let clauses = guards
        .iter()
        .map(|guard| remap_expression(guard.expression(), guard.predicates(), &predicates))
        .collect::<Result<Vec<_>, _>>()?;
    let expression = BoolExpr::all(clauses.iter().cloned());
    Ok(MergedGuard {
        predicates,
        expression,
        clauses,
    })
}

struct MergedExpressions {
    predicates: PredicateRegistry,
    left: BoolExpr,
    right: BoolExpr,
}

fn merge_expressions(
    left_registry: &PredicateRegistry,
    left: &BoolExpr,
    right_registry: &PredicateRegistry,
    right: &BoolExpr,
) -> Result<MergedExpressions, ResourcePolicyAnalysisError> {
    let predicates = PredicateRegistry::from_keys(
        left_registry
            .iter()
            .map(|(_, key)| key.clone())
            .chain(right_registry.iter().map(|(_, key)| key.clone())),
    )?;
    Ok(MergedExpressions {
        left: remap_expression(left, left_registry, &predicates)?,
        right: remap_expression(right, right_registry, &predicates)?,
        predicates,
    })
}

fn analyze_invariant(
    invariant: &Invariant,
    predicate: Option<&PredicateKey>,
    merged: &MergedGuard,
    transition_unreachable: bool,
    oracle: ExactKleeneOracle,
) -> Result<InvariantGuardDiagnostic, ResourcePolicyAnalysisError> {
    let Some(predicate) = predicate else {
        return Ok(InvariantGuardDiagnostic {
            invariant: invariant.clone(),
            predicate: None,
            status: InvariantGuardStatus::Unbound,
            predicates: None,
            implication: None,
        });
    };

    let predicates = PredicateRegistry::from_keys(
        merged
            .predicates
            .iter()
            .map(|(_, key)| key.clone())
            .chain(std::iter::once(predicate.clone())),
    )?;
    let eligibility = remap_expression(&merged.expression, &merged.predicates, &predicates)?;
    let predicate_id =
        predicates
            .id(predicate)
            .ok_or(ResourcePolicyAnalysisError::MissingPredicateKey {
                id: PredicateId::new(u32::MAX),
            })?;
    let implication = oracle.implication(&eligibility, &BoolExpr::atom(predicate_id))?;
    let status = if transition_unreachable {
        InvariantGuardStatus::VacuousUnreachable
    } else if implication.holds() {
        InvariantGuardStatus::ImpliedForEligibility
    } else {
        InvariantGuardStatus::NotImpliedForEligibility
    };

    Ok(InvariantGuardDiagnostic {
        invariant: invariant.clone(),
        predicate: Some(predicate.clone()),
        status,
        predicates: Some(predicates),
        implication: Some(implication),
    })
}

fn remap_expression(
    expression: &BoolExpr,
    source: &PredicateRegistry,
    target: &PredicateRegistry,
) -> Result<BoolExpr, ResourcePolicyAnalysisError> {
    Ok(match expression {
        BoolExpr::Const(value) => BoolExpr::Const(*value),
        BoolExpr::Atom(id) => {
            let key = source
                .key(*id)
                .ok_or(ResourcePolicyAnalysisError::MissingPredicateKey { id: *id })?;
            let mapped = target.id(key).ok_or_else(|| {
                ResourcePolicyAnalysisError::MissingMergedPredicateKey { key: key.clone() }
            })?;
            BoolExpr::Atom(mapped)
        }
        BoolExpr::Not(inner) => BoolExpr::Not(Box::new(remap_expression(inner, source, target)?)),
        BoolExpr::All(expressions) => BoolExpr::All(
            expressions
                .iter()
                .map(|expr| remap_expression(expr, source, target))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        BoolExpr::Any(expressions) => BoolExpr::Any(
            expressions
                .iter()
                .map(|expr| remap_expression(expr, source, target))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        BoolExpr::Xor(left, right) => BoolExpr::Xor(
            Box::new(remap_expression(left, source, target)?),
            Box::new(remap_expression(right, source, target)?),
        ),
        BoolExpr::Implies(left, right) => BoolExpr::Implies(
            Box::new(remap_expression(left, source, target)?),
            Box::new(remap_expression(right, source, target)?),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::{
        AdmissibleTransition, DimensionId, InvariantKind, LogicalResourceId, ResourceClassId,
        ResourceSpec,
    };
    use crate::{BooleanGuard, TransitionMechanism};

    fn key(name: &str) -> PredicateKey {
        PredicateKey::new("elastic.analysis", name).unwrap()
    }

    fn registry(names: &[&str]) -> PredicateRegistry {
        PredicateRegistry::from_keys(names.iter().map(|name| key(name))).unwrap()
    }

    fn atom(registry: &PredicateRegistry, name: &str) -> BoolExpr {
        BoolExpr::atom(registry.id(&key(name)).unwrap())
    }

    fn two_transition_resource(invariants: Vec<Invariant>) -> crate::ResourceSpec {
        let mut builder = ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("policy-analysis").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .allow(DimensionId::CONCURRENCY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::CAPACITY,
        ))
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Recompute,
            DimensionId::CONCURRENCY,
        ));
        for invariant in invariants {
            builder = builder.preserve(invariant);
        }
        builder.build().unwrap()
    }

    #[test]
    fn unreachable_transition_is_detected_without_treating_unknown_as_false() {
        let registry = registry(&["a"]);
        let resource_guard =
            BooleanGuard::new(GuardScope::Resource, registry.clone(), atom(&registry, "a"))
                .unwrap();
        let transition_guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reencode,
                dimension: DimensionId::CAPACITY,
            },
            registry.clone(),
            BoolExpr::negate(atom(&registry, "a")),
        )
        .unwrap();
        let spec = GuardedResourceSpec::new(
            two_transition_resource(Vec::new()),
            vec![resource_guard, transition_guard],
        )
        .unwrap();
        let analysis = analyze_resource_policy(&spec, &[], ExactOracleLimits::default()).unwrap();
        let capacity = analysis
            .transitions()
            .iter()
            .find(|report| report.transition().dimension() == &DimensionId::CAPACITY)
            .unwrap();
        assert!(!capacity.is_reachable());
        assert_eq!(analysis.unreachable_transitions().count(), 1);
    }

    #[test]
    fn pairwise_analysis_remaps_stable_keys_across_different_compact_ids() {
        let first_registry = registry(&["a", "shared"]);
        let second_registry = registry(&["shared"]);
        assert_ne!(
            first_registry.id(&key("shared")),
            second_registry.id(&key("shared"))
        );
        let capacity = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reencode,
                dimension: DimensionId::CAPACITY,
            },
            first_registry.clone(),
            atom(&first_registry, "shared"),
        )
        .unwrap();
        let concurrency = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Recompute,
                dimension: DimensionId::CONCURRENCY,
            },
            second_registry.clone(),
            BoolExpr::negate(atom(&second_registry, "shared")),
        )
        .unwrap();
        let spec = GuardedResourceSpec::new(
            two_transition_resource(Vec::new()),
            vec![capacity, concurrency],
        )
        .unwrap();
        let analysis = analyze_resource_policy(&spec, &[], ExactOracleLimits::default()).unwrap();
        assert_eq!(analysis.pairs().len(), 1);
        assert!(analysis.pairs()[0].are_mutually_exclusive());
    }

    #[test]
    fn exact_redundancy_detects_shadowing_but_preserves_unknown_sensitive_difference() {
        let a_registry = registry(&["a"]);
        let ab_registry = registry(&["a", "b"]);
        let resource_guard = BooleanGuard::new(
            GuardScope::Resource,
            a_registry.clone(),
            atom(&a_registry, "a"),
        )
        .unwrap();
        let transition_guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reencode,
                dimension: DimensionId::CAPACITY,
            },
            ab_registry.clone(),
            BoolExpr::all([atom(&ab_registry, "a"), atom(&ab_registry, "b")]),
        )
        .unwrap();
        let spec = GuardedResourceSpec::new(
            two_transition_resource(Vec::new()),
            vec![resource_guard, transition_guard],
        )
        .unwrap();
        let analysis = analyze_resource_policy(&spec, &[], ExactOracleLimits::default()).unwrap();
        let capacity = analysis
            .transitions()
            .iter()
            .find(|report| report.transition().dimension() == &DimensionId::CAPACITY)
            .unwrap();
        assert!(capacity.redundant_scopes().contains(&GuardScope::Resource));

        let excluded = BooleanGuard::new(
            GuardScope::Resource,
            a_registry.clone(),
            BoolExpr::any([
                atom(&a_registry, "a"),
                BoolExpr::negate(atom(&a_registry, "a")),
            ]),
        )
        .unwrap();
        let b_registry = registry(&["b"]);
        let b_guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reencode,
                dimension: DimensionId::CAPACITY,
            },
            b_registry.clone(),
            atom(&b_registry, "b"),
        )
        .unwrap();
        let spec =
            GuardedResourceSpec::new(two_transition_resource(Vec::new()), vec![excluded, b_guard])
                .unwrap();
        let analysis = analyze_resource_policy(&spec, &[], ExactOracleLimits::default()).unwrap();
        let capacity = analysis
            .transitions()
            .iter()
            .find(|report| report.transition().dimension() == &DimensionId::CAPACITY)
            .unwrap();
        assert!(!capacity.redundant_scopes().contains(&GuardScope::Resource));
    }

    #[test]
    fn invariant_precheck_diagnostic_never_substitutes_for_runtime_validation() {
        let invariant =
            Invariant::new(InvariantKind::PreserveContents).along(DimensionId::CAPACITY);
        let guard_registry = registry(&["capacity-ok"]);
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reencode,
                dimension: DimensionId::CAPACITY,
            },
            guard_registry.clone(),
            atom(&guard_registry, "capacity-ok"),
        )
        .unwrap();
        let spec = GuardedResourceSpec::new(
            two_transition_resource(vec![invariant.clone()]),
            vec![guard],
        )
        .unwrap();
        let binding = InvariantPredicateBinding::new(invariant, key("contents-ok"));
        let analysis =
            analyze_resource_policy(&spec, &[binding], ExactOracleLimits::default()).unwrap();
        let diagnostic = &analysis.transitions()[0].invariant_diagnostics()[0];
        assert_eq!(
            diagnostic.status(),
            InvariantGuardStatus::NotImpliedForEligibility
        );
        assert!(diagnostic.implication().unwrap().counterexample().is_some());
    }

    #[test]
    fn invariant_precheck_implication_is_reported_only_as_eligibility_evidence() {
        let invariant =
            Invariant::new(InvariantKind::PreserveContents).along(DimensionId::CAPACITY);
        let guard_registry = registry(&["capacity-ok", "contents-ok"]);
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reencode,
                dimension: DimensionId::CAPACITY,
            },
            guard_registry.clone(),
            BoolExpr::all([
                atom(&guard_registry, "capacity-ok"),
                atom(&guard_registry, "contents-ok"),
            ]),
        )
        .unwrap();
        let spec = GuardedResourceSpec::new(
            two_transition_resource(vec![invariant.clone()]),
            vec![guard],
        )
        .unwrap();
        let binding = InvariantPredicateBinding::new(invariant, key("contents-ok"));
        let analysis =
            analyze_resource_policy(&spec, &[binding], ExactOracleLimits::default()).unwrap();
        assert_eq!(
            analysis.transitions()[0].invariant_diagnostics()[0].status(),
            InvariantGuardStatus::ImpliedForEligibility
        );
    }

    #[test]
    fn duplicate_invariant_bindings_fail_closed() {
        let invariant = Invariant::new(InvariantKind::PreserveIdentity);
        let spec =
            GuardedResourceSpec::new(two_transition_resource(vec![invariant.clone()]), Vec::new())
                .unwrap();
        let bindings = [
            InvariantPredicateBinding::new(invariant.clone(), key("first")),
            InvariantPredicateBinding::new(invariant.clone(), key("second")),
        ];
        assert_eq!(
            analyze_resource_policy(&spec, &bindings, ExactOracleLimits::default()),
            Err(ResourcePolicyAnalysisError::DuplicateInvariantBinding { invariant })
        );
    }
}
