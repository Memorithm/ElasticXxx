//! EX-SR-2 fail-closed planning for storage-versus-recomputation candidates.
//!
//! Planning checks declared evidence against explicit limits. A successful
//! plan is still non-actuating: trusted validation immediately before an
//! action remains authoritative.

use elastic_eir::Fingerprint;
use std::fmt;

use crate::storage_recompute::{
    ReplayReconstructionV1, StorageRecomputeActionV1, StorageRecomputeCandidateV1,
};
use crate::storage_recompute_cost::{
    ByteEvidenceScopeV1, CostEvidenceBasisV1, QualityGuardEvidenceV1,
    StorageRecomputeCostVectorV1,
};

/// Stable schema identity for storage/recompute planning.
pub const STORAGE_RECOMPUTE_PLAN_V1: &str = "elastic.kv.storage-recompute-plan@1.0.0";

/// Explicit limits used to screen one declared candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageRecomputePlanLimitsV1 {
    max_resident_bytes_after: Option<u64>,
    max_transfer_bytes: Option<u64>,
    max_total_latency_ns: Option<u64>,
    allow_forecast_evidence: bool,
}

impl StorageRecomputePlanLimitsV1 {
    /// Construct a bounded planning policy.
    pub fn new(
        max_resident_bytes_after: Option<u64>,
        max_transfer_bytes: Option<u64>,
        max_total_latency_ns: Option<u64>,
        allow_forecast_evidence: bool,
    ) -> Result<Self, StorageRecomputePlanError> {
        if max_resident_bytes_after.is_none()
            && max_transfer_bytes.is_none()
            && max_total_latency_ns.is_none()
        {
            return Err(StorageRecomputePlanError::UnboundedPolicy);
        }
        Ok(Self {
            max_resident_bytes_after,
            max_transfer_bytes,
            max_total_latency_ns,
            allow_forecast_evidence,
        })
    }
}

/// A candidate that passed EX-SR-2 screening.
///
/// This record cannot authorize actuation; it only preserves the screened
/// identity and the checked aggregate latency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRecomputePlanV1 {
    candidate_fingerprint: Fingerprint,
    cost_fingerprint: Fingerprint,
    limits: StorageRecomputePlanLimitsV1,
    action: StorageRecomputeActionV1,
    total_latency_ns: u64,
    fingerprint: Fingerprint,
}

impl StorageRecomputePlanV1 {
    /// Screen an exact candidate/cost pair against explicit limits.
    pub fn screen(
        candidate: &StorageRecomputeCandidateV1,
        costs: &StorageRecomputeCostVectorV1,
        limits: StorageRecomputePlanLimitsV1,
    ) -> Result<Self, StorageRecomputePlanError> {
        if candidate.fingerprint() != costs.candidate_fingerprint() {
            return Err(StorageRecomputePlanError::CandidateFingerprintMismatch);
        }

        require_action_evidence(candidate.action(), costs)?;
        require_verified_approximate_replay(candidate, costs)?;
        reject_disallowed_forecasts(costs, limits.allow_forecast_evidence)?;
        require_physical_byte_evidence(costs, limits)?;

        if let Some(limit) = limits.max_resident_bytes_after {
            let observed = costs
                .resident_bytes_after()
                .ok_or(StorageRecomputePlanError::MissingResidentBytes)?
                .bytes();
            if observed > limit {
                return Err(StorageRecomputePlanError::ResidentBytesExceeded { observed, limit });
            }
        }

        if let Some(limit) = limits.max_transfer_bytes {
            let observed = costs.transfer_bytes().map_or(0, |value| value.bytes());
            if observed > limit {
                return Err(StorageRecomputePlanError::TransferBytesExceeded { observed, limit });
            }
        }

        if limits.max_total_latency_ns.is_some() && costs.controller_latency().is_none() {
            return Err(StorageRecomputePlanError::MissingControllerLatency);
        }

        let total_latency_ns = [
            costs.transfer_latency(),
            costs.recompute_latency(),
            costs.controller_latency(),
        ]
        .into_iter()
        .flatten()
        .try_fold(0_u64, |total, value| {
            total
                .checked_add(value.nanoseconds())
                .ok_or(StorageRecomputePlanError::LatencyOverflow)
        })?;

        if let Some(limit) = limits.max_total_latency_ns {
            if total_latency_ns > limit {
                return Err(StorageRecomputePlanError::LatencyExceeded {
                    observed: total_latency_ns,
                    limit,
                });
            }
        }

        let fingerprint = plan_fingerprint(
            candidate.fingerprint(),
            costs.fingerprint(),
            limits,
            total_latency_ns,
        );
        Ok(Self {
            candidate_fingerprint: candidate.fingerprint(),
            cost_fingerprint: costs.fingerprint(),
            limits,
            action: candidate.action(),
            total_latency_ns,
            fingerprint,
        })
    }

