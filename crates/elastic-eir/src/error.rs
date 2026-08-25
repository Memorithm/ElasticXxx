//! Structured EIR validation errors.

use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, ObjectiveId,
    ObservationSignalId,
};
use std::fmt;

/// Structural validation failures for EIR content.
///
/// These are the only way an [`EirResource`](crate::EirResource) or
/// [`EirDocument`](crate::EirDocument) construction can fail; invalid EIR is
/// never constructed silently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    /// The document contained no resources.
    EmptyDocument,
    /// Two resources in one document shared a logical identity.
    DuplicateResourceIdentity {
        /// The repeated identity text.
        identity: String,
    },
    /// The logical resource identity text was blank.
    EmptyResourceIdentity,
    /// A diagnostic label key was blank.
    InvalidLabelKey,
    /// No elastic dimension was declared.
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
    /// An invariant was scoped to a dimension that cannot change.
    VacuousInvariant {
        /// The vacuous invariant.
        invariant: Invariant,
    },
    /// The same transition was admitted more than once.
    DuplicateTransition {
        /// The repeated admission.
        transition: AdmissibleTransition,
    },
    /// A transition concerns a dimension that is not declared elastic.
    TransitionBeyondElasticDimensions {
        /// The misplaced admission.
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
    /// A capability requirement has no matching admitted transition.
    ///
    /// EIR v0.1 normative rule: every required capability must ground at least
    /// one admissible transition of the same resource (same mechanism and
    /// dimension).
    CapabilityNotGroundedInAdmission {
        /// The ungrounded requirement.
        requirement: CapabilityRequirement,
    },
    /// The same observation signal was declared more than once.
    DuplicateObservation {
        /// The repeated signal.
        signal: ObservationSignalId,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDocument => write!(f, "an EIR document must contain at least one resource"),
            Self::DuplicateResourceIdentity { identity } => write!(
                f,
                "logical resource identity {identity} appears more than once in one document"
            ),
            Self::EmptyResourceIdentity => write!(f, "logical resource identity must not be empty"),
            Self::InvalidLabelKey => write!(f, "diagnostic label key must not be empty"),
            Self::NoElasticDimensions => write!(
                f,
                "an EIR resource must declare at least one elastic dimension"
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
            Self::DuplicateTransition { transition } => {
                write!(f, "admitted transition {transition} declared more than once")
            }
            Self::TransitionBeyondElasticDimensions { transition } => write!(
                f,
                "admitted transition {transition} concerns a dimension that is not declared elastic"
            ),
            Self::DuplicateCapabilityRequirement { requirement } => {
                write!(f, "capability requirement {requirement} declared more than once")
            }
            Self::CapabilityBeyondElasticDimensions { requirement } => write!(
                f,
                "capability requirement {requirement} concerns a dimension that is not declared elastic"
            ),
            Self::CapabilityNotGroundedInAdmission { requirement } => write!(
                f,
                "capability requirement {requirement} does not match any admitted transition; require capabilities only for transitions the resource admits"
            ),
            Self::DuplicateObservation { signal } => {
                write!(f, "observation signal {signal} declared more than once")
            }
        }
    }
}

impl std::error::Error for ValidationError {}
