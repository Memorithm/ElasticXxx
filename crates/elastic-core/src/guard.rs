//! Typed Boolean guard declarations and resource binding.
//!
//! Guards are declarative data. They scope one canonical Boolean expression to
//! a resource, elastic dimension, or already-admitted transition. A guard never
//! executes an effect and never makes an undeclared transition legal. Runtime
//! interpretation is deliberately layered later in `elastic-runtime`.

use crate::resource::{DimensionId, ResourceSpec};
use crate::{
    BoolExpr, BoolExprFingerprint, CanonicalizationError, FactSet, LogicError, PredicateId,
    PredicateKey, PredicateRegistry, TransitionMechanism, TruthValue,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Scope to which one Boolean guard declaration applies.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuardScope {
    /// Guard applies to the logical resource as a whole.
    Resource,
    /// Guard applies when considering changes along one elastic dimension.
    Dimension(DimensionId),
    /// Guard applies to one mechanism/dimension transition pair.
    Transition {
        /// Transition materialization mechanism.
        mechanism: TransitionMechanism,
        /// Elastic dimension moved by the transition.
        dimension: DimensionId,
    },
}

impl GuardScope {
    /// Dimension constrained by this scope, if any.
    #[must_use]
    pub const fn dimension(&self) -> Option<&DimensionId> {
        match self {
            Self::Resource => None,
            Self::Dimension(dimension) | Self::Transition { dimension, .. } => Some(dimension),
        }
    }

    /// Transition mechanism constrained by this scope, if transition-specific.
    #[must_use]
    pub const fn mechanism(&self) -> Option<TransitionMechanism> {
        match self {
            Self::Transition { mechanism, .. } => Some(*mechanism),
            Self::Resource | Self::Dimension(_) => None,
        }
    }

    /// Whether this scope constrains one concrete transition.
    #[must_use]
    pub fn applies_to(&self, mechanism: TransitionMechanism, dimension: &DimensionId) -> bool {
        match self {
            Self::Resource => true,
            Self::Dimension(scoped_dimension) => scoped_dimension == dimension,
            Self::Transition {
                mechanism: scoped_mechanism,
                dimension: scoped_dimension,
            } => *scoped_mechanism == mechanism && scoped_dimension == dimension,
        }
    }
}

impl fmt::Display for GuardScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource => f.write_str("resource"),
            Self::Dimension(dimension) => write!(f, "dimension:{dimension}"),
            Self::Transition {
                mechanism,
                dimension,
            } => write!(f, "transition:{mechanism:?}@{dimension}"),
        }
    }
}

/// Stable source of three-valued facts used to evaluate guards.
///
/// Missing evidence must be returned as [`TruthValue::Unknown`]. BE5 will add
/// runtime snapshots that implement this contract; BE4 intentionally keeps the
/// semantics independent from observation acquisition.
pub trait GuardFactSource {
    /// Return the current truth value for one stable predicate key.
    fn truth(&self, key: &PredicateKey) -> TruthValue;
}

impl GuardFactSource for BTreeMap<PredicateKey, TruthValue> {
    fn truth(&self, key: &PredicateKey) -> TruthValue {
        self.get(key).copied().unwrap_or(TruthValue::Unknown)
    }
}

/// Canonical, stable-identity Boolean policy declaration.
///
/// Construction validates that every expression atom is registered, applies
/// the semantics-preserving canonicalizer, and binds the result to a stable
/// expression fingerprint derived from predicate keys rather than compact IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BooleanGuard {
    scope: GuardScope,
    predicates: PredicateRegistry,
    expression: BoolExpr,
    fingerprint: BoolExprFingerprint,
}

impl BooleanGuard {
    /// Validate and canonicalize one guard declaration.
    ///
    /// # Errors
    ///
    /// Returns a canonicalization error for oversized expressions or atoms not
    /// present in `predicates`.
    pub fn new(
        scope: GuardScope,
        predicates: PredicateRegistry,
        expression: BoolExpr,
    ) -> Result<Self, CanonicalizationError> {
        let expression = expression.canonicalize()?;
        let fingerprint = expression.canonical_fingerprint(&predicates)?;
        Ok(Self {
            scope,
            predicates,
            expression,
            fingerprint,
        })
    }

