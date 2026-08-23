//! Core contracts for ElasticXxx.
//!
//! The crate models adaptive resources as explicit state spaces.  The first
//! executable slice focuses on *representational resources*: data whose
//! mathematical/numerical representation may change only through declared,
//! validated transitions.

#![forbid(unsafe_code)]

pub mod frontier;
pub mod representation;

pub use frontier::{FrontierError, VersionFrontier};
pub use representation::{
    CapabilitySet, EvidenceKind, EvidenceToken, IssuerId, RepresentationEpoch, RepresentationId,
    RepresentationState, RepresentationTransition, TargetContract, TransitionAttestations,
    TransitionError, TransitionMechanism,
};
