//! Integrated, non-actuating evidence for one Boolean-gated numeric planning cycle.
//!
//! This module binds the existing durable [`DecisionTrace`] to the exact
//! numerical [`PlanningContext`], the Boolean pruning result, and the honest
//! [`PlanOutcome`] returned by the guarded planner. The capture is explanatory
//! evidence only: even a successful invariant precheck explicitly carries no
//! trusted-validation or actuation authority.

use crate::{
    DecisionTrace, DecisionTraceError, InvariantPrecheckReport, InvariantPrecheckStatus,
    MAX_EVIDENCE_BYTES,
};
use elastic_core::{GuardScope, TransitionMechanism, TruthValue};
use elastic_eir::{
    EirGuardedResource, Fingerprint, PlanOutcome, PlanningContext, TransitionCandidate,
    TransitionPruningReport,
};
use serde_json::{json, Value};

/// Schema for the integrated guarded-planning capture envelope.
pub const GUARDED_PLANNING_CAPTURE_SCHEMA_V1: &str = "elastic-guarded-planning-capture-v1";

/// Exact-bit structural identity of the numerical planning context.
///
/// This is deliberately non-cryptographic. Floating-point values are absorbed
/// by their IEEE-754 bit patterns, so signed zero and distinct NaN payloads do
/// not silently collapse to one identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanningContextFingerprint(u64);

impl PlanningContextFingerprint {
    /// Raw diagnostic bits. These bits never authorize actuation.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

/// Structural identity of the complete Boolean pruning classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PruningReportFingerprint(u64);

impl PruningReportFingerprint {
    /// Raw diagnostic bits. These bits never authenticate a pruning report.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

/// The four honest outcomes of the guarded numerical planning contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuardedPlanningOutcomeKind {
    /// A declared, capability-grounded Boolean survivor was selected.
    Candidate,
    /// Declared candidates existed but none was selectable.
    NoCandidate,
    /// The requested transition vocabulary was unsupported.
    Unsupported,
    /// Available evidence was insufficient to select safely.
    InsufficientEvidence,
}

impl GuardedPlanningOutcomeKind {
    pub(crate) const fn from_outcome(outcome: &PlanOutcome) -> Self {
        match outcome {
            PlanOutcome::Candidate(_) => Self::Candidate,
            PlanOutcome::NoCandidate => Self::NoCandidate,
            PlanOutcome::Unsupported => Self::Unsupported,
            PlanOutcome::InsufficientEvidence { .. } => Self::InsufficientEvidence,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::NoCandidate => "no-candidate",
            Self::Unsupported => "unsupported",
            Self::InsufficientEvidence => "insufficient-evidence",
        }
    }
}

/// Non-authoritative summary of an optional invariant Boolean precheck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvariantPrecheckTraceSummary {
    status: InvariantPrecheckStatus,
    applicable_invariants: usize,
    false_facts: usize,
    unknown_facts: usize,
}

impl InvariantPrecheckTraceSummary {
    fn from_report(report: &InvariantPrecheckReport) -> Self {
        Self {
            status: report.status(),
            applicable_invariants: report.entries().len(),
            false_facts: report
                .entries()
                .iter()
                .filter(|entry| entry.truth() == TruthValue::False)
                .count(),
            unknown_facts: report
                .entries()
                .iter()
                .filter(|entry| entry.truth() == TruthValue::Unknown)
                .count(),
        }
    }

    /// Aggregate precheck status.
    #[must_use]
    pub const fn status(self) -> InvariantPrecheckStatus {
        self.status
    }

    /// Number of invariants covered by the precheck report.
    #[must_use]
    pub const fn applicable_invariants(self) -> usize {
        self.applicable_invariants
    }

    /// Number of explicit false precheck facts.
    #[must_use]
    pub const fn false_facts(self) -> usize {
        self.false_facts
    }

    /// Number of unknown or missing precheck facts.
    #[must_use]
    pub const fn unknown_facts(self) -> usize {
        self.unknown_facts
    }

    /// A Boolean invariant precheck is never trusted validation authority.
    #[must_use]
    pub const fn trusted_validation_authorized(self) -> bool {
        false
    }
}

/// Explanatory evidence captured from one guarded planning cycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedPlanningCapture {
    decision_trace: DecisionTrace,
    planning_context_fingerprint: PlanningContextFingerprint,
    pruning_report_fingerprint: PruningReportFingerprint,
    outcome: GuardedPlanningOutcomeKind,
    invariant_precheck: Option<InvariantPrecheckTraceSummary>,
}

impl GuardedPlanningCapture {
    pub(crate) fn from_cycle(
        resource: &EirGuardedResource,
        context: &PlanningContext,
        report: &TransitionPruningReport,
        outcome: &PlanOutcome,
        decision_trace: DecisionTrace,
    ) -> Self {
        Self {
            decision_trace,
            planning_context_fingerprint: planning_context_fingerprint(context),
            pruning_report_fingerprint: pruning_report_fingerprint(resource, report),
            outcome: GuardedPlanningOutcomeKind::from_outcome(outcome),
            invariant_precheck: None,
        }
    }

    /// Underlying durable Boolean decision trace bound to the original guarded resource.
    #[must_use]
    pub const fn decision_trace(&self) -> &DecisionTrace {
        &self.decision_trace
    }

    /// Exact-bit structural identity of numerical observations used for ranking.
    #[must_use]
    pub const fn planning_context_fingerprint(&self) -> PlanningContextFingerprint {
        self.planning_context_fingerprint
    }

    /// Structural identity of the eligible/rejected/unknown pruning partition.
    #[must_use]
    pub const fn pruning_report_fingerprint(&self) -> PruningReportFingerprint {
        self.pruning_report_fingerprint
    }

