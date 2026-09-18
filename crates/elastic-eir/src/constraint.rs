//! Versioned pseudo-Boolean constraint representation in Elastic Intermediate Representation.
//!
//! Pseudo-Boolean declarations use stable [`PredicateKey`] identities and are
//! lowered into an immutable, deterministically ordered EIR envelope. This is
//! policy data only: lowering and fingerprinting never validate or authorize a
//! physical transition. The constrained-resource fingerprint binds the already
//! normalized guarded EIR identity plus every declared constraint.

use crate::{lower_guarded, EirGuardedResource, Fingerprint, ValidationError};
use elastic_core::{
    GuardedResourceSpec, PredicateKey, PseudoBooleanConstraintDeclaration, PseudoBooleanRelation,
    PseudoBooleanScale, BOOLEAN_PREDICATE_SCHEMA_V1,
};

/// Schema version of the pseudo-Boolean constraint EIR envelope.
pub const EIR_PSEUDO_BOOLEAN_CONSTRAINT_SCHEMA_VERSION: u16 = 1;

/// One durable weighted term in EIR canonical stable-key order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EirPseudoBooleanTerm {
    predicate: PredicateKey,
    weight: i128,
}

impl EirPseudoBooleanTerm {
    /// Stable predicate identity. Compact runtime predicate IDs are never persisted here.
    #[must_use]
    pub const fn predicate(&self) -> &PredicateKey {
        &self.predicate
    }

    /// Signed integer weight in the constraint scale's ticks.
    #[must_use]
    pub const fn weight(&self) -> i128 {
        self.weight
    }
}

/// One canonical pseudo-Boolean constraint in EIR.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EirPseudoBooleanConstraint {
    terms: Vec<EirPseudoBooleanTerm>,
    relation: PseudoBooleanRelation,
    threshold: i128,
    scale: PseudoBooleanScale,
    fingerprint: Fingerprint,
}

impl EirPseudoBooleanConstraint {
    /// Lower an already validated durable core declaration into deterministic EIR data.
    #[must_use]
    pub fn from_declaration(declaration: &PseudoBooleanConstraintDeclaration) -> Self {
        let terms = declaration
            .terms()
            .iter()
            .map(|term| EirPseudoBooleanTerm {
                predicate: term.predicate().clone(),
                weight: term.weight(),
            })
            .collect::<Vec<_>>();

        let mut fingerprint = Fingerprint::EMPTY
            .text("eir-pseudo-boolean-constraint")
            .number(u64::from(EIR_PSEUDO_BOOLEAN_CONSTRAINT_SCHEMA_VERSION))
            .number(u64::from(BOOLEAN_PREDICATE_SCHEMA_V1))
            .text(relation_text(declaration.relation()))
            .text(&declaration.threshold().to_string())
            .text(declaration.scale().unit())
            .number(declaration.scale().quantum())
            .number(terms.len() as u64);
        for term in &terms {
            fingerprint = fingerprint
                .text(term.predicate.namespace())
                .text(term.predicate.name())
                .text(&term.weight.to_string());
        }

        Self {
            terms,
            relation: declaration.relation(),
            threshold: declaration.threshold(),
            scale: declaration.scale().clone(),
            fingerprint,
        }
    }

    /// Canonically ordered durable terms.
    #[must_use]
    pub fn terms(&self) -> &[EirPseudoBooleanTerm] {
        &self.terms
    }

    /// Declared linear relation.
    #[must_use]
    pub const fn relation(&self) -> PseudoBooleanRelation {
        self.relation
    }

    /// Integer threshold in scale ticks.
    #[must_use]
    pub const fn threshold(&self) -> i128 {
        self.threshold
    }

    /// Explicit shared unit and integer quantum.
    #[must_use]
    pub const fn scale(&self) -> &PseudoBooleanScale {
        &self.scale
    }

