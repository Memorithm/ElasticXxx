//! ELANG5c numeric objective metadata and advisory planner hints in EIR.
//!
//! The base [`crate::EirResourcePolicy`] remains the semantic policy identity
//! for target + guards + pseudo-Boolean constraints. This envelope adds a
//! separate advisory fingerprint so hints never silently change eligibility.

use crate::{lower_resource_policy, EirResourcePolicy, Fingerprint, ResourcePolicyLoweringError};
use elastic_core::{
    ObjectiveId, PlannerHint, PolicyNumericObjective, PolicyObjectiveDirection,
    ResourcePolicyAdvisorySpec,
};
use std::fmt;

/// Schema version of the advisory resource-policy EIR envelope.
pub const EIR_RESOURCE_POLICY_ADVISORY_SCHEMA_VERSION: u16 = 1;

/// Numeric metadata for one objective in authoritative resource priority order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirPolicyNumericObjective {
    rank: u32,
    objective: ObjectiveId,
    direction: PolicyObjectiveDirection,
    unit: String,
    quantum: u64,
    fingerprint: Fingerprint,
}

impl EirPolicyNumericObjective {
    fn lower(
        rank: usize,
        value: &PolicyNumericObjective,
    ) -> Result<Self, PolicyAdvisoryLoweringError> {
        let rank = u32::try_from(rank)
            .map_err(|_| PolicyAdvisoryLoweringError::ObjectiveRankOverflow { rank })?;
        let fingerprint = Fingerprint::EMPTY
            .text("eir-policy-numeric-objective")
            .number(u64::from(EIR_RESOURCE_POLICY_ADVISORY_SCHEMA_VERSION))
            .number(u64::from(rank))
            .text(objective_kind(value.objective()))
            .text(value.objective().as_str())
            .text(value.direction().as_str())
            .text(value.scale().unit())
            .number(value.scale().quantum());
        Ok(Self {
            rank,
            objective: value.objective().clone(),
            direction: value.direction(),
            unit: value.scale().unit().to_owned(),
            quantum: value.scale().quantum(),
            fingerprint,
        })
    }

    #[must_use]
    pub const fn rank(&self) -> u32 {
        self.rank
    }

    #[must_use]
    pub const fn objective(&self) -> &ObjectiveId {
        &self.objective
    }

    #[must_use]
    pub const fn direction(&self) -> PolicyObjectiveDirection {
        self.direction
    }

    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    #[must_use]
    pub const fn quantum(&self) -> u64 {
        self.quantum
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Canonical advisory planner hint in EIR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirPlannerHint {
    key: String,
    value: String,
    fingerprint: Fingerprint,
}

impl EirPlannerHint {
    fn lower(value: &PlannerHint) -> Self {
        let fingerprint = Fingerprint::EMPTY
            .text("eir-planner-hint")
            .number(u64::from(EIR_RESOURCE_POLICY_ADVISORY_SCHEMA_VERSION))
            .text(value.key().as_str())
            .text(value.value());
        Self {
            key: value.key().as_str().to_owned(),
            value: value.value().to_owned(),
            fingerprint,
        }
    }

    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Resource policy plus explicitly non-authoritative numeric/advisory metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirResourcePolicyAdvisory {
    policy: EirResourcePolicy,
    numeric_objectives: Vec<EirPolicyNumericObjective>,
    planner_hints: Vec<EirPlannerHint>,
    fingerprint: Fingerprint,
}

impl EirResourcePolicyAdvisory {
    /// Underlying semantic resource policy. Its fingerprint is intentionally
    /// independent from advisory hints and numeric metadata.
    #[must_use]
    pub const fn policy(&self) -> &EirResourcePolicy {
        &self.policy
    }

    #[must_use]
    pub fn numeric_objectives(&self) -> &[EirPolicyNumericObjective] {
        &self.numeric_objectives
    }

    #[must_use]
    pub fn planner_hints(&self) -> &[EirPlannerHint] {
        &self.planner_hints
    }

    /// Identity of the full advisory envelope, not eligibility authority.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Lower advisory policy metadata without giving it evaluation authority.
pub fn lower_resource_policy_advisory(
    advisory: &ResourcePolicyAdvisorySpec,
) -> Result<EirResourcePolicyAdvisory, PolicyAdvisoryLoweringError> {
    let policy = lower_resource_policy(advisory.policy())
        .map_err(PolicyAdvisoryLoweringError::ResourcePolicy)?;
    let numeric_objectives = advisory
        .numeric_objectives()
        .iter()
        .enumerate()
        .map(|(rank, objective)| EirPolicyNumericObjective::lower(rank, objective))
        .collect::<Result<Vec<_>, _>>()?;
    let planner_hints = advisory
        .planner_hints()
        .iter()
        .map(EirPlannerHint::lower)
        .collect::<Vec<_>>();

    let mut fingerprint = Fingerprint::EMPTY
        .text("eir-resource-policy-advisory")
        .number(u64::from(EIR_RESOURCE_POLICY_ADVISORY_SCHEMA_VERSION))
        .number(policy.fingerprint().bits())
        .number(numeric_objectives.len() as u64);
    for objective in &numeric_objectives {
        fingerprint = fingerprint.number(objective.fingerprint().bits());
    }
    fingerprint = fingerprint.number(planner_hints.len() as u64);
    for hint in &planner_hints {
        fingerprint = fingerprint.number(hint.fingerprint().bits());
    }

    Ok(EirResourcePolicyAdvisory {
        policy,
        numeric_objectives,
        planner_hints,
        fingerprint,
    })
}

/// Fail-closed advisory lowering errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyAdvisoryLoweringError {
    ResourcePolicy(ResourcePolicyLoweringError),
    ObjectiveRankOverflow { rank: usize },
}

impl fmt::Display for PolicyAdvisoryLoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourcePolicy(error) => error.fmt(f),
            Self::ObjectiveRankOverflow { rank } => {
                write!(f, "numeric objective rank {rank} exceeds u32 range")
            }
        }
    }
}

