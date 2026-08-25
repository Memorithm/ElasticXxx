//! Structured errors for the general Elastic resource model.
//!
//! All fallible operations of the [`crate::resource`] module return these
//! variants instead of panicking. Variants carry the offending typed value so
//! callers can programmatically identify the rejected declaration fragment.

use super::terms::{DimensionId, ObjectiveId};
use super::{AdmissibleTransition, CapabilityRequirement, Invariant, ObservationSignalId};
use std::fmt;

/// Category of an extensible identifier whose custom text was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TermKind {
    /// Elastic-dimension identifier.
    Dimension,
    /// Optimization-objective identifier.
    Objective,
    /// Resource-class identifier.
    ResourceClass,
    /// Observation-signal identifier.
    ObservationSignal,
    /// External semantic-contract identifier.
    Contract,
}

impl fmt::Display for TermKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dimension => write!(f, "dimension"),
            Self::Objective => write!(f, "objective"),
            Self::ResourceClass => write!(f, "resource class"),
            Self::ObservationSignal => write!(f, "observation signal"),
            Self::Contract => write!(f, "semantic contract"),
        }
    }
}

/// Errors produced while constructing or validating general resource
/// declarations.
///
/// Validation is structural: it proves properties of the declaration itself
/// (uniqueness, internal consistency, bounded identifiers). It does not solve
/// planning or satisfiability problems, and it does not authenticate
/// capabilities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceSpecError {
    /// A logical resource identifier was blank.
    EmptyResourceId,
    /// A custom extension identifier was blank.
    InvalidCustomTerm {
        /// Which kind of identifier was rejected.
        term_kind: TermKind,
    },
    /// A diagnostic label key was blank.
    InvalidLabelKey,
    /// No elastic dimension was declared. An elastic resource declaration
    /// must expose at least one dimension along which adaptation may occur;
    /// a fully rigid resource gains nothing from the Elastic model.
    NoElasticDimensions,
    /// The same elastic dimension was declared more than once.
    DuplicateDimension {
        /// The repeated dimension.
        dimension: DimensionId,
    },
    /// The same objective was declared more than once.
    DuplicateObjective {
        /// The repeated objective.
        objective: ObjectiveId,
    },
    /// The same invariant was declared more than once.
    DuplicateInvariant {
        /// The repeated invariant.
        invariant: Invariant,
    },
    /// An invariant was scoped to a dimension that is not declared elastic.
    ///
    /// A non-elastic dimension cannot change, so such a scoped invariant is
    /// vacuous and almost certainly signals a mistaken declaration.
    VacuousInvariant {
        /// The vacuous invariant.
        invariant: Invariant,
    },
    /// The same admissible transition was declared more than once.
    DuplicateAdmissibleTransition {
        /// The repeated transition.
        transition: AdmissibleTransition,
    },
    /// An admissible transition was declared along a dimension that is not
    /// declared elastic.
    TransitionBeyondElasticDimensions {
        /// The misplaced transition declaration.
        transition: AdmissibleTransition,
    },
    /// The same capability requirement was declared more than once.
    DuplicateCapabilityRequirement {
        /// The repeated requirement.
        requirement: CapabilityRequirement,
    },
    /// A capability requirement concerns a dimension that is not declared
    /// elastic.
    CapabilityBeyondElasticDimensions {
        /// The misplaced requirement.
        requirement: CapabilityRequirement,
    },
    /// A capability requirement matches no admitted transition of the
    /// resource (same mechanism and dimension).
    ///
    /// Requiring the ability to execute a transition the declaration never
    /// admits is meaningless intent; declare the admission first.
    UngroundedCapabilityRequirement {
        /// The ungrounded requirement.
        requirement: CapabilityRequirement,
    },
    /// The same observation signal was declared more than once.
    DuplicateObservation {
        /// The repeated signal.
        signal: ObservationSignalId,
    },
}

impl fmt::Display for ResourceSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyResourceId => write!(f, "logical resource identifier must not be empty"),
            Self::InvalidCustomTerm { term_kind } => {
                write!(f, "custom {term_kind} identifier must not be empty")
            }
            Self::InvalidLabelKey => write!(f, "diagnostic label key must not be empty"),
            Self::NoElasticDimensions => write!(
                f,
                "an elastic resource declaration must allow at least one elastic dimension"
            ),
            Self::DuplicateDimension { dimension } => {
                write!(f, "elastic dimension {dimension} declared more than once")
            }
            Self::DuplicateObjective { objective } => {
                write!(f, "objective {objective} declared more than once")
            }
            Self::DuplicateInvariant { invariant } => {
                write!(f, "invariant {invariant} declared more than once")
            }
            Self::VacuousInvariant { invariant } => write!(
                f,
                "invariant {invariant} is scoped to a dimension that is not elastic and therefore can never apply"
            ),
            Self::DuplicateAdmissibleTransition { transition } => {
                write!(f, "admissible transition {transition} declared more than once")
            }
            Self::TransitionBeyondElasticDimensions { transition } => write!(
                f,
                "admissible transition {transition} concerns dimension {} which is not declared elastic",
                transition.dimension()
            ),
            Self::DuplicateCapabilityRequirement { requirement } => {
                write!(f, "capability requirement {requirement} declared more than once")
            }
            Self::CapabilityBeyondElasticDimensions { requirement } => write!(
                f,
                "capability requirement {requirement} concerns dimension {} which is not declared elastic",
                requirement.dimension()
            ),
            Self::UngroundedCapabilityRequirement { requirement } => write!(
                f,
                "capability requirement {requirement} does not match any admitted transition; admit the transition before requiring its capability"
            ),
            Self::DuplicateObservation { signal } => {
                write!(f, "observation signal {signal} declared more than once")
            }
        }
    }
}

impl std::error::Error for ResourceSpecError {}
