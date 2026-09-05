//! Backend-neutral elastic model-execution planning boundary.
//!
//! This module defines a versioned, pre-execution contract for three conditional
//! computation axes:
//!
//! - activation budget;
//! - active expert count;
//! - active expert width.
//!
//! The contract deliberately does **not** assume that an arbitrary model may be
//! truncated or routed at arbitrary levels. A provider must publish the exact
//! discrete levels it has qualified for a concrete model revision. Plans are
//! accepted only when they are bound to the same capability fingerprint and
//! select values from those published sets.
//!
//! This is not a live-actuation adapter. It does not load weights, route tokens,
//! resize experts, mutate model state, or make model-quality/performance claims.
//! A backend-specific adapter must separately qualify physical actuation before
//! any of these knobs may change during execution.

use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, ContractId, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObservationSignalId, ResourceClassId, ResourceSpec, ResourceSpecError,
};
use elastic_core::TransitionMechanism;
use elastic_eir::Fingerprint;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Versioned capability contract for backend/model-qualified elastic execution levels.
pub const MODEL_EXECUTION_CAPABILITIES_V1: &str =
    "elastic.model-execution.capabilities@1.0.0";
/// JSON media type for [`MODEL_EXECUTION_CAPABILITIES_V1`].
pub const MODEL_EXECUTION_CAPABILITIES_MEDIA_TYPE_V1: &str =
    "application/vnd.elastic.model-execution-capabilities.v1+json";
/// Versioned pre-execution resource-plan contract.
pub const MODEL_EXECUTION_RESOURCE_PLAN_V1: &str =
    "elastic.model-execution.resource-plan@1.0.0";
/// JSON media type for [`MODEL_EXECUTION_RESOURCE_PLAN_V1`].
pub const MODEL_EXECUTION_RESOURCE_PLAN_MEDIA_TYPE_V1: &str =
    "application/vnd.elastic.model-execution-resource-plan.v1+json";

/// One basis point is 1/10,000 of the provider-declared full level.
pub const MODEL_EXECUTION_BASIS_POINTS_FULL: u16 = 10_000;

/// Custom Elastic dimension for the number of experts active in one qualified execution profile.
pub const MODEL_EXECUTION_ACTIVE_EXPERTS_DIMENSION: &str =
    "model-execution.active-expert-count";
/// Custom Elastic dimension for the active fraction of each qualified expert width.
pub const MODEL_EXECUTION_EXPERT_WIDTH_DIMENSION: &str = "model-execution.expert-width-bps";
/// Custom Elastic dimension for the provider-defined activation-compute envelope.
pub const MODEL_EXECUTION_ACTIVATION_BUDGET_DIMENSION: &str =
    "model-execution.activation-budget-bps";

/// Basis-point axis used by validation errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelExecutionBasisPointAxis {
    /// Fraction of the provider-qualified full expert width.
    ExpertWidth,
    /// Fraction of the provider-qualified full activation-compute envelope.
    ActivationBudget,
}

impl fmt::Display for ModelExecutionBasisPointAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpertWidth => f.write_str("expert-width-bps"),
            Self::ActivationBudget => f.write_str("activation-budget-bps"),
        }
    }
}

/// Stable wire form of backend/model-qualified model-execution capabilities.
///
/// Deserialization validates only JSON shape. Call [`Self::into_validated`] to
/// enforce semantic bounds and canonicalize the discrete level sets.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionCapabilitiesWireV1 {
    contract: String,
    provider_id: String,
    model_revision: String,
    total_experts: u32,
    active_expert_counts: Vec<u32>,
    expert_width_bps: Vec<u16>,
    activation_budget_bps: Vec<u16>,
}

