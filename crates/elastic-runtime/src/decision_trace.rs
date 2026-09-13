//! Deterministic evidence for Boolean-guarded planning decisions.
//!
//! A [`DecisionTrace`] binds the exact guarded-EIR identity to a structural
//! fingerprint of the fact snapshot, records every fact relevant to the guard
//! policy (including missing facts as `Unknown`), and captures eligible,
//! rejected, unknown, and selected transitions. Trace capture re-evaluates only
//! the pure Boolean guards over the supplied snapshot; it never replays or
//! performs actuation.
//!
//! Structural fingerprints in this module are diagnostic/replay identities
//! inside one trust domain. They are intentionally non-cryptographic and must
//! not be treated as authentication tokens.

use crate::{
    FactFreshnessError, FactSnapshot, FactSourceId, MAX_EVIDENCE_BYTES,
    MAX_EVIDENCE_COLLECTION_ITEMS,
};
use elastic_core::resource::{DimensionId, LogicalResourceId};
use elastic_core::{
    FreshnessSnapshot, GuardScope, LogicError, ObservationEpoch, PredicateKey, ResourceGeneration,
    TransitionMechanism, TruthValue,
};
use elastic_eir::{
    prune_transition_candidates, EirGuardedResource, Fingerprint, TransitionCandidate,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Schema identifier for the first typed Boolean decision-trace contract.
pub const DECISION_TRACE_SCHEMA_V1: &str = "elastic-boolean-decision-trace-v1";

/// Decision traces share the runtime evidence envelope's maximum byte size.
pub const MAX_DECISION_TRACE_BYTES: usize = MAX_EVIDENCE_BYTES;

/// Non-cryptographic structural identity of one semantic fact snapshot.
///
/// Monotonic [`std::time::Instant`] values are deliberately excluded: freshness
/// is validated separately before capture/replay, while the fingerprint remains
/// deterministic for the same source, epoch, resource generation, and ordered
/// facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FactSnapshotFingerprint(u64);

impl FactSnapshotFingerprint {
    /// Raw structural fingerprint bits for diagnostics and persisted evidence.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl fmt::Display for FactSnapshotFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "facts:{:016x}", self.0)
    }
}

/// One predicate value recorded for explanation/replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PredicateTraceEntry {
    key: PredicateKey,
    truth: TruthValue,
    materialized: bool,
    referenced_by_guard: bool,
}

impl PredicateTraceEntry {
    /// Stable predicate identity.
    #[must_use]
    pub const fn key(&self) -> &PredicateKey {
        &self.key
    }

    /// Three-valued fact used by guard evaluation.
    #[must_use]
    pub const fn truth(&self) -> TruthValue {
        self.truth
    }

    /// Whether the fact snapshot explicitly contained this key.
    ///
    /// `false` plus [`TruthValue::Unknown`] distinguishes missing evidence from
    /// an explicitly materialized unknown fact.
    #[must_use]
    pub const fn materialized(&self) -> bool {
        self.materialized
    }

    /// Whether at least one guard in the EIR references this predicate.
    #[must_use]
    pub const fn referenced_by_guard(&self) -> bool {
        self.referenced_by_guard
    }
}

/// Stable trace representation of one transition candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateDecisionTrace {
    mechanism: TransitionMechanism,
    dimension: DimensionId,
    capability_grounded: bool,
    magnitude: Option<u64>,
}

impl CandidateDecisionTrace {
    fn from_candidate(candidate: &TransitionCandidate) -> Self {
        Self {
            mechanism: candidate.mechanism(),
            dimension: candidate.dimension().clone(),
            capability_grounded: candidate.capability_grounded(),
            magnitude: candidate.magnitude(),
        }
    }

    /// Transition mechanism.
    #[must_use]
    pub const fn mechanism(&self) -> TransitionMechanism {
        self.mechanism
    }

    /// Elastic dimension.
    #[must_use]
    pub const fn dimension(&self) -> &DimensionId {
        &self.dimension
    }

