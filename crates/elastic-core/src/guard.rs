//! Typed Boolean guard declarations and resource binding.
//!
//! Guards are declarative data. They scope one canonical Boolean expression to
//! a resource, elastic dimension, or already-admitted transition. A guard never
//! executes an effect and never makes an undeclared transition legal. Runtime
//! interpretation is deliberately layered later in `elastic-runtime`.

use crate::resource::{DimensionId, ResourceSpec};
use crate::{
    BoolExpr, BoolExprFingerprint, CanonicalizationError, PredicateRegistry, TransitionMechanism,
};
use std::collections::BTreeSet;
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
                write!(f, "Boolean guard scope {scope} targets a non-elastic dimension")
            }
            Self::UnadmittedTransition { scope } => {
                write!(f, "Boolean guard scope {scope} targets an unadmitted transition")
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
    use crate::resource::{
        AdmissibleTransition, LogicalResourceId, ResourceClassId, ResourceSpec,
    };
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
            guard
                .expression()
                .canonical_fingerprint(&registry)
                .unwrap()
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
