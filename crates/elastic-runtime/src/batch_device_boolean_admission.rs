//! BE14g fail-closed Boolean screening for declared batch/device-placement candidates.
//!
//! This module is deliberately planning-only. A candidate declares an opaque
//! placement identity, a positive batch size, and a provider-owned numeric
//! preference score. A trusted observer/test provider supplies per-placement
//! available batch-item capacity. Boolean capacity screening happens before
//! numeric preference ranking: `False` candidates are pruned, any structurally
//! relevant `Unknown` blocks selection, and only complete `True` evidence enters
//! numeric ranking.
//!
//! No device is discovered, reserved, leased, scheduled, or actuated here.
//! Placement execution/orchestration remains owned by the downstream runtime or
//! Hub boundary. A `Selected` result is explanatory planning evidence only.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use elastic_core::{PredicateKey, TruthValue};
use serde::{Deserialize, Serialize};

/// Stable namespace for BE14g candidate capacity predicates.
pub const BATCH_DEVICE_PREDICATE_NAMESPACE: &str = "elastic.batch-device";
/// Unit of the explicit per-placement capacity source and requested batch size.
pub const BATCH_DEVICE_CAPACITY_SOURCE_UNIT: &str = "batch-items";
/// Maximum age accepted for placement-capacity evidence.
pub const BATCH_DEVICE_MAX_AGE: Duration = Duration::from_secs(1);
/// Maximum candidates evaluated by one bounded planning pass.
pub const MAX_BATCH_DEVICE_CANDIDATES: usize = 64;
/// Maximum per-placement samples accepted by one snapshot.
pub const MAX_BATCH_DEVICE_SAMPLES: usize = 128;
/// Durable BE14g decision-trace schema.
pub const BOOLEAN_BATCH_DEVICE_DECISION_TRACE_SCHEMA_V1: u16 = 1;
/// Maximum JSON bytes accepted or emitted by the BE14g trace codec.
pub const MAX_BOOLEAN_BATCH_DEVICE_TRACE_BYTES: usize = 64 * 1024;
const MAX_CANDIDATE_ID_BYTES: usize = 32;
const MAX_PLACEMENT_ID_BYTES: usize = 128;
const MAX_SOURCE_ID_BYTES: usize = 128;
const MAX_EXACT_F64_INTEGER: f64 = 9_007_199_254_740_992.0;

/// Stable candidate-scoped BE14g predicate key.
///
/// Candidate identifiers are intentionally restricted to the same canonical
/// vocabulary accepted by [`PredicateKey`], so a durable key never depends on
/// runtime compact IDs.
pub fn batch_device_capacity_predicate_key(candidate_id: &str) -> Result<PredicateKey, String> {
    PredicateKey::new(
        BATCH_DEVICE_PREDICATE_NAMESPACE,
        format!("{candidate_id}-capacity-fit"),
    )
    .map_err(|error| error.to_string())
}

/// One explicitly declared batch/device-placement candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchDeviceCandidateV1 {
    candidate_id: String,
    placement_id: String,
    batch_size: u32,
    preference_score: u64,
}

impl BatchDeviceCandidateV1 {
    /// Construct one bounded planning candidate.
    ///
    /// Lower `preference_score` values are preferred, but only after Boolean
    /// screening. The score is provider policy, not a measured performance value.
    pub fn new(
        candidate_id: impl Into<String>,
        placement_id: impl Into<String>,
        batch_size: u32,
        preference_score: u64,
    ) -> Result<Self, String> {
        let candidate_id = candidate_id.into();
        let placement_id = placement_id.into();
        if candidate_id.is_empty() || candidate_id.len() > MAX_CANDIDATE_ID_BYTES {
            return Err(format!(
                "BE14g candidate id must contain 1..={MAX_CANDIDATE_ID_BYTES} bytes"
            ));
        }
        batch_device_capacity_predicate_key(&candidate_id)?;
        validate_text("placement id", &placement_id, MAX_PLACEMENT_ID_BYTES)?;
        if batch_size == 0 {
            return Err("BE14g batch size must be non-zero".into());
        }
        Ok(Self {
            candidate_id,
            placement_id,
            batch_size,
            preference_score,
        })
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub fn placement_id(&self) -> &str {
        &self.placement_id
    }

    #[must_use]
    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }

    #[must_use]
    pub const fn preference_score(&self) -> u64 {
        self.preference_score
    }
}

/// One trusted/test-provider capacity sample for an opaque placement identity.
///
/// `available_batch_items` is a provider-defined capacity count in
/// [`BATCH_DEVICE_CAPACITY_SOURCE_UNIT`]. It is not memory bytes, queue depth,
/// throughput, or proof that a physical device is healthy.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchDeviceCapacitySampleV1 {
    placement_id: String,
    available_batch_items: f64,
    observed_at: Instant,
    valid: bool,
}

