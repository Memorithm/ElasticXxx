use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Upper bound for representation and issuer identifier length, in bytes.
const MAX_ID_LEN: usize = 256;

/// Stable identifier for a mathematical or numerical representation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepresentationId(String);

impl RepresentationId {
    /// Construct a non-empty, trimmed and bounded representation identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, TransitionError> {
        let value = validate_identifier(value.into(), TransitionError::EmptyRepresentationId)?;
        Ok(Self(value))
    }

    /// Borrow the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validate one identifier without silently normalizing caller input.
fn validate_identifier(
    value: String,
    empty_error: TransitionError,
) -> Result<String, TransitionError> {
    if value.trim().is_empty() {
        return Err(empty_error);
    }
    if value.len() > MAX_ID_LEN {
        return Err(TransitionError::IdentifierTooLong { len: value.len() });
    }
    if value.chars().next().is_some_and(char::is_whitespace)
        || value.chars().next_back().is_some_and(char::is_whitespace)
    {
        return Err(TransitionError::UntrimmedIdentifier);
    }
    Ok(value)
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

    /// Whether this epoch is strictly before `rhs`.
    pub const fn lt(self, rhs: Self) -> bool {
        self.0 < rhs.0
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

    /// Derive the admissible target state for a transition from this state.
    ///
    /// The epoch rules mirror `RepresentationTransition::validate`:
    /// same-contract `Reinterpret` keeps the epoch; every other combination
    /// advances it. Using this constructor instead of hand-picking an epoch
    /// removes the `EpochMustAdvance` failure mode by construction.
    pub fn derive_target(
        &self,
        contract: TargetContract,
        mechanism: TransitionMechanism,
    ) -> Result<RepresentationState, TransitionError> {
        let keeps_epoch = matches!(contract, TargetContract::Same)
            && matches!(mechanism, TransitionMechanism::Reinterpret);
        let (id, schema_version) = match contract {
            TargetContract::Same => (self.id.clone(), self.schema_version),
            TargetContract::New { id, schema_version } => (id, schema_version),
        };
        let epoch = if keeps_epoch {
            self.epoch
        } else {
            self.epoch.next()?
        };
        Ok(RepresentationState::new(id, schema_version, epoch))
    }
}

/// Contract selection for [`RepresentationState::derive_target`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetContract {
    /// Stay within the source representation contract.
    Same,
    /// Move to the given representation contract.
    New {
        /// Representation family identifier of the target.
        id: RepresentationId,
        /// Schema/contract version of the target.
        schema_version: u32,
    },
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
    entries: BTreeMap<RepresentationId, BTreeSet<u32>>,
}

impl CapabilitySet {
    /// Empty capability set.
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Add one declared supported representation contract.
    pub fn insert(&mut self, id: RepresentationId, schema_version: u32) {
        self.entries.entry(id).or_default().insert(schema_version);
    }

    /// Remove one declared contract; returns whether it was present.
    pub fn remove(&mut self, id: &RepresentationId, schema_version: u32) -> bool {
        let Some(versions) = self.entries.get_mut(id) else {
            return false;
        };
        let removed = versions.remove(&schema_version);
        if versions.is_empty() {
            self.entries.remove(id);
        }
        removed
    }

    /// Check whether a representation contract is explicitly declared supported.
    pub fn supports(&self, state: &RepresentationState) -> bool {
        self.supports_contract(&state.id, state.schema_version)
    }

    /// Check whether a contract/version pair is explicitly declared supported.
    pub fn supports_contract(&self, id: &RepresentationId, schema_version: u32) -> bool {
        self.entries
            .get(id)
            .is_some_and(|versions| versions.contains(&schema_version))
    }

    /// Iterate over all declared contracts.
    pub fn iter(&self) -> impl Iterator<Item = (&RepresentationId, u32)> {
        self.entries
            .iter()
            .flat_map(|(id, versions)| versions.iter().map(move |version| (id, *version)))
    }

