//! Typed identity and target binding for Elastic language policies.
//!
//! This module intentionally contains no guard evaluation, planner selection,
//! transition creation, validation, or actuation semantics. It gives future
//! ELANG5 policy blocks a stable, versioned identity and an unambiguous target
//! while existing Boolean/pseudo-Boolean/resource contracts remain the semantic
//! authorities.

use crate::resource::{LogicalResourceId, ObjectiveId, ResourceGroupId, ResourceSpec};
use crate::{
    BooleanGuard, GuardBindingError, GuardedResourceSpec, PseudoBooleanConstraintDeclaration,
};
use std::fmt;
use std::num::NonZeroU64;

/// Maximum canonical UTF-8 byte length of one policy identity.
///
/// Policy IDs are restricted to canonical lowercase ASCII, so byte length is
/// also character length and no Unicode-normalization ambiguity is possible.
pub const MAX_POLICY_ID_BYTES: usize = 128;
/// Maximum pseudo-Boolean constraints attached to one resource policy revision.
pub const MAX_RESOURCE_POLICY_CONSTRAINTS: usize = 64;
/// Maximum byte length of one numeric objective unit label.
pub const MAX_POLICY_METRIC_UNIT_BYTES: usize = 64;
/// Maximum canonical planner-hint key length.
pub const MAX_PLANNER_HINT_KEY_BYTES: usize = 64;
/// Maximum byte length of one advisory planner-hint value.
pub const MAX_PLANNER_HINT_VALUE_BYTES: usize = 256;
/// Maximum advisory planner hints on one policy revision.
pub const MAX_PLANNER_HINTS: usize = 64;

/// Stable, canonical identity of one policy lineage.
///
/// Accepted bytes are lowercase ASCII letters, decimal digits, `.`, `_`, and
/// `-`. Version is deliberately a separate typed field, so changing a policy
/// revision never changes its lineage identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyId(String);

impl PolicyId {
    /// Construct a canonical policy ID.
    pub fn new(value: impl Into<String>) -> Result<Self, PolicyIdentityError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PolicyIdentityError::EmptyPolicyId);
        }
        if value.len() > MAX_POLICY_ID_BYTES {
            return Err(PolicyIdentityError::PolicyIdTooLong {
                bytes: value.len(),
                maximum: MAX_POLICY_ID_BYTES,
            });
        }
        if !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        }) {
            return Err(PolicyIdentityError::InvalidPolicyIdCharacter);
        }
        Ok(Self(value))
    }

    /// Canonical lineage identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PolicyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Semantic policy version carried independently from policy lineage identity.
///
/// This is deliberately a small dependency-free semver-shaped value. It does
/// not itself define compatibility rules; ELANG productization may later state
/// such rules explicitly. Ordering is lexicographic `(major, minor, patch)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl PolicyVersion {
    /// Construct one explicit policy version.
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    #[must_use]
    pub const fn major(self) -> u32 {
        self.major
    }

    #[must_use]
    pub const fn minor(self) -> u32 {
        self.minor
    }

    #[must_use]
    pub const fn patch(self) -> u32 {
        self.patch
    }
}

impl fmt::Display for PolicyVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Stable identity of one exact policy revision.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyIdentity {
    id: PolicyId,
    version: PolicyVersion,
}

impl PolicyIdentity {
    /// Bind a lineage ID to one exact semantic version.
    #[must_use]
    pub const fn new(id: PolicyId, version: PolicyVersion) -> Self {
        Self { id, version }
    }

    #[must_use]
    pub const fn id(&self) -> &PolicyId {
        &self.id
    }

    #[must_use]
    pub const fn version(&self) -> PolicyVersion {
        self.version
    }
}

impl fmt::Display for PolicyIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.id, self.version)
    }
}

/// Typed target to which one policy revision is attached.
///
/// Resource and group targets are distinct variants even if their canonical
/// text happens to be identical, preventing string-level aliasing.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolicyTarget {
    Resource(LogicalResourceId),
    Group(ResourceGroupId),
}

impl PolicyTarget {
    #[must_use]
    pub const fn resource(resource: LogicalResourceId) -> Self {
        Self::Resource(resource)
    }

