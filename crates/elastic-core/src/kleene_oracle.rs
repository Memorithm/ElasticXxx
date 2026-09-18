//! Exact strong-Kleene analysis for bounded small policy domains.
//!
//! This oracle enumerates `False`, `True`, and `Unknown` explicitly. It is the
//! reference for runtime guard equivalence and redundancy, where classical
//! Boolean enumeration can erase meaningful `Unknown` behavior. Analysis is
//! read-only and never grants actuation authority.

use std::collections::BTreeSet;
use std::fmt;

use crate::{
    BoolExpr, ExactOracleLimits, FactSet, LogicError, PredicateId, TruthValue,
    FAST_PREDICATE_CAPACITY, MAX_BOOLEAN_EXPR_DEPTH,
};

/// Fail-closed errors from exact three-valued enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KleeneOracleError {
    /// More distinct variables were referenced than the configured limit.
    VariableLimit { variables: usize, maximum: usize },
    /// The ternary assignment space exceeds the configured budget.
    AssignmentLimit { assignments: usize, maximum: usize },
    /// A conjunction clause index was outside the input list.
    ClauseOutOfRange { index: usize, clauses: usize },
    /// The bounded Boolean evaluator rejected the expression.
    Logic(LogicError),
}
impl fmt::Display for KleeneOracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VariableLimit { variables, maximum } => write!(
                f,
                "Kleene oracle variable count {variables} exceeds configured maximum {maximum}"
            ),
            Self::AssignmentLimit {
                assignments,
                maximum,
            } => write!(
                f,
                "Kleene oracle assignment count {assignments} exceeds configured maximum {maximum}"
            ),
            Self::ClauseOutOfRange { index, clauses } => write!(
                f,
                "conjunction clause index {index} is out of range for {clauses} clauses"
            ),
            Self::Logic(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for KleeneOracleError {}

impl From<LogicError> for KleeneOracleError {
    fn from(value: LogicError) -> Self {
        Self::Logic(value)
    }
}
/// Exact satisfiability result under explicit strong-Kleene assignments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KleeneSatisfiabilityReport {
    variables: usize,
    assignment_space: usize,
    assignments_evaluated: usize,
    witness: Option<FactSet>,
}

impl KleeneSatisfiabilityReport {
    #[must_use]
    pub const fn is_satisfiable(self) -> bool {
        self.witness.is_some()
    }
    #[must_use]
    pub const fn is_unsatisfiable(self) -> bool {
        self.witness.is_none()
    }
    #[must_use]
    pub const fn witness(self) -> Option<FactSet> {
        self.witness
    }
    #[must_use]
    pub const fn variables(self) -> usize {
        self.variables
    }
    #[must_use]
    pub const fn assignment_space(self) -> usize {
        self.assignment_space
    }
    #[must_use]
    pub const fn assignments_evaluated(self) -> usize {
        self.assignments_evaluated
    }
}

/// Exact universal-property result with a three-valued counterexample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KleenePropertyReport {
    variables: usize,
    assignment_space: usize,
    assignments_evaluated: usize,
    counterexample: Option<FactSet>,
}
impl KleenePropertyReport {
    #[must_use]
    pub const fn holds(self) -> bool {
        self.counterexample.is_none()
    }
    #[must_use]
    pub const fn counterexample(self) -> Option<FactSet> {
        self.counterexample
    }
    #[must_use]
    pub const fn variables(self) -> usize {
        self.variables
    }
    #[must_use]
    pub const fn assignment_space(self) -> usize {
        self.assignment_space
    }
    #[must_use]
    pub const fn assignments_evaluated(self) -> usize {
        self.assignments_evaluated
    }
}

/// Dependency-free exact oracle over strong-Kleene assignments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactKleeneOracle {
    limits: ExactOracleLimits,
}

impl ExactKleeneOracle {
    #[must_use]
    pub const fn new(limits: ExactOracleLimits) -> Self {
        Self { limits }
    }
    #[must_use]
    pub const fn limits(self) -> ExactOracleLimits {
        self.limits
    }

    /// A guard is satisfiable only when some assignment evaluates explicitly `True`.
    pub fn satisfiability(
        self,
        expression: &BoolExpr,
    ) -> Result<KleeneSatisfiabilityReport, KleeneOracleError> {
        let domain = self.prepare(&[expression])?;
        for assignment in 0..domain.assignment_space {
            let facts = domain.fact_set(assignment)?;
            if expression.evaluate(&facts)? == TruthValue::True {
                return Ok(KleeneSatisfiabilityReport {
                    variables: domain.predicates.len(),
                    assignment_space: domain.assignment_space,
                    assignments_evaluated: assignment + 1,
                    witness: Some(facts),
                });
            }
        }
        Ok(KleeneSatisfiabilityReport {
            variables: domain.predicates.len(),
            assignment_space: domain.assignment_space,
            assignments_evaluated: domain.assignment_space,
            witness: None,
        })
    }

