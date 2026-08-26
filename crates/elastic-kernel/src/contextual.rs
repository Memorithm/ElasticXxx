//! Kernel planning results bound to recommendation freshness assumptions.
//!
//! [`plan_with_context`] is a thin, domain-neutral wrapper around the existing
//! kernel planner. It records the [`RecommendationContext`] that was current
//! when planning ran and requires a trusted [`FreshnessSnapshot`] check before
//! callers can consume a selected realization.
//!
//! This module does not make planner output authoritative: a fresh selection
//! still requires the existing resource-specific semantic, dispatch, lifecycle,
//! and actuation checks.

use elastic_core::{
    FreshnessSnapshot, LogicalResourceId, RecommendationContext, RecommendationFreshnessError,
};
use elastic_eir::Fingerprint;

use crate::candidate::KernelCandidate;
use crate::capability::CapabilitySnapshot;
use crate::planner::{plan, SelectionOutcome, SelectionPolicy, SelectionRecord};

/// One kernel-planner outcome bound to the assumptions used when it was
/// produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextualSelection {
    context: RecommendationContext,
    outcome: SelectionOutcome,
}

impl ContextualSelection {
    /// Freshness assumptions captured at planning time.
    #[must_use]
    pub const fn context(&self) -> &RecommendationContext {
        &self.context
    }

    /// Raw honest planner outcome.
    ///
    /// Reading the outcome is useful for audit/telemetry. Callers intending to
    /// act on a selected realization should use [`Self::selected_record_if_fresh`]
    /// rather than bypassing the freshness gate.
    #[must_use]
    pub const fn outcome(&self) -> &SelectionOutcome {
        &self.outcome
    }

    /// Validate that the assumptions used for this planner result still hold.
    ///
    /// # Errors
    ///
    /// Returns the typed stale-context failure from `elastic-core` when the
    /// planner epoch, observation epoch, or any recorded resource generation
    /// no longer matches the trusted current snapshot.
    pub fn validate_freshness(
        &self,
        current: &FreshnessSnapshot,
    ) -> Result<(), RecommendationFreshnessError> {
        self.context.validate_freshness(current)
    }

    /// Expose a successful selection only after revalidating freshness.
    ///
    /// `Ok(None)` means the underlying honest planner outcome was not
    /// `Selected`; it does not turn `NoCandidate`, `InsufficientEvidence`, or
    /// `Unsupported` into success.
    ///
    /// # Errors
    ///
    /// Returns a freshness error before exposing any selected realization when
    /// the planning assumptions are stale.
    pub fn selected_record_if_fresh(
        &self,
        current: &FreshnessSnapshot,
    ) -> Result<Option<&SelectionRecord>, RecommendationFreshnessError> {
        self.validate_freshness(current)?;
        Ok(match &self.outcome {
            SelectionOutcome::Selected(record) => Some(record.as_ref()),
            SelectionOutcome::NoCandidate { .. }
            | SelectionOutcome::InsufficientEvidence { .. }
            | SelectionOutcome::Unsupported { .. } => None,
        })
    }

    /// Consume the envelope into its freshness context and raw planner outcome.
    #[must_use]
    pub fn into_parts(self) -> (RecommendationContext, SelectionOutcome) {
        (self.context, self.outcome)
    }
}

