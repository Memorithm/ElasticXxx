//! Correlated model-execution profiles layered over the v1 axis contract.
//!
//! `model_execution` v1 deliberately publishes discrete values per axis. This
//! module adds an explicit correlation boundary for backends where only some
//! combinations of active experts, expert width, and activation budget are
//! qualified together.
//!
//! Profiles are provider-owned declarations. ElasticXxx does not infer that a
//! Cartesian product of individually supported axis values is valid, and it does
//! not infer model quality from a profile's rank. The provider supplies a unique
//! `preference_rank`; the deterministic selector chooses the first ranked profile
//! that fits an explicit resource envelope.
//!
//! This remains pre-execution planning. It does not actuate a live model.

use crate::model_execution::{
    ModelExecutionCapabilitiesV1, ModelExecutionContractError, ModelExecutionResourcePlanV1,
    MODEL_EXECUTION_BASIS_POINTS_FULL,
};
use elastic_core::resource::ResourceSpec;
use elastic_eir::Fingerprint;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Versioned contract for a correlated set of qualified model-execution profiles.
pub const MODEL_EXECUTION_PROFILE_SET_V1: &str = "elastic.model-execution.profile-set@1.0.0";
/// JSON media type for [`MODEL_EXECUTION_PROFILE_SET_V1`].
pub const MODEL_EXECUTION_PROFILE_SET_MEDIA_TYPE_V1: &str =
    "application/vnd.elastic.model-execution-profile-set.v1+json";
/// Versioned contract for one selected correlated model-execution profile.
pub const MODEL_EXECUTION_PROFILE_PLAN_V1: &str = "elastic.model-execution.profile-plan@1.0.0";
/// JSON media type for [`MODEL_EXECUTION_PROFILE_PLAN_V1`].
pub const MODEL_EXECUTION_PROFILE_PLAN_MEDIA_TYPE_V1: &str =
    "application/vnd.elastic.model-execution-profile-plan.v1+json";

/// One provider-qualified correlated execution profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionProfileV1 {
    profile_id: String,
    preference_rank: u32,
    active_experts: u32,
    expert_width_bps: u16,
    activation_budget_bps: u16,
}

impl ModelExecutionProfileV1 {
    /// Define one profile before binding it to an exact capability set.
    ///
    /// The tuple is validated against [`ModelExecutionCapabilitiesV1`] when a
    /// [`ModelExecutionProfileSetV1`] is constructed. This constructor only
    /// rejects a blank profile identity.
    ///
    /// # Errors
    ///
    /// Returns [`ModelExecutionProfileError::BlankProfileId`] for a blank id.
    pub fn new(
        profile_id: impl Into<String>,
        preference_rank: u32,
        active_experts: u32,
        expert_width_bps: u16,
        activation_budget_bps: u16,
    ) -> Result<Self, ModelExecutionProfileError> {
        let profile_id = profile_id.into();
        let profile_id = profile_id.trim();
        if profile_id.is_empty() {
            return Err(ModelExecutionProfileError::BlankProfileId);
        }
        Ok(Self {
            profile_id: profile_id.to_owned(),
            preference_rank,
            active_experts,
            expert_width_bps,
            activation_budget_bps,
        })
    }

    /// Provider-defined stable profile identity.
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// Provider-defined preference rank; lower values are selected first.
    #[must_use]
    pub const fn preference_rank(&self) -> u32 {
        self.preference_rank
    }

    /// Qualified active-expert count.
    #[must_use]
    pub const fn active_experts(&self) -> u32 {
        self.active_experts
    }

    /// Qualified expert width in basis points.
    #[must_use]
    pub const fn expert_width_bps(&self) -> u16 {
        self.expert_width_bps
    }

    /// Qualified activation budget in basis points.
    #[must_use]
    pub const fn activation_budget_bps(&self) -> u16 {
        self.activation_budget_bps
    }

    fn to_wire(&self) -> ModelExecutionProfileWireV1 {
        ModelExecutionProfileWireV1 {
            profile_id: self.profile_id.clone(),
            preference_rank: self.preference_rank,
            active_experts: self.active_experts,
            expert_width_bps: self.expert_width_bps,
            activation_budget_bps: self.activation_budget_bps,
        }
    }
}