    /// Whether the EIR admission is capability-grounded.
    #[must_use]
    pub const fn capability_grounded(&self) -> bool {
        self.capability_grounded
    }

    /// Numeric planner magnitude, when one was attached.
    #[must_use]
    pub const fn magnitude(&self) -> Option<u64> {
        self.magnitude
    }
}

/// Candidate eliminated by an explicit false guard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedCandidateTrace {
    candidate: CandidateDecisionTrace,
    failed_scope: GuardScope,
}

impl RejectedCandidateTrace {
    /// Rejected candidate.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateDecisionTrace {
        &self.candidate
    }

    /// First deterministic guard scope that evaluated to `False`.
    #[must_use]
    pub const fn failed_scope(&self) -> &GuardScope {
        &self.failed_scope
    }
}

/// Candidate blocked by unknown guard evidence or absent capability grounding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownCandidateTrace {
    candidate: CandidateDecisionTrace,
    unknown_scopes: Vec<GuardScope>,
    capability_grounded: bool,
}

impl UnknownCandidateTrace {
    /// Candidate whose eligibility could not be established.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateDecisionTrace {
        &self.candidate
    }

    /// Applicable guard scopes that evaluated to `Unknown`.
    #[must_use]
    pub fn unknown_scopes(&self) -> &[GuardScope] {
        &self.unknown_scopes
    }

    /// Whether capability grounding itself was present.
    #[must_use]
    pub const fn capability_grounded(&self) -> bool {
        self.capability_grounded
    }
}

/// Why a guarded planning cycle ended without a selected candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionStopReason {
    /// The resource declares no transitions.
    NoDeclaredTransitions,
    /// Every declared candidate was explicitly rejected by Boolean guards.
    AllCandidatesRejected,
    /// No candidate was eligible and at least one remained unknown.
    InsufficientEvidence,
    /// Boolean-eligible candidates existed but numeric planning selected none.
    NumericPlannerNoCandidate,
}

/// Full deterministic trace of one guarded planning decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionTrace {
    resource: LogicalResourceId,
    guarded_resource_fingerprint: Fingerprint,
    fact_snapshot_fingerprint: FactSnapshotFingerprint,
    fact_source: FactSourceId,
    observation_epoch: ObservationEpoch,
    resource_generation: ResourceGeneration,
    predicates: Vec<PredicateTraceEntry>,
    eligible: Vec<CandidateDecisionTrace>,
    rejected: Vec<RejectedCandidateTrace>,
    unknown: Vec<UnknownCandidateTrace>,
    selected: Option<CandidateDecisionTrace>,
    stop_reason: Option<DecisionStopReason>,
}

impl DecisionTrace {
    /// Logical resource whose decision was traced.
    #[must_use]
    pub const fn resource(&self) -> &LogicalResourceId {
        &self.resource
    }

    /// Structural identity of base EIR plus Boolean guard policy.
    #[must_use]
    pub const fn guarded_resource_fingerprint(&self) -> Fingerprint {
        self.guarded_resource_fingerprint
    }

    /// Structural identity of fact source/epoch/generation/content.
    #[must_use]
    pub const fn fact_snapshot_fingerprint(&self) -> FactSnapshotFingerprint {
        self.fact_snapshot_fingerprint
    }

    /// Runtime component that produced the fact snapshot.
    #[must_use]
    pub const fn fact_source(&self) -> &FactSourceId {
        &self.fact_source
    }

    /// Observation epoch bound to the decision.
    #[must_use]
    pub const fn observation_epoch(&self) -> ObservationEpoch {
        self.observation_epoch
    }

    /// Logical-resource generation bound to the decision.
    #[must_use]
    pub const fn resource_generation(&self) -> ResourceGeneration {
        self.resource_generation
    }

    /// Predicate values in stable [`PredicateKey`] order.
    #[must_use]
    pub fn predicates(&self) -> &[PredicateTraceEntry] {
        &self.predicates
    }

    /// Predicates whose decision value was unknown.
    pub fn unknown_predicates(&self) -> impl Iterator<Item = &PredicateTraceEntry> {
        self.predicates
            .iter()
            .filter(|entry| entry.truth == TruthValue::Unknown)
    }