impl std::error::Error for PolicyAdvisoryLoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ResourcePolicy(error) => Some(error),
            Self::ObjectiveRankOverflow { .. } => None,
        }
    }
}

const fn objective_kind(objective: &ObjectiveId) -> &'static str {
    if objective.builtin_part().is_some() {
        "builtin"
    } else {
        "custom"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::{DimensionId, LogicalResourceId, ResourceClassId, ResourceSpec};
    use elastic_core::{
        PlannerHintKey, PolicyHeader, PolicyId, PolicyIdentity, PolicyMetricScale, PolicyTarget,
        PolicyVersion, ResourcePolicySpec,
    };

    fn base_policy() -> ResourcePolicySpec {
        let resource = ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("runtime").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .optimize(ObjectiveId::LATENCY)
        .optimize(ObjectiveId::THROUGHPUT)
        .build()
        .unwrap();
        ResourcePolicySpec::new(
            PolicyHeader::new(
                PolicyIdentity::new(
                    PolicyId::new("runtime.advisory").unwrap(),
                    PolicyVersion::new(1, 0, 0),
                ),
                PolicyTarget::resource(LogicalResourceId::new("runtime").unwrap()),
            ),
            resource,
            vec![],
            vec![],
        )
        .unwrap()
    }

    fn advisory(hint: &str) -> ResourcePolicyAdvisorySpec {
        ResourcePolicyAdvisorySpec::new(
            base_policy(),
            vec![
                PolicyNumericObjective::new(
                    ObjectiveId::THROUGHPUT,
                    PolicyObjectiveDirection::Maximize,
                    PolicyMetricScale::new("ops-per-second", 1).unwrap(),
                ),
                PolicyNumericObjective::new(
                    ObjectiveId::LATENCY,
                    PolicyObjectiveDirection::Minimize,
                    PolicyMetricScale::new("microseconds", 1).unwrap(),
                ),
            ],
            vec![PlannerHint::new(PlannerHintKey::new("search.mode").unwrap(), hint).unwrap()],
        )
        .unwrap()
    }

    #[test]
    fn objective_metadata_keeps_authoritative_resource_priority() {
        let lowered = lower_resource_policy_advisory(&advisory("balanced")).unwrap();
        assert_eq!(lowered.numeric_objectives()[0].rank(), 0);
        assert_eq!(
            lowered.numeric_objectives()[0].objective(),
            &ObjectiveId::LATENCY
        );
        assert_eq!(lowered.numeric_objectives()[1].rank(), 1);
        assert_eq!(
            lowered.numeric_objectives()[1].objective(),
            &ObjectiveId::THROUGHPUT
        );
    }

    #[test]
    fn changing_hint_changes_advisory_fingerprint_not_semantic_policy_fingerprint() {
        let balanced = lower_resource_policy_advisory(&advisory("balanced")).unwrap();
        let exhaustive = lower_resource_policy_advisory(&advisory("exhaustive")).unwrap();
        assert_eq!(
            balanced.policy().fingerprint(),
            exhaustive.policy().fingerprint()
        );
        assert_ne!(balanced.fingerprint(), exhaustive.fingerprint());
    }

    #[test]
    fn typed_builtin_and_custom_objectives_do_not_alias() {
        let custom = ObjectiveId::custom("latency").unwrap();
        let resource = ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("typed-objective").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .optimize(ObjectiveId::LATENCY)
        .optimize(custom.clone())
        .build()
        .unwrap();
        let policy = ResourcePolicySpec::new(
            PolicyHeader::new(
                PolicyIdentity::new(
                    PolicyId::new("typed.objective").unwrap(),
                    PolicyVersion::new(1, 0, 0),
                ),
                PolicyTarget::resource(LogicalResourceId::new("typed-objective").unwrap()),
            ),
            resource,
            vec![],
            vec![],
        )
        .unwrap();
        let lowered = lower_resource_policy_advisory(
            &ResourcePolicyAdvisorySpec::new(
                policy,
                vec![
                    PolicyNumericObjective::new(
                        ObjectiveId::LATENCY,
                        PolicyObjectiveDirection::Minimize,
                        PolicyMetricScale::new("us", 1).unwrap(),
                    ),
                    PolicyNumericObjective::new(
                        custom,
                        PolicyObjectiveDirection::Maximize,
                        PolicyMetricScale::new("score", 1).unwrap(),
                    ),
                ],
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(
            lowered.numeric_objectives()[0].fingerprint(),
            lowered.numeric_objectives()[1].fingerprint()
        );
    }
}
