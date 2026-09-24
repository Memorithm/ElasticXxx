//! Versioned storage-versus-recomputation candidate contract.
//!
//! This is the EX-SR-0 structural slice. It defines admissible candidate
//! metadata only; it does not plan, actuate, verify, or claim that replay is
//! cheaper than storage. Domain runtimes remain authoritative for semantic
//! reconstruction and quality verification.

use elastic_eir::Fingerprint;
use std::fmt;

/// Stable schema identity for the first storage/recompute candidate contract.
pub const STORAGE_RECOMPUTE_CANDIDATE_V1: &str = "elastic.kv.storage-recompute-candidate@1.0.0";

/// Maximum UTF-8 bytes accepted for opaque contract identifiers.
pub const MAX_STORAGE_RECOMPUTE_ID_BYTES: usize = 128;

/// Generic action family. These labels do not imply domain-specific mechanics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageRecomputeActionV1 {
    Keep,
    Compress,
    Offload,
    DropAndReplay,
}

/// Reconstruction class for a drop-and-replay candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayReconstructionV1 {
    /// The owning domain claims exact reconstruction under its declared oracle.
    Exact,
    /// Reconstruction is approximate and requires an explicit domain verifier.
    Approximate,
}

/// Domain-owned replay metadata for a drop-and-replay candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayContractV1 {
    window_items: u32,
    reconstruction: ReplayReconstructionV1,
    verifier_id: Option<String>,
}

impl ReplayContractV1 {
    /// Construct a replay contract.
    ///
    /// Approximate replay must name a domain verifier. Exact replay may also
    /// name one, but the generic layer does not interpret it.
    pub fn new(
        window_items: u32,
        reconstruction: ReplayReconstructionV1,
        verifier_id: Option<String>,
    ) -> Result<Self, StorageRecomputeContractError> {
        if window_items == 0 {
            return Err(StorageRecomputeContractError::ZeroReplayWindow);
        }
        if let Some(verifier) = verifier_id.as_deref() {
            validate_id("verifier_id", verifier)?;
        }
        if matches!(reconstruction, ReplayReconstructionV1::Approximate) && verifier_id.is_none() {
            return Err(StorageRecomputeContractError::ApproximateReplayMissingVerifier);
        }
        Ok(Self {
            window_items,
            reconstruction,
            verifier_id,
        })
    }

    #[must_use]
    pub const fn window_items(&self) -> u32 {
        self.window_items
    }

    #[must_use]
    pub const fn reconstruction(&self) -> ReplayReconstructionV1 {
        self.reconstruction
    }

    #[must_use]
    pub fn verifier_id(&self) -> Option<&str> {
        self.verifier_id.as_deref()
    }
}

/// Structurally validated candidate for one storage/recompute decision.
///
/// The opaque state ids are owned by the destination runtime. ElasticXxx uses
/// them only to bind identity and prevent action substitution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRecomputeCandidateV1 {
    candidate_id: String,
    semantic_contract_id: String,
    source_generation: u64,
    target_state_id: String,
    action: StorageRecomputeActionV1,
    replay: Option<ReplayContractV1>,
    fingerprint: Fingerprint,
}