    /// Boolean-eligible candidates before numeric ranking.
    #[must_use]
    pub fn eligible(&self) -> &[CandidateDecisionTrace] {
        &self.eligible
    }

    /// Candidates explicitly rejected by guards.
    #[must_use]
    pub fn rejected(&self) -> &[RejectedCandidateTrace] {
        &self.rejected
    }

    /// Candidates blocked by insufficient evidence/capability grounding.
    #[must_use]
    pub fn unknown(&self) -> &[UnknownCandidateTrace] {
        &self.unknown
    }

    /// Candidate selected by numeric planning, if any.
    #[must_use]
    pub const fn selected(&self) -> Option<&CandidateDecisionTrace> {
        self.selected.as_ref()
    }

    /// Why the cycle stopped without a selected candidate.
    #[must_use]
    pub const fn stop_reason(&self) -> Option<DecisionStopReason> {
        self.stop_reason
    }

    /// Validate that a replay uses the same resource policy and semantic fact
    /// snapshot, after first revalidating fact freshness against trusted state.
    ///
    /// This checks identity only; it never authorizes actuation.
    pub fn validate_replay_identity(
        &self,
        resource: &EirGuardedResource,
        facts: &FactSnapshot,
        freshness: &FreshnessSnapshot,
    ) -> Result<(), DecisionReplayError> {
        facts
            .validate_freshness(freshness)
            .map_err(DecisionReplayError::StaleFacts)?;

        let Some(binding) = facts.resource_binding() else {
            return Err(DecisionReplayError::MissingResourceBinding);
        };
        if binding.resource() != resource.resource().identity() {
            return Err(DecisionReplayError::ResourceBindingMismatch {
                snapshot: binding.resource().clone(),
                requested: resource.resource().identity().clone(),
            });
        }
        if self.resource != *resource.resource().identity() {
            return Err(DecisionReplayError::ResourceIdentityMismatch {
                trace: self.resource.clone(),
                current: resource.resource().identity().clone(),
            });
        }
        if self.guarded_resource_fingerprint != resource.fingerprint() {
            return Err(DecisionReplayError::GuardFingerprintMismatch {
                trace: self.guarded_resource_fingerprint,
                current: resource.fingerprint(),
            });
        }
        let current_facts = fact_snapshot_fingerprint(facts);
        if self.fact_snapshot_fingerprint != current_facts {
            return Err(DecisionReplayError::FactFingerprintMismatch {
                trace: self.fact_snapshot_fingerprint,
                current: current_facts,
            });
        }
        Ok(())
    }

    /// Encode this trace into bounded JSON suitable for embedding in the
    /// existing runtime evidence system.
    ///
    /// # Errors
    ///
    /// Returns [`DecisionTraceError::EvidenceTooLarge`] if the encoded trace
    /// exceeds [`MAX_DECISION_TRACE_BYTES`], or `Encoding` on JSON failure.
    pub fn to_bounded_json(&self) -> Result<String, DecisionTraceError> {
        self.to_bounded_json_with_limit(MAX_DECISION_TRACE_BYTES)
    }

    fn to_bounded_json_with_limit(&self, limit: usize) -> Result<String, DecisionTraceError> {
        let value = self.to_json_value();
        let encoded = serde_json::to_string(&value)
            .map_err(|error| DecisionTraceError::Encoding(error.to_string()))?;
        if encoded.len() > limit {
            return Err(DecisionTraceError::EvidenceTooLarge {
                max_bytes: limit,
                actual_bytes: encoded.len(),
            });
        }
        Ok(encoded)
    }

