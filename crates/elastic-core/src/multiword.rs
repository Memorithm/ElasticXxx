//! Bounded multiword three-valued facts for Boolean policy evaluation.
//!
//! The historical [`crate::FactSet`] remains the dependency-free `u64` fast
//! path. This module extends fact storage beyond 64 compact predicate IDs while
//! preserving the same [`crate::PredicateId`] identity and explicit
//! [`crate::TruthValue::Unknown`] semantics.

use std::fmt;

use crate::{PredicateId, TruthValue};

/// Number of bits carried by one portable fact word.
pub const MULTIWORD_FACT_WORD_BITS: u32 = u64::BITS;

/// Hard allocation bound for one multiword fact set.
pub const MAX_MULTIWORD_FACT_WORDS: usize = 64;

/// Maximum predicate capacity admitted by [`MultiwordFactSet`].
pub const MAX_MULTIWORD_FACT_PREDICATES: u32 =
    (MAX_MULTIWORD_FACT_WORDS as u32) * MULTIWORD_FACT_WORD_BITS;

/// Errors produced by bounded multiword fact storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiwordFactError {
    /// Requested logical capacity exceeds the hard allocation limit.
    CapacityTooLarge { max: u32, actual: u32 },
    /// A predicate lies outside the capacity declared for this fact set.
    PredicateOutOfRange { id: PredicateId, capacity: u32 },
}

impl fmt::Display for MultiwordFactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityTooLarge { max, actual } => {
                write!(f, "multiword fact capacity {actual} exceeds maximum {max}")
            }
            Self::PredicateOutOfRange { id, capacity } => write!(
                f,
                "predicate {} exceeds declared multiword fact capacity {capacity}",
                id.index()
            ),
        }
    }
}

impl std::error::Error for MultiwordFactError {}

/// One portable word of mutually exclusive True/False/Unknown masks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MultiwordFactWord {
    known_true: u64,
    known_false: u64,
    unknown: u64,
}

impl MultiwordFactWord {
    /// Predicates known true in this word.
    #[must_use]
    pub const fn known_true(self) -> u64 {
        self.known_true
    }

    /// Predicates known false in this word.
    #[must_use]
    pub const fn known_false(self) -> u64 {
        self.known_false
    }

    /// Predicates whose value is unknown in this word.
    #[must_use]
    pub const fn unknown(self) -> u64 {
        self.unknown
    }

    /// Whether the three masks are pairwise disjoint.
    #[must_use]
    pub const fn is_disjoint(self) -> bool {
        self.known_true & self.known_false == 0
            && self.known_true & self.unknown == 0
            && self.known_false & self.unknown == 0
    }
}

/// Bounded multiword store for three-valued predicate facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiwordFactSet {
    predicate_capacity: u32,
    known_true: Vec<u64>,
    known_false: Vec<u64>,
}

impl MultiwordFactSet {
    /// Allocate storage for up to `predicate_capacity` compact predicate IDs.
    pub fn new(predicate_capacity: u32) -> Result<Self, MultiwordFactError> {
        if predicate_capacity > MAX_MULTIWORD_FACT_PREDICATES {
            return Err(MultiwordFactError::CapacityTooLarge {
                max: MAX_MULTIWORD_FACT_PREDICATES,
                actual: predicate_capacity,
            });
        }
        let words = usize::try_from(predicate_capacity.div_ceil(MULTIWORD_FACT_WORD_BITS))
            .expect("bounded multiword capacity always fits usize");
        Ok(Self {
            predicate_capacity,
            known_true: vec![0; words],
            known_false: vec![0; words],
        })
    }

    /// Declared logical predicate capacity.
    #[must_use]
    pub const fn predicate_capacity(&self) -> u32 {
        self.predicate_capacity
    }

    /// Number of allocated words.
    #[must_use]
    pub fn word_len(&self) -> usize {
        self.known_true.len()
    }

    /// Immutable raw true-mask words for later compiled fast paths.
    #[must_use]
    pub fn known_true_words(&self) -> &[u64] {
        &self.known_true
    }

    /// Immutable raw false-mask words for later compiled fast paths.
    #[must_use]
    pub fn known_false_words(&self) -> &[u64] {
        &self.known_false
    }

    /// Set one predicate while preserving pairwise-disjoint masks.
    pub fn set(&mut self, id: PredicateId, value: TruthValue) -> Result<(), MultiwordFactError> {
        let (word, bit) = self.location(id)?;
        let mask = 1_u64 << bit;
        self.known_true[word] &= !mask;
        self.known_false[word] &= !mask;
        match value {
            TruthValue::True => self.known_true[word] |= mask,
            TruthValue::False => self.known_false[word] |= mask,
            TruthValue::Unknown => {}
        }
        Ok(())
    }

    /// Builder-style form of [`MultiwordFactSet::set`].
    pub fn with(mut self, id: PredicateId, value: TruthValue) -> Result<Self, MultiwordFactError> {
        self.set(id, value)?;
        Ok(self)
    }

