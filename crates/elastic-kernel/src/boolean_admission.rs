//! BE14f fail-closed Boolean front-end for kernel-realization planning.
//!
//! The front-end is deliberately narrower than the numerical kernel planner.
//! It classifies whether each declared realization is structurally relevant and
//! compatible with a fresh capability snapshot as `True`, `False`, or
//! `Unknown`. `False` candidates are pruned before objective ranking. Any
//! structurally relevant `Unknown` candidate blocks ranking rather than being
//! silently discarded, because it might have won had its capability evidence
//! been available.
//!
//! This module does not activate, compile, verify, or commit a kernel. The
//! existing [`crate::lifecycle`] remains authoritative for
//! `VALIDATE -> ACT -> VERIFY -> COMMIT / ROLLBACK` after planning.
//!
//! Capability sources and units are explicit: numeric workgroup/binding limits
//! are counts (`invocations`, `bind-groups`) or bytes, while optional features
//! are three-valued declarations (`known-true`, `known-false`, `unknown`). A
//! missing, future-dated, stale, or internally invalid snapshot therefore maps
//! to `Unknown`; it is never guessed as unsupported or supported.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use elastic_core::{LogicalResourceId, PredicateKey, TruthValue};
use elastic_eir::Fingerprint;
use serde::{Deserialize, Serialize};

use crate::{
    plan, CapabilityRejectionReason, CapabilitySnapshot, KernelCandidate, RealizationIdentity,
    SelectionOutcome, SelectionPolicy,
};

/// Namespace for BE14f kernel-realization predicates.
pub const KERNEL_CAPABILITY_PREDICATE_NAMESPACE: &str = "elastic.kernel";
/// Stable predicate meaning that the current trusted capability snapshot can
/// satisfy one candidate's declared kernel requirements.
pub const KERNEL_CAPABILITY_PREDICATE_NAME: &str = "capability-compatible";
/// Versioned source contract evaluated by this front-end.
pub const KERNEL_CAPABILITY_SOURCE_SCHEMA: &str = "elastic-kernel/capability-snapshot/v1";
/// Numeric unit discipline for capability limits used by the predicate.
pub const KERNEL_CAPABILITY_NUMERIC_UNITS: &str = "invocations|bind-groups|bytes";
/// Unit discipline for optional feature declarations.
pub const KERNEL_CAPABILITY_FEATURE_UNIT: &str = "known-true|known-false|unknown";
/// Default maximum age accepted for one capability observation.
pub const KERNEL_CAPABILITY_MAX_AGE: Duration = Duration::from_secs(1);
/// Maximum number of candidates accepted by one Boolean screening pass.
pub const MAX_BOOLEAN_KERNEL_CANDIDATES: usize = 128;

/// Durable BE14f trace schema.
pub const BOOLEAN_KERNEL_DECISION_TRACE_SCHEMA_V1: u16 = 1;
/// Maximum JSON evidence accepted or emitted by the kernel trace codec.
pub const MAX_BOOLEAN_KERNEL_DECISION_TRACE_BYTES: usize = 64 * 1024;
/// Maximum UTF-8 bytes in one persisted trace string.
pub const MAX_BOOLEAN_KERNEL_TRACE_STRING_BYTES: usize = 4 * 1024;
/// Maximum objective keys persisted in one trace.
pub const MAX_BOOLEAN_KERNEL_TRACE_OBJECTIVES: usize = 64;

/// Durable per-candidate explanation for BE14f Boolean screening.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanKernelCandidateTraceV1 {
    realization: String,
    predicate_key: String,
    truth: String,
    reason: String,
}

impl BooleanKernelCandidateTraceV1 {
    #[must_use]
    pub fn realization(&self) -> &str {
        &self.realization
    }
    #[must_use]
    pub fn predicate_key(&self) -> &str {
        &self.predicate_key
    }
    #[must_use]
    pub fn truth(&self) -> &str {
        &self.truth
    }
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Bounded durable evidence for a BE14f admission + planning decision.
///
/// This is explanatory data only. Decoding never validates or actuates a
/// kernel and cannot replace the trusted lifecycle immediately before action.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanKernelDecisionTraceV1 {
    schema_version: u16,
    source_schema: String,
    logical_resource_id: String,
    workload_fingerprint: u64,
    capability_fingerprint: Option<u64>,
    policy_contract: String,
    policy_objectives: Vec<String>,
    allow_static_estimates: bool,
    accept_uncontested_fallback: bool,
    max_age_secs: u64,
    max_age_nanos: u32,
    screen_outcome: String,
    planner_outcome: String,
    planner_detail: Option<String>,
    selected_realization: Option<String>,
    selection_fingerprint: Option<u64>,
    candidates: Vec<BooleanKernelCandidateTraceV1>,
}

