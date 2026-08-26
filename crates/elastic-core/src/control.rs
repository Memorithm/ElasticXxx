//! Freshness contracts for planner recommendations.
//!
//! This module deliberately stops before planning or actuation. A
//! [`RecommendationContext`] records the planner/observation epochs and
//! resource generations that a recommendation depended on. A trusted boundary
//! compares that context with a current [`FreshnessSnapshot`] before allowing
//! any downstream validator or actuator to consider the recommendation.
//!
//! The separation is intentional:
//! - recommendations are not authority tokens;
//! - freshness is not proof of semantic legality;
//! - a fresh recommendation must still pass resource-specific validation.

use crate::resource::LogicalResourceId;
use std::collections::BTreeMap;
use std::fmt;

macro_rules! epoch_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            /// Construct an epoch/generation counter from its raw value.
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// Return the raw counter.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

epoch_type!(
    /// Version of the planner/policy that produced a recommendation.
    PlannerEpoch
);
epoch_type!(
    /// Version of the observation snapshot used by a recommendation.
    ObservationEpoch
);
epoch_type!(
    /// Version of one externally observed logical resource/capability state.
    ResourceGeneration
);

/// Dependency context carried by a planner recommendation.
///
/// The context records assumptions; it grants no authority to mutate any
/// resource. Only resources whose generation influenced the recommendation
/// need to be listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecommendationContext {
    planner_epoch: PlannerEpoch,
    observation_epoch: ObservationEpoch,
    resource_generations: BTreeMap<LogicalResourceId, ResourceGeneration>,
}

impl RecommendationContext {
    /// Start a recommendation context at the planner and observation epochs
    /// used to produce it.
    #[must_use]
    pub const fn new(planner_epoch: PlannerEpoch, observation_epoch: ObservationEpoch) -> Self {
        Self {
            planner_epoch,
            observation_epoch,
            resource_generations: BTreeMap::new(),
        }
    }

    /// Planner version that produced this recommendation.
    #[must_use]
    pub const fn planner_epoch(&self) -> PlannerEpoch {
        self.planner_epoch
    }

    /// Observation version used by this recommendation.
    #[must_use]
    pub const fn observation_epoch(&self) -> ObservationEpoch {
        self.observation_epoch
    }

    /// Record a resource generation dependency, replacing an earlier value for
    /// the same logical resource when present.
    pub fn insert_resource_generation(
        &mut self,
        resource: LogicalResourceId,
        generation: ResourceGeneration,
    ) -> Option<ResourceGeneration> {
        self.resource_generations.insert(resource, generation)
    }

    /// Builder-style resource generation dependency.
    #[must_use]
    pub fn with_resource_generation(
        mut self,
        resource: LogicalResourceId,
        generation: ResourceGeneration,
    ) -> Self {
        self.insert_resource_generation(resource, generation);
        self
    }

    /// Generation assumed for one logical resource, when that resource was a
    /// recommendation dependency.
    #[must_use]
    pub fn resource_generation(
        &self,
        resource: &LogicalResourceId,
    ) -> Option<ResourceGeneration> {
        self.resource_generations.get(resource).copied()
    }

    /// Iterate through resource dependencies in stable logical-resource order.
    pub fn resource_generations(
        &self,
    ) -> impl Iterator<Item = (&LogicalResourceId, ResourceGeneration)> {
        self.resource_generations
            .iter()
            .map(|(resource, generation)| (resource, *generation))
    }

