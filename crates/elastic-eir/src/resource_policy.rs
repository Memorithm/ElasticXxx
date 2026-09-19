//! ELANG5 resource-policy EIR envelope.
//!
//! This envelope composes existing authorities rather than introducing a new
//! evaluator: [`elastic_core::ResourcePolicySpec`] owns typed policy/resource
//! binding, [`crate::lower_constrained`] owns guard/constraint lowering, and
//! [`crate::EirPolicyHeader`] owns exact target identity/version binding.

use crate::{
    lower_constrained, ConstraintLoweringError, EirConstrainedResource, EirPolicyHeader,
    Fingerprint,
};
use elastic_core::{PolicyTarget, ResourcePolicySpec};
use std::fmt;

/// Schema version of the combined ELANG5 resource-policy EIR envelope.
pub const EIR_RESOURCE_POLICY_SCHEMA_VERSION: u16 = 1;

/// One canonical resource policy lowered through existing guard/constraint EIR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirResourcePolicy {
    header: EirPolicyHeader,
    constrained: EirConstrainedResource,
    fingerprint: Fingerprint,
}

impl EirResourcePolicy {
    #[must_use]
    pub const fn header(&self) -> &EirPolicyHeader {
        &self.header
    }

    #[must_use]
    pub const fn constrained_resource(&self) -> &EirConstrainedResource {
        &self.constrained
    }

    /// Structural identity of exact policy revision, exact target definition,
    /// guards and pseudo-Boolean constraints.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Lower one typed resource policy through existing guarded/constrained EIR.
pub fn lower_resource_policy(
    policy: &ResourcePolicySpec,
) -> Result<EirResourcePolicy, ResourcePolicyLoweringError> {
    let constrained = lower_constrained(policy.guarded_resource(), policy.constraints())
        .map_err(ResourcePolicyLoweringError::Constrained)?;
    let target = PolicyTarget::Resource(policy.guarded_resource().resource().resource_id().clone());
    let target_fingerprint = constrained.guarded_resource().resource().fingerprint();
    let header = EirPolicyHeader::from_bound_target(policy.header(), target, target_fingerprint);
    let fingerprint = Fingerprint::EMPTY
        .text("eir-resource-policy")
        .number(u64::from(EIR_RESOURCE_POLICY_SCHEMA_VERSION))
        .number(header.fingerprint().bits())
        .number(constrained.fingerprint().bits());
    Ok(EirResourcePolicy {
        header,
        constrained,
        fingerprint,
    })
}

/// Fail-closed errors while lowering a typed resource policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourcePolicyLoweringError {
    Constrained(ConstraintLoweringError),
}

impl fmt::Display for ResourcePolicyLoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constrained(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ResourcePolicyLoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Constrained(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::{
        AdmissibleTransition, DimensionId, LogicalResourceId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::{
        BoolExpr, BooleanGuard, GuardScope, PolicyHeader, PolicyId, PolicyIdentity, PolicyTarget,
        PolicyVersion, PredicateKey, PredicateRegistry, PseudoBooleanConstraintDeclaration,
        PseudoBooleanScale, ResourcePolicySpec, TransitionMechanism, WeightedPredicateKey,
    };

    fn key(name: &str) -> PredicateKey {
        PredicateKey::new("elastic.policy", name).unwrap()
    }

    fn resource() -> ResourceSpec {
        ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("ram").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap()
    }

    fn header(version: PolicyVersion) -> PolicyHeader {
        PolicyHeader::new(
            PolicyIdentity::new(PolicyId::new("runtime.ram").unwrap(), version),
            PolicyTarget::resource(LogicalResourceId::new("ram").unwrap()),
        )
    }

    fn guard(name: &str) -> BooleanGuard {
        let key = key(name);
        let registry = PredicateRegistry::from_keys([key.clone()]).unwrap();
        let id = registry.id(&key).unwrap();
        BooleanGuard::new(GuardScope::Resource, registry, BoolExpr::atom(id)).unwrap()
    }

    fn budget(weight: i128) -> PseudoBooleanConstraintDeclaration {
        PseudoBooleanConstraintDeclaration::capacity_budget(
            vec![WeightedPredicateKey::new(key("high-mode"), weight).unwrap()],
            10,
            PseudoBooleanScale::new("units", 1).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn policy_fingerprint_binds_header_guard_and_constraint_semantics() {
        let base = lower_resource_policy(
            &ResourcePolicySpec::new(
                header(PolicyVersion::new(1, 0, 0)),
                resource(),
                vec![guard("capacity-ok")],
                vec![budget(4)],
            )
            .unwrap(),
        )
        .unwrap();
        let version_changed = lower_resource_policy(
            &ResourcePolicySpec::new(
                header(PolicyVersion::new(1, 0, 1)),
                resource(),
                vec![guard("capacity-ok")],
                vec![budget(4)],
            )
            .unwrap(),
        )
        .unwrap();
        let guard_changed = lower_resource_policy(
            &ResourcePolicySpec::new(
                header(PolicyVersion::new(1, 0, 0)),
                resource(),
                vec![guard("different")],
                vec![budget(4)],
            )
            .unwrap(),
        )
        .unwrap();
        let budget_changed = lower_resource_policy(
            &ResourcePolicySpec::new(
                header(PolicyVersion::new(1, 0, 0)),
                resource(),
                vec![guard("capacity-ok")],
                vec![budget(5)],
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(base.fingerprint(), version_changed.fingerprint());
        assert_ne!(base.fingerprint(), guard_changed.fingerprint());
        assert_ne!(base.fingerprint(), budget_changed.fingerprint());
    }

    #[test]
    fn constraint_input_order_is_canonical_in_policy_eir() {
        let a = PseudoBooleanConstraintDeclaration::at_most_keys([key("a")], 1).unwrap();
        let b = PseudoBooleanConstraintDeclaration::at_most_keys([key("b")], 1).unwrap();
        let left = lower_resource_policy(
            &ResourcePolicySpec::new(
                header(PolicyVersion::new(1, 0, 0)),
                resource(),
                vec![],
                vec![a.clone(), b.clone()],
            )
            .unwrap(),
        )
        .unwrap();
        let right = lower_resource_policy(
            &ResourcePolicySpec::new(
                header(PolicyVersion::new(1, 0, 0)),
                resource(),
                vec![],
                vec![b, a],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(left, right);
        assert_eq!(left.fingerprint(), right.fingerprint());
    }
}
