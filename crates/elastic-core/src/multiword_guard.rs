//! Compiled multiword Boolean guards with exact strong-Kleene fallback.
//!
//! This module extends the existing `u64` compiled-guard optimization to the
//! bounded [`crate::MultiwordFactSet`] representation. Fast paths are purely an
//! optimization: unsupported expression shapes are evaluated by the same
//! three-valued semantics, and no result grants validation or actuation authority.

use std::fmt;

use crate::{
    BoolExpr, MultiwordFactError, MultiwordFactSet, PredicateId, TruthValue,
    MAX_BOOLEAN_EXPR_DEPTH, MAX_CANONICAL_EXPRESSION_NODES, MAX_MULTIWORD_FACT_PREDICATES,
    MULTIWORD_FACT_WORD_BITS,
};

/// Execution path selected for one compiled multiword guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiwordGuardPath {
    /// Conjunction of positive/negated atoms and constants.
    Conjunction,
    /// Disjunction of positive/negated atoms and constants.
    Disjunction,
    /// Bounded generic expression evaluation.
    Generic,
}

/// Errors produced by bounded multiword guard compilation or evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiwordGuardError {
    /// Fact storage rejected a requested capacity or predicate.
    Facts(MultiwordFactError),
    /// The expression exceeded the shared recursive depth bound.
    ExpressionTooDeep { max_depth: usize },
    /// The expression exceeded the shared canonical node bound.
    ExpressionTooLarge { max_nodes: usize },
    /// Evaluation facts do not cover the compiled guard's declared domain.
    FactCapacityTooSmall { required: u32, actual: u32 },
}

impl fmt::Display for MultiwordGuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Facts(error) => error.fmt(f),
            Self::ExpressionTooDeep { max_depth } => {
                write!(f, "Boolean expression exceeds maximum depth {max_depth}")
            }
            Self::ExpressionTooLarge { max_nodes } => {
                write!(
                    f,
                    "Boolean expression exceeds maximum node count {max_nodes}"
                )
            }
            Self::FactCapacityTooSmall { required, actual } => write!(
                f,
                "multiword facts cover {actual} predicates but guard requires {required}"
            ),
        }
    }
}

impl std::error::Error for MultiwordGuardError {}