/// Strict wire form of one correlated execution profile.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionProfileWireV1 {
    profile_id: String,
    preference_rank: u32,
    active_experts: u32,
    expert_width_bps: u16,
    activation_budget_bps: u16,
}

impl ModelExecutionProfileWireV1 {
    fn into_native(self) -> Result<ModelExecutionProfileV1, ModelExecutionProfileError> {
        ModelExecutionProfileV1::new(
            self.profile_id,
            self.preference_rank,
            self.active_experts,
            self.expert_width_bps,
            self.activation_budget_bps,
        )
    }
}

/// Stable wire envelope for a correlated profile set.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionProfileSetWireV1 {
    contract: String,
    provider_id: String,
    model_revision: String,
    capability_fingerprint: String,
    profiles: Vec<ModelExecutionProfileWireV1>,
}

impl ModelExecutionProfileSetWireV1 {
    /// Revalidate this wire set against the exact base capability declaration.
    ///
    /// # Errors
    ///
    /// Fails closed for contract/identity/fingerprint mismatches or invalid,
    /// duplicate, or unsupported correlated profiles.
    pub fn into_validated(
        self,
        capabilities: &ModelExecutionCapabilitiesV1,
    ) -> Result<ModelExecutionProfileSetV1, ModelExecutionProfileError> {
        if self.contract != MODEL_EXECUTION_PROFILE_SET_V1 {
            return Err(ModelExecutionProfileError::UnsupportedProfileSetContract {
                contract: self.contract,
            });
        }
        validate_capability_identity(
            &self.provider_id,
            &self.model_revision,
            &self.capability_fingerprint,
            capabilities,
        )?;
        let profiles = self
            .profiles
            .into_iter()
            .map(ModelExecutionProfileWireV1::into_native)
            .collect::<Result<Vec<_>, _>>()?;
        ModelExecutionProfileSetV1::new(capabilities, profiles)
    }
}

/// A validated set of provider-qualified correlated execution profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionProfileSetV1 {
    provider_id: String,
    model_revision: String,
    capability_fingerprint: Fingerprint,
    profiles: Vec<ModelExecutionProfileV1>,
    fingerprint: Fingerprint,
}

impl ModelExecutionProfileSetV1 {
    /// Bind correlated profiles to one exact base capability declaration.
    ///
    /// Profiles are sorted by their provider-defined preference rank. The
    /// constructor rejects duplicate ids, ranks, and tuples, and it validates
    /// every tuple through [`ModelExecutionResourcePlanV1::new`].
    ///
    /// # Errors
    ///
    /// Fails closed if the set is empty, duplicates correlation identity, or a
    /// profile tuple is not admitted by `capabilities`.
    pub fn new(
        capabilities: &ModelExecutionCapabilitiesV1,
        mut profiles: Vec<ModelExecutionProfileV1>,
    ) -> Result<Self, ModelExecutionProfileError> {
        if profiles.is_empty() {
            return Err(ModelExecutionProfileError::EmptyProfiles);
        }

        for profile in &profiles {
            ModelExecutionResourcePlanV1::new(
                capabilities,
                profile.active_experts,
                profile.expert_width_bps,
                profile.activation_budget_bps,
            )
            .map_err(|source| ModelExecutionProfileError::InvalidProfileTarget {
                profile_id: profile.profile_id.clone(),
                source,
            })?;
        }

        profiles.sort_by(|left, right| {
            left.preference_rank
                .cmp(&right.preference_rank)
                .then_with(|| left.profile_id.cmp(&right.profile_id))
        });

        for (index, profile) in profiles.iter().enumerate() {
            for previous in &profiles[..index] {
                if previous.profile_id == profile.profile_id {
                    return Err(ModelExecutionProfileError::DuplicateProfileId {
                        profile_id: profile.profile_id.clone(),
                    });
                }
                if previous.preference_rank == profile.preference_rank {
                    return Err(ModelExecutionProfileError::DuplicatePreferenceRank {
                        rank: profile.preference_rank,
                    });
                }
                if previous.active_experts == profile.active_experts
                    && previous.expert_width_bps == profile.expert_width_bps
                    && previous.activation_budget_bps == profile.activation_budget_bps
                {
                    return Err(ModelExecutionProfileError::DuplicateProfileTuple {
                        active_experts: profile.active_experts,
                        expert_width_bps: profile.expert_width_bps,
                        activation_budget_bps: profile.activation_budget_bps,
                    });
                }
            }
        }

        let provider_id = capabilities.provider_id().to_owned();
        let model_revision = capabilities.model_revision().to_owned();
        let capability_fingerprint = capabilities.fingerprint();
        let mut fingerprint = Fingerprint::EMPTY
            .text(MODEL_EXECUTION_PROFILE_SET_V1)
            .text(&provider_id)
            .text(&model_revision)
            .number(capability_fingerprint.bits());
        for profile in &profiles {
            fingerprint = fingerprint
                .text(&profile.profile_id)
                .number(u64::from(profile.preference_rank))
                .number(u64::from(profile.active_experts))
                .number(u64::from(profile.expert_width_bps))
                .number(u64::from(profile.activation_budget_bps));
        }

        Ok(Self {
            provider_id,
            model_revision,
            capability_fingerprint,
            profiles,
            fingerprint,
        })
    }