    fn to_json_value(&self) -> Value {
        json!({
            "schema": DECISION_TRACE_SCHEMA_V1,
            "resource_id": self.resource.as_str(),
            "guarded_resource_fingerprint": format!("{:016x}", self.guarded_resource_fingerprint.bits()),
            "fact_snapshot_fingerprint": format!("{:016x}", self.fact_snapshot_fingerprint.bits()),
            "fact_source": self.fact_source.as_str(),
            "observation_epoch": self.observation_epoch.get(),
            "resource_generation": self.resource_generation.get(),
            "predicates": self.predicates.iter().map(predicate_json).collect::<Vec<_>>(),
            "eligible": self.eligible.iter().map(candidate_json).collect::<Vec<_>>(),
            "rejected": self.rejected.iter().map(|entry| json!({
                "candidate": candidate_json(&entry.candidate),
                "failed_scope": scope_text(&entry.failed_scope),
            })).collect::<Vec<_>>(),
            "unknown": self.unknown.iter().map(|entry| json!({
                "candidate": candidate_json(&entry.candidate),
                "unknown_scopes": entry.unknown_scopes.iter().map(scope_text).collect::<Vec<_>>(),
                "capability_grounded": entry.capability_grounded,
            })).collect::<Vec<_>>(),
            "selected": self.selected.as_ref().map(candidate_json),
            "stop_reason": self.stop_reason.map(stop_reason_text),
        })
    }
}

/// Capture deterministic Boolean decision evidence for one fresh fact snapshot.
///
/// Guard evaluation is re-run as a pure operation on `facts`; no adapter,
/// validator, or actuator is invoked. If `selected` is supplied, it must match
/// an eligible mechanism/dimension pair from the same re-evaluation.
///
/// # Errors
///
/// Fails closed for stale/cross-resource facts, Boolean evaluation failures,
/// selected candidates outside the eligible subset, or evidence bounds.
pub fn capture_decision_trace(
    resource: &EirGuardedResource,
    facts: &FactSnapshot,
    freshness: &FreshnessSnapshot,
    selected: Option<&TransitionCandidate>,
) -> Result<DecisionTrace, DecisionTraceError> {
    facts.validate_freshness(freshness)?;
    let Some(binding) = facts.resource_binding() else {
        return Err(DecisionTraceError::MissingResourceBinding);
    };
    if binding.resource() != resource.resource().identity() {
        return Err(DecisionTraceError::ResourceBindingMismatch {
            snapshot: binding.resource().clone(),
            requested: resource.resource().identity().clone(),
        });
    }

    let report = prune_transition_candidates(resource, facts)?;
    if report.total_classified() != resource.resource().transitions().len() {
        return Err(DecisionTraceError::IncompletePruningReport {
            classified: report.total_classified(),
            declared: resource.resource().transitions().len(),
        });
    }

    if let Some(candidate) = selected {
        if !report
            .eligible()
            .iter()
            .any(|eligible| same_transition(eligible, candidate))
        {
            return Err(DecisionTraceError::SelectedCandidateNotEligible {
                mechanism: candidate.mechanism(),
                dimension: candidate.dimension().clone(),
            });
        }
    }

    let materialized = facts
        .iter()
        .map(|(key, truth)| (key.clone(), truth))
        .collect::<BTreeMap<_, _>>();
    let mut referenced = BTreeSet::new();
    for guard in resource.guards() {
        for predicate in guard.predicates() {
            referenced.insert(predicate.key().clone());
        }
    }
    let mut keys = BTreeSet::new();
    keys.extend(materialized.keys().cloned());
    keys.extend(referenced.iter().cloned());

    if keys.len() > MAX_EVIDENCE_COLLECTION_ITEMS {
        return Err(DecisionTraceError::TooManyTraceEntries {
            max: MAX_EVIDENCE_COLLECTION_ITEMS,
            actual: keys.len(),
        });
    }
    if report.total_classified() > MAX_EVIDENCE_COLLECTION_ITEMS {
        return Err(DecisionTraceError::TooManyCandidates {
            max: MAX_EVIDENCE_COLLECTION_ITEMS,
            actual: report.total_classified(),
        });
    }

    let predicates = keys
        .into_iter()
        .map(|key| PredicateTraceEntry {
            truth: materialized
                .get(&key)
                .copied()
                .unwrap_or(TruthValue::Unknown),
            materialized: materialized.contains_key(&key),
            referenced_by_guard: referenced.contains(&key),
            key,
        })
        .collect();
    let eligible = report
        .eligible()
        .iter()
        .map(CandidateDecisionTrace::from_candidate)
        .collect();
    let rejected = report
        .rejected()
        .iter()
        .map(|entry| RejectedCandidateTrace {
            candidate: CandidateDecisionTrace::from_candidate(entry.candidate()),
            failed_scope: entry.failed_scope().clone(),
        })
        .collect();
    let unknown = report
        .unknown()
        .iter()
        .map(|entry| UnknownCandidateTrace {
            candidate: CandidateDecisionTrace::from_candidate(entry.candidate()),
            unknown_scopes: entry.unknown_scopes().to_vec(),
            capability_grounded: entry.capability_grounded(),
        })
        .collect::<Vec<_>>();
    let selected = selected.map(CandidateDecisionTrace::from_candidate);
    let stop_reason = if selected.is_some() {
        None
    } else if resource.resource().transitions().is_empty() {
        Some(DecisionStopReason::NoDeclaredTransitions)
    } else if !report.eligible().is_empty() {
        Some(DecisionStopReason::NumericPlannerNoCandidate)
    } else if !report.unknown().is_empty() {
        Some(DecisionStopReason::InsufficientEvidence)
    } else {
        Some(DecisionStopReason::AllCandidatesRejected)
    };

    Ok(DecisionTrace {
        resource: resource.resource().identity().clone(),
        guarded_resource_fingerprint: resource.fingerprint(),
        fact_snapshot_fingerprint: fact_snapshot_fingerprint(facts),
        fact_source: facts.source().clone(),
        observation_epoch: facts.observation_epoch(),
        resource_generation: binding.generation(),
        predicates,
        eligible,
        rejected,
        unknown,
        selected,
        stop_reason,
    })
}