impl StorageRecomputeCandidateV1 {
    pub fn new(
        candidate_id: impl Into<String>,
        semantic_contract_id: impl Into<String>,
        source_generation: u64,
        target_state_id: impl Into<String>,
        action: StorageRecomputeActionV1,
        replay: Option<ReplayContractV1>,
    ) -> Result<Self, StorageRecomputeContractError> {
        let candidate_id = candidate_id.into();
        let semantic_contract_id = semantic_contract_id.into();
        let target_state_id = target_state_id.into();

        validate_id("candidate_id", &candidate_id)?;
        validate_id("semantic_contract_id", &semantic_contract_id)?;
        validate_id("target_state_id", &target_state_id)?;

        match action {
            StorageRecomputeActionV1::DropAndReplay if replay.is_none() => {
                return Err(StorageRecomputeContractError::DropAndReplayMissingContract);
            }
            StorageRecomputeActionV1::Keep
            | StorageRecomputeActionV1::Compress
            | StorageRecomputeActionV1::Offload
                if replay.is_some() =>
            {
                return Err(StorageRecomputeContractError::ReplayOnNonReplayAction);
            }
            _ => {}
        }

        let fingerprint = candidate_fingerprint(
            &candidate_id,
            &semantic_contract_id,
            source_generation,
            &target_state_id,
            action,
            replay.as_ref(),
        );

        Ok(Self {
            candidate_id,
            semantic_contract_id,
            source_generation,
            target_state_id,
            action,
            replay,
            fingerprint,
        })
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub fn semantic_contract_id(&self) -> &str {
        &self.semantic_contract_id
    }

    #[must_use]
    pub const fn source_generation(&self) -> u64 {
        self.source_generation
    }

    #[must_use]
    pub fn target_state_id(&self) -> &str {
        &self.target_state_id
    }

    #[must_use]
    pub const fn action(&self) -> StorageRecomputeActionV1 {
        self.action
    }

    #[must_use]
    pub const fn replay(&self) -> Option<&ReplayContractV1> {
        self.replay.as_ref()
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

fn validate_id(field: &'static str, value: &str) -> Result<(), StorageRecomputeContractError> {
    if value.is_empty() {
        return Err(StorageRecomputeContractError::EmptyId { field });
    }
    if value.len() > MAX_STORAGE_RECOMPUTE_ID_BYTES {
        return Err(StorageRecomputeContractError::IdTooLong {
            field,
            bytes: value.len(),
        });
    }
    Ok(())
}

fn candidate_fingerprint(
    candidate_id: &str,
    semantic_contract_id: &str,
    source_generation: u64,
    target_state_id: &str,
    action: StorageRecomputeActionV1,
    replay: Option<&ReplayContractV1>,
) -> Fingerprint {
    let mut fingerprint = Fingerprint::EMPTY
        .text(STORAGE_RECOMPUTE_CANDIDATE_V1)
        .text(candidate_id)
        .text(semantic_contract_id)
        .number(source_generation)
        .text(target_state_id)
        .text(action_name(action));

    if let Some(replay) = replay {
        fingerprint = fingerprint
            .number(u64::from(replay.window_items()))
            .text(reconstruction_name(replay.reconstruction()))
            .text(replay.verifier_id().unwrap_or(""));
    }
    fingerprint
}

const fn action_name(action: StorageRecomputeActionV1) -> &'static str {
    match action {
        StorageRecomputeActionV1::Keep => "keep",
        StorageRecomputeActionV1::Compress => "compress",
        StorageRecomputeActionV1::Offload => "offload",
        StorageRecomputeActionV1::DropAndReplay => "drop-and-replay",
    }
}

const fn reconstruction_name(value: ReplayReconstructionV1) -> &'static str {
    match value {
        ReplayReconstructionV1::Exact => "exact",
        ReplayReconstructionV1::Approximate => "approximate",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageRecomputeContractError {
    EmptyId { field: &'static str },
    IdTooLong { field: &'static str, bytes: usize },
    ZeroReplayWindow,
    ApproximateReplayMissingVerifier,
    DropAndReplayMissingContract,
    ReplayOnNonReplayAction,
}

impl fmt::Display for StorageRecomputeContractError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId { field } => write!(output, "{field} must not be empty"),
            Self::IdTooLong { field, bytes } => write!(
                output,
                "{field} uses {bytes} bytes, maximum is {MAX_STORAGE_RECOMPUTE_ID_BYTES}"
            ),
            Self::ZeroReplayWindow => output.write_str("replay window must be non-zero"),
            Self::ApproximateReplayMissingVerifier => {
                output.write_str("approximate replay requires a domain verifier id")
            }
            Self::DropAndReplayMissingContract => {
                output.write_str("drop-and-replay candidate requires replay metadata")
            }
            Self::ReplayOnNonReplayAction => {
                output.write_str("non-replay action must not carry replay metadata")
            }
        }
    }
}

impl std::error::Error for StorageRecomputeContractError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact_replay() -> ReplayContractV1 {
        ReplayContractV1::new(128, ReplayReconstructionV1::Exact, None).unwrap()
    }

    #[test]
    fn accepts_all_four_action_families_with_correct_shapes() {
        for (id, action) in [
            ("keep", StorageRecomputeActionV1::Keep),
            ("compress", StorageRecomputeActionV1::Compress),
            ("offload", StorageRecomputeActionV1::Offload),
        ] {
            let candidate =
                StorageRecomputeCandidateV1::new(id, "kv.semantic.v1", 7, "target", action, None)
                    .unwrap();
            assert_eq!(candidate.action(), action);
            assert!(candidate.replay().is_none());
        }

        let replay = StorageRecomputeCandidateV1::new(
            "replay",
            "kv.semantic.v1",
            7,
            "dropped",
            StorageRecomputeActionV1::DropAndReplay,
            Some(exact_replay()),
        )
        .unwrap();
        assert_eq!(replay.replay().unwrap().window_items(), 128);
    }

    #[test]
    fn approximate_replay_requires_named_domain_verifier() {
        assert_eq!(
            ReplayContractV1::new(64, ReplayReconstructionV1::Approximate, None),
            Err(StorageRecomputeContractError::ApproximateReplayMissingVerifier)
        );
        assert!(ReplayContractV1::new(
            64,
            ReplayReconstructionV1::Approximate,
            Some("slhav2.quality.v1".into())
        )
        .is_ok());
    }

    #[test]
    fn action_and_replay_metadata_cannot_be_substituted() {
        assert_eq!(
            StorageRecomputeCandidateV1::new(
                "bad",
                "kv.semantic.v1",
                1,
                "target",
                StorageRecomputeActionV1::DropAndReplay,
                None,
            ),
            Err(StorageRecomputeContractError::DropAndReplayMissingContract)
        );
        assert_eq!(
            StorageRecomputeCandidateV1::new(
                "bad",
                "kv.semantic.v1",
                1,
                "target",
                StorageRecomputeActionV1::Compress,
                Some(exact_replay()),
            ),
            Err(StorageRecomputeContractError::ReplayOnNonReplayAction)
        );
    }

    #[test]
    fn fingerprint_is_deterministic_and_identity_sensitive() {
        let a = StorageRecomputeCandidateV1::new(
            "replay",
            "kv.semantic.v1",
            3,
            "dropped",
            StorageRecomputeActionV1::DropAndReplay,
            Some(exact_replay()),
        )
        .unwrap();
        let b = a.clone();
        let c = StorageRecomputeCandidateV1::new(
            "replay",
            "kv.semantic.v1",
            4,
            "dropped",
            StorageRecomputeActionV1::DropAndReplay,
            Some(exact_replay()),
        )
        .unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_ne!(a.fingerprint(), c.fingerprint());
    }

    #[test]
    fn identifiers_and_windows_fail_closed() {
        assert_eq!(
            ReplayContractV1::new(0, ReplayReconstructionV1::Exact, None),
            Err(StorageRecomputeContractError::ZeroReplayWindow)
        );
        assert!(matches!(
            StorageRecomputeCandidateV1::new(
                "",
                "kv.semantic.v1",
                0,
                "same",
                StorageRecomputeActionV1::Keep,
                None,
            ),
            Err(StorageRecomputeContractError::EmptyId {
                field: "candidate_id"
            })
        ));
    }
}
