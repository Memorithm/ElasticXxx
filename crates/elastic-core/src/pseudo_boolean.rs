//! Bounded pseudo-Boolean constraints for declarative eligibility analysis.
//!
//! A constraint is a linear integer form over Boolean predicates. Missing facts
//! remain [`TruthValue::Unknown`]; the evaluator computes conservative exact
//! lower/upper bounds from the currently unknown independent variables and only
//! returns `True` or `False` when every completion agrees. Constraints are
//! analysis/filtering data only. They do not validate or authorize actuation.

use std::fmt;
use std::num::NonZeroU64;

use crate::{FactSet, LogicError, PredicateId, TruthValue, FAST_PREDICATE_CAPACITY};

/// Maximum number of weighted terms in one dependency-free constraint.
pub const MAX_PSEUDO_BOOLEAN_TERMS: usize = FAST_PREDICATE_CAPACITY as usize;

/// Maximum UTF-8 byte length of one explicit unit label.
pub const MAX_PSEUDO_BOOLEAN_UNIT_BYTES: usize = 64;

/// Explicit integer scale shared by all terms and the threshold of a constraint.
///
/// `quantum` states how many base units one integer tick represents. Arithmetic
/// is intentionally performed in integer ticks; callers that need conversion to
/// base units must do so explicitly at their typed boundary.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PseudoBooleanScale {
    unit: String,
    quantum: NonZeroU64,
}

impl PseudoBooleanScale {
    /// Construct an explicit bounded unit/quantum pair.
    pub fn new(unit: impl Into<String>, quantum: u64) -> Result<Self, PseudoBooleanError> {
        let unit = unit.into();
        if unit.is_empty() {
            return Err(PseudoBooleanError::EmptyUnit);
        }
        if unit.trim() != unit {
            return Err(PseudoBooleanError::UnitNotTrimmed);
        }
        if unit.len() > MAX_PSEUDO_BOOLEAN_UNIT_BYTES {
            return Err(PseudoBooleanError::UnitTooLong {
                bytes: unit.len(),
                maximum: MAX_PSEUDO_BOOLEAN_UNIT_BYTES,
            });
        }
        let quantum = NonZeroU64::new(quantum).ok_or(PseudoBooleanError::ZeroQuantum)?;
        Ok(Self { unit, quantum })
    }

    /// Canonical scale for cardinality constraints.
    #[must_use]
    pub fn count() -> Self {
        Self {
            unit: "count".to_owned(),
            quantum: NonZeroU64::new(1).expect("one is non-zero"),
        }
    }

    /// Declared base-unit label.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Base units represented by one integer tick.
    #[must_use]
    pub const fn quantum(&self) -> u64 {
        self.quantum.get()
    }
}

/// One integer-weighted Boolean predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WeightedPredicate {
    predicate: PredicateId,
    weight: i128,
}

impl WeightedPredicate {
    /// Construct one non-zero weighted term.
    pub fn new(predicate: PredicateId, weight: i128) -> Result<Self, PseudoBooleanError> {
        if predicate.index() >= FAST_PREDICATE_CAPACITY {
            return Err(PseudoBooleanError::PredicateOutOfRange { predicate });
        }
        if weight == 0 {
            return Err(PseudoBooleanError::ZeroWeight { predicate });
        }
        Ok(Self { predicate, weight })
    }

    /// Predicate referenced by this term.
    #[must_use]
    pub const fn predicate(self) -> PredicateId {
        self.predicate
    }

    /// Signed integer weight in the constraint scale's ticks.
    #[must_use]
    pub const fn weight(self) -> i128 {
        self.weight
    }
}

/// Supported linear relation to the integer threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PseudoBooleanRelation {
    LessOrEqual,
    GreaterOrEqual,
    Equal,
}

/// A bounded, canonical pseudo-Boolean constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PseudoBooleanConstraint {
    terms: Vec<WeightedPredicate>,
    relation: PseudoBooleanRelation,
    threshold: i128,
    scale: PseudoBooleanScale,
}

