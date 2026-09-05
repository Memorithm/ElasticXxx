//! Atomic runtime bridge for correlated model-execution profiles.
//!
//! A correlated profile changes several model-execution coordinates together.
//! Representing that change as three independent runtime transitions would lose
//! the profile-set correlation invariant. This module therefore exposes one
//! atomic custom dimension, `model-execution.profile`, whose magnitude is the
//! provider-defined preference rank inside one exact profile-set fingerprint.
//!
//! This is still planning-only. A physical backend must later interpret the
//! selected profile and implement ElasticXxx's existing transactional actuation
//! boundary; this module does not create a second transaction protocol.

use crate::model_execution_profiles::{
    ModelExecutionProfileError, ModelExecutionProfilePlanV1, ModelExecutionProfileSetV1,
};
use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, ContractId, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObservationSignalId, ResourceClassId, ResourceSpec, ResourceSpecError,
};
use elastic_core::TransitionMechanism;
use elastic_eir::{
    EirResource, PlanOutcome, PlanningContext, TransitionCandidate, TransitionPlanner,
};
use std::fmt;

/// Versioned contract for an atomic correlated-profile runtime transition.
pub const MODEL_EXECUTION_ATOMIC_PROFILE_V1: &str =
    "elastic.model-execution.atomic-profile@1.0.0";
/// Atomic runtime dimension carrying one correlated profile choice.
pub const MODEL_EXECUTION_PROFILE_DIMENSION: &str = "model-execution.profile";
/// Observation carrying the current profile's unique provider preference rank.
pub const MODEL_EXECUTION_CURRENT_PROFILE_RANK_SIGNAL: &str =
    "model-execution.current-profile-rank";

/// Construct the typed atomic profile dimension.
pub fn model_execution_profile_dimension() -> DimensionId {
    DimensionId::custom(MODEL_EXECUTION_PROFILE_DIMENSION)
        .expect("model-execution.profile is a valid custom dimension")
}

/// Construct the typed current-profile-rank observation signal.
pub fn model_execution_current_profile_rank_signal() -> ObservationSignalId {
    ObservationSignalId::custom(MODEL_EXECUTION_CURRENT_PROFILE_RANK_SIGNAL)
        .expect("model-execution.current-profile-rank is a valid observation signal")
}

impl ModelExecutionProfileSetV1 {
    /// Map an exact correlated profile set to one atomic Elastic runtime resource.
    ///
    /// The resource deliberately exposes a single elastic dimension rather than
    /// the three underlying model coordinates. A physical transition along this
    /// dimension must therefore switch one complete published profile as a unit.
    ///
    /// # Errors
    ///
    /// Returns a structured error if the generic Elastic resource declaration
    /// cannot be built.
    pub fn atomic_resource_spec(
        &self,
        resource_id: impl Into<String>,
    ) -> Result<ResourceSpec, ModelExecutionAtomicProfileError> {
        let resource_id = LogicalResourceId::new(resource_id.into())?;
        let contract = ContractId::new(MODEL_EXECUTION_ATOMIC_PROFILE_V1)?;
        let dimension = model_execution_profile_dimension();

        Ok(ResourceSpec::builder(ResourceClassId::CONFIGURATIONAL, resource_id)
            .allow(dimension.clone())
            .preserve(Invariant::new(InvariantKind::PreserveIdentity))
            .preserve(Invariant::new(InvariantKind::UpholdContract(contract)))
            .admit(AdmissibleTransition::new(
                TransitionMechanism::Reinterpret,
                dimension.clone(),
            ))
            .require_capability(CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                dimension,
            ))
            .observe(model_execution_current_profile_rank_signal())
            .observe(ObservationSignalId::FREE_CAPACITY)
            .observe(ObservationSignalId::UTILIZATION)
            .label("model-execution.atomic-contract", MODEL_EXECUTION_ATOMIC_PROFILE_V1)
            .label("model-execution.provider", self.provider_id())
            .label("model-execution.model-revision", self.model_revision())
            .label(
                "model-execution.capability-fingerprint",
                self.capability_fingerprint().to_string(),
            )
            .label(
                "model-execution.profile-set-fingerprint",
                self.fingerprint().to_string(),
            )
            .build()?)
    }
}

/// Deterministic planner that lowers one selected correlated profile into the
/// generic EIR transition vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionAtomicProfilePlannerV1 {
    provider_id: String,
    model_revision: String,
    capability_fingerprint: String,
    profile_set_fingerprint: String,
    profile_id: String,
    target_rank: u32,
}