    #[must_use]
    pub const fn candidate_fingerprint(&self) -> Fingerprint {
        self.candidate_fingerprint
    }

    #[must_use]
    pub const fn cost_fingerprint(&self) -> Fingerprint {
        self.cost_fingerprint
    }

    #[must_use]
    pub const fn limits(&self) -> StorageRecomputePlanLimitsV1 {
        self.limits
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    #[must_use]
    pub const fn action(&self) -> StorageRecomputeActionV1 {
        self.action
    }

    #[must_use]
    pub const fn total_latency_ns(&self) -> u64 {
        self.total_latency_ns
    }

    /// Planning evidence is never actuation authority.
    #[must_use]
    pub const fn authorizes_actuation(&self) -> bool {
        false
    }
}

fn plan_fingerprint(
    candidate: Fingerprint,
    costs: Fingerprint,
    limits: StorageRecomputePlanLimitsV1,
    total_latency_ns: u64,
) -> Fingerprint {
    fn optional(value: Option<u64>, fingerprint: Fingerprint) -> Fingerprint {
        match value {
            Some(value) => fingerprint.text("some").number(value),
            None => fingerprint.text("none"),
        }
    }

    let fingerprint = Fingerprint::EMPTY
        .text(STORAGE_RECOMPUTE_PLAN_V1)
        .number(candidate.bits())
        .number(costs.bits());
    let fingerprint = optional(limits.max_resident_bytes_after, fingerprint);
    let fingerprint = optional(limits.max_transfer_bytes, fingerprint);
    let fingerprint = optional(limits.max_total_latency_ns, fingerprint);
    fingerprint
        .number(u64::from(limits.allow_forecast_evidence))
        .number(total_latency_ns)
}

fn require_physical_byte_evidence(
    costs: &StorageRecomputeCostVectorV1,
    limits: StorageRecomputePlanLimitsV1,
) -> Result<(), StorageRecomputePlanError> {
    if limits.max_resident_bytes_after.is_some() {
        let scope = costs
            .resident_bytes_after()
            .ok_or(StorageRecomputePlanError::MissingResidentBytes)?
            .scope();
        if scope != ByteEvidenceScopeV1::Physical {
            return Err(StorageRecomputePlanError::ResidentByteScopeMismatch { observed: scope });
        }
    }
    if limits.max_transfer_bytes.is_some() {
        if let Some(transfer) = costs.transfer_bytes() {
            let scope = transfer.scope();
            if scope != ByteEvidenceScopeV1::Physical {
                return Err(StorageRecomputePlanError::TransferByteScopeMismatch {
                    observed: scope,
                });
            }
        }
    }
    Ok(())
}

fn require_action_evidence(
    action: StorageRecomputeActionV1,
    costs: &StorageRecomputeCostVectorV1,
) -> Result<(), StorageRecomputePlanError> {
    match action {
        StorageRecomputeActionV1::Keep | StorageRecomputeActionV1::Compress => {
            costs
                .resident_bytes_after()
                .ok_or(StorageRecomputePlanError::MissingResidentBytes)?;
        }
        StorageRecomputeActionV1::Offload => {
            costs
                .resident_bytes_after()
                .ok_or(StorageRecomputePlanError::MissingResidentBytes)?;
            costs
                .transfer_bytes()
                .ok_or(StorageRecomputePlanError::MissingTransferBytes)?;
            costs
                .transfer_latency()
                .ok_or(StorageRecomputePlanError::MissingTransferLatency)?;
        }
        StorageRecomputeActionV1::DropAndReplay => {
            costs
                .resident_bytes_after()
                .ok_or(StorageRecomputePlanError::MissingResidentBytes)?;
            costs
                .recompute_latency()
                .ok_or(StorageRecomputePlanError::MissingRecomputeLatency)?;
        }
    }
    Ok(())
}

fn require_verified_approximate_replay(
    candidate: &StorageRecomputeCandidateV1,
    costs: &StorageRecomputeCostVectorV1,
) -> Result<(), StorageRecomputePlanError> {
    let Some(replay) = candidate.replay() else {
        return Ok(());
    };
    if replay.reconstruction() != ReplayReconstructionV1::Approximate {
        return Ok(());
    }
    let required = replay
        .verifier_id()
        .ok_or(StorageRecomputePlanError::ApproximateReplayVerifierMissing)?;
    match costs.quality_guard() {
        QualityGuardEvidenceV1::Verified { verifier_id, .. } if verifier_id == required => Ok(()),
        QualityGuardEvidenceV1::Verified { .. } => {
            Err(StorageRecomputePlanError::QualityVerifierMismatch)
        }
        QualityGuardEvidenceV1::Pending { .. } | QualityGuardEvidenceV1::NotAttached => {
            Err(StorageRecomputePlanError::QualityEvidenceNotVerified)
        }
    }
}

fn reject_disallowed_forecasts(
    costs: &StorageRecomputeCostVectorV1,
    allow_forecasts: bool,
) -> Result<(), StorageRecomputePlanError> {
    if allow_forecasts {
        return Ok(());
    }
    let forecast_present = costs
        .resident_bytes_after()
        .is_some_and(|value| value.basis() == CostEvidenceBasisV1::Forecast)
        || costs
            .transfer_bytes()
            .is_some_and(|value| value.basis() == CostEvidenceBasisV1::Forecast)
        || costs
            .transfer_latency()
            .is_some_and(|value| value.basis() == CostEvidenceBasisV1::Forecast)
        || costs
            .recompute_latency()
            .is_some_and(|value| value.basis() == CostEvidenceBasisV1::Forecast)
        || costs
            .controller_latency()
            .is_some_and(|value| value.basis() == CostEvidenceBasisV1::Forecast);
    if forecast_present {
        return Err(StorageRecomputePlanError::ForecastEvidenceForbidden);
    }
    Ok(())
}

/// Fail-closed EX-SR-2 screening failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageRecomputePlanError {
    UnboundedPolicy,
    CandidateFingerprintMismatch,
    MissingResidentBytes,
    MissingTransferBytes,
    MissingTransferLatency,
    MissingRecomputeLatency,
    MissingControllerLatency,
    ApproximateReplayVerifierMissing,
    QualityEvidenceNotVerified,
    QualityVerifierMismatch,
    ForecastEvidenceForbidden,
    ResidentBytesExceeded { observed: u64, limit: u64 },
    TransferBytesExceeded { observed: u64, limit: u64 },
    ResidentByteScopeMismatch { observed: ByteEvidenceScopeV1 },
    TransferByteScopeMismatch { observed: ByteEvidenceScopeV1 },
    LatencyOverflow,
    LatencyExceeded { observed: u64, limit: u64 },
}

