//! Boolean decision primitives for fail-closed elastic policy evaluation.
//!
//! Elastic resources remain numerically observed and optimized. This module
//! provides the compact logical layer used to decide whether a candidate is
//! admissible before more expensive ranking or actuation work is attempted.
//! Missing evidence is represented explicitly as [`TruthValue::Unknown`].

use std::fmt;

/// Maximum number of predicates represented by the dependency-free fast path.
pub const FAST_PREDICATE_CAPACITY: u32 = u64::BITS;

/// Maximum recursive expression depth accepted by the generic evaluator.
pub const MAX_BOOLEAN_EXPR_DEPTH: usize = 64;

/// Three-valued truth used by fail-closed policy evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TruthValue {
    /// The proposition is established by available evidence.
    True,
    /// The proposition is disproved by available evidence.
    False,
    /// Available evidence does not establish either truth value.
    Unknown,
}

impl TruthValue {
    /// Kleene negation.
    #[must_use]
    pub const fn negated(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }

    /// Strong Kleene conjunction.
    #[must_use]
    pub const fn kleene_and(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }

    /// Strong Kleene disjunction.
    #[must_use]
    pub const fn kleene_or(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }

    /// Three-valued exclusive-or.
    #[must_use]
    pub const fn kleene_xor(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::True, Self::False) | (Self::False, Self::True) => Self::True,
            _ => Self::False,
        }
    }

    /// Material implication `!self || rhs` under strong Kleene semantics.
    #[must_use]
    pub const fn implies(self, rhs: Self) -> Self {
        self.negated().kleene_or(rhs)
    }
}

/// Stable identifier for one Boolean predicate in a compiled decision context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PredicateId(u32);

impl PredicateId {
    /// Construct an identifier from its canonical zero-based index.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Canonical zero-based index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Errors produced by bounded Boolean policy primitives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicError {
    /// A predicate cannot be represented by the current `u64` fast path.
    PredicateOutOfRange { id: PredicateId },
    /// An expression exceeded the bounded recursive evaluator depth.
    ExpressionTooDeep { max_depth: usize },
}

impl fmt::Display for LogicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PredicateOutOfRange { id } => write!(
                f,
                "predicate {} exceeds fast-path capacity {}",
                id.index(),
                FAST_PREDICATE_CAPACITY
            ),
            Self::ExpressionTooDeep { max_depth } => {
                write!(f, "Boolean expression exceeds maximum depth {max_depth}")
            }
        }
    }
}

impl std::error::Error for LogicError {}

/// Compact set of predicate bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FactMask(u64);

impl FactMask {
    /// Empty mask.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Raw bits for diagnostics, serialization adapters, or benchmarking.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Insert one predicate bit.
    pub fn insert(&mut self, id: PredicateId) -> Result<(), LogicError> {
        self.0 |= predicate_bit(id)?;
        Ok(())
    }

    /// Remove one predicate bit.
    pub fn remove(&mut self, id: PredicateId) -> Result<(), LogicError> {
        self.0 &= !predicate_bit(id)?;
        Ok(())
    }

    /// Whether the mask contains one predicate.
    pub fn contains(self, id: PredicateId) -> Result<bool, LogicError> {
        Ok(self.0 & predicate_bit(id)? != 0)
    }

    /// Whether every bit in `required` is present.
    #[must_use]
    pub const fn contains_all(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    /// Whether the two masks share at least one bit.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

/// A contradiction-free collection of known true and known false predicates.
///
/// A predicate absent from both masks is [`TruthValue::Unknown`]. Setting a
/// value always clears its previous value first, so the public API cannot
/// create a true/false contradiction for one predicate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FactSet {
    known_true: FactMask,
    known_false: FactMask,
}

impl FactSet {
    /// Empty set: every predicate is unknown.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            known_true: FactMask::empty(),
            known_false: FactMask::empty(),
        }
    }

    /// Set one predicate to a three-valued result.
    pub fn set(&mut self, id: PredicateId, value: TruthValue) -> Result<(), LogicError> {
        self.known_true.remove(id)?;
        self.known_false.remove(id)?;
        match value {
            TruthValue::True => self.known_true.insert(id)?,
            TruthValue::False => self.known_false.insert(id)?,
            TruthValue::Unknown => {}
        }
        Ok(())
    }

    /// Builder-style form of [`FactSet::set`].
    pub fn with(mut self, id: PredicateId, value: TruthValue) -> Result<Self, LogicError> {
        self.set(id, value)?;
        Ok(self)
    }

    /// Read one predicate value.
    pub fn get(self, id: PredicateId) -> Result<TruthValue, LogicError> {
        if self.known_true.contains(id)? {
            Ok(TruthValue::True)
        } else if self.known_false.contains(id)? {
            Ok(TruthValue::False)
        } else {
            Ok(TruthValue::Unknown)
        }
    }

    /// Predicates currently known true.
    #[must_use]
    pub const fn known_true(self) -> FactMask {
        self.known_true
    }

    /// Predicates currently known false.
    #[must_use]
    pub const fn known_false(self) -> FactMask {
        self.known_false
    }
}

