//! Deterministic, objective-ordered selection among kernel candidates.
//!
//! The planner is the first honest planner on the Elastic surface. Its
//! contract:
//!
//! - candidates are supplied by domain adapters; the planner invents nothing
//!   about any specific kernel family;
//! - capability-infeasible and contract-incompatible candidates are rejected
//!   with auditable reasons;
//! - remaining candidates are compared lexicographically along the declared
//!   objective priority order (the only cross-objective structure the core
//!   model defines). There is no scalarization and no universal conversion
//!   factor between objectives;
//! - measured evidence dominates static estimates: when any admissible
//!   candidate carries a measurement for an objective, that objective's
//!   comparison runs only over measured candidates;
//! - when evidence cannot decide, the outcome says so
//!   ([`SelectionOutcome::InsufficientEvidence`]) instead of guessing;
//! - every selection produces an auditable [`SelectionRecord`] whose
//!   fingerprint is deterministic for deterministic inputs.
//!
//! The outcome set follows the honesty philosophy of the surface model:
//! `{selected, no candidate, insufficient evidence, unsupported}`.

use std::fmt;

use elastic_core::{BuiltinObjective, ContractId, LogicalResourceId, ObjectiveId};
use elastic_eir::Fingerprint;

use crate::candidate::{
    Evidence, EvidenceTier, EvidenceUnit, KernelCandidate, MeasuredQuantity, RealizationIdentity,
    StaticQuantity,
};
use crate::capability::CapabilitySnapshot;
use crate::requirements::RejectionReason;

/// Version of this planner's decision procedure.
///
/// Bumped whenever ranking or filtering semantics change in a way that could
/// alter a selection for identical inputs. Recorded inside every
/// [`SelectionRecord`].
pub const PLANNER_VERSION: u32 = 1;

/// Canonical namespace tag for selection-record fingerprints.
pub(crate) const SELECTION_FINGERPRINT_DOMAIN: &str = "elastic-kernel/selection/v1";

/// Objective priority order with an optional semantic contract gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionPolicy {
    objectives: Vec<ObjectiveId>,
    contract: ContractId,
    /// When `false`, candidates whose top-objective evidence is merely
    /// static cannot be selected; the planner returns
    /// [`SelectionOutcome::InsufficientEvidence`] unless measured evidence
    /// exists.
    allow_static_estimates: bool,
}

impl SelectionPolicy {
    /// Assemble and validate a policy.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::NoObjectives`] when the objective list is
    /// empty and [`PolicyError::DuplicateObjective`] when one objective
    /// appears twice. Custom objectives are accepted at construction but
    /// make the plan [`SelectionOutcome::Unsupported`] because this planner
    /// version defines no comparison direction for them.
    pub fn new(
        objectives: Vec<ObjectiveId>,
        contract: ContractId,
        allow_static_estimates: bool,
    ) -> Result<Self, PolicyError> {
        if objectives.is_empty() {
            return Err(PolicyError::NoObjectives);
        }
        let mut seen = std::collections::BTreeSet::new();
        for objective in &objectives {
            if !seen.insert(objective.clone()) {
                return Err(PolicyError::DuplicateObjective {
                    objective: objective.as_str().to_string(),
                });
            }
        }
        Ok(Self {
            objectives,
            contract,
            allow_static_estimates,
        })
    }

    /// Priority-ordered objectives (first entry is highest priority).
    #[must_use]
    pub fn objectives(&self) -> &[ObjectiveId] {
        &self.objectives
    }

    /// Semantic contract selected realizations must uphold.
    #[must_use]
    pub const fn contract(&self) -> &ContractId {
        &self.contract
    }

    /// Whether static estimates may decide a selection.
    #[must_use]
    pub const fn allows_static_estimates(&self) -> bool {
        self.allow_static_estimates
    }
}

/// Errors produced while assembling selection policies.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicyError {
    /// No objective was declared; there would be nothing to rank by.
    NoObjectives,
    /// One objective appeared more than once.
    DuplicateObjective {
        /// Duplicated canonical objective text.
        objective: String,
    },
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoObjectives => write!(f, "selection policy requires at least one objective"),
            Self::DuplicateObjective { objective } => {
                write!(f, "objective `{objective}` appears twice in the policy")
            }
        }
    }
}

impl std::error::Error for PolicyError {}