    /// Alias for declaring an arbitrary conditional guard.
    pub fn when(
        scope: GuardScope,
        predicates: PredicateRegistry,
        expression: BoolExpr,
    ) -> Result<Self, CanonicalizationError> {
        Self::new(scope, predicates, expression)
    }

    /// Declare that one predicate must be true.
    pub fn requires(
        scope: GuardScope,
        predicates: PredicateRegistry,
        predicate: PredicateId,
    ) -> Result<Self, CanonicalizationError> {
        Self::new(scope, predicates, BoolExpr::atom(predicate))
    }

    /// Declare that one predicate must be false.
    pub fn forbids(
        scope: GuardScope,
        predicates: PredicateRegistry,
        predicate: PredicateId,
    ) -> Result<Self, CanonicalizationError> {
        Self::new(
            scope,
            predicates,
            BoolExpr::negate(BoolExpr::atom(predicate)),
        )
    }

    /// Evaluate this guard from stable-key facts under strong Kleene semantics.
    ///
    /// Missing source values become `Unknown`; only an explicit `True` can be
    /// treated by later planning layers as satisfied.
    pub fn evaluate(&self, source: &impl GuardFactSource) -> Result<TruthValue, LogicError> {
        let mut facts = FactSet::new();
        for (id, key) in self.predicates.iter() {
            facts.set(id, source.truth(key))?;
        }
        self.expression.evaluate(&facts)
    }

    /// Declaration scope.
    #[must_use]
    pub const fn scope(&self) -> &GuardScope {
        &self.scope
    }

    /// Deterministic predicate registry used by the expression.
    #[must_use]
    pub const fn predicates(&self) -> &PredicateRegistry {
        &self.predicates
    }

    /// Canonical Boolean expression.
    #[must_use]
    pub const fn expression(&self) -> &BoolExpr {
        &self.expression
    }

    /// Stable structural fingerprint of the canonical expression.
    #[must_use]
    pub const fn fingerprint(&self) -> BoolExprFingerprint {
        self.fingerprint
    }
}

/// Specialized transition-scoped guard convenience wrapper.
///
/// This type introduces no second semantics: it always lowers to one
/// [`BooleanGuard`] whose scope is [`GuardScope::Transition`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionGuard(BooleanGuard);

impl TransitionGuard {
    /// Guard a transition with an arbitrary Boolean expression.
    pub fn when(
        mechanism: TransitionMechanism,
        dimension: DimensionId,
        predicates: PredicateRegistry,
        expression: BoolExpr,
    ) -> Result<Self, CanonicalizationError> {
        BooleanGuard::when(
            GuardScope::Transition {
                mechanism,
                dimension,
            },
            predicates,
            expression,
        )
        .map(Self)
    }

    /// Require one predicate to be true for the transition.
    pub fn requires(
        mechanism: TransitionMechanism,
        dimension: DimensionId,
        predicates: PredicateRegistry,
        predicate: PredicateId,
    ) -> Result<Self, CanonicalizationError> {
        BooleanGuard::requires(
            GuardScope::Transition {
                mechanism,
                dimension,
            },
            predicates,
            predicate,
        )
        .map(Self)
    }

    /// Require one predicate to be false for the transition.
    pub fn forbids(
        mechanism: TransitionMechanism,
        dimension: DimensionId,
        predicates: PredicateRegistry,
        predicate: PredicateId,
    ) -> Result<Self, CanonicalizationError> {
        BooleanGuard::forbids(
            GuardScope::Transition {
                mechanism,
                dimension,
            },
            predicates,
            predicate,
        )
        .map(Self)
    }

    /// Borrow the single underlying semantic guard.
    #[must_use]
    pub const fn as_guard(&self) -> &BooleanGuard {
        &self.0
    }

    /// Consume this convenience wrapper into the shared core guard type.
    #[must_use]
    pub fn into_guard(self) -> BooleanGuard {
        self.0
    }
}

/// Error while binding canonical guards to one typed resource declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardBindingError {
    /// More than one guard targeted exactly the same scope.
    DuplicateScope { scope: GuardScope },
    /// A dimension-scoped guard targeted a non-elastic dimension.
    NonElasticDimension { scope: GuardScope },
    /// A transition-scoped guard targeted a transition not admitted by the
    /// resource declaration.
    UnadmittedTransition { scope: GuardScope },
}