/// Compute the deterministic semantic identity used by decision traces.
#[must_use]
pub fn fact_snapshot_fingerprint(facts: &FactSnapshot) -> FactSnapshotFingerprint {
    let mut fingerprint = Fingerprint::EMPTY
        .text("runtime-fact-snapshot")
        .number(1)
        .text(facts.source().as_str())
        .number(facts.observation_epoch().get());
    match facts.resource_binding() {
        Some(binding) => {
            fingerprint = fingerprint
                .text("resource-bound")
                .text(binding.resource().as_str())
                .number(binding.generation().get());
        }
        None => {
            fingerprint = fingerprint.text("resource-unbound");
        }
    }
    fingerprint = fingerprint.number(facts.len() as u64);
    for (key, truth) in facts.iter() {
        fingerprint = fingerprint
            .text(key.namespace())
            .text(key.name())
            .number(truth_code(truth));
    }
    FactSnapshotFingerprint(fingerprint.bits())
}

/// Trace construction/encoding failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionTraceError {
    /// Fact snapshot freshness no longer matches trusted runtime state.
    StaleFacts(FactFreshnessError),
    /// Resource-specific traces require a resource-bound fact snapshot.
    MissingResourceBinding,
    /// Facts were derived for another logical resource.
    ResourceBindingMismatch {
        /// Resource carried by the fact snapshot.
        snapshot: LogicalResourceId,
        /// Resource being traced.
        requested: LogicalResourceId,
    },
    /// Pure Boolean guard evaluation failed.
    Logic(LogicError),
    /// Guard pruning did not classify exactly the declared transition set.
    IncompletePruningReport { classified: usize, declared: usize },
    /// Numeric planning selected a transition outside the Boolean-eligible set.
    SelectedCandidateNotEligible {
        mechanism: TransitionMechanism,
        dimension: DimensionId,
    },
    /// Predicate trace exceeded the evidence collection bound.
    TooManyTraceEntries { max: usize, actual: usize },
    /// Candidate trace exceeded the evidence collection bound.
    TooManyCandidates { max: usize, actual: usize },
    /// JSON encoding exceeded the runtime evidence byte bound.
    EvidenceTooLarge {
        max_bytes: usize,
        actual_bytes: usize,
    },
    /// JSON encoding failed unexpectedly.
    Encoding(String),
}

