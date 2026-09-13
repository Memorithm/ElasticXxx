//! Canonical Boolean expressions and stable structural fingerprints.
//!
//! Canonicalization intentionally applies only rewrites that preserve strong
//! Kleene three-valued semantics. Classical rewrites such as `A && !A -> false`
//! or `A xor A -> false` are deliberately forbidden because they collapse
//! [`crate::TruthValue::Unknown`].

use crate::{BoolExpr, FactSet, PredicateId, PredicateRegistry, TruthValue, MAX_BOOLEAN_EXPR_DEPTH};
use std::cmp::Ordering;
use std::fmt;

/// Schema version for canonical Boolean-expression fingerprints.
pub const BOOLEAN_EXPRESSION_SCHEMA_V1: u32 = 1;

/// Maximum number of input expression nodes accepted by canonicalization.
pub const MAX_CANONICAL_EXPRESSION_NODES: usize = 4096;

/// Canonicalization or stable-fingerprint failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalizationError {
    /// The input expression exceeded the bounded recursive depth.
    ExpressionTooDeep { max_depth: usize },
    /// The input expression exceeded the bounded node count.
    ExpressionTooLarge { max_nodes: usize },
    /// A compact atom ID could not be resolved through the supplied registry.
    UnregisteredPredicate { id: PredicateId },
}

impl fmt::Display for CanonicalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpressionTooDeep { max_depth } => {
                write!(f, "Boolean expression exceeds maximum depth {max_depth}")
            }
            Self::ExpressionTooLarge { max_nodes } => {
                write!(f, "Boolean expression exceeds maximum node count {max_nodes}")
            }
            Self::UnregisteredPredicate { id } => {
                write!(f, "predicate {} is not present in the registry", id.index())
            }
        }
    }
}

impl std::error::Error for CanonicalizationError {}

/// Stable non-cryptographic fingerprint of one canonical Boolean expression.
///
/// Atom identity is derived from stable [`crate::PredicateKey`] components,
/// never raw compact IDs. The fingerprint is suitable for deterministic EIR
/// identity, caching, traces, and same-trust-domain equality checks. It is not
/// cryptographic and must not authenticate data across trust domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoolExprFingerprint(u64);

impl BoolExprFingerprint {
    /// Raw fingerprint bits for diagnostics and internal serialization.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl fmt::Display for BoolExprFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "boolfp:{:016x}", self.0)
    }
}

impl BoolExpr {
    /// Produce a deterministic, semantics-preserving canonical expression.
    ///
    /// The canonicalizer:
    /// - recursively canonicalizes children;
    /// - flattens nested `All` / `Any` nodes;
    /// - sorts and deduplicates idempotent `All` / `Any` operands;
    /// - removes their neutral Boolean constants;
    /// - folds absorbing constants;
    /// - removes double negation;
    /// - deterministically orders `Xor` operands;
    /// - applies only constant rewrites that preserve strong Kleene semantics.
    ///
    /// # Errors
    ///
    /// Returns a bounded-shape error before normalization if the complete input
    /// exceeds the declared depth or node limits.
    pub fn canonicalize(&self) -> Result<Self, CanonicalizationError> {
        validate_shape(self)?;
        Ok(canonicalize_validated(self))
    }

    /// Compute a stable fingerprint of the canonical expression.
    ///
    /// Compact [`PredicateId`] atoms are resolved to stable predicate keys via
    /// `registry`, so insertion order and runtime ID allocation do not leak into
    /// durable expression identity.
    pub fn canonical_fingerprint(
        &self,
        registry: &PredicateRegistry,
    ) -> Result<BoolExprFingerprint, CanonicalizationError> {
        let canonical = self.canonicalize()?;
        let mut hasher = StableHasher::new();
        hasher.tag(b'v');
        hasher.number(u64::from(BOOLEAN_EXPRESSION_SCHEMA_V1));
        fingerprint_expr(&canonical, registry, &mut hasher)?;
        Ok(BoolExprFingerprint(hasher.finish()))
    }
}

fn validate_shape(expression: &BoolExpr) -> Result<(), CanonicalizationError> {
    let mut nodes = 0_usize;
    validate_shape_at_depth(expression, 0, &mut nodes)
}