/// Comparison direction implied by a built-in objective.
///
/// Returns `None` for custom objectives: this planner version refuses to
/// guess what an unknown objective means rather than assuming a direction.
#[must_use]
fn objective_direction(objective: &ObjectiveId) -> Option<Direction> {
    match objective.builtin_part() {
        Some(BuiltinObjective::Latency)
        | Some(BuiltinObjective::MemoryFootprint)
        | Some(BuiltinObjective::Energy)
        | Some(BuiltinObjective::MigrationCost) => Some(Direction::LowerIsBetter),
        Some(BuiltinObjective::Throughput) | Some(BuiltinObjective::Stability) => {
            Some(Direction::HigherIsBetter)
        }
        _ => None,
    }
}

/// Direction in which smaller ranks are better after normalization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    LowerIsBetter,
    HigherIsBetter,
}

/// One rejected candidate plus its reason, kept in selection evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateRejection {
    realization: RealizationIdentity,
    reason: RejectedReason,
}

impl CandidateRejection {
    /// Realization identity that was rejected.
    #[must_use]
    pub fn realization(&self) -> &RealizationIdentity {
        &self.realization
    }

    /// Why it was rejected.
    #[must_use]
    pub const fn reason(&self) -> &RejectedReason {
        &self.reason
    }
}

/// Why a candidate was not selectable.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RejectedReason {
    /// Capability requirements failed against the snapshot.
    Infeasible(RejectionReason),
    /// The candidate upholds a different semantic contract than required.
    ContractMismatch {
        /// Required contract text.
        expected: String,
        /// Candidate contract text.
        actual: String,
    },
}

impl fmt::Display for RejectedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Infeasible(reason) => write!(f, "capability infeasible: {reason}"),
            Self::ContractMismatch { expected, actual } => write!(
                f,
                "upholds contract `{actual}` but policy requires `{expected}`"
            ),
        }
    }
}

impl fmt::Display for CandidateRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "candidate `{}`: {}", self.realization, self.reason)
    }
}

/// What prevented a selection even though admissible candidates existed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvidenceShortfall {
    /// Every admissible candidate lacks comparable evidence for the named
    /// objective.
    AllUnknown {
        /// Blocking objective.
        objective: String,
    },
    /// Comparable evidence exists but only as static estimates, and the
    /// policy forbids selecting on estimates.
    OnlyStaticDisallowed {
        /// Blocking objective.
        objective: String,
    },
    /// Admissible candidates disagree on the physical unit of the blocking
    /// objective; their magnitudes are incomparable by construction.
    UnitMismatch {
        /// Blocking objective.
        objective: String,
        /// Units observed across the tier being compared.
        observed_units: Vec<String>,
    },
}

impl fmt::Display for EvidenceShortfall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllUnknown { objective } => write!(
                f,
                "no admissible candidate carries evidence for objective `{objective}`"
            ),
            Self::OnlyStaticDisallowed { objective } => write!(
                f,
                "objective `{objective}` has only static estimates and the policy forbids selecting on them"
            ),
            Self::UnitMismatch {
                objective,
                observed_units,
            } => write!(
                f,
                "objective `{objective}` mixes units ({}) across candidates; magnitudes are incomparable",
                observed_units.join(", ")
            ),
        }
    }
}

/// Why planning refused to run at all.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnsupportedReason {
    /// The policy contains an objective this planner version cannot direct.
    ///
    /// Extending support means defining the direction and unit discipline
    /// for that objective first, not assuming one.
    UnknownObjectiveDirection {
        /// First offending canonical objective text.
        objective: String,
    },
}

impl fmt::Display for UnsupportedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownObjectiveDirection { objective } => write!(
                f,
                "planner v{PLANNER_VERSION} defines no comparison direction for objective `{objective}`"
            ),
        }
    }
}

/// Honest planner outcomes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectionOutcome {
    /// A candidate was selected; see [`SelectionRecord`].
    Selected(Box<SelectionRecord>),
    /// No candidate was offered, or none survived filtering. Rejections are
    /// listed deterministically.
    NoCandidate {
        /// Number of offered candidates.
        offered: usize,
        /// Per-candidate rejection reasons, sorted by realization identity.
        rejections: Vec<CandidateRejection>,
    },
    /// Admissible candidates exist but evidence cannot honestly decide.
    InsufficientEvidence {
        /// Admissible realization identities, sorted.
        admissible: Vec<RealizationIdentity>,
        /// What blocked comparison.
        shortfall: EvidenceShortfall,
    },
    /// Planning itself refused to run for a stated reason.
    Unsupported {
        /// Why planning refused.
        reason: UnsupportedReason,
    },
}