impl BatchDeviceCapacitySampleV1 {
    /// Create a provider-declared valid numeric sample.
    pub fn valid(
        placement_id: impl Into<String>,
        available_batch_items: f64,
        observed_at: Instant,
    ) -> Result<Self, String> {
        let placement_id = placement_id.into();
        validate_text("placement id", &placement_id, MAX_PLACEMENT_ID_BYTES)?;
        Ok(Self {
            placement_id,
            available_batch_items,
            observed_at,
            valid: true,
        })
    }

    /// Create an explicit unsupported/invalid sample without fabricating zero.
    pub fn unsupported(
        placement_id: impl Into<String>,
        observed_at: Instant,
    ) -> Result<Self, String> {
        let placement_id = placement_id.into();
        validate_text("placement id", &placement_id, MAX_PLACEMENT_ID_BYTES)?;
        Ok(Self {
            placement_id,
            available_batch_items: f64::NAN,
            observed_at,
            valid: false,
        })
    }

    #[must_use]
    pub fn placement_id(&self) -> &str {
        &self.placement_id
    }
}

/// Bounded exact-unit observation set used by BE14g screening.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchDeviceCapacitySnapshotV1 {
    source_id: String,
    source_unit: String,
    source_generation: u64,
    samples: Vec<BatchDeviceCapacitySampleV1>,
}

impl BatchDeviceCapacitySnapshotV1 {
    /// Construct a bounded snapshot and reject duplicate placement identities.
    pub fn new(
        source_id: impl Into<String>,
        source_unit: impl Into<String>,
        samples: Vec<BatchDeviceCapacitySampleV1>,
    ) -> Result<Self, String> {
        Self::new_with_generation(source_id, source_unit, 0, samples)
    }

    /// Construct a bounded snapshot with an explicit provider generation.
    ///
    /// The generation is an opaque monotonic/provider-owned evidence identity.
    /// It is persisted only for explanatory context binding; it is never an
    /// actuation lease, fencing token, or authorization.
    pub fn new_with_generation(
        source_id: impl Into<String>,
        source_unit: impl Into<String>,
        source_generation: u64,
        samples: Vec<BatchDeviceCapacitySampleV1>,
    ) -> Result<Self, String> {
        let source_id = source_id.into();
        let source_unit = source_unit.into();
        validate_text("source id", &source_id, MAX_SOURCE_ID_BYTES)?;
        validate_text("source unit", &source_unit, 64)?;
        if samples.len() > MAX_BATCH_DEVICE_SAMPLES {
            return Err(format!(
                "BE14g sample count exceeds {MAX_BATCH_DEVICE_SAMPLES}"
            ));
        }
        let mut placements = BTreeSet::new();
        for sample in &samples {
            if !placements.insert(sample.placement_id.clone()) {
                return Err(format!(
                    "duplicate BE14g placement sample {:?}",
                    sample.placement_id
                ));
            }
        }
        Ok(Self {
            source_id,
            source_unit,
            source_generation,
            samples,
        })
    }

    /// Opaque provider generation bound into durable decision traces.
    #[must_use]
    pub const fn source_generation(&self) -> u64 {
        self.source_generation
    }
}

/// Durable explanatory evidence for one screened candidate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanBatchDeviceCandidateEvidenceV1 {
    pub candidate_id: String,
    pub placement_id: String,
    pub batch_size: u32,
    pub preference_score: u64,
    pub predicate_key: String,
    pub source_id: String,
    pub source_unit: String,
    pub observed_available_batch_items: Option<u64>,
    pub truth: String,
    pub reason: String,
}

/// Fail-closed BE14g planning-only outcome.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BooleanBatchDeviceOutcomeV1 {
    /// Lowest provider-owned score among candidates with complete `True` evidence.
    Selected {
        candidate_id: String,
        placement_id: String,
        batch_size: u32,
        preference_score: u64,
    },
    /// Every declared candidate was conclusively `False`.
    NoCandidate,
    /// At least one candidate had unusable/missing evidence, so ranking is blocked.
    InsufficientEvidence { blocking_candidate_id: String },
}

/// Durable BE14g screening report. This report grants no actuation authority.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanBatchDeviceReportV1 {
    pub schema_version: u16,
    pub source_id: String,
    pub source_unit: String,
    pub expected_source_unit: String,
    pub outcome: BooleanBatchDeviceOutcomeV1,
    pub candidates: Vec<BooleanBatchDeviceCandidateEvidenceV1>,
}

/// Bounded durable BE14g decision evidence.
///
/// This trace binds the complete declared candidate policy, the provider source
/// identity/generation and the exact planning result. It is explanatory only:
/// decoding or context validation cannot reserve a placement, dispatch work, or
/// authorize any downstream actuation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanBatchDeviceDecisionTraceV1 {
    schema_version: u16,
    source_id: String,
    source_unit: String,
    source_generation: u64,
    policy_fingerprint: u64,
    max_age_secs: u64,
    max_age_nanos: u32,
    outcome: BooleanBatchDeviceOutcomeV1,
    candidates: Vec<BooleanBatchDeviceCandidateEvidenceV1>,
}