    /// Holds only when every three-valued assignment evaluates explicitly `True`.
    pub fn tautology(
        self,
        expression: &BoolExpr,
    ) -> Result<KleenePropertyReport, KleeneOracleError> {
        self.universal(&[expression], |facts| {
            Ok(expression.evaluate(facts)? == TruthValue::True)
        })
    }

    /// Dead-guard check: the expression can never evaluate explicitly `True`.
    pub fn contradiction(
        self,
        expression: &BoolExpr,
    ) -> Result<KleenePropertyReport, KleeneOracleError> {
        self.universal(&[expression], |facts| {
            Ok(expression.evaluate(facts)? != TruthValue::True)
        })
    }

    /// Eligibility implication: `lhs=True` always requires `rhs=True`.
    pub fn implication(
        self,
        lhs: &BoolExpr,
        rhs: &BoolExpr,
    ) -> Result<KleenePropertyReport, KleeneOracleError> {
        self.universal(&[lhs, rhs], |facts| {
            let left = lhs.evaluate(facts)?;
            let right = rhs.evaluate(facts)?;
            Ok(left != TruthValue::True || right == TruthValue::True)
        })
    }

    /// Exact semantic equivalence including `Unknown`.
    pub fn equivalence(
        self,
        lhs: &BoolExpr,
        rhs: &BoolExpr,
    ) -> Result<KleenePropertyReport, KleeneOracleError> {
        self.universal(&[lhs, rhs], |facts| {
            Ok(lhs.evaluate(facts)? == rhs.evaluate(facts)?)
        })
    }

    /// No assignment may make both expressions explicitly `True`.
    pub fn mutual_exclusion(
        self,
        lhs: &BoolExpr,
        rhs: &BoolExpr,
    ) -> Result<KleenePropertyReport, KleeneOracleError> {
        self.universal(&[lhs, rhs], |facts| {
            Ok(!(lhs.evaluate(facts)? == TruthValue::True
                && rhs.evaluate(facts)? == TruthValue::True))
        })
    }

    /// Removing the indexed clause must preserve the exact three-valued result.
    pub fn conjunction_clause_redundancy(
        self,
        clauses: &[BoolExpr],
        index: usize,
    ) -> Result<KleenePropertyReport, KleeneOracleError> {
        if index >= clauses.len() {
            return Err(KleeneOracleError::ClauseOutOfRange {
                index,
                clauses: clauses.len(),
            });
        }
        let full = BoolExpr::all(clauses.iter().cloned());
        let without = BoolExpr::all(
            clauses
                .iter()
                .enumerate()
                .filter(|(candidate, _)| *candidate != index)
                .map(|(_, expression)| expression.clone()),
        );
        self.equivalence(&full, &without)
    }
    fn universal(
        self,
        expressions: &[&BoolExpr],
        mut property: impl FnMut(&FactSet) -> Result<bool, KleeneOracleError>,
    ) -> Result<KleenePropertyReport, KleeneOracleError> {
        let domain = self.prepare(expressions)?;
        for assignment in 0..domain.assignment_space {
            let facts = domain.fact_set(assignment)?;
            if !property(&facts)? {
                return Ok(KleenePropertyReport {
                    variables: domain.predicates.len(),
                    assignment_space: domain.assignment_space,
                    assignments_evaluated: assignment + 1,
                    counterexample: Some(facts),
                });
            }
        }
        Ok(KleenePropertyReport {
            variables: domain.predicates.len(),
            assignment_space: domain.assignment_space,
            assignments_evaluated: domain.assignment_space,
            counterexample: None,
        })
    }

    fn prepare(self, expressions: &[&BoolExpr]) -> Result<KleeneDomain, KleeneOracleError> {
        let mut predicates = BTreeSet::new();
        for expression in expressions {
            collect_predicates(expression, 0, &mut predicates)?;
        }
        if predicates.len() > self.limits.max_variables() {
            return Err(KleeneOracleError::VariableLimit {
                variables: predicates.len(),
                maximum: self.limits.max_variables(),
            });
        }
        let mut assignment_space = 1usize;
        for _ in 0..predicates.len() {
            assignment_space =
                assignment_space
                    .checked_mul(3)
                    .ok_or(KleeneOracleError::AssignmentLimit {
                        assignments: usize::MAX,
                        maximum: self.limits.max_assignments(),
                    })?;
            if assignment_space > self.limits.max_assignments() {
                return Err(KleeneOracleError::AssignmentLimit {
                    assignments: assignment_space,
                    maximum: self.limits.max_assignments(),
                });
            }
        }
        Ok(KleeneDomain {
            predicates: predicates.into_iter().collect(),
            assignment_space,
        })
    }
}

impl Default for ExactKleeneOracle {
    fn default() -> Self {
        Self::new(ExactOracleLimits::default())
    }
}
#[derive(Clone, Debug)]
struct KleeneDomain {
    predicates: Vec<PredicateId>,
    assignment_space: usize,
}

