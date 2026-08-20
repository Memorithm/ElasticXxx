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

/// Set of representation contracts explicitly supported by a model/runtime boundary.
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

    /// Add one supported representation contract.
    pub fn insert(&mut self, id: RepresentationId, schema_version: u32) {
        self.entries.insert((id, schema_version));
    }

    /// Check whether a representation contract is explicitly supported.
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
    /// This is legal across representation contracts only with an explicit
    /// semantic-equivalence proof supplied by the caller.
    Reinterpret,
    /// Transform the existing materialization into the target representation.
    Reencode,
    /// Regenerate the target representation from a trusted source state.
    Recompute,
}

/// Runtime facts required to validate a requested representation transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransitionFacts {
    /// A proof/contract establishes byte-level semantic equivalence between source and target.
    pub semantic_equivalence_proven: bool,
    /// A re-encoder for this exact source/target pair is available.
    pub reencoder_available: bool,
    /// A trusted source exists from which the target can be recomputed.
    pub recompute_source_available: bool,
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
    /// Validate this transition against declared capabilities and runtime facts.
    ///
    /// Any contract-changing transition and any transition that creates a new
    /// materialization (`Reencode` or `Recompute`) must advance the
    /// representation epoch. Silent reinterpretation is rejected unless
    /// semantic equivalence is explicitly proven. Re-encoding and recomputation
    /// require their corresponding mechanisms to exist.
    pub fn validate(
        &self,
        capabilities: &CapabilitySet,
        facts: TransitionFacts,
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

        if must_advance_epoch && self.to.epoch <= self.from.epoch {
            return Err(TransitionError::EpochMustAdvance {
                from: self.from.epoch,
                to: self.to.epoch,
            });
        }
        if self.to.epoch < self.from.epoch {
            return Err(TransitionError::EpochRegression {
                from: self.from.epoch,
                to: self.to.epoch,
            });
        }

        match self.mechanism {
            TransitionMechanism::Reinterpret => {
                if contract_changes && !facts.semantic_equivalence_proven {
                    return Err(TransitionError::UnsafeReinterpretation);
                }
            }
            TransitionMechanism::Reencode => {
                if !facts.reencoder_available {
                    return Err(TransitionError::MissingReencoder);
                }
            }
            TransitionMechanism::Recompute => {
                if !facts.recompute_source_available {
                    return Err(TransitionError::MissingRecomputeSource);
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
    /// Contract-changing byte reinterpretation lacks a semantic-equivalence proof.
    UnsafeReinterpretation,
    /// Requested re-encoding has no implementation for the source/target pair.
    MissingReencoder,
    /// Requested recomputation has no trusted source state.
    MissingRecomputeSource,
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
            Self::UnsafeReinterpretation => write!(
                f,
                "representation-changing reinterpretation requires an explicit semantic-equivalence proof"
            ),
            Self::MissingReencoder => write!(f, "requested representation transition has no re-encoder"),
            Self::MissingRecomputeSource => write!(f, "requested representation transition has no trusted recompute source"),
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
    fn geometry_like_change_requires_epoch_and_reencoder() {
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
            transition.validate(&caps, TransitionFacts::default()),
            Err(TransitionError::MissingReencoder)
        );
        assert!(transition
            .validate(
                &caps,
                TransitionFacts {
                    reencoder_available: true,
                    ..TransitionFacts::default()
                }
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
                TransitionFacts {
                    reencoder_available: true,
                    ..TransitionFacts::default()
                }
            ),
            Err(TransitionError::EpochMustAdvance {
                from: RepresentationEpoch::new(3),
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

        assert!(transition.validate(&caps, TransitionFacts::default()).is_ok());
    }

    #[test]
    fn silent_cross_contract_reinterpretation_is_rejected() {
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
            transition.validate(&caps, TransitionFacts::default()),
            Err(TransitionError::UnsafeReinterpretation)
        );
    }
}