impl From<MultiwordFactError> for MultiwordGuardError {
    fn from(value: MultiwordFactError) -> Self {
        Self::Facts(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MultiwordFastPath {
    Conjunction {
        required_true: Vec<u64>,
        required_false: Vec<u64>,
        constant_false: bool,
    },
    Disjunction {
        sufficient_true: Vec<u64>,
        sufficient_false: Vec<u64>,
        constant_true: bool,
    },
}

/// Bounded compiled guard for [`MultiwordFactSet`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiwordCompiledGuard {
    predicate_capacity: u32,
    fast_path: Option<MultiwordFastPath>,
    fallback: Option<BoolExpr>,
}

impl MultiwordCompiledGuard {
    /// Compile one expression for a declared bounded predicate domain.
    ///
    /// Conjunctions and disjunctions of atoms, negated atoms, and constants use
    /// word-parallel masks. Other shapes retain a bounded generic fallback with
    /// identical strong-Kleene semantics.
    pub fn compile(
        expression: &BoolExpr,
        predicate_capacity: u32,
    ) -> Result<Self, MultiwordGuardError> {
        if predicate_capacity > MAX_MULTIWORD_FACT_PREDICATES {
            return Err(MultiwordFactError::CapacityTooLarge {
                max: MAX_MULTIWORD_FACT_PREDICATES,
                actual: predicate_capacity,
            }
            .into());
        }
        let mut nodes = 0;
        validate_expression(expression, predicate_capacity, 0, &mut nodes)?;
        let words = word_count(predicate_capacity);

        let mut required_true = vec![0; words];
        let mut required_false = vec![0; words];
        let mut constant_false = false;
        if collect_conjunction(
            expression,
            predicate_capacity,
            &mut required_true,
            &mut required_false,
            &mut constant_false,
        )? {
            return Ok(Self {
                predicate_capacity,
                fast_path: Some(MultiwordFastPath::Conjunction {
                    required_true,
                    required_false,
                    constant_false,
                }),
                fallback: None,
            });
        }

        let mut sufficient_true = vec![0; words];
        let mut sufficient_false = vec![0; words];
        let mut constant_true = false;
        if collect_disjunction(
            expression,
            predicate_capacity,
            &mut sufficient_true,
            &mut sufficient_false,
            &mut constant_true,
        )? {
            return Ok(Self {
                predicate_capacity,
                fast_path: Some(MultiwordFastPath::Disjunction {
                    sufficient_true,
                    sufficient_false,
                    constant_true,
                }),
                fallback: None,
            });
        }

        Ok(Self {
            predicate_capacity,
            fast_path: None,
            fallback: Some(expression.clone()),
        })
    }

    /// Evaluate with semantics identical to the generic strong-Kleene AST.
    pub fn evaluate(&self, facts: &MultiwordFactSet) -> Result<TruthValue, MultiwordGuardError> {
        if facts.predicate_capacity() < self.predicate_capacity {
            return Err(MultiwordGuardError::FactCapacityTooSmall {
                required: self.predicate_capacity,
                actual: facts.predicate_capacity(),
            });
        }
        if let Some(fallback) = &self.fallback {
            return evaluate_expression(fallback, facts, 0);
        }
        match self
            .fast_path
            .as_ref()
            .expect("compiled guard has one path")
        {
            MultiwordFastPath::Conjunction {
                required_true,
                required_false,
                constant_false,
            } => Ok(evaluate_conjunction(
                facts,
                required_true,
                required_false,
                *constant_false,
            )),
            MultiwordFastPath::Disjunction {
                sufficient_true,
                sufficient_false,
                constant_true,
            } => Ok(evaluate_disjunction(
                facts,
                sufficient_true,
                sufficient_false,
                *constant_true,
            )),
        }
    }

    /// Selected execution path for diagnostics and later portable benchmarking.
    #[must_use]
    pub const fn path(&self) -> MultiwordGuardPath {
        match (&self.fast_path, &self.fallback) {
            (Some(MultiwordFastPath::Conjunction { .. }), None) => MultiwordGuardPath::Conjunction,
            (Some(MultiwordFastPath::Disjunction { .. }), None) => MultiwordGuardPath::Disjunction,
            _ => MultiwordGuardPath::Generic,
        }
    }

    /// Predicate capacity declared at compile time.
    #[must_use]
    pub const fn predicate_capacity(&self) -> u32 {
        self.predicate_capacity
    }
}

fn word_count(predicate_capacity: u32) -> usize {
    (predicate_capacity.div_ceil(MULTIWORD_FACT_WORD_BITS)) as usize
}

fn location(id: PredicateId, capacity: u32) -> Result<(usize, u32), MultiwordGuardError> {
    if id.index() >= capacity {
        return Err(MultiwordFactError::PredicateOutOfRange { id, capacity }.into());
    }
    Ok((
        (id.index() / MULTIWORD_FACT_WORD_BITS) as usize,
        id.index() % MULTIWORD_FACT_WORD_BITS,
    ))
}

fn insert(mask: &mut [u64], id: PredicateId, capacity: u32) -> Result<(), MultiwordGuardError> {
    let (word, bit) = location(id, capacity)?;
    mask[word] |= 1_u64 << bit;
    Ok(())
}

fn validate_expression(
    expression: &BoolExpr,
    capacity: u32,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), MultiwordGuardError> {
    if depth > MAX_BOOLEAN_EXPR_DEPTH {
        return Err(MultiwordGuardError::ExpressionTooDeep {
            max_depth: MAX_BOOLEAN_EXPR_DEPTH,
        });
    }
    *nodes = nodes
        .checked_add(1)
        .ok_or(MultiwordGuardError::ExpressionTooLarge {
            max_nodes: MAX_CANONICAL_EXPRESSION_NODES,
        })?;
    if *nodes > MAX_CANONICAL_EXPRESSION_NODES {
        return Err(MultiwordGuardError::ExpressionTooLarge {
            max_nodes: MAX_CANONICAL_EXPRESSION_NODES,
        });
    }
    match expression {
        BoolExpr::Const(_) => Ok(()),
        BoolExpr::Atom(id) => location(*id, capacity).map(|_| ()),
        BoolExpr::Not(inner) => validate_expression(inner, capacity, depth + 1, nodes),
        BoolExpr::All(expressions) | BoolExpr::Any(expressions) => expressions
            .iter()
            .try_for_each(|item| validate_expression(item, capacity, depth + 1, nodes)),
        BoolExpr::Xor(lhs, rhs) | BoolExpr::Implies(lhs, rhs) => {
            validate_expression(lhs, capacity, depth + 1, nodes)?;
            validate_expression(rhs, capacity, depth + 1, nodes)
        }
    }
}

fn collect_conjunction(
    expression: &BoolExpr,
    capacity: u32,
    required_true: &mut [u64],
    required_false: &mut [u64],
    constant_false: &mut bool,
) -> Result<bool, MultiwordGuardError> {
    match expression {
        BoolExpr::Const(true) => Ok(true),
        BoolExpr::Const(false) => {
            *constant_false = true;
            Ok(true)
        }
        BoolExpr::Atom(id) => {
            insert(required_true, *id, capacity)?;
            Ok(true)
        }
        BoolExpr::Not(inner) => match inner.as_ref() {
            BoolExpr::Atom(id) => {
                insert(required_false, *id, capacity)?;
                Ok(true)
            }
            _ => Ok(false),
        },
        BoolExpr::All(items) => {
            for item in items {
                if !collect_conjunction(
                    item,
                    capacity,
                    required_true,
                    required_false,
                    constant_false,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        BoolExpr::Any(_) | BoolExpr::Xor(_, _) | BoolExpr::Implies(_, _) => Ok(false),
    }
}

fn collect_disjunction(
    expression: &BoolExpr,
    capacity: u32,
    sufficient_true: &mut [u64],
    sufficient_false: &mut [u64],
    constant_true: &mut bool,
) -> Result<bool, MultiwordGuardError> {
    match expression {
        BoolExpr::Const(false) => Ok(true),
        BoolExpr::Const(true) => {
            *constant_true = true;
            Ok(true)
        }
        BoolExpr::Atom(id) => {
            insert(sufficient_true, *id, capacity)?;
            Ok(true)
        }
        BoolExpr::Not(inner) => match inner.as_ref() {
            BoolExpr::Atom(id) => {
                insert(sufficient_false, *id, capacity)?;
                Ok(true)
            }
            _ => Ok(false),
        },
        BoolExpr::Any(items) => {
            for item in items {
                if !collect_disjunction(
                    item,
                    capacity,
                    sufficient_true,
                    sufficient_false,
                    constant_true,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        BoolExpr::All(_) | BoolExpr::Xor(_, _) | BoolExpr::Implies(_, _) => Ok(false),
    }
}

fn evaluate_conjunction(
    facts: &MultiwordFactSet,
    required_true: &[u64],
    required_false: &[u64],
    constant_false: bool,
) -> TruthValue {
    if constant_false {
        return TruthValue::False;
    }
    for (word, (&need_true, &need_false)) in required_true.iter().zip(required_false).enumerate() {
        let known_true = facts.known_true_words()[word];
        let known_false = facts.known_false_words()[word];
        if known_false & need_true != 0 || known_true & need_false != 0 {
            return TruthValue::False;
        }
    }
    let complete = required_true.iter().zip(required_false).enumerate().all(
        |(word, (&need_true, &need_false))| {
            facts.known_true_words()[word] & need_true == need_true
                && facts.known_false_words()[word] & need_false == need_false
        },
    );
    if complete {
        TruthValue::True
    } else {
        TruthValue::Unknown
    }
}

fn evaluate_disjunction(
    facts: &MultiwordFactSet,
    sufficient_true: &[u64],
    sufficient_false: &[u64],
    constant_true: bool,
) -> TruthValue {
    if constant_true {
        return TruthValue::True;
    }
    for (word, (&accept_true, &accept_false)) in
        sufficient_true.iter().zip(sufficient_false).enumerate()
    {
        let known_true = facts.known_true_words()[word];
        let known_false = facts.known_false_words()[word];
        if known_true & accept_true != 0 || known_false & accept_false != 0 {
            return TruthValue::True;
        }
    }
    let all_false = sufficient_true
        .iter()
        .zip(sufficient_false)
        .enumerate()
        .all(|(word, (&accept_true, &accept_false))| {
            facts.known_false_words()[word] & accept_true == accept_true
                && facts.known_true_words()[word] & accept_false == accept_false
        });
    if all_false {
        TruthValue::False
    } else {
        TruthValue::Unknown
    }
}

fn evaluate_expression(
    expression: &BoolExpr,
    facts: &MultiwordFactSet,
    depth: usize,
) -> Result<TruthValue, MultiwordGuardError> {
    if depth > MAX_BOOLEAN_EXPR_DEPTH {
        return Err(MultiwordGuardError::ExpressionTooDeep {
            max_depth: MAX_BOOLEAN_EXPR_DEPTH,
        });
    }
    match expression {
        BoolExpr::Const(value) => Ok(if *value {
            TruthValue::True
        } else {
            TruthValue::False
        }),
        BoolExpr::Atom(id) => Ok(facts.get(*id)?),
        BoolExpr::Not(inner) => Ok(evaluate_expression(inner, facts, depth + 1)?.negated()),
        BoolExpr::All(items) => {
            let mut result = TruthValue::True;
            for item in items {
                result = result.kleene_and(evaluate_expression(item, facts, depth + 1)?);
                if result == TruthValue::False {
                    break;
                }
            }
            Ok(result)
        }
        BoolExpr::Any(items) => {
            let mut result = TruthValue::False;
            for item in items {
                result = result.kleene_or(evaluate_expression(item, facts, depth + 1)?);
                if result == TruthValue::True {
                    break;
                }
            }
            Ok(result)
        }
        BoolExpr::Xor(lhs, rhs) => Ok(evaluate_expression(lhs, facts, depth + 1)?
            .kleene_xor(evaluate_expression(rhs, facts, depth + 1)?)),
        BoolExpr::Implies(lhs, rhs) => Ok(evaluate_expression(lhs, facts, depth + 1)?
            .implies(evaluate_expression(rhs, facts, depth + 1)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: PredicateId = PredicateId::new(1);
    const B: PredicateId = PredicateId::new(64);
    const C: PredicateId = PredicateId::new(129);
    const VALUES: [TruthValue; 3] = [TruthValue::True, TruthValue::False, TruthValue::Unknown];

    fn facts(values: &[(PredicateId, TruthValue)]) -> MultiwordFactSet {
        let mut facts = MultiwordFactSet::new(130).unwrap();
        for (id, value) in values {
            facts.set(*id, *value).unwrap();
        }
        facts
    }

    #[test]
    fn conjunction_fast_path_spans_multiple_words() {
        let expression = BoolExpr::all([
            BoolExpr::atom(A),
            BoolExpr::atom(B),
            BoolExpr::negate(BoolExpr::atom(C)),
        ]);
        let guard = MultiwordCompiledGuard::compile(&expression, 130).unwrap();
        assert_eq!(guard.path(), MultiwordGuardPath::Conjunction);
        assert_eq!(
            guard
                .evaluate(&facts(&[
                    (A, TruthValue::True),
                    (B, TruthValue::True),
                    (C, TruthValue::False),
                ]))
                .unwrap(),
            TruthValue::True
        );
        assert_eq!(
            guard
                .evaluate(&facts(&[(A, TruthValue::True), (B, TruthValue::True)]))
                .unwrap(),
            TruthValue::Unknown
        );
    }

    #[test]
    fn disjunction_fast_path_spans_multiple_words() {
        let expression = BoolExpr::any([
            BoolExpr::atom(A),
            BoolExpr::atom(B),
            BoolExpr::negate(BoolExpr::atom(C)),
        ]);
        let guard = MultiwordCompiledGuard::compile(&expression, 130).unwrap();
        assert_eq!(guard.path(), MultiwordGuardPath::Disjunction);
        assert_eq!(
            guard
                .evaluate(&facts(&[
                    (A, TruthValue::False),
                    (B, TruthValue::False),
                    (C, TruthValue::True),
                ]))
                .unwrap(),
            TruthValue::False
        );
        assert_eq!(
            guard
                .evaluate(&facts(&[(A, TruthValue::False), (B, TruthValue::True)]))
                .unwrap(),
            TruthValue::True
        );
    }

    #[test]
    fn generic_fallback_handles_non_mask_shapes_beyond_u64() {
        let expression = BoolExpr::Xor(Box::new(BoolExpr::atom(B)), Box::new(BoolExpr::atom(C)));
        let guard = MultiwordCompiledGuard::compile(&expression, 130).unwrap();
        assert_eq!(guard.path(), MultiwordGuardPath::Generic);
        assert_eq!(
            guard
                .evaluate(&facts(&[(B, TruthValue::True), (C, TruthValue::False)]))
                .unwrap(),
            TruthValue::True
        );
        assert_eq!(
            guard.evaluate(&facts(&[(B, TruthValue::True)])).unwrap(),
            TruthValue::Unknown
        );
    }

    #[test]
    fn fast_and_generic_paths_match_strong_kleene_reference() {
        let expressions = [
            BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(B))]),
            BoolExpr::any([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(B))]),
            BoolExpr::Implies(Box::new(BoolExpr::atom(A)), Box::new(BoolExpr::atom(B))),
        ];
        for expression in expressions {
            let guard = MultiwordCompiledGuard::compile(&expression, 130).unwrap();
            for a in VALUES {
                for b in VALUES {
                    let facts = facts(&[(A, a), (B, b)]);
                    let reference = evaluate_expression(&expression, &facts, 0).unwrap();
                    assert_eq!(guard.evaluate(&facts).unwrap(), reference);
                }
            }
        }
    }

    #[test]
    fn opposite_literals_preserve_unknown_semantics() {
        let conjunction = BoolExpr::all([BoolExpr::atom(B), BoolExpr::negate(BoolExpr::atom(B))]);
        let disjunction = BoolExpr::any([BoolExpr::atom(B), BoolExpr::negate(BoolExpr::atom(B))]);
        let conjunction = MultiwordCompiledGuard::compile(&conjunction, 130).unwrap();
        let disjunction = MultiwordCompiledGuard::compile(&disjunction, 130).unwrap();
        let unknown = facts(&[]);
        assert_eq!(conjunction.evaluate(&unknown).unwrap(), TruthValue::Unknown);
        assert_eq!(disjunction.evaluate(&unknown).unwrap(), TruthValue::Unknown);
        for value in [TruthValue::True, TruthValue::False] {
            let assigned = facts(&[(B, value)]);
            assert_eq!(conjunction.evaluate(&assigned).unwrap(), TruthValue::False);
            assert_eq!(disjunction.evaluate(&assigned).unwrap(), TruthValue::True);
        }
    }

    #[test]
    fn declared_capacity_is_enforced_at_compile_and_evaluate_time() {
        let expression = BoolExpr::atom(C);
        assert_eq!(
            MultiwordCompiledGuard::compile(&expression, 129),
            Err(MultiwordGuardError::Facts(
                MultiwordFactError::PredicateOutOfRange {
                    id: C,
                    capacity: 129,
                }
            ))
        );
        let guard = MultiwordCompiledGuard::compile(&expression, 130).unwrap();
        let too_small = MultiwordFactSet::new(129).unwrap();
        assert_eq!(
            guard.evaluate(&too_small),
            Err(MultiwordGuardError::FactCapacityTooSmall {
                required: 130,
                actual: 129,
            })
        );
    }

    #[test]
    fn empty_conjunction_and_disjunction_keep_identity_values() {
        let facts = MultiwordFactSet::new(0).unwrap();
        let all = MultiwordCompiledGuard::compile(&BoolExpr::all([]), 0).unwrap();
        let any = MultiwordCompiledGuard::compile(&BoolExpr::any([]), 0).unwrap();
        assert_eq!(all.evaluate(&facts).unwrap(), TruthValue::True);
        assert_eq!(any.evaluate(&facts).unwrap(), TruthValue::False);
    }
}