impl fmt::Display for DecisionTraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleFacts(error) => write!(f, "stale decision facts: {error}"),
            Self::MissingResourceBinding => {
                f.write_str("decision trace requires a resource-bound fact snapshot")
            }
            Self::ResourceBindingMismatch {
                snapshot,
                requested,
            } => write!(
                f,
                "decision facts are bound to resource {} but trace requests {}",
                snapshot.as_str(),
                requested.as_str()
            ),
            Self::Logic(error) => write!(f, "Boolean decision trace evaluation failed: {error}"),
            Self::IncompletePruningReport {
                classified,
                declared,
            } => write!(
                f,
                "Boolean pruning classified {classified} transitions but resource declares {declared}"
            ),
            Self::SelectedCandidateNotEligible {
                mechanism,
                dimension,
            } => write!(
                f,
                "selected candidate {}@{} is not Boolean-eligible",
                mechanism_text(*mechanism),
                dimension
            ),
            Self::TooManyTraceEntries { max, actual } => write!(
                f,
                "decision trace has {actual} predicate entries; maximum is {max}"
            ),
            Self::TooManyCandidates { max, actual } => write!(
                f,
                "decision trace has {actual} candidates; maximum is {max}"
            ),
            Self::EvidenceTooLarge {
                max_bytes,
                actual_bytes,
            } => write!(
                f,
                "decision trace has {actual_bytes} encoded bytes; maximum is {max_bytes}"
            ),
            Self::Encoding(detail) => write!(f, "decision trace JSON encoding failed: {detail}"),
        }
    }
}

impl std::error::Error for DecisionTraceError {}

impl From<FactFreshnessError> for DecisionTraceError {
    fn from(value: FactFreshnessError) -> Self {
        Self::StaleFacts(value)
    }
}

impl From<LogicError> for DecisionTraceError {
    fn from(value: LogicError) -> Self {
        Self::Logic(value)
    }
}

/// Replay identity failures. None of these outcomes authorize fallback or
/// actuation; callers must fail closed or create a new decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionReplayError {
    StaleFacts(FactFreshnessError),
    MissingResourceBinding,
    ResourceBindingMismatch {
        snapshot: LogicalResourceId,
        requested: LogicalResourceId,
    },
    ResourceIdentityMismatch {
        trace: LogicalResourceId,
        current: LogicalResourceId,
    },
    GuardFingerprintMismatch {
        trace: Fingerprint,
        current: Fingerprint,
    },
    FactFingerprintMismatch {
        trace: FactSnapshotFingerprint,
        current: FactSnapshotFingerprint,
    },
}

impl fmt::Display for DecisionReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleFacts(error) => write!(f, "replay facts are stale: {error}"),
            Self::MissingResourceBinding => {
                f.write_str("replay requires a resource-bound fact snapshot")
            }
            Self::ResourceBindingMismatch {
                snapshot,
                requested,
            } => write!(
                f,
                "replay facts are bound to resource {} but current resource is {}",
                snapshot.as_str(),
                requested.as_str()
            ),
            Self::ResourceIdentityMismatch { trace, current } => write!(
                f,
                "trace resource {} does not match current resource {}",
                trace.as_str(),
                current.as_str()
            ),
            Self::GuardFingerprintMismatch { trace, current } => {
                write!(
                    f,
                    "trace guard identity {trace} does not match current {current}"
                )
            }
            Self::FactFingerprintMismatch { trace, current } => {
                write!(
                    f,
                    "trace fact identity {trace} does not match current {current}"
                )
            }
        }
    }
}

impl std::error::Error for DecisionReplayError {}

fn predicate_json(entry: &PredicateTraceEntry) -> Value {
    json!({
        "namespace": entry.key.namespace(),
        "name": entry.key.name(),
        "truth": truth_text(entry.truth),
        "materialized": entry.materialized,
        "referenced_by_guard": entry.referenced_by_guard,
    })
}