    #[must_use]
    pub const fn group(group: ResourceGroupId) -> Self {
        Self::Group(group)
    }

    /// Canonical target text without discarding target kind.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Resource(resource) => resource.as_str(),
            Self::Group(group) => group.as_str(),
        }
    }

    /// Stable target-kind discriminator for fingerprints/wire formats.
    #[must_use]
    pub const fn kind(&self) -> PolicyTargetKind {
        match self {
            Self::Resource(_) => PolicyTargetKind::Resource,
            Self::Group(_) => PolicyTargetKind::Group,
        }
    }
}

/// Stable discriminator for [`PolicyTarget`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolicyTargetKind {
    Resource,
    Group,
}

impl PolicyTargetKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resource => "resource",
            Self::Group => "group",
        }
    }
}

impl fmt::Display for PolicyTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind().as_str(), self.as_str())
    }
}

/// Non-actuating header shared by all future ELANG5 policy forms.
///
/// The header proves only identity and attachment. It cannot introduce a
/// transition, authorize planning, or bypass existing resource/guard/constraint
/// validation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyHeader {
    identity: PolicyIdentity,
    target: PolicyTarget,
}

impl PolicyHeader {
    #[must_use]
    pub const fn new(identity: PolicyIdentity, target: PolicyTarget) -> Self {
        Self { identity, target }
    }

    #[must_use]
    pub const fn identity(&self) -> &PolicyIdentity {
        &self.identity
    }

    #[must_use]
    pub const fn target(&self) -> &PolicyTarget {
        &self.target
    }
}

/// Validated ELANG5 policy rules for one exact logical resource.
///
/// This type deliberately reuses the existing guard and pseudo-Boolean
/// authorities. It does not evaluate facts, plan a transition or authorize
/// actuation. Group-targeted policies require separate group semantics and are
/// rejected here rather than implicitly applying a resource guard to a group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcePolicySpec {
    header: PolicyHeader,
    guarded_resource: GuardedResourceSpec,
    constraints: Vec<PseudoBooleanConstraintDeclaration>,
}

impl ResourcePolicySpec {
    /// Bind one policy revision to an exact validated resource declaration.
    pub fn new(
        header: PolicyHeader,
        resource: ResourceSpec,
        guards: Vec<BooleanGuard>,
        constraints: Vec<PseudoBooleanConstraintDeclaration>,
    ) -> Result<Self, ResourcePolicyError> {
        let PolicyTarget::Resource(target) = header.target() else {
            return Err(ResourcePolicyError::TargetKindMismatch {
                observed: header.target().kind(),
            });
        };
        if target != resource.resource_id() {
            return Err(ResourcePolicyError::TargetResourceMismatch {
                target: target.clone(),
                resource: resource.resource_id().clone(),
            });
        }
        if constraints.len() > MAX_RESOURCE_POLICY_CONSTRAINTS {
            return Err(ResourcePolicyError::TooManyConstraints {
                constraints: constraints.len(),
                maximum: MAX_RESOURCE_POLICY_CONSTRAINTS,
            });
        }
        let guarded_resource = GuardedResourceSpec::new(resource, guards)
            .map_err(ResourcePolicyError::GuardBinding)?;
        Ok(Self {
            header,
            guarded_resource,
            constraints,
        })
    }

    #[must_use]
    pub const fn header(&self) -> &PolicyHeader {
        &self.header
    }

    #[must_use]
    pub const fn guarded_resource(&self) -> &GuardedResourceSpec {
        &self.guarded_resource
    }

    #[must_use]
    pub fn constraints(&self) -> &[PseudoBooleanConstraintDeclaration] {
        &self.constraints
    }
}

/// Fail-closed policy-to-resource binding errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourcePolicyError {
    TargetKindMismatch {
        observed: PolicyTargetKind,
    },
    TargetResourceMismatch {
        target: LogicalResourceId,
        resource: LogicalResourceId,
    },
    TooManyConstraints {
        constraints: usize,
        maximum: usize,
    },
    GuardBinding(GuardBindingError),
}