impl BooleanBatchDeviceDecisionTraceV1 {
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    #[must_use]
    pub const fn source_generation(&self) -> u64 {
        self.source_generation
    }

    #[must_use]
    pub const fn policy_fingerprint(&self) -> u64 {
        self.policy_fingerprint
    }

    #[must_use]
    pub const fn outcome(&self) -> &BooleanBatchDeviceOutcomeV1 {
        &self.outcome
    }

    #[must_use]
    pub fn candidates(&self) -> &[BooleanBatchDeviceCandidateEvidenceV1] {
        &self.candidates
    }

    /// Encode bounded explanatory evidence.
    pub fn to_bounded_json(&self) -> Result<String, String> {
        self.validate()?;
        let json = serde_json::to_string(self).map_err(|error| error.to_string())?;
        if json.len() > MAX_BOOLEAN_BATCH_DEVICE_TRACE_BYTES {
            return Err("BE14g decision trace exceeds the persisted byte bound".into());
        }
        Ok(json)
    }

    /// Strictly decode bounded explanatory evidence.
    pub fn from_bounded_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BOOLEAN_BATCH_DEVICE_TRACE_BYTES {
            return Err("BE14g decision trace input exceeds the persisted byte bound".into());
        }
        let trace: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        trace.validate()?;
        Ok(trace)
    }

    /// Recompute one explicit planning context and require byte-semantic equality.
    ///
    /// This is a replay-style explanatory check only. It does not establish
    /// freshness for later actuation and never calls an actuation backend.
    pub fn validate_explanatory_context(
        &self,
        planner: &BooleanBatchDevicePreplannerV1,
        snapshot: &BatchDeviceCapacitySnapshotV1,
        now: Instant,
    ) -> Result<(), String> {
        let expected = planner.decision_trace(snapshot, now)?;
        if &expected != self {
            return Err("BE14g decision trace context mismatch".into());
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != BOOLEAN_BATCH_DEVICE_DECISION_TRACE_SCHEMA_V1 {
            return Err("unsupported BE14g decision trace schema".into());
        }
        validate_text("trace source id", &self.source_id, MAX_SOURCE_ID_BYTES)?;
        validate_text("trace source unit", &self.source_unit, 64)?;
        if self.max_age_secs != BATCH_DEVICE_MAX_AGE.as_secs()
            || self.max_age_nanos != BATCH_DEVICE_MAX_AGE.subsec_nanos()
        {
            return Err("BE14g decision trace freshness contract mismatch".into());
        }
        if self.candidates.is_empty() || self.candidates.len() > MAX_BATCH_DEVICE_CANDIDATES {
            return Err("BE14g decision trace candidate count is outside bounds".into());
        }
        let computed_policy = batch_device_policy_fingerprint(&self.candidates)?;
        if computed_policy != self.policy_fingerprint {
            return Err("BE14g decision trace policy fingerprint mismatch".into());
        }
        validate_trace_outcome(
            &self.outcome,
            &self.candidates,
            &self.source_id,
            &self.source_unit,
        )
    }
}

/// Planning-only BE14g Boolean preplanner.
#[derive(Clone, Debug)]
pub struct BooleanBatchDevicePreplannerV1 {
    candidates: Vec<BatchDeviceCandidateV1>,
}

impl BooleanBatchDevicePreplannerV1 {
    /// Bind one bounded declared candidate policy.
    pub fn new(mut candidates: Vec<BatchDeviceCandidateV1>) -> Result<Self, String> {
        if candidates.is_empty() {
            return Err("BE14g requires at least one batch/device candidate".into());
        }
        if candidates.len() > MAX_BATCH_DEVICE_CANDIDATES {
            return Err(format!(
                "BE14g candidate count exceeds {MAX_BATCH_DEVICE_CANDIDATES}"
            ));
        }
        candidates.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
        for pair in candidates.windows(2) {
            if pair[0].candidate_id == pair[1].candidate_id {
                return Err(format!(
                    "duplicate BE14g candidate id {:?}",
                    pair[0].candidate_id
                ));
            }
        }
        Ok(Self { candidates })
    }

    /// Stable non-cryptographic identity of the exact declared candidate policy.
    ///
    /// This fingerprint is only an evidence-association guard. It is not a
    /// signature, attestation, lease, or authorization token.
    #[must_use]
    pub fn policy_fingerprint(&self) -> u64 {
        policy_fingerprint_from_declared_candidates(&self.candidates)
    }

    pub(crate) fn candidate_by_id(&self, candidate_id: &str) -> Option<&BatchDeviceCandidateV1> {
        self.candidates
            .iter()
            .find(|candidate| candidate.candidate_id == candidate_id)
    }