impl ModelExecutionAtomicProfilePlannerV1 {
    /// Bind a planner to one already-validated correlated profile selection.
    #[must_use]
    pub fn new(plan: &ModelExecutionProfilePlanV1) -> Self {
        Self {
            provider_id: plan.provider_id().to_owned(),
            model_revision: plan.model_revision().to_owned(),
            capability_fingerprint: plan.capability_fingerprint().to_string(),
            profile_set_fingerprint: plan.profile_set_fingerprint().to_string(),
            profile_id: plan.profile_id().to_owned(),
            target_rank: plan.preference_rank(),
        }
    }

    /// Stable target profile identity retained for diagnostics/audit.
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// Unique target rank inside the exact bound profile set.
    #[must_use]
    pub const fn target_rank(&self) -> u32 {
        self.target_rank
    }

    fn resource_identity_matches(&self, resource: &EirResource) -> bool {
        resource.label("model-execution.atomic-contract") == Some(MODEL_EXECUTION_ATOMIC_PROFILE_V1)
            && resource.label("model-execution.provider") == Some(self.provider_id.as_str())
            && resource.label("model-execution.model-revision")
                == Some(self.model_revision.as_str())
            && resource.label("model-execution.capability-fingerprint")
                == Some(self.capability_fingerprint.as_str())
            && resource.label("model-execution.profile-set-fingerprint")
                == Some(self.profile_set_fingerprint.as_str())
    }

    fn admitted_transition(
        &self,
        resource: &EirResource,
    ) -> Result<elastic_eir::AdmittedTransition, PlanOutcome> {
        let dimension = model_execution_profile_dimension();
        let Some(admitted) = resource.transitions().iter().find(|admitted| {
            admitted.transition().mechanism() == TransitionMechanism::Reinterpret
                && admitted.transition().dimension() == &dimension
        }) else {
            return Err(PlanOutcome::Unsupported);
        };
        if !admitted.capability_grounded() {
            return Err(PlanOutcome::InsufficientEvidence {
                detail: "atomic model-execution profile transition lacks a required capability"
                    .to_owned(),
            });
        }
        Ok(admitted.clone())
    }
}

impl TransitionPlanner for ModelExecutionAtomicProfilePlannerV1 {
    fn propose_transition(&self, _resource: &EirResource) -> PlanOutcome {
        PlanOutcome::InsufficientEvidence {
            detail: "atomic model-execution planner requires the current profile rank observation"
                .to_owned(),
        }
    }

    fn propose_transition_with_context(
        &self,
        resource: &EirResource,
        context: &PlanningContext,
    ) -> PlanOutcome {
        if !self.resource_identity_matches(resource) {
            return PlanOutcome::InsufficientEvidence {
                detail: "atomic model-execution resource identity/fingerprint does not match the selected profile"
                    .to_owned(),
            };
        }

        let Some(raw_current_rank) = context.get(model_execution_current_profile_rank_signal()) else {
            return PlanOutcome::InsufficientEvidence {
                detail: "missing current model-execution profile rank observation".to_owned(),
            };
        };
        let Some(current_rank) = exact_profile_rank(raw_current_rank) else {
            return PlanOutcome::InsufficientEvidence {
                detail: format!(
                    "current model-execution profile rank must be an exact u32; got {raw_current_rank}"
                ),
            };
        };
        if current_rank == self.target_rank {
            return PlanOutcome::NoCandidate;
        }

        let admitted = match self.admitted_transition(resource) {
            Ok(admitted) => admitted,
            Err(outcome) => return outcome,
        };
        let candidate = TransitionCandidate::from_admitted(&admitted)
            .with_magnitude(u64::from(self.target_rank));
        if candidate.is_declared_in(resource) {
            PlanOutcome::Candidate(candidate)
        } else {
            PlanOutcome::NoCandidate
        }
    }
}

fn exact_profile_rank(value: f64) -> Option<u32> {
    if !value.is_finite() || value < 0.0 || value > f64::from(u32::MAX) || value.fract() != 0.0 {
        return None;
    }
    Some(value as u32)
}

/// Fail-closed errors for atomic profile resource construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelExecutionAtomicProfileError {
    /// Generic Elastic resource construction rejected the declaration.
    ResourceSpec(ResourceSpecError),
    /// Correlated profile validation error propagated by a future bridge caller.
    Profile(ModelExecutionProfileError),
}