impl BooleanKernelDecisionTraceV1 {
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }
    #[must_use]
    pub fn logical_resource_id(&self) -> &str {
        &self.logical_resource_id
    }
    #[must_use]
    pub const fn workload_fingerprint(&self) -> Fingerprint {
        Fingerprint::from_bits(self.workload_fingerprint)
    }
    #[must_use]
    pub const fn capability_fingerprint(&self) -> Option<Fingerprint> {
        match self.capability_fingerprint {
            Some(bits) => Some(Fingerprint::from_bits(bits)),
            None => None,
        }
    }
    #[must_use]
    pub fn screen_outcome(&self) -> &str {
        &self.screen_outcome
    }
    #[must_use]
    pub fn planner_outcome(&self) -> &str {
        &self.planner_outcome
    }
    #[must_use]
    pub fn selected_realization(&self) -> Option<&str> {
        self.selected_realization.as_deref()
    }
    /// Selection-record fingerprint captured by this explanatory trace.
    #[must_use]
    pub const fn selection_fingerprint(&self) -> Option<Fingerprint> {
        match self.selection_fingerprint {
            Some(bits) => Some(Fingerprint::from_bits(bits)),
            None => None,
        }
    }
    #[must_use]
    pub fn candidates(&self) -> &[BooleanKernelCandidateTraceV1] {
        &self.candidates
    }

    /// Encode bounded explanatory evidence.
    pub fn to_bounded_json(&self) -> Result<String, String> {
        self.validate()?;
        let json = serde_json::to_string(self).map_err(|error| error.to_string())?;
        if json.len() > MAX_BOOLEAN_KERNEL_DECISION_TRACE_BYTES {
            return Err("BE14f decision trace exceeds the persisted byte bound".into());
        }
        Ok(json)
    }

    /// Strictly decode bounded explanatory evidence.
    pub fn from_bounded_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BOOLEAN_KERNEL_DECISION_TRACE_BYTES {
            return Err("BE14f decision trace input exceeds the persisted byte bound".into());
        }
        let trace: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        trace.validate()?;
        Ok(trace)
    }

    /// Match historical evidence against an explicitly supplied decision context.
    /// This identity check is non-actuating and does not establish freshness.
    pub fn validate_explanatory_context(
        &self,
        logical_resource_id: &LogicalResourceId,
        workload_fingerprint: Fingerprint,
        capability_fingerprint: Option<Fingerprint>,
        policy: &SelectionPolicy,
    ) -> Result<(), String> {
        let objectives: Vec<_> = policy
            .objectives()
            .iter()
            .map(|objective| objective.as_str().to_owned())
            .collect();
        if self.logical_resource_id != logical_resource_id.as_str()
            || self.workload_fingerprint != workload_fingerprint.bits()
            || self.capability_fingerprint != capability_fingerprint.map(Fingerprint::bits)
            || self.policy_contract != policy.contract().as_str()
            || self.policy_objectives != objectives
            || self.allow_static_estimates != policy.allows_static_estimates()
            || self.accept_uncontested_fallback != policy.accepts_uncontested_fallback()
        {
            return Err("BE14f decision trace context mismatch".into());
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != BOOLEAN_KERNEL_DECISION_TRACE_SCHEMA_V1 {
            return Err("unsupported BE14f decision trace schema".into());
        }
        if self.source_schema != KERNEL_CAPABILITY_SOURCE_SCHEMA {
            return Err("BE14f decision trace source schema mismatch".into());
        }
        validate_trace_text(&self.logical_resource_id)?;
        validate_trace_text(&self.policy_contract)?;
        if self.policy_objectives.is_empty()
            || self.policy_objectives.len() > MAX_BOOLEAN_KERNEL_TRACE_OBJECTIVES
        {
            return Err("BE14f decision trace objective count is outside bounds".into());
        }
        let mut objectives = BTreeSet::new();
        for objective in &self.policy_objectives {
            validate_trace_text(objective)?;
            if !objectives.insert(objective) {
                return Err("BE14f decision trace contains duplicate objectives".into());
            }
        }
        if (self.max_age_secs == 0 && self.max_age_nanos == 0)
            || self.max_age_nanos >= 1_000_000_000
        {
            return Err("BE14f decision trace freshness bound is invalid".into());
        }
        if !matches!(
            self.screen_outcome.as_str(),
            "ready" | "no-candidate" | "insufficient-evidence"
        ) || !matches!(
            self.planner_outcome.as_str(),
            "not-run" | "selected" | "no-candidate" | "insufficient-evidence" | "unsupported"
        ) {
            return Err("BE14f decision trace contains an unknown outcome".into());
        }
        if self.candidates.is_empty() || self.candidates.len() > MAX_BOOLEAN_KERNEL_CANDIDATES {
            return Err("BE14f decision trace candidate count is outside bounds".into());
        }
        let expected_key = kernel_capability_predicate_key().to_string();
        let mut prior: Option<&str> = None;
        let mut true_ids = BTreeSet::new();
        for candidate in &self.candidates {
            validate_trace_text(&candidate.realization)?;
            validate_trace_text(&candidate.predicate_key)?;
            validate_trace_text(&candidate.reason)?;
            if candidate.predicate_key != expected_key {
                return Err("BE14f decision trace predicate key mismatch".into());
            }
            if !matches!(candidate.truth.as_str(), "true" | "false" | "unknown") {
                return Err("BE14f decision trace contains invalid truth text".into());
            }
            if prior.is_some_and(|value| value >= candidate.realization.as_str()) {
                return Err("BE14f decision trace candidates are not strictly ordered".into());
            }
            prior = Some(&candidate.realization);
            if candidate.truth == "true" {
                true_ids.insert(candidate.realization.as_str());
            }
        }
        let has_unknown = self
            .candidates
            .iter()
            .any(|candidate| candidate.truth == "unknown");
        let expected_screen = if has_unknown {
            "insufficient-evidence"
        } else if true_ids.is_empty() {
            "no-candidate"
        } else {
            "ready"
        };
        if self.screen_outcome != expected_screen {
            return Err("BE14f decision trace Boolean partition is inconsistent".into());
        }
        if let Some(selected) = &self.selected_realization {
            validate_trace_text(selected)?;
            if self.planner_outcome != "selected" || !true_ids.contains(selected.as_str()) {
                return Err("BE14f selected trace is inconsistent with Boolean eligibility".into());
            }
            if self.selection_fingerprint.is_none() || self.capability_fingerprint.is_none() {
                return Err("BE14f selected trace lacks required fingerprints".into());
            }
        } else if self.planner_outcome == "selected" || self.selection_fingerprint.is_some() {
            return Err("BE14f selected trace fields are incomplete".into());
        }
        match self.planner_outcome.as_str() {
            "not-run"
                if self.screen_outcome != "insufficient-evidence"
                    && self.capability_fingerprint.is_some() =>
            {
                return Err(
                    "BE14f trace says planner did not run despite complete admission state".into(),
                );
            }
            "selected" | "insufficient-evidence" | "unsupported"
                if self.screen_outcome != "ready" =>
            {
                return Err("BE14f planner outcome is inconsistent with Boolean admission".into());
            }
            "no-candidate" if self.screen_outcome != "no-candidate" => {
                return Err(
                    "BE14f no-candidate planner result is inconsistent with screening".into(),
                );
            }
            _ => {}
        }
        if let Some(detail) = &self.planner_detail {
            validate_trace_text(detail)?;
        }
        Ok(())
    }
}

