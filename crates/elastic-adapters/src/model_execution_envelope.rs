//! Backend-supplied resource-envelope policy for correlated model execution.
//!
//! This module turns explicit resource observations into one already-qualified
//! [`ModelExecutionProfileEnvelopeV1`]. It deliberately does not probe hardware
//! or invent a universal mapping from RAM/VRAM/thermal state to model semantics.
//! The backend supplies the capacity unit, thresholds, rule order, and envelopes.
//!
//! The resulting envelope can then be passed to the correlated-profile selector,
//! preserving the profile-set correlation contract before any future actuation.

use crate::model_execution::MODEL_EXECUTION_BASIS_POINTS_FULL;
use crate::model_execution_profiles::{
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileError, ModelExecutionProfilePlanV1,
    ModelExecutionProfileSelectionV1, ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1,
};
use elastic_eir::Fingerprint;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Versioned backend-supplied envelope policy contract.
pub const MODEL_EXECUTION_ENVELOPE_POLICY_V1: &str =
    "elastic.model-execution.envelope-policy@1.0.0";
/// JSON media type for [`MODEL_EXECUTION_ENVELOPE_POLICY_V1`].
pub const MODEL_EXECUTION_ENVELOPE_POLICY_MEDIA_TYPE_V1: &str =
    "application/vnd.elastic.model-execution-envelope-policy.v1+json";

/// One explicit resource snapshot supplied by a trusted observer/backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionResourceSnapshotV1 {
    capacity_unit: String,
    free_capacity: u64,
    utilization_bps: u16,
}

impl ModelExecutionResourceSnapshotV1 {
    /// Construct one already-observed resource snapshot.
    ///
    /// `capacity_unit` is an opaque backend-owned unit identity such as `bytes`
    /// or `cuda-device-bytes`. ElasticXxx only checks exact identity equality.
    ///
    /// # Errors
    ///
    /// Rejects a blank capacity unit or utilization outside `0..=10_000` bps.
    pub fn new(
        capacity_unit: impl Into<String>,
        free_capacity: u64,
        utilization_bps: u16,
    ) -> Result<Self, ModelExecutionEnvelopeError> {
        let capacity_unit = capacity_unit.into();
        let capacity_unit = capacity_unit.trim();
        if capacity_unit.is_empty() {
            return Err(ModelExecutionEnvelopeError::BlankCapacityUnit);
        }
        if utilization_bps > MODEL_EXECUTION_BASIS_POINTS_FULL {
            return Err(ModelExecutionEnvelopeError::InvalidUtilizationBps {
                value: utilization_bps,
            });
        }
        Ok(Self {
            capacity_unit: capacity_unit.to_owned(),
            free_capacity,
            utilization_bps,
        })
    }

    /// Backend-owned capacity unit identity.
    #[must_use]
    pub fn capacity_unit(&self) -> &str {
        &self.capacity_unit
    }

    /// Observed free capacity in the declared backend unit.
    #[must_use]
    pub const fn free_capacity(&self) -> u64 {
        self.free_capacity
    }

    /// Observed utilization in basis points.
    #[must_use]
    pub const fn utilization_bps(&self) -> u16 {
        self.utilization_bps
    }
}

/// One provider-owned threshold rule mapping observations to a profile envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionEnvelopeRuleV1 {
    rule_id: String,
    preference_rank: u32,
    min_free_capacity: u64,
    max_utilization_bps: u16,
    envelope: ModelExecutionProfileEnvelopeV1,
}

impl ModelExecutionEnvelopeRuleV1 {
    /// Define one rule before binding it to an exact profile set.
    ///
    /// Lower `preference_rank` values are evaluated first. A rule matches when
    /// `free_capacity >= min_free_capacity` and
    /// `utilization_bps <= max_utilization_bps`.
    ///
    /// # Errors
    ///
    /// Rejects a blank rule id or utilization above `10_000` bps.
    pub fn new(
        rule_id: impl Into<String>,
        preference_rank: u32,
        min_free_capacity: u64,
        max_utilization_bps: u16,
        envelope: ModelExecutionProfileEnvelopeV1,
    ) -> Result<Self, ModelExecutionEnvelopeError> {
        let rule_id = rule_id.into();
        let rule_id = rule_id.trim();
        if rule_id.is_empty() {
            return Err(ModelExecutionEnvelopeError::BlankRuleId);
        }
        if max_utilization_bps > MODEL_EXECUTION_BASIS_POINTS_FULL {
            return Err(ModelExecutionEnvelopeError::InvalidUtilizationBps {
                value: max_utilization_bps,
            });
        }
        Ok(Self {
            rule_id: rule_id.to_owned(),
            preference_rank,
            min_free_capacity,
            max_utilization_bps,
            envelope,
        })
    }