impl fmt::Display for GuardBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateScope { scope } => {
                write!(f, "more than one Boolean guard targets {scope}")
            }
            Self::NonElasticDimension { scope } => {
                write!(
                    f,
                    "Boolean guard scope {scope} targets a non-elastic dimension"
                )
            }
            Self::UnadmittedTransition { scope } => {
                write!(
                    f,
                    "Boolean guard scope {scope} targets an unadmitted transition"
                )
            }
        }
    }
}

impl std::error::Error for GuardBindingError {}

/// A validated resource declaration together with its canonical Boolean guards.
///
/// This wrapper deliberately leaves [`ResourceSpec`] source-compatible while
/// the guard contract matures. One guard is allowed per scope; callers compose
/// multiple conditions explicitly inside one [`BoolExpr::All`] or
/// [`BoolExpr::Any`], avoiding ambiguous implicit composition rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedResourceSpec {
    resource: ResourceSpec,
    guards: Vec<BooleanGuard>,
}

impl GuardedResourceSpec {
    /// Bind guards to one already-validated resource declaration.
    ///
    /// Guards are sorted by scope after validation. Resource-level guards are
    /// always legal; dimension scopes must name declared elastic dimensions;
    /// transition scopes must match an admitted mechanism/dimension pair.
    ///
    /// # Errors
    ///
    /// Returns a binding error for duplicate scopes or targets outside the
    /// resource's admitted elasticity surface.
    pub fn new(
        resource: ResourceSpec,
        mut guards: Vec<BooleanGuard>,
    ) -> Result<Self, GuardBindingError> {
        let mut seen = BTreeSet::new();
        for guard in &guards {
            let scope = guard.scope();
            if !seen.insert(scope.clone()) {
                return Err(GuardBindingError::DuplicateScope {
                    scope: scope.clone(),
                });
            }
            match scope {
                GuardScope::Resource => {}
                GuardScope::Dimension(dimension) => {
                    if !resource.is_elastic(dimension) {
                        return Err(GuardBindingError::NonElasticDimension {
                            scope: scope.clone(),
                        });
                    }
                }
                GuardScope::Transition {
                    mechanism,
                    dimension,
                } => {
                    if !resource.admits(*mechanism, dimension) {
                        return Err(GuardBindingError::UnadmittedTransition {
                            scope: scope.clone(),
                        });
                    }
                }
            }
        }
        guards.sort_by(|left, right| left.scope().cmp(right.scope()));
        Ok(Self { resource, guards })
    }

    /// Underlying validated resource declaration.
    #[must_use]
    pub const fn resource(&self) -> &ResourceSpec {
        &self.resource
    }

    /// Canonical guards, sorted by scope.
    #[must_use]
    pub fn guards(&self) -> &[BooleanGuard] {
        &self.guards
    }

