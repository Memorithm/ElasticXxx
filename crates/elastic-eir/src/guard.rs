//! Boolean guard representation in Elastic Intermediate Representation.
//!
//! This module introduces a versioned, deterministic guard envelope without
//! changing the historical EIR v1 resource shape. The wrapper fingerprint binds
//! the normalized base resource and its guard set, allowing later runtime
//! layers to consume Boolean policy without silently changing legacy EIR
//! semantics.

use crate::{lower, EirResource, Fingerprint, ValidationError};
use elastic_core::{
    BoolExpr, BoolExprFingerprint, BooleanGuard, GuardScope, GuardedResourceSpec, PredicateId,
    PredicateKey, TransitionMechanism, BOOLEAN_EXPRESSION_SCHEMA_V1, BOOLEAN_PREDICATE_SCHEMA_V1,
};
use std::fmt;

/// Schema version of the Boolean guard EIR envelope.
pub const EIR_BOOLEAN_GUARD_SCHEMA_VERSION: u16 = 1;

/// One canonical predicate entry in EIR compact-ID order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirPredicate {
    id: PredicateId,
    key: PredicateKey,
}

impl EirPredicate {
    /// Compact predicate identifier used by the canonical expression.
    #[must_use]
    pub const fn id(&self) -> PredicateId {
        self.id
    }

    /// Stable predicate key bound to this compact identifier.
    #[must_use]
    pub const fn key(&self) -> &PredicateKey {
        &self.key
    }
}

/// One canonical Boolean guard in EIR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirGuard {
    scope: GuardScope,
    predicates: Vec<EirPredicate>,
    expression: BoolExpr,
    expression_fingerprint: BoolExprFingerprint,
    fingerprint: Fingerprint,
}

impl EirGuard {
    /// Lower one already-validated core guard into deterministic EIR data.
    #[must_use]
    pub fn from_guard(guard: &BooleanGuard) -> Self {
        let predicates = guard
            .predicates()
            .iter()
            .map(|(id, key)| EirPredicate {
                id,
                key: key.clone(),
            })
            .collect::<Vec<_>>();

        let mut fingerprint = Fingerprint::EMPTY
            .text("eir-boolean-guard")
            .number(u64::from(EIR_BOOLEAN_GUARD_SCHEMA_VERSION))
            .number(u64::from(BOOLEAN_PREDICATE_SCHEMA_V1))
            .number(u64::from(BOOLEAN_EXPRESSION_SCHEMA_V1));
        fingerprint = fingerprint_scope(fingerprint, guard.scope());
        for predicate in &predicates {
            fingerprint = fingerprint
                .number(u64::from(predicate.id.index()))
                .text(predicate.key.namespace())
                .text(predicate.key.name());
        }
        fingerprint = fingerprint.number(guard.fingerprint().bits());

        Self {
            scope: guard.scope().clone(),
            predicates,
            expression: guard.expression().clone(),
            expression_fingerprint: guard.fingerprint(),
            fingerprint,
        }
    }

    /// Guard scope.
    #[must_use]
    pub const fn scope(&self) -> &GuardScope {
        &self.scope
    }

    /// Canonical predicate table in compact-ID order.
    #[must_use]
    pub fn predicates(&self) -> &[EirPredicate] {
        &self.predicates
    }

    /// Canonical Boolean expression.
    #[must_use]
    pub const fn expression(&self) -> &BoolExpr {
        &self.expression
    }

    /// Stable expression fingerprint inherited from the core contract.
    #[must_use]
    pub const fn expression_fingerprint(&self) -> BoolExprFingerprint {
        self.expression_fingerprint
    }

    /// EIR structural fingerprint including schema versions, scope, predicates,
    /// and canonical expression identity.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Normalized EIR resource together with canonical Boolean guards.
///
/// The combined fingerprint is a separate identity from the legacy
/// [`EirResource::fingerprint`]. Legacy unguarded EIR remains byte-for-byte and
/// schema-version compatible while guarded consumers can bind to the stronger
/// identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirGuardedResource {
    resource: EirResource,
    guards: Vec<EirGuard>,
    fingerprint: Fingerprint,
}

impl EirGuardedResource {
    fn new(resource: EirResource, guards: Vec<EirGuard>) -> Self {
        let mut fingerprint = Fingerprint::EMPTY
            .text("eir-guarded-resource")
            .number(u64::from(EIR_BOOLEAN_GUARD_SCHEMA_VERSION))
            .number(resource.fingerprint().bits());
        for guard in &guards {
            fingerprint = fingerprint.number(guard.fingerprint().bits());
        }
        Self {
            resource,
            guards,
            fingerprint,
        }
    }

    /// Normalized legacy EIR resource node.
    #[must_use]
    pub const fn resource(&self) -> &EirResource {
        &self.resource
    }

    /// Canonical guards in scope order inherited from [`GuardedResourceSpec`].
    #[must_use]
    pub fn guards(&self) -> &[EirGuard] {
        &self.guards
    }