    /// Number of declared contracts.
    pub fn len(&self) -> usize {
        self.entries.values().map(BTreeSet::len).sum()
    }

    /// Whether no contracts are declared.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl fmt::Display for RepresentationEpoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for RepresentationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} v{} @e{}",
            self.id.as_str(),
            self.schema_version,
            self.epoch
        )
    }
}

/// Mechanism used to materialize the target representation.
///
/// The vocabulary is shared with the general resource model
/// ([`crate::resource`]): the same three classes describe whether a
/// transition reuses, transforms, or regenerates a materialization. Ordering
/// follows declaration order and is structural only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    /// Build attestations from provenance-carrying [`EvidenceToken`]s.
    ///
    /// A token only sets its claim when it was issued for **exactly** this
    /// transition (same endpoints, mechanism, and therefore same fingerprint).
    /// This keeps validation semantics identical to the boolean builders while
    /// letting a trusted validator layer carry provenance through to the
    /// structural check. Unauthenticated builder methods remain available for
    /// callers that operate inside an already-trusted boundary.
    pub fn from_evidence<'a>(
        tokens: impl IntoIterator<Item = &'a EvidenceToken>,
        transition: &RepresentationTransition,
    ) -> Self {
        let mut attestations = Self::none();
        for token in tokens {
            if !token.matches(transition) {
                continue;
            }
            match token.kind {
                EvidenceKind::SemanticEquivalence => {
                    attestations.semantic_equivalence = true;
                }
                EvidenceKind::ReencoderAvailable => {
                    attestations.reencoder_available = true;
                }
                EvidenceKind::RecomputeSourceAvailable => {
                    attestations.recompute_source_available = true;
                }
            }
        }
        attestations
    }
}

/// Identity of a trusted validator boundary that issues evidence.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IssuerId(String);

impl IssuerId {
    /// Construct a non-empty, trimmed and bounded issuer identity.
    pub fn new(value: impl Into<String>) -> Result<Self, TransitionError> {
        let value = validate_identifier(value.into(), TransitionError::EmptyIssuerId)?;
        Ok(Self(value))
    }

    /// Borrow the issuer text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The claim carried by an [`EvidenceToken`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvidenceKind {
    /// Semantic equivalence of a cross-contract reinterpretation.
    SemanticEquivalence,
    /// Availability of a re-encoder for the exact transition.
    ReencoderAvailable,
    /// Availability of a trusted source for recomputation.
    RecomputeSourceAvailable,
}

/// Provenance-carrying evidence issued by a trusted validator boundary.
///
/// A token binds one claim ([`EvidenceKind`]) to one exact
/// [`RepresentationTransition`] via a fingerprint, and records the issuing
/// boundary. `elastic-core` cannot authenticate issuers; the type exists so
/// that evidence is structurally bound to the transition it justifies instead
/// of being an unbound boolean claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceToken {
    issuer: IssuerId,
    kind: EvidenceKind,
    transition_fingerprint: u64,
}

impl EvidenceToken {
    /// Issue a token from a trusted boundary for exactly this transition.
    pub fn issue(
        issuer: IssuerId,
        kind: EvidenceKind,
        transition: &RepresentationTransition,
    ) -> Self {
        Self {
            issuer,
            kind,
            transition_fingerprint: fingerprint(transition),
        }
    }

    /// The issuing boundary.
    pub fn issuer(&self) -> &IssuerId {
        &self.issuer
    }

    /// The claim carried by this token.
    pub const fn kind(&self) -> EvidenceKind {
        self.kind
    }

    /// Whether this token justifies exactly this transition.
    pub fn matches(&self, transition: &RepresentationTransition) -> bool {
        self.transition_fingerprint == fingerprint(transition)
    }
}