/// The decisive evidence summary embedded in a selection record.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecisiveEvidence {
    /// Selection decided by hardware measurements under protocol.
    Measured {
        /// Winning quantity.
        quantity: MeasuredQuantity,
    },
    /// Selection decided by proven static estimates.
    StaticEstimate {
        /// Winning quantity.
        quantity: StaticQuantity,
    },
}

/// Auditable record of one successful selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionRecord {
    logical_resource_id: LogicalResourceId,
    workload_fingerprint: Fingerprint,
    capability_fingerprint: Fingerprint,
    candidate_set_fingerprint: Fingerprint,
    selected_realization: RealizationIdentity,
    selected_schema_version: u32,
    selected_contract: ContractId,
    rejected: Vec<CandidateRejection>,
    objectives: Vec<ObjectiveId>,
    decisive_evidence: Option<DecisiveEvidence>,
    planner_version: u32,
    fingerprint: Fingerprint,
}

impl SelectionRecord {
    /// Logical resource that was planned for.
    #[must_use]
    pub fn logical_resource_id(&self) -> &LogicalResourceId {
        &self.logical_resource_id
    }

    /// Caller-supplied workload fingerprint.
    #[must_use]
    pub const fn workload_fingerprint(&self) -> Fingerprint {
        self.workload_fingerprint
    }

    /// Capability snapshot fingerprint used during planning.
    #[must_use]
    pub const fn capability_fingerprint(&self) -> Fingerprint {
        self.capability_fingerprint
    }

    /// Fingerprint over the ordered-normalized candidate set.
    #[must_use]
    pub const fn candidate_set_fingerprint(&self) -> Fingerprint {
        self.candidate_set_fingerprint
    }

    /// Selected realization identity.
    #[must_use]
    pub fn selected_realization(&self) -> &RealizationIdentity {
        &self.selected_realization
    }

    /// Schema version of the selected realization description.
    #[must_use]
    pub const fn selected_schema_version(&self) -> u32 {
        self.selected_schema_version
    }

    /// Contract upheld by the selection.
    #[must_use]
    pub const fn selected_contract(&self) -> &ContractId {
        &self.selected_contract
    }

    /// Deterministically sorted rejections of all other candidates.
    #[must_use]
    pub fn rejected(&self) -> &[CandidateRejection] {
        &self.rejected
    }

    /// Objective priority order used for the decision.
    #[must_use]
    pub fn objectives(&self) -> &[ObjectiveId] {
        &self.objectives
    }

    /// Evidence class that decided the comparison, when comparable evidence
    /// existed.
    #[must_use]
    pub const fn decisive_evidence(&self) -> Option<&DecisiveEvidence> {
        self.decisive_evidence.as_ref()
    }

    /// [`PLANNER_VERSION`] of the deciding procedure.
    #[must_use]
    pub const fn planner_version(&self) -> u32 {
        self.planner_version
    }

