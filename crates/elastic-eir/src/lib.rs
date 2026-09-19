//! Normalized Elastic Intermediate Representation (EIR) v0.1.
//!
//! EIR is the deterministic, inspectable data form of declared Elastic intent.
//! It sits between the Rust surface model ([`elastic_core::resource`]) and any
//! future planning/validation consumers:
//!
//! ```text
//! ResourceSpec (typed declaration)
//!        ↓ lower()      — normalize
//! EirDocument           — validated, versioned, fingerprinted pure data
//! ```
//!
//! Boolean guards are carried by a separate versioned envelope
//! ([`EirGuardedResource`]) so adding policy data does not silently mutate the
//! historical EIR v0.1 resource schema or its fingerprints.
//!
//! # Neutrality
//!
//! The IR is backend-, runtime-, hardware-, and OS-neutral. It contains no
//! CUDA/WGPU concepts, no LLM assumptions, no process handles, and no
//! executable behavior: it *represents* transitions and requirements, it never
//! performs them.
//!
//! Term vocabularies ([`elastic_core::resource::DimensionId`] and friends) are
//! reused from the surface model instead of being redefined, so there is one
//! semantic implementation end to end. EIR neutrality comes from its shape:
//! sorted plain data with an explicit schema version and structural
//! fingerprints.
//!
//! # Determinism
//!
//! Equivalent declarations lower to identical documents: all unordered
//! collections are sorted, objective priorities become explicit ranks,
//! fingerprints absorb fields in a fixed order, and nothing depends on hash
//! iteration order, addresses, threads, or randomness. Fingerprints are
//! FNV-1a **structural fingerprints** for identity checks, caching, and tests;
//! they are not cryptographic and must never be used across trust domains.
//!
//! # Validation
//!
//! Every construction path validates. Documents built from surface specs,
//! assembled from parts by tools, or extended later all pass the same
//! structural validation, so invalid EIR cannot silently reach runtime
//! planning. EIR v0.1 additionally requires capability requirements to be
//! grounded in an admitted transition of the same resource.

#![forbid(unsafe_code)]

//! # Examples
//!
//! Lower a typed surface declaration into a validated document:
//!
//! ```
//! use elastic_core::resource::{
//!     AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, InvariantKind,
//!     LogicalResourceId, ObjectiveId, ResourceClassId, ResourceSpec,
//! };
//! use elastic_core::TransitionMechanism;
//! use elastic_eir::lower;
//!
//! # fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let spec = ResourceSpec::builder(
//!         ResourceClassId::REPRESENTATIONAL,
//!         LogicalResourceId::new("session-kv")?,
//!     )
//!     .allow(DimensionId::REPRESENTATION)
//!     .preserve(Invariant::new(InvariantKind::PreserveContents))
//!     .optimize(ObjectiveId::LATENCY)
//!     .admit(AdmissibleTransition::new(
//!         TransitionMechanism::Reencode,
//!         DimensionId::REPRESENTATION,
//!     ))
//!     .require_capability(CapabilityRequirement::new(
//!         TransitionMechanism::Reencode,
//!         DimensionId::REPRESENTATION,
//!     ))
//!     .build()?;
//!
//! let document = lower(&spec)?;
//! assert_eq!(document.schema_version(), elastic_eir::SchemaVersion::LATEST);
//!
//! let resource = document.resource("session-kv").expect("resource present");
//! assert!(resource.transitions()[0].capability_grounded());
//! assert_eq!(
//!     resource.objective_ranking()[0].objective(),
//!     &ObjectiveId::LATENCY
//! );
//! # Ok(())
//! # }
//! # run().unwrap();
//! ```

mod constraint;
mod document;
mod error;
mod fingerprint;
mod group;
mod guard;
mod guard_plan;
mod plan;
mod planning_subset;
mod policy;
mod resource;
mod resource_policy;
mod validate;

pub use constraint::{
    lower_constrained, ConstraintLoweringError, EirConstrainedResource, EirPseudoBooleanConstraint,
    EirPseudoBooleanTerm, EIR_PSEUDO_BOOLEAN_CONSTRAINT_SCHEMA_VERSION,
    MAX_EIR_PSEUDO_BOOLEAN_CONSTRAINTS,
};
pub use document::{lower, EirDocument, EirDocumentBuilder, MAX_EIR_DOCUMENT_RESOURCES};
pub use error::ValidationError;
pub use fingerprint::Fingerprint;
pub use group::{
    EirCrossResourceInvariant, EirGroupedDocument, EirResourceDependency, EirResourceGroup,
    EirSharedBudget, EirSharedBudgetTerm, GroupLoweringError, EIR_RESOURCE_GROUP_SCHEMA_VERSION,
    MAX_EIR_RESOURCE_GROUPS,
};
pub use guard::{
    lower_guarded, EirGuard, EirGuardedResource, EirPredicate, EIR_BOOLEAN_GUARD_SCHEMA_VERSION,
};
pub use guard_plan::{
    evaluate_transition_guards, prune_transition_candidates, GuardedTransitionOutcome,
    RejectedTransition, TransitionPruningReport, UnknownTransition,
};
pub use plan::{
    FirstGroundedPlanner, PlanOutcome, PlanningContext, TransitionCandidate, TransitionPlanner,
};
pub use planning_subset::PlanningSubsetError;
pub use policy::{EirPolicyHeader, PolicyLoweringError, EIR_POLICY_HEADER_SCHEMA_VERSION};
pub use resource::{AdmittedTransition, EirResource, EirResourceParts, ObjectiveRank};
pub use resource_policy::{
    lower_resource_policy, EirResourcePolicy, ResourcePolicyLoweringError,
    EIR_RESOURCE_POLICY_SCHEMA_VERSION,
};
pub use validate::validate_resource_parts;

/// Current EIR schema version produced by the legacy unguarded document crate.
pub const EIR_SCHEMA_VERSION: u16 = 1;

/// Explicit schema version carried by every [`EirDocument`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion(u16);

impl SchemaVersion {
    /// The schema version this crate produces.
    pub const LATEST: Self = Self(EIR_SCHEMA_VERSION);

    /// Wrap a raw schema version.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Raw version number.
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "eir-v{}", self.0)
    }
}
