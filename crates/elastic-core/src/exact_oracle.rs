//! Dependency-free bounded exact analysis for small Boolean policy domains.
//!
//! This module is deliberately separate from runtime validation and actuation.
//! It exhaustively enumerates fully-grounded Boolean assignments for small
//! expressions and returns witnesses or counterexamples. Resource limits are
//! explicit and an analysis error is never a positive policy result.

use std::collections::BTreeSet;
use std::fmt;

use crate::{
    BoolExpr, FactSet, LogicError, PredicateId, TruthValue, FAST_PREDICATE_CAPACITY,
    MAX_BOOLEAN_EXPR_DEPTH,
};

/// Hard implementation cap for variables accepted by the exact small-domain oracle.
pub const MAX_EXACT_ORACLE_VARIABLES: usize = 20;

/// Hard implementation cap for assignments evaluated by one exact query.
pub const MAX_EXACT_ORACLE_ASSIGNMENTS: usize = 1 << MAX_EXACT_ORACLE_VARIABLES;

/// Default variable cap keeps routine exact checks small while remaining configurable.
pub const DEFAULT_EXACT_ORACLE_VARIABLES: usize = 12;

/// Default assignment cap corresponding to [`DEFAULT_EXACT_ORACLE_VARIABLES`].
pub const DEFAULT_EXACT_ORACLE_ASSIGNMENTS: usize = 1 << DEFAULT_EXACT_ORACLE_VARIABLES;

/// Bounded exact-oracle configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactOracleLimits {
    max_variables: usize,
    max_assignments: usize,
}

impl ExactOracleLimits {
    /// Construct bounded limits. Values above the hard implementation caps fail closed.
    pub fn new(max_variables: usize, max_assignments: usize) -> Result<Self, ExactOracleError> {
        if max_variables > MAX_EXACT_ORACLE_VARIABLES {
            return Err(ExactOracleError::InvalidLimits {
                max_variables,
                max_assignments,
            });
        }
        if max_assignments == 0 || max_assignments > MAX_EXACT_ORACLE_ASSIGNMENTS {
            return Err(ExactOracleError::InvalidLimits {
                max_variables,
                max_assignments,
            });
        }
        Ok(Self {
            max_variables,
            max_assignments,
        })
    }

    /// Maximum number of distinct predicate variables permitted by this oracle.
    #[must_use]
    pub const fn max_variables(self) -> usize {
        self.max_variables
    }

    /// Maximum Boolean assignments permitted by this oracle.
    #[must_use]
    pub const fn max_assignments(self) -> usize {
        self.max_assignments
    }
}

impl Default for ExactOracleLimits {
    fn default() -> Self {
        Self {
            max_variables: DEFAULT_EXACT_ORACLE_VARIABLES,
            max_assignments: DEFAULT_EXACT_ORACLE_ASSIGNMENTS,
        }
    }
}

/// Fail-closed errors from bounded exact analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactOracleError {
    /// The caller supplied invalid resource limits.
    InvalidLimits {
        max_variables: usize,
        max_assignments: usize,
    },
    /// The query references more distinct predicates than permitted.
    VariableLimit { variables: usize, maximum: usize },
    /// Exhaustive enumeration would exceed the configured assignment budget.
    AssignmentLimit { assignments: usize, maximum: usize },
    /// A conjunction clause index was outside the declared clause list.
    ClauseOutOfRange { index: usize, clauses: usize },
    /// The underlying bounded Boolean evaluator rejected the expression.
    Logic(LogicError),
    /// A fully assigned Boolean expression unexpectedly evaluated to `Unknown`.
    UnexpectedUnknown,
}