    /// Stable provider-owned rule identity.
    #[must_use]
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    /// Provider-defined rule preference; lower values are evaluated first.
    #[must_use]
    pub const fn preference_rank(&self) -> u32 {
        self.preference_rank
    }

    /// Minimum free capacity required for this rule.
    #[must_use]
    pub const fn min_free_capacity(&self) -> u64 {
        self.min_free_capacity
    }

    /// Maximum utilization admitted for this rule.
    #[must_use]
    pub const fn max_utilization_bps(&self) -> u16 {
        self.max_utilization_bps
    }

    /// Profile envelope produced by this rule.
    #[must_use]
    pub const fn envelope(&self) -> ModelExecutionProfileEnvelopeV1 {
        self.envelope
    }

    /// Whether the supplied observation matches this rule's explicit thresholds.
    #[must_use]
    pub fn matches(&self, snapshot: &ModelExecutionResourceSnapshotV1) -> bool {
        snapshot.free_capacity >= self.min_free_capacity
            && snapshot.utilization_bps <= self.max_utilization_bps
    }
}

/// Wire form of one backend envelope rule.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionEnvelopeRuleWireV1 {
    rule_id: String,
    preference_rank: u32,
    min_free_capacity: u64,
    max_utilization_bps: u16,
    max_active_experts: u32,
    max_expert_width_bps: u16,
    max_activation_budget_bps: u16,
}

impl ModelExecutionEnvelopeRuleWireV1 {
    fn into_native(self) -> Result<ModelExecutionEnvelopeRuleV1, ModelExecutionEnvelopeError> {
        let envelope = ModelExecutionProfileEnvelopeV1::new(
            self.max_active_experts,
            self.max_expert_width_bps,
            self.max_activation_budget_bps,
        )?;
        ModelExecutionEnvelopeRuleV1::new(
            self.rule_id,
            self.preference_rank,
            self.min_free_capacity,
            self.max_utilization_bps,
            envelope,
        )
    }
}

impl From<&ModelExecutionEnvelopeRuleV1> for ModelExecutionEnvelopeRuleWireV1 {
    fn from(rule: &ModelExecutionEnvelopeRuleV1) -> Self {
        let envelope = rule.envelope;
        Self {
            rule_id: rule.rule_id.clone(),
            preference_rank: rule.preference_rank,
            min_free_capacity: rule.min_free_capacity,
            max_utilization_bps: rule.max_utilization_bps,
            max_active_experts: envelope.max_active_experts(),
            max_expert_width_bps: envelope.max_expert_width_bps(),
            max_activation_budget_bps: envelope.max_activation_budget_bps(),
        }
    }
}

/// Strict wire form of an envelope policy bound to an exact profile set.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionEnvelopePolicyWireV1 {
    contract: String,
    provider_id: String,
    model_revision: String,
    capability_fingerprint: String,
    profile_set_fingerprint: String,
    capacity_unit: String,
    rules: Vec<ModelExecutionEnvelopeRuleWireV1>,
}

impl ModelExecutionEnvelopePolicyWireV1 {
    /// Revalidate this wire policy against the exact current profile set.
    ///
    /// # Errors
    ///
    /// Fails closed on contract/identity/fingerprint mismatch or invalid rules.
    pub fn into_validated(
        self,
        profiles: &ModelExecutionProfileSetV1,
    ) -> Result<ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeError> {
        if self.contract != MODEL_EXECUTION_ENVELOPE_POLICY_V1 {
            return Err(ModelExecutionEnvelopeError::UnsupportedPolicyContract {
                contract: self.contract,
            });
        }
        validate_profile_identity(
            &self.provider_id,
            &self.model_revision,
            &self.capability_fingerprint,
            &self.profile_set_fingerprint,
            profiles,
        )?;
        let rules = self
            .rules
            .into_iter()
            .map(ModelExecutionEnvelopeRuleWireV1::into_native)
            .collect::<Result<Vec<_>, _>>()?;
        ModelExecutionEnvelopePolicyV1::new(profiles, self.capacity_unit, rules)
    }
}

