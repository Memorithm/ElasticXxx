//! Bounded stable-order batch screening for multiword Boolean guards.
//!
//! A batch precompiles candidate guards once, then evaluates them against one
//! [`crate::MultiwordFactSet`] while preserving caller order and explicit
//! `True` / `False` / `Unknown` outcomes. Screening remains descriptive and
//! never authorizes validation or actuation.

use std::fmt;

use crate::{
    BoolExpr, MultiwordCompiledGuard, MultiwordFactError, MultiwordFactSet, MultiwordGuardError,
    TruthValue, MAX_MULTIWORD_FACT_PREDICATES,
};

/// Hard bound on candidate guards held by one precomputed batch.
pub const MAX_MULTIWORD_GUARDS_PER_BATCH: usize = 1024;

/// Errors from bounded multiword batch compilation or evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiwordBatchError {
    /// The caller supplied more candidates than the batch allocation bound.
    TooManyGuards { max: usize, actual: usize },
    /// One candidate guard or fact set failed closed.
    Guard(MultiwordGuardError),
}

impl fmt::Display for MultiwordBatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyGuards { max, actual } => {
                write!(
                    f,
                    "multiword guard batch contains {actual} guards; maximum is {max}"
                )
            }
            Self::Guard(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for MultiwordBatchError {}

impl From<MultiwordGuardError> for MultiwordBatchError {
    fn from(value: MultiwordGuardError) -> Self {
        Self::Guard(value)
    }
}

/// Precompiled stable-order candidate guard batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiwordGuardBatch {
    predicate_capacity: u32,
    guards: Vec<MultiwordCompiledGuard>,
}

impl MultiwordGuardBatch {
    /// Compile all candidate expressions in caller order.
    pub fn compile(
        expressions: &[BoolExpr],
        predicate_capacity: u32,
    ) -> Result<Self, MultiwordBatchError> {
        if expressions.len() > MAX_MULTIWORD_GUARDS_PER_BATCH {
            return Err(MultiwordBatchError::TooManyGuards {
                max: MAX_MULTIWORD_GUARDS_PER_BATCH,
                actual: expressions.len(),
            });
        }
        if predicate_capacity > MAX_MULTIWORD_FACT_PREDICATES {
            return Err(MultiwordBatchError::Guard(MultiwordGuardError::Facts(
                MultiwordFactError::CapacityTooLarge {
                    max: MAX_MULTIWORD_FACT_PREDICATES,
                    actual: predicate_capacity,
                },
            )));
        }
        let mut guards = Vec::with_capacity(expressions.len());
        for expression in expressions {
            guards.push(MultiwordCompiledGuard::compile(
                expression,
                predicate_capacity,
            )?);
        }
        Ok(Self {
            predicate_capacity,
            guards,
        })
    }

    /// Evaluate every candidate against the same fact snapshot in stable order.
    pub fn evaluate(
        &self,
        facts: &MultiwordFactSet,
    ) -> Result<MultiwordBatchScreen, MultiwordBatchError> {
        if facts.predicate_capacity() < self.predicate_capacity {
            return Err(MultiwordBatchError::Guard(
                MultiwordGuardError::FactCapacityTooSmall {
                    required: self.predicate_capacity,
                    actual: facts.predicate_capacity(),
                },
            ));
        }
        let mut outcomes = Vec::with_capacity(self.guards.len());
        for guard in &self.guards {
            outcomes.push(guard.evaluate(facts)?);
        }
        Ok(MultiwordBatchScreen { outcomes })
    }

    /// Number of precompiled candidates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.guards.len()
    }

    /// Whether the batch has no candidates.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.guards.is_empty()
    }

    /// Predicate domain shared by every compiled candidate.
    #[must_use]
    pub const fn predicate_capacity(&self) -> u32 {
        self.predicate_capacity
    }

    /// Precomputed guards in stable caller order.
    #[must_use]
    pub fn guards(&self) -> &[MultiwordCompiledGuard] {
        &self.guards
    }
}

/// Stable-order batch outcomes. `Unknown` is retained and never collapsed into `False`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiwordBatchScreen {
    outcomes: Vec<TruthValue>,
}

impl MultiwordBatchScreen {
    /// Outcomes in exactly the same order as the compiled candidates.
    #[must_use]
    pub fn outcomes(&self) -> &[TruthValue] {
        &self.outcomes
    }