impl fmt::Display for ExactOracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits {
                max_variables,
                max_assignments,
            } => write!(
                f,
                "invalid exact-oracle limits: variables={max_variables}, assignments={max_assignments}"
            ),
            Self::VariableLimit { variables, maximum } => write!(
                f,
                "exact-oracle variable count {variables} exceeds configured maximum {maximum}"
            ),
            Self::AssignmentLimit {
                assignments,
                maximum,
            } => write!(
                f,
                "exact-oracle assignment count {assignments} exceeds configured maximum {maximum}"
            ),
            Self::ClauseOutOfRange { index, clauses } => {
                write!(f, "conjunction clause index {index} is out of range for {clauses} clauses")
            }
            Self::Logic(error) => error.fmt(f),
            Self::UnexpectedUnknown => write!(
                f,
                "fully assigned Boolean expression unexpectedly evaluated to Unknown"
            ),
        }
    }
}

impl std::error::Error for ExactOracleError {}

impl From<LogicError> for ExactOracleError {
    fn from(value: LogicError) -> Self {
        Self::Logic(value)
    }
}

/// Exact satisfiability result with a concrete witness when satisfiable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactSatisfiabilityReport {
    variables: usize,
    assignment_space: usize,
    assignments_evaluated: usize,
    witness: Option<FactSet>,
}

impl ExactSatisfiabilityReport {
    /// Whether at least one fully-grounded Boolean assignment satisfies the expression.
    #[must_use]
    pub const fn is_satisfiable(self) -> bool {
        self.witness.is_some()
    }

    /// Whether exhaustive enumeration established that no satisfying assignment exists.
    #[must_use]
    pub const fn is_unsatisfiable(self) -> bool {
        self.witness.is_none()
    }

    /// First satisfying assignment in deterministic binary enumeration order.
    #[must_use]
    pub const fn witness(self) -> Option<FactSet> {
        self.witness
    }

    /// Number of distinct predicate variables in the exact query.
    #[must_use]
    pub const fn variables(self) -> usize {
        self.variables
    }

    /// Total number of assignments in the exact Boolean domain.
    #[must_use]
    pub const fn assignment_space(self) -> usize {
        self.assignment_space
    }

    /// Assignments actually evaluated before proving unsatisfiability or finding a witness.
    #[must_use]
    pub const fn assignments_evaluated(self) -> usize {
        self.assignments_evaluated
    }
}

/// Exact universal-property result with a concrete counterexample when refuted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactPropertyReport {
    variables: usize,
    assignment_space: usize,
    assignments_evaluated: usize,
    counterexample: Option<FactSet>,
}

impl ExactPropertyReport {
    /// Whether the queried property holds for every Boolean assignment in the domain.
    #[must_use]
    pub const fn holds(self) -> bool {
        self.counterexample.is_none()
    }

    /// First counterexample in deterministic binary enumeration order, if any.
    #[must_use]
    pub const fn counterexample(self) -> Option<FactSet> {
        self.counterexample
    }

    /// Number of distinct predicate variables in the exact query.
    #[must_use]
    pub const fn variables(self) -> usize {
        self.variables
    }

    /// Total number of assignments in the exact Boolean domain.
    #[must_use]
    pub const fn assignment_space(self) -> usize {
        self.assignment_space
    }

    /// Assignments actually evaluated before proving the property or finding a counterexample.
    #[must_use]
    pub const fn assignments_evaluated(self) -> usize {
        self.assignments_evaluated
    }
}

/// Dependency-free exhaustive Boolean oracle for bounded small domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactBooleanOracle {
    limits: ExactOracleLimits,
}

impl ExactBooleanOracle {
    /// Construct an oracle with explicit resource limits.
    #[must_use]
    pub const fn new(limits: ExactOracleLimits) -> Self {
        Self { limits }
    }

    /// Limits enforced by this oracle.
    #[must_use]
    pub const fn limits(self) -> ExactOracleLimits {
        self.limits
    }