impl fmt::Display for ModelExecutionAtomicProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceSpec(error) => write!(f, "invalid atomic profile resource: {error}"),
            Self::Profile(error) => write!(f, "invalid correlated profile: {error}"),
        }
    }
}

impl std::error::Error for ModelExecutionAtomicProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ResourceSpec(error) => Some(error),
            Self::Profile(error) => Some(error),
        }
    }
}

impl From<ResourceSpecError> for ModelExecutionAtomicProfileError {
    fn from(value: ResourceSpecError) -> Self {
        Self::ResourceSpec(value)
    }
}

impl From<ModelExecutionProfileError> for ModelExecutionAtomicProfileError {
    fn from(value: ModelExecutionProfileError) -> Self {
        Self::Profile(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_execution::ModelExecutionCapabilitiesV1;
    use crate::model_execution_profiles::{
        ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSelectionV1,
        ModelExecutionProfileSelectorV1, ModelExecutionProfileV1,
    };
    use elastic_eir::lower;

    fn fixture() -> (ModelExecutionProfileSetV1, ModelExecutionProfilePlanV1) {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let profiles = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
                ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
            ],
        )
        .unwrap();
        let selection = ModelExecutionProfileSelectorV1
            .select(
                &profiles,
                ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
            )
            .unwrap();
        let ModelExecutionProfileSelectionV1::Selected(plan) = selection else {
            panic!("expected profile selection")
        };
        (profiles, plan)
    }

    #[test]
    fn profile_set_maps_to_one_atomic_runtime_dimension() {
        let (profiles, _) = fixture();
        let spec = profiles.atomic_resource_spec("model-runtime").unwrap();
        assert_eq!(spec.elastic_dimensions(), &[model_execution_profile_dimension()]);
        assert!(spec.admits(
            TransitionMechanism::Reinterpret,
            &model_execution_profile_dimension()
        ));
    }

    #[test]
    fn planner_emits_rank_as_atomic_candidate_magnitude() {
        let (profiles, target) = fixture();
        let spec = profiles.atomic_resource_spec("model-runtime").unwrap();
        let doc = lower(&spec).unwrap();
        let resource = doc.resource("model-runtime").unwrap();
        let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
        let context = PlanningContext::new()
            .observe(model_execution_current_profile_rank_signal(), 0.0);

        let outcome = planner.propose_transition_with_context(resource, &context);
        let PlanOutcome::Candidate(candidate) = outcome else {
            panic!("expected atomic candidate")
        };
        assert_eq!(candidate.dimension(), &model_execution_profile_dimension());
        assert_eq!(candidate.magnitude(), Some(10));
        assert!(candidate.is_declared_in(resource));
    }

    #[test]
    fn planner_is_noop_when_target_profile_is_current() {
        let (profiles, target) = fixture();
        let spec = profiles.atomic_resource_spec("model-runtime").unwrap();
        let doc = lower(&spec).unwrap();
        let resource = doc.resource("model-runtime").unwrap();
        let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
        let context = PlanningContext::new()
            .observe(model_execution_current_profile_rank_signal(), 10.0);

        assert_eq!(
            planner.propose_transition_with_context(resource, &context),
            PlanOutcome::NoCandidate
        );
    }

    #[test]
    fn planner_fails_closed_on_stale_profile_set_resource() {
        let (profiles, target) = fixture();
        let changed_capabilities = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4, 8],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let changed_profiles = ModelExecutionProfileSetV1::new(
            &changed_capabilities,
            profiles.profiles().to_vec(),
        )
        .unwrap();
        let spec = changed_profiles
            .atomic_resource_spec("model-runtime")
            .unwrap();
        let doc = lower(&spec).unwrap();
        let resource = doc.resource("model-runtime").unwrap();
        let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
        let context = PlanningContext::new()
            .observe(model_execution_current_profile_rank_signal(), 0.0);

        assert!(matches!(
            planner.propose_transition_with_context(resource, &context),
            PlanOutcome::InsufficientEvidence { .. }
        ));
    }

    #[test]
    fn invalid_current_rank_evidence_is_not_rounded() {
        let (profiles, target) = fixture();
        let spec = profiles.atomic_resource_spec("model-runtime").unwrap();
        let doc = lower(&spec).unwrap();
        let resource = doc.resource("model-runtime").unwrap();
        let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
        let context = PlanningContext::new()
            .observe(model_execution_current_profile_rank_signal(), 9.5);

        assert!(matches!(
            planner.propose_transition_with_context(resource, &context),
            PlanOutcome::InsufficientEvidence { .. }
        ));
    }
}
