//! EX-SR-1 evidence contract for storage-versus-recomputation cost vectors.
//!
//! This module records cost evidence without selecting an action. Measured and
//! forecast values are different types at the semantic level and byte evidence
//! explicitly states whether it is logical accounting or physical observation.

use elastic_eir::Fingerprint;
use std::fmt;

use crate::storage_recompute::{
    ReplayReconstructionV1, StorageRecomputeActionV1, StorageRecomputeCandidateV1,
};

/// Stable schema identity for the first storage/recompute cost vector.
pub const STORAGE_RECOMPUTE_COST_VECTOR_V1: &str =
    "elastic.kv.storage-recompute-cost-vector@1.0.0";

/// Maximum UTF-8 bytes accepted for evidence and verifier identifiers.
pub const MAX_STORAGE_RECOMPUTE_EVIDENCE_ID_BYTES: usize = 128;

/// Whether a numeric cost came from observation or a forecasting model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostEvidenceBasisV1 {
    /// Value came from a declared measurement surface.
    Measured,
    /// Value is an advisory forecast and must not be relabelled as measurement.
    Forecast,
}

/// Whether byte accounting is logical or physically observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteEvidenceScopeV1 {
    /// Representation-level accounting only.
    Logical,
    /// Physical resident/transfer byte observation from a declared measurement surface.
    Physical,
}

/// Versioned byte-cost evidence with provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ByteCostEvidenceV1 {
    bytes: u64,
    scope: ByteEvidenceScopeV1,
    basis: CostEvidenceBasisV1,
    evidence_id: String,
}

impl ByteCostEvidenceV1 {
    /// Construct byte evidence with an explicit accounting scope and provenance id.
    pub fn new(
        bytes: u64,
        scope: ByteEvidenceScopeV1,
        basis: CostEvidenceBasisV1,
        evidence_id: impl Into<String>,
    ) -> Result<Self, StorageRecomputeCostError> {
        let evidence_id = evidence_id.into();
        validate_id("evidence_id", &evidence_id)?;
        Ok(Self {
            bytes,
            scope,
            basis,
            evidence_id,
        })
    }

    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn scope(&self) -> ByteEvidenceScopeV1 {
        self.scope
    }

    #[must_use]
    pub const fn basis(&self) -> CostEvidenceBasisV1 {
        self.basis
    }

    #[must_use]
    pub fn evidence_id(&self) -> &str {
        &self.evidence_id
    }
}

/// Versioned duration evidence in integer nanoseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurationCostEvidenceV1 {
    nanoseconds: u64,
    basis: CostEvidenceBasisV1,
    evidence_id: String,
}

impl DurationCostEvidenceV1 {
    /// Construct measured or forecast duration evidence.
    pub fn new(
        nanoseconds: u64,
        basis: CostEvidenceBasisV1,
        evidence_id: impl Into<String>,
    ) -> Result<Self, StorageRecomputeCostError> {
        let evidence_id = evidence_id.into();
        validate_id("evidence_id", &evidence_id)?;
        Ok(Self {
            nanoseconds,
            basis,
            evidence_id,
        })
    }

    #[must_use]
    pub const fn nanoseconds(&self) -> u64 {
        self.nanoseconds
    }

    #[must_use]
    pub const fn basis(&self) -> CostEvidenceBasisV1 {
        self.basis
    }

    #[must_use]
    pub fn evidence_id(&self) -> &str {
        &self.evidence_id
    }
}

/// Domain-owned quality/semantic guard evidence.
///
/// The generic layer records the state but does not evaluate model quality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QualityGuardEvidenceV1 {
    /// No domain quality guard is attached to this candidate/evidence record.
    NotAttached,
    /// A verifier is required but has not yet produced accepted evidence.
    Pending { verifier_id: String },
    /// The named verifier produced an accepted domain-owned evidence record.
    Verified {
        verifier_id: String,
        evidence_id: String,
    },
}