/// Validated backend policy mapping resource observations to profile envelopes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionEnvelopePolicyV1 {
    provider_id: String,
    model_revision: String,
    capability_fingerprint: Fingerprint,
    profile_set_fingerprint: Fingerprint,
    capacity_unit: String,
    rules: Vec<ModelExecutionEnvelopeRuleV1>,
    fingerprint: Fingerprint,
}

impl ModelExecutionEnvelopePolicyV1 {
    /// Bind a rule table to one exact correlated profile set.
    ///
    /// Every rule must admit at least one currently published correlated profile;
    /// this prevents a policy from resolving to an envelope that can never select
    /// a valid tuple.
    ///
    /// # Errors
    ///
    /// Rejects blank units, empty rule tables, duplicate ids/ranks, or rules with
    /// no feasible correlated profile.
    pub fn new(
        profiles: &ModelExecutionProfileSetV1,
        capacity_unit: impl Into<String>,
        mut rules: Vec<ModelExecutionEnvelopeRuleV1>,
    ) -> Result<Self, ModelExecutionEnvelopeError> {
        let capacity_unit = capacity_unit.into();
        let capacity_unit = capacity_unit.trim();
        if capacity_unit.is_empty() {
            return Err(ModelExecutionEnvelopeError::BlankCapacityUnit);
        }
        if rules.is_empty() {
            return Err(ModelExecutionEnvelopeError::EmptyRules);
        }

        rules.sort_by(|left, right| {
            left.preference_rank
                .cmp(&right.preference_rank)
                .then_with(|| left.rule_id.cmp(&right.rule_id))
        });
        for (index, rule) in rules.iter().enumerate() {
            for previous in &rules[..index] {
                if previous.rule_id == rule.rule_id {
                    return Err(ModelExecutionEnvelopeError::DuplicateRuleId {
                        rule_id: rule.rule_id.clone(),
                    });
                }
                if previous.preference_rank == rule.preference_rank {
                    return Err(ModelExecutionEnvelopeError::DuplicateRuleRank {
                        rank: rule.preference_rank,
                    });
                }
            }
            if !profiles
                .profiles()
                .iter()
                .any(|profile| rule.envelope.allows(profile))
            {
                return Err(ModelExecutionEnvelopeError::RuleHasNoFeasibleProfile {
                    rule_id: rule.rule_id.clone(),
                });
            }
        }

        let provider_id = profiles.provider_id().to_owned();
        let model_revision = profiles.model_revision().to_owned();
        let capability_fingerprint = profiles.capability_fingerprint();
        let profile_set_fingerprint = profiles.fingerprint();
        let mut fingerprint = Fingerprint::EMPTY
            .text(MODEL_EXECUTION_ENVELOPE_POLICY_V1)
            .text(&provider_id)
            .text(&model_revision)
            .number(capability_fingerprint.bits())
            .number(profile_set_fingerprint.bits())
            .text(capacity_unit);
        for rule in &rules {
            let envelope = rule.envelope;
            fingerprint = fingerprint
                .text(&rule.rule_id)
                .number(u64::from(rule.preference_rank))
                .number(rule.min_free_capacity)
                .number(u64::from(rule.max_utilization_bps))
                .number(u64::from(envelope.max_active_experts()))
                .number(u64::from(envelope.max_expert_width_bps()))
                .number(u64::from(envelope.max_activation_budget_bps()));
        }

        Ok(Self {
            provider_id,
            model_revision,
            capability_fingerprint,
            profile_set_fingerprint,
            capacity_unit: capacity_unit.to_owned(),
            rules,
            fingerprint,
        })
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

    /// Backend-owned capacity unit identity.
    #[must_use]
    pub fn capacity_unit(&self) -> &str {
        &self.capacity_unit
    }

    /// Rule table in deterministic provider-preference order.
    #[must_use]
    pub fn rules(&self) -> &[ModelExecutionEnvelopeRuleV1] {
        &self.rules
    }

    /// Structural fingerprint of this exact policy.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Convert this policy to its strict v1 JSON envelope.
    #[must_use]
    pub fn to_wire(&self) -> ModelExecutionEnvelopePolicyWireV1 {
        ModelExecutionEnvelopePolicyWireV1 {
            contract: MODEL_EXECUTION_ENVELOPE_POLICY_V1.to_owned(),
            provider_id: self.provider_id.clone(),
            model_revision: self.model_revision.clone(),
            capability_fingerprint: self.capability_fingerprint.to_string(),
            profile_set_fingerprint: self.profile_set_fingerprint.to_string(),
            capacity_unit: self.capacity_unit.clone(),
            rules: self.rules.iter().map(Into::into).collect(),
        }
    }
}

/// End-to-end result of resolving a resource snapshot and selecting a profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelExecutionHardwareSelectionV1 {
    /// A policy rule matched and selected one correlated profile.
    Selected {
        /// Matched provider rule identity.
        rule_id: String,
        /// Provider rule preference rank.
        rule_rank: u32,
        /// Selected correlated model-execution plan.
        plan: ModelExecutionProfilePlanV1,
    },
    /// No backend rule matched the supplied resource snapshot.
    NoMatchingRule,
    /// A matched rule produced no feasible profile; retained as an explicit
    /// fail-closed outcome even though policy construction prevents it for an
    /// unchanged profile set.
    NoFeasibleProfile { rule_id: String },
}