/// FNV-1a 64-bit fingerprint over the canonical textual form of a transition.
///
/// Dependency-free and stable within a process; fingerprints are not persisted
/// or shared across trust domains, where a real validator layer would use a
/// stronger canonical serialization.
fn fingerprint(transition: &RepresentationTransition) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    fn absorb(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    absorb(
        &mut hash,
        format!(
            "{}/{}/{}/{}/{}/{}/{}",
            transition.from.id.as_str(),
            transition.from.schema_version,
            transition.from.epoch,
            transition.to.id.as_str(),
            transition.to.schema_version,
            transition.to.epoch,
            match transition.mechanism {
                TransitionMechanism::Reinterpret => "reinterpret",
                TransitionMechanism::Reencode => "reencode",
                TransitionMechanism::Recompute => "recompute",
            }
        )
        .as_bytes(),
    );
    hash
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
    /// Issuer identities may not be blank.
    EmptyIssuerId,
    /// An identifier exceeded the 256-byte input bound.
    IdentifierTooLong {
        /// Rejected length in bytes.
        len: usize,
    },
    /// An identifier carried leading or trailing whitespace.
    UntrimmedIdentifier,
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
            Self::EmptyIssuerId => write!(f, "issuer identifier must not be empty"),
            Self::IdentifierTooLong { len } => write!(
                f,
                "identifier of {len} bytes exceeds the {MAX_ID_LEN} byte limit"
            ),
            Self::UntrimmedIdentifier => write!(
                f,
                "identifier must not carry leading or trailing whitespace"
            ),
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

        assert!(transition
            .validate(&caps, TransitionAttestations::default())
            .is_ok());
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

    /// Attestations exactly sufficient for `transition`, derived independently
    /// of `validate`'s internals.
    fn sufficient_attestations(transition: &RepresentationTransition) -> TransitionAttestations {
        let attestations = TransitionAttestations::default();
        match transition.mechanism {
            TransitionMechanism::Reinterpret => {
                if !transition.from.same_contract(&transition.to) {
                    return attestations.attest_semantic_equivalence();
                }
                attestations
            }
            TransitionMechanism::Reencode => attestations.attest_reencoder_available(),
            TransitionMechanism::Recompute => attestations.attest_recompute_source_available(),
        }
    }

    /// Exhaustive decision table: for every (mechanism, contract-change)
    /// combination, `derive_target` applies exactly the epoch policy enforced
    /// by `validate`, and the resulting transition validates with
    /// exactly-sufficient attestations.
    #[test]
    fn exhaustive_transition_decision_table_is_consistent_with_derive_target() {
        use TargetContract;

        const MECHANISMS: [TransitionMechanism; 3] = [
            TransitionMechanism::Reinterpret,
            TransitionMechanism::Reencode,
            TransitionMechanism::Recompute,
        ];
        const CONTRACT_CHANGES: [bool; 2] = [false, true];

        for &mechanism in &MECHANISMS {
            for &changes_contract in &CONTRACT_CHANGES {
                let from = RepresentationState::new(id("src"), 1, RepresentationEpoch::new(3));
                let contract = if changes_contract {
                    TargetContract::New {
                        id: id("dst"),
                        schema_version: 1,
                    }
                } else {
                    TargetContract::Same
                };
                let derived = from.derive_target(contract.clone(), mechanism).unwrap();

                let keeps_epoch =
                    !changes_contract && matches!(mechanism, TransitionMechanism::Reinterpret);
                let expected_epoch = if keeps_epoch { 3 } else { 4 };
                assert_eq!(
                    derived.epoch.get(),
                    expected_epoch,
                    "epoch policy diverged for {mechanism:?} x changes_contract={changes_contract}"
                );

                let mut caps = CapabilitySet::new();
                caps.insert(derived.id.clone(), derived.schema_version);
                let transition = RepresentationTransition {
                    from: from.clone(),
                    to: derived,
                    mechanism,
                };
                assert!(transition
                    .validate(&caps, sufficient_attestations(&transition))
                    .is_ok());
            }
        }

        // Hand-picking an equal epoch where advancement is mandatory still
        // fails: derive_target removes the mistake, validate keeps enforcing.
        let from = RepresentationState::new(id("src"), 1, RepresentationEpoch::new(3));
        let stuck = RepresentationState::new(id("src"), 1, RepresentationEpoch::new(3));
        let mut caps = CapabilitySet::new();
        caps.insert(stuck.id.clone(), stuck.schema_version);
        assert_eq!(
            RepresentationTransition {
                from,
                to: stuck,
                mechanism: TransitionMechanism::Reencode
            }
            .validate(
                &caps,
                TransitionAttestations::default().attest_reencoder_available()
            ),
            Err(TransitionError::EpochMustAdvance {
                from: RepresentationEpoch::new(3),
                to: RepresentationEpoch::new(3),
            })
        );

        // Epoch overflow is surfaced by derive_target rather than panicking.
        let saturated = RepresentationState::new(id("src"), 1, RepresentationEpoch::new(u64::MAX));
        assert_eq!(
            saturated.derive_target(TargetContract::Same, TransitionMechanism::Reencode),
            Err(TransitionError::EpochOverflow)
        );
    }

    /// Deterministic xorshift64* so failures are reproducible without adding
    /// external test dependencies to this deliberately dependency-free crate.
    struct Xorshift(u64);

    impl Xorshift {
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
    }

    /// Property: any walk of derived transitions keeps epochs monotonic and
    /// advances them exactly when the mechanism creates a new materialization
    /// or changes contract.
    #[test]
    fn random_derived_walks_keep_epochs_monotonic() {
        use TargetContract;

        let mut rng = Xorshift(0xE15_1C5E_ED01);
        for _walk in 0..200 {
            let seed_state = RepresentationState::new(
                id("walk"),
                1,
                RepresentationEpoch::new(rng.next_u64() % 1000),
            );
            let mut current = seed_state;
            for _step in 0..16 {
                let mechanism = match rng.next_u64() % 3 {
                    0 => TransitionMechanism::Reinterpret,
                    1 => TransitionMechanism::Reencode,
                    _ => TransitionMechanism::Recompute,
                };
                let contract = if rng.next_u64() % 2 == 0 {
                    TargetContract::Same
                } else {
                    TargetContract::New {
                        id: id("walk.dst"),
                        schema_version: 1,
                    }
                };
                let before = current.epoch.get();
                let next = current.derive_target(contract.clone(), mechanism).unwrap();
                let creates_materialization = matches!(
                    mechanism,
                    TransitionMechanism::Reencode | TransitionMechanism::Recompute
                );
                let must_advance =
                    !matches!(contract, TargetContract::Same) || creates_materialization;
                if must_advance {
                    assert!(next.epoch.get() > before);
                } else {
                    assert_eq!(next.epoch.get(), before);
                }
                current = next;
            }
        }
    }

    /// The exact-attestation error surface: a transition missing its one
    /// required claim always yields that claim's specific
    /// missing-attestation error.
    #[test]
    fn each_required_claim_maps_to_its_specific_error() {
        use TargetContract;

        let from = RepresentationState::new(id("src"), 1, RepresentationEpoch::new(1));

        // Reinterpret across contracts without semantic equivalence.
        let to_other = from
            .derive_target(
                TargetContract::New {
                    id: id("dst"),
                    schema_version: 1,
                },
                TransitionMechanism::Reinterpret,
            )
            .unwrap();
        let mut caps = CapabilitySet::new();
        caps.insert(to_other.id.clone(), to_other.schema_version);
        assert_eq!(
            RepresentationTransition {
                from: from.clone(),
                to: to_other,
                mechanism: TransitionMechanism::Reinterpret
            }
            .validate(&caps, TransitionAttestations::default()),
            Err(TransitionError::MissingSemanticEquivalenceAttestation)
        );

        // Reencode without reencoder attestation.
        let to_next = from
            .derive_target(TargetContract::Same, TransitionMechanism::Reencode)
            .unwrap();
        let mut caps = CapabilitySet::new();
        caps.insert(to_next.id.clone(), to_next.schema_version);
        assert_eq!(
            RepresentationTransition {
                from: from.clone(),
                to: to_next.clone(),
                mechanism: TransitionMechanism::Reencode
            }
            .validate(&caps, TransitionAttestations::default()),
            Err(TransitionError::MissingReencoderAttestation)
        );

        // Recompute without source attestation.
        assert_eq!(
            RepresentationTransition {
                from,
                to: to_next,
                mechanism: TransitionMechanism::Recompute
            }
            .validate(&caps, TransitionAttestations::default()),
            Err(TransitionError::MissingRecomputeSourceAttestation)
        );
    }

    #[test]
    fn evidence_token_binds_claim_to_exact_transition() {
        use TargetContract;

        let from = RepresentationState::new(id("kv.raw"), 1, RepresentationEpoch::new(4));
        let to = from
            .derive_target(
                TargetContract::New {
                    id: id("kv.int4"),
                    schema_version: 1,
                },
                TransitionMechanism::Reencode,
            )
            .unwrap();
        let transition = RepresentationTransition {
            from: from.clone(),
            to: to.clone(),
            mechanism: TransitionMechanism::Reencode,
        };

        let issuer = IssuerId::new("trusted-validator").unwrap();
        let token = EvidenceToken::issue(
            issuer.clone(),
            EvidenceKind::ReencoderAvailable,
            &transition,
        );
        assert_eq!(token.issuer(), &issuer);
        assert_eq!(token.kind(), EvidenceKind::ReencoderAvailable);
        assert!(token.matches(&transition));

        let attestations = TransitionAttestations::from_evidence([&token], &transition);
        let mut caps = CapabilitySet::new();
        caps.insert(to.id.clone(), to.schema_version);
        assert!(transition.validate(&caps, attestations).is_ok());

        // A different transition (epoch bumped) is not justified by this token.
        let other = RepresentationTransition {
            mechanism: TransitionMechanism::Reencode,
            to: RepresentationState::new(
                to.id.clone(),
                to.schema_version,
                to.epoch.next().unwrap(),
            ),
            from,
        };
        assert!(!token.matches(&other));
        let insufficient = TransitionAttestations::from_evidence([&token], &other);
        assert_eq!(
            other.validate(&caps, insufficient),
            Err(TransitionError::MissingReencoderAttestation)
        );

        // A token of the wrong kind does not satisfy the claim.
        let wrong_kind =
            EvidenceToken::issue(issuer, EvidenceKind::SemanticEquivalence, &transition);
        let mismatched = TransitionAttestations::from_evidence([&wrong_kind], &transition);
        assert_eq!(
            transition.validate(&caps, mismatched),
            Err(TransitionError::MissingReencoderAttestation)
        );
    }

    #[test]
    fn blank_issuer_identity_is_rejected() {
        assert_eq!(IssuerId::new("   "), Err(TransitionError::EmptyIssuerId));
    }

    #[test]
    fn display_forms_are_stable_and_informative() {
        let state = RepresentationState::new(id("epg.so2"), 3, RepresentationEpoch::new(7));
        assert_eq!(state.epoch.to_string(), "7");
        assert_eq!(state.to_string(), "epg.so2 v3 @e7");
    }

    #[test]
    fn capability_set_supports_insert_remove_iteration() {
        let mut caps = CapabilitySet::new();
        assert!(caps.is_empty());
        caps.insert(id("a"), 1);
        caps.insert(id("b"), 2);
        assert_eq!(caps.len(), 2);

        let state_a = RepresentationState::new(id("a"), 1, RepresentationEpoch::new(0));
        assert!(caps.supports(&state_a));
        assert!(caps.supports_contract(&id("b"), 2));
        assert!(!caps.supports_contract(&id("b"), 3));

        let listed: Vec<(String, u32)> = caps
            .iter()
            .map(|(id, version)| (id.as_str().to_string(), version))
            .collect();
        assert_eq!(listed, vec![("a".to_string(), 1), ("b".to_string(), 2)]);

        assert!(caps.remove(&id("a"), 1));
        assert!(!caps.remove(&id("a"), 1));
        assert!(!caps.supports(&state_a));
    }
}
