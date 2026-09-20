//! ELANG7 binding between provider-qualified model profiles and generic policy.
//!
//! This bridge does not select, rank, or actuate model profiles. It binds one
//! exact [`ModelExecutionProfileSetV1`] to one exact resource-targeted Elastic
//! policy revision and gives each published profile a stable predicate key. The
//! generic policy core then enforces an `exactly one profile active` constraint.
//! Physical selection and transaction semantics remain owned by the existing
//! model-execution adapters.

use crate::model_execution::model_execution_resource_spec;
use crate::model_execution_profiles::{
    ModelExecutionProfileError, ModelExecutionProfilePlanV1, ModelExecutionProfileSetV1,
};
use elastic_core::{
    PolicyHeader, PolicyTarget, PolicyTargetKind, PredicateKey, PredicateRegistryError,
    PseudoBooleanBindingError, PseudoBooleanConstraintDeclaration, ResourcePolicyError,
    ResourcePolicySpec, MAX_REGISTERED_PREDICATES,
};
use elastic_eir::Fingerprint;
use std::fmt;

/// Versioned identity of the model-profile/policy binding contract.
pub const MODEL_EXECUTION_PROFILE_POLICY_BINDING_V1: &str =
    "elastic.model-execution.profile-policy-binding@1.0.0";
/// Stable namespace for the profile-active predicates emitted by this bridge.
pub const MODEL_EXECUTION_PROFILE_PREDICATE_NAMESPACE_V1: &str =
    "elastic.model-execution.profile-active";

/// One provider profile mapped to one stable policy predicate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionProfilePolicyEntryV1 {
    profile_id: String,
    preference_rank: u32,
    predicate: PredicateKey,
    active_experts: u32,
    expert_width_bps: u16,
    activation_budget_bps: u16,
}

impl ModelExecutionProfilePolicyEntryV1 {
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    #[must_use]
    pub const fn preference_rank(&self) -> u32 {
        self.preference_rank
    }

    #[must_use]
    pub const fn predicate(&self) -> &PredicateKey {
        &self.predicate
    }

    #[must_use]
    pub const fn active_experts(&self) -> u32 {
        self.active_experts
    }

    #[must_use]
    pub const fn expert_width_bps(&self) -> u16 {
        self.expert_width_bps
    }

    #[must_use]
    pub const fn activation_budget_bps(&self) -> u16 {
        self.activation_budget_bps
    }
}

/// Exact binding of a generic Elastic resource policy to one provider profile set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionProfilePolicyBindingV1 {
    policy: ResourcePolicySpec,
    profiles: ModelExecutionProfileSetV1,
    entries: Vec<ModelExecutionProfilePolicyEntryV1>,
    fingerprint: Fingerprint,
}

impl ModelExecutionProfilePolicyBindingV1 {
    /// Bind one resource policy revision to one exact provider-qualified profile set.
    ///
    /// The target resource identity comes from `header`; group targets fail
    /// closed. The generated generic resource declaration carries the exact
    /// provider/model/capability identity. Profile predicates are stable within
    /// this exact set and the generic policy receives one pseudo-Boolean
    /// `exactly-one` constraint over all published profiles.
    pub fn new(
        header: PolicyHeader,
        profiles: &ModelExecutionProfileSetV1,
    ) -> Result<Self, ModelExecutionProfilePolicyBindingError> {
        let PolicyTarget::Resource(resource_id) = header.target() else {
            return Err(
                ModelExecutionProfilePolicyBindingError::TargetKindMismatch {
                    observed: header.target().kind(),
                },
            );
        };
        if profiles.profiles().len() > MAX_REGISTERED_PREDICATES {
            return Err(ModelExecutionProfilePolicyBindingError::TooManyProfiles {
                profiles: profiles.profiles().len(),
                maximum: MAX_REGISTERED_PREDICATES,
            });
        }
        let resource = model_execution_resource_spec(
            profiles.provider_id(),
            profiles.model_revision(),
            profiles.capability_fingerprint(),
            Some(profiles.fingerprint()),
            resource_id.as_str(),
        )?;

        let mut entries = Vec::with_capacity(profiles.profiles().len());
        let mut predicates = Vec::with_capacity(profiles.profiles().len());
        for profile in profiles.profiles() {
            let predicate = PredicateKey::new(
                MODEL_EXECUTION_PROFILE_PREDICATE_NAMESPACE_V1,
                format!("rank-{}", profile.preference_rank()),
            )?;
            predicates.push(predicate.clone());
            entries.push(ModelExecutionProfilePolicyEntryV1 {
                profile_id: profile.profile_id().to_owned(),
                preference_rank: profile.preference_rank(),
                predicate,
                active_experts: profile.active_experts(),
                expert_width_bps: profile.expert_width_bps(),
                activation_budget_bps: profile.activation_budget_bps(),
            });
        }
        let exactly_one = PseudoBooleanConstraintDeclaration::exactly_keys(predicates, 1)?;
        let policy = ResourcePolicySpec::new(header, resource, Vec::new(), vec![exactly_one])?;

        let mut fingerprint = Fingerprint::EMPTY
            .text(MODEL_EXECUTION_PROFILE_POLICY_BINDING_V1)
            .text(policy.header().identity().id().as_str())
            .text(&policy.header().identity().version().to_string())
            .text(policy.header().target().kind().as_str())
            .text(policy.header().target().as_str())
            .text(profiles.provider_id())
            .text(profiles.model_revision())
            .number(profiles.capability_fingerprint().bits())
            .number(profiles.fingerprint().bits())
            .number(entries.len() as u64);
        for entry in &entries {
            fingerprint = fingerprint
                .text(entry.profile_id())
                .number(u64::from(entry.preference_rank()))
                .text(entry.predicate().namespace())
                .text(entry.predicate().name())
                .number(u64::from(entry.active_experts()))
                .number(u64::from(entry.expert_width_bps()))
                .number(u64::from(entry.activation_budget_bps()));
        }

        Ok(Self {
            policy,
            profiles: profiles.clone(),
            entries,
            fingerprint,
        })
    }

