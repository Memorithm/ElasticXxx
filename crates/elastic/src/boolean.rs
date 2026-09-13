//! Ergonomic public Boolean guard construction for downstream Elastic users.
//!
//! This module is a façade only. It introduces no second evaluator, truth
//! model, predicate registry, or Boolean AST. [`ElasticPredicates`] resolves
//! stable [`PredicateKey`] values through the canonical `elastic-core`
//! [`PredicateRegistry`], and [`ElasticGuard`] lowers every convenience method
//! directly into the existing [`BoolExpr`] and [`BooleanGuard`] contracts.

use elastic_core::resource::DimensionId;
use elastic_core::{
    BoolExpr, BooleanGuard, CanonicalizationError, GuardScope, PredicateKey, PredicateRegistry,
    PredicateRegistryError, TransitionMechanism,
};
use std::fmt;

/// Construct one validated stable predicate identity through the public facade.
///
/// This is exactly [`PredicateKey::new`]; the helper exists so typical
/// downstream code never needs to name an implementation crate.
pub fn predicate(
    namespace: impl Into<String>,
    name: impl Into<String>,
) -> Result<PredicateKey, PredicateRegistryError> {
    PredicateKey::new(namespace, name)
}

/// Canonical stable-key predicate registry used by ergonomic guard builders.
///
/// Construction sorts and deduplicates the supplied stable keys exactly as
/// [`PredicateRegistry::from_keys`] does. Expressions produced by this type are
/// ordinary core [`BoolExpr`] values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElasticPredicates {
    registry: PredicateRegistry,
}

impl ElasticPredicates {
    /// Build a deterministic predicate registry from stable keys.
    pub fn new(
        keys: impl IntoIterator<Item = PredicateKey>,
    ) -> Result<Self, PredicateRegistryError> {
        PredicateRegistry::from_keys(keys).map(|registry| Self { registry })
    }

    /// Build an empty predicate registry.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            registry: PredicateRegistry::empty(),
        }
    }

    /// Borrow the single canonical registry used by all generated expressions.
    #[must_use]
    pub const fn registry(&self) -> &PredicateRegistry {
        &self.registry
    }

    /// Convert one registered stable key into the core atom assigned by this
    /// canonical registry.
    ///
    /// # Errors
    ///
    /// Returns [`ElasticGuardError::UnknownPredicate`] when `key` was not part
    /// of this registry. No implicit registration or ID fabrication occurs.
    pub fn atom(&self, key: &PredicateKey) -> Result<BoolExpr, ElasticGuardError> {
        let Some(id) = self.registry.id(key) else {
            return Err(ElasticGuardError::UnknownPredicate { key: key.clone() });
        };
        Ok(BoolExpr::atom(id))
    }

    /// Conjunction of registered predicate atoms.
    pub fn all_atoms<'a>(
        &self,
        keys: impl IntoIterator<Item = &'a PredicateKey>,
    ) -> Result<BoolExpr, ElasticGuardError> {
        keys.into_iter()
            .map(|key| self.atom(key))
            .collect::<Result<Vec<_>, _>>()
            .map(BoolExpr::all)
    }

    /// Disjunction of registered predicate atoms.
    pub fn any_atoms<'a>(
        &self,
        keys: impl IntoIterator<Item = &'a PredicateKey>,
    ) -> Result<BoolExpr, ElasticGuardError> {
        keys.into_iter()
            .map(|key| self.atom(key))
            .collect::<Result<Vec<_>, _>>()
            .map(BoolExpr::any)
    }
}

/// User-facing builder for one typed Boolean guard scope.
///
/// The builder owns no evaluation semantics. Every terminal method returns the
/// core [`BooleanGuard`] type after the same bounded canonicalization and stable
/// fingerprinting used by internal callers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElasticGuard {
    scope: GuardScope,
    predicates: ElasticPredicates,
}

impl ElasticGuard {
    /// Create a resource-wide guard builder.
    #[must_use]
    pub const fn resource(predicates: ElasticPredicates) -> Self {
        Self {
            scope: GuardScope::Resource,
            predicates,
        }
    }

    /// Create a dimension-scoped guard builder.
    #[must_use]
    pub const fn dimension(dimension: DimensionId, predicates: ElasticPredicates) -> Self {
        Self {
            scope: GuardScope::Dimension(dimension),
            predicates,
        }
    }

    /// Create a transition-scoped guard builder.
    #[must_use]
    pub const fn transition(
        mechanism: TransitionMechanism,
        dimension: DimensionId,
        predicates: ElasticPredicates,
    ) -> Self {
        Self {
            scope: GuardScope::Transition {
                mechanism,
                dimension,
            },
            predicates,
        }
    }

    /// Borrow this builder's scope.
    #[must_use]
    pub const fn scope(&self) -> &GuardScope {
        &self.scope
    }

    /// Borrow the canonical predicate registry wrapper.
    #[must_use]
    pub const fn predicates(&self) -> &ElasticPredicates {
        &self.predicates
    }

    /// Build a guard from an arbitrary core Boolean expression.
    ///
    /// # Errors
    ///
    /// Propagates the core bounded canonicalization/fingerprint contract.
    pub fn when(&self, expression: BoolExpr) -> Result<BooleanGuard, ElasticGuardError> {
        BooleanGuard::when(
            self.scope.clone(),
            self.predicates.registry.clone(),
            expression,
        )
        .map_err(ElasticGuardError::Canonicalization)
    }

    /// Require one registered stable predicate to be true.
    pub fn requires(&self, key: &PredicateKey) -> Result<BooleanGuard, ElasticGuardError> {
        let Some(id) = self.predicates.registry.id(key) else {
            return Err(ElasticGuardError::UnknownPredicate { key: key.clone() });
        };
        BooleanGuard::requires(self.scope.clone(), self.predicates.registry.clone(), id)
            .map_err(ElasticGuardError::Canonicalization)
    }