    /// Validate only freshness assumptions against a trusted current snapshot.
    ///
    /// This is intentionally conservative in v0.1: planner and observation
    /// epochs must match exactly, and every resource generation named by the
    /// recommendation must still exist with exactly the same generation.
    /// Resource-specific semantic/physical legality remains a later gate.
    ///
    /// # Errors
    ///
    /// Returns a typed [`RecommendationFreshnessError`] for the first stale or
    /// unavailable assumption in deterministic validation order.
    pub fn validate_freshness(
        &self,
        current: &FreshnessSnapshot,
    ) -> Result<(), RecommendationFreshnessError> {
        if self.planner_epoch != current.planner_epoch {
            return Err(RecommendationFreshnessError::PlannerEpochMismatch {
                recommended: self.planner_epoch,
                current: current.planner_epoch,
            });
        }
        if self.observation_epoch != current.observation_epoch {
            return Err(RecommendationFreshnessError::ObservationEpochMismatch {
                recommended: self.observation_epoch,
                current: current.observation_epoch,
            });
        }
        for (resource, recommended) in &self.resource_generations {
            let Some(current_generation) = current.resource_generations.get(resource).copied()
            else {
                return Err(RecommendationFreshnessError::MissingResourceGeneration {
                    resource: resource.clone(),
                });
            };
            if *recommended != current_generation {
                return Err(RecommendationFreshnessError::ResourceGenerationMismatch {
                    resource: resource.clone(),
                    recommended: *recommended,
                    current: current_generation,
                });
            }
        }
        Ok(())
    }
}

/// Trusted snapshot against which recommendation freshness is checked.
///
/// Extra resource generations are harmless: a recommendation is invalidated
/// only by assumptions it actually recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FreshnessSnapshot {
    planner_epoch: PlannerEpoch,
    observation_epoch: ObservationEpoch,
    resource_generations: BTreeMap<LogicalResourceId, ResourceGeneration>,
}

impl FreshnessSnapshot {
    /// Construct the trusted planner/observation epoch snapshot.
    #[must_use]
    pub const fn new(planner_epoch: PlannerEpoch, observation_epoch: ObservationEpoch) -> Self {
        Self {
            planner_epoch,
            observation_epoch,
            resource_generations: BTreeMap::new(),
        }
    }

    /// Current planner epoch.
    #[must_use]
    pub const fn planner_epoch(&self) -> PlannerEpoch {
        self.planner_epoch
    }

    /// Current observation epoch.
    #[must_use]
    pub const fn observation_epoch(&self) -> ObservationEpoch {
        self.observation_epoch
    }

    /// Record/replace the current generation for one logical resource.
    pub fn insert_resource_generation(
        &mut self,
        resource: LogicalResourceId,
        generation: ResourceGeneration,
    ) -> Option<ResourceGeneration> {
        self.resource_generations.insert(resource, generation)
    }

    /// Builder-style current resource generation.
    #[must_use]
    pub fn with_resource_generation(
        mut self,
        resource: LogicalResourceId,
        generation: ResourceGeneration,
    ) -> Self {
        self.insert_resource_generation(resource, generation);
        self
    }

    /// Current generation for one resource when known to this snapshot.
    #[must_use]
    pub fn resource_generation(
        &self,
        resource: &LogicalResourceId,
    ) -> Option<ResourceGeneration> {
        self.resource_generations.get(resource).copied()
    }
}

/// Freshness failures detected before semantic validation or actuation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecommendationFreshnessError {
    /// The recommendation was produced by a planner version other than the
    /// currently trusted one.
    PlannerEpochMismatch {
        /// Planner epoch carried by the recommendation.
        recommended: PlannerEpoch,
        /// Current trusted planner epoch.
        current: PlannerEpoch,
    },
    /// The recommendation was based on a different observation snapshot.
    ObservationEpochMismatch {
        /// Observation epoch carried by the recommendation.
        recommended: ObservationEpoch,
        /// Current trusted observation epoch.
        current: ObservationEpoch,
    },
    /// A resource dependency is no longer present in the trusted snapshot.
    MissingResourceGeneration {
        /// Missing logical resource dependency.
        resource: LogicalResourceId,
    },
    /// A resource dependency has changed since recommendation production.
    ResourceGenerationMismatch {
        /// Logical resource whose generation changed.
        resource: LogicalResourceId,
        /// Generation assumed by the recommendation.
        recommended: ResourceGeneration,
        /// Current trusted generation.
        current: ResourceGeneration,
    },
}