/// Run the existing deterministic kernel planner while binding its result to a
/// caller-supplied freshness context.
///
/// The context must describe the planner/observation/resource-generation facts
/// actually used by the caller to assemble the workload, capability snapshot,
/// policy, and candidates. This function does not authenticate those facts;
/// the trusted boundary supplies the later [`FreshnessSnapshot`].
#[must_use]
pub fn plan_with_context(
    logical_resource_id: &LogicalResourceId,
    workload_fingerprint: Fingerprint,
    snapshot: &CapabilitySnapshot,
    policy: &SelectionPolicy,
    candidates: &[KernelCandidate],
    context: RecommendationContext,
) -> ContextualSelection {
    ContextualSelection {
        context,
        outcome: plan(
            logical_resource_id,
            workload_fingerprint,
            snapshot,
            policy,
            candidates,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::{
        BuiltinObjective, ContractId, ObservationEpoch, ObjectiveId, PlannerEpoch,
        ResourceGeneration,
    };

    use crate::candidate::{ObjectiveEvidence, RealizationIdentity};
    use crate::capability::{
        BindingLimits, FeatureSupport, SubgroupSupport, WorkgroupLimits,
    };
    use crate::requirements::{FeatureRequirement, KernelRequirements};

    fn resource() -> LogicalResourceId {
        LogicalResourceId::new("attention/contextual").expect("valid logical resource")
    }

    fn contract() -> ContractId {
        ContractId::new("attention-forward-v1").expect("valid contract")
    }

    fn requirements() -> KernelRequirements {
        KernelRequirements {
            invocations_per_workgroup: 64,
            invocations_per_axis: [64, 1, 1],
            workgroup_storage_bytes: 1024,
            bind_groups: 2,
            max_storage_buffer_binding_bytes: 4096,
            subgroup_min_width: None,
            shader_f16: FeatureRequirement::NotRequired,
            matrix_ops: FeatureRequirement::NotRequired,
        }
    }

    fn candidate() -> KernelCandidate {
        KernelCandidate::new(
            resource(),
            RealizationIdentity::new("portable").expect("valid realization"),
            1,
            requirements(),
            contract(),
            ObjectiveEvidence::new(),
        )
        .expect("valid candidate")
    }

    fn capabilities() -> CapabilitySnapshot {
        CapabilitySnapshot::new(CapabilitySnapshot {
            workgroup_limits: WorkgroupLimits {
                max_invocations_per_axis: [256, 256, 64],
                max_invocations_per_workgroup: 256,
                max_workgroups_per_axis: 65_535,
                max_workgroup_storage_bytes: 32_768,
            },
            binding_limits: BindingLimits {
                max_bind_groups: 8,
                max_storage_buffer_binding_bytes: 128 << 20,
            },
            subgroup_support: SubgroupSupport::unsupported(),
            shader_f16: FeatureSupport::Known(false),
            matrix_ops: FeatureSupport::Unknown,
        })
        .expect("valid capabilities")
    }

    fn policy() -> SelectionPolicy {
        SelectionPolicy::with_options(
            vec![ObjectiveId::builtin(BuiltinObjective::Latency)],
            contract(),
            false,
            true,
        )
        .expect("valid policy")
    }

    fn context() -> RecommendationContext {
        RecommendationContext::new(PlannerEpoch::new(7), ObservationEpoch::new(11))
            .with_resource_generation(resource(), ResourceGeneration::new(5))
    }

    fn current() -> FreshnessSnapshot {
        FreshnessSnapshot::new(PlannerEpoch::new(7), ObservationEpoch::new(11))
            .with_resource_generation(resource(), ResourceGeneration::new(5))
    }

    #[test]
    fn selected_record_is_exposed_only_when_context_remains_fresh() {
        let planned = plan_with_context(
            &resource(),
            Fingerprint::EMPTY.text("workload"),
            &capabilities(),
            &policy(),
            &[candidate()],
            context(),
        );

        let selected = planned
            .selected_record_if_fresh(&current())
            .expect("fresh context")
            .expect("uncontested portable candidate selected");
        assert_eq!(selected.selected_realization().as_str(), "portable");
    }

    #[test]
    fn stale_planner_epoch_blocks_access_to_selected_record() {
        let planned = plan_with_context(
            &resource(),
            Fingerprint::EMPTY.text("workload"),
            &capabilities(),
            &policy(),
            &[candidate()],
            context(),
        );
        let stale = FreshnessSnapshot::new(PlannerEpoch::new(8), ObservationEpoch::new(11))
            .with_resource_generation(resource(), ResourceGeneration::new(5));

        assert_eq!(
            planned.selected_record_if_fresh(&stale),
            Err(RecommendationFreshnessError::PlannerEpochMismatch {
                recommended: PlannerEpoch::new(7),
                current: PlannerEpoch::new(8),
            })
        );
    }

    #[test]
    fn changed_resource_generation_blocks_access_to_selected_record() {
        let planned = plan_with_context(
            &resource(),
            Fingerprint::EMPTY.text("workload"),
            &capabilities(),
            &policy(),
            &[candidate()],
            context(),
        );
        let stale = FreshnessSnapshot::new(PlannerEpoch::new(7), ObservationEpoch::new(11))
            .with_resource_generation(resource(), ResourceGeneration::new(6));

        assert_eq!(
            planned.selected_record_if_fresh(&stale),
            Err(RecommendationFreshnessError::ResourceGenerationMismatch {
                resource: resource(),
                recommended: ResourceGeneration::new(5),
                current: ResourceGeneration::new(6),
            })
        );
    }

    #[test]
    fn non_selected_outcome_stays_non_selected_after_freshness_check() {
        let planned = plan_with_context(
            &resource(),
            Fingerprint::EMPTY.text("workload"),
            &capabilities(),
            &policy(),
            &[],
            context(),
        );

        assert_eq!(planned.selected_record_if_fresh(&current()), Ok(None));
        assert!(matches!(
            planned.outcome(),
            SelectionOutcome::NoCandidate { offered: 0, .. }
        ));
    }
}