impl ModelExecutionCapabilitiesWireV1 {
    /// Revalidate this wire envelope as native typed capabilities.
    ///
    /// # Errors
    ///
    /// Fails closed for an unknown contract, blank identity, invalid expert
    /// bounds, invalid basis-point levels, or duplicate levels.
    pub fn into_validated(self) -> Result<ModelExecutionCapabilitiesV1, ModelExecutionContractError> {
        if self.contract != MODEL_EXECUTION_CAPABILITIES_V1 {
            return Err(ModelExecutionContractError::UnsupportedCapabilitiesContract {
                contract: self.contract,
            });
        }
        ModelExecutionCapabilitiesV1::new(
            self.provider_id,
            self.model_revision,
            self.total_experts,
            self.active_expert_counts,
            self.expert_width_bps,
            self.activation_budget_bps,
        )
    }
}

/// Qualified discrete execution levels for one provider and one exact model revision.
///
/// `expert_width_bps` and `activation_budget_bps` use integer basis points so
/// the contract has no floating-point ambiguity. `10_000` denotes the provider's
/// declared full level; lower values are meaningful only when the provider has
/// explicitly published them in this capability set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionCapabilitiesV1 {
    provider_id: String,
    model_revision: String,
    total_experts: u32,
    active_expert_counts: Vec<u32>,
    expert_width_bps: Vec<u16>,
    activation_budget_bps: Vec<u16>,
    fingerprint: Fingerprint,
}

impl ModelExecutionCapabilitiesV1 {
    /// Construct and validate one capability set.
    ///
    /// # Errors
    ///
    /// Rejects blank identities, zero total experts, empty level sets, duplicate
    /// levels, expert counts outside `1..=total_experts`, and basis-point values
    /// outside `1..=10_000`.
    pub fn new(
        provider_id: impl Into<String>,
        model_revision: impl Into<String>,
        total_experts: u32,
        mut active_expert_counts: Vec<u32>,
        mut expert_width_bps: Vec<u16>,
        mut activation_budget_bps: Vec<u16>,
    ) -> Result<Self, ModelExecutionContractError> {
        let provider_id = provider_id.into();
        if provider_id.trim().is_empty() {
            return Err(ModelExecutionContractError::BlankProviderId);
        }
        let model_revision = model_revision.into();
        if model_revision.trim().is_empty() {
            return Err(ModelExecutionContractError::BlankModelRevision);
        }
        if total_experts == 0 {
            return Err(ModelExecutionContractError::ZeroTotalExperts);
        }

        validate_expert_counts(&active_expert_counts, total_experts)?;
        validate_basis_points(
            &expert_width_bps,
            ModelExecutionBasisPointAxis::ExpertWidth,
        )?;
        validate_basis_points(
            &activation_budget_bps,
            ModelExecutionBasisPointAxis::ActivationBudget,
        )?;

        active_expert_counts.sort_unstable();
        expert_width_bps.sort_unstable();
        activation_budget_bps.sort_unstable();

        let mut fingerprint = Fingerprint::EMPTY
            .text(MODEL_EXECUTION_CAPABILITIES_V1)
            .text(provider_id.trim())
            .text(model_revision.trim())
            .number(u64::from(total_experts));
        for value in &active_expert_counts {
            fingerprint = fingerprint.number(u64::from(*value));
        }
        for value in &expert_width_bps {
            fingerprint = fingerprint.number(u64::from(*value));
        }
        for value in &activation_budget_bps {
            fingerprint = fingerprint.number(u64::from(*value));
        }

        Ok(Self {
            provider_id: provider_id.trim().to_owned(),
            model_revision: model_revision.trim().to_owned(),
            total_experts,
            active_expert_counts,
            expert_width_bps,
            activation_budget_bps,
            fingerprint,
        })
    }

    /// Provider/backend identity that owns the execution semantics.
    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Exact model revision for which these levels were qualified.
    #[must_use]
    pub fn model_revision(&self) -> &str {
        &self.model_revision
    }

    /// Total expert count in the provider-declared model topology.
    #[must_use]
    pub const fn total_experts(&self) -> u32 {
        self.total_experts
    }