    /// Provider/backend identity shared with the base capabilities.
    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Exact model revision shared with the base capabilities.
    #[must_use]
    pub fn model_revision(&self) -> &str {
        &self.model_revision
    }

    /// Structural fingerprint of the base capability declaration.
    #[must_use]
    pub const fn capability_fingerprint(&self) -> Fingerprint {
        self.capability_fingerprint
    }

    /// Structural fingerprint of this exact correlated profile set.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Correlated profiles in deterministic provider-preference order.
    #[must_use]
    pub fn profiles(&self) -> &[ModelExecutionProfileV1] {
        &self.profiles
    }

    /// Find one profile by stable provider id.
    #[must_use]
    pub fn profile_by_id(&self, profile_id: &str) -> Option<&ModelExecutionProfileV1> {
        self.profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
    }

    /// Find an explicitly qualified complete tuple.
    ///
    /// This is the correlation guard: values that each exist in the underlying
    /// v1 axis sets still return `None` unless their complete tuple was published
    /// here.
    #[must_use]
    pub fn profile_for_tuple(
        &self,
        active_experts: u32,
        expert_width_bps: u16,
        activation_budget_bps: u16,
    ) -> Option<&ModelExecutionProfileV1> {
        self.profiles.iter().find(|profile| {
            profile.active_experts == active_experts
                && profile.expert_width_bps == expert_width_bps
                && profile.activation_budget_bps == activation_budget_bps
        })
    }

    /// Convert this set to the strict v1 JSON envelope.
    #[must_use]
    pub fn to_wire(&self) -> ModelExecutionProfileSetWireV1 {
        ModelExecutionProfileSetWireV1 {
            contract: MODEL_EXECUTION_PROFILE_SET_V1.to_owned(),
            provider_id: self.provider_id.clone(),
            model_revision: self.model_revision.clone(),
            capability_fingerprint: self.capability_fingerprint.to_string(),
            profiles: self
                .profiles
                .iter()
                .map(ModelExecutionProfileV1::to_wire)
                .collect(),
        }
    }
}

/// Explicit upper bounds used by the deterministic correlated-profile selector.
///
/// These values are an already-resolved resource envelope. This type does not
/// infer them from hardware observations because units and safety margins remain
/// backend/operator responsibilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelExecutionProfileEnvelopeV1 {
    max_active_experts: u32,
    max_expert_width_bps: u16,
    max_activation_budget_bps: u16,
}

impl ModelExecutionProfileEnvelopeV1 {
    /// Construct an explicit selection envelope.
    ///
    /// # Errors
    ///
    /// Rejects zero expert count and basis-point bounds outside `1..=10_000`.
    pub fn new(
        max_active_experts: u32,
        max_expert_width_bps: u16,
        max_activation_budget_bps: u16,
    ) -> Result<Self, ModelExecutionProfileError> {
        if max_active_experts == 0
            || !(1..=MODEL_EXECUTION_BASIS_POINTS_FULL).contains(&max_expert_width_bps)
            || !(1..=MODEL_EXECUTION_BASIS_POINTS_FULL).contains(&max_activation_budget_bps)
        {
            return Err(ModelExecutionProfileError::InvalidEnvelope {
                max_active_experts,
                max_expert_width_bps,
                max_activation_budget_bps,
            });
        }
        Ok(Self {
            max_active_experts,
            max_expert_width_bps,
            max_activation_budget_bps,
        })
    }

