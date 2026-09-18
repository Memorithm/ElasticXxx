//! Dependency-free exact reference enumeration for bounded pseudo-Boolean constraints.
//!
//! The runtime evaluator in [`crate::pseudo_boolean`] intentionally uses cheap
//! conservative interval bounds. This module provides a separate small-domain
//! reference oracle that enumerates every completion of the currently unknown
//! predicates under explicit [`ExactOracleLimits`]. It is analysis-only: a
//! successful result never validates or authorizes actuation, and exceeding a
//! configured resource bound is an error rather than permission.

use std::fmt;

use crate::{
    ExactOracleLimits, FactSet, LogicError, PredicateId, PseudoBooleanConstraint,
    PseudoBooleanError, TruthValue,
};

/// Fail-closed errors from exact pseudo-Boolean small-domain enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactPseudoBooleanOracleError {
    /// The number of currently unknown referenced predicates exceeds the configured bound.
    VariableLimit { variables: usize, maximum: usize },
    /// Exhaustive completion would exceed the configured assignment budget.
    AssignmentLimit { assignments: usize, maximum: usize },
    /// Constraint evaluation failed, including checked-arithmetic overflow.
    Constraint(PseudoBooleanError),
    /// Construction of a fully grounded fact assignment failed.
    Logic(LogicError),
    /// A fully grounded constraint unexpectedly retained `Unknown`.
    UnexpectedUnknown,
}

impl fmt::Display for ExactPseudoBooleanOracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VariableLimit { variables, maximum } => write!(
                f,
                "exact pseudo-Boolean variable count {variables} exceeds configured maximum {maximum}"
            ),
            Self::AssignmentLimit {
                assignments,
                maximum,
            } => write!(
                f,
                "exact pseudo-Boolean assignment count {assignments} exceeds configured maximum {maximum}"
            ),
            Self::Constraint(error) => error.fmt(f),
            Self::Logic(error) => error.fmt(f),
            Self::UnexpectedUnknown => write!(
                f,
                "fully grounded pseudo-Boolean constraint unexpectedly evaluated to Unknown"
            ),
        }
    }
}

impl std::error::Error for ExactPseudoBooleanOracleError {}

impl From<PseudoBooleanError> for ExactPseudoBooleanOracleError {
    fn from(value: PseudoBooleanError) -> Self {
        Self::Constraint(value)
    }
}

impl From<LogicError> for ExactPseudoBooleanOracleError {
    fn from(value: LogicError) -> Self {
        Self::Logic(value)
    }
}

/// Exact completion report for one pseudo-Boolean constraint under partial facts.
///
/// Counts are exact for the bounded domain because every completion is evaluated.
/// `truth_value()` follows the same fail-closed three-valued contract as the
/// runtime evaluator: `True` means every completion satisfies the constraint,
/// `False` means none does, and `Unknown` means satisfying and violating
/// completions both exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactPseudoBooleanReport {
    unknown_variables: usize,
    assignment_space: usize,
    satisfying_assignments: usize,
    violating_assignments: usize,
    first_satisfying: Option<FactSet>,
    first_violating: Option<FactSet>,
}

impl ExactPseudoBooleanReport {
    /// Number of referenced predicates that were `Unknown` in the input facts.
    #[must_use]
    pub const fn unknown_variables(self) -> usize {
        self.unknown_variables
    }

    /// Number of exact Boolean completions evaluated.
    #[must_use]
    pub const fn assignment_space(self) -> usize {
        self.assignment_space
    }

    /// Exact number of satisfying completions.
    #[must_use]
    pub const fn satisfying_assignments(self) -> usize {
        self.satisfying_assignments
    }

    /// Exact number of violating completions.
    #[must_use]
    pub const fn violating_assignments(self) -> usize {
        self.violating_assignments
    }

    /// Deterministic first satisfying completion, if one exists.
    #[must_use]
    pub const fn first_satisfying(self) -> Option<FactSet> {
        self.first_satisfying
    }

    /// Deterministic first violating completion, if one exists.
    #[must_use]
    pub const fn first_violating(self) -> Option<FactSet> {
        self.first_violating
    }

    /// Whether at least one completion satisfies the constraint.
    #[must_use]
    pub const fn is_satisfiable(self) -> bool {
        self.satisfying_assignments != 0
    }

    /// Whether no completion satisfies the constraint.
    #[must_use]
    pub const fn is_unsatisfiable(self) -> bool {
        self.satisfying_assignments == 0
    }

    /// Whether every completion satisfies the constraint.
    #[must_use]
    pub const fn is_tautological(self) -> bool {
        self.violating_assignments == 0
    }

    /// Exact three-valued result across every completion.
    #[must_use]
    pub const fn truth_value(self) -> TruthValue {
        if self.violating_assignments == 0 {
            TruthValue::True
        } else if self.satisfying_assignments == 0 {
            TruthValue::False
        } else {
            TruthValue::Unknown
        }
    }
}