    /// Qualified active-expert counts, sorted ascending.
    #[must_use]
    pub fn active_expert_counts(&self) -> &[u32] {
        &self.active_expert_counts
    }

    /// Qualified expert-width levels in basis points, sorted ascending.
    #[must_use]
    pub fn expert_width_bps(&self) -> &[u16] {
        &self.expert_width_bps
    }

    /// Qualified activation-budget levels in basis points, sorted ascending.
    #[must_use]
    pub fn activation_budget_bps(&self) -> &[u16] {
        &self.activation_budget_bps
    }

    /// Non-cryptographic structural identity of this exact capability set.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Whether one complete target tuple is admitted by this capability set.
    #[must_use]
    pub fn supports(
        &self,
        active_experts: u32,
        expert_width_bps: u16,
        activation_budget_bps: u16,
    ) -> bool {
        self.active_expert_counts.contains(&active_experts)
            && self.expert_width_bps.contains(&expert_width_bps)
            && self.activation_budget_bps.contains(&activation_budget_bps)
    }

    /// Convert this validated set to the strict v1 JSON envelope.
    #[must_use]
    pub fn to_wire(&self) -> ModelExecutionCapabilitiesWireV1 {
        ModelExecutionCapabilitiesWireV1 {
            contract: MODEL_EXECUTION_CAPABILITIES_V1.to_owned(),
            provider_id: self.provider_id.clone(),
            model_revision: self.model_revision.clone(),
            total_experts: self.total_experts,
            active_expert_counts: self.active_expert_counts.clone(),
            expert_width_bps: self.expert_width_bps.clone(),
            activation_budget_bps: self.activation_budget_bps.clone(),
        }
    }
}

/// Strict wire envelope for one validated model-execution target.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionResourcePlanWireV1 {
    contract: String,
    provider_id: String,
    model_revision: String,
    capability_fingerprint: String,
    active_experts: u32,
    expert_width_bps: u16,
    activation_budget_bps: u16,
}

impl ModelExecutionResourcePlanWireV1 {
    /// Revalidate this plan against the exact current capability set.
    ///
    /// # Errors
    ///
    /// Fails closed on contract, provider, model-revision, capability-fingerprint,
    /// or target-level mismatch.
    pub fn into_validated(
        self,
        capabilities: &ModelExecutionCapabilitiesV1,
    ) -> Result<ModelExecutionResourcePlanV1, ModelExecutionContractError> {
        if self.contract != MODEL_EXECUTION_RESOURCE_PLAN_V1 {
            return Err(ModelExecutionContractError::UnsupportedResourcePlanContract {
                contract: self.contract,
            });
        }
        if self.provider_id != capabilities.provider_id() {
            return Err(ModelExecutionContractError::ProviderMismatch {
                expected: capabilities.provider_id().to_owned(),
                actual: self.provider_id,
            });
        }
        if self.model_revision != capabilities.model_revision() {
            return Err(ModelExecutionContractError::ModelRevisionMismatch {
                expected: capabilities.model_revision().to_owned(),
                actual: self.model_revision,
            });
        }
        let expected_fingerprint = capabilities.fingerprint().to_string();
        if self.capability_fingerprint != expected_fingerprint {
            return Err(ModelExecutionContractError::CapabilityFingerprintMismatch {
                expected: expected_fingerprint,
                actual: self.capability_fingerprint,
            });
        }
        ModelExecutionResourcePlanV1::new(
            capabilities,
            self.active_experts,
            self.expert_width_bps,
            self.activation_budget_bps,
        )
    }
}

/// Validated pre-execution target for the three elastic model-execution axes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionResourcePlanV1 {
    provider_id: String,
    model_revision: String,
    capability_fingerprint: Fingerprint,
    active_experts: u32,
    expert_width_bps: u16,
    activation_budget_bps: u16,
}