/// Deterministic bridge from backend observations to correlated profile choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelExecutionHardwarePlannerV1;

impl ModelExecutionHardwarePlannerV1 {
    /// Resolve a resource snapshot through `policy` and then select a correlated
    /// profile from `profiles`.
    ///
    /// # Errors
    ///
    /// Fails closed if policy/profile identity is stale, the snapshot capacity
    /// unit differs, or the correlated selector fails validation.
    pub fn select(
        &self,
        policy: &ModelExecutionEnvelopePolicyV1,
        profiles: &ModelExecutionProfileSetV1,
        snapshot: &ModelExecutionResourceSnapshotV1,
    ) -> Result<ModelExecutionHardwareSelectionV1, ModelExecutionEnvelopeError> {
        validate_policy_identity(policy, profiles)?;
        if snapshot.capacity_unit != policy.capacity_unit {
            return Err(ModelExecutionEnvelopeError::CapacityUnitMismatch {
                expected: policy.capacity_unit.clone(),
                actual: snapshot.capacity_unit.clone(),
            });
        }
        let Some(rule) = policy.rules.iter().find(|rule| rule.matches(snapshot)) else {
            return Ok(ModelExecutionHardwareSelectionV1::NoMatchingRule);
        };
        match ModelExecutionProfileSelectorV1.select(profiles, rule.envelope)? {
            ModelExecutionProfileSelectionV1::Selected(plan) => {
                Ok(ModelExecutionHardwareSelectionV1::Selected {
                    rule_id: rule.rule_id.clone(),
                    rule_rank: rule.preference_rank,
                    plan,
                })
            }
            ModelExecutionProfileSelectionV1::NoFeasibleProfile => {
                Ok(ModelExecutionHardwareSelectionV1::NoFeasibleProfile {
                    rule_id: rule.rule_id.clone(),
                })
            }
        }
    }
}

/// Fail-closed errors for backend envelope resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelExecutionEnvelopeError {
    /// Wire envelope carries an unknown policy contract.
    UnsupportedPolicyContract { contract: String },
    /// Capacity unit is blank.
    BlankCapacityUnit,
    /// Rule identity is blank.
    BlankRuleId,
    /// Utilization must be in `0..=10_000` basis points.
    InvalidUtilizationBps { value: u16 },
    /// Policy must contain at least one rule.
    EmptyRules,
    /// Rule ids must be unique.
    DuplicateRuleId { rule_id: String },
    /// Rule preference ranks must be unique.
    DuplicateRuleRank { rank: u32 },
    /// One rule's envelope cannot select any correlated profile.
    RuleHasNoFeasibleProfile { rule_id: String },
    /// Provider identity differs from the bound profile set.
    ProviderMismatch { expected: String, actual: String },
    /// Model revision differs from the bound profile set.
    ModelRevisionMismatch { expected: String, actual: String },
    /// Base capability fingerprint differs.
    CapabilityFingerprintMismatch { expected: String, actual: String },
    /// Correlated profile-set fingerprint differs.
    ProfileSetFingerprintMismatch { expected: String, actual: String },
    /// Resource snapshot uses a different backend capacity unit.
    CapacityUnitMismatch { expected: String, actual: String },
    /// Correlated profile validation/selection failed.
    Profile(ModelExecutionProfileError),
}

