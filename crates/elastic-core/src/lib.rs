//! Core contracts for ElasticXxx.
//!
//! The crate models adaptive resources as explicit state spaces.  Three layers
//! live here today:
//!
//! - the general elastic resource model ([`resource`]): typed declarations of
//!   what a resource is, which dimensions may change, which transitions are
//!   admissible, which invariants must hold, and what the runtime may
//!   optimize;
//! - recommendation freshness contracts ([`control`]): planner/observation
//!   epochs plus resource generations used to reject stale recommendations
//!   before semantic validation or actuation;
//! - the first resource-specific specialization ([`representation`],
//!   [`frontier`]): materialized representation states, validated transitions,
//!   and the propose/validate/commit/rollback frontier.

#![forbid(unsafe_code)]

pub mod control;
pub mod frontier;
pub mod representation;
pub mod resource;

pub use control::{
    FreshnessSnapshot, ObservationEpoch, PlannerEpoch, RecommendationContext,
    RecommendationFreshnessError, ResourceGeneration,
};
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
