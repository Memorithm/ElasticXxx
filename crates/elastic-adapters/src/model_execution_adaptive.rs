//! Dynamic runtime planner for resource-guided correlated model execution.
//!
//! This module composes the already-versioned model-execution boundaries:
//!
//! `PlanningContext -> resource snapshot -> envelope policy -> correlated profile
//! -> atomic profile transition`.
//!
//! It does not infer hardware/model relationships. `FREE_CAPACITY` is interpreted
//! in the backend-owned native unit declared by the policy, and `UTILIZATION`
//! follows EIR's generic fractional `0.0..=1.0` convention. The policy remains
//! the authority for thresholds and allowed profile envelopes.

use crate::model_execution_envelope::{
    ModelExecutionEnvelopeError, ModelExecutionEnvelopePolicyV1, ModelExecutionHardwarePlannerV1,
    ModelExecutionHardwareSelectionV1, ModelExecutionResourceSnapshotV1,
};
use crate::model_execution_profiles::ModelExecutionProfileSetV1;
use crate::model_execution_runtime::ModelExecutionAtomicProfilePlannerV1;
use elastic_core::resource::ObservationSignalId;
use elastic_eir::{EirResource, PlanOutcome, PlanningContext, TransitionPlanner};

/// Largest integer that can be represented exactly by an IEEE-754 `f64`.
const MAX_EXACT_F64_INTEGER: f64 = 9_007_199_254_740_992.0;

/// Deterministic planner that resolves current resource observations into one
/// already-qualified atomic model-execution profile transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionAdaptivePlannerV1 {
    policy: ModelExecutionEnvelopePolicyV1,
    profiles: ModelExecutionProfileSetV1,
}

impl ModelExecutionAdaptivePlannerV1 {
    /// Bind one exact envelope policy to its correlated profile set.
    ///
    /// # Errors
    ///
    /// Fails closed if policy identity/fingerprints do not match `profiles`.
    pub fn new(
        policy: ModelExecutionEnvelopePolicyV1,
        profiles: ModelExecutionProfileSetV1,
    ) -> Result<Self, ModelExecutionEnvelopeError> {
        // `select` validates the policy/profile identity before rule matching.
        // A zero-valued probe is used only for identity validation; the result
        // itself is intentionally ignored.
        let probe = ModelExecutionResourceSnapshotV1::new(policy.capacity_unit(), 0, 0)?;
        let _ = ModelExecutionHardwarePlannerV1.select(&policy, &profiles, &probe)?;
        Ok(Self { policy, profiles })
    }

    /// Exact backend-owned capacity unit expected for `FREE_CAPACITY`.
    #[must_use]
    pub fn capacity_unit(&self) -> &str {
        self.policy.capacity_unit()
    }

    /// Exact bound envelope policy.
    #[must_use]
    pub const fn policy(&self) -> &ModelExecutionEnvelopePolicyV1 {
        &self.policy
    }

    /// Exact bound correlated profile set.
    #[must_use]
    pub const fn profiles(&self) -> &ModelExecutionProfileSetV1 {
        &self.profiles
    }

    fn snapshot_from_context(
        &self,
        context: &PlanningContext,
    ) -> Result<ModelExecutionResourceSnapshotV1, String> {
        let free = context
            .get(ObservationSignalId::FREE_CAPACITY)
            .ok_or_else(|| "missing free-capacity observation".to_owned())?;
        let utilization = context
            .get(ObservationSignalId::UTILIZATION)
            .ok_or_else(|| "missing utilization observation".to_owned())?;

        let free_capacity = exact_nonnegative_u64(free).ok_or_else(|| {
            format!(
                "free-capacity observation must be an exact non-negative integer <= 2^53 in policy unit {:?}; got {free}",
                self.policy.capacity_unit()
            )
        })?;
        let utilization_bps = utilization_fraction_to_bps(utilization).ok_or_else(|| {
            format!("utilization observation must be finite in [0, 1]; got {utilization}")
        })?;

        ModelExecutionResourceSnapshotV1::new(
            self.policy.capacity_unit(),
            free_capacity,
            utilization_bps,
        )
        .map_err(|error| error.to_string())
    }
}

impl TransitionPlanner for ModelExecutionAdaptivePlannerV1 {
    fn propose_transition(&self, _resource: &EirResource) -> PlanOutcome {
        PlanOutcome::InsufficientEvidence {
            detail: "adaptive model-execution planner requires free-capacity, utilization, and current-profile observations"
                .to_owned(),
        }
    }

    fn propose_transition_with_context(
        &self,
        resource: &EirResource,
        context: &PlanningContext,
    ) -> PlanOutcome {
        let snapshot = match self.snapshot_from_context(context) {
            Ok(snapshot) => snapshot,
            Err(detail) => return PlanOutcome::InsufficientEvidence { detail },
        };

        let selection =
            match ModelExecutionHardwarePlannerV1.select(&self.policy, &self.profiles, &snapshot) {
                Ok(selection) => selection,
                Err(error) => {
                    return PlanOutcome::InsufficientEvidence {
                        detail: format!("model-execution envelope resolution failed: {error}"),
                    };
                }
            };

        match selection {
            ModelExecutionHardwareSelectionV1::Selected { plan, .. } => {
                ModelExecutionAtomicProfilePlannerV1::new(&plan)
                    .propose_transition_with_context(resource, context)
            }
            ModelExecutionHardwareSelectionV1::NoMatchingRule => PlanOutcome::NoCandidate,
            ModelExecutionHardwareSelectionV1::NoFeasibleProfile { rule_id } => {
                PlanOutcome::InsufficientEvidence {
                    detail: format!(
                        "matched model-execution rule {rule_id:?} has no feasible correlated profile"
                    ),
                }
            }
        }
    }
}

