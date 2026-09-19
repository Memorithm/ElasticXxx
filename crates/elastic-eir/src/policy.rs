//! ELANG5 typed policy identity lowering.
//!
//! This layer binds a non-actuating [`elastic_core::PolicyHeader`] to the exact
//! EIR definition of its resource/group target. It intentionally carries no
//! guard, constraint, planner, validation, or actuation semantics yet.

use crate::{EirDocument, EirGroupedDocument, Fingerprint};
use elastic_core::{PolicyHeader, PolicyIdentity, PolicyTarget};
use std::fmt;

/// Schema version of the EIR policy-header binding.
pub const EIR_POLICY_HEADER_SCHEMA_VERSION: u16 = 1;

/// Exact EIR binding of one policy identity/version to one typed target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirPolicyHeader {
    identity: PolicyIdentity,
    target: PolicyTarget,
    target_fingerprint: Fingerprint,
    fingerprint: Fingerprint,
}

impl EirPolicyHeader {
    /// Bind a resource-targeted policy header to the exact resource EIR node.
    pub fn lower_resource(
        header: &PolicyHeader,
        document: &EirDocument,
    ) -> Result<Self, PolicyLoweringError> {
        let PolicyTarget::Resource(resource) = header.target() else {
            return Err(PolicyLoweringError::TargetKindMismatch {
                expected: "resource",
                observed: header.target().kind().as_str(),
            });
        };
        let node = document.resource(resource.as_str()).ok_or_else(|| {
            PolicyLoweringError::UnknownResource {
                resource: resource.as_str().to_owned(),
            }
        })?;
        Ok(Self::from_bound_target(
            header,
            PolicyTarget::Resource(resource.clone()),
            node.fingerprint(),
        ))
    }

    /// Bind a group-targeted policy header to the exact ELANG3 group EIR node.
    pub fn lower_group(
        header: &PolicyHeader,
        document: &EirGroupedDocument,
    ) -> Result<Self, PolicyLoweringError> {
        let PolicyTarget::Group(group) = header.target() else {
            return Err(PolicyLoweringError::TargetKindMismatch {
                expected: "group",
                observed: header.target().kind().as_str(),
            });
        };
        let node =
            document
                .group(group.as_str())
                .ok_or_else(|| PolicyLoweringError::UnknownGroup {
                    group: group.as_str().to_owned(),
                })?;
        Ok(Self::from_bound_target(
            header,
            PolicyTarget::Group(group.clone()),
            node.fingerprint(),
        ))
    }

    pub(crate) fn from_bound_target(
        header: &PolicyHeader,
        target: PolicyTarget,
        target_fingerprint: Fingerprint,
    ) -> Self {
        let identity = header.identity().clone();
        let version = identity.version();
        let fingerprint = Fingerprint::EMPTY
            .text("eir-policy-header")
            .number(u64::from(EIR_POLICY_HEADER_SCHEMA_VERSION))
            .text(identity.id().as_str())
            .number(u64::from(version.major()))
            .number(u64::from(version.minor()))
            .number(u64::from(version.patch()))
            .text(target.kind().as_str())
            .text(target.as_str())
            .number(target_fingerprint.bits());
        Self {
            identity,
            target,
            target_fingerprint,
            fingerprint,
        }
    }

    #[must_use]
    pub const fn identity(&self) -> &PolicyIdentity {
        &self.identity
    }

    #[must_use]
    pub const fn target(&self) -> &PolicyTarget {
        &self.target
    }

    /// Exact structural identity of the bound EIR resource/group definition.
    #[must_use]
    pub const fn target_fingerprint(&self) -> Fingerprint {
        self.target_fingerprint
    }

    /// Structural identity of policy lineage/version, target kind/text and
    /// bound target definition.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Fail-closed errors while binding an ELANG5 policy header to EIR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyLoweringError {
    TargetKindMismatch {
        expected: &'static str,
        observed: &'static str,
    },
    UnknownResource {
        resource: String,
    },
    UnknownGroup {
        group: String,
    },
}

impl fmt::Display for PolicyLoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetKindMismatch { expected, observed } => write!(
                f,
                "policy target kind mismatch: expected {expected}, observed {observed}"
            ),
            Self::UnknownResource { resource } => {
                write!(f, "policy target resource {resource} is absent from EIR")
            }
            Self::UnknownGroup { group } => {
                write!(f, "policy target group {group} is absent from grouped EIR")
            }
        }
    }
}

