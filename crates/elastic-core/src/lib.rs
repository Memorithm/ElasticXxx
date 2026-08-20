//! Core contracts for ElasticXxx.
//!
//! The crate models adaptive resources as explicit state spaces.  The first
//! executable slice focuses on *representational resources*: data whose
//! mathematical/numerical representation may change only through declared,
//! validated transitions.

#![forbid(unsafe_code)]

pub mod representation;

pub use representation::{
    CapabilitySet, RepresentationEpoch, RepresentationId, RepresentationState,
    RepresentationTransition, TransitionError, TransitionFacts, TransitionMechanism,
};
