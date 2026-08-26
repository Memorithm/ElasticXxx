//! Freshness gate for trusted adapter actuation.
//!
//! Planner output is advisory. Before a resource adapter is allowed to perform
//! a physical effect, the caller can route that effect through
//! [`actuate_if_fresh`]. The gate enforces two conditions before invoking the
//! action closure:
//!
//! 1. the recommendation explicitly tracked the logical resource about to be
//!    mutated;
//! 2. the full recommendation context is still fresh against the trusted
//!    current snapshot.
//!
//! Only after both checks pass is the adapter action invoked. The adapter then
//! performs its ordinary action-time validation (bounds, invariants, active
//! holders, and so on). Freshness therefore complements rather than replaces
//! adapter legality checks.

use crate::AdapterError;
use elastic_core::{
    FreshnessSnapshot, LogicalResourceId, RecommendationContext, RecommendationFreshnessError,
};
use std::fmt;

/// Failures at the freshness-to-actuation boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActuationGateError {
    /// The recommendation did not record a generation dependency for the
    /// logical resource it is attempting to mutate.
    UntrackedResource {
        /// Resource that would have been actuated.
        resource: LogicalResourceId,
    },
    /// The recommendation tracked the resource but one of its freshness
    /// assumptions no longer matches the trusted current snapshot.
    StaleRecommendation(RecommendationFreshnessError),
    /// Freshness passed, but the resource adapter rejected the physical action
    /// under its own trusted bounds/invariant checks.
    Adapter(AdapterError),
}

impl fmt::Display for ActuationGateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UntrackedResource { resource } => write!(
                f,
                "recommendation does not track generation for actuated resource {}",
                resource.as_str()
            ),
            Self::StaleRecommendation(error) => write!(f, "stale recommendation: {error}"),
            Self::Adapter(error) => write!(f, "adapter rejected actuation: {error}"),
        }
    }
}

impl std::error::Error for ActuationGateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UntrackedResource { .. } => None,
            Self::StaleRecommendation(error) => Some(error),
            Self::Adapter(error) => Some(error),
        }
    }
}

