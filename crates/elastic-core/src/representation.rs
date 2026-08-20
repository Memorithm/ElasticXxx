use std::collections::BTreeSet;
use std::fmt;

/// Stable identifier for a mathematical or numerical representation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepresentationId(String);

impl RepresentationId {
    /// Construct a non-empty representation identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, TransitionError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(TransitionError::EmptyRepresentationId);
        }
        Ok(Self(value))
    }

    /// Borrow the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Monotonic epoch attached to materialized representation state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepresentationEpoch(u64);

impl RepresentationEpoch {
    /// Construct an epoch from its raw counter.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Raw epoch counter.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Checked successor.
    pub fn next(self) -> Result<Self, TransitionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(TransitionError::EpochOverflow)
    }
}

/// Versioned materialized representation state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationState {
    /// Representation family identifier.
    pub id: RepresentationId,
    /// Schema/contract version understood by the producer and consumer.
    pub schema_version: u32,
    /// Materialization epoch.
    pub epoch: RepresentationEpoch,
}

impl RepresentationState {
    /// Construct a representation state.
    pub const fn new(
        id: RepresentationId,
        schema_version: u32,
        epoch: RepresentationEpoch,
    ) -> Self {
        Self {
            id,
            schema_version,
            epoch,
        }
    }

    /// Whether two states use the same representation contract independent of epoch.
    pub fn same_contract(&self, rhs: &Self) -> bool {
        self.id == rhs.id && self.schema_version == rhs.schema_version
    }
}

/// Set of representation contracts explicitly declared as supported by a
/// model/runtime boundary.
///
/// This type records a capability declaration; it does not authenticate who
/// made that declaration. A trusted runtime must construct capability snapshots
/// from authoritative discovery/configuration rather than untrusted application
/// input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    entries: BTreeSet<(RepresentationId, u32)>,
}

impl CapabilitySet {
    /// Empty capability set.
    pub const fn new() -> Self {
        Self {
            entries: BTreeSet::new(),
        }
    }

    /// Add one declared supported representation contract.
    pub fn insert(&mut self, id: RepresentationId, schema_version: u32) {
        self.entries.insert((id, schema_version));
    }

    /// Check whether a representation contract is explicitly declared supported.
    pub fn supports(&self, state: &RepresentationState) -> bool {
        self.entries
            .contains(&(state.id.clone(), state.schema_version))
    }

    /// Number of declared contracts.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no contracts are declared.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Mechanism used to materialize the target representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionMechanism {
    /// Reuse bytes/storage without changing their interpretation.
    /// This is legal across representation contracts only when the trusted
    /// boundary supplies a semantic-equivalence attestation.
    Reinterpret,
    /// Transform the existing materialization into the target representation.
    Reencode,
    /// Regenerate the target representation from a trusted source state.
    Recompute,
}

/// Explicit attestations consumed by structural transition validation.
///
/// These are **claims from a trusted adapter/runtime boundary**, not proofs
/// authenticated by `elastic-core`. The private fields prevent accidental raw
/// boolean construction and make call sites name each trust decision, but the
/// public attestation methods deliberately do not pretend to verify the truth of
/// the claim. A future trusted-validator layer may issue stronger provenance-
/// carrying evidence tokens without changing the transition semantics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransitionAttestations {
    semantic_equivalence: bool,
    reencoder_available: bool,
    recompute_source_available: bool,
}

impl TransitionAttestations {
    /// No positive attestations.
    pub const fn none() -> Self {
        Self {
            semantic_equivalence: false,
            reencoder_available: false,
            recompute_source_available: false,
        }
    }

    /// Attest that a trusted contract/evidence source establishes semantic
    /// equivalence for the requested reinterpretation.
    #[must_use]
    pub const fn attest_semantic_equivalence(mut self) -> Self {
        self.semantic_equivalence = true;
        self
    }

    /// Attest that a re-encoder exists for the exact requested source/target
    /// representation transition.
    #[must_use]
    pub const fn attest_reencoder_available(mut self) -> Self {
        self.reencoder_available = true;
        self
    }

    /// Attest that a trusted source exists from which the target can be
    /// recomputed.
    #[must_use]
    pub const fn attest_recompute_source_available(mut self) -> Self {
        self.recompute_source_available = true;
        self
    }

    const fn semantic_equivalence_attested(self) -> bool {
        self.semantic_equivalence
    }