    /// Maximum active experts admitted by the resolved envelope.
    #[must_use]
    pub const fn max_active_experts(&self) -> u32 {
        self.max_active_experts
    }

    /// Maximum expert width admitted by the resolved envelope.
    #[must_use]
    pub const fn max_expert_width_bps(&self) -> u16 {
        self.max_expert_width_bps
    }

    /// Maximum activation budget admitted by the resolved envelope.
    #[must_use]
    pub const fn max_activation_budget_bps(&self) -> u16 {
        self.max_activation_budget_bps
    }

    /// Whether this envelope admits the complete profile tuple.
    #[must_use]
    pub fn allows(&self, profile: &ModelExecutionProfileV1) -> bool {
        profile.active_experts <= self.max_active_experts
            && profile.expert_width_bps <= self.max_expert_width_bps
            && profile.activation_budget_bps <= self.max_activation_budget_bps
    }
}

/// Deterministic selector for already-qualified correlated profiles.
///
/// The selector performs no learned optimization and invents no ranking. It
/// scans the profile set in provider-supplied preference order and selects the
/// first complete tuple that fits the explicit envelope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelExecutionProfileSelectorV1;

impl ModelExecutionProfileSelectorV1 {
    /// Select the first provider-preferred correlated profile that fits `envelope`.
    #[must_use]
    pub fn select(
        &self,
        profiles: &ModelExecutionProfileSetV1,
        envelope: ModelExecutionProfileEnvelopeV1,
    ) -> ModelExecutionProfileSelectionV1 {
        match profiles
            .profiles
            .iter()
            .find(|profile| envelope.allows(profile))
        {
            Some(profile) => ModelExecutionProfileSelectionV1::Selected(
                ModelExecutionProfilePlanV1::from_profile(profiles, profile),
            ),
            None => ModelExecutionProfileSelectionV1::NoFeasibleProfile,
        }
    }
}

/// Explicit outcome of correlated-profile selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelExecutionProfileSelectionV1 {
    /// One complete, explicitly published tuple was selected.
    Selected(ModelExecutionProfilePlanV1),
    /// No published complete tuple fits the explicit envelope.
    NoFeasibleProfile,
}

/// Selected correlated profile bound to both capability and profile-set identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionProfilePlanV1 {
    provider_id: String,
    model_revision: String,
    capability_fingerprint: Fingerprint,
    profile_set_fingerprint: Fingerprint,
    profile_id: String,
    preference_rank: u32,
    resource_plan: ModelExecutionResourcePlanV1,
}

impl ModelExecutionProfilePlanV1 {
    fn from_profile(
        profiles: &ModelExecutionProfileSetV1,
        profile: &ModelExecutionProfileV1,
    ) -> Self {
        let resource_plan = ModelExecutionResourcePlanV1::new_unchecked_from_profile_set(
            profiles.provider_id.clone(),
            profiles.model_revision.clone(),
            profiles.capability_fingerprint,
            profile.active_experts,
            profile.expert_width_bps,
            profile.activation_budget_bps,
        );
        Self {
            provider_id: profiles.provider_id.clone(),
            model_revision: profiles.model_revision.clone(),
            capability_fingerprint: profiles.capability_fingerprint,
            profile_set_fingerprint: profiles.fingerprint,
            profile_id: profile.profile_id.clone(),
            preference_rank: profile.preference_rank,
            resource_plan,
        }
    }

    /// Provider/backend identity.
    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Exact model revision.
    #[must_use]
    pub fn model_revision(&self) -> &str {
        &self.model_revision
    }

    /// Base capability fingerprint.
    #[must_use]
    pub const fn capability_fingerprint(&self) -> Fingerprint {
        self.capability_fingerprint
    }

    /// Correlated profile-set fingerprint.
    #[must_use]
    pub const fn profile_set_fingerprint(&self) -> Fingerprint {
        self.profile_set_fingerprint
    }

    /// Stable selected profile identity.
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// Provider preference rank of the selected profile.
    #[must_use]
    pub const fn preference_rank(&self) -> u32 {
        self.preference_rank
    }

