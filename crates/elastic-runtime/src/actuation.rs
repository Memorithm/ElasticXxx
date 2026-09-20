//! Actuation types for validated plans.

use crate::plan::ValidatedPlan;

/// Actuation ready for adapter execution.
#[derive(Clone, Debug, PartialEq)]
pub struct Actuation {
    /// Exact validated plan that authorized preparation.
    pub plan: ValidatedPlan,
    /// Optional numeric magnitude of the selected candidate.
    ///
    /// This must equal `plan.plan.candidate().and_then(|candidate| candidate.magnitude())`.
    /// Adapters may not substitute a second target at the prepare boundary.
    pub target: Option<u64>,
    /// Identity of the adapter that prepared this actuation.
    pub adapter_name: String,
}

impl Actuation {
    pub fn new(plan: ValidatedPlan, target: Option<u64>, adapter_name: impl Into<String>) -> Self {
        Self {
            plan,
            target,
            adapter_name: adapter_name.into(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.plan.validated && self.plan.plan.candidate().is_some()
    }
}