/// Revalidate recommendation freshness immediately before a trusted adapter
/// action.
///
/// The action closure is **not invoked** when the recommendation omitted the
/// actuated resource or when any planner/observation/resource-generation
/// assumption is stale. If freshness passes, the closure runs exactly once and
/// its ordinary [`AdapterError`] is preserved inside [`ActuationGateError`].
///
/// This function grants no authority by itself. The caller remains responsible
/// for obtaining `current` from its trusted discovery/control boundary and for
/// placing the actual adapter effect inside `action`.
///
/// # Errors
///
/// Returns [`ActuationGateError::UntrackedResource`] when `resource` was not an
/// explicit dependency of the recommendation,
/// [`ActuationGateError::StaleRecommendation`] when freshness validation fails,
/// or [`ActuationGateError::Adapter`] when the adapter rejects the effect.
pub fn actuate_if_fresh<T>(
    resource: &LogicalResourceId,
    context: &RecommendationContext,
    current: &FreshnessSnapshot,
    action: impl FnOnce() -> Result<T, AdapterError>,
) -> Result<T, ActuationGateError> {
    if context.resource_generation(resource).is_none() {
        return Err(ActuationGateError::UntrackedResource {
            resource: resource.clone(),
        });
    }

    context
        .validate_freshness(current)
        .map_err(ActuationGateError::StaleRecommendation)?;

    action().map_err(ActuationGateError::Adapter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConcurrencyPermits, RamBudget};
    use elastic_core::{ObservationEpoch, PlannerEpoch, ResourceGeneration};

    fn context_for(resource: LogicalResourceId) -> RecommendationContext {
        RecommendationContext::new(PlannerEpoch::new(7), ObservationEpoch::new(11))
            .with_resource_generation(resource, ResourceGeneration::new(5))
    }

    fn current_for(resource: LogicalResourceId) -> FreshnessSnapshot {
        FreshnessSnapshot::new(PlannerEpoch::new(7), ObservationEpoch::new(11))
            .with_resource_generation(resource, ResourceGeneration::new(5))
    }

    #[test]
    fn fresh_context_allows_ram_effect() {
        let mut budget =
            RamBudget::new("ram", 4096, 512, 4096, 1024, Some(2048)).expect("valid RAM fixture");
        let resource = budget.spec().resource_id().clone();
        let context = context_for(resource.clone());
        let current = current_for(resource.clone());

        assert_eq!(
            actuate_if_fresh(&resource, &context, &current, || budget.apply(1536)),
            Ok((1024, 1536))
        );
        assert_eq!(budget.committed(), 1536);
    }

    #[test]
    fn untracked_resource_blocks_action_without_mutation() {
        let mut permits =
            ConcurrencyPermits::new("workers", 8, 2).expect("valid concurrency fixture");
        let resource = permits.spec().resource_id().clone();
        let context = RecommendationContext::new(PlannerEpoch::new(7), ObservationEpoch::new(11));
        let current = FreshnessSnapshot::new(PlannerEpoch::new(7), ObservationEpoch::new(11));

        assert_eq!(
            actuate_if_fresh(&resource, &context, &current, || permits.apply(4)),
            Err(ActuationGateError::UntrackedResource {
                resource: resource.clone(),
            })
        );
        assert_eq!(permits.width(), 2);
    }

    #[test]
    fn stale_planner_epoch_blocks_valid_ram_effect_without_mutation() {
        let mut budget =
            RamBudget::new("ram", 4096, 512, 4096, 1024, Some(2048)).expect("valid RAM fixture");
        let resource = budget.spec().resource_id().clone();
        let context = context_for(resource.clone());
        let current = FreshnessSnapshot::new(PlannerEpoch::new(8), ObservationEpoch::new(11))
            .with_resource_generation(resource.clone(), ResourceGeneration::new(5));

        assert_eq!(
            actuate_if_fresh(&resource, &context, &current, || budget.apply(1536)),
            Err(ActuationGateError::StaleRecommendation(
                RecommendationFreshnessError::PlannerEpochMismatch {
                    recommended: PlannerEpoch::new(7),
                    current: PlannerEpoch::new(8),
                }
            ))
        );
        assert_eq!(budget.committed(), 1024);
    }

    #[test]
    fn changed_resource_generation_blocks_concurrency_effect_without_mutation() {
        let mut permits =
            ConcurrencyPermits::new("workers", 8, 2).expect("valid concurrency fixture");
        let resource = permits.spec().resource_id().clone();
        let context = context_for(resource.clone());
        let current = FreshnessSnapshot::new(PlannerEpoch::new(7), ObservationEpoch::new(11))
            .with_resource_generation(resource.clone(), ResourceGeneration::new(6));

        assert_eq!(
            actuate_if_fresh(&resource, &context, &current, || permits.apply(4)),
            Err(ActuationGateError::StaleRecommendation(
                RecommendationFreshnessError::ResourceGenerationMismatch {
                    resource: resource.clone(),
                    recommended: ResourceGeneration::new(5),
                    current: ResourceGeneration::new(6),
                }
            ))
        );
        assert_eq!(permits.width(), 2);
    }

    #[test]
    fn fresh_context_does_not_override_adapter_legality() {
        let mut permits =
            ConcurrencyPermits::new("workers", 4, 2).expect("valid concurrency fixture");
        permits.acquire().expect("first holder admitted");
        permits.acquire().expect("second holder admitted");
        let resource = permits.spec().resource_id().clone();
        let context = context_for(resource.clone());
        let current = current_for(resource.clone());

        assert_eq!(
            actuate_if_fresh(&resource, &context, &current, || permits.apply(1)),
            Err(ActuationGateError::Adapter(
                AdapterError::WouldStrandHolders {
                    requested_width: 1,
                    active: 2,
                }
            ))
        );
        assert_eq!(permits.width(), 2);
        assert_eq!(permits.active(), 2);
    }
}
