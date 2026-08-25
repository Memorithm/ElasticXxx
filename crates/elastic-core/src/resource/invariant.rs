//! Invariants: properties that adaptation is forbidden to violate.

use super::terms::{ContractId, DimensionId};
use std::fmt;

/// The semantic content of an [`Invariant`].
///
/// Kinds are typed rather than textual; extension happens through
/// [`InvariantKind::UpholdContract`] with a validated [`ContractId`], or
/// through future dedicated kinds, never through free-form strings.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InvariantKind {
    /// The contents carried by the resource must be preserved across
    /// transitions.
    PreserveContents,
    /// Transitions must not split or rename the logical resource: the same
    /// logical identity must survive every admitted adaptation.
    PreserveIdentity,
    /// An externally defined semantic contract must continue to hold.
    ///
    /// The core records the promise; the owning adapter defines what it means
    /// to check it.
    UpholdContract(ContractId),
}

impl fmt::Display for InvariantKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PreserveContents => write!(f, "preserve-contents"),
            Self::PreserveIdentity => write!(f, "preserve-identity"),
            Self::UpholdContract(contract) => write!(f, "uphold-contract({contract})"),
        }
    }
}

/// A property that adaptation is forbidden to violate.
///
/// Invariants are constraints, not objectives: a planner may rank candidate
/// transitions by objectives, but no objective can outweigh an invariant.
///
/// An invariant either applies to every transition of the resource or only to
/// transitions moving along one elastic dimension (see [`Invariant::along`]).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Invariant {
    kind: InvariantKind,
    scope: Option<DimensionId>,
}

impl Invariant {
    /// Declare an invariant that applies to every transition of the resource.
    #[must_use]
    pub const fn new(kind: InvariantKind) -> Self {
        Self { kind, scope: None }
    }

    /// Scope the invariant to transitions that move along `dimension`.
    ///
    /// Scoping an invariant to a dimension that is not declared elastic is
    /// rejected at build time: such a dimension can never change, so the
    /// scoped invariant could never apply.
    #[must_use]
    pub fn along(mut self, dimension: DimensionId) -> Self {
        self.scope = Some(dimension);
        self
    }

    /// The invariant's semantic content.
    #[must_use]
    pub const fn kind(&self) -> &InvariantKind {
        &self.kind
    }

    /// The dimension this invariant constrains, if scoped.
    #[must_use]
    pub const fn scope(&self) -> Option<&DimensionId> {
        self.scope.as_ref()
    }
}

impl fmt::Display for Invariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.scope {
            Some(dimension) => write!(f, "{} along {}", self.kind, dimension),
            None => write!(f, "{}", self.kind),
        }
    }
}