    /// Structural fingerprint of the versioned durable constraint declaration.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Normalized guarded EIR resource together with canonical pseudo-Boolean constraints.
///
/// This is a new versioned envelope rather than a mutation of the historical
/// guarded-resource schema. Existing resource and guard fingerprints therefore
/// remain stable. The combined fingerprint is suitable for deterministic local
/// identity, caching, replay diagnostics, and tests inside one trust domain; it
/// is not cryptographic authentication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirConstrainedResource {
    guarded: EirGuardedResource,
    constraints: Vec<EirPseudoBooleanConstraint>,
    fingerprint: Fingerprint,
}

impl EirConstrainedResource {
    fn new(
        guarded: EirGuardedResource,
        mut constraints: Vec<EirPseudoBooleanConstraint>,
    ) -> Self {
        constraints.sort();
        let mut fingerprint = Fingerprint::EMPTY
            .text("eir-constrained-resource")
            .number(u64::from(EIR_PSEUDO_BOOLEAN_CONSTRAINT_SCHEMA_VERSION))
            .number(guarded.fingerprint().bits())
            .number(constraints.len() as u64);
        for constraint in &constraints {
            fingerprint = fingerprint.number(constraint.fingerprint().bits());
        }
        Self {
            guarded,
            constraints,
            fingerprint,
        }
    }

    /// Existing normalized resource and Boolean-guard policy.
    #[must_use]
    pub const fn guarded_resource(&self) -> &EirGuardedResource {
        &self.guarded
    }

    /// Canonical constraints ordered independently of caller input order.
    #[must_use]
    pub fn constraints(&self) -> &[EirPseudoBooleanConstraint] {
        &self.constraints
    }