    /// Determine exact Boolean satisfiability and retain the first satisfying witness.
    pub fn satisfiability(
        self,
        expression: &BoolExpr,
    ) -> Result<ExactSatisfiabilityReport, ExactOracleError> {
        let domain = self.prepare(&[expression])?;
        for assignment in 0..domain.assignment_space {
            let facts = domain.fact_set(assignment)?;
            match expression.evaluate(&facts)? {
                TruthValue::True => {
                    return Ok(ExactSatisfiabilityReport {
                        variables: domain.predicates.len(),
                        assignment_space: domain.assignment_space,
                        assignments_evaluated: assignment + 1,
                        witness: Some(facts),
                    });
                }
                TruthValue::False => {}
                TruthValue::Unknown => return Err(ExactOracleError::UnexpectedUnknown),
            }
        }
        Ok(ExactSatisfiabilityReport {
            variables: domain.predicates.len(),
            assignment_space: domain.assignment_space,
            assignments_evaluated: domain.assignment_space,
            witness: None,
        })
    }

    /// Determine whether an expression is true under every Boolean assignment.
    pub fn tautology(self, expression: &BoolExpr) -> Result<ExactPropertyReport, ExactOracleError> {
        self.universal(&[expression], |facts| match expression.evaluate(facts)? {
            TruthValue::True => Ok(true),
            TruthValue::False => Ok(false),
            TruthValue::Unknown => Err(ExactOracleError::UnexpectedUnknown),
        })
    }

    /// Determine whether an expression is a contradiction (a dead Boolean guard).
    pub fn contradiction(
        self,
        expression: &BoolExpr,
    ) -> Result<ExactPropertyReport, ExactOracleError> {
        self.universal(&[expression], |facts| match expression.evaluate(facts)? {
            TruthValue::True => Ok(false),
            TruthValue::False => Ok(true),
            TruthValue::Unknown => Err(ExactOracleError::UnexpectedUnknown),
        })
    }

    /// Exact material implication `lhs -> rhs` over the union of referenced predicates.
    pub fn implication(
        self,
        lhs: &BoolExpr,
        rhs: &BoolExpr,
    ) -> Result<ExactPropertyReport, ExactOracleError> {
        self.universal(&[lhs, rhs], |facts| {
            let lhs = boolean_value(lhs.evaluate(facts)?)?;
            let rhs = boolean_value(rhs.evaluate(facts)?)?;
            Ok(!lhs || rhs)
        })
    }

    /// Exact semantic equivalence over the union of referenced predicates.
    pub fn equivalence(
        self,
        lhs: &BoolExpr,
        rhs: &BoolExpr,
    ) -> Result<ExactPropertyReport, ExactOracleError> {
        self.universal(&[lhs, rhs], |facts| {
            Ok(boolean_value(lhs.evaluate(facts)?)? == boolean_value(rhs.evaluate(facts)?)?)
        })
    }

    /// Exact mutual exclusion: no assignment may make both expressions true.
    pub fn mutual_exclusion(
        self,
        lhs: &BoolExpr,
        rhs: &BoolExpr,
    ) -> Result<ExactPropertyReport, ExactOracleError> {
        self.universal(&[lhs, rhs], |facts| {
            let lhs = boolean_value(lhs.evaluate(facts)?)?;
            let rhs = boolean_value(rhs.evaluate(facts)?)?;
            Ok(!(lhs && rhs))
        })
    }