impl fmt::Display for RecommendationFreshnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlannerEpochMismatch {
                recommended,
                current,
            } => write!(
                f,
                "recommendation planner epoch {recommended} does not match current epoch {current}"
            ),
            Self::ObservationEpochMismatch {
                recommended,
                current,
            } => write!(
                f,
                "recommendation observation epoch {recommended} does not match current epoch {current}"
            ),
            Self::MissingResourceGeneration { resource } => write!(
                f,
                "recommendation depends on resource {} whose current generation is unavailable",
                resource.as_str()
            ),
            Self::ResourceGenerationMismatch {
                resource,
                recommended,
                current,
            } => write!(
                f,
                "recommendation resource {} generation {recommended} does not match current generation {current}",
                resource.as_str()
            ),
        }
    }
}

impl std::error::Error for RecommendationFreshnessError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(value: &str) -> LogicalResourceId {
        LogicalResourceId::new(value).expect("valid test resource id")
    }

    #[test]
    fn exact_context_is_fresh_and_extra_current_resources_do_not_invalidate_it() {
        let context = RecommendationContext::new(PlannerEpoch::new(4), ObservationEpoch::new(9))
            .with_resource_generation(resource("workers"), ResourceGeneration::new(7));
        let current = FreshnessSnapshot::new(PlannerEpoch::new(4), ObservationEpoch::new(9))
            .with_resource_generation(resource("workers"), ResourceGeneration::new(7))
            .with_resource_generation(resource("ram"), ResourceGeneration::new(12));

        assert_eq!(context.validate_freshness(&current), Ok(()));
    }

    #[test]
    fn planner_and_observation_mismatches_are_rejected_before_resources() {
        let context = RecommendationContext::new(PlannerEpoch::new(3), ObservationEpoch::new(5));
        let planner_changed =
            FreshnessSnapshot::new(PlannerEpoch::new(4), ObservationEpoch::new(5));
        assert_eq!(
            context.validate_freshness(&planner_changed),
            Err(RecommendationFreshnessError::PlannerEpochMismatch {
                recommended: PlannerEpoch::new(3),
                current: PlannerEpoch::new(4),
            })
        );

        let observation_changed =
            FreshnessSnapshot::new(PlannerEpoch::new(3), ObservationEpoch::new(6));
        assert_eq!(
            context.validate_freshness(&observation_changed),
            Err(RecommendationFreshnessError::ObservationEpochMismatch {
                recommended: ObservationEpoch::new(5),
                current: ObservationEpoch::new(6),
            })
        );
    }

    #[test]
    fn missing_and_changed_resource_generations_are_typed_failures() {
        let workers = resource("workers");
        let context = RecommendationContext::new(PlannerEpoch::new(1), ObservationEpoch::new(1))
            .with_resource_generation(workers.clone(), ResourceGeneration::new(10));
        let empty = FreshnessSnapshot::new(PlannerEpoch::new(1), ObservationEpoch::new(1));
        assert_eq!(
            context.validate_freshness(&empty),
            Err(RecommendationFreshnessError::MissingResourceGeneration {
                resource: workers.clone(),
            })
        );

        let changed = FreshnessSnapshot::new(PlannerEpoch::new(1), ObservationEpoch::new(1))
            .with_resource_generation(workers.clone(), ResourceGeneration::new(11));
        assert_eq!(
            context.validate_freshness(&changed),
            Err(RecommendationFreshnessError::ResourceGenerationMismatch {
                resource: workers,
                recommended: ResourceGeneration::new(10),
                current: ResourceGeneration::new(11),
            })
        );
    }

    #[test]
    fn resource_dependency_iteration_is_deterministic() {
        let mut context = RecommendationContext::new(PlannerEpoch::new(1), ObservationEpoch::new(2));
        context.insert_resource_generation(resource("z"), ResourceGeneration::new(3));
        context.insert_resource_generation(resource("a"), ResourceGeneration::new(1));
        context.insert_resource_generation(resource("m"), ResourceGeneration::new(2));

        assert_eq!(
            context
                .resource_generations()
                .map(|(resource, generation)| (resource.as_str(), generation.get()))
                .collect::<Vec<_>>(),
            vec![("a", 1), ("m", 2), ("z", 3)]
        );
    }
}