fn validate_trace_text(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > MAX_BOOLEAN_KERNEL_TRACE_STRING_BYTES {
        return Err("BE14f decision trace string is outside bounds".into());
    }
    Ok(())
}

/// Stable BE14f predicate key.
#[must_use]
pub fn kernel_capability_predicate_key() -> PredicateKey {
    PredicateKey::new(
        KERNEL_CAPABILITY_PREDICATE_NAMESPACE,
        KERNEL_CAPABILITY_PREDICATE_NAME,
    )
    .expect("static BE14f PredicateKey is valid")
}

/// Why one candidate received its Boolean capability classification.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KernelBooleanReasonV1 {
    /// Candidate names another logical kernel resource.
    LogicalResourceMismatch,
    /// Candidate does not uphold the policy's semantic contract.
    ContractMismatch,
    /// No capability snapshot was supplied.
    MissingCapabilitySnapshot,
    /// No capture time was supplied for the snapshot.
    MissingObservationTime,
    /// Capture time is later than the evaluation time.
    FutureObservation,
    /// Capability evidence exceeded the declared freshness envelope.
    StaleObservation,
    /// Capability snapshot is internally inconsistent.
    InvalidCapabilitySnapshot,
    /// All structural and capability requirements are grounded and satisfied.
    Compatible,
    /// A grounded requirement is not satisfied.
    Incompatible { detail: String },
    /// A required optional feature was not observed.
    CapabilityUnknown { detail: String },
}

/// Auditable classification for one offered kernel realization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BooleanKernelCandidateEvidenceV1 {
    realization: RealizationIdentity,
    predicate_key: PredicateKey,
    truth: TruthValue,
    reason: KernelBooleanReasonV1,
}

impl BooleanKernelCandidateEvidenceV1 {
    /// Concrete realization classified by this evidence entry.
    #[must_use]
    pub fn realization(&self) -> &RealizationIdentity {
        &self.realization
    }

    /// Stable predicate identity used for the classification.
    #[must_use]
    pub const fn predicate_key(&self) -> &PredicateKey {
        &self.predicate_key
    }

    /// Three-valued classification.
    #[must_use]
    pub const fn truth(&self) -> TruthValue {
        self.truth
    }

    /// Explicit reason for the classification.
    #[must_use]
    pub const fn reason(&self) -> &KernelBooleanReasonV1 {
        &self.reason
    }
}

/// Result of the Boolean front-end before objective ranking.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BooleanKernelScreenOutcomeV1 {
    /// Every structurally relevant candidate is grounded; these candidates may
    /// proceed to the ordinary kernel planner.
    Ready {
        /// Realizations with a `True` capability predicate, in deterministic
        /// realization-identity order.
        survivors: Vec<RealizationIdentity>,
    },
    /// No offered candidate can satisfy the declared resource/contract/capability boundary.
    NoCandidate,
    /// At least one structurally relevant candidate lacks grounded capability
    /// evidence, so ranking is not allowed to continue.
    InsufficientEvidence {
        /// Unresolved realizations in deterministic identity order.
        unresolved: Vec<RealizationIdentity>,
    },
}

/// Complete BE14f planning-front-end report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BooleanKernelScreenReportV1 {
    source_schema: &'static str,
    numeric_units: &'static str,
    feature_unit: &'static str,
    capability_fingerprint: Option<Fingerprint>,
    max_age: Duration,
    outcome: BooleanKernelScreenOutcomeV1,
    candidates: Vec<BooleanKernelCandidateEvidenceV1>,
}

impl BooleanKernelScreenReportV1 {
    #[must_use]
    pub const fn source_schema(&self) -> &'static str {
        self.source_schema
    }

    #[must_use]
    pub const fn numeric_units(&self) -> &'static str {
        self.numeric_units
    }

    #[must_use]
    pub const fn feature_unit(&self) -> &'static str {
        self.feature_unit
    }

    #[must_use]
    pub const fn capability_fingerprint(&self) -> Option<Fingerprint> {
        self.capability_fingerprint
    }

    #[must_use]
    pub const fn max_age(&self) -> Duration {
        self.max_age
    }

    #[must_use]
    pub const fn outcome(&self) -> &BooleanKernelScreenOutcomeV1 {
        &self.outcome
    }

    #[must_use]
    pub fn candidates(&self) -> &[BooleanKernelCandidateEvidenceV1] {
        &self.candidates
    }
}