impl ModelExecutionResourcePlanV1 {
    /// Validate a target tuple against one exact capability set.
    ///
    /// # Errors
    ///
    /// Each axis must select a discrete level published by `capabilities`.
    pub fn new(
        capabilities: &ModelExecutionCapabilitiesV1,
        active_experts: u32,
        expert_width_bps: u16,
        activation_budget_bps: u16,
    ) -> Result<Self, ModelExecutionContractError> {
        if !capabilities.active_expert_counts.contains(&active_experts) {
            return Err(ModelExecutionContractError::UnsupportedActiveExpertCount {
                value: active_experts,
            });
        }
        if !capabilities.expert_width_bps.contains(&expert_width_bps) {
            return Err(ModelExecutionContractError::UnsupportedExpertWidthBps {
                value: expert_width_bps,
            });
        }
        if !capabilities
            .activation_budget_bps
            .contains(&activation_budget_bps)
        {
            return Err(ModelExecutionContractError::UnsupportedActivationBudgetBps {
                value: activation_budget_bps,
            });
        }

        Ok(Self {
            provider_id: capabilities.provider_id.clone(),
            model_revision: capabilities.model_revision.clone(),
            capability_fingerprint: capabilities.fingerprint,
            active_experts,
            expert_width_bps,
            activation_budget_bps,
        })
    }

    /// Provider/backend identity bound to this plan.
    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Exact model revision bound to this plan.
    #[must_use]
    pub fn model_revision(&self) -> &str {
        &self.model_revision
    }

    /// Capability fingerprint against which this target was validated.
    #[must_use]
    pub const fn capability_fingerprint(&self) -> Fingerprint {
        self.capability_fingerprint
    }

    /// Number of experts requested active at the qualified execution point.
    #[must_use]
    pub const fn active_experts(&self) -> u32 {
        self.active_experts
    }

    /// Active expert-width level in integer basis points.
    #[must_use]
    pub const fn expert_width_bps(&self) -> u16 {
        self.expert_width_bps
    }

    /// Activation-compute budget in integer basis points.
    #[must_use]
    pub const fn activation_budget_bps(&self) -> u16 {
        self.activation_budget_bps
    }

    /// Convert this validated plan to its strict v1 JSON envelope.
    #[must_use]
    pub fn to_wire(&self) -> ModelExecutionResourcePlanWireV1 {
        ModelExecutionResourcePlanWireV1 {
            contract: MODEL_EXECUTION_RESOURCE_PLAN_V1.to_owned(),
            provider_id: self.provider_id.clone(),
            model_revision: self.model_revision.clone(),
            capability_fingerprint: self.capability_fingerprint.to_string(),
            active_experts: self.active_experts,
            expert_width_bps: self.expert_width_bps,
            activation_budget_bps: self.activation_budget_bps,
        }
    }

    /// Map the qualified target into ElasticXxx's generic resource declaration.
    ///
    /// The three axes are custom dimensions because the core intentionally keeps
    /// an open vocabulary. `Reinterpret` denotes pre-execution reconfiguration
    /// of an existing model materialization here; this method does not authorize
    /// live mutation. A future physical adapter must independently validate and
    /// verify any actuation before commit.
    pub fn resource_spec(
        &self,
        resource_id: impl Into<String>,
    ) -> Result<ResourceSpec, ModelExecutionContractError> {
        let resource_id = LogicalResourceId::new(resource_id.into())?;
        let contract = ContractId::new(MODEL_EXECUTION_RESOURCE_PLAN_V1)?;
        let active_experts = DimensionId::custom(MODEL_EXECUTION_ACTIVE_EXPERTS_DIMENSION)?;
        let expert_width = DimensionId::custom(MODEL_EXECUTION_EXPERT_WIDTH_DIMENSION)?;
        let activation_budget = DimensionId::custom(MODEL_EXECUTION_ACTIVATION_BUDGET_DIMENSION)?;

        let mut builder = ResourceSpec::builder(ResourceClassId::CONFIGURATIONAL, resource_id)
            .preserve(Invariant::new(InvariantKind::PreserveIdentity))
            .preserve(Invariant::new(InvariantKind::UpholdContract(contract)))
            .observe(ObservationSignalId::FREE_CAPACITY)
            .observe(ObservationSignalId::UTILIZATION)
            .observe(ObservationSignalId::QUEUE_DEPTH)
            .label("model-execution.contract", MODEL_EXECUTION_RESOURCE_PLAN_V1)
            .label("model-execution.provider", self.provider_id.clone())
            .label("model-execution.model-revision", self.model_revision.clone())
            .label(
                "model-execution.capability-fingerprint",
                self.capability_fingerprint.to_string(),
            );

        for dimension in [active_experts, expert_width, activation_budget] {
            builder = builder
                .allow(dimension.clone())
                .admit(AdmissibleTransition::new(
                    TransitionMechanism::Reinterpret,
                    dimension.clone(),
                ))
                .require_capability(CapabilityRequirement::new(
                    TransitionMechanism::Reinterpret,
                    dimension,
                ));
        }

        Ok(builder.build()?)
    }
}