    /// Generic policy carrying the authoritative exactly-one profile constraint.
    #[must_use]
    pub const fn policy(&self) -> &ResourcePolicySpec {
        &self.policy
    }

    /// Exact provider-owned profile set bound to this policy.
    #[must_use]
    pub const fn profiles(&self) -> &ModelExecutionProfileSetV1 {
        &self.profiles
    }

    /// Deterministic profile/predicate mapping in provider-preference order.
    #[must_use]
    pub fn entries(&self) -> &[ModelExecutionProfilePolicyEntryV1] {
        &self.entries
    }

    /// Structural identity of policy target + exact provider profile set + mapping.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Find the stable policy predicate corresponding to one provider profile id.
    #[must_use]
    pub fn predicate_for_profile(&self, profile_id: &str) -> Option<&PredicateKey> {
        self.entries
            .iter()
            .find(|entry| entry.profile_id == profile_id)
            .map(ModelExecutionProfilePolicyEntryV1::predicate)
    }

    /// Revalidate a selected profile plan against this exact binding.
    ///
    /// This deliberately round-trips through the existing strict profile-plan
    /// wire validator, preserving provider/model/capability/profile-set identity
    /// as the semantic authority instead of reimplementing those checks here.
    pub fn validate_plan<'a>(
        &'a self,
        plan: &ModelExecutionProfilePlanV1,
    ) -> Result<&'a PredicateKey, ModelExecutionProfilePolicyBindingError> {
        let replayed = plan.to_wire().into_validated(&self.profiles)?;
        if &replayed != plan {
            return Err(ModelExecutionProfilePolicyBindingError::PlanDrift {
                profile_id: plan.profile_id().to_owned(),
            });
        }
        self.predicate_for_profile(plan.profile_id())
            .ok_or_else(
                || ModelExecutionProfilePolicyBindingError::UnknownProfileId {
                    profile_id: plan.profile_id().to_owned(),
                },
            )
    }
}

/// Fail-closed errors for the ELANG7 model-profile policy bridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelExecutionProfilePolicyBindingError {
    TargetKindMismatch { observed: PolicyTargetKind },
    TooManyProfiles { profiles: usize, maximum: usize },
    Resource(crate::ModelExecutionContractError),
    Predicate(PredicateRegistryError),
    Constraint(PseudoBooleanBindingError),
    Policy(ResourcePolicyError),
    Profile(ModelExecutionProfileError),
    UnknownProfileId { profile_id: String },
    PlanDrift { profile_id: String },
}

impl From<crate::ModelExecutionContractError> for ModelExecutionProfilePolicyBindingError {
    fn from(value: crate::ModelExecutionContractError) -> Self {
        Self::Resource(value)
    }
}
impl From<PredicateRegistryError> for ModelExecutionProfilePolicyBindingError {
    fn from(value: PredicateRegistryError) -> Self {
        Self::Predicate(value)
    }
}
impl From<PseudoBooleanBindingError> for ModelExecutionProfilePolicyBindingError {
    fn from(value: PseudoBooleanBindingError) -> Self {
        Self::Constraint(value)
    }
}
impl From<ResourcePolicyError> for ModelExecutionProfilePolicyBindingError {
    fn from(value: ResourcePolicyError) -> Self {
        Self::Policy(value)
    }
}
impl From<ModelExecutionProfileError> for ModelExecutionProfilePolicyBindingError {
    fn from(value: ModelExecutionProfileError) -> Self {
        Self::Profile(value)
    }
}