impl QualityGuardEvidenceV1 {
    /// Construct a pending guard identity.
    pub fn pending(verifier_id: impl Into<String>) -> Result<Self, StorageRecomputeCostError> {
        let verifier_id = verifier_id.into();
        validate_id("verifier_id", &verifier_id)?;
        Ok(Self::Pending { verifier_id })
    }

    /// Construct verified guard evidence.
    pub fn verified(
        verifier_id: impl Into<String>,
        evidence_id: impl Into<String>,
    ) -> Result<Self, StorageRecomputeCostError> {
        let verifier_id = verifier_id.into();
        let evidence_id = evidence_id.into();
        validate_id("verifier_id", &verifier_id)?;
        validate_id("quality_evidence_id", &evidence_id)?;
        Ok(Self::Verified {
            verifier_id,
            evidence_id,
        })
    }

    fn verifier_id(&self) -> Option<&str> {
        match self {
            Self::NotAttached => None,
            Self::Pending { verifier_id } | Self::Verified { verifier_id, .. } => Some(verifier_id),
        }
    }
}

/// Complete EX-SR-1 cost evidence bound to one exact EX-SR-0 candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRecomputeCostVectorV1 {
    candidate_fingerprint: Fingerprint,
    resident_bytes_after: Option<ByteCostEvidenceV1>,
    transfer_bytes: Option<ByteCostEvidenceV1>,
    transfer_latency: Option<DurationCostEvidenceV1>,
    recompute_latency: Option<DurationCostEvidenceV1>,
    controller_latency: Option<DurationCostEvidenceV1>,
    quality_guard: QualityGuardEvidenceV1,
}

impl StorageRecomputeCostVectorV1 {
    /// Construct a cost vector without selecting or authorizing the candidate.
    ///
    /// A drop-and-replay candidate must include recompute-duration evidence.
    /// Approximate replay must carry the exact verifier identity declared by
    /// the EX-SR-0 replay contract, though that verifier may still be pending.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        candidate: &StorageRecomputeCandidateV1,
        resident_bytes_after: Option<ByteCostEvidenceV1>,
        transfer_bytes: Option<ByteCostEvidenceV1>,
        transfer_latency: Option<DurationCostEvidenceV1>,
        recompute_latency: Option<DurationCostEvidenceV1>,
        controller_latency: Option<DurationCostEvidenceV1>,
        quality_guard: QualityGuardEvidenceV1,
    ) -> Result<Self, StorageRecomputeCostError> {
        let any_cost = resident_bytes_after.is_some()
            || transfer_bytes.is_some()
            || transfer_latency.is_some()
            || recompute_latency.is_some()
            || controller_latency.is_some();
        if !any_cost {
            return Err(StorageRecomputeCostError::EmptyCostVector);
        }

        match candidate.action() {
            StorageRecomputeActionV1::DropAndReplay => {
                if recompute_latency.is_none() {
                    return Err(StorageRecomputeCostError::ReplayMissingRecomputeLatency);
                }
                let replay = candidate
                    .replay()
                    .ok_or(StorageRecomputeCostError::ReplayContractMissing)?;
                if matches!(replay.reconstruction(), ReplayReconstructionV1::Approximate) {
                    let required = replay
                        .verifier_id()
                        .ok_or(StorageRecomputeCostError::ReplayVerifierMissing)?;
                    let observed = quality_guard
                        .verifier_id()
                        .ok_or(StorageRecomputeCostError::ApproximateReplayGuardMissing)?;
                    if required != observed {
                        return Err(StorageRecomputeCostError::QualityVerifierMismatch);
                    }
                }
            }
            StorageRecomputeActionV1::Keep
            | StorageRecomputeActionV1::Compress
            | StorageRecomputeActionV1::Offload => {
                if recompute_latency.is_some() {
                    return Err(StorageRecomputeCostError::RecomputeLatencyOnNonReplayAction);
                }
            }
        }

        Ok(Self {
            candidate_fingerprint: candidate.fingerprint(),
            resident_bytes_after,
            transfer_bytes,
            transfer_latency,
            recompute_latency,
            controller_latency,
            quality_guard,
        })
    }

    #[must_use]
    pub const fn candidate_fingerprint(&self) -> Fingerprint {
        self.candidate_fingerprint
    }

    #[must_use]
    pub const fn resident_bytes_after(&self) -> Option<&ByteCostEvidenceV1> {
        self.resident_bytes_after.as_ref()
    }

    #[must_use]
    pub const fn transfer_bytes(&self) -> Option<&ByteCostEvidenceV1> {
        self.transfer_bytes.as_ref()
    }

    #[must_use]
    pub const fn transfer_latency(&self) -> Option<&DurationCostEvidenceV1> {
        self.transfer_latency.as_ref()
    }

    #[must_use]
    pub const fn recompute_latency(&self) -> Option<&DurationCostEvidenceV1> {
        self.recompute_latency.as_ref()
    }

    #[must_use]
    pub const fn controller_latency(&self) -> Option<&DurationCostEvidenceV1> {
        self.controller_latency.as_ref()
    }

    #[must_use]
    pub const fn quality_guard(&self) -> &QualityGuardEvidenceV1 {
        &self.quality_guard
    }
}