    /// Underlying three-axis resource plan.
    #[must_use]
    pub const fn resource_plan(&self) -> &ModelExecutionResourcePlanV1 {
        &self.resource_plan
    }

    /// Map the selected tuple to the generic Elastic resource declaration.
    pub fn resource_spec(
        &self,
        resource_id: impl Into<String>,
    ) -> Result<ResourceSpec, ModelExecutionContractError> {
        self.resource_plan.resource_spec(resource_id)
    }

    /// Convert this selected profile to its strict replay envelope.
    #[must_use]
    pub fn to_wire(&self) -> ModelExecutionProfilePlanWireV1 {
        ModelExecutionProfilePlanWireV1 {
            contract: MODEL_EXECUTION_PROFILE_PLAN_V1.to_owned(),
            provider_id: self.provider_id.clone(),
            model_revision: self.model_revision.clone(),
            capability_fingerprint: self.capability_fingerprint.to_string(),
            profile_set_fingerprint: self.profile_set_fingerprint.to_string(),
            profile_id: self.profile_id.clone(),
        }
    }
}

/// Strict wire envelope for one correlated selected profile.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionProfilePlanWireV1 {
    contract: String,
    provider_id: String,
    model_revision: String,
    capability_fingerprint: String,
    profile_set_fingerprint: String,
    profile_id: String,
}

impl ModelExecutionProfilePlanWireV1 {
    /// Revalidate this selected profile against the current exact profile set.
    ///
    /// # Errors
    ///
    /// Fails closed if any contract, identity, or fingerprint changed, or the
    /// selected profile id is absent.
    pub fn into_validated(
        self,
        profiles: &ModelExecutionProfileSetV1,
    ) -> Result<ModelExecutionProfilePlanV1, ModelExecutionProfileError> {
        if self.contract != MODEL_EXECUTION_PROFILE_PLAN_V1 {
            return Err(ModelExecutionProfileError::UnsupportedProfilePlanContract {
                contract: self.contract,
            });
        }
        if self.provider_id != profiles.provider_id {
            return Err(ModelExecutionProfileError::ProviderMismatch {
                expected: profiles.provider_id.clone(),
                actual: self.provider_id,
            });
        }
        if self.model_revision != profiles.model_revision {
            return Err(ModelExecutionProfileError::ModelRevisionMismatch {
                expected: profiles.model_revision.clone(),
                actual: self.model_revision,
            });
        }
        let expected_capability = profiles.capability_fingerprint.to_string();
        if self.capability_fingerprint != expected_capability {
            return Err(ModelExecutionProfileError::CapabilityFingerprintMismatch {
                expected: expected_capability,
                actual: self.capability_fingerprint,
            });
        }
        let expected_profiles = profiles.fingerprint.to_string();
        if self.profile_set_fingerprint != expected_profiles {
            return Err(ModelExecutionProfileError::ProfileSetFingerprintMismatch {
                expected: expected_profiles,
                actual: self.profile_set_fingerprint,
            });
        }
        let profile = profiles.profile_by_id(&self.profile_id).ok_or_else(|| {
            ModelExecutionProfileError::UnknownProfileId {
                profile_id: self.profile_id.clone(),
            }
        })?;
        Ok(ModelExecutionProfilePlanV1::from_profile(profiles, profile))
    }
}