    /// Determine whether one conjunction clause is semantically redundant.
    ///
    /// The result holds iff removing `clauses[index]` leaves the conjunction
    /// exactly equivalent under every Boolean assignment.
    pub fn conjunction_clause_redundancy(
        self,
        clauses: &[BoolExpr],
        index: usize,
    ) -> Result<ExactPropertyReport, ExactOracleError> {
        if index >= clauses.len() {
            return Err(ExactOracleError::ClauseOutOfRange {
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
        mut property: impl FnMut(&FactSet) -> Result<bool, ExactOracleError>,
    ) -> Result<ExactPropertyReport, ExactOracleError> {
        let domain = self.prepare(expressions)?;
        for assignment in 0..domain.assignment_space {
            let facts = domain.fact_set(assignment)?;
            if !property(&facts)? {
                return Ok(ExactPropertyReport {
                    variables: domain.predicates.len(),
                    assignment_space: domain.assignment_space,
                    assignments_evaluated: assignment + 1,
                    counterexample: Some(facts),
                });
            }
        }
        Ok(ExactPropertyReport {
            variables: domain.predicates.len(),
            assignment_space: domain.assignment_space,
            assignments_evaluated: domain.assignment_space,
            counterexample: None,
        })
    }

    fn prepare(self, expressions: &[&BoolExpr]) -> Result<ExactDomain, ExactOracleError> {
        let mut predicates = BTreeSet::new();
        for expression in expressions {
            collect_predicates(expression, 0, &mut predicates)?;
        }
        if predicates.len() > self.limits.max_variables {
            return Err(ExactOracleError::VariableLimit {
                variables: predicates.len(),
                maximum: self.limits.max_variables,
            });
        }
        let assignment_space = 1usize.checked_shl(predicates.len() as u32).ok_or(
            ExactOracleError::AssignmentLimit {
                assignments: usize::MAX,
                maximum: self.limits.max_assignments,
            },
        )?;
        if assignment_space > self.limits.max_assignments {
            return Err(ExactOracleError::AssignmentLimit {
                assignments: assignment_space,
                maximum: self.limits.max_assignments,
            });
        }
        Ok(ExactDomain {
            predicates: predicates.into_iter().collect(),
            assignment_space,
        })
    }
}

impl Default for ExactBooleanOracle {
    fn default() -> Self {
        Self::new(ExactOracleLimits::default())
    }
}

#[derive(Clone, Debug)]
struct ExactDomain {
    predicates: Vec<PredicateId>,
    assignment_space: usize,
}

impl ExactDomain {
    fn fact_set(&self, assignment: usize) -> Result<FactSet, ExactOracleError> {
        let mut facts = FactSet::new();
        for (bit, predicate) in self.predicates.iter().copied().enumerate() {
            let value = if assignment & (1usize << bit) == 0 {
                TruthValue::False
            } else {
                TruthValue::True
            };
            facts.set(predicate, value)?;
        }
        Ok(facts)
    }
}

fn collect_predicates(
    expression: &BoolExpr,
    depth: usize,
    predicates: &mut BTreeSet<PredicateId>,
) -> Result<(), ExactOracleError> {
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

fn boolean_value(value: TruthValue) -> Result<bool, ExactOracleError> {
    match value {
        TruthValue::True => Ok(true),
        TruthValue::False => Ok(false),
        TruthValue::Unknown => Err(ExactOracleError::UnexpectedUnknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: PredicateId = PredicateId::new(0);
    const B: PredicateId = PredicateId::new(1);
    const C: PredicateId = PredicateId::new(2);

    #[test]
    fn satisfiable_and_contradictory_queries_return_exact_witnesses() {
        let oracle = ExactBooleanOracle::default();
        let satisfiable = BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(B))]);
        let report = oracle.satisfiability(&satisfiable).unwrap();
        assert!(report.is_satisfiable());
        assert_eq!(report.variables(), 2);
        assert_eq!(report.assignment_space(), 4);
        let witness = report.witness().unwrap();
        assert_eq!(witness.get(A).unwrap(), TruthValue::True);
        assert_eq!(witness.get(B).unwrap(), TruthValue::False);

        let contradiction = BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(A))]);
        let sat = oracle.satisfiability(&contradiction).unwrap();
        assert!(sat.is_unsatisfiable());
        assert_eq!(sat.assignments_evaluated(), 2);
        assert!(oracle.contradiction(&contradiction).unwrap().holds());
    }

    #[test]
    fn tautology_implication_equivalence_and_exclusion_are_exact() {
        let oracle = ExactBooleanOracle::default();
        let excluded_middle =
            BoolExpr::any([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(A))]);
        assert!(oracle.tautology(&excluded_middle).unwrap().holds());

