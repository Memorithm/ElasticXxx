//! Core contracts for ElasticXxx.
//!
//! The crate models adaptive resources as explicit state spaces.  Two layers
//! live here today:
//!
//! - the general elastic resource model ([`resource`]): typed declarations of
//!   what a resource is, which dimensions may change, which transitions are
//!   admissible, which invariants must hold, and what the runtime may
//!   optimize;
//! - the first resource-specific specialization ([`representation`],
//!   [`frontier`]): materialized representation states, validated transitions,
//!   and the propose/validate/commit/rollback frontier.

#![forbid(unsafe_code)]

pub mod frontier;
pub mod representation;
pub mod resource;

pub use frontier::{FrontierError, VersionFrontier};
pub use representation::{
    CapabilitySet, EvidenceKind, EvidenceToken, IssuerId, RepresentationEpoch, RepresentationId,
    RepresentationState, RepresentationTransition, TargetContract, TransitionAttestations,
    TransitionError, TransitionMechanism,
};
pub use resource::{
    AdmissibleTransition, BuiltinDimension, BuiltinObjective, BuiltinObservationSignal,
    BuiltinResourceClass, CapabilityRequirement, ContractId, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
    ResourceSpecError,
};
