//! Trusted transaction boundary between planning and physical actuation.
//!
//! Planners are advisory. Implementations of [`TransactionalActuator`] own the
//! resource-specific knowledge required to validate invariants, prepare and
//! perform an effect, verify the resulting state, and either commit or roll it
//! back.

use crate::{
    Actuation, CommitRecord, InvariantCheck, Plan, RollbackRecord, RuntimeError, ValidatedPlan,
    VerificationResult,
};

/// Trusted runtime boundary for one resource adapter.
///
/// Implementations must re-check action-time feasibility rather than trusting
/// planner output. Returning an error from any phase never authorizes the
/// runtime to report a commit.
pub trait TransactionalActuator {
    /// Human-readable adapter identity used in audit records.
    fn name(&self) -> &str;

    /// Validate hard invariants and action-time preconditions for `plan`.
    ///
    /// Every invariant applicable to the candidate transition must have a
    /// corresponding [`InvariantCheck`]. Physical feasibility failures should
    /// be returned as [`RuntimeError::Validation`].
    fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError>;

    /// Prepare the concrete actuation without applying its physical effect.
    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError>;

    /// Apply the prepared physical effect.
    fn actuate(&mut self, actuation: &Actuation) -> Result<(), RuntimeError>;

    /// Verify the post-actuation state.
    fn verify(&self, actuation: &Actuation) -> Result<VerificationResult, RuntimeError>;

    /// Commit a verified actuation.
    fn commit(&mut self, actuation: &Actuation) -> Result<CommitRecord, RuntimeError>;

    /// Roll back an actuation that could not be verified.
    fn rollback(
        &mut self,
        actuation: &Actuation,
        verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError>;
}