impl std::error::Error for PolicyLoweringError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EirDocumentBuilder;
    use elastic_core::resource::{
        DimensionId, LogicalResourceId, ResourceClassId, ResourceGroupBuilder, ResourceGroupId,
        ResourceSpec,
    };
    use elastic_core::{PolicyId, PolicyVersion};

    fn spec(id: &str, dimension: DimensionId) -> ResourceSpec {
        ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new(id).unwrap(),
        )
        .allow(dimension)
        .build()
        .unwrap()
    }

    fn policy(target: PolicyTarget, version: PolicyVersion) -> PolicyHeader {
        PolicyHeader::new(
            PolicyIdentity::new(PolicyId::new("runtime.balance").unwrap(), version),
            target,
        )
    }

    #[test]
    fn resource_policy_binds_exact_target_definition_and_version() {
        let document = crate::lower(&spec("ram", DimensionId::CAPACITY)).unwrap();
        let v1 = EirPolicyHeader::lower_resource(
            &policy(
                PolicyTarget::resource(LogicalResourceId::new("ram").unwrap()),
                PolicyVersion::new(1, 0, 0),
            ),
            &document,
        )
        .unwrap();
        let v2 = EirPolicyHeader::lower_resource(
            &policy(
                PolicyTarget::resource(LogicalResourceId::new("ram").unwrap()),
                PolicyVersion::new(1, 0, 1),
            ),
            &document,
        )
        .unwrap();
        assert_eq!(
            v1.target_fingerprint(),
            document.resource("ram").unwrap().fingerprint()
        );
        assert_ne!(v1.fingerprint(), v2.fingerprint());
    }

    #[test]
    fn resource_definition_drift_changes_policy_binding_fingerprint() {
        let capacity = crate::lower(&spec("ram", DimensionId::CAPACITY)).unwrap();
        let energy = crate::lower(&spec("ram", DimensionId::ENERGY)).unwrap();
        let header = policy(
            PolicyTarget::resource(LogicalResourceId::new("ram").unwrap()),
            PolicyVersion::new(1, 0, 0),
        );
        let left = EirPolicyHeader::lower_resource(&header, &capacity).unwrap();
        let right = EirPolicyHeader::lower_resource(&header, &energy).unwrap();
        assert_ne!(left.target_fingerprint(), right.target_fingerprint());
        assert_ne!(left.fingerprint(), right.fingerprint());
    }

    #[test]
    fn group_policy_binds_exact_group_topology() {
        let mut builder = EirDocumentBuilder::new();
        for id in ["flight", "vision"] {
            builder.push(&spec(id, DimensionId::CAPACITY)).unwrap();
        }
        let document = builder.finish().unwrap();
        let group = ResourceGroupBuilder::new(ResourceGroupId::new("drone").unwrap())
            .members([
                LogicalResourceId::new("flight").unwrap(),
                LogicalResourceId::new("vision").unwrap(),
            ])
            .build()
            .unwrap();
        let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
        let lowered = EirPolicyHeader::lower_group(
            &policy(
                PolicyTarget::group(ResourceGroupId::new("drone").unwrap()),
                PolicyVersion::new(0, 1, 0),
            ),
            &grouped,
        )
        .unwrap();
        assert_eq!(
            lowered.target_fingerprint(),
            grouped.group("drone").unwrap().fingerprint()
        );
    }

    #[test]
    fn target_kind_and_unknown_targets_fail_closed() {
        let document = crate::lower(&spec("ram", DimensionId::CAPACITY)).unwrap();
        let wrong_kind = policy(
            PolicyTarget::group(ResourceGroupId::new("ram").unwrap()),
            PolicyVersion::new(1, 0, 0),
        );
        assert!(matches!(
            EirPolicyHeader::lower_resource(&wrong_kind, &document),
            Err(PolicyLoweringError::TargetKindMismatch { .. })
        ));
        let missing = policy(
            PolicyTarget::resource(LogicalResourceId::new("missing").unwrap()),
            PolicyVersion::new(1, 0, 0),
        );
        assert!(matches!(
            EirPolicyHeader::lower_resource(&missing, &document),
            Err(PolicyLoweringError::UnknownResource { .. })
        ));
    }
}