impl PseudoBooleanConstraint {
    /// Construct and canonicalize a constraint by predicate identifier.
    ///
    /// Duplicate predicates are rejected rather than implicitly combining
    /// weights, so a persisted or generated declaration cannot change meaning
    /// through hidden normalization.
    pub fn new(
        mut terms: Vec<WeightedPredicate>,
        relation: PseudoBooleanRelation,
        threshold: i128,
        scale: PseudoBooleanScale,
    ) -> Result<Self, PseudoBooleanError> {
        if terms.len() > MAX_PSEUDO_BOOLEAN_TERMS {
            return Err(PseudoBooleanError::TooManyTerms {
                terms: terms.len(),
                maximum: MAX_PSEUDO_BOOLEAN_TERMS,
            });
        }
        terms.sort_unstable_by_key(|term| term.predicate.index());
        for window in terms.windows(2) {
            if window[0].predicate == window[1].predicate {
                return Err(PseudoBooleanError::DuplicatePredicate {
                    predicate: window[0].predicate,
                });
            }
        }
        Ok(Self {
            terms,
            relation,
            threshold,
            scale,
        })
    }

    /// Cardinality `sum(predicates) <= maximum`.
    pub fn at_most(
        predicates: impl IntoIterator<Item = PredicateId>,
        maximum: usize,
    ) -> Result<Self, PseudoBooleanError> {
        Self::cardinality(predicates, PseudoBooleanRelation::LessOrEqual, maximum)
    }

    /// Cardinality `sum(predicates) >= minimum`.
    pub fn at_least(
        predicates: impl IntoIterator<Item = PredicateId>,
        minimum: usize,
    ) -> Result<Self, PseudoBooleanError> {
        Self::cardinality(predicates, PseudoBooleanRelation::GreaterOrEqual, minimum)
    }

    /// Cardinality `sum(predicates) == exact`.
    pub fn exactly(
        predicates: impl IntoIterator<Item = PredicateId>,
        exact: usize,
    ) -> Result<Self, PseudoBooleanError> {
        Self::cardinality(predicates, PseudoBooleanRelation::Equal, exact)
    }

    fn cardinality(
        predicates: impl IntoIterator<Item = PredicateId>,
        relation: PseudoBooleanRelation,
        threshold: usize,
    ) -> Result<Self, PseudoBooleanError> {
        let threshold = i128::try_from(threshold)
            .map_err(|_| PseudoBooleanError::ThresholdOutOfRange { threshold })?;
        let terms = predicates
            .into_iter()
            .map(|predicate| WeightedPredicate::new(predicate, 1))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(terms, relation, threshold, PseudoBooleanScale::count())
    }

    /// Canonically ordered weighted terms.
    #[must_use]
    pub fn terms(&self) -> &[WeightedPredicate] {
        &self.terms
    }

    /// Constraint relation.
    #[must_use]
    pub const fn relation(&self) -> PseudoBooleanRelation {
        self.relation
    }

    /// Integer threshold in the declared scale's ticks.
    #[must_use]
    pub const fn threshold(&self) -> i128 {
        self.threshold
    }

    /// Explicit common unit/quantum.
    #[must_use]
    pub const fn scale(&self) -> &PseudoBooleanScale {
        &self.scale
    }

    /// Compute the minimum and maximum sum reachable from current partial facts.
    ///
    /// Arithmetic uses checked `i128`; overflow is an error, never saturation.
    pub fn bounds(&self, facts: &FactSet) -> Result<(i128, i128), PseudoBooleanError> {
        let mut minimum = 0_i128;
        let mut maximum = 0_i128;
        for term in &self.terms {
            match facts.get(term.predicate)? {
                TruthValue::True => {
                    minimum = checked_add(minimum, term.weight)?;
                    maximum = checked_add(maximum, term.weight)?;
                }
                TruthValue::False => {}
                TruthValue::Unknown => {
                    minimum = checked_add(minimum, term.weight.min(0))?;
                    maximum = checked_add(maximum, term.weight.max(0))?;
                }
            }
        }
        Ok((minimum, maximum))
    }

    /// Evaluate under partial facts using fail-closed three-valued semantics.
    ///
    /// `Unknown` means the current evidence does not establish the relation or
    /// its negation for every completion. In particular, equality may remain
    /// unknown when its threshold is inside the reachable interval; an exact
    /// small-domain oracle may refine such a query separately.
    pub fn evaluate(&self, facts: &FactSet) -> Result<TruthValue, PseudoBooleanError> {
        let (minimum, maximum) = self.bounds(facts)?;
        let value = match self.relation {
            PseudoBooleanRelation::LessOrEqual => {
                if maximum <= self.threshold {
                    TruthValue::True
                } else if minimum > self.threshold {
                    TruthValue::False
                } else {
                    TruthValue::Unknown
                }
            }
            PseudoBooleanRelation::GreaterOrEqual => {
                if minimum >= self.threshold {
                    TruthValue::True
                } else if maximum < self.threshold {
                    TruthValue::False
                } else {
                    TruthValue::Unknown
                }
            }
            PseudoBooleanRelation::Equal => {
                if minimum == maximum && minimum == self.threshold {
                    TruthValue::True
                } else if self.threshold < minimum || self.threshold > maximum {
                    TruthValue::False
                } else {
                    TruthValue::Unknown
                }
            }
        };
        Ok(value)
    }
}