impl fmt::Display for ModelExecutionEnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPolicyContract { contract } => write!(
                f,
                "model-execution envelope policy {contract:?} is unsupported; expected {MODEL_EXECUTION_ENVELOPE_POLICY_V1}"
            ),
            Self::BlankCapacityUnit => f.write_str("model-execution capacity unit must not be blank"),
            Self::BlankRuleId => f.write_str("model-execution envelope rule id must not be blank"),
            Self::InvalidUtilizationBps { value } => write!(
                f,
                "model-execution utilization must be in [0, {MODEL_EXECUTION_BASIS_POINTS_FULL}] bps; got {value}"
            ),
            Self::EmptyRules => f.write_str("model-execution envelope policy must contain rules"),
            Self::DuplicateRuleId { rule_id } => {
                write!(f, "model-execution envelope rule id {rule_id:?} is duplicated")
            }
            Self::DuplicateRuleRank { rank } => {
                write!(f, "model-execution envelope rule rank {rank} is duplicated")
            }
            Self::RuleHasNoFeasibleProfile { rule_id } => write!(
                f,
                "model-execution envelope rule {rule_id:?} admits no correlated profile"
            ),
            Self::ProviderMismatch { expected, actual } => write!(
                f,
                "model-execution envelope provider mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::ModelRevisionMismatch { expected, actual } => write!(
                f,
                "model-execution envelope revision mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::CapabilityFingerprintMismatch { expected, actual } => write!(
                f,
                "model-execution envelope capability fingerprint mismatch: expected {expected}, got {actual}"
            ),
            Self::ProfileSetFingerprintMismatch { expected, actual } => write!(
                f,
                "model-execution envelope profile-set fingerprint mismatch: expected {expected}, got {actual}"
            ),
            Self::CapacityUnitMismatch { expected, actual } => write!(
                f,
                "model-execution capacity unit mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::Profile(error) => write!(f, "model-execution profile selection failed: {error}"),
        }
    }
}