impl fmt::Display for ResourcePolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetKindMismatch { observed } => write!(
                f,
                "resource policy requires resource target, observed {}",
                observed.as_str()
            ),
            Self::TargetResourceMismatch { target, resource } => write!(
                f,
                "policy targets resource {target}, but rules were bound to {resource}"
            ),
            Self::TooManyConstraints {
                constraints,
                maximum,
            } => write!(
                f,
                "resource policy contains {constraints} constraints; maximum is {maximum}"
            ),
            Self::GuardBinding(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ResourcePolicyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::GuardBinding(error) => Some(error),
            Self::TargetKindMismatch { .. }
            | Self::TargetResourceMismatch { .. }
            | Self::TooManyConstraints { .. } => None,
        }
    }
}

/// Explicit integer scale for one numeric objective measurement.
///
/// The scale is descriptive only. `quantum` states how many base units one
/// integer tick represents for a planner that explicitly understands the
/// objective. It does not define a cross-objective scalar score.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyMetricScale {
    unit: String,
    quantum: NonZeroU64,
}

impl PolicyMetricScale {
    pub fn new(unit: impl Into<String>, quantum: u64) -> Result<Self, PolicyAdvisoryError> {
        let unit = unit.into();
        if unit.is_empty() {
            return Err(PolicyAdvisoryError::EmptyMetricUnit);
        }
        if unit.trim() != unit {
            return Err(PolicyAdvisoryError::MetricUnitNotTrimmed);
        }
        if unit.len() > MAX_POLICY_METRIC_UNIT_BYTES {
            return Err(PolicyAdvisoryError::MetricUnitTooLong {
                bytes: unit.len(),
                maximum: MAX_POLICY_METRIC_UNIT_BYTES,
            });
        }
        let quantum = NonZeroU64::new(quantum).ok_or(PolicyAdvisoryError::ZeroMetricQuantum)?;
        Ok(Self { unit, quantum })
    }

    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    #[must_use]
    pub const fn quantum(&self) -> u64 {
        self.quantum.get()
    }
}

/// Desired numeric direction for one already-declared resource objective.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolicyObjectiveDirection {
    Minimize,
    Maximize,
}

impl PolicyObjectiveDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Maximize => "maximize",
        }
    }
}

/// Numeric metadata for one objective already present in `ResourceSpec`.
///
/// This does not change objective priority: `ResourceSpec::objectives()` remains
/// the sole ordering. It only states how the selected objective is numerically
/// interpreted by a planner that explicitly supports this metadata.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyNumericObjective {
    objective: ObjectiveId,
    direction: PolicyObjectiveDirection,
    scale: PolicyMetricScale,
}

impl PolicyNumericObjective {
    #[must_use]
    pub const fn new(
        objective: ObjectiveId,
        direction: PolicyObjectiveDirection,
        scale: PolicyMetricScale,
    ) -> Self {
        Self {
            objective,
            direction,
            scale,
        }
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
    pub const fn scale(&self) -> &PolicyMetricScale {
        &self.scale
    }
}

/// Canonical key for advisory planner metadata.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlannerHintKey(String);

impl PlannerHintKey {
    pub fn new(value: impl Into<String>) -> Result<Self, PolicyAdvisoryError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PolicyAdvisoryError::EmptyPlannerHintKey);
        }
        if value.len() > MAX_PLANNER_HINT_KEY_BYTES {
            return Err(PolicyAdvisoryError::PlannerHintKeyTooLong {
                bytes: value.len(),
                maximum: MAX_PLANNER_HINT_KEY_BYTES,
            });
        }
        if !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        }) {
            return Err(PolicyAdvisoryError::InvalidPlannerHintKeyCharacter);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One bounded advisory planner hint.
///
/// Hints are metadata only. A planner may explicitly opt in to a known key, but
/// absence, presence or value of a hint cannot admit a transition, satisfy a
/// guard, change a pseudo-Boolean truth value or bypass trusted validation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlannerHint {
    key: PlannerHintKey,
    value: String,
}