    /// Number of candidate outcomes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.outcomes.len()
    }

    /// Whether no candidate was screened.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outcomes.is_empty()
    }

    /// Stable indices whose guards evaluated to `True`.
    pub fn true_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.indices_with(TruthValue::True)
    }

    /// Stable indices whose guards evaluated to `False`.
    pub fn false_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.indices_with(TruthValue::False)
    }

    /// Stable indices whose guards remain `Unknown`.
    pub fn unknown_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.indices_with(TruthValue::Unknown)
    }

    fn indices_with(&self, expected: TruthValue) -> impl Iterator<Item = usize> + '_ {
        self.outcomes
            .iter()
            .enumerate()
            .filter_map(move |(index, value)| (*value == expected).then_some(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MultiwordFactError, PredicateId};

    const A: PredicateId = PredicateId::new(1);
    const B: PredicateId = PredicateId::new(64);
    const C: PredicateId = PredicateId::new(129);

    fn facts(values: &[(PredicateId, TruthValue)]) -> MultiwordFactSet {
        let mut facts = MultiwordFactSet::new(130).unwrap();
        for (id, value) in values {
            facts.set(*id, *value).unwrap();
        }
        facts
    }

    #[test]
    fn batch_preserves_candidate_order_and_unknown() {
        let expressions = vec![
            BoolExpr::atom(B),
            BoolExpr::all([BoolExpr::atom(A), BoolExpr::atom(C)]),
            BoolExpr::negate(BoolExpr::atom(A)),
        ];
        let batch = MultiwordGuardBatch::compile(&expressions, 130).unwrap();
        let screen = batch
            .evaluate(&facts(&[(A, TruthValue::True), (B, TruthValue::True)]))
            .unwrap();
        assert_eq!(
            screen.outcomes(),
            &[TruthValue::True, TruthValue::Unknown, TruthValue::False]
        );
        assert_eq!(screen.true_indices().collect::<Vec<_>>(), vec![0]);
        assert_eq!(screen.unknown_indices().collect::<Vec<_>>(), vec![1]);
        assert_eq!(screen.false_indices().collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn batch_matches_individual_precompiled_guards() {
        let expressions = vec![
            BoolExpr::all([BoolExpr::atom(A), BoolExpr::negate(BoolExpr::atom(B))]),
            BoolExpr::any([BoolExpr::atom(B), BoolExpr::atom(C)]),
            BoolExpr::Xor(Box::new(BoolExpr::atom(A)), Box::new(BoolExpr::atom(C))),
        ];
        let batch = MultiwordGuardBatch::compile(&expressions, 130).unwrap();
        let facts = facts(&[
            (A, TruthValue::False),
            (B, TruthValue::False),
            (C, TruthValue::True),
        ]);
        let expected = expressions
            .iter()
            .map(|expression| {
                MultiwordCompiledGuard::compile(expression, 130)
                    .unwrap()
                    .evaluate(&facts)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(batch.evaluate(&facts).unwrap().outcomes(), expected);
    }

    #[test]
    fn oversize_batch_fails_before_compilation() {
        let expressions = vec![BoolExpr::Const(true); MAX_MULTIWORD_GUARDS_PER_BATCH + 1];
        assert_eq!(
            MultiwordGuardBatch::compile(&expressions, 0),
            Err(MultiwordBatchError::TooManyGuards {
                max: MAX_MULTIWORD_GUARDS_PER_BATCH,
                actual: MAX_MULTIWORD_GUARDS_PER_BATCH + 1,
            })
        );
    }

    #[test]
    fn invalid_candidate_fails_the_whole_batch_closed() {
        let expressions = vec![BoolExpr::Const(true), BoolExpr::atom(C)];
        assert_eq!(
            MultiwordGuardBatch::compile(&expressions, 129),
            Err(MultiwordBatchError::Guard(MultiwordGuardError::Facts(
                MultiwordFactError::PredicateOutOfRange {
                    id: C,
                    capacity: 129,
                }
            )))
        );
    }

    #[test]
    fn too_small_fact_domain_fails_without_partial_screen() {
        let batch = MultiwordGuardBatch::compile(&[BoolExpr::atom(C)], 130).unwrap();
        let too_small = MultiwordFactSet::new(129).unwrap();
        assert_eq!(
            batch.evaluate(&too_small),
            Err(MultiwordBatchError::Guard(
                MultiwordGuardError::FactCapacityTooSmall {
                    required: 130,
                    actual: 129,
                }
            ))
        );
    }

    #[test]
    fn empty_batch_still_rejects_unrepresentable_domain() {
        assert_eq!(
            MultiwordGuardBatch::compile(&[], MAX_MULTIWORD_FACT_PREDICATES + 1),
            Err(MultiwordBatchError::Guard(MultiwordGuardError::Facts(
                MultiwordFactError::CapacityTooLarge {
                    max: MAX_MULTIWORD_FACT_PREDICATES,
                    actual: MAX_MULTIWORD_FACT_PREDICATES + 1,
                }
            )))
        );
    }

    #[test]
    fn empty_batch_still_requires_declared_fact_domain() {
        let batch = MultiwordGuardBatch::compile(&[], 130).unwrap();
        let too_small = MultiwordFactSet::new(129).unwrap();
        assert_eq!(
            batch.evaluate(&too_small),
            Err(MultiwordBatchError::Guard(
                MultiwordGuardError::FactCapacityTooSmall {
                    required: 130,
                    actual: 129,
                }
            ))
        );
    }

    #[test]
    fn empty_batch_is_valid_and_stable() {
        let batch = MultiwordGuardBatch::compile(&[], 0).unwrap();
        assert!(batch.is_empty());
        let screen = batch.evaluate(&MultiwordFactSet::new(0).unwrap()).unwrap();
        assert!(screen.is_empty());
    }
}
