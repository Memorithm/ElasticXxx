//! Versioned, representation-only TDI-9.3 Boolean interop boundary.
//!
//! TDI owns the C3 carrier, policy/action semantics, preregistration, holdout,
//! confirmation and scientific verdicts. This module only maps the nine
//! versioned abstract predicate positions onto ElasticXxx stable predicate keys
//! and [`TruthValue`] values. It deliberately exposes no TDI action API.
//!
//! TDI's v1 source carrier is binary and cannot represent missing evidence.
//! Missing downstream evidence therefore maps to [`TruthValue::Unknown`], never
//! to `False`; a value containing `Unknown` cannot be projected back to the
//! binary carrier.

use elastic_core::{PredicateKey, PredicateRegistryError, TruthValue};
use std::collections::BTreeMap;

/// TDI source contract consumed by this adapter.
pub const TDI93_C3_INTEROP_SCHEMA_V1: &str = "tdi9.3.elasticxxx-c3-carrier.v1";
/// Exact merged TDI revision that supplied the pinned BE15b fixture.
pub const TDI93_C3_SOURCE_COMMIT_V1: &str = "7dab3bfa97e74eeff7965cefcada56c59b4322ab";
/// Stable namespace used for the nine TDI-9.3 predicate positions.
pub const TDI93_C3_PREDICATE_NAMESPACE_V1: &str = "tdi.9.3.c3.v1";
/// Number of abstract predicates in the versioned C3 carrier.
pub const TDI93_C3_PREDICATE_COUNT_V1: usize = 9;

/// Stable predicate positions from TDI's C3 v1 representation contract.
///
/// The discriminants intentionally match TDI's exported p0→p8 bit order. They
/// are representation positions only and carry no action or scientific
/// authority in ElasticXxx.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Tdi93C3PredicateV1 {
    BaseStop = 0,
    VerifyBeforeStop = 1,
    CadenceDue = 2,
    CheckpointAvailable = 3,
    RemainingWork = 4,
    VerifierViolated = 5,
    VerifierSatisfied = 6,
    VerifierIndeterminate = 7,
    VerifierAbsent = 8,
}

impl Tdi93C3PredicateV1 {
    /// All versioned predicate positions in exact TDI p0→p8 order.
    pub const ALL: [Self; TDI93_C3_PREDICATE_COUNT_V1] = [
        Self::BaseStop,
        Self::VerifyBeforeStop,
        Self::CadenceDue,
        Self::CheckpointAvailable,
        Self::RemainingWork,
        Self::VerifierViolated,
        Self::VerifierSatisfied,
        Self::VerifierIndeterminate,
        Self::VerifierAbsent,
    ];

    /// Exact source bit position.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Stable Elastic predicate-name component for this source position.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BaseStop => "base-stop",
            Self::VerifyBeforeStop => "verify-before-stop",
            Self::CadenceDue => "cadence-due",
            Self::CheckpointAvailable => "checkpoint-available",
            Self::RemainingWork => "remaining-work",
            Self::VerifierViolated => "verifier-violated",
            Self::VerifierSatisfied => "verifier-satisfied",
            Self::VerifierIndeterminate => "verifier-indeterminate",
            Self::VerifierAbsent => "verifier-absent",
        }
    }

    /// Construct the canonical stable Elastic key for this source position.
    ///
    /// # Errors
    ///
    /// Propagates stable-key validation failures rather than fabricating an ID.
    pub fn key(self) -> Result<PredicateKey, PredicateRegistryError> {
        PredicateKey::new(TDI93_C3_PREDICATE_NAMESPACE_V1, self.name())
    }
}

/// Elastic three-valued representation of one TDI-9.3 C3 predicate row.
///
/// This type does not validate the TDI carrier's verifier one-hot invariant and
/// does not derive a TDI action. Those semantics remain owned by TDI. Its sole
/// purpose is byte/order-stable representation adaptation into Elastic truth
/// values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tdi93C3FactsV1 {
    values: [TruthValue; TDI93_C3_PREDICATE_COUNT_V1],
}

impl Tdi93C3FactsV1 {
    /// Adapt a fully grounded TDI binary carrier row losslessly.
    #[must_use]
    pub fn from_binary(values: [bool; TDI93_C3_PREDICATE_COUNT_V1]) -> Self {
        Self {
            values: values.map(|value| {
                if value {
                    TruthValue::True
                } else {
                    TruthValue::False
                }
            }),
        }
    }

    /// Adapt a possibly incomplete downstream observation.
    ///
    /// `None` is represented as Elastic [`TruthValue::Unknown`]. It is never
    /// interpreted as the TDI binary value `false`.
    #[must_use]
    pub fn from_optional(values: [Option<bool>; TDI93_C3_PREDICATE_COUNT_V1]) -> Self {
        Self {
            values: values.map(|value| match value {
                Some(true) => TruthValue::True,
                Some(false) => TruthValue::False,
                None => TruthValue::Unknown,
            }),
        }
    }

    /// Read one adapted predicate position.
    #[must_use]
    pub const fn get(&self, predicate: Tdi93C3PredicateV1) -> TruthValue {
        self.values[predicate.index()]
    }

    /// Borrow all adapted values in exact source p0→p8 order.
    #[must_use]
    pub const fn values(&self) -> &[TruthValue; TDI93_C3_PREDICATE_COUNT_V1] {
        &self.values
    }

    /// Whether at least one source predicate is missing.
    #[must_use]
    pub fn has_unknown(&self) -> bool {
        self.values.contains(&TruthValue::Unknown)
    }

    /// Project back to the TDI binary representation only when fully grounded.
    ///
    /// `None` means at least one predicate is `Unknown`; callers must fail closed
    /// rather than inventing a TDI binary row.
    #[must_use]
    pub fn try_binary(&self) -> Option<[bool; TDI93_C3_PREDICATE_COUNT_V1]> {
        let mut binary = [false; TDI93_C3_PREDICATE_COUNT_V1];
        for (index, value) in self.values.iter().enumerate() {
            binary[index] = match value {
                TruthValue::True => true,
                TruthValue::False => false,
                TruthValue::Unknown => return None,
            };
        }
        Some(binary)
    }

    /// Materialize stable-key Elastic facts without using ephemeral PredicateId.
    ///
    /// # Errors
    ///
    /// Propagates stable predicate-key validation errors.
    pub fn fact_map(&self) -> Result<BTreeMap<PredicateKey, TruthValue>, PredicateRegistryError> {
        Tdi93C3PredicateV1::ALL
            .into_iter()
            .map(|predicate| predicate.key().map(|key| (key, self.get(predicate))))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_keys_and_source_order_are_explicit() {
        for (index, predicate) in Tdi93C3PredicateV1::ALL.into_iter().enumerate() {
            assert_eq!(predicate.index(), index);
            let key = predicate.key().unwrap();
            assert_eq!(key.namespace(), TDI93_C3_PREDICATE_NAMESPACE_V1);
            assert_eq!(key.name(), predicate.name());
        }
    }

    #[test]
    fn unknown_never_projects_to_tdi_false() {
        let mut source = [Some(false); TDI93_C3_PREDICATE_COUNT_V1];
        source[Tdi93C3PredicateV1::CadenceDue.index()] = None;
        let facts = Tdi93C3FactsV1::from_optional(source);

        assert_eq!(
            facts.get(Tdi93C3PredicateV1::CadenceDue),
            TruthValue::Unknown
        );
        assert!(facts.has_unknown());
        assert_eq!(facts.try_binary(), None);
    }
}