/// Bounded Boolean expression over typed predicate identifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoolExpr {
    /// Constant truth value.
    Const(bool),
    /// One predicate atom.
    Atom(PredicateId),
    /// Logical negation.
    Not(Box<Self>),
    /// Conjunction. The empty conjunction evaluates to true.
    All(Vec<Self>),
    /// Disjunction. The empty disjunction evaluates to false.
    Any(Vec<Self>),
    /// Exclusive-or.
    Xor(Box<Self>, Box<Self>),
    /// Material implication.
    Implies(Box<Self>, Box<Self>),
}

impl BoolExpr {
    /// Construct an atom.
    #[must_use]
    pub const fn atom(id: PredicateId) -> Self {
        Self::Atom(id)
    }

    /// Construct a negation.
    #[must_use]
    pub fn negate(expr: Self) -> Self {
        Self::Not(Box::new(expr))
    }

    /// Construct a conjunction.
    #[must_use]
    pub fn all(expressions: impl IntoIterator<Item = Self>) -> Self {
        Self::All(expressions.into_iter().collect())
    }

    /// Construct a disjunction.
    #[must_use]
    pub fn any(expressions: impl IntoIterator<Item = Self>) -> Self {
        Self::Any(expressions.into_iter().collect())
    }

    /// Evaluate under bounded strong-Kleene semantics.
    pub fn evaluate(&self, facts: &FactSet) -> Result<TruthValue, LogicError> {
        self.evaluate_at_depth(facts, 0)
    }

    fn evaluate_at_depth(&self, facts: &FactSet, depth: usize) -> Result<TruthValue, LogicError> {
        if depth > MAX_BOOLEAN_EXPR_DEPTH {
            return Err(LogicError::ExpressionTooDeep {
                max_depth: MAX_BOOLEAN_EXPR_DEPTH,
            });
        }

        match self {
            Self::Const(value) => Ok(if *value {
                TruthValue::True
            } else {
                TruthValue::False
            }),
            Self::Atom(id) => facts.get(*id),
            Self::Not(expr) => Ok(expr.evaluate_at_depth(facts, depth + 1)?.negated()),
            Self::All(expressions) => {
                let mut result = TruthValue::True;
                for expression in expressions {
                    result = result.kleene_and(expression.evaluate_at_depth(facts, depth + 1)?);
                    if result == TruthValue::False {
                        break;
                    }
                }
                Ok(result)
            }
            Self::Any(expressions) => {
                let mut result = TruthValue::False;
                for expression in expressions {
                    result = result.kleene_or(expression.evaluate_at_depth(facts, depth + 1)?);
                    if result == TruthValue::True {
                        break;
                    }
                }
                Ok(result)
            }
            Self::Xor(lhs, rhs) => Ok(lhs
                .evaluate_at_depth(facts, depth + 1)?
                .kleene_xor(rhs.evaluate_at_depth(facts, depth + 1)?)),
            Self::Implies(lhs, rhs) => Ok(lhs
                .evaluate_at_depth(facts, depth + 1)?
                .implies(rhs.evaluate_at_depth(facts, depth + 1)?)),
        }
    }

    fn validate_predicates(&self, depth: usize) -> Result<(), LogicError> {
        if depth > MAX_BOOLEAN_EXPR_DEPTH {
            return Err(LogicError::ExpressionTooDeep {
                max_depth: MAX_BOOLEAN_EXPR_DEPTH,
            });
        }
        match self {
            Self::Const(_) => Ok(()),
            Self::Atom(id) => {
                let _ = predicate_bit(*id)?;
                Ok(())
            }
            Self::Not(expr) => expr.validate_predicates(depth + 1),
            Self::All(expressions) | Self::Any(expressions) => expressions
                .iter()
                .try_for_each(|expr| expr.validate_predicates(depth + 1)),
            Self::Xor(lhs, rhs) | Self::Implies(lhs, rhs) => {
                lhs.validate_predicates(depth + 1)?;
                rhs.validate_predicates(depth + 1)
            }
        }
    }
}