/// Invalid EX-SR-1 cost evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageRecomputeCostError {
    EmptyId { field: &'static str },
    IdTooLong { field: &'static str, bytes: usize },
    EmptyCostVector,
    ReplayMissingRecomputeLatency,
    ReplayContractMissing,
    ReplayVerifierMissing,
    ApproximateReplayGuardMissing,
    QualityVerifierMismatch,
    RecomputeLatencyOnNonReplayAction,
}

impl fmt::Display for StorageRecomputeCostError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId { field } => write!(output, "{field} must not be empty"),
            Self::IdTooLong { field, bytes } => write!(
                output,
                "{field} uses {bytes} bytes, maximum is {MAX_STORAGE_RECOMPUTE_EVIDENCE_ID_BYTES}"
            ),
            Self::EmptyCostVector => output.write_str("storage/recompute cost vector is empty"),
            Self::ReplayMissingRecomputeLatency => {
                output.write_str("drop-and-replay requires recompute-duration evidence")
            }
            Self::ReplayContractMissing => {
                output.write_str("drop-and-replay candidate is missing its replay contract")
            }
            Self::ReplayVerifierMissing => {
                output.write_str("approximate replay contract is missing its verifier")
            }
            Self::ApproximateReplayGuardMissing => {
                output.write_str("approximate replay cost evidence is missing its quality guard")
            }
            Self::QualityVerifierMismatch => {
                output.write_str("quality guard verifier does not match replay contract")
            }
            Self::RecomputeLatencyOnNonReplayAction => {
                output.write_str("non-replay action must not carry recompute-duration evidence")
            }
        }
    }
}

impl std::error::Error for StorageRecomputeCostError {}