/// Construction or evaluation failure for pseudo-Boolean constraints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PseudoBooleanError {
    EmptyUnit,
    UnitNotTrimmed,
    UnitTooLong { bytes: usize, maximum: usize },
    ZeroQuantum,
    ZeroWeight { predicate: PredicateId },
    PredicateOutOfRange { predicate: PredicateId },
    DuplicatePredicate { predicate: PredicateId },
    TooManyTerms { terms: usize, maximum: usize },
    ThresholdOutOfRange { threshold: usize },
    ArithmeticOverflow,
    Logic(LogicError),
}

impl fmt::Display for PseudoBooleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyUnit => write!(f, "pseudo-Boolean unit must not be empty"),
            Self::UnitNotTrimmed => write!(f, "pseudo-Boolean unit must be trimmed"),
            Self::UnitTooLong { bytes, maximum } => write!(
                f,
                "pseudo-Boolean unit length {bytes} exceeds maximum {maximum} bytes"
            ),
            Self::ZeroQuantum => write!(f, "pseudo-Boolean scale quantum must be non-zero"),
            Self::ZeroWeight { predicate } => write!(
                f,
                "pseudo-Boolean predicate {} has a zero weight",
                predicate.index()
            ),
            Self::PredicateOutOfRange { predicate } => write!(
                f,
                "pseudo-Boolean predicate {} exceeds current capacity {}",
                predicate.index(),
                FAST_PREDICATE_CAPACITY
            ),
            Self::DuplicatePredicate { predicate } => write!(
                f,
                "pseudo-Boolean predicate {} is declared more than once",
                predicate.index()
            ),
            Self::TooManyTerms { terms, maximum } => {
                write!(
                    f,
                    "pseudo-Boolean term count {terms} exceeds maximum {maximum}"
                )
            }
            Self::ThresholdOutOfRange { threshold } => write!(
                f,
                "cardinality threshold {threshold} cannot be represented as i128"
            ),
            Self::ArithmeticOverflow => write!(f, "pseudo-Boolean i128 accumulation overflow"),
            Self::Logic(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for PseudoBooleanError {}

impl From<LogicError> for PseudoBooleanError {
    fn from(value: LogicError) -> Self {
        Self::Logic(value)
    }
}

fn checked_add(lhs: i128, rhs: i128) -> Result<i128, PseudoBooleanError> {
    lhs.checked_add(rhs)
        .ok_or(PseudoBooleanError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: PredicateId = PredicateId::new(0);
    const B: PredicateId = PredicateId::new(1);
    const C: PredicateId = PredicateId::new(2);

    fn bytes() -> PseudoBooleanScale {
        PseudoBooleanScale::new("bytes", 1024).unwrap()
    }

    #[test]
    fn partial_positive_budget_is_true_false_or_unknown_only_when_established() {
        let constraint = PseudoBooleanConstraint::new(
            vec![
                WeightedPredicate::new(A, 4).unwrap(),
                WeightedPredicate::new(B, 3).unwrap(),
            ],
            PseudoBooleanRelation::LessOrEqual,
            5,
            bytes(),
        )
        .unwrap();

        let a_only = FactSet::new().with(A, TruthValue::True).unwrap();
        assert_eq!(constraint.bounds(&a_only).unwrap(), (4, 7));
        assert_eq!(constraint.evaluate(&a_only).unwrap(), TruthValue::Unknown);
        assert_eq!(
            constraint
                .evaluate(&a_only.with(B, TruthValue::False).unwrap())
                .unwrap(),
            TruthValue::True
        );
        assert_eq!(
            constraint
                .evaluate(&a_only.with(B, TruthValue::True).unwrap())
                .unwrap(),
            TruthValue::False
        );
    }

    #[test]
    fn signed_weights_produce_correct_partial_bounds() {
        let constraint = PseudoBooleanConstraint::new(
            vec![WeightedPredicate::new(A, -4).unwrap()],
            PseudoBooleanRelation::GreaterOrEqual,
            -2,
            PseudoBooleanScale::new("score", 1).unwrap(),
        )
        .unwrap();
        let unknown = FactSet::new();
        assert_eq!(constraint.bounds(&unknown).unwrap(), (-4, 0));
        assert_eq!(constraint.evaluate(&unknown).unwrap(), TruthValue::Unknown);
        assert_eq!(
            constraint
                .evaluate(&unknown.with(A, TruthValue::False).unwrap())
                .unwrap(),
            TruthValue::True
        );
        assert_eq!(
            constraint
                .evaluate(&unknown.with(A, TruthValue::True).unwrap())
                .unwrap(),
            TruthValue::False
        );
    }

    #[test]
    fn cardinality_helpers_preserve_partial_semantics() {
        let at_least_two = PseudoBooleanConstraint::at_least([A, B, C], 2).unwrap();
        let partial = FactSet::new()
            .with(A, TruthValue::True)
            .unwrap()
            .with(C, TruthValue::False)
            .unwrap();
        assert_eq!(
            at_least_two.evaluate(&partial).unwrap(),
            TruthValue::Unknown
        );
        assert_eq!(
            at_least_two
                .evaluate(&partial.with(B, TruthValue::True).unwrap())
                .unwrap(),
            TruthValue::True
        );
        assert_eq!(
            at_least_two
                .evaluate(&partial.with(B, TruthValue::False).unwrap())
                .unwrap(),
            TruthValue::False
        );
        assert_eq!(at_least_two.scale().unit(), "count");
        assert_eq!(at_least_two.scale().quantum(), 1);
    }

    #[test]
    fn equality_stays_unknown_when_interval_cannot_prove_reachability() {
        let constraint = PseudoBooleanConstraint::new(
            vec![
                WeightedPredicate::new(A, 2).unwrap(),
                WeightedPredicate::new(B, 2).unwrap(),
            ],
            PseudoBooleanRelation::Equal,
            1,
            PseudoBooleanScale::new("units", 1).unwrap(),
        )
        .unwrap();
        assert_eq!(
            constraint.evaluate(&FactSet::new()).unwrap(),
            TruthValue::Unknown
        );
        let grounded = FactSet::new()
            .with(A, TruthValue::False)
            .unwrap()
            .with(B, TruthValue::False)
            .unwrap();
        assert_eq!(constraint.evaluate(&grounded).unwrap(), TruthValue::False);
    }

    #[test]
    fn arithmetic_overflow_fails_closed() {
        let constraint = PseudoBooleanConstraint::new(
            vec![
                WeightedPredicate::new(A, i128::MAX).unwrap(),
                WeightedPredicate::new(B, 1).unwrap(),
            ],
            PseudoBooleanRelation::LessOrEqual,
            i128::MAX,
            PseudoBooleanScale::new("ticks", 1).unwrap(),
        )
        .unwrap();
        let facts = FactSet::new()
            .with(A, TruthValue::True)
            .unwrap()
            .with(B, TruthValue::True)
            .unwrap();
        assert_eq!(
            constraint.evaluate(&facts),
            Err(PseudoBooleanError::ArithmeticOverflow)
        );
    }

    #[test]
    fn malformed_terms_and_scales_fail_before_use() {
        assert_eq!(
            PseudoBooleanScale::new("", 1),
            Err(PseudoBooleanError::EmptyUnit)
        );
        assert_eq!(
            PseudoBooleanScale::new(" bytes", 1),
            Err(PseudoBooleanError::UnitNotTrimmed)
        );
        assert_eq!(
            PseudoBooleanScale::new("bytes", 0),
            Err(PseudoBooleanError::ZeroQuantum)
        );
        assert_eq!(
            WeightedPredicate::new(PredicateId::new(FAST_PREDICATE_CAPACITY), 1),
            Err(PseudoBooleanError::PredicateOutOfRange {
                predicate: PredicateId::new(FAST_PREDICATE_CAPACITY)
            })
        );
        let duplicate = PseudoBooleanConstraint::new(
            vec![
                WeightedPredicate::new(A, 1).unwrap(),
                WeightedPredicate::new(A, 2).unwrap(),
            ],
            PseudoBooleanRelation::LessOrEqual,
            1,
            PseudoBooleanScale::count(),
        );
        assert_eq!(
            duplicate,
            Err(PseudoBooleanError::DuplicatePredicate { predicate: A })
        );
    }
}