fn exact_nonnegative_u64(value: f64) -> Option<u64> {
    if !value.is_finite() || !(0.0..=MAX_EXACT_F64_INTEGER).contains(&value) || value.fract() != 0.0
    {
        return None;
    }
    Some(value as u64)
}

fn utilization_fraction_to_bps(value: f64) -> Option<u16> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return None;
    }
    // The policy vocabulary is integer basis points. Runtime utilization is a
    // fractional signal, so conversion is explicitly nearest-basis-point.
    Some((value * 10_000.0).round() as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_execution::ModelExecutionCapabilitiesV1;
    use crate::model_execution_envelope::ModelExecutionEnvelopeRuleV1;
    use crate::model_execution_profiles::{
        ModelExecutionProfileEnvelopeV1, ModelExecutionProfileV1,
    };
    use crate::model_execution_runtime::{
        model_execution_current_profile_rank_signal, model_execution_profile_dimension,
    };
    use elastic_eir::{lower, PlanOutcome};

    fn profiles() -> ModelExecutionProfileSetV1 {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
                ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
            ],
        )
        .unwrap()
    }

    fn policy(profiles: &ModelExecutionProfileSetV1) -> ModelExecutionEnvelopePolicyV1 {
        ModelExecutionEnvelopePolicyV1::new(
            profiles,
            "bytes",
            vec![
                ModelExecutionEnvelopeRuleV1::new(
                    "rich",
                    0,
                    8_000,
                    7_000,
                    ModelExecutionProfileEnvelopeV1::new(4, 10_000, 10_000).unwrap(),
                )
                .unwrap(),
                ModelExecutionEnvelopeRuleV1::new(
                    "balanced",
                    10,
                    2_000,
                    9_000,
                    ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
                )
                .unwrap(),
                ModelExecutionEnvelopeRuleV1::new(
                    "survival",
                    20,
                    0,
                    10_000,
                    ModelExecutionProfileEnvelopeV1::new(1, 2_500, 2_500).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap()
    }

    fn resource(profiles: &ModelExecutionProfileSetV1) -> EirResource {
        let spec = profiles.atomic_resource_spec("model-runtime").unwrap();
        lower(&spec)
            .unwrap()
            .resource("model-runtime")
            .unwrap()
            .clone()
    }

    fn context(free: f64, utilization: f64, current_rank: f64) -> PlanningContext {
        PlanningContext::new()
            .observe(ObservationSignalId::FREE_CAPACITY, free)
            .observe(ObservationSignalId::UTILIZATION, utilization)
            .observe(model_execution_current_profile_rank_signal(), current_rank)
    }

    #[test]
    fn rich_evidence_selects_full_profile_atomically() {
        let profiles = profiles();
        let planner =
            ModelExecutionAdaptivePlannerV1::new(policy(&profiles), profiles.clone()).unwrap();
        let resource = resource(&profiles);
        let outcome =
            planner.propose_transition_with_context(&resource, &context(9_000.0, 0.60, 20.0));
        let PlanOutcome::Candidate(candidate) = outcome else {
            panic!("expected atomic full-profile candidate")
        };
        assert_eq!(candidate.dimension(), &model_execution_profile_dimension());
        assert_eq!(candidate.magnitude(), Some(0));
    }

    #[test]
    fn constrained_evidence_selects_balanced_profile() {
        let profiles = profiles();
        let planner =
            ModelExecutionAdaptivePlannerV1::new(policy(&profiles), profiles.clone()).unwrap();
        let resource = resource(&profiles);
        let outcome =
            planner.propose_transition_with_context(&resource, &context(3_000.0, 0.80, 0.0));
        let PlanOutcome::Candidate(candidate) = outcome else {
            panic!("expected atomic balanced-profile candidate")
        };
        assert_eq!(candidate.magnitude(), Some(10));
    }

    #[test]
    fn selected_current_profile_is_noop() {
        let profiles = profiles();
        let planner =
            ModelExecutionAdaptivePlannerV1::new(policy(&profiles), profiles.clone()).unwrap();
        let resource = resource(&profiles);
        assert_eq!(
            planner.propose_transition_with_context(&resource, &context(3_000.0, 0.80, 10.0),),
            PlanOutcome::NoCandidate
        );
    }

    #[test]
    fn missing_or_ambiguous_resource_evidence_fails_closed() {
        let profiles = profiles();
        let planner =
            ModelExecutionAdaptivePlannerV1::new(policy(&profiles), profiles.clone()).unwrap();
        let resource = resource(&profiles);
        let missing = PlanningContext::new()
            .observe(ObservationSignalId::UTILIZATION, 0.8)
            .observe(model_execution_current_profile_rank_signal(), 0.0);
        assert!(matches!(
            planner.propose_transition_with_context(&resource, &missing),
            PlanOutcome::InsufficientEvidence { .. }
        ));
        assert!(matches!(
            planner.propose_transition_with_context(
                &resource,
                &context(MAX_EXACT_F64_INTEGER + 2.0, 0.8, 0.0),
            ),
            PlanOutcome::InsufficientEvidence { .. }
        ));
    }
}