    /// Require one registered stable predicate to be false.
    pub fn forbids(&self, key: &PredicateKey) -> Result<BooleanGuard, ElasticGuardError> {
        let Some(id) = self.predicates.registry.id(key) else {
            return Err(ElasticGuardError::UnknownPredicate { key: key.clone() });
        };
        BooleanGuard::forbids(self.scope.clone(), self.predicates.registry.clone(), id)
            .map_err(ElasticGuardError::Canonicalization)
    }

    /// Ergonomic conjunction constructor over ordinary core expressions.
    #[must_use]
    pub fn all(expressions: impl IntoIterator<Item = BoolExpr>) -> BoolExpr {
        BoolExpr::all(expressions)
    }

    /// Ergonomic disjunction constructor over ordinary core expressions.
    #[must_use]
    pub fn any(expressions: impl IntoIterator<Item = BoolExpr>) -> BoolExpr {
        BoolExpr::any(expressions)
    }

    /// Ergonomic negation constructor over an ordinary core expression.
    #[must_use]
    pub fn not(expression: BoolExpr) -> BoolExpr {
        BoolExpr::negate(expression)
    }

    /// Ergonomic exclusive-or constructor over ordinary core expressions.
    #[must_use]
    pub fn xor(lhs: BoolExpr, rhs: BoolExpr) -> BoolExpr {
        BoolExpr::Xor(Box::new(lhs), Box::new(rhs))
    }

    /// Ergonomic material-implication constructor over ordinary core
    /// expressions.
    #[must_use]
    pub fn implies(lhs: BoolExpr, rhs: BoolExpr) -> BoolExpr {
        BoolExpr::Implies(Box::new(lhs), Box::new(rhs))
    }
}

/// Public façade construction failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElasticGuardError {
    /// A stable key was used with a registry that does not contain it.
    UnknownPredicate { key: PredicateKey },
    /// Core canonicalization/fingerprinting rejected the expression.
    Canonicalization(CanonicalizationError),
}

impl fmt::Display for ElasticGuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPredicate { key } => {
                write!(f, "predicate {key} is not registered for this guard")
            }
            Self::Canonicalization(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ElasticGuardError {}

impl From<CanonicalizationError> for ElasticGuardError {
    fn from(value: CanonicalizationError) -> Self {
        Self::Canonicalization(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::TruthValue;
    use std::collections::BTreeMap;

    #[test]
    fn stable_key_builder_lowers_to_the_core_guard_semantics() {
        let capacity = predicate("elastic.ram", "capacity-ok").unwrap();
        let pressure = predicate("elastic.ram", "pressure-critical").unwrap();
        let predicates = ElasticPredicates::new([pressure.clone(), capacity.clone()]).unwrap();
        let expression = ElasticGuard::all([
            predicates.atom(&capacity).unwrap(),
            ElasticGuard::not(predicates.atom(&pressure).unwrap()),
        ]);
        let guard = ElasticGuard::transition(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
            predicates,
        )
        .when(expression)
        .unwrap();
        let facts = BTreeMap::from([(capacity, TruthValue::True), (pressure, TruthValue::False)]);

        assert_eq!(guard.evaluate(&facts).unwrap(), TruthValue::True);
        assert!(matches!(guard.scope(), GuardScope::Transition { .. }));
    }

    #[test]
    fn insertion_order_does_not_change_public_guard_identity() {
        let a = predicate("elastic.test", "a").unwrap();
        let b = predicate("elastic.test", "b").unwrap();
        let first = ElasticPredicates::new([b.clone(), a.clone()]).unwrap();
        let second = ElasticPredicates::new([a.clone(), b.clone()]).unwrap();

        let first_guard = ElasticGuard::resource(first.clone())
            .when(ElasticGuard::all([
                first.atom(&a).unwrap(),
                first.atom(&b).unwrap(),
            ]))
            .unwrap();
        let second_guard = ElasticGuard::resource(second.clone())
            .when(ElasticGuard::all([
                second.atom(&b).unwrap(),
                second.atom(&a).unwrap(),
            ]))
            .unwrap();

        assert_eq!(first_guard.expression(), second_guard.expression());
        assert_eq!(first_guard.fingerprint(), second_guard.fingerprint());
    }

    #[test]
    fn unregistered_stable_key_fails_closed() {
        let registered = predicate("elastic.test", "registered").unwrap();
        let foreign = predicate("elastic.test", "foreign").unwrap();
        let predicates = ElasticPredicates::new([registered]).unwrap();

        assert!(matches!(
            predicates.atom(&foreign),
            Err(ElasticGuardError::UnknownPredicate { .. })
        ));
        assert!(matches!(
            ElasticGuard::resource(predicates).requires(&foreign),
            Err(ElasticGuardError::UnknownPredicate { .. })
        ));
    }

    #[test]
    fn convenience_operators_are_the_core_ast_variants() {
        let a = BoolExpr::atom(elastic_core::PredicateId::new(0));
        let b = BoolExpr::atom(elastic_core::PredicateId::new(1));

        assert!(matches!(
            ElasticGuard::all([a.clone(), b.clone()]),
            BoolExpr::All(_)
        ));
        assert!(matches!(
            ElasticGuard::any([a.clone(), b.clone()]),
            BoolExpr::Any(_)
        ));
        assert!(matches!(ElasticGuard::not(a.clone()), BoolExpr::Not(_)));
        assert!(matches!(
            ElasticGuard::xor(a.clone(), b.clone()),
            BoolExpr::Xor(_, _)
        ));
        assert!(matches!(
            ElasticGuard::implies(a, b),
            BoolExpr::Implies(_, _)
        ));
    }
}
