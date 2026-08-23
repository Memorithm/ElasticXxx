//! Two-phase commit/rollback frontier for representational resources.
//!
//! This module implements the PLAN → VALIDATE → COMMIT / ROLLBACK portion of
//! the provisional Elastic control loop for a single resource: proposals are
//! staged against a committed state, validated structurally, and either
//! committed (advancing the frontier) or rolled back (leaving the committed
//! state untouched).

use crate::representation::{
    CapabilitySet, RepresentationState, RepresentationTransition, TransitionAttestations,
    TransitionError, TransitionMechanism,
};
use std::fmt;

/// A staged, not-yet-committed transition candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionFrontier {
    committed: RepresentationState,
    pending: Option<RepresentationTransition>,
}

impl VersionFrontier {
    /// Start a frontier at the given materialized state.
    pub const fn new(committed: RepresentationState) -> Self {
        Self {
            committed,
            pending: None,
        }
    }

    /// The currently committed state.
    pub const fn committed(&self) -> &RepresentationState {
        &self.committed
    }

    /// The staged proposal, if any.
    pub const fn pending(&self) -> Option<&RepresentationTransition> {
        self.pending.as_ref()
    }

    /// Stage a proposal from the committed state to `to` via `mechanism`.
    ///
    /// Staging does not validate; validation happens explicitly through
    /// [`VersionFrontier::validate_pending`] or at commit time. A proposal
    /// must be resolved (committed or rolled back) before another one can be
    /// staged, keeping every decision point explicit.
    pub fn propose(
        &mut self,
        to: RepresentationState,
        mechanism: TransitionMechanism,
    ) -> Result<(), FrontierError> {
        if self.pending.is_some() {
            return Err(FrontierError::ProposalAlreadyStaged);
        }
        self.pending = Some(RepresentationTransition {
            from: self.committed.clone(),
            to,
            mechanism,
        });
        Ok(())
    }

    /// Structurally validate the staged proposal without committing it.
    pub fn validate_pending(
        &self,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
    ) -> Result<(), FrontierError> {
        let transition = self.pending.as_ref().ok_or(FrontierError::NoProposal)?;
        transition
            .validate(capabilities, attestations)
            .map_err(FrontierError::Core)
    }

    /// Validate and commit the staged proposal.
    ///
    /// On success the committed state advances to the proposal's target and
    /// the proposal is consumed. On validation failure the committed state is
    /// untouched and the proposal remains staged so the caller may retry with
    /// corrected capabilities/attestations.
    pub fn commit(
        &mut self,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
    ) -> Result<&RepresentationState, FrontierError> {
        let transition = self.pending.as_ref().ok_or(FrontierError::NoProposal)?;
        transition
            .validate(capabilities, attestations)
            .map_err(FrontierError::Core)?;
        self.committed = transition.to.clone();
        self.pending = None;
        Ok(&self.committed)
    }

    /// Discard the staged proposal, returning it. The committed state is
    /// unchanged.
    pub fn rollback(&mut self) -> Option<RepresentationTransition> {
        self.pending.take()
    }
}

/// Errors raised by [`VersionFrontier`] control-flow (as opposed to contract
/// violations, which surface as [`FrontierError::Core`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontierError {
    /// No proposal is staged.
    NoProposal,
    /// A proposal is already staged and must be committed or rolled back
    /// before staging another.
    ProposalAlreadyStaged,
    /// The staged proposal failed structural transition validation.
    Core(TransitionError),
}

impl fmt::Display for FrontierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProposal => write!(f, "no transition proposal is staged"),
            Self::ProposalAlreadyStaged => write!(
                f,
                "a transition proposal is already staged; commit or roll it back first"
            ),
            Self::Core(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for FrontierError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Core(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::representation::{RepresentationEpoch, RepresentationId};

    fn state(name: &str, epoch: u64) -> RepresentationState {
        RepresentationState::new(
            RepresentationId::new(name).unwrap(),
            1,
            RepresentationEpoch::new(epoch),
        )
    }

    fn caps_for(state: &RepresentationState) -> CapabilitySet {
        let mut caps = CapabilitySet::new();
        caps.insert(state.id.clone(), state.schema_version);
        caps
    }

    #[test]
    fn propose_commit_advances_frontier_and_consumes_proposal() {
        let seed = state("kv.raw", 1);
        let target = state("kv.int4", 2);
        let mut frontier = VersionFrontier::new(seed.clone());
        frontier
            .propose(target.clone(), TransitionMechanism::Reencode)
            .unwrap();

        assert!(frontier.pending().is_some());
        let committed = frontier
            .commit(
                &caps_for(&target),
                TransitionAttestations::default().attest_reencoder_available(),
            )
            .unwrap();
        assert_eq!(committed, &target);
        assert!(frontier.pending().is_none());
    }

    #[test]
    fn commit_without_proposal_is_rejected() {
        let mut frontier = VersionFrontier::new(state("kv.raw", 1));
        assert_eq!(
            frontier.commit(&CapabilitySet::new(), TransitionAttestations::default()),
            Err(FrontierError::NoProposal)
        );
    }

    #[test]
    fn second_proposal_requires_resolving_the_first() {
        let mut frontier = VersionFrontier::new(state("kv.raw", 1));
        frontier
            .propose(state("kv.int4", 2), TransitionMechanism::Reencode)
            .unwrap();
        assert_eq!(
            frontier.propose(state("kv.int8", 3), TransitionMechanism::Reencode),
            Err(FrontierError::ProposalAlreadyStaged)
        );
    }

    #[test]
    fn failed_commit_keeps_committed_state_and_staged_proposal() {
        let seed = state("kv.raw", 1);
        let target = state("kv.int4", 2);
        let mut frontier = VersionFrontier::new(seed.clone());
        frontier
            .propose(target, TransitionMechanism::Reencode)
            .unwrap();

        assert_eq!(
            frontier.validate_pending(&CapabilitySet::new(), TransitionAttestations::default()),
            Err(FrontierError::Core(TransitionError::UnsupportedTarget {
                id: RepresentationId::new("kv.int4").unwrap(),
                schema_version: 1,
            }))
        );

        assert_eq!(frontier.committed(), &seed);
        assert!(frontier.pending().is_some());

        frontier.rollback();
        assert_eq!(frontier.committed(), &seed);
        assert!(frontier.pending().is_none());
    }

    #[test]
    fn rollback_returns_the_discarded_proposal() {
        let mut frontier = VersionFrontier::new(state("kv.raw", 1));
        assert!(frontier.rollback().is_none());
        frontier
            .propose(state("kv.int4", 2), TransitionMechanism::Reencode)
            .unwrap();
        let discarded = frontier.rollback().unwrap();
        assert_eq!(discarded.from, state("kv.raw", 1));
        assert_eq!(discarded.to, state("kv.int4", 2));
    }

    #[test]
    fn repeated_cycles_advance_epochs_monotonically() {
        use crate::representation::TargetContract;

        let mut frontier = VersionFrontier::new(state("kv.raw", 0));
        for cycle in 0..8u64 {
            let target = frontier
                .committed()
                .derive_target(TargetContract::Same, TransitionMechanism::Reencode)
                .unwrap();
            assert_eq!(target.epoch.get(), cycle + 1);
            frontier
                .propose(target, TransitionMechanism::Reencode)
                .unwrap();
            frontier
                .commit(
                    &caps_for(&state("kv.raw", cycle + 1)),
                    TransitionAttestations::default().attest_reencoder_available(),
                )
                .unwrap();
            assert!(frontier.committed().epoch.get() > cycle);
        }
    }
}