/// Fail-closed errors for the model-execution capability/plan boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelExecutionContractError {
    /// Capability wire envelope carries an unknown contract identity.
    UnsupportedCapabilitiesContract { contract: String },
    /// Resource-plan wire envelope carries an unknown contract identity.
    UnsupportedResourcePlanContract { contract: String },
    /// Provider/backend identity is blank.
    BlankProviderId,
    /// Model revision identity is blank.
    BlankModelRevision,
    /// The topology cannot advertise zero total experts.
    ZeroTotalExperts,
    /// No active-expert level was advertised.
    EmptyActiveExpertCounts,
    /// One advertised active-expert count is outside the topology bound.
    InvalidActiveExpertCount { value: u32, total_experts: u32 },
    /// One active-expert count is advertised more than once.
    DuplicateActiveExpertCount { value: u32 },
    /// No level was advertised for one basis-point axis.
    EmptyBasisPointLevels { axis: ModelExecutionBasisPointAxis },
    /// A basis-point level is outside `1..=10_000`.
    InvalidBasisPointLevel {
        axis: ModelExecutionBasisPointAxis,
        value: u16,
    },
    /// A basis-point level is advertised more than once.
    DuplicateBasisPointLevel {
        axis: ModelExecutionBasisPointAxis,
        value: u16,
    },
    /// A plan names a different provider than the capability set.
    ProviderMismatch { expected: String, actual: String },
    /// A plan names a different model revision than the capability set.
    ModelRevisionMismatch { expected: String, actual: String },
    /// A plan was validated against a different capability set.
    CapabilityFingerprintMismatch { expected: String, actual: String },
    /// Requested expert count is absent from the qualified set.
    UnsupportedActiveExpertCount { value: u32 },
    /// Requested expert width is absent from the qualified set.
    UnsupportedExpertWidthBps { value: u16 },
    /// Requested activation budget is absent from the qualified set.
    UnsupportedActivationBudgetBps { value: u16 },
    /// Generic Elastic resource construction rejected the mapped declaration.
    ResourceSpec(ResourceSpecError),
}