    /// Combined structural identity of guarded EIR plus pseudo-Boolean constraints.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Lower a guarded resource and durable pseudo-Boolean declarations into a
/// deterministic constraint envelope.
///
/// The resource and guards go through [`lower_guarded`]. Constraint declarations
/// have already been structurally bounded and canonicalized by `elastic-core`;
/// this layer preserves their stable keys, explicit units/scales, relation, and
/// signed integer weights. No constraint is evaluated and no actuation authority
/// is created by this function.
///
/// # Errors
///
/// Returns the ordinary guarded-resource [`ValidationError`] if the base resource
/// cannot be lowered.
pub fn lower_constrained(
    spec: &GuardedResourceSpec,
    declarations: &[PseudoBooleanConstraintDeclaration],
) -> Result<EirConstrainedResource, ValidationError> {
    let guarded = lower_guarded(spec)?;
    let constraints = declarations
        .iter()
        .map(EirPseudoBooleanConstraint::from_declaration)
        .collect();
    Ok(EirConstrainedResource::new(guarded, constraints))
}

const fn relation_text(relation: PseudoBooleanRelation) -> &'static str {
    match relation {
        PseudoBooleanRelation::LessOrEqual => "le",
        PseudoBooleanRelation::GreaterOrEqual => "ge",
        PseudoBooleanRelation::Equal => "eq",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::{
        AdmissibleTransition, DimensionId, LogicalResourceId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::{
        BoolExpr, BooleanGuard, GuardScope, PredicateRegistry, TransitionMechanism,
        WeightedPredicateKey,
    };

    fn predicate(name: &str) -> PredicateKey {
        PredicateKey::new("elastic.memory", name).unwrap()
    }

    fn guarded_resource() -> GuardedResourceSpec {
        let resource = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("eir-constrained-memory").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap();
        let key = predicate("capacity-ok");
        let registry = PredicateRegistry::from_keys([key.clone()]).unwrap();
        let id = registry.id(&key).unwrap();
        let guard = BooleanGuard::new(
            GuardScope::Resource,
            registry,
            BoolExpr::atom(id),
        )
        .unwrap();
        GuardedResourceSpec::new(resource, vec![guard]).unwrap()
    }

    fn budget(weight: i128, threshold: i128) -> PseudoBooleanConstraintDeclaration {
        PseudoBooleanConstraintDeclaration::new(
            vec![WeightedPredicateKey::new(predicate("high-memory-mode"), weight).unwrap()],
            PseudoBooleanRelation::LessOrEqual,
            threshold,
            PseudoBooleanScale::new("MiB", 1).unwrap(),
        )
        .unwrap()
    }

    fn cardinality() -> PseudoBooleanConstraintDeclaration {
        PseudoBooleanConstraintDeclaration::at_most_keys(
            [predicate("gpu-a"), predicate("gpu-b")],
            1,
        )
        .unwrap()
    }

    #[test]
    fn lowering_preserves_stable_identity_units_relation_and_signed_values() {
        let lowered = lower_constrained(&guarded_resource(), &[budget(-3, 8)]).unwrap();
        let constraint = &lowered.constraints()[0];
        assert_eq!(constraint.terms().len(), 1);
        assert_eq!(
            constraint.terms()[0].predicate().to_string(),
            "elastic.memory::high-memory-mode"
        );
        assert_eq!(constraint.terms()[0].weight(), -3);
        assert_eq!(constraint.relation(), PseudoBooleanRelation::LessOrEqual);
        assert_eq!(constraint.threshold(), 8);
        assert_eq!(constraint.scale().unit(), "MiB");
        assert_eq!(constraint.scale().quantum(), 1);
    }

    #[test]
    fn constrained_fingerprint_is_independent_of_constraint_input_order() {
        let spec = guarded_resource();
        let first = budget(4, 8);
        let second = cardinality();
        let forward = lower_constrained(&spec, &[first.clone(), second.clone()]).unwrap();
        let reverse = lower_constrained(&spec, &[second, first]).unwrap();
        assert_eq!(forward.constraints(), reverse.constraints());
        assert_eq!(forward.fingerprint(), reverse.fingerprint());
    }

    #[test]
    fn constraint_identity_changes_with_semantically_material_fields() {
        let spec = guarded_resource();
        let baseline = lower_constrained(&spec, &[budget(4, 8)]).unwrap();
        let changed_weight = lower_constrained(&spec, &[budget(5, 8)]).unwrap();
        let changed_threshold = lower_constrained(&spec, &[budget(4, 9)]).unwrap();
        let changed_relation = PseudoBooleanConstraintDeclaration::new(
            vec![WeightedPredicateKey::new(predicate("high-memory-mode"), 4).unwrap()],
            PseudoBooleanRelation::GreaterOrEqual,
            8,
            PseudoBooleanScale::new("MiB", 1).unwrap(),
        )
        .unwrap();
        let changed_relation = lower_constrained(&spec, &[changed_relation]).unwrap();
        let changed_scale = PseudoBooleanConstraintDeclaration::new(
            vec![WeightedPredicateKey::new(predicate("high-memory-mode"), 4).unwrap()],
            PseudoBooleanRelation::LessOrEqual,
            8,
            PseudoBooleanScale::new("bytes", 1024).unwrap(),
        )
        .unwrap();
        let changed_scale = lower_constrained(&spec, &[changed_scale]).unwrap();

        for changed in [
            changed_weight,
            changed_threshold,
            changed_relation,
            changed_scale,
        ] {
            assert_ne!(baseline.fingerprint(), changed.fingerprint());
            assert_ne!(
                baseline.constraints()[0].fingerprint(),
                changed.constraints()[0].fingerprint()
            );
        }
    }

    #[test]
    fn base_guard_identity_is_bound_into_constrained_resource_identity() {
        let baseline_spec = guarded_resource();
        let baseline = lower_constrained(&baseline_spec, &[budget(4, 8)]).unwrap();

        let resource = baseline_spec.resource().clone();
        let changed_key = predicate("different-capacity-ok");
        let registry = PredicateRegistry::from_keys([changed_key.clone()]).unwrap();
        let changed_id = registry.id(&changed_key).unwrap();
        let changed_spec = GuardedResourceSpec::new(
            resource,
            vec![BooleanGuard::new(
                GuardScope::Resource,
                registry,
                BoolExpr::atom(changed_id),
            )
            .unwrap()],
        )
        .unwrap();
        let changed = lower_constrained(&changed_spec, &[budget(4, 8)]).unwrap();

        assert_ne!(
            baseline.guarded_resource().fingerprint(),
            changed.guarded_resource().fingerprint()
        );
        assert_ne!(baseline.fingerprint(), changed.fingerprint());
    }
}