impl fmt::Display for StorageRecomputePlanError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnboundedPolicy => {
                output.write_str("storage/recompute planning policy is unbounded")
            }
            Self::CandidateFingerprintMismatch => {
                output.write_str("cost vector belongs to another candidate")
            }
            Self::MissingResidentBytes => {
                output.write_str("candidate is missing resident-byte evidence")
            }
            Self::MissingTransferBytes => {
                output.write_str("candidate is missing transfer-byte evidence")
            }
            Self::MissingTransferLatency => {
                output.write_str("offload candidate is missing transfer latency")
            }
            Self::MissingRecomputeLatency => {
                output.write_str("replay candidate is missing recompute latency")
            }
            Self::MissingControllerLatency => {
                output.write_str("latency-bounded plan is missing controller latency")
            }
            Self::ApproximateReplayVerifierMissing => {
                output.write_str("approximate replay contract is missing its verifier")
            }
            Self::QualityEvidenceNotVerified => {
                output.write_str("approximate replay quality evidence is not verified")
            }
            Self::QualityVerifierMismatch => {
                output.write_str("quality verifier does not match the replay contract")
            }
            Self::ForecastEvidenceForbidden => {
                output.write_str("planning policy forbids forecast evidence")
            }
            Self::ResidentBytesExceeded { observed, limit } => {
                write!(output, "resident bytes {observed} exceed limit {limit}")
            }
            Self::TransferBytesExceeded { observed, limit } => {
                write!(output, "transfer bytes {observed} exceed limit {limit}")
            }
            Self::ResidentByteScopeMismatch { observed } => {
                write!(
                    output,
                    "resident-byte limit requires physical evidence, observed {observed:?}"
                )
            }
            Self::TransferByteScopeMismatch { observed } => {
                write!(
                    output,
                    "transfer-byte limit requires physical evidence, observed {observed:?}"
                )
            }
            Self::LatencyOverflow => output.write_str("aggregate latency overflowed u64"),
            Self::LatencyExceeded { observed, limit } => write!(
                output,
                "aggregate latency {observed} ns exceeds limit {limit} ns"
            ),
        }
    }
}