fn candidate_json(candidate: &CandidateDecisionTrace) -> Value {
    json!({
        "mechanism": mechanism_text(candidate.mechanism),
        "dimension": candidate.dimension.as_str(),
        "capability_grounded": candidate.capability_grounded,
        "magnitude": candidate.magnitude,
    })
}

fn scope_text(scope: &GuardScope) -> String {
    match scope {
        GuardScope::Resource => "resource".to_owned(),
        GuardScope::Dimension(dimension) => format!("dimension:{}", dimension.as_str()),
        GuardScope::Transition {
            mechanism,
            dimension,
        } => format!(
            "transition:{}@{}",
            mechanism_text(*mechanism),
            dimension.as_str()
        ),
    }
}

const fn mechanism_text(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

const fn truth_text(truth: TruthValue) -> &'static str {
    match truth {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
}

const fn truth_code(truth: TruthValue) -> u64 {
    match truth {
        TruthValue::False => 0,
        TruthValue::True => 1,
        TruthValue::Unknown => 2,
    }
}

const fn stop_reason_text(reason: DecisionStopReason) -> &'static str {
    match reason {
        DecisionStopReason::NoDeclaredTransitions => "no-declared-transitions",
        DecisionStopReason::AllCandidatesRejected => "all-candidates-rejected",
        DecisionStopReason::InsufficientEvidence => "insufficient-evidence",
        DecisionStopReason::NumericPlannerNoCandidate => "numeric-planner-no-candidate",
    }
}

