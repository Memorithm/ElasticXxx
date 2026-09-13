//! Typed Boolean guard declarations.
//!
//! Guards are declarative data. They scope one canonical Boolean expression to
//! a resource, elastic dimension, or already-admitted transition. A guard never
//! executes an effect and never makes an undeclared transition legal. Runtime
//! interpretation is deliberately layered later in `elastic-runtime`.

use crate::resource::DimensionId;
use crate::{
    BoolExpr, BoolExprFingerprint, CanonicalizationError, PredicateRegistry, TransitionMechanism,
};
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
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PredicateKey, PredicateRegistry};

    fn registry() -> PredicateRegistry {
        PredicateRegistry::from_keys([
            PredicateKey::new("elastic.test", "a").unwrap(),
            PredicateKey::new("elastic.test", "b").unwrap(),
        ])
        .unwrap()
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
}