impl std::error::Error for ModelExecutionEnvelopeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Profile(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ModelExecutionProfileError> for ModelExecutionEnvelopeError {
    fn from(value: ModelExecutionProfileError) -> Self {
        Self::Profile(value)
    }
}

fn validate_profile_identity(
    provider_id: &str,
    model_revision: &str,
    capability_fingerprint: &str,
    profile_set_fingerprint: &str,
    profiles: &ModelExecutionProfileSetV1,
) -> Result<(), ModelExecutionEnvelopeError> {
    if provider_id != profiles.provider_id() {
        return Err(ModelExecutionEnvelopeError::ProviderMismatch {
            expected: profiles.provider_id().to_owned(),
            actual: provider_id.to_owned(),
        });
    }
    if model_revision != profiles.model_revision() {
        return Err(ModelExecutionEnvelopeError::ModelRevisionMismatch {
            expected: profiles.model_revision().to_owned(),
            actual: model_revision.to_owned(),
        });
    }
    let expected_capability = profiles.capability_fingerprint().to_string();
    if capability_fingerprint != expected_capability {
        return Err(ModelExecutionEnvelopeError::CapabilityFingerprintMismatch {
            expected: expected_capability,
            actual: capability_fingerprint.to_owned(),
        });
    }
    let expected_profiles = profiles.fingerprint().to_string();
    if profile_set_fingerprint != expected_profiles {
        return Err(ModelExecutionEnvelopeError::ProfileSetFingerprintMismatch {
            expected: expected_profiles,
            actual: profile_set_fingerprint.to_owned(),
        });
    }
    Ok(())
}

fn validate_policy_identity(
    policy: &ModelExecutionEnvelopePolicyV1,
    profiles: &ModelExecutionProfileSetV1,
) -> Result<(), ModelExecutionEnvelopeError> {
    if policy.provider_id != profiles.provider_id() {
        return Err(ModelExecutionEnvelopeError::ProviderMismatch {
            expected: profiles.provider_id().to_owned(),
            actual: policy.provider_id.clone(),
        });
    }
    if policy.model_revision != profiles.model_revision() {
        return Err(ModelExecutionEnvelopeError::ModelRevisionMismatch {
            expected: profiles.model_revision().to_owned(),
            actual: policy.model_revision.clone(),
        });
    }
    if policy.capability_fingerprint != profiles.capability_fingerprint() {
        return Err(ModelExecutionEnvelopeError::CapabilityFingerprintMismatch {
            expected: profiles.capability_fingerprint().to_string(),
            actual: policy.capability_fingerprint.to_string(),
        });
    }
    if policy.profile_set_fingerprint != profiles.fingerprint() {
        return Err(ModelExecutionEnvelopeError::ProfileSetFingerprintMismatch {
            expected: profiles.fingerprint().to_string(),
            actual: policy.profile_set_fingerprint.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_execution::ModelExecutionCapabilitiesV1;
    use crate::model_execution_profiles::ModelExecutionProfileV1;

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

    #[test]
    fn hardware_planner_selects_rule_then_correlated_profile() {
        let profiles = profiles();
        let policy = policy(&profiles);
        let snapshot = ModelExecutionResourceSnapshotV1::new("bytes", 3_000, 8_000).unwrap();
        let selection = ModelExecutionHardwarePlannerV1
            .select(&policy, &profiles, &snapshot)
            .unwrap();
        let ModelExecutionHardwareSelectionV1::Selected {
            rule_id,
            rule_rank,
            plan,
        } = selection
        else {
            panic!("expected selected profile")
        };
        assert_eq!(rule_id, "balanced");
        assert_eq!(rule_rank, 10);
        assert_eq!(plan.profile_id(), "balanced");
    }

    #[test]
    fn rule_order_is_provider_defined_and_deterministic() {
        let profiles = profiles();
        let policy = policy(&profiles);
        let snapshot = ModelExecutionResourceSnapshotV1::new("bytes", 9_000, 6_000).unwrap();
        let selection = ModelExecutionHardwarePlannerV1
            .select(&policy, &profiles, &snapshot)
            .unwrap();
        let ModelExecutionHardwareSelectionV1::Selected { rule_id, plan, .. } = selection else {
            panic!("expected selected profile")
        };
        assert_eq!(rule_id, "rich");
        assert_eq!(plan.profile_id(), "full");
    }

    #[test]
    fn capacity_unit_mismatch_fails_closed() {
        let profiles = profiles();
        let policy = policy(&profiles);
        let snapshot = ModelExecutionResourceSnapshotV1::new("mib", 3_000, 8_000).unwrap();
        assert!(matches!(
            ModelExecutionHardwarePlannerV1.select(&policy, &profiles, &snapshot),
            Err(ModelExecutionEnvelopeError::CapacityUnitMismatch { .. })
        ));
    }

    #[test]
    fn policy_rejects_envelope_with_no_correlated_profile() {
        let profiles = profiles();
        let error = ModelExecutionEnvelopePolicyV1::new(
            &profiles,
            "bytes",
            vec![ModelExecutionEnvelopeRuleV1::new(
                "impossible",
                0,
                0,
                10_000,
                ModelExecutionProfileEnvelopeV1::new(1, 5_000, 2_000).unwrap(),
            )
            .unwrap()],
        )
        .unwrap_err();
        assert_eq!(
            error,
            ModelExecutionEnvelopeError::RuleHasNoFeasibleProfile {
                rule_id: "impossible".to_owned()
            }
        );
    }

    #[test]
    fn policy_wire_round_trip_revalidates_profile_identity() {
        let profiles = profiles();
        let policy = policy(&profiles);
        let json = serde_json::to_string(&policy.to_wire()).unwrap();
        let wire: ModelExecutionEnvelopePolicyWireV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(wire.into_validated(&profiles).unwrap(), policy);
    }

    #[test]
    fn stale_profile_set_is_rejected() {
        let profiles = profiles();
        let policy = policy(&profiles);
        let capabilities = profiles.capabilities().clone();
        let changed = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
            ],
        )
        .unwrap();
        let snapshot = ModelExecutionResourceSnapshotV1::new("bytes", 9_000, 6_000).unwrap();
        assert!(matches!(
            ModelExecutionHardwarePlannerV1.select(&policy, &changed, &snapshot),
            Err(ModelExecutionEnvelopeError::ProfileSetFingerprintMismatch { .. })
        ));
    }
}
