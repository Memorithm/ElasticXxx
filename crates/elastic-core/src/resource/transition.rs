//! Admissible transitions and capability requirements.
//!
//! Declaring an elastic dimension says *what* may change; admissible
//! transitions say *how* it may be changed. An intended target state is never
//! automatically legal: only transitions admitted here, and later validated
//! against trusted capabilities, may be applied.

use super::terms::DimensionId;
use std::fmt;

pub use crate::representation::TransitionMechanism;

/// One admitted way of changing the resource along one elastic dimension.
///
/// The mechanism vocabulary is shared with the representation layer:
/// [`TransitionMechanism::Reinterpret`], [`TransitionMechanism::Reencode`],
/// and [`TransitionMechanism::Recompute`] describe whether a transition reuses
/// the existing materialization, transforms it in place, or regenerates it
/// from a trusted source.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdmissibleTransition {
    mechanism: TransitionMechanism,
    dimension: DimensionId,
}

impl AdmissibleTransition {
    /// Admit `mechanism` as a legal way of moving along `dimension`.
    #[must_use]
    pub const fn new(mechanism: TransitionMechanism, dimension: DimensionId) -> Self {
        Self {
            mechanism,
            dimension,
        }
    }

    /// The admitted mechanism.
    #[must_use]
    pub const fn mechanism(&self) -> TransitionMechanism {
        self.mechanism
    }

    /// The dimension along which the mechanism is admitted.
    #[must_use]
    pub const fn dimension(&self) -> &DimensionId {
        &self.dimension
    }
}

impl fmt::Display for AdmissibleTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", mechanism_text(self.mechanism), self.dimension)
    }
}

/// Canonical text of the shared mechanism vocabulary.
const fn mechanism_text(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

/// A capability the runtime boundary must provide so that a declared
/// admissible transition can actually be executed.
///
/// Applications may *require* capabilities; they cannot fabricate them. A
/// requirement states that a trusted adapter must exist which can apply the
/// given mechanism along the given dimension. Whether such an adapter exists
/// is discovered by the trusted runtime, mirroring the capability/attestation
/// discipline of the representation layer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityRequirement {
    mechanism: TransitionMechanism,
    dimension: DimensionId,
}

impl CapabilityRequirement {
    /// Require a trusted adapter able to apply `mechanism` along `dimension`.
    #[must_use]
    pub const fn new(mechanism: TransitionMechanism, dimension: DimensionId) -> Self {
        Self {
            mechanism,
            dimension,
        }
    }

    /// The required mechanism.
    #[must_use]
    pub const fn mechanism(&self) -> TransitionMechanism {
        self.mechanism
    }

    /// The dimension along which the mechanism must be applicable.
    #[must_use]
    pub const fn dimension(&self) -> &DimensionId {
        &self.dimension
    }
}

impl fmt::Display for CapabilityRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", mechanism_text(self.mechanism), self.dimension)
    }
}