    /// Combined structural identity of base EIR plus guard policy.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

impl fmt::Display for EirGuardedResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{} Boolean guards]",
            self.resource,
            self.guards.len()
        )
    }
}

/// Lower a typed guarded resource declaration into deterministic EIR.
///
/// The underlying resource is lowered through the same legacy EIR validation
/// path as [`crate::lower`]. Guards are already canonical and scope-validated by
/// [`GuardedResourceSpec`], then copied into an immutable EIR envelope.
///
/// # Errors
///
/// Returns the same [`ValidationError`] as ordinary resource lowering if the
/// underlying resource declaration fails EIR structural validation.
pub fn lower_guarded(spec: &GuardedResourceSpec) -> Result<EirGuardedResource, ValidationError> {
    let document = lower(spec.resource())?;
    let resource = document
        .resources()
        .first()
        .expect("lowering one validated ResourceSpec always yields one resource")
        .clone();
    let guards = spec.guards().iter().map(EirGuard::from_guard).collect();
    Ok(EirGuardedResource::new(resource, guards))
}

fn fingerprint_scope(mut fingerprint: Fingerprint, scope: &GuardScope) -> Fingerprint {
    match scope {
        GuardScope::Resource => fingerprint.text("resource"),
        GuardScope::Dimension(dimension) => fingerprint.text("dimension").text(dimension.as_str()),
        GuardScope::Transition {
            mechanism,
            dimension,
        } => {
            fingerprint = fingerprint
                .text("transition")
                .text(mechanism_text(*mechanism));
            fingerprint.text(dimension.as_str())
        }
    }
}

const fn mechanism_text(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::{
        AdmissibleTransition, DimensionId, LogicalResourceId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::{BoolExpr, BooleanGuard, PredicateKey, PredicateRegistry};

    fn guarded_resource(reverse_guard_order: bool) -> GuardedResourceSpec {
        let resource = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("eir-guarded-memory").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap();

        let registry = PredicateRegistry::from_keys([
            PredicateKey::new("elastic.memory", "capacity-ok").unwrap(),
            PredicateKey::new("elastic.memory", "pressure-critical").unwrap(),
        ])
        .unwrap();
        let capacity_ok = registry
            .id(&PredicateKey::new("elastic.memory", "capacity-ok").unwrap())
            .unwrap();
        let pressure_critical = registry
            .id(&PredicateKey::new("elastic.memory", "pressure-critical").unwrap())
            .unwrap();

        let resource_guard = BooleanGuard::new(
            GuardScope::Resource,
            registry.clone(),
            BoolExpr::atom(capacity_ok),
        )
        .unwrap();
        let transition_guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CAPACITY,
            },
            registry,
            BoolExpr::negate(BoolExpr::atom(pressure_critical)),
        )
        .unwrap();

        let guards = if reverse_guard_order {
            vec![transition_guard, resource_guard]
        } else {
            vec![resource_guard, transition_guard]
        };
        GuardedResourceSpec::new(resource, guards).unwrap()
    }

    #[test]
    fn lowering_preserves_canonical_predicate_identity_and_scope() {
        let lowered = lower_guarded(&guarded_resource(false)).unwrap();
        assert_eq!(lowered.guards().len(), 2);
        assert_eq!(lowered.guards()[0].scope(), &GuardScope::Resource);
        assert_eq!(
            lowered.guards()[0].predicates()[0].id(),
            PredicateId::new(0)
        );
        assert_eq!(
            lowered.guards()[0].predicates()[0].key().to_string(),
            "elastic.memory::capacity-ok"
        );
    }

    #[test]
    fn lowering_and_combined_fingerprint_are_independent_of_guard_input_order() {
        let forward = lower_guarded(&guarded_resource(false)).unwrap();
        let reverse = lower_guarded(&guarded_resource(true)).unwrap();
        assert_eq!(forward, reverse);
        assert_eq!(forward.fingerprint(), reverse.fingerprint());
    }

    #[test]
    fn combined_fingerprint_changes_when_guard_policy_changes() {
        let baseline = guarded_resource(false);
        let baseline_eir = lower_guarded(&baseline).unwrap();

        let resource = baseline.resource().clone();
        let registry =
            PredicateRegistry::from_keys([
                PredicateKey::new("elastic.memory", "capacity-ok").unwrap()
            ])
            .unwrap();
        let capacity_ok = registry
            .id(&PredicateKey::new("elastic.memory", "capacity-ok").unwrap())
            .unwrap();
        let changed = GuardedResourceSpec::new(
            resource,
            vec![BooleanGuard::new(
                GuardScope::Resource,
                registry,
                BoolExpr::negate(BoolExpr::atom(capacity_ok)),
            )
            .unwrap()],
        )
        .unwrap();
        let changed_eir = lower_guarded(&changed).unwrap();

        assert_ne!(baseline_eir.fingerprint(), changed_eir.fingerprint());
    }
}