/// Compiled Boolean guard with a `u64` mask fast path for simple conjunctions.
///
/// Expressions that cannot be represented as a conjunction of positive atoms,
/// negated atoms, and constants remain valid and use the bounded generic
/// evaluator. This keeps optimization separate from semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledGuard {
    required_true: FactMask,
    required_false: FactMask,
    fallback: Option<BoolExpr>,
    constant_false: bool,
}

impl CompiledGuard {
    /// Compile an expression without changing its three-valued truth semantics.
    pub fn compile(expression: &BoolExpr) -> Result<Self, LogicError> {
        expression.validate_predicates(0)?;

        let mut required_true = FactMask::empty();
        let mut required_false = FactMask::empty();
        let mut constant_false = false;
        let fast_path = collect_conjunction(
            expression,
            &mut required_true,
            &mut required_false,
            &mut constant_false,
        )?;

        if fast_path {
            Ok(Self {
                required_true,
                required_false,
                fallback: None,
                constant_false,
            })
        } else {
            Ok(Self {
                required_true: FactMask::empty(),
                required_false: FactMask::empty(),
                fallback: Some(expression.clone()),
                constant_false: false,
            })
        }
    }

    /// Evaluate the compiled guard with semantics identical to [`BoolExpr`].
    pub fn evaluate(&self, facts: &FactSet) -> Result<TruthValue, LogicError> {
        if self.constant_false {
            return Ok(TruthValue::False);
        }
        if let Some(expression) = &self.fallback {
            return expression.evaluate(facts);
        }

        if facts.known_false().intersects(self.required_true)
            || facts.known_true().intersects(self.required_false)
        {
            return Ok(TruthValue::False);
        }

        if facts.known_true().contains_all(self.required_true)
            && facts.known_false().contains_all(self.required_false)
        {
            Ok(TruthValue::True)
        } else {
            Ok(TruthValue::Unknown)
        }
    }

    /// Whether the expression was compiled to the mask fast path.
    #[must_use]
    pub const fn uses_mask_fast_path(&self) -> bool {
        self.fallback.is_none()
    }

    /// Positive predicate requirements for the mask fast path.
    #[must_use]
    pub const fn required_true(&self) -> FactMask {
        self.required_true
    }

    /// Negative predicate requirements for the mask fast path.
    #[must_use]
    pub const fn required_false(&self) -> FactMask {
        self.required_false
    }

    /// Whether this fast-path conjunction can never evaluate to `True`.
    ///
    /// Opposing requirements such as `A && !A` are unsatisfiable as an
    /// eligibility condition, but still evaluate to [`TruthValue::Unknown`]
    /// while `A` itself is unknown. This method reports the structural
    /// impossibility of a `True` result without collapsing `Unknown` to
    /// `False`.
    #[must_use]
    pub const fn is_contradictory(&self) -> bool {
        self.constant_false || self.required_true.intersects(self.required_false)
    }
}

fn predicate_bit(id: PredicateId) -> Result<u64, LogicError> {
    if id.index() >= FAST_PREDICATE_CAPACITY {
        return Err(LogicError::PredicateOutOfRange { id });
    }
    Ok(1_u64 << id.index())
}