impl fmt::Display for ModelExecutionContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCapabilitiesContract { contract } => write!(
                f,
                "model-execution capabilities contract {contract:?} is unsupported; expected {MODEL_EXECUTION_CAPABILITIES_V1}"
            ),
            Self::UnsupportedResourcePlanContract { contract } => write!(
                f,
                "model-execution resource-plan contract {contract:?} is unsupported; expected {MODEL_EXECUTION_RESOURCE_PLAN_V1}"
            ),
            Self::BlankProviderId => f.write_str("model-execution provider id must not be blank"),
            Self::BlankModelRevision => {
                f.write_str("model-execution model revision must not be blank")
            }
            Self::ZeroTotalExperts => {
                f.write_str("model-execution total expert count must be >= 1")
            }
            Self::EmptyActiveExpertCounts => {
                f.write_str("model-execution active-expert capability set must not be empty")
            }
            Self::InvalidActiveExpertCount {
                value,
                total_experts,
            } => write!(
                f,
                "active expert count must be in [1, {total_experts}]; got {value}"
            ),
            Self::DuplicateActiveExpertCount { value } => {
                write!(f, "active expert count {value} is duplicated")
            }
            Self::EmptyBasisPointLevels { axis } => {
                write!(f, "model-execution {axis} capability set must not be empty")
            }
            Self::InvalidBasisPointLevel { axis, value } => write!(
                f,
                "model-execution {axis} level must be in [1, {MODEL_EXECUTION_BASIS_POINTS_FULL}]; got {value}"
            ),
            Self::DuplicateBasisPointLevel { axis, value } => {
                write!(f, "model-execution {axis} level {value} is duplicated")
            }
            Self::ProviderMismatch { expected, actual } => write!(
                f,
                "model-execution provider mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::ModelRevisionMismatch { expected, actual } => write!(
                f,
                "model-execution revision mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::CapabilityFingerprintMismatch { expected, actual } => write!(
                f,
                "model-execution capability fingerprint mismatch: expected {expected}, got {actual}"
            ),
            Self::UnsupportedActiveExpertCount { value } => write!(
                f,
                "active expert count {value} is not published by the qualified capability set"
            ),
            Self::UnsupportedExpertWidthBps { value } => write!(
                f,
                "expert-width level {value} bps is not published by the qualified capability set"
            ),
            Self::UnsupportedActivationBudgetBps { value } => write!(
                f,
                "activation-budget level {value} bps is not published by the qualified capability set"
            ),
            Self::ResourceSpec(error) => write!(f, "invalid Elastic resource mapping: {error}"),
        }
    }
}

impl std::error::Error for ModelExecutionContractError {}

impl From<ResourceSpecError> for ModelExecutionContractError {
    fn from(value: ResourceSpecError) -> Self {
        Self::ResourceSpec(value)
    }
}

