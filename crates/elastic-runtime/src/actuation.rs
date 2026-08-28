//! Actuation types for validated plans.

use crate::plan::ValidatedPlan;

/// Actuation ready for adapter execution.
#[derive(Clone, Debug, PartialEq)]
pub struct Actuation {
    pub plan: ValidatedPlan,
    pub target: Option<u64>,
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
