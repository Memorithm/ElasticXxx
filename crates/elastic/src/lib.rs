//! User-facing facade for the ElasticXxx Rust surface model.
//!
//! This crate re-exports, in one place:
//!
//! - the typed declaration API ([`resource`], [`ResourceSpec`], …) from
//!   `elastic-core`;
//! - deterministic lowering to EIR ([`lower`], [`EirDocument`]) from
//!   `elastic-eir`;
//! - the [`ElasticResource`] derive macro from `elastic-macros`.
//!
//! The facade owns no semantics of its own: everything it exposes lowers to
//! the single typed implementation in `elastic-core`.

#![forbid(unsafe_code)]

pub use elastic_core::resource;
pub use elastic_core::resource::{
    AdmissibleTransition, BuiltinDimension, BuiltinObjective, BuiltinObservationSignal,
    BuiltinResourceClass, CapabilityRequirement, ContractId, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
    ResourceSpecBuilder, ResourceSpecError,
};
pub use elastic_core::TransitionMechanism;
pub use elastic_eir::{lower, EirDocument, EirDocumentBuilder, EirResource};
pub use elastic_macros::ElasticResource;

/// Everything a typical Elastic declaration needs, importable in one line.
pub mod prelude {
    pub use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, ContractId, DimensionId, Invariant,
        InvariantKind, LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId,
        ResourceSpec, ResourceSpecError,
    };
    pub use elastic_core::TransitionMechanism;
    pub use elastic_eir::{lower, EirDocument};
    pub use elastic_macros::ElasticResource;
}