    pub(crate) fn evaluate_candidate_truth(
        &self,
        candidate: &BatchDeviceCandidateV1,
        snapshot: &BatchDeviceCapacitySnapshotV1,
        now: Instant,
    ) -> TruthValue {
        let sample = snapshot
            .samples
            .iter()
            .find(|sample| sample.placement_id == candidate.placement_id);
        evaluate_candidate(snapshot, sample, candidate, now).0
    }

    /// Capture a strict, bounded, decision-only trace for one planning pass.
    pub fn decision_trace(
        &self,
        snapshot: &BatchDeviceCapacitySnapshotV1,
        now: Instant,
    ) -> Result<BooleanBatchDeviceDecisionTraceV1, String> {
        let report = self.screen(snapshot, now);
        let trace = BooleanBatchDeviceDecisionTraceV1 {
            schema_version: BOOLEAN_BATCH_DEVICE_DECISION_TRACE_SCHEMA_V1,
            source_id: snapshot.source_id.clone(),
            source_unit: snapshot.source_unit.clone(),
            source_generation: snapshot.source_generation,
            policy_fingerprint: self.policy_fingerprint(),
            max_age_secs: BATCH_DEVICE_MAX_AGE.as_secs(),
            max_age_nanos: BATCH_DEVICE_MAX_AGE.subsec_nanos(),
            outcome: report.outcome,
            candidates: report.candidates,
        };
        trace.validate()?;
        Ok(trace)
    }

    /// Screen capacity first, then rank only complete `True` survivors.
    ///
    /// Missing, future, stale, unsupported, non-finite, fractional, negative,
    /// over-precise, or unit-mismatched capacity evidence maps to `Unknown`.
    /// Any `Unknown` blocks numeric ranking because silently dropping an
    /// unresolved placement could change declared placement semantics.
    #[must_use]
    pub fn screen(
        &self,
        snapshot: &BatchDeviceCapacitySnapshotV1,
        now: Instant,
    ) -> BooleanBatchDeviceReportV1 {
        let mut evidence = Vec::with_capacity(self.candidates.len());
        let mut unknown = None;
        let mut eligible = Vec::new();

        for candidate in &self.candidates {
            let sample = snapshot
                .samples
                .iter()
                .find(|sample| sample.placement_id == candidate.placement_id);
            let (truth, observed, reason) = evaluate_candidate(snapshot, sample, candidate, now);
            if truth == TruthValue::Unknown && unknown.is_none() {
                unknown = Some(candidate.candidate_id.clone());
            }
            if truth == TruthValue::True {
                eligible.push(candidate);
            }
            evidence.push(BooleanBatchDeviceCandidateEvidenceV1 {
                candidate_id: candidate.candidate_id.clone(),
                placement_id: candidate.placement_id.clone(),
                batch_size: candidate.batch_size,
                preference_score: candidate.preference_score,
                predicate_key: batch_device_capacity_predicate_key(&candidate.candidate_id)
                    .expect("candidate was validated at construction")
                    .to_string(),
                source_id: snapshot.source_id.clone(),
                source_unit: snapshot.source_unit.clone(),
                observed_available_batch_items: observed,
                truth: truth_text(truth).to_owned(),
                reason: reason.to_owned(),
            });
        }

        let outcome = if let Some(blocking_candidate_id) = unknown {
            BooleanBatchDeviceOutcomeV1::InsufficientEvidence {
                blocking_candidate_id,
            }
        } else if let Some(selected) = eligible.into_iter().min_by(|left, right| {
            left.preference_score
                .cmp(&right.preference_score)
                .then_with(|| left.candidate_id.cmp(&right.candidate_id))
        }) {
            BooleanBatchDeviceOutcomeV1::Selected {
                candidate_id: selected.candidate_id.clone(),
                placement_id: selected.placement_id.clone(),
                batch_size: selected.batch_size,
                preference_score: selected.preference_score,
            }
        } else {
            BooleanBatchDeviceOutcomeV1::NoCandidate
        };

        BooleanBatchDeviceReportV1 {
            schema_version: 1,
            source_id: snapshot.source_id.clone(),
            source_unit: snapshot.source_unit.clone(),
            expected_source_unit: BATCH_DEVICE_CAPACITY_SOURCE_UNIT.to_owned(),
            outcome,
            candidates: evidence,
        }
    }
}

const FNV1A64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a64_update(mut state: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV1A64_PRIME);
    }
    state
}

fn hash_trace_field(mut state: u64, label: &str, value: &str) -> u64 {
    state = fnv1a64_update(state, label.as_bytes());
    state = fnv1a64_update(state, b"=");
    state = fnv1a64_update(state, value.len().to_string().as_bytes());
    state = fnv1a64_update(state, b":");
    state = fnv1a64_update(state, value.as_bytes());
    fnv1a64_update(state, b"\n")
}