    /// Read one predicate value.
    pub fn get(&self, id: PredicateId) -> Result<TruthValue, MultiwordFactError> {
        let (word, bit) = self.location(id)?;
        let mask = 1_u64 << bit;
        if self.known_true[word] & mask != 0 {
            Ok(TruthValue::True)
        } else if self.known_false[word] & mask != 0 {
            Ok(TruthValue::False)
        } else {
            Ok(TruthValue::Unknown)
        }
    }

    /// Return the three disjoint masks for one allocated word.
    #[must_use]
    pub fn word_masks(&self, word: usize) -> Option<MultiwordFactWord> {
        let known_true = *self.known_true.get(word)?;
        let known_false = *self.known_false.get(word)?;
        let valid = self.valid_mask(word)?;
        let unknown = valid & !(known_true | known_false);
        let masks = MultiwordFactWord {
            known_true,
            known_false,
            unknown,
        };
        debug_assert!(masks.is_disjoint());
        Some(masks)
    }

    fn location(&self, id: PredicateId) -> Result<(usize, u32), MultiwordFactError> {
        if id.index() >= self.predicate_capacity {
            return Err(MultiwordFactError::PredicateOutOfRange {
                id,
                capacity: self.predicate_capacity,
            });
        }
        let word = usize::try_from(id.index() / MULTIWORD_FACT_WORD_BITS)
            .expect("bounded predicate index always fits usize");
        let bit = id.index() % MULTIWORD_FACT_WORD_BITS;
        Ok((word, bit))
    }

    fn valid_mask(&self, word: usize) -> Option<u64> {
        if word >= self.word_len() {
            return None;
        }
        let start = word.checked_mul(MULTIWORD_FACT_WORD_BITS as usize)?;
        let capacity = usize::try_from(self.predicate_capacity).ok()?;
        let remaining = capacity.saturating_sub(start);
        Some(if remaining >= MULTIWORD_FACT_WORD_BITS as usize {
            u64::MAX
        } else if remaining == 0 {
            0
        } else {
            (1_u64 << remaining) - 1
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn represents_predicates_beyond_single_word() {
        let mut facts = MultiwordFactSet::new(130).unwrap();
        for (index, value) in [
            (0, TruthValue::True),
            (63, TruthValue::False),
            (64, TruthValue::True),
            (65, TruthValue::False),
            (127, TruthValue::True),
            (129, TruthValue::False),
        ] {
            facts.set(PredicateId::new(index), value).unwrap();
            assert_eq!(facts.get(PredicateId::new(index)).unwrap(), value);
        }
        assert_eq!(facts.word_len(), 3);
        assert_eq!(
            facts.get(PredicateId::new(128)).unwrap(),
            TruthValue::Unknown
        );
    }

    #[test]
    fn setting_unknown_clears_prior_fact() {
        let id = PredicateId::new(64);
        let mut facts = MultiwordFactSet::new(65).unwrap();
        facts.set(id, TruthValue::True).unwrap();
        facts.set(id, TruthValue::Unknown).unwrap();
        assert_eq!(facts.get(id).unwrap(), TruthValue::Unknown);
    }

    #[test]
    fn word_masks_are_disjoint_and_bound_unknown_tail() {
        let facts = MultiwordFactSet::new(65)
            .unwrap()
            .with(PredicateId::new(0), TruthValue::True)
            .unwrap()
            .with(PredicateId::new(64), TruthValue::False)
            .unwrap();
        let first = facts.word_masks(0).unwrap();
        assert!(first.is_disjoint());
        assert_eq!(first.known_true(), 1);
        assert_eq!(first.known_false(), 0);
        assert_eq!(first.unknown(), u64::MAX ^ 1);
        let second = facts.word_masks(1).unwrap();
        assert!(second.is_disjoint());
        assert_eq!(second.known_true(), 0);
        assert_eq!(second.known_false(), 1);
        assert_eq!(second.unknown(), 0);
        assert!(facts.word_masks(2).is_none());
    }

    #[test]
    fn capacity_and_predicate_bounds_fail_closed() {
        assert_eq!(
            MultiwordFactSet::new(MAX_MULTIWORD_FACT_PREDICATES + 1),
            Err(MultiwordFactError::CapacityTooLarge {
                max: MAX_MULTIWORD_FACT_PREDICATES,
                actual: MAX_MULTIWORD_FACT_PREDICATES + 1,
            })
        );
        let mut facts = MultiwordFactSet::new(65).unwrap();
        let id = PredicateId::new(65);
        assert_eq!(
            facts.set(id, TruthValue::True),
            Err(MultiwordFactError::PredicateOutOfRange { id, capacity: 65 })
        );
    }

    #[test]
    fn zero_capacity_allocates_no_words() {
        let facts = MultiwordFactSet::new(0).unwrap();
        assert_eq!(facts.word_len(), 0);
        assert!(facts.known_true_words().is_empty());
        assert!(facts.known_false_words().is_empty());
    }

    #[test]
    fn predicate_identity_is_unchanged_across_word_boundaries() {
        let id = PredicateId::new(130);
        let facts = MultiwordFactSet::new(131)
            .unwrap()
            .with(id, TruthValue::True)
            .unwrap();
        assert_eq!(id.index(), 130);
        assert_eq!(facts.get(id).unwrap(), TruthValue::True);
    }
}