impl PlannerHint {
    pub fn new(key: PlannerHintKey, value: impl Into<String>) -> Result<Self, PolicyAdvisoryError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PolicyAdvisoryError::EmptyPlannerHintValue { key });
        }
        if value.trim() != value {
            return Err(PolicyAdvisoryError::PlannerHintValueNotTrimmed { key });
        }
        if value.len() > MAX_PLANNER_HINT_VALUE_BYTES {
            return Err(PolicyAdvisoryError::PlannerHintValueTooLong {
                key,
                bytes: value.len(),
                maximum: MAX_PLANNER_HINT_VALUE_BYTES,
            });
        }
        Ok(Self { key, value })
    }

    #[must_use]
    pub const fn key(&self) -> &PlannerHintKey {
        &self.key
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// ELANG5 advisory metadata attached to one typed resource policy.
///
/// Numeric objectives are normalized to the priority order already declared by
/// the resource. Planner hints are sorted by canonical key. Neither collection
/// has independent planning authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcePolicyAdvisorySpec {
    policy: ResourcePolicySpec,
    numeric_objectives: Vec<PolicyNumericObjective>,
    planner_hints: Vec<PlannerHint>,
}

impl ResourcePolicyAdvisorySpec {
    pub fn new(
        policy: ResourcePolicySpec,
        mut numeric_objectives: Vec<PolicyNumericObjective>,
        mut planner_hints: Vec<PlannerHint>,
    ) -> Result<Self, PolicyAdvisoryError> {
        let declared = policy.guarded_resource().resource().objectives();
        let mut ranks = std::collections::BTreeMap::new();
        for (rank, objective) in declared.iter().enumerate() {
            ranks.insert(objective.clone(), rank);
        }
        for objective in &numeric_objectives {
            if !ranks.contains_key(objective.objective()) {
                return Err(PolicyAdvisoryError::UndeclaredNumericObjective {
                    objective: objective.objective().clone(),
                });
            }
        }
        numeric_objectives.sort_by_key(|entry| ranks[entry.objective()]);
        for pair in numeric_objectives.windows(2) {
            if pair[0].objective() == pair[1].objective() {
                return Err(PolicyAdvisoryError::DuplicateNumericObjective {
                    objective: pair[0].objective().clone(),
                });
            }
        }

        if planner_hints.len() > MAX_PLANNER_HINTS {
            return Err(PolicyAdvisoryError::TooManyPlannerHints {
                hints: planner_hints.len(),
                maximum: MAX_PLANNER_HINTS,
            });
        }
        planner_hints.sort_by(|left, right| left.key.cmp(&right.key));
        for pair in planner_hints.windows(2) {
            if pair[0].key == pair[1].key {
                return Err(PolicyAdvisoryError::DuplicatePlannerHint {
                    key: pair[0].key.clone(),
                });
            }
        }

        Ok(Self {
            policy,
            numeric_objectives,
            planner_hints,
        })
    }

    #[must_use]
    pub const fn policy(&self) -> &ResourcePolicySpec {
        &self.policy
    }

    /// Numeric metadata in the authoritative `ResourceSpec` objective priority order.
    #[must_use]
    pub fn numeric_objectives(&self) -> &[PolicyNumericObjective] {
        &self.numeric_objectives
    }

    /// Canonically sorted advisory hints.
    #[must_use]
    pub fn planner_hints(&self) -> &[PlannerHint] {
        &self.planner_hints
    }
}

/// Validation failures for ELANG5 numeric/advisory metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyAdvisoryError {
    EmptyMetricUnit,
    MetricUnitNotTrimmed,
    MetricUnitTooLong {
        bytes: usize,
        maximum: usize,
    },
    ZeroMetricQuantum,
    UndeclaredNumericObjective {
        objective: ObjectiveId,
    },
    DuplicateNumericObjective {
        objective: ObjectiveId,
    },
    EmptyPlannerHintKey,
    PlannerHintKeyTooLong {
        bytes: usize,
        maximum: usize,
    },
    InvalidPlannerHintKeyCharacter,
    EmptyPlannerHintValue {
        key: PlannerHintKey,
    },
    PlannerHintValueNotTrimmed {
        key: PlannerHintKey,
    },
    PlannerHintValueTooLong {
        key: PlannerHintKey,
        bytes: usize,
        maximum: usize,
    },
    DuplicatePlannerHint {
        key: PlannerHintKey,
    },
    TooManyPlannerHints {
        hints: usize,
        maximum: usize,
    },
}