fn validate_expert_counts(
    values: &[u32],
    total_experts: u32,
) -> Result<(), ModelExecutionContractError> {
    if values.is_empty() {
        return Err(ModelExecutionContractError::EmptyActiveExpertCounts);
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    for &value in &sorted {
        if value == 0 || value > total_experts {
            return Err(ModelExecutionContractError::InvalidActiveExpertCount {
                value,
                total_experts,
            });
        }
    }
    for pair in sorted.windows(2) {
        if pair[0] == pair[1] {
            return Err(ModelExecutionContractError::DuplicateActiveExpertCount {
                value: pair[0],
            });
        }
    }
    Ok(())
}

fn validate_basis_points(
    values: &[u16],
    axis: ModelExecutionBasisPointAxis,
) -> Result<(), ModelExecutionContractError> {
    if values.is_empty() {
        return Err(ModelExecutionContractError::EmptyBasisPointLevels { axis });
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    for &value in &sorted {
        if !(1..=MODEL_EXECUTION_BASIS_POINTS_FULL).contains(&value) {
            return Err(ModelExecutionContractError::InvalidBasisPointLevel { axis, value });
        }
    }
    for pair in sorted.windows(2) {
        if pair[0] == pair[1] {
            return Err(ModelExecutionContractError::DuplicateBasisPointLevel {
                axis,
                value: pair[0],
            });
        }
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
            vec![4, 1, 2],
            vec![10_000, 2_500, 5_000],
            vec![10_000, 2_500, 5_000],
        )
        .unwrap()
    }

    #[test]
    fn capability_levels_are_canonicalized_and_fingerprinted() {
        let capabilities = capabilities();
        assert_eq!(capabilities.active_expert_counts(), &[1, 2, 4]);
        assert_eq!(capabilities.expert_width_bps(), &[2_500, 5_000, 10_000]);
        assert_eq!(
            capabilities.activation_budget_bps(),
            &[2_500, 5_000, 10_000]
        );

        let changed = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 8],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        assert_ne!(capabilities.fingerprint(), changed.fingerprint());
    }

    #[test]
    fn invalid_capability_levels_fail_closed() {
        assert_eq!(
            ModelExecutionCapabilitiesV1::new(
                "backend",
                "rev",
                8,
                vec![0, 1],
                vec![10_000],
                vec![10_000],
            )
            .unwrap_err(),
            ModelExecutionContractError::InvalidActiveExpertCount {
                value: 0,
                total_experts: 8
            }
        );
        assert_eq!(
            ModelExecutionCapabilitiesV1::new(
                "backend",
                "rev",
                8,
                vec![1],
                vec![0],
                vec![10_000],
            )
            .unwrap_err(),
            ModelExecutionContractError::InvalidBasisPointLevel {
                axis: ModelExecutionBasisPointAxis::ExpertWidth,
                value: 0
            }
        );
    }

    #[test]
    fn plan_accepts_only_published_discrete_levels() {
        let capabilities = capabilities();
        let plan = ModelExecutionResourcePlanV1::new(&capabilities, 2, 5_000, 2_500).unwrap();
        assert_eq!(plan.active_experts(), 2);
        assert_eq!(plan.expert_width_bps(), 5_000);
        assert_eq!(plan.activation_budget_bps(), 2_500);

        assert_eq!(
            ModelExecutionResourcePlanV1::new(&capabilities, 3, 5_000, 2_500).unwrap_err(),
            ModelExecutionContractError::UnsupportedActiveExpertCount { value: 3 }
        );
    }

    #[test]
    fn plan_wire_is_bound_to_exact_capability_fingerprint() {
        let capabilities = capabilities();
        let plan = ModelExecutionResourcePlanV1::new(&capabilities, 1, 2_500, 2_500).unwrap();
        let json = serde_json::to_string(&plan.to_wire()).unwrap();
        let decoded: ModelExecutionResourcePlanWireV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.into_validated(&capabilities).unwrap(), plan);

        let changed = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4, 8],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let decoded: ModelExecutionResourcePlanWireV1 = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            decoded.into_validated(&changed).unwrap_err(),
            ModelExecutionContractError::CapabilityFingerprintMismatch { .. }
        ));
    }

    #[test]
    fn capability_wire_round_trip_revalidates_and_rejects_unknown_fields() {
        let capabilities = capabilities();
        let json = serde_json::to_string(&capabilities.to_wire()).unwrap();
        let wire: ModelExecutionCapabilitiesWireV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.into_validated().unwrap(), capabilities);

        let raw = format!(
            r#"{{"contract":"{MODEL_EXECUTION_CAPABILITIES_V1}","provider_id":"backend","model_revision":"rev","total_experts":8,"active_expert_counts":[1],"expert_width_bps":[10000],"activation_budget_bps":[10000],"extra":true}}"#
        );
        assert!(serde_json::from_str::<ModelExecutionCapabilitiesWireV1>(&raw).is_err());
    }

    #[test]
    fn resource_mapping_uses_three_open_set_dimensions() {
        let capabilities = capabilities();
        let plan = ModelExecutionResourcePlanV1::new(&capabilities, 4, 10_000, 5_000).unwrap();
        let spec = plan.resource_spec("model-execution").unwrap();

        for text in [
            MODEL_EXECUTION_ACTIVE_EXPERTS_DIMENSION,
            MODEL_EXECUTION_EXPERT_WIDTH_DIMENSION,
            MODEL_EXECUTION_ACTIVATION_BUDGET_DIMENSION,
        ] {
            let dimension = DimensionId::custom(text).unwrap();
            assert!(spec.is_elastic(&dimension));
            assert!(spec.admits(TransitionMechanism::Reinterpret, &dimension));
        }
        assert_eq!(
            spec.label("model-execution.contract"),
            Some(MODEL_EXECUTION_RESOURCE_PLAN_V1)
        );
        assert_eq!(spec.label("model-execution.provider"), Some("reference-backend"));
    }
}