    const fn reencoder_attested(self) -> bool {
        self.reencoder_available
    }

    const fn recompute_source_attested(self) -> bool {
        self.recompute_source_available
    }
}

/// Requested transition between two materialized representation states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationTransition {
    /// Current materialized state.
    pub from: RepresentationState,
    /// Requested target state.
    pub to: RepresentationState,
    /// Mechanism proposed by the runtime.
    pub mechanism: TransitionMechanism,
}

impl RepresentationTransition {
    /// Structurally validate this transition against declared capabilities and
    /// trusted-boundary attestations.
    ///
    /// Any contract-changing transition and any transition that creates a new
    /// materialization (`Reencode` or `Recompute`) must advance the
    /// representation epoch. Silent cross-contract reinterpretation is rejected
    /// unless semantic equivalence is explicitly attested. Re-encoding and
    /// recomputation require corresponding availability attestations.
    ///
    /// This function validates **consistency of the supplied declarations**; it
    /// does not authenticate capability discovery or prove the attestations.
    /// Those belong to the trusted adapter/runtime boundary.
    pub fn validate(
        &self,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
    ) -> Result<(), TransitionError> {
        if !capabilities.supports(&self.to) {
            return Err(TransitionError::UnsupportedTarget {
                id: self.to.id.clone(),
                schema_version: self.to.schema_version,
            });
        }

        let contract_changes = !self.from.same_contract(&self.to);
        let creates_new_materialization = matches!(
            self.mechanism,
            TransitionMechanism::Reencode | TransitionMechanism::Recompute
        );
        let must_advance_epoch = contract_changes || creates_new_materialization;

        if self.to.epoch < self.from.epoch {
            return Err(TransitionError::EpochRegression {
                from: self.from.epoch,
                to: self.to.epoch,
            });
        }
        if must_advance_epoch && self.to.epoch == self.from.epoch {
            return Err(TransitionError::EpochMustAdvance {
                from: self.from.epoch,
                to: self.to.epoch,
            });
        }

        match self.mechanism {
            TransitionMechanism::Reinterpret => {
                if contract_changes && !attestations.semantic_equivalence_attested() {
                    return Err(TransitionError::MissingSemanticEquivalenceAttestation);
                }
            }
            TransitionMechanism::Reencode => {
                if !attestations.reencoder_attested() {
                    return Err(TransitionError::MissingReencoderAttestation);
                }
            }
            TransitionMechanism::Recompute => {
                if !attestations.recompute_source_attested() {
                    return Err(TransitionError::MissingRecomputeSourceAttestation);
                }
            }
        }
        Ok(())
    }
}

/// Validation errors for representational-resource transitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionError {
    /// Representation identifiers may not be blank.
    EmptyRepresentationId,
    /// Epoch counter cannot be advanced further.
    EpochOverflow,
    /// Target representation is not declared as supported.
    UnsupportedTarget {
        /// Target identifier.
        id: RepresentationId,
        /// Target schema version.
        schema_version: u32,
    },
    /// A transition that changes the contract or creates a new materialization
    /// failed to advance the epoch.
    EpochMustAdvance {
        /// Source epoch.
        from: RepresentationEpoch,
        /// Target epoch.
        to: RepresentationEpoch,
    },
    /// A transition regressed the materialization epoch.
    EpochRegression {
        /// Source epoch.
        from: RepresentationEpoch,
        /// Target epoch.
        to: RepresentationEpoch,
    },
    /// Cross-contract reinterpretation lacks a semantic-equivalence attestation.
    MissingSemanticEquivalenceAttestation,
    /// Requested re-encoding lacks an attestation that the mechanism exists.
    MissingReencoderAttestation,
    /// Requested recomputation lacks an attested trusted source.
    MissingRecomputeSourceAttestation,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRepresentationId => write!(f, "representation identifier must not be empty"),
            Self::EpochOverflow => write!(f, "representation epoch overflow"),
            Self::UnsupportedTarget { id, schema_version } => write!(
                f,
                "target representation {} v{} is not declared supported",
                id.as_str(),
                schema_version
            ),
            Self::EpochMustAdvance { from, to } => write!(
                f,
                "transition must advance representation epoch ({} -> {})",
                from.get(),
                to.get()
            ),
            Self::EpochRegression { from, to } => write!(
                f,
                "representation epoch regressed ({} -> {})",
                from.get(),
                to.get()
            ),
            Self::MissingSemanticEquivalenceAttestation => write!(
                f,
                "cross-contract reinterpretation requires a trusted-boundary semantic-equivalence attestation"
            ),
            Self::MissingReencoderAttestation => write!(
                f,
                "requested representation transition lacks a trusted-boundary re-encoder attestation"
            ),
            Self::MissingRecomputeSourceAttestation => write!(
                f,
                "requested representation transition lacks a trusted-boundary recompute-source attestation"
            ),
        }
    }
}