/// Dependency-free exhaustive reference oracle for small pseudo-Boolean domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactPseudoBooleanOracle {
    limits: ExactOracleLimits,
}

impl ExactPseudoBooleanOracle {
    /// Construct an exact oracle with explicit shared small-domain limits.
    #[must_use]
    pub const fn new(limits: ExactOracleLimits) -> Self {
        Self { limits }
    }

    /// Resource limits enforced before enumeration begins.
    #[must_use]
    pub const fn limits(self) -> ExactOracleLimits {
        self.limits
    }

    /// Exhaustively enumerate every completion of unknown referenced predicates.
    ///
    /// Known `True`/`False` facts are held fixed. Only predicates referenced by
    /// the constraint participate in the enumeration. The ordering is stable:
    /// terms are already canonical by compact predicate id, and assignment bits
    /// follow that order.
    pub fn analyze(
        self,
        constraint: &PseudoBooleanConstraint,
        facts: &FactSet,
    ) -> Result<ExactPseudoBooleanReport, ExactPseudoBooleanOracleError> {
        let unknown = constraint
            .terms()
            .iter()
            .filter_map(|term| match facts.get(term.predicate()) {
                Ok(TruthValue::Unknown) => Some(Ok(term.predicate())),
                Ok(TruthValue::True | TruthValue::False) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()?;

        if unknown.len() > self.limits.max_variables() {
            return Err(ExactPseudoBooleanOracleError::VariableLimit {
                variables: unknown.len(),
                maximum: self.limits.max_variables(),
            });
        }

        let assignment_space = 1usize.checked_shl(unknown.len() as u32).ok_or(
            ExactPseudoBooleanOracleError::AssignmentLimit {
                assignments: usize::MAX,
                maximum: self.limits.max_assignments(),
            },
        )?;
        if assignment_space > self.limits.max_assignments() {
            return Err(ExactPseudoBooleanOracleError::AssignmentLimit {
                assignments: assignment_space,
                maximum: self.limits.max_assignments(),
            });
        }

        let mut satisfying_assignments = 0usize;
        let mut violating_assignments = 0usize;
        let mut first_satisfying = None;
        let mut first_violating = None;

        for assignment in 0..assignment_space {
            let completion = complete_facts(*facts, &unknown, assignment)?;
            match constraint.evaluate(&completion)? {
                TruthValue::True => {
                    satisfying_assignments += 1;
                    if first_satisfying.is_none() {
                        first_satisfying = Some(completion);
                    }
                }
                TruthValue::False => {
                    violating_assignments += 1;
                    if first_violating.is_none() {
                        first_violating = Some(completion);
                    }
                }
                TruthValue::Unknown => {
                    return Err(ExactPseudoBooleanOracleError::UnexpectedUnknown);
                }
            }
        }

        Ok(ExactPseudoBooleanReport {
            unknown_variables: unknown.len(),
            assignment_space,
            satisfying_assignments,
            violating_assignments,
            first_satisfying,
            first_violating,
        })
    }
}

impl Default for ExactPseudoBooleanOracle {
    fn default() -> Self {
        Self::new(ExactOracleLimits::default())
    }
}

fn complete_facts(
    mut facts: FactSet,
    unknown: &[PredicateId],
    assignment: usize,
) -> Result<FactSet, ExactPseudoBooleanOracleError> {
    for (bit, predicate) in unknown.iter().copied().enumerate() {
        let value = if assignment & (1usize << bit) == 0 {
            TruthValue::False
        } else {
            TruthValue::True
        };
        facts.set(predicate, value)?;
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PseudoBooleanRelation, PseudoBooleanScale, WeightedPredicate,
        DEFAULT_EXACT_ORACLE_ASSIGNMENTS,
    };

    const A: PredicateId = PredicateId::new(0);
    const B: PredicateId = PredicateId::new(1);
    const C: PredicateId = PredicateId::new(2);

    fn constraint(
        terms: &[(PredicateId, i128)],
        relation: PseudoBooleanRelation,
        threshold: i128,
    ) -> PseudoBooleanConstraint {
        PseudoBooleanConstraint::new(
            terms
                .iter()
                .map(|(predicate, weight)| WeightedPredicate::new(*predicate, *weight).unwrap())
                .collect(),
            relation,
            threshold,
            PseudoBooleanScale::count(),
        )
        .unwrap()
    }

    #[test]
    fn exact_oracle_refines_interval_unknown_for_equality() {
        let exactly_one = constraint(
            &[(A, 1), (B, 1)],
            PseudoBooleanRelation::Equal,
            1,
        );
        let facts = FactSet::new();
        assert_eq!(exactly_one.evaluate(&facts).unwrap(), TruthValue::Unknown);

        let report = ExactPseudoBooleanOracle::default()
            .analyze(&exactly_one, &facts)
            .unwrap();
        assert_eq!(report.unknown_variables(), 2);
        assert_eq!(report.assignment_space(), 4);
        assert_eq!(report.satisfying_assignments(), 2);
        assert_eq!(report.violating_assignments(), 2);
        assert_eq!(report.truth_value(), TruthValue::Unknown);
        assert!(report.is_satisfiable());
        assert!(!report.is_tautological());
    }

    #[test]
    fn exact_oracle_holds_known_facts_fixed() {
        let budget = constraint(
            &[(A, 2), (B, 3), (C, 5)],
            PseudoBooleanRelation::LessOrEqual,
            5,
        );
        let facts = FactSet::new()
            .with(A, TruthValue::True)
            .unwrap()
            .with(B, TruthValue::False)
            .unwrap();

        let report = ExactPseudoBooleanOracle::default()
            .analyze(&budget, &facts)
            .unwrap();
        assert_eq!(report.unknown_variables(), 1);
        assert_eq!(report.assignment_space(), 2);
        assert_eq!(report.satisfying_assignments(), 1);
        assert_eq!(report.violating_assignments(), 1);
        let witness = report.first_satisfying().unwrap();
        assert_eq!(witness.get(A).unwrap(), TruthValue::True);
        assert_eq!(witness.get(B).unwrap(), TruthValue::False);
        assert_eq!(witness.get(C).unwrap(), TruthValue::False);
    }

    #[test]
    fn exact_oracle_proves_unsatisfiable_and_tautological_small_domains() {
        let impossible = constraint(
            &[(A, 1), (B, 1)],
            PseudoBooleanRelation::GreaterOrEqual,
            3,
        );
        let impossible_report = ExactPseudoBooleanOracle::default()
            .analyze(&impossible, &FactSet::new())
            .unwrap();
        assert!(impossible_report.is_unsatisfiable());
        assert_eq!(impossible_report.truth_value(), TruthValue::False);
        assert!(impossible_report.first_satisfying().is_none());
        assert!(impossible_report.first_violating().is_some());

        let always_within = constraint(
            &[(A, 1), (B, 1)],
            PseudoBooleanRelation::LessOrEqual,
            2,
        );
        let tautology_report = ExactPseudoBooleanOracle::default()
            .analyze(&always_within, &FactSet::new())
            .unwrap();
        assert!(tautology_report.is_tautological());
        assert_eq!(tautology_report.truth_value(), TruthValue::True);
        assert!(tautology_report.first_violating().is_none());
    }

    #[test]
    fn signed_weights_are_enumerated_exactly() {
        let implication = constraint(
            &[(A, 1), (B, -1)],
            PseudoBooleanRelation::LessOrEqual,
            0,
        );
        let report = ExactPseudoBooleanOracle::default()
            .analyze(&implication, &FactSet::new())
            .unwrap();
        assert_eq!(report.assignment_space(), 4);
        assert_eq!(report.satisfying_assignments(), 3);
        assert_eq!(report.violating_assignments(), 1);
        let counterexample = report.first_violating().unwrap();
        assert_eq!(counterexample.get(A).unwrap(), TruthValue::True);
        assert_eq!(counterexample.get(B).unwrap(), TruthValue::False);
    }

    #[test]
    fn resource_limits_fail_closed_before_enumeration() {
        let three = constraint(
            &[(A, 1), (B, 1), (C, 1)],
            PseudoBooleanRelation::LessOrEqual,
            2,
        );
        let variable_limited = ExactPseudoBooleanOracle::new(ExactOracleLimits::new(2, 4).unwrap());
        assert!(matches!(
            variable_limited.analyze(&three, &FactSet::new()),
            Err(ExactPseudoBooleanOracleError::VariableLimit {
                variables: 3,
                maximum: 2,
            })
        ));

        let assignment_limited =
            ExactPseudoBooleanOracle::new(ExactOracleLimits::new(3, 4).unwrap());
        assert!(matches!(
            assignment_limited.analyze(&three, &FactSet::new()),
            Err(ExactPseudoBooleanOracleError::AssignmentLimit {
                assignments: 8,
                maximum: 4,
            })
        ));
        assert!(DEFAULT_EXACT_ORACLE_ASSIGNMENTS >= 4);
    }

    #[test]
    fn arithmetic_overflow_is_a_non_result() {
        let overflowing = constraint(
            &[(A, i128::MAX), (B, 1)],
            PseudoBooleanRelation::LessOrEqual,
            i128::MAX,
        );
        let facts = FactSet::new()
            .with(A, TruthValue::True)
            .unwrap()
            .with(B, TruthValue::True)
            .unwrap();
        assert!(matches!(
            ExactPseudoBooleanOracle::default().analyze(&overflowing, &facts),
            Err(ExactPseudoBooleanOracleError::Constraint(
                PseudoBooleanError::ArithmeticOverflow
            ))
        ));
    }
}