    /// Honest guarded-planning outcome captured by the cycle.
    #[must_use]
    pub const fn outcome(&self) -> GuardedPlanningOutcomeKind {
        self.outcome
    }

    /// Optional invariant precheck summary, when the caller has run that pure precheck.
    #[must_use]
    pub const fn invariant_precheck(&self) -> Option<InvariantPrecheckTraceSummary> {
        self.invariant_precheck
    }

    /// Attach a non-authoritative invariant precheck summary.
    ///
    /// This never marks the capture as validated and never calls an adapter,
    /// validator, actuator, or planner.
    #[must_use]
    pub fn with_invariant_precheck(mut self, report: &InvariantPrecheckReport) -> Self {
        self.invariant_precheck = Some(InvariantPrecheckTraceSummary::from_report(report));
        self
    }

    /// Integrated planning evidence never grants physical actuation authority.
    #[must_use]
    pub const fn actuation_authorized(&self) -> bool {
        false
    }

    /// Encode a bounded explanatory envelope containing the durable v1 decision trace.
    ///
    /// The decoder/replay qualification for this integrated envelope belongs to
    /// the later BE8 replay/hardening slices. Encoding performs no observation,
    /// planning, validation, adapter call, or actuation.
    ///
    /// # Errors
    ///
    /// Returns [`DecisionTraceError`] if the nested trace cannot be encoded or
    /// the resulting envelope exceeds the runtime evidence byte limit.
    pub fn to_bounded_json(&self) -> Result<String, DecisionTraceError> {
        let trace_json = self.decision_trace.to_bounded_json()?;
        let trace_value: Value = serde_json::from_str(&trace_json)
            .map_err(|error| DecisionTraceError::Encoding(error.to_string()))?;
        let invariant_precheck = self.invariant_precheck.map(|summary| {
            json!({
                "status": invariant_status_text(summary.status),
                "applicable_invariants": summary.applicable_invariants,
                "false_facts": summary.false_facts,
                "unknown_facts": summary.unknown_facts,
                "trusted_validation_authorized": false,
            })
        });
        let value = json!({
            "schema": GUARDED_PLANNING_CAPTURE_SCHEMA_V1,
            "decision_trace": trace_value,
            "planning_context_fingerprint": format!("{:016x}", self.planning_context_fingerprint.bits()),
            "pruning_report_fingerprint": format!("{:016x}", self.pruning_report_fingerprint.bits()),
            "outcome": self.outcome.as_str(),
            "invariant_precheck": invariant_precheck,
            "actuation_authorized": false,
        });
        let encoded = serde_json::to_string(&value)
            .map_err(|error| DecisionTraceError::Encoding(error.to_string()))?;
        if encoded.len() > MAX_EVIDENCE_BYTES {
            return Err(DecisionTraceError::EvidenceTooLarge {
                max_bytes: MAX_EVIDENCE_BYTES,
                actual_bytes: encoded.len(),
            });
        }
        Ok(encoded)
    }
}

/// Compute a deterministic identity for the exact numerical context.
#[must_use]
pub fn planning_context_fingerprint(context: &PlanningContext) -> PlanningContextFingerprint {
    let mut fingerprint = Fingerprint::EMPTY.text("planning-context").number(1);
    let observations = context.iter().collect::<Vec<_>>();
    fingerprint = fingerprint.number(observations.len() as u64);
    for (signal, value) in observations {
        fingerprint = fingerprint.text(signal.as_str()).number(value.to_bits());
    }
    PlanningContextFingerprint(fingerprint.bits())
}

/// Compute a deterministic identity for one source-bound pruning partition.
#[must_use]
pub fn pruning_report_fingerprint(
    resource: &EirGuardedResource,
    report: &TransitionPruningReport,
) -> PruningReportFingerprint {
    let mut fingerprint = Fingerprint::EMPTY
        .text("transition-pruning-report")
        .number(1)
        .text(resource.resource().identity().as_str())
        .number(resource.fingerprint().bits())
        .number(report.eligible().len() as u64)
        .number(report.rejected().len() as u64)
        .number(report.unknown().len() as u64);

    for candidate in report.eligible() {
        fingerprint = absorb_candidate(fingerprint.text("eligible"), candidate);
    }
    for rejected in report.rejected() {
        fingerprint = absorb_candidate(fingerprint.text("rejected"), rejected.candidate())
            .text(&scope_text(rejected.failed_scope()));
    }
    for unknown in report.unknown() {
        fingerprint = absorb_candidate(fingerprint.text("unknown"), unknown.candidate())
            .number(u64::from(unknown.capability_grounded()))
            .number(unknown.unknown_scopes().len() as u64);
        for scope in unknown.unknown_scopes() {
            fingerprint = fingerprint.text(&scope_text(scope));
        }
    }
    PruningReportFingerprint(fingerprint.bits())
}

fn absorb_candidate(mut fingerprint: Fingerprint, candidate: &TransitionCandidate) -> Fingerprint {
    fingerprint = fingerprint
        .text(mechanism_text(candidate.mechanism()))
        .text(candidate.dimension().as_str())
        .number(u64::from(candidate.capability_grounded()));
    match candidate.magnitude() {
        Some(magnitude) => fingerprint.text("magnitude").number(magnitude),
        None => fingerprint.text("no-magnitude"),
    }
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

const fn invariant_status_text(status: InvariantPrecheckStatus) -> &'static str {
    match status {
        InvariantPrecheckStatus::NoCandidate => "no-candidate",
        InvariantPrecheckStatus::InvalidCandidate => "invalid-candidate",
        InvariantPrecheckStatus::Passed => "passed",
        InvariantPrecheckStatus::Rejected => "rejected",
        InvariantPrecheckStatus::InsufficientEvidence => "insufficient-evidence",
    }
}