/// Fail-closed errors for correlated model-execution profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelExecutionProfileError {
    /// Profile-set wire envelope carries an unknown contract.
    UnsupportedProfileSetContract { contract: String },
    /// Profile-plan wire envelope carries an unknown contract.
    UnsupportedProfilePlanContract { contract: String },
    /// Profile identity is empty after trimming.
    BlankProfileId,
    /// A correlated profile set must contain at least one profile.
    EmptyProfiles,
    /// Profile ids must be unique inside one set.
    DuplicateProfileId { profile_id: String },
    /// Preference ranks must be unique inside one set.
    DuplicatePreferenceRank { rank: u32 },
    /// The same complete tuple was published more than once.
    DuplicateProfileTuple {
        active_experts: u32,
        expert_width_bps: u16,
        activation_budget_bps: u16,
    },
    /// One profile tuple is not admitted by the underlying v1 capabilities.
    InvalidProfileTarget {
        profile_id: String,
        source: ModelExecutionContractError,
    },
    /// Provider identity differs from the bound capability/profile set.
    ProviderMismatch { expected: String, actual: String },
    /// Model revision differs from the bound capability/profile set.
    ModelRevisionMismatch { expected: String, actual: String },
    /// Base capability fingerprint differs from the bound declaration.
    CapabilityFingerprintMismatch { expected: String, actual: String },
    /// Correlated profile-set fingerprint differs from the bound declaration.
    ProfileSetFingerprintMismatch { expected: String, actual: String },
    /// Selected profile id is absent from the exact correlated set.
    UnknownProfileId { profile_id: String },
    /// Selection envelope has an invalid zero/out-of-range bound.
    InvalidEnvelope {
        max_active_experts: u32,
        max_expert_width_bps: u16,
        max_activation_budget_bps: u16,
    },
}

impl fmt::Display for ModelExecutionProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProfileSetContract { contract } => write!(
                f,
                "model-execution profile-set contract {contract:?} is unsupported; expected {MODEL_EXECUTION_PROFILE_SET_V1}"
            ),
            Self::UnsupportedProfilePlanContract { contract } => write!(
                f,
                "model-execution profile-plan contract {contract:?} is unsupported; expected {MODEL_EXECUTION_PROFILE_PLAN_V1}"
            ),
            Self::BlankProfileId => f.write_str("model-execution profile id must not be blank"),
            Self::EmptyProfiles => f.write_str("model-execution profile set must not be empty"),
            Self::DuplicateProfileId { profile_id } => {
                write!(f, "model-execution profile id {profile_id:?} is duplicated")
            }
            Self::DuplicatePreferenceRank { rank } => {
                write!(f, "model-execution preference rank {rank} is duplicated")
            }
            Self::DuplicateProfileTuple {
                active_experts,
                expert_width_bps,
                activation_budget_bps,
            } => write!(
                f,
                "model-execution correlated tuple ({active_experts}, {expert_width_bps}, {activation_budget_bps}) is duplicated"
            ),
            Self::InvalidProfileTarget { profile_id, source } => {
                write!(f, "model-execution profile {profile_id:?} is invalid: {source}")
            }
            Self::ProviderMismatch { expected, actual } => write!(
                f,
                "model-execution profile provider mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::ModelRevisionMismatch { expected, actual } => write!(
                f,
                "model-execution profile revision mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::CapabilityFingerprintMismatch { expected, actual } => write!(
                f,
                "model-execution profile capability fingerprint mismatch: expected {expected}, got {actual}"
            ),
            Self::ProfileSetFingerprintMismatch { expected, actual } => write!(
                f,
                "model-execution profile-set fingerprint mismatch: expected {expected}, got {actual}"
            ),
            Self::UnknownProfileId { profile_id } => {
                write!(f, "model-execution profile id {profile_id:?} is not published")
            }
            Self::InvalidEnvelope {
                max_active_experts,
                max_expert_width_bps,
                max_activation_budget_bps,
            } => write!(
                f,
                "invalid model-execution envelope: experts={max_active_experts}, expert-width-bps={max_expert_width_bps}, activation-budget-bps={max_activation_budget_bps}"
            ),
        }
    }
}

