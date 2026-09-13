//! Core contracts for ElasticXxx.
//!
//! The crate models adaptive resources as explicit state spaces. Four layers
//! live here today:
//!
//! - the general elastic resource model ([`resource`]): typed declarations of
//!   what a resource is, which dimensions may change, which transitions are
//!   admissible, which invariants must hold, and what the runtime may
//!   optimize;
//! - Boolean decision primitives ([`logic`], [`predicate`], [`canonical`],
//!   [`guard`]): fail-closed three-valued facts, stable predicate identities,
//!   bounded expressions, deterministic canonicalization, structural
//!   fingerprints, typed resource-bound guards, and a compact mask fast path
//!   for eligibility decisions;
//! - recommendation freshness contracts ([`control`]): planner/observation
//!   epochs plus resource generations used to reject stale recommendations
//!   before semantic validation or actuation;
//! - the first resource-specific specialization ([`representation`],
//!   [`frontier`]): materialized representation states, validated transitions,
//!   and the propose/validate/commit/rollback frontier.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod control;
pub mod frontier;
pub mod guard;
pub mod logic;
pub mod predicate;
pub mod representation;
pub mod resource;

pub use canonical::{
    BoolExprFingerprint, CanonicalizationError, BOOLEAN_EXPRESSION_SCHEMA_V1,
    MAX_CANONICAL_EXPRESSION_NODES,
};
pub use control::{
    FreshnessSnapshot, ObservationEpoch, PlannerEpoch, RecommendationContext,
    RecommendationFreshnessError, ResourceGeneration,
};
pub use frontier::{FrontierError, VersionFrontier};
pub use guard::{
    BooleanGuard, GuardBindingError, GuardFactSource, GuardScope, GuardedResourceSpec,
    TransitionGuard,
};
pub use logic::{
    BoolExpr, CompiledGuard, FactMask, FactSet, LogicError, PredicateId, TruthValue,
    FAST_PREDICATE_CAPACITY, MAX_BOOLEAN_EXPR_DEPTH,
};
pub use predicate::{
    PredicateComponent, PredicateComponentError, PredicateKey, PredicateRegistry,
    PredicateRegistryError, BOOLEAN_PREDICATE_SCHEMA_V1, MAX_PREDICATE_COMPONENT_BYTES,
    MAX_REGISTERED_PREDICATES,
};
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