/// Combined Boolean-front-end plus ordinary numerical-planner outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BooleanKernelPlanOutcomeV1 {
    /// The Boolean front-end was fully grounded and the existing planner was
    /// run only on `True` survivors.
    Planned {
        report: BooleanKernelScreenReportV1,
        planner_outcome: SelectionOutcome,
    },
    /// Ranking did not run because capability evidence was incomplete.
    InsufficientEvidence { report: BooleanKernelScreenReportV1 },
}

/// Screen kernel candidates with explicit `True`/`False`/`Unknown` semantics.
///
/// Structural mismatches are conclusive `False` values independent of device
/// evidence. Only candidates matching the requested logical resource and
/// semantic contract can become `Unknown` because of missing/stale/invalid
/// capability observations.
///
/// # Errors
///
/// Returns an error when the caller supplies an empty or over-sized candidate
/// set, a zero freshness bound, or duplicate realization identities. Candidate
/// requirements themselves were already validated by [`KernelCandidate`].
pub fn screen_kernel_candidates(
    logical_resource_id: &LogicalResourceId,
    policy: &SelectionPolicy,
    candidates: &[KernelCandidate],
    snapshot: Option<&CapabilitySnapshot>,
    observed_at: Option<Instant>,
    now: Instant,
    max_age: Duration,
) -> Result<BooleanKernelScreenReportV1, String> {
    if candidates.is_empty() {
        return Err("BE14f requires at least one kernel candidate".into());
    }
    if candidates.len() > MAX_BOOLEAN_KERNEL_CANDIDATES {
        return Err(format!(
            "BE14f candidate count exceeds {MAX_BOOLEAN_KERNEL_CANDIDATES}"
        ));
    }
    if max_age.is_zero() {
        return Err("BE14f capability freshness bound must be non-zero".into());
    }

    let mut ordered: Vec<&KernelCandidate> = candidates.iter().collect();
    ordered.sort_by(|a, b| a.realization().cmp(b.realization()));
    for pair in ordered.windows(2) {
        if pair[0].realization() == pair[1].realization() {
            return Err(format!(
                "duplicate BE14f realization identity {:?}",
                pair[0].realization().as_str()
            ));
        }
    }

    let snapshot_state = capability_snapshot_state(snapshot, observed_at, now, max_age);
    let capability_fingerprint = snapshot_state
        .as_ref()
        .ok()
        .map(|snapshot| snapshot.fingerprint());
    let key = kernel_capability_predicate_key();
    let mut evidence = Vec::with_capacity(ordered.len());

    for candidate in ordered {
        let (truth, reason) = if candidate.logical_resource_id() != logical_resource_id {
            (
                TruthValue::False,
                KernelBooleanReasonV1::LogicalResourceMismatch,
            )
        } else if candidate.contract() != policy.contract() {
            (TruthValue::False, KernelBooleanReasonV1::ContractMismatch)
        } else {
            match snapshot_state.as_ref() {
                Err(reason) => (TruthValue::Unknown, reason.clone()),
                Ok(snapshot) => match candidate.requirements().check_against(snapshot) {
                    Ok(()) => (TruthValue::True, KernelBooleanReasonV1::Compatible),
                    Err(rejection @ CapabilityRejectionReason::FeatureUnknown { .. }) => (
                        TruthValue::Unknown,
                        KernelBooleanReasonV1::CapabilityUnknown {
                            detail: rejection.to_string(),
                        },
                    ),
                    Err(rejection) => (
                        TruthValue::False,
                        KernelBooleanReasonV1::Incompatible {
                            detail: rejection.to_string(),
                        },
                    ),
                },
            }
        };
        evidence.push(BooleanKernelCandidateEvidenceV1 {
            realization: candidate.realization().clone(),
            predicate_key: key.clone(),
            truth,
            reason,
        });
    }

    let mut unresolved = Vec::new();
    let mut survivors = Vec::new();
    for entry in &evidence {
        match entry.truth {
            TruthValue::Unknown => unresolved.push(entry.realization.clone()),
            TruthValue::True => survivors.push(entry.realization.clone()),
            TruthValue::False => {}
        }
    }
    let outcome = if !unresolved.is_empty() {
        BooleanKernelScreenOutcomeV1::InsufficientEvidence { unresolved }
    } else if survivors.is_empty() {
        BooleanKernelScreenOutcomeV1::NoCandidate
    } else {
        BooleanKernelScreenOutcomeV1::Ready { survivors }
    };

    Ok(BooleanKernelScreenReportV1 {
        source_schema: KERNEL_CAPABILITY_SOURCE_SCHEMA,
        numeric_units: KERNEL_CAPABILITY_NUMERIC_UNITS,
        feature_unit: KERNEL_CAPABILITY_FEATURE_UNIT,
        capability_fingerprint,
        max_age,
        outcome,
        candidates: evidence,
    })
}