fn same_transition(left: &TransitionCandidate, right: &TransitionCandidate) -> bool {
    left.mechanism() == right.mechanism() && left.dimension() == right.dimension()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilityPredicate, FactResourceBinding, ObservationSnapshot, PredicateEvaluationInput,
        PredicateEvaluator,
    };
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, ResourceClassId, ResourceSpec,
    };
    use elastic_core::{
        BoolExpr, BooleanGuard, GuardedResourceSpec, PlannerEpoch, PredicateRegistry,
    };
    use elastic_eir::lower_guarded;
    use std::time::{Duration, Instant};

    fn guarded_fixture(
        expression_negated: bool,
    ) -> (EirGuardedResource, PredicateKey, LogicalResourceId) {
        let resource_id = LogicalResourceId::new("decision-trace").unwrap();
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
        let key = PredicateKey::new("elastic.trace", "capacity-ok").unwrap();
        let registry = PredicateRegistry::from_keys([key.clone()]).unwrap();
        let id = registry.id(&key).unwrap();
        let atom = BoolExpr::atom(id);
        let expression = if expression_negated {
            BoolExpr::negate(atom)
        } else {
            atom
        };
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CAPACITY,
            },
            registry,
            expression,
        )
        .unwrap();
        let guarded = lower_guarded(&GuardedResourceSpec::new(spec, vec![guard]).unwrap()).unwrap();
        (guarded, key, resource_id)
    }

    fn fact_snapshot(
        resource_id: &LogicalResourceId,
        key: &PredicateKey,
        value: Option<bool>,
        now: Instant,
        reverse_extra: bool,
    ) -> FactSnapshot {
        let observations = ObservationSnapshot::new(now, Vec::new());
        let context = elastic_eir::PlanningContext::new();
        let input = PredicateEvaluationInput::new(&context, &observations, now);
        let primary = CapabilityPredicate::new(key.clone(), value);
        let extra_key = PredicateKey::new("elastic.trace", "extra").unwrap();
        let extra = CapabilityPredicate::new(extra_key, Some(false));
        let evaluators: Vec<&dyn PredicateEvaluator> = if reverse_extra {
            vec![&extra, &primary]
        } else {
            vec![&primary, &extra]
        };
        FactSnapshot::derive(
            FactSourceId::new("runtime:decision-trace-test").unwrap(),
            ObservationEpoch::new(14),
            Some(FactResourceBinding::new(
                resource_id.clone(),
                ResourceGeneration::new(7),
            )),
            &input,
            &evaluators,
        )
        .unwrap()
    }

    fn freshness(resource_id: &LogicalResourceId) -> FreshnessSnapshot {
        FreshnessSnapshot::new(PlannerEpoch::new(3), ObservationEpoch::new(14))
            .with_resource_generation(resource_id.clone(), ResourceGeneration::new(7))
    }

    #[test]
    fn deterministic_trace_and_fact_identity_ignore_evaluator_order_and_instants() {
        let (resource, key, resource_id) = guarded_fixture(false);
        let first = fact_snapshot(&resource_id, &key, Some(true), Instant::now(), false);
        let second = fact_snapshot(
            &resource_id,
            &key,
            Some(true),
            Instant::now() + Duration::from_secs(5),
            true,
        );
        let current = freshness(&resource_id);
        let first_trace = capture_decision_trace(&resource, &first, &current, None).unwrap();
        let second_trace = capture_decision_trace(&resource, &second, &current, None).unwrap();

        assert_eq!(
            fact_snapshot_fingerprint(&first),
            fact_snapshot_fingerprint(&second)
        );
        assert_eq!(first_trace, second_trace);
    }

    #[test]
    fn missing_guard_fact_is_explicit_unknown_evidence() {
        let (resource, key, resource_id) = guarded_fixture(false);
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, Vec::new());
        let context = elastic_eir::PlanningContext::new();
        let input = PredicateEvaluationInput::new(&context, &observations, now);
        let facts = FactSnapshot::derive(
            FactSourceId::new("runtime:decision-trace-test").unwrap(),
            ObservationEpoch::new(14),
            Some(FactResourceBinding::new(
                resource_id.clone(),
                ResourceGeneration::new(7),
            )),
            &input,
            &[],
        )
        .unwrap();
        let trace =
            capture_decision_trace(&resource, &facts, &freshness(&resource_id), None).unwrap();
        let entry = trace
            .predicates()
            .iter()
            .find(|entry| entry.key() == &key)
            .unwrap();

        assert_eq!(entry.truth(), TruthValue::Unknown);
        assert!(!entry.materialized());
        assert!(entry.referenced_by_guard());
        assert_eq!(trace.unknown_predicates().count(), 1);
        assert_eq!(
            trace.stop_reason(),
            Some(DecisionStopReason::InsufficientEvidence)
        );
    }

    #[test]
    fn rejected_candidate_cannot_be_recorded_as_selected() {
        let (resource, key, resource_id) = guarded_fixture(false);
        let facts = fact_snapshot(&resource_id, &key, Some(false), Instant::now(), false);
        let selected = TransitionCandidate::from_admitted(&resource.resource().transitions()[0]);
        let error =
            capture_decision_trace(&resource, &facts, &freshness(&resource_id), Some(&selected))
                .unwrap_err();
        assert!(matches!(
            error,
            DecisionTraceError::SelectedCandidateNotEligible { .. }
        ));
    }

    #[test]
    fn replay_rejects_changed_guard_policy() {
        let (resource, key, resource_id) = guarded_fixture(false);
        let facts = fact_snapshot(&resource_id, &key, Some(true), Instant::now(), false);
        let current = freshness(&resource_id);
        let trace = capture_decision_trace(&resource, &facts, &current, None).unwrap();
        let (changed, _, _) = guarded_fixture(true);

        assert!(matches!(
            trace.validate_replay_identity(&changed, &facts, &current),
            Err(DecisionReplayError::GuardFingerprintMismatch { .. })
        ));
    }

    #[test]
    fn bounded_json_contains_schema_and_enforces_limit() {
        let (resource, key, resource_id) = guarded_fixture(false);
        let facts = fact_snapshot(&resource_id, &key, Some(true), Instant::now(), false);
        let trace =
            capture_decision_trace(&resource, &facts, &freshness(&resource_id), None).unwrap();
        let encoded = trace.to_bounded_json().unwrap();
        assert!(encoded.contains(DECISION_TRACE_SCHEMA_V1));
        assert!(matches!(
            trace.to_bounded_json_with_limit(8),
            Err(DecisionTraceError::EvidenceTooLarge { .. })
        ));
    }
}