impl fmt::Display for PolicyAdvisoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMetricUnit => write!(f, "numeric objective unit must not be empty"),
            Self::MetricUnitNotTrimmed => write!(f, "numeric objective unit must be trimmed"),
            Self::MetricUnitTooLong { bytes, maximum } => write!(
                f,
                "numeric objective unit length {bytes} exceeds maximum {maximum}"
            ),
            Self::ZeroMetricQuantum => write!(f, "numeric objective quantum must be non-zero"),
            Self::UndeclaredNumericObjective { objective } => write!(
                f,
                "numeric policy metadata references undeclared resource objective {objective}"
            ),
            Self::DuplicateNumericObjective { objective } => {
                write!(f, "numeric policy metadata repeats objective {objective}")
            }
            Self::EmptyPlannerHintKey => write!(f, "planner hint key must not be empty"),
            Self::PlannerHintKeyTooLong { bytes, maximum } => write!(
                f,
                "planner hint key length {bytes} exceeds maximum {maximum}"
            ),
            Self::InvalidPlannerHintKeyCharacter => write!(
                f,
                "planner hint key must use only lowercase ASCII letters, digits, '.', '_' or '-'"
            ),
            Self::EmptyPlannerHintValue { key } => {
                write!(f, "planner hint {} has empty value", key.as_str())
            }
            Self::PlannerHintValueNotTrimmed { key } => {
                write!(f, "planner hint {} value must be trimmed", key.as_str())
            }
            Self::PlannerHintValueTooLong {
                key,
                bytes,
                maximum,
            } => write!(
                f,
                "planner hint {} value length {bytes} exceeds maximum {maximum}",
                key.as_str()
            ),
            Self::DuplicatePlannerHint { key } => write!(
                f,
                "planner hint {} is declared more than once",
                key.as_str()
            ),
            Self::TooManyPlannerHints { hints, maximum } => write!(
                f,
                "resource policy contains {hints} planner hints; maximum is {maximum}"
            ),
        }
    }
}
impl std::error::Error for PolicyAdvisoryError {}

/// Canonical policy identity construction errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyIdentityError {
    EmptyPolicyId,
    PolicyIdTooLong { bytes: usize, maximum: usize },
    InvalidPolicyIdCharacter,
}

impl fmt::Display for PolicyIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPolicyId => write!(f, "policy identity must not be empty"),
            Self::PolicyIdTooLong { bytes, maximum } => write!(
                f,
                "policy identity length {bytes} exceeds maximum {maximum} bytes"
            ),
            Self::InvalidPolicyIdCharacter => write!(
                f,
                "policy identity must use only lowercase ASCII letters, digits, '.', '_' or '-'"
            ),
        }
    }
}