/// Run the existing deterministic kernel planner only after fail-closed Boolean
/// capability screening.
///
/// When any structurally relevant candidate is `Unknown`, this function does
/// not call the numerical planner. When the screen is fully grounded, only
/// `True` survivors are offered to [`plan`]. This preserves the ordinary
/// planner as the sole authority for objective ranking; Boolean logic only
/// rejects/prunes or blocks.
#[allow(clippy::too_many_arguments)]
pub fn plan_with_boolean_admission(
    logical_resource_id: &LogicalResourceId,
    workload_fingerprint: Fingerprint,
    policy: &SelectionPolicy,
    candidates: &[KernelCandidate],
    snapshot: Option<&CapabilitySnapshot>,
    observed_at: Option<Instant>,
    now: Instant,
    max_age: Duration,
) -> Result<BooleanKernelPlanOutcomeV1, String> {
    let report = screen_kernel_candidates(
        logical_resource_id,
        policy,
        candidates,
        snapshot,
        observed_at,
        now,
        max_age,
    )?;
    if matches!(
        report.outcome(),
        BooleanKernelScreenOutcomeV1::InsufficientEvidence { .. }
    ) {
        return Ok(BooleanKernelPlanOutcomeV1::InsufficientEvidence { report });
    }

    let survivors: Vec<KernelCandidate> = report
        .candidates()
        .iter()
        .filter(|entry| entry.truth() == TruthValue::True)
        .filter_map(|entry| {
            candidates
                .iter()
                .find(|candidate| candidate.realization() == entry.realization())
                .cloned()
        })
        .collect();
    let Some(snapshot) = snapshot else {
        // Missing snapshots make all structurally relevant candidates Unknown,
        // handled above. Reaching here means every candidate was structurally
        // false, so any internally-valid placeholder snapshot would be wrong.
        // Return the ordinary empty-plan result is impossible without inventing
        // capabilities; keep this state fail-closed instead.
        return Ok(BooleanKernelPlanOutcomeV1::InsufficientEvidence { report });
    };
    let planner_outcome = plan(
        logical_resource_id,
        workload_fingerprint,
        snapshot,
        policy,
        &survivors,
    );
    Ok(BooleanKernelPlanOutcomeV1::Planned {
        report,
        planner_outcome,
    })
}

/// Run BE14f admission and capture the exact planning result as strict durable evidence.
///
/// The returned trace is explanatory only. It contains no lifecycle authority and
/// must never be used to skip fresh capability discovery or trusted validation.
#[allow(clippy::too_many_arguments)]
pub fn plan_with_boolean_admission_traced(
    logical_resource_id: &LogicalResourceId,
    workload_fingerprint: Fingerprint,
    policy: &SelectionPolicy,
    candidates: &[KernelCandidate],
    snapshot: Option<&CapabilitySnapshot>,
    observed_at: Option<Instant>,
    now: Instant,
    max_age: Duration,
) -> Result<(BooleanKernelPlanOutcomeV1, BooleanKernelDecisionTraceV1), String> {
    let outcome = plan_with_boolean_admission(
        logical_resource_id,
        workload_fingerprint,
        policy,
        candidates,
        snapshot,
        observed_at,
        now,
        max_age,
    )?;
    let report = match &outcome {
        BooleanKernelPlanOutcomeV1::Planned { report, .. }
        | BooleanKernelPlanOutcomeV1::InsufficientEvidence { report } => report,
    };
    let screen_outcome = match report.outcome() {
        BooleanKernelScreenOutcomeV1::Ready { .. } => "ready",
        BooleanKernelScreenOutcomeV1::NoCandidate => "no-candidate",
        BooleanKernelScreenOutcomeV1::InsufficientEvidence { .. } => "insufficient-evidence",
    };
    let candidate_traces = report
        .candidates()
        .iter()
        .map(|entry| BooleanKernelCandidateTraceV1 {
            realization: entry.realization().as_str().to_owned(),
            predicate_key: entry.predicate_key().to_string(),
            truth: truth_text(entry.truth()).to_owned(),
            reason: kernel_reason_text(entry.reason()),
        })
        .collect();
    let (planner_outcome, planner_detail, selected_realization, selection_fingerprint) =
        match &outcome {
            BooleanKernelPlanOutcomeV1::InsufficientEvidence { .. } => (
                "not-run".to_owned(),
                Some("Boolean admission did not provide complete trusted evidence".to_owned()),
                None,
                None,
            ),
            BooleanKernelPlanOutcomeV1::Planned {
                planner_outcome, ..
            } => match planner_outcome {
                SelectionOutcome::Selected(record) => (
                    "selected".to_owned(),
                    None,
                    Some(record.selected_realization().as_str().to_owned()),
                    Some(record.fingerprint().bits()),
                ),
                SelectionOutcome::NoCandidate { .. } => (
                    "no-candidate".to_owned(),
                    Some("ordinary kernel planner returned no candidate".to_owned()),
                    None,
                    None,
                ),
                SelectionOutcome::InsufficientEvidence { shortfall, .. } => (
                    "insufficient-evidence".to_owned(),
                    Some(shortfall.to_string()),
                    None,
                    None,
                ),
                SelectionOutcome::Unsupported { reason } => (
                    "unsupported".to_owned(),
                    Some(reason.to_string()),
                    None,
                    None,
                ),
            },
        };
    let trace = BooleanKernelDecisionTraceV1 {
        schema_version: BOOLEAN_KERNEL_DECISION_TRACE_SCHEMA_V1,
        source_schema: report.source_schema().to_owned(),
        logical_resource_id: logical_resource_id.as_str().to_owned(),
        workload_fingerprint: workload_fingerprint.bits(),
        capability_fingerprint: report.capability_fingerprint().map(Fingerprint::bits),
        policy_contract: policy.contract().as_str().to_owned(),
        policy_objectives: policy
            .objectives()
            .iter()
            .map(|objective| objective.as_str().to_owned())
            .collect(),
        allow_static_estimates: policy.allows_static_estimates(),
        accept_uncontested_fallback: policy.accepts_uncontested_fallback(),
        max_age_secs: report.max_age().as_secs(),
        max_age_nanos: report.max_age().subsec_nanos(),
        screen_outcome: screen_outcome.to_owned(),
        planner_outcome,
        planner_detail,
        selected_realization,
        selection_fingerprint,
        candidates: candidate_traces,
    };
    trace.validate()?;
    Ok((outcome, trace))
}

