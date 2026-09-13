//! Explicit bindings between semantic invariants and Boolean predicate facts.
//!
//! A binding is metadata only. It does not prove that an invariant holds and
//! it never replaces trusted runtime or adapter validation. Runtime layers may
//! use the associated predicate for an early fail-closed precheck, then must
//! still perform the authoritative invariant check before actuation.

use crate::{Invariant, PredicateKey};

/// Explicit association between one semantic invariant and one stable Boolean
/// predicate identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvariantPredicateBinding {
    invariant: Invariant,
    predicate: PredicateKey,
}

impl InvariantPredicateBinding {
    /// Bind `invariant` to a stable predicate key used for prechecking.
    ///
    /// Construction grants no authority: a `True` fact can only permit later
    /// validation to continue; it cannot make a plan validated by itself.
    #[must_use]
    pub const fn new(invariant: Invariant, predicate: PredicateKey) -> Self {
        Self {
            invariant,
            predicate,
        }
    }

    /// Semantic invariant represented by this precheck fact.
    #[must_use]
    pub const fn invariant(&self) -> &Invariant {
        &self.invariant
    }

    /// Stable predicate identity carrying the precheck value.
    #[must_use]
    pub const fn predicate(&self) -> &PredicateKey {
        &self.predicate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InvariantKind;

    #[test]
    fn binding_preserves_typed_invariant_and_stable_predicate_identity() {
        let invariant = Invariant::new(InvariantKind::PreserveContents);
        let predicate = PredicateKey::new("elastic.invariant", "contents-preserved").unwrap();
        let binding = InvariantPredicateBinding::new(invariant.clone(), predicate.clone());

        assert_eq!(binding.invariant(), &invariant);
        assert_eq!(binding.predicate(), &predicate);
    }
}
