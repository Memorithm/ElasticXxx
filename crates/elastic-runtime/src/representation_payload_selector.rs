//! Planning-only selection over externally supplied representation payload costs.
//!
//! Profile identifiers and payload costs come from the owning domain.
//! ElasticXxx canonicalizes candidates and retains every exact minimum tie.
//! This module does not infer semantic admissibility, transition cost, runtime
//! latency, quality, or actuation authority.

use std::collections::BTreeSet;
use std::fmt;

/// Versioned identity of the structural payload selector.
pub const REPRESENTATION_PAYLOAD_SELECTOR_V1: &str =
    "elastic.representation-payload-selector@1.0.0";

/// Maximum candidates admitted to one bounded selection.
pub const MAX_REPRESENTATION_PAYLOAD_CANDIDATES_V1: usize = 64;

/// Maximum UTF-8 bytes in one candidate identifier.
pub const MAX_REPRESENTATION_PROFILE_ID_BYTES_V1: usize = 128;

/// One domain-supplied candidate and its declared representation payload.
///
/// The payload value is structural accounting only. It is not physical
/// DRAM/HBM traffic, process memory, allocator usage, or runtime cost.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RepresentationPayloadCandidateV1 {
    profile_id: String,
    payload_bits: u64,
}

impl RepresentationPayloadCandidateV1 {
    /// Construct one bounded candidate.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty, oversized, or control-character-bearing
    /// profile identifier.
    pub fn new(
        profile_id: impl Into<String>,
        payload_bits: u64,
    ) -> Result<Self, RepresentationPayloadSelectorError> {
        let profile_id = profile_id.into();
        validate_profile_id(&profile_id)?;
        Ok(Self {
            profile_id,
            payload_bits,
        })
    }

    /// Domain-owned stable candidate identifier.
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// Exact declared structural payload in bits.
    #[must_use]
    pub const fn payload_bits(&self) -> u64 {
        self.payload_bits
    }
}

/// Canonical minimum-payload result retaining every exact tie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationPayloadMinimumV1 {
    minimum_payload_bits: u64,
    profiles: Vec<RepresentationPayloadCandidateV1>,
}

impl RepresentationPayloadMinimumV1 {
    /// Smallest observed structural payload.
    #[must_use]
    pub const fn minimum_payload_bits(&self) -> u64 {
        self.minimum_payload_bits
    }

    /// Canonically profile-id-ordered exact minimum set.
    #[must_use]
    pub fn profiles(&self) -> &[RepresentationPayloadCandidateV1] {
        &self.profiles
    }

    /// Whether the minimum is unique.
    #[must_use]
    pub fn is_unique(&self) -> bool {
        self.profiles.len() == 1
    }
}

/// Select every exact minimum from a bounded domain-supplied candidate set.
///
/// Candidate input order is not semantic. Duplicate profile identifiers are
/// rejected instead of silently overwritten.
///
/// # Errors
///
/// Returns an error for an empty or oversized set, or for duplicate profile
/// identity.
pub fn select_minimum_payload_v1(
    candidates: impl IntoIterator<Item = RepresentationPayloadCandidateV1>,
) -> Result<RepresentationPayloadMinimumV1, RepresentationPayloadSelectorError> {
    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(RepresentationPayloadSelectorError::EmptyCandidateSet);
    }
    if candidates.len() > MAX_REPRESENTATION_PAYLOAD_CANDIDATES_V1 {
        return Err(RepresentationPayloadSelectorError::TooManyCandidates {
            observed: candidates.len(),
            maximum: MAX_REPRESENTATION_PAYLOAD_CANDIDATES_V1,
        });
    }

    candidates.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
    let mut identities = BTreeSet::new();
    for candidate in &candidates {
        if !identities.insert(candidate.profile_id.clone()) {
            return Err(RepresentationPayloadSelectorError::DuplicateProfile {
                profile_id: candidate.profile_id.clone(),
            });
        }
    }

    let minimum_payload_bits = candidates
        .iter()
        .map(RepresentationPayloadCandidateV1::payload_bits)
        .min()
        .expect("non-empty candidate set");
    let profiles = candidates
        .into_iter()
        .filter(|candidate| candidate.payload_bits == minimum_payload_bits)
        .collect::<Vec<_>>();

    Ok(RepresentationPayloadMinimumV1 {
        minimum_payload_bits,
        profiles,
    })
}