impl KleeneDomain {
    fn fact_set(&self, mut assignment: usize) -> Result<FactSet, KleeneOracleError> {
        let mut facts = FactSet::new();
        for predicate in self.predicates.iter().copied() {
            let value = match assignment % 3 {
                0 => TruthValue::False,
                1 => TruthValue::True,
                _ => TruthValue::Unknown,
            };
            facts.set(predicate, value)?;
            assignment /= 3;
        }
        Ok(facts)
    }
}

fn collect_predicates(
    expression: &BoolExpr,
    depth: usize,
    predicates: &mut BTreeSet<PredicateId>,
) -> Result<(), KleeneOracleError> {
    if depth > MAX_BOOLEAN_EXPR_DEPTH {
        return Err(LogicError::ExpressionTooDeep {
            max_depth: MAX_BOOLEAN_EXPR_DEPTH,
        }
        .into());
    }
    match expression {
        BoolExpr::Const(_) => Ok(()),
        BoolExpr::Atom(id) => {
            if id.index() >= FAST_PREDICATE_CAPACITY {
                return Err(LogicError::PredicateOutOfRange { id: *id }.into());
            }
            predicates.insert(*id);
            Ok(())
        }
        BoolExpr::Not(inner) => collect_predicates(inner, depth + 1, predicates),
        BoolExpr::All(expressions) | BoolExpr::Any(expressions) => expressions
            .iter()
            .try_for_each(|inner| collect_predicates(inner, depth + 1, predicates)),
        BoolExpr::Xor(lhs, rhs) | BoolExpr::Implies(lhs, rhs) => {
            collect_predicates(lhs, depth + 1, predicates)?;
            collect_predicates(rhs, depth + 1, predicates)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: PredicateId = PredicateId::new(0);
    const B: PredicateId = PredicateId::new(1);
    const C: PredicateId = PredicateId::new(2);
    #[test]
    fn excluded_middle_is_classically_equivalent_to_true_but_not_under_strong_kleene() {
        let expression = BoolExpr::any([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(A))]);
        let always_true = BoolExpr::Const(true);

        let classical = crate::ExactBooleanOracle::default()
            .equivalence(&expression, &always_true)
            .unwrap();
        assert!(classical.holds());

        let kleene = ExactKleeneOracle::default()
            .equivalence(&expression, &always_true)
            .unwrap();
        assert!(!kleene.holds());
        assert_eq!(kleene.assignment_space(), 3);
        assert_eq!(
            kleene.counterexample().unwrap().get(A).unwrap(),
            TruthValue::Unknown
        );
    }

    #[test]
    fn kleene_redundancy_preserves_unknown_differences() {
        let excluded_middle =
            BoolExpr::any([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(A))]);
        let clauses = vec![excluded_middle, BoolExpr::Const(true)];
        let report = ExactKleeneOracle::default()
            .conjunction_clause_redundancy(&clauses, 0)
            .unwrap();
        assert!(!report.holds());
        assert_eq!(
            report.counterexample().unwrap().get(A).unwrap(),
            TruthValue::Unknown
        );
    }
    #[test]
    fn dead_guard_implication_equivalence_and_exclusion_are_exact() {
        let oracle = ExactKleeneOracle::default();
        let dead = BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(A))]);
        assert!(oracle.contradiction(&dead).unwrap().holds());
        assert!(oracle.satisfiability(&dead).unwrap().is_unsatisfiable());

        let lhs = BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B)]);
        assert!(oracle
            .implication(&lhs, &BoolExpr::atom(A))
            .unwrap()
            .holds());
        assert!(!oracle
            .implication(&BoolExpr::atom(A), &BoolExpr::atom(B))
            .unwrap()
            .holds());

        let left = BoolExpr::negate(BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B)]));
        let right = BoolExpr::any([
            BoolExpr::negate(BoolExpr::atom(A)),
            BoolExpr::negate(BoolExpr::atom(B)),
        ]);
        assert!(oracle.equivalence(&left, &right).unwrap().holds());
        assert!(oracle
            .mutual_exclusion(&BoolExpr::atom(A), &BoolExpr::negate(BoolExpr::atom(A)))
            .unwrap()
            .holds());
    }
    #[test]
    fn ternary_assignment_space_is_bounded_before_enumeration() {
        let oracle = ExactKleeneOracle::new(ExactOracleLimits::new(3, 8).unwrap());
        let expression = BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B), BoolExpr::atom(C)]);
        assert!(matches!(
            oracle.satisfiability(&expression),
            Err(KleeneOracleError::AssignmentLimit {
                assignments: 9,
                maximum: 8,
            })
        ));
    }

    #[test]
    fn constant_domain_has_one_three_valued_assignment() {
        let oracle = ExactKleeneOracle::default();
        let report = oracle.satisfiability(&BoolExpr::Const(true)).unwrap();
        assert_eq!(report.variables(), 0);
        assert_eq!(report.assignment_space(), 1);
        assert_eq!(report.assignments_evaluated(), 1);
        assert!(report.is_satisfiable());
    }
}