fn validate_id(field: &'static str, value: &str) -> Result<(), StorageRecomputeCostError> {
    if value.is_empty() {
        return Err(StorageRecomputeCostError::EmptyId { field });
    }
    if value.len() > MAX_STORAGE_RECOMPUTE_EVIDENCE_ID_BYTES {
        return Err(StorageRecomputeCostError::IdTooLong {
            field,
            bytes: value.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_recompute::{ReplayContractV1, ReplayReconstructionV1};

    fn replay_candidate(reconstruction: ReplayReconstructionV1) -> StorageRecomputeCandidateV1 {
        let verifier = matches!(reconstruction, ReplayReconstructionV1::Approximate)
            .then(|| "slha.replay-quality.v1".to_string());
        StorageRecomputeCandidateV1::new(
            "replay",
            "slha.state.v1",
            9,
            "dropped",
            StorageRecomputeActionV1::DropAndReplay,
            Some(ReplayContractV1::new(128, reconstruction, verifier).unwrap()),
        )
        .unwrap()
    }

    fn measured_ns(value: u64, id: &str) -> DurationCostEvidenceV1 {
        DurationCostEvidenceV1::new(value, CostEvidenceBasisV1::Measured, id).unwrap()
    }

    #[test]
    fn measured_and_forecast_evidence_remain_distinct() {
        let measured = ByteCostEvidenceV1::new(
            96,
            ByteEvidenceScopeV1::Physical,
            CostEvidenceBasisV1::Measured,
            "allocator-snapshot-1",
        )
        .unwrap();
        let forecast = ByteCostEvidenceV1::new(
            96,
            ByteEvidenceScopeV1::Physical,
            CostEvidenceBasisV1::Forecast,
            "capacity-model-1",
        )
        .unwrap();
        assert_ne!(measured.basis(), forecast.basis());
        assert_eq!(measured.bytes(), forecast.bytes());
    }

    #[test]
    fn approximate_replay_binds_exact_domain_verifier() {
        let candidate = replay_candidate(ReplayReconstructionV1::Approximate);
        let vector = StorageRecomputeCostVectorV1::new(
            &candidate,
            None,
            None,
            None,
            Some(measured_ns(25_000, "replay-timer-1")),
            None,
            QualityGuardEvidenceV1::pending("slha.replay-quality.v1").unwrap(),
        )
        .unwrap();
        assert_eq!(vector.candidate_fingerprint(), candidate.fingerprint());

        assert_eq!(
            StorageRecomputeCostVectorV1::new(
                &candidate,
                None,
                None,
                None,
                Some(measured_ns(25_000, "replay-timer-1")),
                None,
                QualityGuardEvidenceV1::pending("other.verifier").unwrap(),
            ),
            Err(StorageRecomputeCostError::QualityVerifierMismatch)
        );
    }

    #[test]
    fn replay_requires_recompute_latency_evidence() {
        let candidate = replay_candidate(ReplayReconstructionV1::Exact);
        assert_eq!(
            StorageRecomputeCostVectorV1::new(
                &candidate,
                Some(
                    ByteCostEvidenceV1::new(
                        0,
                        ByteEvidenceScopeV1::Logical,
                        CostEvidenceBasisV1::Forecast,
                        "layout-model-1",
                    )
                    .unwrap()
                ),
                None,
                None,
                None,
                None,
                QualityGuardEvidenceV1::NotAttached,
            ),
            Err(StorageRecomputeCostError::ReplayMissingRecomputeLatency)
        );
    }

    #[test]
    fn non_replay_action_rejects_recompute_duration() {
        let candidate = StorageRecomputeCandidateV1::new(
            "keep",
            "slha.state.v1",
            9,
            "resident",
            StorageRecomputeActionV1::Keep,
            None,
        )
        .unwrap();
        assert_eq!(
            StorageRecomputeCostVectorV1::new(
                &candidate,
                None,
                None,
                None,
                Some(measured_ns(1, "invalid-recompute")),
                None,
                QualityGuardEvidenceV1::NotAttached,
            ),
            Err(StorageRecomputeCostError::RecomputeLatencyOnNonReplayAction)
        );
    }

    #[test]
    fn empty_cost_vector_fails_closed() {
        let candidate = StorageRecomputeCandidateV1::new(
            "keep",
            "slha.state.v1",
            9,
            "resident",
            StorageRecomputeActionV1::Keep,
            None,
        )
        .unwrap();
        assert_eq!(
            StorageRecomputeCostVectorV1::new(
                &candidate,
                None,
                None,
                None,
                None,
                None,
                QualityGuardEvidenceV1::NotAttached,
            ),
            Err(StorageRecomputeCostError::EmptyCostVector)
        );
    }
}