    /// Structural fingerprint over the entire record.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Plan one logical kernel resource onto one capability snapshot.
///
/// # Determinism
///
/// Identical `(candidates, snapshot, policy, workload_fingerprint)` inputs —
/// in any offer order — always produce identical outcomes and identical
/// selection fingerprints.
#[allow(clippy::too_many_lines)]
pub fn plan(
    logical_resource_id: &LogicalResourceId,
    workload_fingerprint: Fingerprint,
    snapshot: &CapabilitySnapshot,
    policy: &SelectionPolicy,
    candidates: &[KernelCandidate],
) -> SelectionOutcome {
    // Refuse to run when a policy objective has no defined direction.
    for objective in policy.objectives() {
        if objective_direction(objective).is_none() {
            return SelectionOutcome::Unsupported {
                reason: UnsupportedReason::UnknownObjectiveDirection {
                    objective: objective.as_str().to_string(),
                },
            };
        }
    }

    let mut admitted: Vec<&KernelCandidate> = Vec::new();
    let mut rejections: Vec<CandidateRejection> = Vec::new();
    for candidate in candidates {
        if *candidate.logical_resource_id() != *logical_resource_id {
            rejections.push(CandidateRejection {
                realization: candidate.realization().clone(),
                reason: RejectedReason::ContractMismatch {
                    expected: format!("logical resource `{}`", logical_resource_id.as_str()),
                    actual: format!("logical resource `{}`", candidate.logical_resource_id()),
                },
            });
            continue;
        }
        if *candidate.contract() != *policy.contract() {
            rejections.push(CandidateRejection {
                realization: candidate.realization().clone(),
                reason: RejectedReason::ContractMismatch {
                    expected: policy.contract().as_str().to_string(),
                    actual: candidate.contract().as_str().to_string(),
                },
            });
            continue;
        }
        match candidate.requirements().check_against(snapshot) {
            Ok(()) => admitted.push(candidate),
            Err(reason) => rejections.push(CandidateRejection {
                realization: candidate.realization().clone(),
                reason: RejectedReason::Infeasible(reason),
            }),
        }
    }
    rejections.sort_by(|a, b| a.realization.cmp(&b.realization));

    if admitted.is_empty() {
        return SelectionOutcome::NoCandidate {
            offered: candidates.len(),
            rejections,
        };
    }

    // Lexicographic comparison along the policy's objective order.
    admitted.sort_by(|a, b| compare_candidates(a, b, policy));

    let winner = admitted[0];

    // Determine the decisive-evidence class for the primary objective and
    // enforce honesty rules before declaring success.
    let primary = &policy.objectives()[0];
    let winner_tier = winner.evidence().get(primary).tier();
    match winner_tier {
        EvidenceTier::Unknown => {
            return insufficient(
                admitted.iter().map(|c| c.realization().clone()).collect(),
                EvidenceShortfall::AllUnknown {
                    objective: primary.as_str().to_string(),
                },
            );
        }
        EvidenceTier::Static if !policy.allow_static_estimates => {
            return insufficient(
                admitted.iter().map(|c| c.realization().clone()).collect(),
                EvidenceShortfall::OnlyStaticDisallowed {
                    objective: primary.as_str().to_string(),
                },
            );
        }
        EvidenceTier::Measured | EvidenceTier::Static => {}
    }

    // Candidates sharing the winner's evidence tier form a contiguous prefix
    // of the sorted admission list because tier ordering dominates every
    // magnitude comparison.
    let mut units: Vec<EvidenceUnit> = admitted
        .iter()
        .map(|candidate| candidate.evidence().get(primary))
        .take_while(|evidence| evidence.tier() == winner_tier)
        .filter_map(|evidence| evidence.quantity())
        .map(|(_, unit)| unit)
        .collect();
    units.sort();
    units.dedup();
    if units.len() > 1 {
        return insufficient(
            admitted.iter().map(|c| c.realization().clone()).collect(),
            EvidenceShortfall::UnitMismatch {
                objective: primary.as_str().to_string(),
                observed_units: units
                    .iter()
                    .map(|unit| unit.canonical().to_string())
                    .collect(),
            },
        );
    }

    let decisive_evidence = match winner.evidence().get(primary) {
        Evidence::Measured(quantity) => DecisiveEvidence::Measured { quantity },
        Evidence::StaticEstimate(quantity) => DecisiveEvidence::StaticEstimate { quantity },
        Evidence::Unknown => unreachable!("tier check above excludes unknown"),
    };

    let candidate_set_fingerprint = candidate_set_fingerprint(candidates);
    let record = SelectionRecord {
        logical_resource_id: logical_resource_id.clone(),
        workload_fingerprint,
        capability_fingerprint: snapshot.fingerprint(),
        candidate_set_fingerprint,
        selected_realization: winner.realization().clone(),
        selected_schema_version: winner.schema_version(),
        selected_contract: winner.contract().clone(),
        rejected: rejections,
        objectives: policy.objectives().to_vec(),
        decisive_evidence: Some(decisive_evidence),
        planner_version: PLANNER_VERSION,
        fingerprint: Fingerprint::EMPTY,
    };
    let fingerprint = record_fingerprint(&record);
    let mut record = record;
    record.fingerprint = fingerprint;
    SelectionOutcome::Selected(Box::new(record))
}

#[allow(clippy::needless_pass_by_value)]
fn insufficient(
    mut admissible: Vec<RealizationIdentity>,
    shortfall: EvidenceShortfall,
) -> SelectionOutcome {
    admissible.sort();
    admissible.dedup();
    SelectionOutcome::InsufficientEvidence {
        admissible,
        shortfall,
    }
}

/// Lexicographic candidate comparison along the policy objectives.
fn compare_candidates(
    a: &KernelCandidate,
    b: &KernelCandidate,
    policy: &SelectionPolicy,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    for objective in policy.objectives() {
        let Some(direction) = objective_direction(objective) else {
            continue;
        };
        let evidence_a = a.evidence().get(objective);
        let evidence_b = b.evidence().get(objective);
        let ordering = match (evidence_a.tier(), evidence_b.tier()) {
            // Higher-trust tiers dominate lower tiers regardless of
            // magnitude; the tier enum orders unknown < static < measured,
            // so a descending comparison puts the more trusted candidate
            // first.
            (tier_a, tier_b) if tier_a != tier_b => tier_b.cmp(&tier_a),
            _ => match (evidence_a.quantity(), evidence_b.quantity()) {
                (Some((value_a, _)), Some((value_b, _))) => {
                    let forward = value_a.cmp(&value_b);
                    match direction {
                        Direction::LowerIsBetter => forward,
                        Direction::HigherIsBetter => forward.reverse(),
                    }
                }
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    a.realization().cmp(b.realization())
}

/// Fingerprint over the normalized (identity-sorted) candidate set, so offer
/// order never changes the key.
#[must_use]
pub fn candidate_set_fingerprint(candidates: &[KernelCandidate]) -> Fingerprint {
    let mut fingerprints: Vec<u64> = candidates
        .iter()
        .map(KernelCandidate::fingerprint)
        .map(Fingerprint::bits)
        .collect();
    fingerprints.sort_unstable();
    let mut fp = Fingerprint::EMPTY.text("elastic-kernel/candidate-set/v1");
    fp = fp.number(fingerprints.len() as u64);
    for bits in fingerprints {
        fp = fp.number(bits);
    }
    fp
}

/// Fingerprint over the complete selection record content.
fn record_fingerprint(record: &SelectionRecord) -> Fingerprint {
    let mut fp = Fingerprint::EMPTY
        .text(SELECTION_FINGERPRINT_DOMAIN)
        .number(u64::from(record.planner_version));
    fp = fp.text(record.logical_resource_id.as_str());
    fp = fp.number(record.workload_fingerprint.bits());
    fp = fp.number(record.capability_fingerprint.bits());
    fp = fp.number(record.candidate_set_fingerprint.bits());
    fp = fp.text(record.selected_realization.as_str());
    fp = fp.number(u64::from(record.selected_schema_version));
    fp = fp.text(record.selected_contract.as_str());
    fp = fp.number(record.rejected.len() as u64);
    for rejection in &record.rejected {
        fp = fp.text(rejection.realization.as_str());
        fp = fp.text(rejection.reason.to_string().as_str());
    }
    fp = fp.number(record.objectives.len() as u64);
    for objective in &record.objectives {
        fp = fp.text(objective.as_str());
    }
    fp = match &record.decisive_evidence {
        Some(DecisiveEvidence::Measured { quantity }) => fp
            .text("measured")
            .number(quantity.magnitude)
            .text(quantity.unit.canonical())
            .number(u64::from(quantity.protocol_version))
            .number(u64::from(quantity.samples)),
        Some(DecisiveEvidence::StaticEstimate { quantity }) => fp
            .text("static")
            .number(quantity.magnitude)
            .text(quantity.unit.canonical()),
        None => fp.text("none"),
    };
    fp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::{ObjectiveEvidence, StaticQuantity};
    use crate::capability::{BindingLimits, FeatureSupport, SubgroupSupport, WorkgroupLimits};

    fn latency() -> ObjectiveId {
        ObjectiveId::builtin(BuiltinObjective::Latency)
    }

    fn memory() -> ObjectiveId {
        ObjectiveId::builtin(BuiltinObjective::MemoryFootprint)
    }

    fn throughput() -> ObjectiveId {
        ObjectiveId::builtin(BuiltinObjective::Throughput)
    }

    fn resource() -> LogicalResourceId {
        LogicalResourceId::new("attention#42").expect("valid")
    }

    fn contract_a() -> ContractId {
        ContractId::new("attention-forward-v1").expect("valid")
    }

    fn contract_b() -> ContractId {
        ContractId::new("attention-forward-v2-experimental").expect("valid")
    }

    fn portable_snapshot() -> CapabilitySnapshot {
        CapabilitySnapshot {
            workgroup_limits: WorkgroupLimits {
                max_invocations_per_axis: [1024, 1024, 64],
                max_invocations_per_workgroup: 1024,
                max_workgroups_per_axis: 65535,
                max_workgroup_storage_bytes: 48 << 10,
            },
            binding_limits: BindingLimits {
                max_bind_groups: 8,
                max_storage_buffer_binding_bytes: 128 << 20,
            },
            subgroup_support: SubgroupSupport::unsupported(),
            shader_f16: FeatureSupport::Known(false),
            matrix_ops: FeatureSupport::Unknown,
        }
    }

    fn subgroup_snapshot() -> CapabilitySnapshot {
        let mut snapshot = portable_snapshot();
        snapshot.subgroup_support = SubgroupSupport::supported(4, 64).expect("valid");
        snapshot
    }

    fn requirements(subgroups: bool) -> crate::requirements::KernelRequirements {
        crate::requirements::KernelRequirements {
            invocations_per_workgroup: 64,
            invocations_per_axis: [64, 1, 1],
            workgroup_storage_bytes: 24_576,
            bind_groups: 2,
            max_storage_buffer_binding_bytes: 4096,
            subgroup_min_width: subgroups.then_some(4),
            shader_f16: crate::requirements::FeatureRequirement::NotRequired,
            matrix_ops: crate::requirements::FeatureRequirement::NotRequired,
        }
    }

    fn candidate(
        realization: &str,
        needs_subgroup: bool,
        evidence: ObjectiveEvidence,
    ) -> KernelCandidate {
        KernelCandidate::new(
            resource(),
            RealizationIdentity::new(realization).expect("valid"),
            1,
            requirements(needs_subgroup),
            contract_a(),
            evidence,
        )
        .expect("fixture valid")
    }

    fn static_latency(nanoseconds: u64) -> ObjectiveEvidence {
        ObjectiveEvidence::new().with(
            latency(),
            Evidence::StaticEstimate(StaticQuantity {
                magnitude: nanoseconds,
                unit: EvidenceUnit::Nanoseconds,
            }),
        )
    }

    fn measured_latency(nanoseconds: u64) -> ObjectiveEvidence {
        ObjectiveEvidence::new().with(
            latency(),
            Evidence::Measured(MeasuredQuantity {
                magnitude: nanoseconds,
                unit: EvidenceUnit::Nanoseconds,
                protocol_version: 1,
                samples: 30,
            }),
        )
    }

    fn policy(objectives: Vec<ObjectiveId>) -> SelectionPolicy {
        SelectionPolicy::new(objectives, contract_a(), true).expect("valid policy")
    }

    fn workload() -> Fingerprint {
        Fingerprint::EMPTY.text("workload/b=1/h=8/n=128/d=64/causal=true")
    }

    #[test]
    fn portable_boundary_selects_portable_and_rejects_subgroup_candidate() {
        let candidates = vec![
            candidate("subgroup-q4", true, static_latency(50)),
            candidate("portable-q4", false, static_latency(100)),
        ];
        let outcome = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency()]),
            &candidates,
        );
        let SelectionOutcome::Selected(record) = outcome else {
            panic!("expected selection, got {outcome:?}");
        };
        assert_eq!(record.selected_realization().as_str(), "portable-q4");
        assert_eq!(record.rejected().len(), 1);
        assert_eq!(
            *record.rejected()[0].reason(),
            RejectedReason::Infeasible(RejectionReason::SubgroupUnsupported)
        );
    }

    #[test]
    fn subgroup_boundary_admits_both_and_prefers_the_faster_estimate() {
        let candidates = vec![
            candidate("portable-q4", false, static_latency(100)),
            candidate("subgroup-q4", true, static_latency(50)),
        ];
        let outcome = plan(
            &resource(),
            workload(),
            &subgroup_snapshot(),
            &policy(vec![latency()]),
            &candidates,
        );
        let SelectionOutcome::Selected(record) = outcome else {
            panic!("expected selection, got {outcome:?}");
        };
        assert_eq!(record.selected_realization().as_str(), "subgroup-q4");
        assert!(record.rejected().is_empty());
    }

    #[test]
    fn offer_order_never_changes_the_outcome_or_fingerprints() {
        let first = vec![
            candidate("portable-q4", false, static_latency(100)),
            candidate("subgroup-q4", true, static_latency(50)),
        ];
        let mut second = first.clone();
        second.reverse();

        let snapshot = subgroup_snapshot();
        let policy = policy(vec![latency()]);
        let a = plan(&resource(), workload(), &snapshot, &policy, &first);
        let b = plan(&resource(), workload(), &snapshot, &policy, &second);
        assert_eq!(a, b);
    }

    #[test]
    fn objective_priority_can_legally_flip_the_selection() {
        let small_slow = {
            let mut evidence = ObjectiveEvidence::new();
            evidence.attach(
                latency(),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 200,
                    unit: EvidenceUnit::Nanoseconds,
                }),
            );
            evidence.attach(
                memory(),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 1024,
                    unit: EvidenceUnit::Bytes,
                }),
            );
            candidate("small-slow", false, evidence)
        };
        let large_fast = {
            let mut evidence = ObjectiveEvidence::new();
            evidence.attach(
                latency(),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 80,
                    unit: EvidenceUnit::Nanoseconds,
                }),
            );
            evidence.attach(
                memory(),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 4096,
                    unit: EvidenceUnit::Bytes,
                }),
            );
            candidate("large-fast", false, evidence)
        };

        let latency_first = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency(), memory()]),
            &[small_slow.clone(), large_fast.clone()],
        );
        let SelectionOutcome::Selected(record) = latency_first else {
            panic!("expected selection");
        };
        assert_eq!(record.selected_realization().as_str(), "large-fast");

        let memory_first = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![memory(), latency()]),
            &[small_slow, large_fast],
        );
        let SelectionOutcome::Selected(record) = memory_first else {
            panic!("expected selection");
        };
        assert_eq!(record.selected_realization().as_str(), "small-slow");
    }

    #[test]
    fn contract_mismatch_is_an_explicit_rejection() {
        let experimental = KernelCandidate::new(
            resource(),
            RealizationIdentity::new("experimental-kernel").expect("valid"),
            1,
            requirements(false),
            contract_b(),
            static_latency(10),
        )
        .expect("valid");
        let outcome = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency()]),
            &[experimental],
        );
        let SelectionOutcome::NoCandidate {
            offered,
            rejections,
        } = outcome
        else {
            panic!("expected no-candidate, got {outcome:?}");
        };
        assert_eq!(offered, 1);
        assert!(matches!(
            rejections[0].reason(),
            RejectedReason::ContractMismatch { .. }
        ));
    }

    #[test]
    fn foreign_logical_resources_are_rejected_not_silently_dropped() {
        let other = KernelCandidate::new(
            LogicalResourceId::new("attention#43").expect("valid"),
            RealizationIdentity::new("other-resource-kernel").expect("valid"),
            1,
            requirements(false),
            contract_a(),
            static_latency(10),
        )
        .expect("valid");
        let outcome = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency()]),
            &[other],
        );
        let SelectionOutcome::NoCandidate { rejections, .. } = outcome else {
            panic!("expected no-candidate");
        };
        assert!(matches!(
            rejections[0].reason(),
            RejectedReason::ContractMismatch { .. }
        ));
    }

    #[test]
    fn all_unknown_primary_evidence_yields_insufficient_evidence() {
        let candidates = vec![candidate("opaque-kernel", false, ObjectiveEvidence::new())];
        let outcome = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency()]),
            &candidates,
        );
        let SelectionOutcome::InsufficientEvidence {
            admissible,
            shortfall,
        } = outcome
        else {
            panic!("expected insufficient evidence, got {outcome:?}");
        };
        assert_eq!(admissible.len(), 1);
        assert_eq!(
            shortfall,
            EvidenceShortfall::AllUnknown {
                objective: "latency".to_string(),
            }
        );
    }

    #[test]
    fn static_only_selection_requires_policy_permission() {
        let candidates = vec![candidate("static-only", false, static_latency(75))];
        let permissive = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency()]),
            &candidates,
        );
        assert!(matches!(permissive, SelectionOutcome::Selected(_)));

        let strict_policy =
            SelectionPolicy::new(vec![latency()], contract_a(), false).expect("valid");
        let strict = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &strict_policy,
            &candidates,
        );
        let SelectionOutcome::InsufficientEvidence { shortfall, .. } = strict else {
            panic!("expected insufficient evidence");
        };
        assert_eq!(
            shortfall,
            EvidenceShortfall::OnlyStaticDisallowed {
                objective: "latency".to_string(),
            }
        );
    }

    #[test]
    fn measured_evidence_dominates_static_even_when_static_looks_better() {
        let candidates = vec![
            candidate("static-fast", false, static_latency(10)),
            candidate("measured-slow", false, measured_latency(500)),
        ];
        let outcome = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency()]),
            &candidates,
        );
        let SelectionOutcome::Selected(record) = outcome else {
            panic!("expected selection");
        };
        // The measured candidate wins the tier comparison: mixing a guess
        // with a measurement would fabricate comparability.
        assert_eq!(record.selected_realization().as_str(), "measured-slow");
        assert!(matches!(
            record.decisive_evidence(),
            Some(DecisiveEvidence::Measured { .. })
        ));
    }

    #[test]
    fn mixed_measured_units_block_selection_instead_of_aliasing() {
        let nanoseconds = candidate("in-nanoseconds", false, measured_latency(100));
        let microseconds = candidate("in-microseconds", false, {
            ObjectiveEvidence::new().with(
                latency(),
                Evidence::Measured(MeasuredQuantity {
                    magnitude: 1,
                    unit: EvidenceUnit::Dimensionless,
                    protocol_version: 1,
                    samples: 5,
                }),
            )
        });
        let outcome = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency()]),
            &[nanoseconds, microseconds],
        );
        let SelectionOutcome::InsufficientEvidence {
            shortfall,
            admissible,
        } = outcome
        else {
            panic!("expected insufficient evidence, got {outcome:?}");
        };
        assert_eq!(admissible.len(), 2);
        assert!(matches!(shortfall, EvidenceShortfall::UnitMismatch { .. }));
    }

    #[test]
    fn custom_objectives_are_unsupported_not_guessed() {
        let custom = ObjectiveId::custom("quantum-advantage").expect("valid");
        let outcome_plan = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![custom]),
            &[candidate("any", false, static_latency(1))],
        );
        let SelectionOutcome::Unsupported { reason } = outcome_plan else {
            panic!("expected unsupported");
        };
        assert!(matches!(
            reason,
            UnsupportedReason::UnknownObjectiveDirection { .. }
        ));
    }

    #[test]
    fn throughput_objective_prefers_higher_magnitudes() {
        let slower = candidate("lower-throughput", false, {
            ObjectiveEvidence::new().with(
                throughput(),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 100,
                    unit: EvidenceUnit::OperationsPerSecond,
                }),
            )
        });
        let faster = candidate("higher-throughput", false, {
            ObjectiveEvidence::new().with(
                throughput(),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 400,
                    unit: EvidenceUnit::OperationsPerSecond,
                }),
            )
        });
        let outcome = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![throughput()]),
            &[slower, faster],
        );
        let SelectionOutcome::Selected(record) = outcome else {
            panic!("expected selection");
        };
        assert_eq!(record.selected_realization().as_str(), "higher-throughput");
    }

    #[test]
    fn duplicate_objectives_are_rejected_at_policy_construction() {
        assert_eq!(
            SelectionPolicy::new(vec![latency(), latency()], contract_a(), true),
            Err(PolicyError::DuplicateObjective {
                objective: "latency".to_string(),
            })
        );
        assert_eq!(
            SelectionPolicy::new(Vec::new(), contract_a(), true),
            Err(PolicyError::NoObjectives)
        );
    }

    #[test]
    fn selection_records_are_deterministic_for_identical_inputs() {
        let candidates = vec![
            candidate("portable-q4", false, static_latency(100)),
            candidate("subgroup-q4", true, static_latency(50)),
        ];
        let snapshot = subgroup_snapshot();
        let policy = policy(vec![latency()]);
        let first = plan(&resource(), workload(), &snapshot, &policy, &candidates);
        let second = plan(&resource(), workload(), &snapshot, &policy, &candidates);
        assert_eq!(first, second);
        let SelectionOutcome::Selected(record) = first else {
            panic!("expected selection");
        };
        assert_eq!(record.planner_version(), PLANNER_VERSION);
        assert_eq!(record.capability_fingerprint(), snapshot.fingerprint());
        assert_eq!(
            record.candidate_set_fingerprint(),
            candidate_set_fingerprint(&candidates)
        );
        // The record fingerprint must cover workload identity.
        let other_workload = plan(
            &resource(),
            Fingerprint::EMPTY.text("workload/different"),
            &snapshot,
            &policy,
            &candidates,
        );
        assert_ne!(second, other_workload);
    }

    #[test]
    fn empty_offer_is_no_candidate_with_empty_rejections() {
        let outcome = plan(
            &resource(),
            workload(),
            &portable_snapshot(),
            &policy(vec![latency()]),
            &[],
        );
        assert_eq!(
            outcome,
            SelectionOutcome::NoCandidate {
                offered: 0,
                rejections: Vec::new(),
            }
        );
    }
}
