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
    MAX_EVIDENCE_COLLECTION_ITEMS, MAX_EVIDENCE_DEPTH, MAX_EVIDENCE_NODES,
    MAX_EVIDENCE_RESOURCE_ID_BYTES, MAX_EVIDENCE_STRING_BYTES,
};
use elastic_core::resource::{DimensionId, LogicalResourceId};
use elastic_core::{
    FreshnessSnapshot, GuardScope, LogicError, ObservationEpoch, PredicateKey, ResourceGeneration,
    TransitionMechanism, TruthValue,
};
use elastic_eir::{
    prune_transition_candidates, EirGuardedResource, Fingerprint, TransitionCandidate,
};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Schema identifier for the first typed Boolean decision-trace contract.
pub const DECISION_TRACE_SCHEMA_V1: &str = "elastic-boolean-decision-trace-v1";

/// Decision traces share the runtime evidence envelope's maximum byte size.
pub const MAX_DECISION_TRACE_BYTES: usize = MAX_EVIDENCE_BYTES;

/// Longest numeric token emitted by the v1 schema (`u64::MAX` in base 10).
const MAX_DECISION_TRACE_NUMBER_BYTES: usize = 20;

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
        if self.resource.as_str().len() > MAX_EVIDENCE_RESOURCE_ID_BYTES {
            return Err(DecisionTraceError::PersistedBounds(format!(
                "resource_id has {} bytes; maximum is {MAX_EVIDENCE_RESOURCE_ID_BYTES}",
                self.resource.as_str().len()
            )));
        }
        let value = self.to_json_value();
        let encoded = serde_json::to_string(&value)
            .map_err(|error| DecisionTraceError::Encoding(error.to_string()))?;
        if encoded.len() > limit {
            return Err(DecisionTraceError::EvidenceTooLarge {
                max_bytes: limit,
                actual_bytes: encoded.len(),
            });
        }
        preflight_json_bounds(encoded.as_bytes())?;
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

    /// Decode one persisted v1 decision trace from bounded, strict JSON.
    ///
    /// Decoding is a data-only operation. It performs no observation, planning,
    /// validation, adapter call, or actuation. Imported traces remain historical
    /// evidence and must pass [`Self::validate_replay_identity`] against fresh
    /// trusted state before their decision identity can be reused.
    ///
    /// The decoder rejects oversized/deep inputs before JSON materialization,
    /// unknown or duplicate fields, unsupported schema versions, invalid enum
    /// values, malformed identifiers, inconsistent classification sets, and a
    /// fact fingerprint that does not match the materialized predicate entries.
    pub fn from_bounded_json(bytes: &[u8]) -> Result<Self, DecisionTraceError> {
        preflight_json_bounds(bytes)?;
        let wire: DecisionTraceWireV1 = serde_json::from_slice(bytes)
            .map_err(|error| DecisionTraceError::Decoding(error.to_string()))?;
        decode_wire_trace(wire)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionTraceWireV1 {
    schema: String,
    resource_id: String,
    guarded_resource_fingerprint: String,
    fact_snapshot_fingerprint: String,
    fact_source: String,
    observation_epoch: u64,
    resource_generation: u64,
    predicates: Vec<PredicateTraceWireV1>,
    eligible: Vec<CandidateTraceWireV1>,
    rejected: Vec<RejectedCandidateWireV1>,
    unknown: Vec<UnknownCandidateWireV1>,
    #[serde(deserialize_with = "deserialize_required_option")]
    selected: Option<CandidateTraceWireV1>,
    #[serde(deserialize_with = "deserialize_required_option")]
    stop_reason: Option<DecisionStopReasonWireV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PredicateTraceWireV1 {
    namespace: String,
    name: String,
    truth: TruthValueWireV1,
    materialized: bool,
    referenced_by_guard: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateTraceWireV1 {
    mechanism: TransitionMechanismWireV1,
    dimension: String,
    capability_grounded: bool,
    #[serde(deserialize_with = "deserialize_required_option")]
    magnitude: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RejectedCandidateWireV1 {
    candidate: CandidateTraceWireV1,
    failed_scope: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnknownCandidateWireV1 {
    candidate: CandidateTraceWireV1,
    unknown_scopes: Vec<String>,
    capability_grounded: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TruthValueWireV1 {
    True,
    False,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TransitionMechanismWireV1 {
    Reinterpret,
    Reencode,
    Recompute,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DecisionStopReasonWireV1 {
    NoDeclaredTransitions,
    AllCandidatesRejected,
    InsufficientEvidence,
    NumericPlannerNoCandidate,
}

fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Copy)]
struct JsonContainerBound {
    opener: u8,
    commas: usize,
    has_content: bool,
}

fn preflight_json_bounds(bytes: &[u8]) -> Result<(), DecisionTraceError> {
    if bytes.len() > MAX_DECISION_TRACE_BYTES {
        return Err(DecisionTraceError::PersistedInputTooLarge {
            max_bytes: MAX_DECISION_TRACE_BYTES,
            actual_bytes: bytes.len(),
        });
    }

    let empty = JsonContainerBound {
        opener: 0,
        commas: 0,
        has_content: false,
    };
    let mut stack = [empty; MAX_EVIDENCE_DEPTH];
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut string_bytes = 0usize;
    let mut scalar_active = false;
    let mut scalar_numeric = false;
    let mut scalar_bytes = 0usize;
    let mut nodes = 0usize;

    for &byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
                string_bytes = string_bytes.saturating_add(1);
            } else if byte == b'\\' {
                escaped = true;
                string_bytes = string_bytes.saturating_add(1);
            } else if byte == b'"' {
                in_string = false;
            } else {
                string_bytes = string_bytes.saturating_add(1);
            }
            if string_bytes > MAX_EVIDENCE_STRING_BYTES {
                return Err(DecisionTraceError::PersistedBounds(format!(
                    "JSON string exceeds maximum {MAX_EVIDENCE_STRING_BYTES} bytes"
                )));
            }
            continue;
        }

        match byte {
            b'"' => {
                mark_container_content(&mut stack, depth);
                nodes = bounded_node_increment(nodes)?;
                in_string = true;
                escaped = false;
                string_bytes = 0;
                scalar_active = false;
                scalar_numeric = false;
                scalar_bytes = 0;
            }
            b'[' | b'{' => {
                mark_container_content(&mut stack, depth);
                nodes = bounded_node_increment(nodes)?;
                if depth >= MAX_EVIDENCE_DEPTH {
                    return Err(DecisionTraceError::PersistedBounds(format!(
                        "JSON nesting exceeds maximum depth {MAX_EVIDENCE_DEPTH}"
                    )));
                }
                stack[depth] = JsonContainerBound {
                    opener: byte,
                    commas: 0,
                    has_content: false,
                };
                depth += 1;
                scalar_active = false;
                scalar_numeric = false;
                scalar_bytes = 0;
            }
            b']' | b'}' => {
                scalar_active = false;
                scalar_numeric = false;
                scalar_bytes = 0;
                if depth == 0 {
                    return Err(DecisionTraceError::Decoding(
                        "unbalanced JSON container".to_owned(),
                    ));
                }
                let frame = stack[depth - 1];
                let expected = if byte == b']' { b'[' } else { b'{' };
                if frame.opener != expected {
                    return Err(DecisionTraceError::Decoding(
                        "mismatched JSON container".to_owned(),
                    ));
                }
                let items = if frame.has_content {
                    frame.commas.saturating_add(1)
                } else {
                    0
                };
                if items > MAX_EVIDENCE_COLLECTION_ITEMS {
                    return Err(DecisionTraceError::PersistedBounds(format!(
                        "JSON collection contains {items} items; maximum is {MAX_EVIDENCE_COLLECTION_ITEMS}"
                    )));
                }
                depth -= 1;
            }
            b',' => {
                scalar_active = false;
                scalar_numeric = false;
                scalar_bytes = 0;
                if depth > 0 {
                    let frame = &mut stack[depth - 1];
                    frame.commas = frame.commas.saturating_add(1);
                    if frame.commas >= MAX_EVIDENCE_COLLECTION_ITEMS {
                        return Err(DecisionTraceError::PersistedBounds(format!(
                            "JSON collection exceeds maximum {MAX_EVIDENCE_COLLECTION_ITEMS} items"
                        )));
                    }
                }
            }
            b':' | b' ' | b'\t' | b'\r' | b'\n' => {
                scalar_active = false;
                scalar_numeric = false;
                scalar_bytes = 0;
            }
            _ => {
                mark_container_content(&mut stack, depth);
                if !scalar_active {
                    nodes = bounded_node_increment(nodes)?;
                    scalar_active = true;
                    scalar_numeric = byte == b'-' || byte.is_ascii_digit();
                    scalar_bytes = 1;
                } else if scalar_numeric {
                    scalar_bytes = scalar_bytes.saturating_add(1);
                }
                if scalar_numeric && scalar_bytes > MAX_DECISION_TRACE_NUMBER_BYTES {
                    return Err(DecisionTraceError::PersistedBounds(format!(
                        "JSON numeric token exceeds maximum {MAX_DECISION_TRACE_NUMBER_BYTES} bytes"
                    )));
                }
            }
        }
    }

    if in_string || depth != 0 {
        return Err(DecisionTraceError::Decoding(
            "truncated or unterminated JSON input".to_owned(),
        ));
    }
    Ok(())
}

fn mark_container_content(stack: &mut [JsonContainerBound; MAX_EVIDENCE_DEPTH], depth: usize) {
    if depth > 0 {
        stack[depth - 1].has_content = true;
    }
}

fn bounded_node_increment(nodes: usize) -> Result<usize, DecisionTraceError> {
    let next = nodes.saturating_add(1);
    if next > MAX_EVIDENCE_NODES {
        return Err(DecisionTraceError::PersistedBounds(format!(
            "JSON node count exceeds maximum {MAX_EVIDENCE_NODES}"
        )));
    }
    Ok(next)
}

fn decode_wire_trace(wire: DecisionTraceWireV1) -> Result<DecisionTrace, DecisionTraceError> {
    if wire.schema != DECISION_TRACE_SCHEMA_V1 {
        return Err(DecisionTraceError::UnsupportedSchema(wire.schema));
    }
    if wire.resource_id.len() > MAX_EVIDENCE_RESOURCE_ID_BYTES {
        return Err(DecisionTraceError::PersistedBounds(format!(
            "resource_id has {} bytes; maximum is {MAX_EVIDENCE_RESOURCE_ID_BYTES}",
            wire.resource_id.len()
        )));
    }
    if wire.predicates.len() > MAX_EVIDENCE_COLLECTION_ITEMS {
        return Err(DecisionTraceError::TooManyTraceEntries {
            max: MAX_EVIDENCE_COLLECTION_ITEMS,
            actual: wire.predicates.len(),
        });
    }

    let resource = LogicalResourceId::new(wire.resource_id)
        .map_err(|error| invalid_persisted(format!("invalid resource_id: {error}")))?;
    let fact_source = FactSourceId::new(wire.fact_source)
        .map_err(|error| invalid_persisted(format!("invalid fact_source: {error}")))?;
    let guarded_resource_fingerprint = Fingerprint::from_bits(parse_hex_fingerprint(
        "guarded_resource_fingerprint",
        &wire.guarded_resource_fingerprint,
    )?);
    let fact_snapshot_fingerprint = FactSnapshotFingerprint(parse_hex_fingerprint(
        "fact_snapshot_fingerprint",
        &wire.fact_snapshot_fingerprint,
    )?);

    let mut predicates = Vec::with_capacity(wire.predicates.len());
    for entry in wire.predicates {
        let key = PredicateKey::new(entry.namespace, entry.name)
            .map_err(|error| invalid_persisted(format!("invalid predicate key: {error}")))?;
        let truth = match entry.truth {
            TruthValueWireV1::True => TruthValue::True,
            TruthValueWireV1::False => TruthValue::False,
            TruthValueWireV1::Unknown => TruthValue::Unknown,
        };
        if !entry.materialized && truth != TruthValue::Unknown {
            return Err(invalid_persisted(format!(
                "non-materialized predicate {key} must be unknown"
            )));
        }
        if !entry.materialized && !entry.referenced_by_guard {
            return Err(invalid_persisted(format!(
                "predicate {key} is neither materialized nor referenced by a guard"
            )));
        }
        if predicates
            .last()
            .is_some_and(|previous: &PredicateTraceEntry| previous.key >= key)
        {
            return Err(invalid_persisted(
                "predicate entries must be strictly ordered by stable key".to_owned(),
            ));
        }
        predicates.push(PredicateTraceEntry {
            key,
            truth,
            materialized: entry.materialized,
            referenced_by_guard: entry.referenced_by_guard,
        });
    }

    let computed_fact_fingerprint = persisted_fact_fingerprint(
        &resource,
        &fact_source,
        wire.observation_epoch,
        wire.resource_generation,
        &predicates,
    );
    if computed_fact_fingerprint != fact_snapshot_fingerprint {
        return Err(invalid_persisted(format!(
            "fact_snapshot_fingerprint {} does not match decoded materialized facts {}",
            fact_snapshot_fingerprint, computed_fact_fingerprint
        )));
    }

    let eligible = wire
        .eligible
        .into_iter()
        .map(decode_candidate)
        .collect::<Result<Vec<_>, _>>()?;
    let rejected = wire
        .rejected
        .into_iter()
        .map(|entry| {
            Ok(RejectedCandidateTrace {
                candidate: decode_candidate(entry.candidate)?,
                failed_scope: parse_scope(&entry.failed_scope)?,
            })
        })
        .collect::<Result<Vec<_>, DecisionTraceError>>()?;
    let unknown = wire
        .unknown
        .into_iter()
        .map(|entry| {
            let candidate = decode_candidate(entry.candidate)?;
            if entry.capability_grounded != candidate.capability_grounded {
                return Err(invalid_persisted(format!(
                    "unknown candidate {}@{} has inconsistent capability grounding",
                    mechanism_text(candidate.mechanism),
                    candidate.dimension
                )));
            }
            let scopes = entry
                .unknown_scopes
                .iter()
                .map(|scope| parse_scope(scope))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(UnknownCandidateTrace {
                candidate,
                unknown_scopes: scopes,
                capability_grounded: entry.capability_grounded,
            })
        })
        .collect::<Result<Vec<_>, DecisionTraceError>>()?;
    let selected = wire.selected.map(decode_candidate).transpose()?;
    let stop_reason = wire.stop_reason.map(|reason| match reason {
        DecisionStopReasonWireV1::NoDeclaredTransitions => {
            DecisionStopReason::NoDeclaredTransitions
        }
        DecisionStopReasonWireV1::AllCandidatesRejected => {
            DecisionStopReason::AllCandidatesRejected
        }
        DecisionStopReasonWireV1::InsufficientEvidence => DecisionStopReason::InsufficientEvidence,
        DecisionStopReasonWireV1::NumericPlannerNoCandidate => {
            DecisionStopReason::NumericPlannerNoCandidate
        }
    });

    validate_persisted_candidate_sets(
        &eligible,
        &rejected,
        &unknown,
        selected.as_ref(),
        stop_reason,
    )?;

    Ok(DecisionTrace {
        resource,
        guarded_resource_fingerprint,
        fact_snapshot_fingerprint,
        fact_source,
        observation_epoch: ObservationEpoch::new(wire.observation_epoch),
        resource_generation: ResourceGeneration::new(wire.resource_generation),
        predicates,
        eligible,
        rejected,
        unknown,
        selected,
        stop_reason,
    })
}

fn decode_candidate(
    wire: CandidateTraceWireV1,
) -> Result<CandidateDecisionTrace, DecisionTraceError> {
    Ok(CandidateDecisionTrace {
        mechanism: match wire.mechanism {
            TransitionMechanismWireV1::Reinterpret => TransitionMechanism::Reinterpret,
            TransitionMechanismWireV1::Reencode => TransitionMechanism::Reencode,
            TransitionMechanismWireV1::Recompute => TransitionMechanism::Recompute,
        },
        dimension: parse_dimension(&wire.dimension)?,
        capability_grounded: wire.capability_grounded,
        magnitude: wire.magnitude,
    })
}

fn parse_dimension(text: &str) -> Result<DimensionId, DecisionTraceError> {
    let dimension = match text {
        "capacity" => DimensionId::CAPACITY,
        "concurrency" => DimensionId::CONCURRENCY,
        "residency" => DimensionId::RESIDENCY,
        "locality" => DimensionId::LOCALITY,
        "representation" => DimensionId::REPRESENTATION,
        "precision" => DimensionId::PRECISION,
        "parallelism" => DimensionId::PARALLELISM,
        "routing" => DimensionId::ROUTING,
        "redundancy" => DimensionId::REDUNDANCY,
        "persistence" => DimensionId::PERSISTENCE,
        "recomputability" => DimensionId::RECOMPUTABILITY,
        "bandwidth" => DimensionId::BANDWIDTH,
        "energy" => DimensionId::ENERGY,
        custom => DimensionId::custom(custom.to_owned())
            .map_err(|error| invalid_persisted(format!("invalid dimension: {error}")))?,
    };
    Ok(dimension)
}

fn parse_scope(text: &str) -> Result<GuardScope, DecisionTraceError> {
    if text == "resource" {
        return Ok(GuardScope::Resource);
    }
    if let Some(dimension) = text.strip_prefix("dimension:") {
        return Ok(GuardScope::Dimension(parse_dimension(dimension)?));
    }
    if let Some(transition) = text.strip_prefix("transition:") {
        let (mechanism, dimension) = transition
            .split_once('@')
            .ok_or_else(|| invalid_persisted(format!("invalid transition guard scope {text:?}")))?;
        return Ok(GuardScope::Transition {
            mechanism: parse_mechanism_text(mechanism)?,
            dimension: parse_dimension(dimension)?,
        });
    }
    Err(invalid_persisted(format!("invalid guard scope {text:?}")))
}

fn parse_mechanism_text(text: &str) -> Result<TransitionMechanism, DecisionTraceError> {
    match text {
        "reinterpret" => Ok(TransitionMechanism::Reinterpret),
        "reencode" => Ok(TransitionMechanism::Reencode),
        "recompute" => Ok(TransitionMechanism::Recompute),
        _ => Err(invalid_persisted(format!(
            "invalid transition mechanism {text:?}"
        ))),
    }
}

fn parse_hex_fingerprint(field: &str, text: &str) -> Result<u64, DecisionTraceError> {
    if text.len() != 16 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_persisted(format!(
            "{field} must contain exactly 16 hexadecimal digits"
        )));
    }
    u64::from_str_radix(text, 16)
        .map_err(|error| invalid_persisted(format!("invalid {field}: {error}")))
}

fn persisted_fact_fingerprint(
    resource: &LogicalResourceId,
    fact_source: &FactSourceId,
    observation_epoch: u64,
    resource_generation: u64,
    predicates: &[PredicateTraceEntry],
) -> FactSnapshotFingerprint {
    let materialized_count = predicates.iter().filter(|entry| entry.materialized).count();
    let mut fingerprint = Fingerprint::EMPTY
        .text("runtime-fact-snapshot")
        .number(1)
        .text(fact_source.as_str())
        .number(observation_epoch)
        .text("resource-bound")
        .text(resource.as_str())
        .number(resource_generation)
        .number(materialized_count as u64);
    for entry in predicates.iter().filter(|entry| entry.materialized) {
        fingerprint = fingerprint
            .text(entry.key.namespace())
            .text(entry.key.name())
            .number(truth_code(entry.truth));
    }
    FactSnapshotFingerprint(fingerprint.bits())
}

fn validate_persisted_candidate_sets(
    eligible: &[CandidateDecisionTrace],
    rejected: &[RejectedCandidateTrace],
    unknown: &[UnknownCandidateTrace],
    selected: Option<&CandidateDecisionTrace>,
    stop_reason: Option<DecisionStopReason>,
) -> Result<(), DecisionTraceError> {
    let total = eligible
        .len()
        .checked_add(rejected.len())
        .and_then(|count| count.checked_add(unknown.len()))
        .ok_or_else(|| invalid_persisted("candidate count overflow".to_owned()))?;
    if total > MAX_EVIDENCE_COLLECTION_ITEMS {
        return Err(DecisionTraceError::TooManyCandidates {
            max: MAX_EVIDENCE_COLLECTION_ITEMS,
            actual: total,
        });
    }

    let mut identities = BTreeSet::new();
    for candidate in eligible {
        if !candidate.capability_grounded {
            return Err(invalid_persisted(format!(
                "eligible candidate {}@{} is not capability-grounded",
                mechanism_text(candidate.mechanism),
                candidate.dimension
            )));
        }
        insert_candidate_identity(&mut identities, candidate)?;
    }
    for entry in rejected {
        insert_candidate_identity(&mut identities, &entry.candidate)?;
    }
    for entry in unknown {
        insert_candidate_identity(&mut identities, &entry.candidate)?;
    }

    if let Some(candidate) = selected {
        if !candidate.capability_grounded {
            return Err(invalid_persisted(
                "selected candidate is not capability-grounded".to_owned(),
            ));
        }
        if !eligible.iter().any(|entry| {
            entry.mechanism == candidate.mechanism && entry.dimension == candidate.dimension
        }) {
            return Err(invalid_persisted(
                "selected candidate is not present in the eligible set".to_owned(),
            ));
        }
        if stop_reason.is_some() {
            return Err(invalid_persisted(
                "selected candidate and stop_reason cannot both be present".to_owned(),
            ));
        }
        return Ok(());
    }

    let expected = if total == 0 {
        DecisionStopReason::NoDeclaredTransitions
    } else if !eligible.is_empty() {
        DecisionStopReason::NumericPlannerNoCandidate
    } else if !unknown.is_empty() {
        DecisionStopReason::InsufficientEvidence
    } else {
        DecisionStopReason::AllCandidatesRejected
    };
    if stop_reason != Some(expected) {
        return Err(invalid_persisted(format!(
            "stop_reason {:?} is inconsistent with decoded candidate classification; expected {:?}",
            stop_reason, expected
        )));
    }
    Ok(())
}

fn insert_candidate_identity(
    identities: &mut BTreeSet<(TransitionMechanism, DimensionId)>,
    candidate: &CandidateDecisionTrace,
) -> Result<(), DecisionTraceError> {
    if !identities.insert((candidate.mechanism, candidate.dimension.clone())) {
        return Err(invalid_persisted(format!(
            "candidate {}@{} appears in more than one decision classification",
            mechanism_text(candidate.mechanism),
            candidate.dimension
        )));
    }
    Ok(())
}

fn invalid_persisted(detail: String) -> DecisionTraceError {
    DecisionTraceError::InvalidPersistedTrace(detail)
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
    /// Persisted JSON exceeded the byte bound before parsing.
    PersistedInputTooLarge {
        max_bytes: usize,
        actual_bytes: usize,
    },
    /// Persisted JSON violated a pre-allocation structural bound.
    PersistedBounds(String),
    /// Persisted JSON was syntactically invalid, duplicated a field, or used an invalid enum.
    Decoding(String),
    /// Persisted trace declared a schema that this decoder does not implement.
    UnsupportedSchema(String),
    /// Persisted trace was syntactically valid but semantically inconsistent.
    InvalidPersistedTrace(String),
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
            Self::PersistedInputTooLarge {
                max_bytes,
                actual_bytes,
            } => write!(
                f,
                "persisted decision trace has {actual_bytes} bytes; maximum is {max_bytes}"
            ),
            Self::PersistedBounds(detail) => {
                write!(f, "persisted decision trace exceeds bounds: {detail}")
            }
            Self::Decoding(detail) => {
                write!(f, "persisted decision trace JSON is invalid: {detail}")
            }
            Self::UnsupportedSchema(schema) => {
                write!(f, "unsupported decision trace schema {schema:?}")
            }
            Self::InvalidPersistedTrace(detail) => {
                write!(f, "persisted decision trace is inconsistent: {detail}")
            }
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

    fn persisted_fixture() -> (DecisionTrace, String) {
        let (resource, key, resource_id) = guarded_fixture(false);
        let facts = fact_snapshot(&resource_id, &key, Some(true), Instant::now(), false);
        let trace =
            capture_decision_trace(&resource, &facts, &freshness(&resource_id), None).unwrap();
        let encoded = trace.to_bounded_json().unwrap();
        (trace, encoded)
    }

    #[test]
    fn persisted_trace_roundtrip_preserves_semantic_identity() {
        let (trace, encoded) = persisted_fixture();
        let decoded = DecisionTrace::from_bounded_json(encoded.as_bytes()).unwrap();

        assert_eq!(decoded, trace);
        assert_eq!(decoded.to_bounded_json().unwrap(), encoded);
    }

    #[test]
    fn persisted_trace_rejects_unknown_duplicate_and_missing_fields() {
        let (_, encoded) = persisted_fixture();

        let with_unknown = format!("{{\"unexpected\":0,{}", &encoded[1..]);
        assert!(matches!(
            DecisionTrace::from_bounded_json(with_unknown.as_bytes()),
            Err(DecisionTraceError::Decoding(_))
        ));

        let with_duplicate = format!(
            "{{\"schema\":\"{DECISION_TRACE_SCHEMA_V1}\",{}",
            &encoded[1..]
        );
        assert!(matches!(
            DecisionTrace::from_bounded_json(with_duplicate.as_bytes()),
            Err(DecisionTraceError::Decoding(_))
        ));

        let mut missing: Value = serde_json::from_str(&encoded).unwrap();
        missing.as_object_mut().unwrap().remove("selected");
        let missing = serde_json::to_vec(&missing).unwrap();
        assert!(matches!(
            DecisionTrace::from_bounded_json(&missing),
            Err(DecisionTraceError::Decoding(_))
        ));
    }

    #[test]
    fn persisted_trace_rejects_future_schema_and_invalid_enum() {
        let (_, encoded) = persisted_fixture();
        let mut future: Value = serde_json::from_str(&encoded).unwrap();
        future["schema"] = Value::String("elastic-boolean-decision-trace-v2".to_owned());
        let future = serde_json::to_vec(&future).unwrap();
        assert!(matches!(
            DecisionTrace::from_bounded_json(&future),
            Err(DecisionTraceError::UnsupportedSchema(schema))
                if schema == "elastic-boolean-decision-trace-v2"
        ));

        let mut invalid_enum: Value = serde_json::from_str(&encoded).unwrap();
        invalid_enum["predicates"][0]["truth"] = Value::String("maybe".to_owned());
        let invalid_enum = serde_json::to_vec(&invalid_enum).unwrap();
        assert!(matches!(
            DecisionTrace::from_bounded_json(&invalid_enum),
            Err(DecisionTraceError::Decoding(_))
        ));
    }

    #[test]
    fn persisted_trace_rejects_tampered_fact_fingerprint_and_stop_reason() {
        let (_, encoded) = persisted_fixture();
        let mut tampered: Value = serde_json::from_str(&encoded).unwrap();
        tampered["fact_snapshot_fingerprint"] = Value::String("0000000000000000".to_owned());
        let tampered = serde_json::to_vec(&tampered).unwrap();
        assert!(matches!(
            DecisionTrace::from_bounded_json(&tampered),
            Err(DecisionTraceError::InvalidPersistedTrace(_))
        ));

        let mut inconsistent: Value = serde_json::from_str(&encoded).unwrap();
        inconsistent["stop_reason"] = Value::String("all-candidates-rejected".to_owned());
        let inconsistent = serde_json::to_vec(&inconsistent).unwrap();
        assert!(matches!(
            DecisionTrace::from_bounded_json(&inconsistent),
            Err(DecisionTraceError::InvalidPersistedTrace(_))
        ));
    }

    #[test]
    fn persisted_trace_preflight_rejects_untrusted_size_depth_collection_and_string_bounds() {
        let oversized = vec![b' '; MAX_DECISION_TRACE_BYTES + 1];
        assert!(matches!(
            DecisionTrace::from_bounded_json(&oversized),
            Err(DecisionTraceError::PersistedInputTooLarge { .. })
        ));

        let too_deep = format!(
            "{}0{}",
            "[".repeat(MAX_EVIDENCE_DEPTH + 1),
            "]".repeat(MAX_EVIDENCE_DEPTH + 1)
        );
        assert!(matches!(
            preflight_json_bounds(too_deep.as_bytes()),
            Err(DecisionTraceError::PersistedBounds(_))
        ));

        let too_many = format!(
            "[{}]",
            std::iter::repeat_n("null", MAX_EVIDENCE_COLLECTION_ITEMS + 1)
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(matches!(
            preflight_json_bounds(too_many.as_bytes()),
            Err(DecisionTraceError::PersistedBounds(_))
        ));

        let long_string = format!("\"{}\"", "a".repeat(MAX_EVIDENCE_STRING_BYTES + 1));
        assert!(matches!(
            preflight_json_bounds(long_string.as_bytes()),
            Err(DecisionTraceError::PersistedBounds(_))
        ));

        let long_number = "1".repeat(MAX_DECISION_TRACE_NUMBER_BYTES + 1);
        assert!(matches!(
            preflight_json_bounds(long_number.as_bytes()),
            Err(DecisionTraceError::PersistedBounds(_))
        ));
    }

    #[test]
    fn persisted_trace_roundtrips_custom_dimensions_in_transition_scopes() {
        let resource = LogicalResourceId::new("custom-dimension-trace").unwrap();
        let source = FactSourceId::new("runtime:custom-dimension-trace").unwrap();
        let predicates = Vec::new();
        let fact_snapshot_fingerprint =
            persisted_fact_fingerprint(&resource, &source, 9, 4, &predicates);
        let eligible_dimension = DimensionId::custom("kv@tier:hot").unwrap();
        let rejected_dimension = DimensionId::custom("storage:tier@cold").unwrap();
        let eligible = CandidateDecisionTrace {
            mechanism: TransitionMechanism::Reencode,
            dimension: eligible_dimension.clone(),
            capability_grounded: true,
            magnitude: Some(3),
        };
        let rejected = RejectedCandidateTrace {
            candidate: CandidateDecisionTrace {
                mechanism: TransitionMechanism::Recompute,
                dimension: rejected_dimension,
                capability_grounded: true,
                magnitude: None,
            },
            failed_scope: GuardScope::Transition {
                mechanism: TransitionMechanism::Reencode,
                dimension: eligible_dimension,
            },
        };
        let trace = DecisionTrace {
            resource,
            guarded_resource_fingerprint: Fingerprint::EMPTY.text("custom-trace"),
            fact_snapshot_fingerprint,
            fact_source: source,
            observation_epoch: ObservationEpoch::new(9),
            resource_generation: ResourceGeneration::new(4),
            predicates,
            eligible: vec![eligible],
            rejected: vec![rejected],
            unknown: Vec::new(),
            selected: None,
            stop_reason: Some(DecisionStopReason::NumericPlannerNoCandidate),
        };

        let encoded = trace.to_bounded_json().unwrap();
        let decoded = DecisionTrace::from_bounded_json(encoded.as_bytes()).unwrap();
        assert_eq!(decoded, trace);
    }
}