fn collect_conjunction(
    expression: &BoolExpr,
    required_true: &mut FactMask,
    required_false: &mut FactMask,
    constant_false: &mut bool,
) -> Result<bool, LogicError> {
    match expression {
        BoolExpr::Const(true) => Ok(true),
        BoolExpr::Const(false) => {
            *constant_false = true;
            Ok(true)
        }
        BoolExpr::Atom(id) => {
            required_true.insert(*id)?;
            Ok(true)
        }
        BoolExpr::Not(inner) => match inner.as_ref() {
            BoolExpr::Atom(id) => {
                required_false.insert(*id)?;
                Ok(true)
            }
            _ => Ok(false),
        },
        BoolExpr::All(expressions) => {
            for expression in expressions {
                if !collect_conjunction(
                    expression,
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

#[cfg(test)]
mod tests {
    use super::*;

    const A: PredicateId = PredicateId::new(0);
    const B: PredicateId = PredicateId::new(1);
    const VALUES: [TruthValue; 3] = [TruthValue::True, TruthValue::False, TruthValue::Unknown];

    #[test]
    fn strong_kleene_truth_tables_preserve_unknown() {
        assert_eq!(TruthValue::Unknown.negated(), TruthValue::Unknown);
        assert_eq!(
            TruthValue::True.kleene_and(TruthValue::Unknown),
            TruthValue::Unknown
        );
        assert_eq!(
            TruthValue::False.kleene_and(TruthValue::Unknown),
            TruthValue::False
        );
        assert_eq!(
            TruthValue::True.kleene_or(TruthValue::Unknown),
            TruthValue::True
        );
        assert_eq!(
            TruthValue::False.kleene_or(TruthValue::Unknown),
            TruthValue::Unknown
        );
    }

    #[test]
    fn fact_set_distinguishes_false_from_unknown() {
        let mut facts = FactSet::new();
        facts.set(A, TruthValue::False).unwrap();
        assert_eq!(facts.get(A).unwrap(), TruthValue::False);
        assert_eq!(facts.get(B).unwrap(), TruthValue::Unknown);

        facts.set(A, TruthValue::Unknown).unwrap();
        assert_eq!(facts.get(A).unwrap(), TruthValue::Unknown);
    }

    #[test]
    fn expression_evaluation_fails_closed_on_missing_evidence() {
        let expression = BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(B))]);
        let facts = FactSet::new().with(A, TruthValue::True).unwrap();
        assert_eq!(expression.evaluate(&facts).unwrap(), TruthValue::Unknown);
    }

    #[test]
    fn simple_conjunction_compiles_to_masks() {
        let expression = BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(B))]);
        let guard = CompiledGuard::compile(&expression).unwrap();
        assert!(guard.uses_mask_fast_path());

        let facts = FactSet::new()
            .with(A, TruthValue::True)
            .unwrap()
            .with(B, TruthValue::False)
            .unwrap();
        assert_eq!(guard.evaluate(&facts).unwrap(), TruthValue::True);
    }

    #[test]
    fn opposing_requirements_preserve_unknown_semantics() {
        let expression = BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(A))]);
        let guard = CompiledGuard::compile(&expression).unwrap();
        assert!(guard.is_contradictory());

        let unknown = FactSet::new();
        assert_eq!(expression.evaluate(&unknown).unwrap(), TruthValue::Unknown);
        assert_eq!(guard.evaluate(&unknown).unwrap(), TruthValue::Unknown);

        for value in [TruthValue::True, TruthValue::False] {
            let facts = FactSet::new().with(A, value).unwrap();
            assert_eq!(expression.evaluate(&facts).unwrap(), TruthValue::False);
            assert_eq!(guard.evaluate(&facts).unwrap(), TruthValue::False);
        }
    }

    #[test]
    fn mask_fast_path_is_exhaustively_equivalent_on_two_atoms() {
        let expressions = [
            BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B)]),
            BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(B))]),
            BoolExpr::all([BoolExpr::negate(BoolExpr::atom(A)), BoolExpr::atom(B)]),
            BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(A))]),
        ];

        for expression in expressions {
            let guard = CompiledGuard::compile(&expression).unwrap();
            assert!(guard.uses_mask_fast_path());
            for a in VALUES {
                for b in VALUES {
                    let facts = FactSet::new().with(A, a).unwrap().with(B, b).unwrap();
                    assert_eq!(
                        guard.evaluate(&facts).unwrap(),
                        expression.evaluate(&facts).unwrap(),
                        "compiled and generic semantics diverged for A={a:?}, B={b:?}, expression={expression:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn explicit_false_constant_remains_false_with_unknown_facts() {
        let expression = BoolExpr::all([BoolExpr::Const(false), BoolExpr::atom(A)]);
        let guard = CompiledGuard::compile(&expression).unwrap();
        assert!(guard.is_contradictory());
        assert_eq!(expression.evaluate(&FactSet::new()).unwrap(), TruthValue::False);
        assert_eq!(guard.evaluate(&FactSet::new()).unwrap(), TruthValue::False);
    }

    #[test]
    fn complex_expression_uses_generic_semantics() {
        let expression =
            BoolExpr::Implies(Box::new(BoolExpr::atom(A)), Box::new(BoolExpr::atom(B)));
        let guard = CompiledGuard::compile(&expression).unwrap();
        assert!(!guard.uses_mask_fast_path());

        let facts = FactSet::new()
            .with(A, TruthValue::True)
            .unwrap()
            .with(B, TruthValue::False)
            .unwrap();
        assert_eq!(guard.evaluate(&facts).unwrap(), TruthValue::False);
    }

    #[test]
    fn out_of_range_predicates_are_explicit_errors() {
        let expression = BoolExpr::atom(PredicateId::new(FAST_PREDICATE_CAPACITY));
        assert_eq!(
            CompiledGuard::compile(&expression),
            Err(LogicError::PredicateOutOfRange {
                id: PredicateId::new(FAST_PREDICATE_CAPACITY)
            })
        );
    }
}