impl std::error::Error for PolicyIdentityError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_policy_identity_keeps_lineage_and_version_separate() {
        let id = PolicyId::new("embedded.flight-control").unwrap();
        let v1 = PolicyIdentity::new(id.clone(), PolicyVersion::new(0, 1, 0));
        let v2 = PolicyIdentity::new(id, PolicyVersion::new(0, 2, 0));
        assert_eq!(v1.id(), v2.id());
        assert_ne!(v1, v2);
        assert_eq!(v1.to_string(), "embedded.flight-control@0.1.0");
    }

    #[test]
    fn policy_id_rejects_noncanonical_or_ambiguous_text() {
        for invalid in [
            "",
            " flight",
            "flight ",
            "Flight",
            "flight/control",
            "énergie",
        ] {
            assert!(
                PolicyId::new(invalid).is_err(),
                "accepted invalid policy ID {invalid:?}"
            );
        }
        assert!(matches!(
            PolicyId::new("x".repeat(MAX_POLICY_ID_BYTES + 1)),
            Err(PolicyIdentityError::PolicyIdTooLong { .. })
        ));
    }

    #[test]
    fn resource_and_group_targets_never_alias_on_same_text() {
        let resource = PolicyTarget::resource(LogicalResourceId::new("runtime").unwrap());
        let group = PolicyTarget::group(ResourceGroupId::new("runtime").unwrap());
        assert_eq!(resource.as_str(), group.as_str());
        assert_ne!(resource, group);
        assert_eq!(resource.kind(), PolicyTargetKind::Resource);
        assert_eq!(group.kind(), PolicyTargetKind::Group);
        assert_eq!(resource.to_string(), "resource:runtime");
        assert_eq!(group.to_string(), "group:runtime");
    }

    fn policy_header_for(resource: &str) -> PolicyHeader {
        PolicyHeader::new(
            PolicyIdentity::new(
                PolicyId::new("runtime.policy").unwrap(),
                PolicyVersion::new(1, 0, 0),
            ),
            PolicyTarget::resource(LogicalResourceId::new(resource).unwrap()),
        )
    }

    fn resource(id: &str) -> ResourceSpec {
        ResourceSpec::builder(
            crate::resource::ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new(id).unwrap(),
        )
        .allow(crate::resource::DimensionId::CAPACITY)
        .admit(crate::resource::AdmissibleTransition::new(
            crate::TransitionMechanism::Reinterpret,
            crate::resource::DimensionId::CAPACITY,
        ))
        .build()
        .unwrap()
    }

    #[test]
    fn resource_policy_reuses_guard_binding_authority() {
        let key = crate::PredicateKey::new("elastic.policy", "capacity-ok").unwrap();
        let registry = crate::PredicateRegistry::from_keys([key.clone()]).unwrap();
        let predicate = registry.id(&key).unwrap();
        let guard = BooleanGuard::requires(
            crate::GuardScope::Transition {
                mechanism: crate::TransitionMechanism::Reinterpret,
                dimension: crate::resource::DimensionId::CAPACITY,
            },
            registry,
            predicate,
        )
        .unwrap();
        let policy = ResourcePolicySpec::new(
            policy_header_for("ram"),
            resource("ram"),
            vec![guard],
            Vec::new(),
        )
        .unwrap();
        assert_eq!(policy.guarded_resource().guards().len(), 1);
        assert_eq!(policy.header().target().as_str(), "ram");
    }

    #[test]
    fn group_or_wrong_resource_target_fails_closed() {
        let group_header = PolicyHeader::new(
            PolicyIdentity::new(
                PolicyId::new("runtime.policy").unwrap(),
                PolicyVersion::new(1, 0, 0),
            ),
            PolicyTarget::group(ResourceGroupId::new("runtime").unwrap()),
        );
        assert!(matches!(
            ResourcePolicySpec::new(group_header, resource("ram"), vec![], vec![]),
            Err(ResourcePolicyError::TargetKindMismatch { .. })
        ));
        assert!(matches!(
            ResourcePolicySpec::new(policy_header_for("other"), resource("ram"), vec![], vec![]),
            Err(ResourcePolicyError::TargetResourceMismatch { .. })
        ));
    }

    #[test]
    fn resource_policy_constraint_count_is_bounded() {
        let key = crate::PredicateKey::new("elastic.policy", "mode").unwrap();
        let declaration =
            crate::PseudoBooleanConstraintDeclaration::at_most_keys([key], 1).unwrap();
        assert!(matches!(
            ResourcePolicySpec::new(
                policy_header_for("ram"),
                resource("ram"),
                vec![],
                vec![declaration; MAX_RESOURCE_POLICY_CONSTRAINTS + 1]
            ),
            Err(ResourcePolicyError::TooManyConstraints { .. })
        ));
    }

    #[test]
    fn advisory_objectives_follow_resource_priority_not_input_order() {
        let resource = ResourceSpec::builder(
            crate::resource::ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("ram").unwrap(),
        )
        .allow(crate::resource::DimensionId::CAPACITY)
        .optimize(ObjectiveId::LATENCY)
        .optimize(ObjectiveId::MEMORY_FOOTPRINT)
        .build()
        .unwrap();
        let policy =
            ResourcePolicySpec::new(policy_header_for("ram"), resource, vec![], vec![]).unwrap();
        let advisory = ResourcePolicyAdvisorySpec::new(
            policy,
            vec![
                PolicyNumericObjective::new(
                    ObjectiveId::MEMORY_FOOTPRINT,
                    PolicyObjectiveDirection::Minimize,
                    PolicyMetricScale::new("bytes", 1).unwrap(),
                ),
                PolicyNumericObjective::new(
                    ObjectiveId::LATENCY,
                    PolicyObjectiveDirection::Minimize,
                    PolicyMetricScale::new("microseconds", 1).unwrap(),
                ),
            ],
            vec![],
        )
        .unwrap();
        assert_eq!(
            advisory.numeric_objectives()[0].objective(),
            &ObjectiveId::LATENCY
        );
        assert_eq!(
            advisory.numeric_objectives()[1].objective(),
            &ObjectiveId::MEMORY_FOOTPRINT
        );
    }

    #[test]
    fn undeclared_or_duplicate_numeric_objectives_fail_closed() {
        let resource = ResourceSpec::builder(
            crate::resource::ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("ram").unwrap(),
        )
        .allow(crate::resource::DimensionId::CAPACITY)
        .optimize(ObjectiveId::LATENCY)
        .build()
        .unwrap();
        let policy =
            ResourcePolicySpec::new(policy_header_for("ram"), resource, vec![], vec![]).unwrap();
        let throughput = PolicyNumericObjective::new(
            ObjectiveId::THROUGHPUT,
            PolicyObjectiveDirection::Maximize,
            PolicyMetricScale::new("ops-per-second", 1).unwrap(),
        );
        assert!(matches!(
            ResourcePolicyAdvisorySpec::new(policy.clone(), vec![throughput], vec![]),
            Err(PolicyAdvisoryError::UndeclaredNumericObjective { .. })
        ));
        let latency = PolicyNumericObjective::new(
            ObjectiveId::LATENCY,
            PolicyObjectiveDirection::Minimize,
            PolicyMetricScale::new("microseconds", 1).unwrap(),
        );
        assert!(matches!(
            ResourcePolicyAdvisorySpec::new(policy, vec![latency.clone(), latency], vec![]),
            Err(PolicyAdvisoryError::DuplicateNumericObjective { .. })
        ));
    }

    #[test]
    fn planner_hints_are_canonical_bounded_metadata() {
        let resource = ResourceSpec::builder(
            crate::resource::ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("ram").unwrap(),
        )
        .allow(crate::resource::DimensionId::CAPACITY)
        .build()
        .unwrap();
        let policy =
            ResourcePolicySpec::new(policy_header_for("ram"), resource, vec![], vec![]).unwrap();
        let slow =
            PlannerHint::new(PlannerHintKey::new("search.mode").unwrap(), "balanced").unwrap();
        let fast = PlannerHint::new(PlannerHintKey::new("candidate.limit").unwrap(), "16").unwrap();
        let advisory =
            ResourcePolicyAdvisorySpec::new(policy.clone(), vec![], vec![slow, fast]).unwrap();
        assert_eq!(
            advisory.planner_hints()[0].key().as_str(),
            "candidate.limit"
        );
        assert_eq!(advisory.planner_hints()[1].key().as_str(), "search.mode");
        let duplicate =
            PlannerHint::new(PlannerHintKey::new("search.mode").unwrap(), "fast").unwrap();
        let duplicate2 =
            PlannerHint::new(PlannerHintKey::new("search.mode").unwrap(), "slow").unwrap();
        assert!(matches!(
            ResourcePolicyAdvisorySpec::new(policy, vec![], vec![duplicate, duplicate2]),
            Err(PolicyAdvisoryError::DuplicatePlannerHint { .. })
        ));
    }

    #[test]
    fn policy_header_carries_no_hidden_semantics() {
        let header = PolicyHeader::new(
            PolicyIdentity::new(
                PolicyId::new("runtime.balance").unwrap(),
                PolicyVersion::new(1, 0, 0),
            ),
            PolicyTarget::resource(LogicalResourceId::new("ram").unwrap()),
        );
        assert_eq!(header.identity().id().as_str(), "runtime.balance");
        assert_eq!(header.identity().version(), PolicyVersion::new(1, 0, 0));
        assert_eq!(header.target().as_str(), "ram");
    }
}