        let lhs = BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B)]);
        assert!(oracle
            .implication(&lhs, &BoolExpr::atom(A))
            .unwrap()
            .holds());
        let false_implication = oracle
            .implication(&BoolExpr::atom(A), &BoolExpr::atom(B))
            .unwrap();
        assert!(!false_implication.holds());
        let counterexample = false_implication.counterexample().unwrap();
        assert_eq!(counterexample.get(A).unwrap(), TruthValue::True);
        assert_eq!(counterexample.get(B).unwrap(), TruthValue::False);

        let de_morgan_lhs = BoolExpr::negate(BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B)]));
        let de_morgan_rhs = BoolExpr::any([
            BoolExpr::negate(BoolExpr::atom(A)),
            BoolExpr::negate(BoolExpr::atom(B)),
        ]);
        assert!(oracle
            .equivalence(&de_morgan_lhs, &de_morgan_rhs)
            .unwrap()
            .holds());

        assert!(oracle
            .mutual_exclusion(&BoolExpr::atom(A), &BoolExpr::negate(BoolExpr::atom(A)))
            .unwrap()
            .holds());
        assert!(!oracle
            .mutual_exclusion(&BoolExpr::atom(A), &BoolExpr::atom(B))
            .unwrap()
            .holds());
    }

    #[test]
    fn redundant_conjunction_clause_is_detected_semantically() {
        let oracle = ExactBooleanOracle::default();
        let clauses = vec![
            BoolExpr::atom(A),
            BoolExpr::any([BoolExpr::atom(A), BoolExpr::atom(B)]),
            BoolExpr::Const(true),
        ];
        assert!(oracle
            .conjunction_clause_redundancy(&clauses, 1)
            .unwrap()
            .holds());
        assert!(oracle
            .conjunction_clause_redundancy(&clauses, 2)
            .unwrap()
            .holds());
        assert!(!oracle
            .conjunction_clause_redundancy(&clauses, 0)
            .unwrap()
            .holds());
        assert!(matches!(
            oracle.conjunction_clause_redundancy(&clauses, 3),
            Err(ExactOracleError::ClauseOutOfRange {
                index: 3,
                clauses: 3
            })
        ));
    }

    #[test]
    fn resource_limits_fail_closed_before_enumeration() {
        let oracle = ExactBooleanOracle::new(ExactOracleLimits::new(2, 4).unwrap());
        let three_variables =
            BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(B), BoolExpr::atom(C)]);
        assert!(matches!(
            oracle.satisfiability(&three_variables),
            Err(ExactOracleError::VariableLimit {
                variables: 3,
                maximum: 2,
            })
        ));

        let assignment_limited = ExactBooleanOracle::new(ExactOracleLimits::new(3, 4).unwrap());
        assert!(matches!(
            assignment_limited.satisfiability(&three_variables),
            Err(ExactOracleError::AssignmentLimit {
                assignments: 8,
                maximum: 4,
            })
        ));
        assert!(ExactOracleLimits::new(MAX_EXACT_ORACLE_VARIABLES + 1, 1).is_err());
        assert!(ExactOracleLimits::new(1, MAX_EXACT_ORACLE_ASSIGNMENTS + 1).is_err());
    }

    #[test]
    fn empty_and_constant_domains_still_have_one_exact_assignment() {
        let oracle = ExactBooleanOracle::default();
        let true_report = oracle.satisfiability(&BoolExpr::Const(true)).unwrap();
        assert_eq!(true_report.variables(), 0);
        assert_eq!(true_report.assignment_space(), 1);
        assert!(true_report.is_satisfiable());
        assert!(oracle.tautology(&BoolExpr::Const(true)).unwrap().holds());
        assert!(oracle
            .contradiction(&BoolExpr::Const(false))
            .unwrap()
            .holds());
    }
}