fn validate_shape_at_depth(
    expression: &BoolExpr,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), CanonicalizationError> {
    if depth > MAX_BOOLEAN_EXPR_DEPTH {
        return Err(CanonicalizationError::ExpressionTooDeep {
            max_depth: MAX_BOOLEAN_EXPR_DEPTH,
        });
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > MAX_CANONICAL_EXPRESSION_NODES {
        return Err(CanonicalizationError::ExpressionTooLarge {
            max_nodes: MAX_CANONICAL_EXPRESSION_NODES,
        });
    }

    match expression {
        BoolExpr::Const(_) | BoolExpr::Atom(_) => Ok(()),
        BoolExpr::Not(inner) => validate_shape_at_depth(inner, depth + 1, nodes),
        BoolExpr::All(expressions) | BoolExpr::Any(expressions) => expressions
            .iter()
            .try_for_each(|inner| validate_shape_at_depth(inner, depth + 1, nodes)),
        BoolExpr::Xor(lhs, rhs) | BoolExpr::Implies(lhs, rhs) => {
            validate_shape_at_depth(lhs, depth + 1, nodes)?;
            validate_shape_at_depth(rhs, depth + 1, nodes)
        }
    }
}

fn canonicalize_validated(expression: &BoolExpr) -> BoolExpr {
    match expression {
        BoolExpr::Const(value) => BoolExpr::Const(*value),
        BoolExpr::Atom(id) => BoolExpr::Atom(*id),
        BoolExpr::Not(inner) => normalize_not(canonicalize_validated(inner)),
        BoolExpr::All(expressions) => normalize_all(
            expressions
                .iter()
                .map(canonicalize_validated)
                .collect(),
        ),
        BoolExpr::Any(expressions) => normalize_any(
            expressions
                .iter()
                .map(canonicalize_validated)
                .collect(),
        ),
        BoolExpr::Xor(lhs, rhs) => normalize_xor(
            canonicalize_validated(lhs),
            canonicalize_validated(rhs),
        ),
        BoolExpr::Implies(lhs, rhs) => normalize_implies(
            canonicalize_validated(lhs),
            canonicalize_validated(rhs),
        ),
    }
}

fn normalize_not(expression: BoolExpr) -> BoolExpr {
    match expression {
        BoolExpr::Const(value) => BoolExpr::Const(!value),
        BoolExpr::Not(inner) => *inner,
        other => BoolExpr::Not(Box::new(other)),
    }
}

fn normalize_all(expressions: Vec<BoolExpr>) -> BoolExpr {
    let mut flattened = Vec::new();
    for expression in expressions {
        match expression {
            BoolExpr::Const(false) => return BoolExpr::Const(false),
            BoolExpr::Const(true) => {}
            BoolExpr::All(inner) => flattened.extend(inner),
            other => flattened.push(other),
        }
    }
    canonicalize_commutative_set(flattened, true)
}

fn normalize_any(expressions: Vec<BoolExpr>) -> BoolExpr {
    let mut flattened = Vec::new();
    for expression in expressions {
        match expression {
            BoolExpr::Const(true) => return BoolExpr::Const(true),
            BoolExpr::Const(false) => {}
            BoolExpr::Any(inner) => flattened.extend(inner),
            other => flattened.push(other),
        }
    }
    canonicalize_commutative_set(flattened, false)
}

fn canonicalize_commutative_set(mut expressions: Vec<BoolExpr>, conjunction: bool) -> BoolExpr {
    expressions.sort_by(compare_expr);
    expressions.dedup();
    match expressions.len() {
        0 if conjunction => BoolExpr::Const(true),
        0 => BoolExpr::Const(false),
        1 => expressions.pop().expect("length checked"),
        _ if conjunction => BoolExpr::All(expressions),
        _ => BoolExpr::Any(expressions),
    }
}

fn normalize_xor(mut lhs: BoolExpr, mut rhs: BoolExpr) -> BoolExpr {
    match (&lhs, &rhs) {
        (BoolExpr::Const(left), BoolExpr::Const(right)) => return BoolExpr::Const(left ^ right),
        (BoolExpr::Const(false), _) => return rhs,
        (_, BoolExpr::Const(false)) => return lhs,
        (BoolExpr::Const(true), _) => return normalize_not(rhs),
        (_, BoolExpr::Const(true)) => return normalize_not(lhs),
        _ => {}
    }
    if compare_expr(&lhs, &rhs) == Ordering::Greater {
        std::mem::swap(&mut lhs, &mut rhs);
    }
    BoolExpr::Xor(Box::new(lhs), Box::new(rhs))
}

fn normalize_implies(lhs: BoolExpr, rhs: BoolExpr) -> BoolExpr {
    match (lhs, rhs) {
        (BoolExpr::Const(false), _) => BoolExpr::Const(true),
        (BoolExpr::Const(true), rhs) => rhs,
        (_, BoolExpr::Const(true)) => BoolExpr::Const(true),
        (lhs, BoolExpr::Const(false)) => normalize_not(lhs),
        (lhs, rhs) => BoolExpr::Implies(Box::new(lhs), Box::new(rhs)),
    }
}

fn compare_expr(lhs: &BoolExpr, rhs: &BoolExpr) -> Ordering {
    let rank = |expression: &BoolExpr| match expression {
        BoolExpr::Const(_) => 0_u8,
        BoolExpr::Atom(_) => 1,
        BoolExpr::Not(_) => 2,
        BoolExpr::All(_) => 3,
        BoolExpr::Any(_) => 4,
        BoolExpr::Xor(_, _) => 5,
        BoolExpr::Implies(_, _) => 6,
    };

    rank(lhs).cmp(&rank(rhs)).then_with(|| match (lhs, rhs) {
        (BoolExpr::Const(left), BoolExpr::Const(right)) => left.cmp(right),
        (BoolExpr::Atom(left), BoolExpr::Atom(right)) => left.cmp(right),
        (BoolExpr::Not(left), BoolExpr::Not(right)) => compare_expr(left, right),
        (BoolExpr::All(left), BoolExpr::All(right)) | (BoolExpr::Any(left), BoolExpr::Any(right)) => {
            compare_expr_slices(left, right)
        }
        (BoolExpr::Xor(ll, lr), BoolExpr::Xor(rl, rr))
        | (BoolExpr::Implies(ll, lr), BoolExpr::Implies(rl, rr)) => {
            compare_expr(ll, rl).then_with(|| compare_expr(lr, rr))
        }
        _ => Ordering::Equal,
    })
}

fn compare_expr_slices(lhs: &[BoolExpr], rhs: &[BoolExpr]) -> Ordering {
    for (left, right) in lhs.iter().zip(rhs) {
        let order = compare_expr(left, right);
        if order != Ordering::Equal {
            return order;
        }
    }
    lhs.len().cmp(&rhs.len())
}

fn fingerprint_expr(
    expression: &BoolExpr,
    registry: &PredicateRegistry,
    hasher: &mut StableHasher,
) -> Result<(), CanonicalizationError> {
    match expression {
        BoolExpr::Const(value) => {
            hasher.tag(b'c');
            hasher.number(u64::from(*value));
        }
        BoolExpr::Atom(id) => {
            let key = registry
                .key(*id)
                .ok_or(CanonicalizationError::UnregisteredPredicate { id: *id })?;
            hasher.tag(b'p');
            hasher.text(key.namespace());
            hasher.text(key.name());
        }
        BoolExpr::Not(inner) => {
            hasher.tag(b'n');
            fingerprint_expr(inner, registry, hasher)?;
        }
        BoolExpr::All(expressions) => {
            hasher.tag(b'a');
            hasher.number(expressions.len() as u64);
            for inner in expressions {
                fingerprint_expr(inner, registry, hasher)?;
            }
        }
        BoolExpr::Any(expressions) => {
            hasher.tag(b'o');
            hasher.number(expressions.len() as u64);
            for inner in expressions {
                fingerprint_expr(inner, registry, hasher)?;
            }
        }
        BoolExpr::Xor(lhs, rhs) => {
            hasher.tag(b'x');
            fingerprint_expr(lhs, registry, hasher)?;
            fingerprint_expr(rhs, registry, hasher)?;
        }
        BoolExpr::Implies(lhs, rhs) => {
            hasher.tag(b'i');
            fingerprint_expr(lhs, registry, hasher)?;
            fingerprint_expr(rhs, registry, hasher)?;
        }
    }
    Ok(())
}

struct StableHasher(u64);

impl StableHasher {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    const fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn tag(&mut self, tag: u8) {
        self.absorb(tag);
    }

    fn number(&mut self, value: u64) {
        self.tag(b'#');
        for byte in value.to_le_bytes() {
            self.absorb(byte);
        }
    }

    fn text(&mut self, value: &str) {
        self.tag(b's');
        self.number(value.len() as u64);
        for byte in value.as_bytes() {
            self.absorb(*byte);
        }
    }

    fn absorb(&mut self, byte: u8) {
        self.0 ^= u64::from(byte);
        self.0 = self.0.wrapping_mul(Self::PRIME);
    }

    const fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PredicateKey, PredicateRegistry};

    const A: PredicateId = PredicateId::new(0);
    const B: PredicateId = PredicateId::new(1);
    const VALUES: [TruthValue; 3] = [TruthValue::True, TruthValue::False, TruthValue::Unknown];

    fn registry() -> PredicateRegistry {
        PredicateRegistry::from_keys([
            PredicateKey::new("elastic.test", "a").unwrap(),
            PredicateKey::new("elastic.test", "b").unwrap(),
        ])
        .unwrap()
    }

    #[test]
    fn all_is_flat_sorted_deduplicated_and_constant_folded() {
        let expression = BoolExpr::All(vec![
            BoolExpr::atom(B),
            BoolExpr::Const(true),
            BoolExpr::All(vec![BoolExpr::atom(A), BoolExpr::atom(B)]),
        ]);
        assert_eq!(
            expression.canonicalize().unwrap(),
            BoolExpr::All(vec![BoolExpr::atom(A), BoolExpr::atom(B)])
        );
    }

    #[test]
    fn canonicalization_is_permutation_invariant_and_idempotent() {
        let first = BoolExpr::any([
            BoolExpr::atom(B),
            BoolExpr::negate(BoolExpr::negate(BoolExpr::atom(A))),
            BoolExpr::Const(false),
        ]);
        let second = BoolExpr::any([BoolExpr::atom(A), BoolExpr::atom(B)]);

        let canonical = first.canonicalize().unwrap();
        assert_eq!(canonical, second.canonicalize().unwrap());
        assert_eq!(canonical.canonicalize().unwrap(), canonical);
    }

    #[test]
    fn classical_contradiction_is_not_folded_across_unknown() {
        let expression = BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(A))]);
        let canonical = expression.canonicalize().unwrap();
        assert!(!matches!(canonical, BoolExpr::Const(false)));
        assert_eq!(
            canonical.evaluate(&FactSet::new()).unwrap(),
            TruthValue::Unknown
        );
    }

    #[test]
    fn canonicalization_preserves_three_valued_semantics_exhaustively() {
        let expression = BoolExpr::All(vec![
            BoolExpr::Const(true),
            BoolExpr::atom(B),
            BoolExpr::All(vec![
                BoolExpr::atom(A),
                BoolExpr::negate(BoolExpr::negate(BoolExpr::atom(A))),
            ]),
        ]);
        let canonical = expression.canonicalize().unwrap();

        for a in VALUES {
            for b in VALUES {
                let facts = FactSet::new().with(A, a).unwrap().with(B, b).unwrap();
                assert_eq!(
                    expression.evaluate(&facts).unwrap(),
                    canonical.evaluate(&facts).unwrap(),
                    "semantic drift for A={a:?}, B={b:?}"
                );
            }
        }
    }

    #[test]
    fn fingerprint_is_stable_across_operand_permutations() {
        let registry = registry();
        let first = BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B)]);
        let second = BoolExpr::all([BoolExpr::atom(B), BoolExpr::atom(A), BoolExpr::atom(A)]);
        assert_eq!(
            first.canonical_fingerprint(&registry).unwrap(),
            second.canonical_fingerprint(&registry).unwrap()
        );
    }

    #[test]
    fn fingerprint_uses_stable_predicate_keys() {
        let key_a = PredicateKey::new("elastic.test", "a").unwrap();
        let key_b = PredicateKey::new("elastic.test", "b").unwrap();
        let forward = PredicateRegistry::from_keys([key_a.clone(), key_b.clone()]).unwrap();
        let reverse = PredicateRegistry::from_keys([key_b, key_a]).unwrap();
        let expression = BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B)]);
        assert_eq!(
            expression.canonical_fingerprint(&forward).unwrap(),
            expression.canonical_fingerprint(&reverse).unwrap()
        );
    }

    #[test]
    fn fingerprint_rejects_unregistered_atoms() {
        let expression = BoolExpr::atom(PredicateId::new(1));
        let registry = PredicateRegistry::from_keys([
            PredicateKey::new("elastic.test", "only").unwrap(),
        ])
        .unwrap();
        assert_eq!(
            expression.canonical_fingerprint(&registry),
            Err(CanonicalizationError::UnregisteredPredicate {
                id: PredicateId::new(1)
            })
        );
    }

    #[test]
    fn normalization_depth_is_bounded_before_rewrite() {
        let mut expression = BoolExpr::atom(A);
        for _ in 0..=MAX_BOOLEAN_EXPR_DEPTH {
            expression = BoolExpr::negate(expression);
        }
        assert_eq!(
            expression.canonicalize(),
            Err(CanonicalizationError::ExpressionTooDeep {
                max_depth: MAX_BOOLEAN_EXPR_DEPTH
            })
        );
    }

    #[test]
    fn normalization_node_count_is_bounded() {
        let expression = BoolExpr::All(
            std::iter::repeat_n(BoolExpr::atom(A), MAX_CANONICAL_EXPRESSION_NODES).collect(),
        );
        assert_eq!(
            expression.canonicalize(),
            Err(CanonicalizationError::ExpressionTooLarge {
                max_nodes: MAX_CANONICAL_EXPRESSION_NODES
            })
        );
    }
}