impl std::error::Error for TransitionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> RepresentationId {
        RepresentationId::new(name).unwrap()
    }

    #[test]
    fn geometry_like_change_requires_epoch_and_reencoder_attestation() {
        let from = RepresentationState::new(id("epg.so2"), 1, RepresentationEpoch::new(7));
        let to = RepresentationState::new(id("epg.so4.structural"), 1, RepresentationEpoch::new(8));
        let mut caps = CapabilitySet::new();
        caps.insert(to.id.clone(), 1);
        let transition = RepresentationTransition {
            from,
            to,
            mechanism: TransitionMechanism::Reencode,
        };
        assert_eq!(
            transition.validate(&caps, TransitionAttestations::default()),
            Err(TransitionError::MissingReencoderAttestation)
        );
        assert!(transition
            .validate(
                &caps,
                TransitionAttestations::default().attest_reencoder_available(),
            )
            .is_ok());
    }

    #[test]
    fn same_contract_reencode_requires_new_materialization_epoch() {
        let from = RepresentationState::new(id("kv.int4"), 1, RepresentationEpoch::new(3));
        let to = RepresentationState::new(id("kv.int4"), 1, RepresentationEpoch::new(3));
        let mut caps = CapabilitySet::new();
        caps.insert(to.id.clone(), 1);
        let transition = RepresentationTransition {
            from,
            to,
            mechanism: TransitionMechanism::Reencode,
        };

        assert_eq!(
            transition.validate(
                &caps,
                TransitionAttestations::default().attest_reencoder_available(),
            ),
            Err(TransitionError::EpochMustAdvance {
                from: RepresentationEpoch::new(3),
                to: RepresentationEpoch::new(3),
            })
        );
    }

    #[test]
    fn epoch_regression_is_reported_before_missing_advance() {
        let from = RepresentationState::new(id("kv.int4"), 1, RepresentationEpoch::new(4));
        let to = RepresentationState::new(id("kv.int4"), 1, RepresentationEpoch::new(3));
        let mut caps = CapabilitySet::new();
        caps.insert(to.id.clone(), 1);
        let transition = RepresentationTransition {
            from,
            to,
            mechanism: TransitionMechanism::Reencode,
        };

        assert_eq!(
            transition.validate(
                &caps,
                TransitionAttestations::default().attest_reencoder_available(),
            ),
            Err(TransitionError::EpochRegression {
                from: RepresentationEpoch::new(4),
                to: RepresentationEpoch::new(3),
            })
        );
    }

    #[test]
    fn same_contract_reinterpretation_may_keep_epoch() {
        let from = RepresentationState::new(id("kv.raw"), 1, RepresentationEpoch::new(3));
        let to = RepresentationState::new(id("kv.raw"), 1, RepresentationEpoch::new(3));
        let mut caps = CapabilitySet::new();
        caps.insert(to.id.clone(), 1);
        let transition = RepresentationTransition {
            from,
            to,
            mechanism: TransitionMechanism::Reinterpret,
        };

        assert!(
            transition
                .validate(&caps, TransitionAttestations::default())
                .is_ok()
        );
    }

    #[test]
    fn cross_contract_reinterpretation_requires_explicit_attestation() {
        let from = RepresentationState::new(id("a"), 1, RepresentationEpoch::new(1));
        let to = RepresentationState::new(id("b"), 1, RepresentationEpoch::new(2));
        let mut caps = CapabilitySet::new();
        caps.insert(to.id.clone(), 1);
        let transition = RepresentationTransition {
            from,
            to,
            mechanism: TransitionMechanism::Reinterpret,
        };
        assert_eq!(
            transition.validate(&caps, TransitionAttestations::default()),
            Err(TransitionError::MissingSemanticEquivalenceAttestation)
        );
        assert!(transition
            .validate(
                &caps,
                TransitionAttestations::default().attest_semantic_equivalence(),
            )
            .is_ok());
    }
}