impl fmt::Display for ModelExecutionProfilePolicyBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetKindMismatch { observed } => write!(
                f,
                "model-execution profile policy requires a resource target, observed {}",
                observed.as_str()
            ),
            Self::TooManyProfiles { profiles, maximum } => write!(
                f,
                "model-execution profile policy contains {profiles} profiles; compact predicate core maximum is {maximum}"
            ),
            Self::Resource(error) => error.fmt(f),
            Self::Predicate(error) => error.fmt(f),
            Self::Constraint(error) => error.fmt(f),
            Self::Policy(error) => error.fmt(f),
            Self::Profile(error) => error.fmt(f),
            Self::UnknownProfileId { profile_id } => write!(
                f,
                "model-execution profile policy does not contain profile {profile_id:?}"
            ),
            Self::PlanDrift { profile_id } => write!(
                f,
                "model-execution profile plan {profile_id:?} did not round-trip through the exact bound profile set"
            ),
        }
    }
}

impl std::error::Error for ModelExecutionProfilePolicyBindingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModelExecutionCapabilitiesV1, ModelExecutionProfileEnvelopeV1,
        ModelExecutionProfileSelectionV1, ModelExecutionProfileSelectorV1, ModelExecutionProfileV1,
    };
    use elastic_core::{PolicyId, PolicyIdentity, PolicyTarget, PolicyVersion, PseudoBooleanScale};
    use elastic_eir::lower_resource_policy;

    fn profiles(provider: &str) -> ModelExecutionProfileSetV1 {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            provider,
            "model-r1",
            4,
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

    fn header() -> PolicyHeader {
        PolicyHeader::new(
            PolicyIdentity::new(
                PolicyId::new("edge-model-profile").unwrap(),
                PolicyVersion::new(1, 0, 0),
            ),
            PolicyTarget::resource(elastic_core::LogicalResourceId::new("inference").unwrap()),
        )
    }

    #[test]
    fn binding_reuses_exactly_one_pseudo_boolean_policy_semantics() {
        let profiles = profiles("nnis");
        let binding = ModelExecutionProfilePolicyBindingV1::new(header(), &profiles).unwrap();
        assert_eq!(binding.entries().len(), 3);
        assert_eq!(binding.policy().constraints().len(), 1);
        let constraint = &binding.policy().constraints()[0];
        assert_eq!(constraint.threshold(), 1);
        assert_eq!(constraint.scale(), &PseudoBooleanScale::count());
        assert_eq!(constraint.terms().len(), 3);
        let lowered = lower_resource_policy(binding.policy()).unwrap();
        assert_eq!(lowered.constrained_resource().constraints().len(), 1);
        assert_eq!(
            binding
                .predicate_for_profile("balanced")
                .unwrap()
                .namespace(),
            MODEL_EXECUTION_PROFILE_PREDICATE_NAMESPACE_V1
        );
    }

    #[test]
    fn selected_plan_revalidates_against_exact_profile_set() {
        let profiles = profiles("nnis");
        let binding = ModelExecutionProfilePolicyBindingV1::new(header(), &profiles).unwrap();
        let selection = ModelExecutionProfileSelectorV1
            .select(
                &profiles,
                ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
            )
            .unwrap();
        let ModelExecutionProfileSelectionV1::Selected(plan) = selection else {
            panic!("balanced profile should be selected");
        };
        assert_eq!(plan.profile_id(), "balanced");
        assert_eq!(
            binding.validate_plan(&plan).unwrap(),
            binding.predicate_for_profile("balanced").unwrap()
        );
    }

    #[test]
    fn binding_fingerprint_changes_with_provider_profile_set_identity() {
        let left =
            ModelExecutionProfilePolicyBindingV1::new(header(), &profiles("nnis-a")).unwrap();
        let right =
            ModelExecutionProfilePolicyBindingV1::new(header(), &profiles("nnis-b")).unwrap();
        assert_ne!(left.fingerprint(), right.fingerprint());
    }

    #[test]
    fn group_target_fails_closed() {
        let header = PolicyHeader::new(
            PolicyIdentity::new(
                PolicyId::new("edge-model-profile").unwrap(),
                PolicyVersion::new(1, 0, 0),
            ),
            PolicyTarget::group(elastic_core::ResourceGroupId::new("edge").unwrap()),
        );
        assert!(matches!(
            ModelExecutionProfilePolicyBindingV1::new(header, &profiles("nnis")),
            Err(ModelExecutionProfilePolicyBindingError::TargetKindMismatch { .. })
        ));
    }

    #[test]
    fn policy_eir_binds_profile_set_fingerprint_not_only_rank_keys() {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            "nnis",
            "model-r1",
            4,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let left_profiles = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
            ],
        )
        .unwrap();
        let right_profiles = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 2_500).unwrap(),
            ],
        )
        .unwrap();
        let left = ModelExecutionProfilePolicyBindingV1::new(header(), &left_profiles).unwrap();
        let right = ModelExecutionProfilePolicyBindingV1::new(header(), &right_profiles).unwrap();
        let left_eir = lower_resource_policy(left.policy()).unwrap();
        let right_eir = lower_resource_policy(right.policy()).unwrap();
        assert_ne!(left_profiles.fingerprint(), right_profiles.fingerprint());
        assert_ne!(left_eir.fingerprint(), right_eir.fingerprint());
    }
}