impl std::error::Error for ModelExecutionProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidProfileTarget { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn validate_capability_identity(
    provider_id: &str,
    model_revision: &str,
    capability_fingerprint: &str,
    capabilities: &ModelExecutionCapabilitiesV1,
) -> Result<(), ModelExecutionProfileError> {
    if provider_id != capabilities.provider_id() {
        return Err(ModelExecutionProfileError::ProviderMismatch {
            expected: capabilities.provider_id().to_owned(),
            actual: provider_id.to_owned(),
        });
    }
    if model_revision != capabilities.model_revision() {
        return Err(ModelExecutionProfileError::ModelRevisionMismatch {
            expected: capabilities.model_revision().to_owned(),
            actual: model_revision.to_owned(),
        });
    }
    let expected = capabilities.fingerprint().to_string();
    if capability_fingerprint != expected {
        return Err(ModelExecutionProfileError::CapabilityFingerprintMismatch {
            expected,
            actual: capability_fingerprint.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities() -> ModelExecutionCapabilitiesV1 {
        ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap()
    }

    fn profiles(capabilities: &ModelExecutionCapabilitiesV1) -> ModelExecutionProfileSetV1 {
        ModelExecutionProfileSetV1::new(
            capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
                ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
            ],
        )
        .unwrap()
    }

    #[test]
    fn correlation_guard_rejects_unpublished_cartesian_tuple() {
        let capabilities = capabilities();
        assert!(capabilities.supports(4, 2_500, 10_000));

        let profiles = profiles(&capabilities);
        assert!(profiles.profile_for_tuple(4, 2_500, 10_000).is_none());
        assert!(profiles.profile_for_tuple(2, 5_000, 5_000).is_some());
    }

    #[test]
    fn selector_uses_provider_rank_only_among_complete_feasible_profiles() {
        let capabilities = capabilities();
        let profiles = profiles(&capabilities);
        let envelope = ModelExecutionProfileEnvelopeV1::new(2, 5_000, 6_000).unwrap();
        let selected = ModelExecutionProfileSelectorV1.select(&profiles, envelope);

        let ModelExecutionProfileSelectionV1::Selected(plan) = selected else {
            panic!("expected correlated profile")
        };
        assert_eq!(plan.profile_id(), "balanced");
        assert_eq!(plan.preference_rank(), 10);
        assert_eq!(plan.resource_plan().active_experts(), 2);
        assert_eq!(plan.resource_plan().expert_width_bps(), 5_000);
        assert_eq!(plan.resource_plan().activation_budget_bps(), 5_000);
    }

    #[test]
    fn selector_returns_explicit_no_feasible_profile() {
        let capabilities = capabilities();
        let profiles = profiles(&capabilities);
        let envelope = ModelExecutionProfileEnvelopeV1::new(1, 2_500, 2_000).unwrap();
        assert_eq!(
            ModelExecutionProfileSelectorV1.select(&profiles, envelope),
            ModelExecutionProfileSelectionV1::NoFeasibleProfile
        );
    }

    #[test]
    fn duplicate_profile_identity_and_rank_fail_closed() {
        let capabilities = capabilities();
        let duplicate_id = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("same", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("same", 1, 2, 5_000, 5_000).unwrap(),
            ],
        )
        .unwrap_err();
        assert!(matches!(
            duplicate_id,
            ModelExecutionProfileError::DuplicateProfileId { .. }
        ));

        let duplicate_rank = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("a", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("b", 0, 2, 5_000, 5_000).unwrap(),
            ],
        )
        .unwrap_err();
        assert!(matches!(
            duplicate_rank,
            ModelExecutionProfileError::DuplicatePreferenceRank { rank: 0 }
        ));
    }

    #[test]
    fn profile_set_wire_round_trip_revalidates_base_capability_identity() {
        let capabilities = capabilities();
        let profiles = profiles(&capabilities);
        let json = serde_json::to_string(&profiles.to_wire()).unwrap();
        let wire: ModelExecutionProfileSetWireV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.into_validated(&capabilities).unwrap(), profiles);
    }

    #[test]
    fn selected_plan_fails_closed_when_profile_set_changes() {
        let capabilities = capabilities();
        let profiles = profiles(&capabilities);
        let selected = ModelExecutionProfileSelectorV1.select(
            &profiles,
            ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
        );
        let ModelExecutionProfileSelectionV1::Selected(plan) = selected else {
            panic!("expected selection")
        };
        let json = serde_json::to_string(&plan.to_wire()).unwrap();

        let changed = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
            ],
        )
        .unwrap();
        let wire: ModelExecutionProfilePlanWireV1 = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            wire.into_validated(&changed).unwrap_err(),
            ModelExecutionProfileError::ProfileSetFingerprintMismatch { .. }
        ));
    }

    #[test]
    fn wire_shapes_reject_unknown_fields() {
        let raw = format!(
            r#"{{"contract":"{MODEL_EXECUTION_PROFILE_SET_V1}","provider_id":"backend","model_revision":"rev","capability_fingerprint":"fp:0","profiles":[],"extra":true}}"#
        );
        assert!(serde_json::from_str::<ModelExecutionProfileSetWireV1>(&raw).is_err());
    }
}