/// Fail-closed structural selector errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadSelectorError {
    EmptyCandidateSet,
    TooManyCandidates { observed: usize, maximum: usize },
    InvalidProfileId { profile_id: String },
    DuplicateProfile { profile_id: String },
}

impl fmt::Display for RepresentationPayloadSelectorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCandidateSet => {
                formatter.write_str("representation payload selector requires candidates")
            }
            Self::TooManyCandidates { observed, maximum } => write!(
                formatter,
                "representation payload selector received {observed} candidates; maximum is {maximum}"
            ),
            Self::InvalidProfileId { profile_id } => {
                write!(formatter, "invalid representation profile identifier {profile_id:?}")
            }
            Self::DuplicateProfile { profile_id } => {
                write!(formatter, "duplicate representation profile identifier {profile_id:?}")
            }
        }
    }
}

impl std::error::Error for RepresentationPayloadSelectorError {}

fn validate_profile_id(profile_id: &str) -> Result<(), RepresentationPayloadSelectorError> {
    if profile_id.is_empty()
        || profile_id.len() > MAX_REPRESENTATION_PROFILE_ID_BYTES_V1
        || profile_id.chars().any(char::is_control)
    {
        return Err(RepresentationPayloadSelectorError::InvalidProfileId {
            profile_id: profile_id.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, bits: u64) -> RepresentationPayloadCandidateV1 {
        RepresentationPayloadCandidateV1::new(id, bits).unwrap()
    }

    #[test]
    fn unique_minimum_is_selected_independent_of_input_order() {
        for candidates in [
            vec![
                candidate("sparse", 512),
                candidate("dense", 8192),
                candidate("hybrid", 4288),
            ],
            vec![
                candidate("hybrid", 4288),
                candidate("sparse", 512),
                candidate("dense", 8192),
            ],
        ] {
            let selected = select_minimum_payload_v1(candidates).unwrap();
            assert_eq!(selected.minimum_payload_bits(), 512);
            assert!(selected.is_unique());
            assert_eq!(selected.profiles()[0].profile_id(), "sparse");
        }
    }

    #[test]
    fn ties_are_retained_in_canonical_profile_order() {
        let selected = select_minimum_payload_v1([
            candidate("hybrid", 384),
            candidate("dense", 384),
            candidate("sparse", 1536),
        ])
        .unwrap();

        assert_eq!(selected.minimum_payload_bits(), 384);
        assert!(!selected.is_unique());
        assert_eq!(
            selected
                .profiles()
                .iter()
                .map(RepresentationPayloadCandidateV1::profile_id)
                .collect::<Vec<_>>(),
            vec!["dense", "hybrid"]
        );
    }

    #[test]
    fn duplicate_identity_fails_closed() {
        assert_eq!(
            select_minimum_payload_v1([candidate("same", 64), candidate("same", 128)]),
            Err(RepresentationPayloadSelectorError::DuplicateProfile {
                profile_id: "same".to_owned(),
            })
        );
    }

    #[test]
    fn candidate_bounds_fail_closed() {
        assert!(matches!(
            RepresentationPayloadCandidateV1::new("", 1),
            Err(RepresentationPayloadSelectorError::InvalidProfileId { .. })
        ));
        assert_eq!(
            select_minimum_payload_v1(Vec::<RepresentationPayloadCandidateV1>::new()),
            Err(RepresentationPayloadSelectorError::EmptyCandidateSet)
        );
        let too_many = (0..=MAX_REPRESENTATION_PAYLOAD_CANDIDATES_V1)
            .map(|index| candidate(&format!("profile-{index}"), index as u64))
            .collect::<Vec<_>>();
        assert!(matches!(
            select_minimum_payload_v1(too_many),
            Err(RepresentationPayloadSelectorError::TooManyCandidates { .. })
        ));
    }
}
