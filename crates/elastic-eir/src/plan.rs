//! Extensible planning interface over validated EIR.
//!
//! The interface defines **what a planner may say**, not how to plan. Every
//! outcome is one of four honest answers, and a candidate can only describe a
//! transition that the resource itself admits — planners select among
//! declared admissibility, they never invent it.
//!
//! No optimization algorithm lives here yet (whitepaper §6: the planner is
//! never the authority that makes an illegal transition legal). The included
//! [`FirstGroundedPlanner`] is a deliberately trivial deterministic selector
//! that demonstrates the contract end-to-end; it weighs no objectives.

use crate::resource::{AdmittedTransition, EirResource};
use elastic_core::TransitionMechanism;
use std::fmt;

/// One proposed transition.
///
/// Constructed by cloning entries of [`EirResource::transitions`]; the fields
/// are private so a candidate cannot be fabricated outside an EIR node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionCandidate {
    mechanism: TransitionMechanism,
    dimension: elastic_core::resource::DimensionId,
    capability_grounded: bool,
}

impl TransitionCandidate {
    /// Wrap an entry of the resource's own admitted set.
    ///
    /// Soundly public: [`AdmittedTransition`] cannot be constructed outside
    /// the IR, so candidates can only ever restate declared admissibility.
    #[must_use]
    pub fn from_admitted(admitted: &AdmittedTransition) -> Self {
        Self {
            mechanism: admitted.transition().mechanism(),
            dimension: admitted.transition().dimension().clone(),
            capability_grounded: admitted.capability_grounded(),
        }
    }

    /// The proposed mechanism.
    #[must_use]
    pub const fn mechanism(&self) -> TransitionMechanism {
        self.mechanism
    }

    /// The dimension the proposal moves along.
    #[must_use]
    pub const fn dimension(&self) -> &elastic_core::resource::DimensionId {
        &self.dimension
    }

    /// Whether the declaration requires a trusted capability for exactly this
    /// transition.
    #[must_use]
    pub const fn capability_grounded(&self) -> bool {
        self.capability_grounded
    }

    /// Whether this candidate is declared by `resource` as an admitted,
    /// capability-grounded transition.
    ///
    /// Custom planners should verify their output with this method before
    /// returning it; [`FirstGroundedPlanner`] output always passes.
    #[must_use]
    pub fn is_declared_in(&self, resource: &EirResource) -> bool {
        resource.transitions().iter().any(|admitted| {
            admitted.transition().mechanism() == self.mechanism
                && admitted.transition().dimension() == &self.dimension
                && admitted.capability_grounded() == self.capability_grounded
        })
    }
}

impl fmt::Display for TransitionCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mechanism = match self.mechanism {
            TransitionMechanism::Reinterpret => "reinterpret",
            TransitionMechanism::Reencode => "reencode",
            TransitionMechanism::Recompute => "recompute",
        };
        write!(f, "{}@{}", mechanism, self.dimension)
    }
}

/// The honest outcome space of a planning request.
///
/// A planner that cannot justify an answer must return
/// [`PlanOutcome::InsufficientEvidence`] or [`PlanOutcome::Unsupported`]
/// rather than silently picking a transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanOutcome {
    /// A concrete, declared, capability-grounded transition candidate.
    Candidate(TransitionCandidate),
    /// The resource declares admissible transitions, but current evidence or
    /// constraints do not justify selecting any of them. The detail text is
    /// diagnostic only; the variant itself carries the semantics.
    InsufficientEvidence { detail: String },
    /// The request is outside the declaration's vocabulary entirely (for
    /// example, nothing is admitted at all).
    Unsupported,
    /// Admissible transitions exist but none is selectable right now.
    NoCandidate,
}

impl PlanOutcome {
    /// Whether this outcome carries a candidate that is declared by
    /// `resource`.
    #[must_use]
    pub fn declares_valid_candidate(&self, resource: &EirResource) -> bool {
        match self {
            Self::Candidate(candidate) => candidate.is_declared_in(resource),
            _ => false,
        }
    }
}

impl fmt::Display for PlanOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Candidate(candidate) => write!(f, "candidate {}", candidate),
            Self::InsufficientEvidence { detail } => {
                write!(f, "insufficient evidence: {detail}")
            }
            Self::Unsupported => write!(f, "unsupported"),
            Self::NoCandidate => write!(f, "no candidate"),
        }
    }
}

/// A planner proposing at most one transition for a validated EIR resource.
///
/// Implementations must uphold two contracts:
///
/// 1. candidates come from `resource`'s own admitted transitions and are
///    capability-grounded (verifiable via
///    [`PlanOutcome::declares_valid_candidate`]);
/// 2. the same inputs produce the same output (determinism).
///
/// Planning never executes anything and never bypasses validation; execution
/// remains gated by the resource adapter's trusted boundary.
pub trait TransitionPlanner {
    /// Propose at most one transition for `resource`.
    fn propose_transition(&self, resource: &EirResource) -> PlanOutcome;
}

/// Trivial reference planner: deterministically selects the first
/// capability-grounded admitted transition in canonical order.
///
/// This is plumbing, not policy: it weighs no objectives and performs no
/// search. Its decision table:
///
/// 1. no admitted transitions → [`PlanOutcome::Unsupported`];
/// 2. grounded transitions exist → first in canonical order;
/// 3. only ungrounded admissions → [`PlanOutcome::InsufficientEvidence`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FirstGroundedPlanner;

impl TransitionPlanner for FirstGroundedPlanner {
    fn propose_transition(&self, resource: &EirResource) -> PlanOutcome {
        if resource.transitions().is_empty() {
            return PlanOutcome::Unsupported;
        }
        match resource
            .transitions()
            .iter()
            .find(|admitted| admitted.capability_grounded())
        {
            Some(admitted) => PlanOutcome::Candidate(TransitionCandidate::from_admitted(admitted)),
            None => PlanOutcome::InsufficientEvidence {
                detail: "every admitted transition lacks a required capability".to_owned(),
            },
        }
    }
}