impl std::error::Error for StorageRecomputePlanError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_recompute::ReplayContractV1;
    use crate::storage_recompute_cost::{
        ByteCostEvidenceV1, ByteEvidenceScopeV1, DurationCostEvidenceV1, StorageRecomputeCostError,
    };

    fn bytes(value: u64) -> ByteCostEvidenceV1 {
        ByteCostEvidenceV1::new(
            value,
            ByteEvidenceScopeV1::Physical,
            CostEvidenceBasisV1::Measured,
            "bytes-1",
        )
        .unwrap()
    }

    fn logical_bytes(value: u64) -> ByteCostEvidenceV1 {
        ByteCostEvidenceV1::new(
            value,
            ByteEvidenceScopeV1::Logical,
            CostEvidenceBasisV1::Measured,
            "logical-bytes-1",
        )
        .unwrap()
    }

    fn duration(value: u64, basis: CostEvidenceBasisV1) -> DurationCostEvidenceV1 {
        DurationCostEvidenceV1::new(value, basis, "duration-1").unwrap()
    }

    fn candidate(
        id: &str,
        action: StorageRecomputeActionV1,
        reconstruction: Option<ReplayReconstructionV1>,
    ) -> StorageRecomputeCandidateV1 {
        let replay = reconstruction.map(|kind| {
            let verifier = (kind == ReplayReconstructionV1::Approximate)
                .then(|| "domain.quality.v1".to_string());
            ReplayContractV1::new(16, kind, verifier).unwrap()
        });
        StorageRecomputeCandidateV1::new(id, "state.v1", 3, "target", action, replay).unwrap()
    }

    fn replay_cost(
        candidate: &StorageRecomputeCandidateV1,
        basis: CostEvidenceBasisV1,
        quality: QualityGuardEvidenceV1,
    ) -> Result<StorageRecomputeCostVectorV1, StorageRecomputeCostError> {
        StorageRecomputeCostVectorV1::new(
            candidate,
            Some(bytes(0)),
            None,
            None,
            Some(duration(40, basis)),
            Some(duration(5, CostEvidenceBasisV1::Measured)),
            quality,
        )
    }

    fn limits(allow_forecasts: bool) -> StorageRecomputePlanLimitsV1 {
        StorageRecomputePlanLimitsV1::new(Some(64), None, Some(100), allow_forecasts).unwrap()
    }

    #[test]
    fn exact_replay_passes_but_never_authorizes_actuation() {
        let candidate = candidate(
            "exact",
            StorageRecomputeActionV1::DropAndReplay,
            Some(ReplayReconstructionV1::Exact),
        );
        let costs = replay_cost(
            &candidate,
            CostEvidenceBasisV1::Measured,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        let plan = StorageRecomputePlanV1::screen(&candidate, &costs, limits(false)).unwrap();

        assert_eq!(plan.action(), StorageRecomputeActionV1::DropAndReplay);
        assert_eq!(plan.total_latency_ns(), 45);
        assert_eq!(plan.candidate_fingerprint(), candidate.fingerprint());
        assert_eq!(plan.cost_fingerprint(), costs.fingerprint());
        assert_eq!(plan.limits(), limits(false));
        assert!(!plan.authorizes_actuation());
    }

    #[test]
    fn approximate_replay_requires_completed_domain_evidence() {
        let candidate = candidate(
            "approx",
            StorageRecomputeActionV1::DropAndReplay,
            Some(ReplayReconstructionV1::Approximate),
        );
        let pending = replay_cost(
            &candidate,
            CostEvidenceBasisV1::Measured,
            QualityGuardEvidenceV1::pending("domain.quality.v1").unwrap(),
        )
        .unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&candidate, &pending, limits(false)),
            Err(StorageRecomputePlanError::QualityEvidenceNotVerified)
        );

        let verified = replay_cost(
            &candidate,
            CostEvidenceBasisV1::Measured,
            QualityGuardEvidenceV1::verified("domain.quality.v1", "quality-run-1").unwrap(),
        )
        .unwrap();
        assert!(StorageRecomputePlanV1::screen(&candidate, &verified, limits(false)).is_ok());
    }

    #[test]
    fn forecast_evidence_is_distinct_and_policy_controlled() {
        let candidate = candidate(
            "forecast",
            StorageRecomputeActionV1::DropAndReplay,
            Some(ReplayReconstructionV1::Exact),
        );
        let costs = replay_cost(
            &candidate,
            CostEvidenceBasisV1::Forecast,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&candidate, &costs, limits(false)),
            Err(StorageRecomputePlanError::ForecastEvidenceForbidden)
        );
        assert!(StorageRecomputePlanV1::screen(&candidate, &costs, limits(true)).is_ok());
    }

    #[test]
    fn foreign_cost_vector_is_rejected() {
        let candidate_a = candidate("a", StorageRecomputeActionV1::Keep, None);
        let candidate_b = candidate("b", StorageRecomputeActionV1::Keep, None);
        let costs = StorageRecomputeCostVectorV1::new(
            &candidate_a,
            Some(bytes(10)),
            None,
            None,
            None,
            None,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&candidate_b, &costs, limits(false)),
            Err(StorageRecomputePlanError::CandidateFingerprintMismatch)
        );
    }

    #[test]
    fn action_specific_evidence_and_budgets_fail_closed() {
        let offload = candidate("offload", StorageRecomputeActionV1::Offload, None);
        let incomplete = StorageRecomputeCostVectorV1::new(
            &offload,
            Some(bytes(10)),
            None,
            None,
            None,
            None,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&offload, &incomplete, limits(false)),
            Err(StorageRecomputePlanError::MissingTransferBytes)
        );

        let keep = candidate("keep", StorageRecomputeActionV1::Keep, None);
        let oversized = StorageRecomputeCostVectorV1::new(
            &keep,
            Some(bytes(65)),
            None,
            None,
            None,
            None,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&keep, &oversized, limits(false)),
            Err(StorageRecomputePlanError::ResidentBytesExceeded {
                observed: 65,
                limit: 64,
            })
        );

        let zero_transfer_limit =
            StorageRecomputePlanLimitsV1::new(Some(64), Some(0), None, false).unwrap();
        let within_resident_budget = StorageRecomputeCostVectorV1::new(
            &keep,
            Some(bytes(64)),
            None,
            None,
            None,
            None,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        assert!(StorageRecomputePlanV1::screen(
            &keep,
            &within_resident_budget,
            zero_transfer_limit,
        )
        .is_ok());
    }

    #[test]
    fn latency_accumulation_is_checked() {
        let replay = candidate(
            "overflow",
            StorageRecomputeActionV1::DropAndReplay,
            Some(ReplayReconstructionV1::Exact),
        );
        let costs = StorageRecomputeCostVectorV1::new(
            &replay,
            Some(bytes(0)),
            None,
            None,
            Some(duration(u64::MAX, CostEvidenceBasisV1::Measured)),
            Some(duration(1, CostEvidenceBasisV1::Measured)),
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&replay, &costs, limits(false)),
            Err(StorageRecomputePlanError::LatencyOverflow)
        );
    }

    #[test]
    fn latency_budget_requires_controller_duration_evidence() {
        let compress = candidate("compress", StorageRecomputeActionV1::Compress, None);
        let costs = StorageRecomputeCostVectorV1::new(
            &compress,
            Some(bytes(10)),
            None,
            None,
            None,
            None,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&compress, &costs, limits(false)),
            Err(StorageRecomputePlanError::MissingControllerLatency)
        );
    }

    #[test]
    fn byte_budgets_reject_logical_accounting() {
        let keep = candidate("logical-resident", StorageRecomputeActionV1::Keep, None);
        let resident = StorageRecomputeCostVectorV1::new(
            &keep,
            Some(logical_bytes(10)),
            None,
            None,
            None,
            None,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        let resident_limits =
            StorageRecomputePlanLimitsV1::new(Some(64), None, None, false).unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&keep, &resident, resident_limits),
            Err(StorageRecomputePlanError::ResidentByteScopeMismatch {
                observed: ByteEvidenceScopeV1::Logical,
            })
        );

        let offload = candidate("logical-transfer", StorageRecomputeActionV1::Offload, None);
        let transfer = StorageRecomputeCostVectorV1::new(
            &offload,
            Some(bytes(0)),
            Some(logical_bytes(10)),
            Some(duration(5, CostEvidenceBasisV1::Measured)),
            None,
            None,
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        let transfer_limits =
            StorageRecomputePlanLimitsV1::new(None, Some(64), None, false).unwrap();
        assert_eq!(
            StorageRecomputePlanV1::screen(&offload, &transfer, transfer_limits),
            Err(StorageRecomputePlanError::TransferByteScopeMismatch {
                observed: ByteEvidenceScopeV1::Logical,
            })
        );
    }
}