    /// Consume the wrapper into its resource and guard parts.
    #[must_use]
    pub fn into_parts(self) -> (ResourceSpec, Vec<BooleanGuard>) {
        (self.resource, self.guards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::{AdmissibleTransition, LogicalResourceId, ResourceClassId, ResourceSpec};
    use crate::{PredicateKey, PredicateRegistry};

    fn registry() -> PredicateRegistry {
        PredicateRegistry::from_keys([
            PredicateKey::new("elastic.test", "a").unwrap(),
            PredicateKey::new("elastic.test", "b").unwrap(),
        ])
        .unwrap()
    }

    fn resource() -> ResourceSpec {
        ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("guarded-memory").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap()
    }

    fn guard(scope: GuardScope) -> BooleanGuard {
        let registry = registry();
        let a = registry
            .id(&PredicateKey::new("elastic.test", "a").unwrap())
            .unwrap();
        BooleanGuard::new(scope, registry, BoolExpr::atom(a)).unwrap()
    }

    #[test]
    fn construction_canonicalizes_expression_and_binds_fingerprint() {
        let registry = registry();
        let a = registry
            .id(&PredicateKey::new("elastic.test", "a").unwrap())
            .unwrap();
        let b = registry
            .id(&PredicateKey::new("elastic.test", "b").unwrap())
            .unwrap();
        let guard = BooleanGuard::new(
            GuardScope::Resource,
            registry.clone(),
            BoolExpr::all([BoolExpr::atom(b), BoolExpr::atom(a), BoolExpr::atom(a)]),
        )
        .unwrap();

        assert_eq!(
            guard.expression(),
            &BoolExpr::all([BoolExpr::atom(a), BoolExpr::atom(b)])
        );
        assert_eq!(
            guard.fingerprint(),
            guard.expression().canonical_fingerprint(&registry).unwrap()
        );
    }

    #[test]
    fn unregistered_atom_is_rejected() {
        let registry = PredicateRegistry::empty();
        assert_eq!(
            BooleanGuard::new(
                GuardScope::Resource,
                registry,
                BoolExpr::atom(crate::PredicateId::new(0)),
            ),
            Err(CanonicalizationError::UnregisteredPredicate {
                id: crate::PredicateId::new(0)
            })
        );
    }

    #[test]
    fn stable_fact_source_preserves_unknown_and_helpers_share_semantics() {
        let registry = registry();
        let key = PredicateKey::new("elastic.test", "a").unwrap();
        let id = registry.id(&key).unwrap();
        let required = BooleanGuard::requires(GuardScope::Resource, registry.clone(), id).unwrap();
        let forbidden = BooleanGuard::forbids(GuardScope::Resource, registry, id).unwrap();
        let mut facts = BTreeMap::new();

        assert_eq!(required.evaluate(&facts).unwrap(), TruthValue::Unknown);
        assert_eq!(forbidden.evaluate(&facts).unwrap(), TruthValue::Unknown);

        facts.insert(key.clone(), TruthValue::True);
        assert_eq!(required.evaluate(&facts).unwrap(), TruthValue::True);
        assert_eq!(forbidden.evaluate(&facts).unwrap(), TruthValue::False);

        facts.insert(key, TruthValue::False);
        assert_eq!(required.evaluate(&facts).unwrap(), TruthValue::False);
        assert_eq!(forbidden.evaluate(&facts).unwrap(), TruthValue::True);
    }

    #[test]
    fn transition_guard_is_only_a_typed_wrapper() {
        let registry = registry();
        let id = registry
            .id(&PredicateKey::new("elastic.test", "a").unwrap())
            .unwrap();
        let guard = TransitionGuard::requires(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
            registry,
            id,
        )
        .unwrap();
        assert_eq!(
            guard.as_guard().scope(),
            &GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CAPACITY,
            }
        );
    }

    #[test]
    fn binding_sorts_scopes_and_rejects_duplicates() {
        let transition = GuardScope::Transition {
            mechanism: TransitionMechanism::Reinterpret,
            dimension: DimensionId::CAPACITY,
        };
        let guarded = GuardedResourceSpec::new(
            resource(),
            vec![guard(transition.clone()), guard(GuardScope::Resource)],
        )
        .unwrap();
        assert_eq!(guarded.guards()[0].scope(), &GuardScope::Resource);
        assert_eq!(guarded.guards()[1].scope(), &transition);

        assert_eq!(
            GuardedResourceSpec::new(
                resource(),
                vec![guard(GuardScope::Resource), guard(GuardScope::Resource)],
            ),
            Err(GuardBindingError::DuplicateScope {
                scope: GuardScope::Resource
            })
        );
    }

    #[test]
    fn binding_rejects_non_elastic_dimension_and_unadmitted_transition() {
        let dimension_scope = GuardScope::Dimension(DimensionId::ENERGY);
        assert_eq!(
            GuardedResourceSpec::new(resource(), vec![guard(dimension_scope.clone())]),
            Err(GuardBindingError::NonElasticDimension {
                scope: dimension_scope
            })
        );

        let transition_scope = GuardScope::Transition {
            mechanism: TransitionMechanism::Recompute,
            dimension: DimensionId::CAPACITY,
        };
        assert_eq!(
            GuardedResourceSpec::new(resource(), vec![guard(transition_scope.clone())]),
            Err(GuardBindingError::UnadmittedTransition {
                scope: transition_scope
            })
        );
    }
}