fn policy_fingerprint_from_declared_candidates(candidates: &[BatchDeviceCandidateV1]) -> u64 {
    let mut state = hash_trace_field(FNV1A64_OFFSET, "schema", "be14g.batch-device-policy.v1");
    for candidate in candidates {
        state = hash_trace_field(state, "candidate_id", &candidate.candidate_id);
        state = hash_trace_field(state, "placement_id", &candidate.placement_id);
        state = hash_trace_field(state, "batch_size", &candidate.batch_size.to_string());
        state = hash_trace_field(
            state,
            "preference_score",
            &candidate.preference_score.to_string(),
        );
    }
    state
}

fn batch_device_policy_fingerprint(
    candidates: &[BooleanBatchDeviceCandidateEvidenceV1],
) -> Result<u64, String> {
    let mut prior: Option<&str> = None;
    let mut state = hash_trace_field(FNV1A64_OFFSET, "schema", "be14g.batch-device-policy.v1");
    for candidate in candidates {
        validate_text(
            "trace candidate id",
            &candidate.candidate_id,
            MAX_CANDIDATE_ID_BYTES,
        )?;
        batch_device_capacity_predicate_key(&candidate.candidate_id)?;
        validate_text(
            "trace placement id",
            &candidate.placement_id,
            MAX_PLACEMENT_ID_BYTES,
        )?;
        if candidate.batch_size == 0 {
            return Err("BE14g decision trace contains zero batch size".into());
        }
        if prior.is_some_and(|value| value >= candidate.candidate_id.as_str()) {
            return Err("BE14g decision trace candidates are not strictly ordered".into());
        }
        prior = Some(&candidate.candidate_id);
        state = hash_trace_field(state, "candidate_id", &candidate.candidate_id);
        state = hash_trace_field(state, "placement_id", &candidate.placement_id);
        state = hash_trace_field(state, "batch_size", &candidate.batch_size.to_string());
        state = hash_trace_field(
            state,
            "preference_score",
            &candidate.preference_score.to_string(),
        );
    }
    Ok(state)
}

fn validate_trace_outcome(
    outcome: &BooleanBatchDeviceOutcomeV1,
    candidates: &[BooleanBatchDeviceCandidateEvidenceV1],
    source_id: &str,
    source_unit: &str,
) -> Result<(), String> {
    let mut first_unknown = None;
    let mut best_true: Option<&BooleanBatchDeviceCandidateEvidenceV1> = None;
    for candidate in candidates {
        if candidate.source_id != source_id || candidate.source_unit != source_unit {
            return Err("BE14g decision trace candidate source context mismatch".into());
        }
        let expected_key =
            batch_device_capacity_predicate_key(&candidate.candidate_id)?.to_string();
        if candidate.predicate_key != expected_key {
            return Err("BE14g decision trace predicate key mismatch".into());
        }
        if candidate.reason.trim().is_empty() || candidate.reason.len() > 128 {
            return Err("BE14g decision trace reason is outside bounds".into());
        }
        match candidate.truth.as_str() {
            "unknown" => {
                if candidate.observed_available_batch_items.is_some() {
                    return Err(
                        "BE14g Unknown trace candidate cannot carry accepted capacity".into(),
                    );
                }
                first_unknown.get_or_insert(candidate.candidate_id.as_str());
            }
            "false" => {
                let observed = candidate.observed_available_batch_items.ok_or_else(|| {
                    "BE14g False trace candidate lacks grounded capacity".to_owned()
                })?;
                if observed >= u64::from(candidate.batch_size) {
                    return Err("BE14g False trace candidate satisfies its capacity bound".into());
                }
            }
            "true" => {
                if candidate.reason != "batch-capacity-satisfied" {
                    return Err("BE14g True trace candidate reason is inconsistent".into());
                }
                let observed = candidate.observed_available_batch_items.ok_or_else(|| {
                    "BE14g True trace candidate lacks grounded capacity".to_owned()
                })?;
                if observed < u64::from(candidate.batch_size) {
                    return Err("BE14g True trace candidate violates its capacity bound".into());
                }
                if best_true.is_none_or(|best| {
                    (candidate.preference_score, candidate.candidate_id.as_str())
                        < (best.preference_score, best.candidate_id.as_str())
                }) {
                    best_true = Some(candidate);
                }
            }
            _ => return Err("BE14g decision trace contains invalid truth text".into()),
        }
    }

    match (first_unknown, best_true, outcome) {
        (
            Some(expected),
            _,
            BooleanBatchDeviceOutcomeV1::InsufficientEvidence {
                blocking_candidate_id,
            },
        ) if blocking_candidate_id == expected => Ok(()),
        (Some(_), _, _) => Err("BE14g decision trace does not preserve Unknown blocking".into()),
        (
            None,
            Some(best),
            BooleanBatchDeviceOutcomeV1::Selected {
                candidate_id,
                placement_id,
                batch_size,
                preference_score,
            },
        ) if candidate_id == &best.candidate_id
            && placement_id == &best.placement_id
            && batch_size == &best.batch_size
            && preference_score == &best.preference_score =>
        {
            Ok(())
        }
        (None, Some(_), _) => Err("BE14g decision trace selected outcome is inconsistent".into()),
        (None, None, BooleanBatchDeviceOutcomeV1::NoCandidate) => Ok(()),
        (None, None, _) => Err("BE14g decision trace no-candidate outcome is inconsistent".into()),
    }
}

