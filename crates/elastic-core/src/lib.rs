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
pub mod exact_oracle;
pub mod frontier;
pub mod guard;
pub mod invariant_predicate;
pub mod kleene_oracle;
pub mod logic;
pub mod multiword;
pub mod multiword_guard;
pub mod predicate;
pub mod pseudo_boolean;
pub mod pseudo_boolean_oracle;
pub mod representation;
pub mod resource;
pub mod resource_policy_analysis;
pub mod symbolic_backend;

pub use canonical::{
    BoolExprFingerprint, CanonicalizationError, BOOLEAN_EXPRESSION_SCHEMA_V1,
    MAX_CANONICAL_EXPRESSION_NODES,
};
pub use control::{
    FreshnessSnapshot, ObservationEpoch, PlannerEpoch, RecommendationContext,
    RecommendationFreshnessError, ResourceGeneration,
};
pub use exact_oracle::{
    ExactBooleanOracle, ExactOracleError, ExactOracleLimits, ExactPropertyReport,
    ExactSatisfiabilityReport, DEFAULT_EXACT_ORACLE_ASSIGNMENTS, DEFAULT_EXACT_ORACLE_VARIABLES,
    MAX_EXACT_ORACLE_ASSIGNMENTS, MAX_EXACT_ORACLE_VARIABLES,
};
pub use frontier::{FrontierError, VersionFrontier};
pub use guard::{
    BooleanGuard, GuardBindingError, GuardFactSource, GuardScope, GuardedResourceSpec,
    TransitionGuard,
};
pub use invariant_predicate::InvariantPredicateBinding;
pub use kleene_oracle::{
    ExactKleeneOracle, KleeneOracleError, KleenePropertyReport, KleeneSatisfiabilityReport,
};
pub use logic::{
    BoolExpr, CompiledGuard, FactMask, FactSet, LogicError, PredicateId, TruthValue,
    FAST_PREDICATE_CAPACITY, MAX_BOOLEAN_EXPR_DEPTH,
};
pub use multiword::{
    MultiwordFactError, MultiwordFactSet, MultiwordFactWord, MAX_MULTIWORD_FACT_PREDICATES,
    MAX_MULTIWORD_FACT_WORDS, MULTIWORD_FACT_WORD_BITS,
};
pub use multiword_guard::{MultiwordCompiledGuard, MultiwordGuardError, MultiwordGuardPath};
pub use predicate::{
    PredicateComponent, PredicateComponentError, PredicateKey, PredicateRegistry,
    PredicateRegistryError, BOOLEAN_PREDICATE_SCHEMA_V1, MAX_PREDICATE_COMPONENT_BYTES,
    MAX_REGISTERED_PREDICATES,
};
pub use pseudo_boolean::{
    PseudoBooleanBindingError, PseudoBooleanConstraint, PseudoBooleanConstraintDeclaration,
    PseudoBooleanError, PseudoBooleanRelation, PseudoBooleanScale, WeightedPredicate,
    WeightedPredicateKey, MAX_PSEUDO_BOOLEAN_TERMS, MAX_PSEUDO_BOOLEAN_UNIT_BYTES,
};
pub use pseudo_boolean_oracle::{
    ExactPseudoBooleanOracle, ExactPseudoBooleanOracleError, ExactPseudoBooleanReport,
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
pub use resource_policy_analysis::{
    analyze_resource_policy, InvariantGuardDiagnostic, InvariantGuardStatus,
    ResourcePolicyAnalysis, ResourcePolicyAnalysisError, TransitionPairAnalysis,
    TransitionPolicyAnalysis, MAX_RESOURCE_POLICY_INVARIANT_BINDINGS, MAX_RESOURCE_POLICY_PAIRS,
    MAX_RESOURCE_POLICY_TRANSITIONS,
};
pub use symbolic_backend::{
    SymbolicBackend, SymbolicBackendConfig, SymbolicBackendConfigError, SymbolicBackendResult,
    SymbolicResource, DEFAULT_SYMBOLIC_MAX_CLAUSES, DEFAULT_SYMBOLIC_MAX_MEMORY_BYTES,
    DEFAULT_SYMBOLIC_MAX_NODES, DEFAULT_SYMBOLIC_SEED, DEFAULT_SYMBOLIC_TIMEOUT_MILLIS,
};
