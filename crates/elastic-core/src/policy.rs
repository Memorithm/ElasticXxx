//! Typed identity and target binding for Elastic language policies.
//!
//! This module intentionally contains no guard evaluation, planner selection,
//! transition creation, validation, or actuation semantics. It gives future
//! ELANG5 policy blocks a stable, versioned identity and an unambiguous target
//! while existing Boolean/pseudo-Boolean/resource contracts remain the semantic
//! authorities.

use crate::resource::{LogicalResourceId, ResourceGroupId, ResourceSpec};
use crate::{
    BooleanGuard, GuardBindingError, GuardedResourceSpec, PseudoBooleanConstraintDeclaration,
};
use std::fmt;

/// Maximum canonical UTF-8 byte length of one policy identity.
///
/// Policy IDs are restricted to canonical lowercase ASCII, so byte length is
/// also character length and no Unicode-normalization ambiguity is possible.
pub const MAX_POLICY_ID_BYTES: usize = 128;
/// Maximum pseudo-Boolean constraints attached to one resource policy revision.
pub const MAX_RESOURCE_POLICY_CONSTRAINTS: usize = 64;

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