fn evaluate_candidate(
    snapshot: &BatchDeviceCapacitySnapshotV1,
    sample: Option<&BatchDeviceCapacitySampleV1>,
    candidate: &BatchDeviceCandidateV1,
    now: Instant,
) -> (TruthValue, Option<u64>, &'static str) {
    if snapshot.source_unit != BATCH_DEVICE_CAPACITY_SOURCE_UNIT {
        return (TruthValue::Unknown, None, "source-unit-mismatch");
    }
    let Some(sample) = sample else {
        return (TruthValue::Unknown, None, "placement-capacity-missing");
    };
    if !sample.valid || !sample.available_batch_items.is_finite() {
        return (TruthValue::Unknown, None, "placement-capacity-invalid");
    }
    let Some(age) = now.checked_duration_since(sample.observed_at) else {
        return (TruthValue::Unknown, None, "placement-capacity-from-future");
    };
    if age > BATCH_DEVICE_MAX_AGE {
        return (TruthValue::Unknown, None, "placement-capacity-stale");
    }
    let value = sample.available_batch_items;
    if !(0.0..=MAX_EXACT_F64_INTEGER).contains(&value) || value.fract() != 0.0 {
        return (
            TruthValue::Unknown,
            None,
            "placement-capacity-not-exact-integer",
        );
    }
    let observed = value as u64;
    if observed >= u64::from(candidate.batch_size) {
        (TruthValue::True, Some(observed), "batch-capacity-satisfied")
    } else {
        (
            TruthValue::False,
            Some(observed),
            "batch-capacity-insufficient",
        )
    }
}

fn validate_text(label: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.trim() != value || value.len() > max_bytes {
        return Err(format!(
            "BE14g {label} must be non-blank, trimmed, and at most {max_bytes} bytes"
        ));
    }
    Ok(())
}

