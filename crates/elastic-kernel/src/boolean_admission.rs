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

use std::time::{Duration, Instant};

use elastic_core::{LogicalResourceId, PredicateKey, TruthValue};
use elastic_eir::Fingerprint;

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