fn truth_text(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
}

fn kernel_reason_text(reason: &KernelBooleanReasonV1) -> String {
    match reason {
        KernelBooleanReasonV1::LogicalResourceMismatch => "logical-resource-mismatch".into(),
        KernelBooleanReasonV1::ContractMismatch => "contract-mismatch".into(),
        KernelBooleanReasonV1::MissingCapabilitySnapshot => "missing-capability-snapshot".into(),
        KernelBooleanReasonV1::MissingObservationTime => "missing-observation-time".into(),
        KernelBooleanReasonV1::FutureObservation => "future-observation".into(),
        KernelBooleanReasonV1::StaleObservation => "stale-observation".into(),
        KernelBooleanReasonV1::InvalidCapabilitySnapshot => "invalid-capability-snapshot".into(),
        KernelBooleanReasonV1::Compatible => "compatible".into(),
        KernelBooleanReasonV1::Incompatible { detail } => format!("incompatible: {detail}"),
        KernelBooleanReasonV1::CapabilityUnknown { detail } => {
            format!("capability-unknown: {detail}")
        }
    }
}

fn capability_snapshot_state(
    snapshot: Option<&CapabilitySnapshot>,
    observed_at: Option<Instant>,
    now: Instant,
    max_age: Duration,
) -> Result<&CapabilitySnapshot, KernelBooleanReasonV1> {
    let snapshot = snapshot.ok_or(KernelBooleanReasonV1::MissingCapabilitySnapshot)?;
    let observed_at = observed_at.ok_or(KernelBooleanReasonV1::MissingObservationTime)?;
    if observed_at > now {
        return Err(KernelBooleanReasonV1::FutureObservation);
    }
    if now.duration_since(observed_at) > max_age {
        return Err(KernelBooleanReasonV1::StaleObservation);
    }
    if snapshot.validate().is_err() {
        return Err(KernelBooleanReasonV1::InvalidCapabilitySnapshot);
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::{BuiltinObjective, ContractId, ObjectiveId};

    use crate::{
        BindingLimits, Evidence, EvidenceUnit, FeatureRequirement, FeatureSupport,
        KernelRequirements, MeasuredQuantity, ObjectiveEvidence, SubgroupSupport, WorkgroupLimits,
    };

    fn logical() -> LogicalResourceId {
        LogicalResourceId::new("be14f.attention").unwrap()
    }

    fn contract() -> ContractId {
        ContractId::new("be14f.semantic-v1").unwrap()
    }

    fn latency() -> ObjectiveId {
        ObjectiveId::builtin(BuiltinObjective::Latency)
    }

    fn policy() -> SelectionPolicy {
        SelectionPolicy::new(vec![latency()], contract(), false).unwrap()
    }

    fn snapshot() -> CapabilitySnapshot {
        CapabilitySnapshot::new(CapabilitySnapshot {
            workgroup_limits: WorkgroupLimits {
                max_invocations_per_axis: [256, 256, 64],
                max_invocations_per_workgroup: 256,
                max_workgroups_per_axis: 65_535,
                max_workgroup_storage_bytes: 32_768,
            },
            binding_limits: BindingLimits {
                max_bind_groups: 8,
                max_storage_buffer_binding_bytes: 1 << 20,
            },
            subgroup_support: SubgroupSupport::unsupported(),
            shader_f16: FeatureSupport::Known(false),
            matrix_ops: FeatureSupport::Unknown,
        })
        .unwrap()
    }

    fn requirements(
        shader_f16: FeatureRequirement,
        matrix_ops: FeatureRequirement,
    ) -> KernelRequirements {
        KernelRequirements {
            invocations_per_workgroup: 64,
            invocations_per_axis: [64, 1, 1],
            workgroup_storage_bytes: 1024,
            bind_groups: 2,
            max_storage_buffer_binding_bytes: 4096,
            subgroup_min_width: None,
            shader_f16,
            matrix_ops,
        }
    }

    fn candidate(
        realization: &str,
        shader_f16: FeatureRequirement,
        matrix_ops: FeatureRequirement,
        latency_ns: u64,
    ) -> KernelCandidate {
        KernelCandidate::new(
            logical(),
            RealizationIdentity::new(realization).unwrap(),
            1,
            requirements(shader_f16, matrix_ops),
            contract(),
            ObjectiveEvidence::new().with(
                latency(),
                Evidence::Measured(MeasuredQuantity {
                    magnitude: latency_ns,
                    unit: EvidenceUnit::Nanoseconds,
                    protocol_version: 1,
                    samples: 5,
                }),
            ),
        )
        .unwrap()
    }

    #[test]
    fn grounded_capabilities_produce_true_false_and_unknown_without_guessing() {
        let now = Instant::now();
        let candidates = vec![
            candidate(
                "portable",
                FeatureRequirement::NotRequired,
                FeatureRequirement::NotRequired,
                100,
            ),
            candidate(
                "requires-f16",
                FeatureRequirement::Required,
                FeatureRequirement::NotRequired,
                80,
            ),
            candidate(
                "requires-matrix",
                FeatureRequirement::NotRequired,
                FeatureRequirement::Required,
                60,
            ),
        ];
        let report = screen_kernel_candidates(
            &logical(),
            &policy(),
            &candidates,
            Some(&snapshot()),
            Some(now),
            now,
            KERNEL_CAPABILITY_MAX_AGE,
        )
        .unwrap();
        let truths: Vec<_> = report
            .candidates()
            .iter()
            .map(|entry| (entry.realization().as_str(), entry.truth()))
            .collect();
        assert_eq!(
            truths,
            vec![
                ("portable", TruthValue::True),
                ("requires-f16", TruthValue::False),
                ("requires-matrix", TruthValue::Unknown),
            ]
        );
        assert!(matches!(
            report.outcome(),
            BooleanKernelScreenOutcomeV1::InsufficientEvidence { unresolved }
                if unresolved.iter().map(RealizationIdentity::as_str).eq(["requires-matrix"])
        ));
    }

    #[test]
    fn missing_stale_future_and_invalid_snapshots_are_unknown() {
        let now = Instant::now();
        let candidates = vec![candidate(
            "portable",
            FeatureRequirement::NotRequired,
            FeatureRequirement::NotRequired,
            100,
        )];
        let cases = [
            (None, Some(now)),
            (Some(snapshot()), None),
            (Some(snapshot()), Some(now + Duration::from_millis(1))),
            (
                Some(snapshot()),
                Some(now - KERNEL_CAPABILITY_MAX_AGE - Duration::from_millis(1)),
            ),
        ];
        for (snapshot, observed_at) in &cases {
            let report = screen_kernel_candidates(
                &logical(),
                &policy(),
                &candidates,
                snapshot.as_ref(),
                *observed_at,
                now,
                KERNEL_CAPABILITY_MAX_AGE,
            )
            .unwrap();
            assert_eq!(report.candidates()[0].truth(), TruthValue::Unknown);
        }

        let mut invalid = snapshot();
        invalid.binding_limits.max_bind_groups = 0;
        let report = screen_kernel_candidates(
            &logical(),
            &policy(),
            &candidates,
            Some(&invalid),
            Some(now),
            now,
            KERNEL_CAPABILITY_MAX_AGE,
        )
        .unwrap();
        assert_eq!(report.candidates()[0].truth(), TruthValue::Unknown);
        assert!(matches!(
            report.candidates()[0].reason(),
            KernelBooleanReasonV1::InvalidCapabilitySnapshot
        ));
    }

    #[test]
    fn unknown_candidate_blocks_numeric_ranking() {
        let now = Instant::now();
        let candidates = vec![
            candidate(
                "portable",
                FeatureRequirement::NotRequired,
                FeatureRequirement::NotRequired,
                100,
            ),
            candidate(
                "potentially-faster-matrix",
                FeatureRequirement::NotRequired,
                FeatureRequirement::Required,
                10,
            ),
        ];
        let guarded = plan_with_boolean_admission(
            &logical(),
            Fingerprint::EMPTY.text("be14f-workload"),
            &policy(),
            &candidates,
            Some(&snapshot()),
            Some(now),
            now,
            KERNEL_CAPABILITY_MAX_AGE,
        )
        .unwrap();
        assert!(matches!(
            guarded,
            BooleanKernelPlanOutcomeV1::InsufficientEvidence { .. }
        ));
    }

    #[test]
    fn grounded_boolean_pruning_preserves_unguarded_selected_realization() {
        let now = Instant::now();
        let candidates = vec![
            candidate(
                "portable",
                FeatureRequirement::NotRequired,
                FeatureRequirement::NotRequired,
                100,
            ),
            candidate(
                "known-incompatible-f16",
                FeatureRequirement::Required,
                FeatureRequirement::NotRequired,
                10,
            ),
        ];
        let capability = snapshot();
        let workload = Fingerprint::EMPTY.text("be14f-workload");
        let baseline = plan(&logical(), workload, &capability, &policy(), &candidates);
        let SelectionOutcome::Selected(baseline_record) = baseline else {
            panic!("baseline should select portable candidate");
        };

        let guarded = plan_with_boolean_admission(
            &logical(),
            workload,
            &policy(),
            &candidates,
            Some(&capability),
            Some(now),
            now,
            KERNEL_CAPABILITY_MAX_AGE,
        )
        .unwrap();
        let BooleanKernelPlanOutcomeV1::Planned {
            report,
            planner_outcome: SelectionOutcome::Selected(guarded_record),
        } = guarded
        else {
            panic!("grounded Boolean path should plan");
        };
        assert_eq!(
            baseline_record.selected_realization(),
            guarded_record.selected_realization()
        );
        assert_eq!(guarded_record.selected_realization().as_str(), "portable");
        assert!(matches!(
            report.outcome(),
            BooleanKernelScreenOutcomeV1::Ready { survivors }
                if survivors.iter().map(RealizationIdentity::as_str).eq(["portable"])
        ));
    }

    #[test]
    fn traced_selected_decision_roundtrips_and_binds_exact_context() {
        let now = Instant::now();
        let candidates = vec![
            candidate(
                "portable",
                FeatureRequirement::NotRequired,
                FeatureRequirement::NotRequired,
                100,
            ),
            candidate(
                "requires-f16",
                FeatureRequirement::Required,
                FeatureRequirement::NotRequired,
                10,
            ),
        ];
        let capability = snapshot();
        let workload = Fingerprint::EMPTY.text("be14f-traced-workload");
        let (outcome, trace) = plan_with_boolean_admission_traced(
            &logical(),
            workload,
            &policy(),
            &candidates,
            Some(&capability),
            Some(now),
            now,
            KERNEL_CAPABILITY_MAX_AGE,
        )
        .unwrap();
        assert!(matches!(
            outcome,
            BooleanKernelPlanOutcomeV1::Planned { .. }
        ));
        assert_eq!(trace.screen_outcome(), "ready");
        assert_eq!(trace.planner_outcome(), "selected");
        assert_eq!(trace.selected_realization(), Some("portable"));
        assert_eq!(
            trace
                .candidates()
                .iter()
                .map(BooleanKernelCandidateTraceV1::truth)
                .collect::<Vec<_>>(),
            vec!["true", "false"]
        );
        trace
            .validate_explanatory_context(
                &logical(),
                workload,
                Some(capability.fingerprint()),
                &policy(),
            )
            .unwrap();
        let encoded = trace.to_bounded_json().unwrap();
        let decoded = BooleanKernelDecisionTraceV1::from_bounded_json(encoded.as_bytes()).unwrap();
        assert_eq!(decoded, trace);
        assert!(decoded
            .validate_explanatory_context(
                &logical(),
                Fingerprint::EMPTY.text("different-workload"),
                Some(capability.fingerprint()),
                &policy(),
            )
            .is_err());
    }

    #[test]
    fn traced_unknown_decision_records_non_actuating_planner_stop() {
        let now = Instant::now();
        let candidates = vec![candidate(
            "requires-matrix",
            FeatureRequirement::NotRequired,
            FeatureRequirement::Required,
            10,
        )];
        let (_, trace) = plan_with_boolean_admission_traced(
            &logical(),
            Fingerprint::EMPTY.text("be14f-unknown"),
            &policy(),
            &candidates,
            Some(&snapshot()),
            Some(now),
            now,
            KERNEL_CAPABILITY_MAX_AGE,
        )
        .unwrap();
        assert_eq!(trace.screen_outcome(), "insufficient-evidence");
        assert_eq!(trace.planner_outcome(), "not-run");
        assert_eq!(trace.selected_realization(), None);
        assert_eq!(trace.candidates()[0].truth(), "unknown");
        let encoded = trace.to_bounded_json().unwrap();
        assert_eq!(
            BooleanKernelDecisionTraceV1::from_bounded_json(encoded.as_bytes()).unwrap(),
            trace
        );
    }

    #[test]
    fn trace_decoder_rejects_unknown_duplicate_future_and_oversized_inputs() {
        let now = Instant::now();
        let candidates = vec![candidate(
            "portable",
            FeatureRequirement::NotRequired,
            FeatureRequirement::NotRequired,
            100,
        )];
        let (_, trace) = plan_with_boolean_admission_traced(
            &logical(),
            Fingerprint::EMPTY.text("be14f-decoder"),
            &policy(),
            &candidates,
            Some(&snapshot()),
            Some(now),
            now,
            KERNEL_CAPABILITY_MAX_AGE,
        )
        .unwrap();
        let encoded = trace.to_bounded_json().unwrap();

        let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        value["unknown_field"] = serde_json::json!(true);
        assert!(BooleanKernelDecisionTraceV1::from_bounded_json(
            serde_json::to_string(&value).unwrap().as_bytes()
        )
        .is_err());

        let duplicate = encoded.replacen(
            "{\"schema_version\":1,",
            "{\"schema_version\":1,\"schema_version\":1,",
            1,
        );
        assert!(BooleanKernelDecisionTraceV1::from_bounded_json(duplicate.as_bytes()).is_err());

        let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        value["schema_version"] = serde_json::json!(2);
        assert!(BooleanKernelDecisionTraceV1::from_bounded_json(
            serde_json::to_string(&value).unwrap().as_bytes()
        )
        .is_err());

        let oversized = vec![b' '; MAX_BOOLEAN_KERNEL_DECISION_TRACE_BYTES + 1];
        assert!(BooleanKernelDecisionTraceV1::from_bounded_json(&oversized).is_err());
    }

    #[test]
    fn structural_mismatch_is_false_even_without_capability_observation() {
        let now = Instant::now();
        let other = KernelCandidate::new(
            LogicalResourceId::new("another-resource").unwrap(),
            RealizationIdentity::new("irrelevant").unwrap(),
            1,
            requirements(
                FeatureRequirement::NotRequired,
                FeatureRequirement::NotRequired,
            ),
            contract(),
            ObjectiveEvidence::new(),
        )
        .unwrap();
        let report = screen_kernel_candidates(
            &logical(),
            &policy(),
            &[other],
            None,
            None,
            now,
            KERNEL_CAPABILITY_MAX_AGE,
        )
        .unwrap();
        assert_eq!(report.candidates()[0].truth(), TruthValue::False);
        assert!(matches!(
            report.outcome(),
            BooleanKernelScreenOutcomeV1::NoCandidate
        ));
    }
}