const fn truth_text(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, placement: &str, batch: u32, score: u64) -> BatchDeviceCandidateV1 {
        BatchDeviceCandidateV1::new(id, placement, batch, score).unwrap()
    }

    fn snapshot(
        _now: Instant,
        samples: Vec<BatchDeviceCapacitySampleV1>,
    ) -> BatchDeviceCapacitySnapshotV1 {
        BatchDeviceCapacitySnapshotV1::new(
            "be14g-test-provider",
            BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
            samples,
        )
        .unwrap()
    }

    #[test]
    fn false_candidates_are_pruned_before_numeric_preference_ranking() {
        let planner = BooleanBatchDevicePreplannerV1::new(vec![
            candidate("small-fast", "device-a", 8, 1),
            candidate("balanced", "device-b", 4, 10),
            candidate("fallback", "device-c", 2, 20),
        ])
        .unwrap();
        let now = Instant::now();
        let report = planner.screen(
            &snapshot(
                now,
                vec![
                    BatchDeviceCapacitySampleV1::valid("device-a", 4.0, now).unwrap(),
                    BatchDeviceCapacitySampleV1::valid("device-b", 8.0, now).unwrap(),
                    BatchDeviceCapacitySampleV1::valid("device-c", 8.0, now).unwrap(),
                ],
            ),
            now,
        );
        assert_eq!(
            report.outcome,
            BooleanBatchDeviceOutcomeV1::Selected {
                candidate_id: "balanced".into(),
                placement_id: "device-b".into(),
                batch_size: 4,
                preference_score: 10,
            }
        );
        let first = report
            .candidates
            .iter()
            .find(|item| item.candidate_id == "small-fast")
            .unwrap();
        assert_eq!(first.truth, "false");
    }

    #[test]
    fn numeric_preference_is_applied_only_to_true_survivors() {
        let planner = BooleanBatchDevicePreplannerV1::new(vec![
            candidate("candidate-a", "device-a", 2, 50),
            candidate("candidate-b", "device-b", 2, 5),
        ])
        .unwrap();
        let now = Instant::now();
        let report = planner.screen(
            &snapshot(
                now,
                vec![
                    BatchDeviceCapacitySampleV1::valid("device-a", 2.0, now).unwrap(),
                    BatchDeviceCapacitySampleV1::valid("device-b", 2.0, now).unwrap(),
                ],
            ),
            now,
        );
        assert!(matches!(
            report.outcome,
            BooleanBatchDeviceOutcomeV1::Selected {
                ref candidate_id,
                preference_score: 5,
                ..
            } if candidate_id == "candidate-b"
        ));
    }

    #[test]
    fn missing_stale_invalid_future_and_wrong_unit_are_unknown() {
        let planner =
            BooleanBatchDevicePreplannerV1::new(vec![candidate("candidate-a", "device-a", 2, 1)])
                .unwrap();
        let now = Instant::now();
        let cases = vec![
            BatchDeviceCapacitySnapshotV1::new("test", BATCH_DEVICE_CAPACITY_SOURCE_UNIT, vec![])
                .unwrap(),
            snapshot(
                now,
                vec![BatchDeviceCapacitySampleV1::valid(
                    "device-a",
                    4.0,
                    now.checked_sub(Duration::from_secs(2)).unwrap(),
                )
                .unwrap()],
            ),
            snapshot(
                now,
                vec![BatchDeviceCapacitySampleV1::unsupported("device-a", now).unwrap()],
            ),
            snapshot(
                now,
                vec![BatchDeviceCapacitySampleV1::valid(
                    "device-a",
                    4.0,
                    now.checked_add(Duration::from_millis(1)).unwrap(),
                )
                .unwrap()],
            ),
            BatchDeviceCapacitySnapshotV1::new(
                "test",
                "bytes",
                vec![BatchDeviceCapacitySampleV1::valid("device-a", 4.0, now).unwrap()],
            )
            .unwrap(),
            snapshot(
                now,
                vec![BatchDeviceCapacitySampleV1::valid("device-a", 2.5, now).unwrap()],
            ),
        ];
        for observed in cases {
            let report = planner.screen(&observed, now);
            assert!(matches!(
                report.outcome,
                BooleanBatchDeviceOutcomeV1::InsufficientEvidence { .. }
            ));
            assert_eq!(report.candidates[0].truth, "unknown");
        }
    }

    #[test]
    fn one_unknown_candidate_blocks_later_true_candidate() {
        let planner = BooleanBatchDevicePreplannerV1::new(vec![
            candidate("candidate-a", "device-a", 2, 100),
            candidate("candidate-b", "device-b", 2, 1),
        ])
        .unwrap();
        let now = Instant::now();
        let report = planner.screen(
            &snapshot(
                now,
                vec![BatchDeviceCapacitySampleV1::valid("device-b", 8.0, now).unwrap()],
            ),
            now,
        );
        assert_eq!(
            report.outcome,
            BooleanBatchDeviceOutcomeV1::InsufficientEvidence {
                blocking_candidate_id: "candidate-a".into(),
            }
        );
    }

    #[test]
    fn all_false_candidates_produce_no_candidate() {
        let planner = BooleanBatchDevicePreplannerV1::new(vec![
            candidate("candidate-a", "device-a", 8, 1),
            candidate("candidate-b", "device-b", 4, 2),
        ])
        .unwrap();
        let now = Instant::now();
        let report = planner.screen(
            &snapshot(
                now,
                vec![
                    BatchDeviceCapacitySampleV1::valid("device-a", 1.0, now).unwrap(),
                    BatchDeviceCapacitySampleV1::valid("device-b", 1.0, now).unwrap(),
                ],
            ),
            now,
        );
        assert_eq!(report.outcome, BooleanBatchDeviceOutcomeV1::NoCandidate);
    }

    #[test]
    fn candidates_samples_and_ids_are_bounded_and_unambiguous() {
        assert!(BatchDeviceCandidateV1::new("Bad", "device-a", 1, 0).is_err());
        assert!(BatchDeviceCandidateV1::new("candidate-a", "device-a", 0, 0).is_err());
        assert!(BooleanBatchDevicePreplannerV1::new(vec![
            candidate("duplicate", "a", 1, 1),
            candidate("duplicate", "b", 1, 2),
        ])
        .is_err());
        let now = Instant::now();
        assert!(BatchDeviceCapacitySnapshotV1::new(
            "test",
            BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
            vec![
                BatchDeviceCapacitySampleV1::valid("device-a", 1.0, now).unwrap(),
                BatchDeviceCapacitySampleV1::valid("device-a", 2.0, now).unwrap(),
            ],
        )
        .is_err());
    }

    #[test]
    fn decision_trace_roundtrips_and_revalidates_exact_explanatory_context() {
        let planner = BooleanBatchDevicePreplannerV1::new(vec![
            candidate("candidate-a", "device-a", 8, 1),
            candidate("candidate-b", "device-b", 2, 10),
        ])
        .unwrap();
        let now = Instant::now();
        let snapshot = BatchDeviceCapacitySnapshotV1::new_with_generation(
            "be14g-test-provider",
            BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
            17,
            vec![
                BatchDeviceCapacitySampleV1::valid("device-a", 4.0, now).unwrap(),
                BatchDeviceCapacitySampleV1::valid("device-b", 8.0, now).unwrap(),
            ],
        )
        .unwrap();
        let trace = planner.decision_trace(&snapshot, now).unwrap();
        let encoded = trace.to_bounded_json().unwrap();
        let decoded =
            BooleanBatchDeviceDecisionTraceV1::from_bounded_json(encoded.as_bytes()).unwrap();
        assert_eq!(decoded, trace);
        assert_eq!(decoded.source_generation(), 17);
        decoded
            .validate_explanatory_context(&planner, &snapshot, now)
            .unwrap();
    }

    #[test]
    fn decision_trace_context_drift_fails_closed_without_actuation() {
        let planner =
            BooleanBatchDevicePreplannerV1::new(vec![candidate("candidate-a", "device-a", 2, 1)])
                .unwrap();
        let now = Instant::now();
        let snapshot = BatchDeviceCapacitySnapshotV1::new_with_generation(
            "be14g-test-provider",
            BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
            5,
            vec![BatchDeviceCapacitySampleV1::valid("device-a", 4.0, now).unwrap()],
        )
        .unwrap();
        let trace = planner.decision_trace(&snapshot, now).unwrap();

        let changed_generation = BatchDeviceCapacitySnapshotV1::new_with_generation(
            "be14g-test-provider",
            BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
            6,
            vec![BatchDeviceCapacitySampleV1::valid("device-a", 4.0, now).unwrap()],
        )
        .unwrap();
        assert!(trace
            .validate_explanatory_context(&planner, &changed_generation, now)
            .is_err());

        let changed_policy =
            BooleanBatchDevicePreplannerV1::new(vec![candidate("candidate-a", "device-a", 3, 1)])
                .unwrap();
        assert!(trace
            .validate_explanatory_context(&changed_policy, &snapshot, now)
            .is_err());
    }

    #[test]
    fn decision_trace_decoder_rejects_unknown_duplicate_future_and_oversized_input() {
        let planner =
            BooleanBatchDevicePreplannerV1::new(vec![candidate("candidate-a", "device-a", 2, 1)])
                .unwrap();
        let now = Instant::now();
        let snapshot = snapshot(
            now,
            vec![BatchDeviceCapacitySampleV1::valid("device-a", 4.0, now).unwrap()],
        );
        let encoded = planner
            .decision_trace(&snapshot, now)
            .unwrap()
            .to_bounded_json()
            .unwrap();

        let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        value["unknown_field"] = serde_json::json!(true);
        assert!(BooleanBatchDeviceDecisionTraceV1::from_bounded_json(
            serde_json::to_string(&value).unwrap().as_bytes()
        )
        .is_err());

        let duplicate = encoded.replacen(
            "{\"schema_version\":1,",
            "{\"schema_version\":1,\"schema_version\":1,",
            1,
        );
        assert!(
            BooleanBatchDeviceDecisionTraceV1::from_bounded_json(duplicate.as_bytes()).is_err()
        );

        let mut future: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        future["schema_version"] = serde_json::json!(2);
        assert!(BooleanBatchDeviceDecisionTraceV1::from_bounded_json(
            serde_json::to_string(&future).unwrap().as_bytes()
        )
        .is_err());

        let oversized = vec![b' '; MAX_BOOLEAN_BATCH_DEVICE_TRACE_BYTES + 1];
        assert!(BooleanBatchDeviceDecisionTraceV1::from_bounded_json(&oversized).is_err());
    }

    #[test]
    fn decision_trace_decoder_rejects_policy_and_reason_tampering() {
        let planner =
            BooleanBatchDevicePreplannerV1::new(vec![candidate("candidate-a", "device-a", 2, 1)])
                .unwrap();
        let now = Instant::now();
        let snapshot = snapshot(
            now,
            vec![BatchDeviceCapacitySampleV1::valid("device-a", 4.0, now).unwrap()],
        );
        let encoded = planner
            .decision_trace(&snapshot, now)
            .unwrap()
            .to_bounded_json()
            .unwrap();

        let mut policy: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        policy["candidates"][0]["batch_size"] = serde_json::json!(3);
        assert!(BooleanBatchDeviceDecisionTraceV1::from_bounded_json(
            serde_json::to_string(&policy).unwrap().as_bytes()
        )
        .is_err());

        let mut reason: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        reason["candidates"][0]["reason"] = serde_json::json!("placement-capacity-missing");
        assert!(BooleanBatchDeviceDecisionTraceV1::from_bounded_json(
            serde_json::to_string(&reason).unwrap().as_bytes()
        )
        .is_err());
    }

    #[test]
    fn report_roundtrips_as_explicit_explanatory_data() {
        let planner =
            BooleanBatchDevicePreplannerV1::new(vec![candidate("candidate-a", "device-a", 1, 7)])
                .unwrap();
        let now = Instant::now();
        let report = planner.screen(
            &snapshot(
                now,
                vec![BatchDeviceCapacitySampleV1::valid("device-a", 2.0, now).unwrap()],
            ),
            now,
        );
        let json = serde_json::to_string(&report).unwrap();
        let decoded: BooleanBatchDeviceReportV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(report, decoded);
    }
}
