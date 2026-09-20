//! Typed byte-budget contracts for RAM and persistent storage.
//!
//! A capacity budget is static policy intent: if a candidate predicate is true,
//! its declared byte cost contributes to a bounded shared budget. It does not
//! claim that the host physically owns that capacity at runtime. Dynamic
//! feasibility remains an observation/validation responsibility.

use super::{
    ContractId, LogicalResourceId, ResourceGroupBuilder, SharedBudget, SharedBudgetError,
    SharedBudgetId, SharedBudgetTerm,
};
use crate::{PredicateKey, PseudoBooleanScale};
use std::fmt;

/// Canonical pseudo-Boolean scale unit for RAM candidate costs.
pub const RAM_CAPACITY_BUDGET_UNIT: &str = "ram-bytes";
/// Canonical pseudo-Boolean scale unit for persistent-storage candidate costs.
pub const STORAGE_CAPACITY_BUDGET_UNIT: &str = "storage-bytes";
/// Schema version of the typed RAM/storage budget contract.
pub const CAPACITY_BUDGET_CONTRACT_SCHEMA_V1: u16 = 1;
/// Schema version of the immutable safety reservation envelope.
pub const SAFETY_CAPACITY_ENVELOPE_SCHEMA_V1: u16 = 1;
/// Maximum UTF-8 byte length of a safety-reservation identity.
pub const MAX_SAFETY_RESERVATION_ID_BYTES: usize = 256;
/// Maximum number of immutable reservations inside one capacity envelope.
pub const MAX_SAFETY_RESERVATIONS_PER_ENVELOPE: usize = 64;

/// Physical capacity domain constrained by one static budget contract.
///
/// RAM and storage intentionally remain distinct even though both are counted in
/// bytes; the type prevents a storage budget from silently satisfying a RAM
/// budget merely because their numeric scales match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapacityBudgetKind {
    Ram,
    Storage,
}

impl CapacityBudgetKind {
    /// Stable wire/debug spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ram => "ram",
            Self::Storage => "storage",
        }
    }

    /// Canonical pseudo-Boolean unit used by the lowered shared budget.
    #[must_use]
    pub const fn scale_unit(self) -> &'static str {
        match self {
            Self::Ram => RAM_CAPACITY_BUDGET_UNIT,
            Self::Storage => STORAGE_CAPACITY_BUDGET_UNIT,
        }
    }
}

impl fmt::Display for CapacityBudgetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable identity of one immutable safety-critical capacity reservation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SafetyReservationId(String);

impl SafetyReservationId {
    pub fn new(value: impl Into<String>) -> Result<Self, SafetyCapacityEnvelopeError> {
        let value = value.into();
        if value.trim().is_empty() || value.trim() != value {
            return Err(SafetyCapacityEnvelopeError::InvalidReservationId);
        }
        if value.len() > MAX_SAFETY_RESERVATION_ID_BYTES {
            return Err(SafetyCapacityEnvelopeError::ReservationIdTooLong {
                bytes: value.len(),
                maximum: MAX_SAFETY_RESERVATION_ID_BYTES,
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SafetyReservationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Immutable static reservation owned by one safety-critical resource.
///
/// This is policy intent, not proof that bytes were physically reserved. The
/// external `contract` names the application-owned safety semantics that justify
/// the reservation; Elastic records but does not interpret that contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImmutableCapacityReservation {
    id: SafetyReservationId,
    kind: CapacityBudgetKind,
    resource: LogicalResourceId,
    contract: ContractId,
    bytes: u64,
}

impl ImmutableCapacityReservation {
    pub fn new(
        id: SafetyReservationId,
        kind: CapacityBudgetKind,
        resource: LogicalResourceId,
        contract: ContractId,
        bytes: u64,
    ) -> Result<Self, SafetyCapacityEnvelopeError> {
        if bytes == 0 {
            return Err(SafetyCapacityEnvelopeError::ZeroReservation { reservation: id });
        }
        Ok(Self {
            id,
            kind,
            resource,
            contract,
            bytes,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &SafetyReservationId {
        &self.id
    }

    #[must_use]
    pub const fn kind(&self) -> CapacityBudgetKind {
        self.kind
    }

    #[must_use]
    pub const fn resource(&self) -> &LogicalResourceId {
        &self.resource
    }

    #[must_use]
    pub const fn contract(&self) -> &ContractId {
        &self.contract
    }

    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Static capacity envelope preserving immutable safety reservations before
/// adaptive candidate costs are admitted.
///
/// `total_policy_bytes` is an operator-declared static policy ceiling, not a
/// runtime observation. The adaptive budget is derived exactly as
/// `total_policy_bytes - sum(reservations)`. Runtime physical feasibility must
/// still be revalidated against current evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafetyCapacityEnvelope {
    schema_version: u16,
    kind: CapacityBudgetKind,
    total_policy_bytes: u64,
    reserved_bytes: u64,
    reservations: Vec<ImmutableCapacityReservation>,
    adaptive_budget: CapacityBudgetContract,
}

impl SafetyCapacityEnvelope {
    pub fn new(
        kind: CapacityBudgetKind,
        adaptive_budget_id: SharedBudgetId,
        total_policy_bytes: u64,
        mut reservations: Vec<ImmutableCapacityReservation>,
        adaptive_terms: Vec<CapacityBudgetTerm>,
    ) -> Result<Self, SafetyCapacityEnvelopeError> {
        if reservations.is_empty() {
            return Err(SafetyCapacityEnvelopeError::EmptyReservations);
        }
        if reservations.len() > MAX_SAFETY_RESERVATIONS_PER_ENVELOPE {
            return Err(SafetyCapacityEnvelopeError::TooManyReservations {
                reservations: reservations.len(),
                maximum: MAX_SAFETY_RESERVATIONS_PER_ENVELOPE,
            });
        }
        reservations.sort();
        for pair in reservations.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(SafetyCapacityEnvelopeError::DuplicateReservation {
                    reservation: pair[0].id.clone(),
                });
            }
        }
        let mut reserved_bytes = 0_u64;
        for reservation in &reservations {
            if reservation.kind != kind {
                return Err(SafetyCapacityEnvelopeError::ReservationKindMismatch {
                    reservation: reservation.id.clone(),
                    envelope: kind,
                    observed: reservation.kind,
                });
            }
            reserved_bytes = reserved_bytes
                .checked_add(reservation.bytes)
                .ok_or(SafetyCapacityEnvelopeError::ReservedBytesOverflow)?;
        }
        if reserved_bytes > total_policy_bytes {
            return Err(SafetyCapacityEnvelopeError::ReservationsExceedTotal {
                reserved_bytes,
                total_policy_bytes,
            });
        }
        let adaptive_ceiling = total_policy_bytes - reserved_bytes;
        let adaptive_budget = CapacityBudgetContract::new(
            kind,
            adaptive_budget_id,
            adaptive_terms,
            adaptive_ceiling,
        )?;
        Ok(Self {
            schema_version: SAFETY_CAPACITY_ENVELOPE_SCHEMA_V1,
            kind,
            total_policy_bytes,
            reserved_bytes,
            reservations,
            adaptive_budget,
        })
    }

    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    #[must_use]
    pub const fn kind(&self) -> CapacityBudgetKind {
        self.kind
    }

    #[must_use]
    pub const fn total_policy_bytes(&self) -> u64 {
        self.total_policy_bytes
    }

    #[must_use]
    pub const fn reserved_bytes(&self) -> u64 {
        self.reserved_bytes
    }

    #[must_use]
    pub const fn adaptive_ceiling_bytes(&self) -> u64 {
        self.adaptive_budget.maximum_bytes()
    }

    #[must_use]
    pub fn reservations(&self) -> &[ImmutableCapacityReservation] {
        &self.reservations
    }

    #[must_use]
    pub const fn adaptive_budget(&self) -> &CapacityBudgetContract {
        &self.adaptive_budget
    }
}

/// Fail-closed construction errors for immutable safety capacity envelopes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SafetyCapacityEnvelopeError {
    InvalidReservationId,
    ReservationIdTooLong {
        bytes: usize,
        maximum: usize,
    },
    ZeroReservation {
        reservation: SafetyReservationId,
    },
    EmptyReservations,
    TooManyReservations {
        reservations: usize,
        maximum: usize,
    },
    DuplicateReservation {
        reservation: SafetyReservationId,
    },
    ReservationKindMismatch {
        reservation: SafetyReservationId,
        envelope: CapacityBudgetKind,
        observed: CapacityBudgetKind,
    },
    ReservedBytesOverflow,
    ReservationsExceedTotal {
        reserved_bytes: u64,
        total_policy_bytes: u64,
    },
    CapacityBudget(CapacityBudgetError),
}

impl From<CapacityBudgetError> for SafetyCapacityEnvelopeError {
    fn from(value: CapacityBudgetError) -> Self {
        Self::CapacityBudget(value)
    }
}

impl fmt::Display for SafetyCapacityEnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReservationId => f.write_str(
                "safety reservation identity must be non-empty and trimmed",
            ),
            Self::ReservationIdTooLong { bytes, maximum } => write!(
                f,
                "safety reservation identity length {bytes} exceeds maximum {maximum} bytes"
            ),
            Self::ZeroReservation { reservation } => write!(
                f,
                "safety reservation {reservation} must reserve at least one byte"
            ),
            Self::EmptyReservations => f.write_str(
                "safety capacity envelope requires at least one immutable reservation",
            ),
            Self::TooManyReservations {
                reservations,
                maximum,
            } => write!(
                f,
                "safety capacity envelope contains {reservations} reservations; maximum is {maximum}"
            ),
            Self::DuplicateReservation { reservation } => write!(
                f,
                "safety capacity envelope repeats reservation {reservation}"
            ),
            Self::ReservationKindMismatch {
                reservation,
                envelope,
                observed,
            } => write!(
                f,
                "safety reservation {reservation} has {observed} kind but envelope is {envelope}"
            ),
            Self::ReservedBytesOverflow => {
                f.write_str("safety reservation byte sum overflowed u64")
            }
            Self::ReservationsExceedTotal {
                reserved_bytes,
                total_policy_bytes,
            } => write!(
                f,
                "safety reservations require {reserved_bytes} bytes but static policy total is {total_policy_bytes}"
            ),
            Self::CapacityBudget(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SafetyCapacityEnvelopeError {}

/// One candidate-owned byte cost inside a RAM/storage budget.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapacityBudgetTerm {
    resource: LogicalResourceId,
    predicate: PredicateKey,
    bytes: u64,
}

impl CapacityBudgetTerm {
    /// Construct one strictly positive candidate byte cost.
    pub fn new(
        resource: LogicalResourceId,
        predicate: PredicateKey,
        bytes: u64,
    ) -> Result<Self, CapacityBudgetError> {
        if bytes == 0 {
            return Err(CapacityBudgetError::ZeroCost {
                resource,
                predicate,
            });
        }
        Ok(Self {
            resource,
            predicate,
            bytes,
        })
    }

    /// Logical resource whose candidate/state owns this cost.
    #[must_use]
    pub const fn resource(&self) -> &LogicalResourceId {
        &self.resource
    }

    /// Stable predicate whose explicit `True` activates the cost.
    #[must_use]
    pub const fn predicate(&self) -> &PredicateKey {
        &self.predicate
    }

    /// Candidate cost in bytes.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Typed RAM/storage capacity budget backed by the existing shared-budget core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapacityBudgetContract {
    schema_version: u16,
    kind: CapacityBudgetKind,
    id: SharedBudgetId,
    maximum_bytes: u64,
    terms: Vec<CapacityBudgetTerm>,
    shared_budget: SharedBudget,
}

impl CapacityBudgetContract {
    /// Construct one typed byte budget.
    ///
    /// `maximum_bytes = 0` is valid and intentionally rejects every positive-cost
    /// candidate once its predicate becomes explicitly true. Empty term sets are
    /// rejected by the existing shared-budget contract.
    pub fn new(
        kind: CapacityBudgetKind,
        id: SharedBudgetId,
        mut terms: Vec<CapacityBudgetTerm>,
        maximum_bytes: u64,
    ) -> Result<Self, CapacityBudgetError> {
        terms.sort();
        let shared_terms = terms
            .iter()
            .map(|term| {
                SharedBudgetTerm::new(
                    term.resource.clone(),
                    term.predicate.clone(),
                    i128::from(term.bytes),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let scale = PseudoBooleanScale::new(kind.scale_unit(), 1)
            .expect("static RAM/storage budget scale is valid");
        let shared_budget =
            SharedBudget::new(id.clone(), shared_terms, i128::from(maximum_bytes), scale)?;
        Ok(Self {
            schema_version: CAPACITY_BUDGET_CONTRACT_SCHEMA_V1,
            kind,
            id,
            maximum_bytes,
            terms,
            shared_budget,
        })
    }

    /// Convenience constructor for RAM byte budgets.
    pub fn ram(
        id: SharedBudgetId,
        terms: Vec<CapacityBudgetTerm>,
        maximum_bytes: u64,
    ) -> Result<Self, CapacityBudgetError> {
        Self::new(CapacityBudgetKind::Ram, id, terms, maximum_bytes)
    }

    /// Convenience constructor for persistent-storage byte budgets.
    pub fn storage(
        id: SharedBudgetId,
        terms: Vec<CapacityBudgetTerm>,
        maximum_bytes: u64,
    ) -> Result<Self, CapacityBudgetError> {
        Self::new(CapacityBudgetKind::Storage, id, terms, maximum_bytes)
    }

    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    #[must_use]
    pub const fn kind(&self) -> CapacityBudgetKind {
        self.kind
    }

    #[must_use]
    pub const fn id(&self) -> &SharedBudgetId {
        &self.id
    }

    #[must_use]
    pub const fn maximum_bytes(&self) -> u64 {
        self.maximum_bytes
    }

    #[must_use]
    pub fn terms(&self) -> &[CapacityBudgetTerm] {
        &self.terms
    }

    /// Canonical shared pseudo-Boolean budget carrying the authoritative logic.
    #[must_use]
    pub const fn shared_budget(&self) -> &SharedBudget {
        &self.shared_budget
    }

    /// Consume the typed wrapper and retain the existing shared-budget semantics.
    #[must_use]
    pub fn into_shared_budget(self) -> SharedBudget {
        self.shared_budget
    }
}

/// Convenience extension: add a typed RAM/storage budget to a resource group.
impl ResourceGroupBuilder {
    #[must_use]
    pub fn capacity_budget(self, budget: CapacityBudgetContract) -> Self {
        self.shared_budget(budget.into_shared_budget())
    }

    #[must_use]
    pub fn safety_capacity_envelope(self, envelope: SafetyCapacityEnvelope) -> Self {
        let adaptive = envelope.adaptive_budget().shared_budget().clone();
        self.safety_capacity_envelope_contract(envelope)
            .shared_budget(adaptive)
    }
}

/// Construction failure for typed RAM/storage budget contracts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapacityBudgetError {
    ZeroCost {
        resource: LogicalResourceId,
        predicate: PredicateKey,
    },
    SharedBudget(SharedBudgetError),
}

impl From<SharedBudgetError> for CapacityBudgetError {
    fn from(value: SharedBudgetError) -> Self {
        Self::SharedBudget(value)
    }
}

impl fmt::Display for CapacityBudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCost {
                resource,
                predicate,
            } => write!(
                f,
                "capacity-budget term {resource}/{predicate} has zero byte cost; omit inactive terms instead"
            ),
            Self::SharedBudget(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CapacityBudgetError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FactSet, PredicateRegistry, TruthValue};

    fn id(value: &str) -> LogicalResourceId {
        LogicalResourceId::new(value).unwrap()
    }

    fn key(value: &str) -> PredicateKey {
        PredicateKey::new("elastic.edge.capacity", value).unwrap()
    }

    #[test]
    fn ram_and_storage_use_distinct_semantic_units() {
        let terms = vec![CapacityBudgetTerm::new(id("model"), key("expanded"), 4096).unwrap()];
        let ram =
            CapacityBudgetContract::ram(SharedBudgetId::new("ram").unwrap(), terms.clone(), 8192)
                .unwrap();
        let storage =
            CapacityBudgetContract::storage(SharedBudgetId::new("storage").unwrap(), terms, 8192)
                .unwrap();
        assert_eq!(ram.kind(), CapacityBudgetKind::Ram);
        assert_eq!(ram.shared_budget().scale().unit(), RAM_CAPACITY_BUDGET_UNIT);
        assert_eq!(
            storage.shared_budget().scale().unit(),
            STORAGE_CAPACITY_BUDGET_UNIT
        );
        assert_ne!(
            ram.shared_budget().constraint().scale(),
            storage.shared_budget().constraint().scale()
        );
    }

    #[test]
    fn budget_delegates_unknown_and_capacity_logic_to_pseudo_boolean_core() {
        let candidate = key("candidate");
        let budget = CapacityBudgetContract::ram(
            SharedBudgetId::new("ram").unwrap(),
            vec![CapacityBudgetTerm::new(id("model"), candidate.clone(), 8).unwrap()],
            4,
        )
        .unwrap();
        let registry = PredicateRegistry::from_keys([candidate.clone()]).unwrap();
        let bound = budget.shared_budget().constraint().bind(&registry).unwrap();
        assert_eq!(
            bound.evaluate(&FactSet::new()).unwrap(),
            TruthValue::Unknown
        );
        let facts = FactSet::new()
            .with(registry.id(&candidate).unwrap(), TruthValue::True)
            .unwrap();
        assert_eq!(bound.evaluate(&facts).unwrap(), TruthValue::False);
    }

    #[test]
    fn zero_cost_is_rejected_instead_of_hidden_normalization() {
        assert!(matches!(
            CapacityBudgetTerm::new(id("model"), key("zero"), 0),
            Err(CapacityBudgetError::ZeroCost { .. })
        ));
    }

    #[test]
    fn resource_group_rejects_capacity_cost_owned_by_non_member() {
        let budget = CapacityBudgetContract::storage(
            SharedBudgetId::new("storage").unwrap(),
            vec![CapacityBudgetTerm::new(id("outside"), key("outside"), 1024).unwrap()],
            4096,
        )
        .unwrap();
        assert!(matches!(
            ResourceGroupBuilder::new(super::super::ResourceGroupId::new("edge").unwrap())
                .member(id("inside"))
                .capacity_budget(budget)
                .build(),
            Err(super::super::ResourceGroupError::UnknownSharedBudgetMember { .. })
        ));
    }

    #[test]
    fn safety_envelope_reserves_capacity_before_adaptive_budget() {
        let flight = ImmutableCapacityReservation::new(
            SafetyReservationId::new("flight-control").unwrap(),
            CapacityBudgetKind::Ram,
            id("flight"),
            ContractId::new("flight.deadline-memory").unwrap(),
            4_096,
        )
        .unwrap();
        let envelope = SafetyCapacityEnvelope::new(
            CapacityBudgetKind::Ram,
            SharedBudgetId::new("adaptive-ram").unwrap(),
            16_384,
            vec![flight],
            vec![CapacityBudgetTerm::new(id("inference"), key("expanded"), 8_192).unwrap()],
        )
        .unwrap();
        assert_eq!(envelope.reserved_bytes(), 4_096);
        assert_eq!(envelope.adaptive_ceiling_bytes(), 12_288);
        assert_eq!(
            envelope.adaptive_budget().shared_budget().scale().unit(),
            RAM_CAPACITY_BUDGET_UNIT
        );
    }

    #[test]
    fn safety_envelope_rejects_overreservation_and_kind_drift() {
        let storage_reservation = ImmutableCapacityReservation::new(
            SafetyReservationId::new("storage").unwrap(),
            CapacityBudgetKind::Storage,
            id("flight"),
            ContractId::new("flight.storage").unwrap(),
            5,
        )
        .unwrap();
        assert!(matches!(
            SafetyCapacityEnvelope::new(
                CapacityBudgetKind::Ram,
                SharedBudgetId::new("adaptive").unwrap(),
                10,
                vec![storage_reservation],
                vec![CapacityBudgetTerm::new(id("inference"), key("x"), 1).unwrap()],
            ),
            Err(SafetyCapacityEnvelopeError::ReservationKindMismatch { .. })
        ));
        let ram_reservation = ImmutableCapacityReservation::new(
            SafetyReservationId::new("ram").unwrap(),
            CapacityBudgetKind::Ram,
            id("flight"),
            ContractId::new("flight.ram").unwrap(),
            11,
        )
        .unwrap();
        assert!(matches!(
            SafetyCapacityEnvelope::new(
                CapacityBudgetKind::Ram,
                SharedBudgetId::new("adaptive").unwrap(),
                10,
                vec![ram_reservation],
                vec![CapacityBudgetTerm::new(id("inference"), key("x"), 1).unwrap()],
            ),
            Err(SafetyCapacityEnvelopeError::ReservationsExceedTotal { .. })
        ));
    }

    #[test]
    fn group_rejects_safety_reservation_owned_by_non_member() {
        let envelope = SafetyCapacityEnvelope::new(
            CapacityBudgetKind::Ram,
            SharedBudgetId::new("adaptive-ram").unwrap(),
            16_384,
            vec![ImmutableCapacityReservation::new(
                SafetyReservationId::new("outside-safety").unwrap(),
                CapacityBudgetKind::Ram,
                id("outside"),
                ContractId::new("outside.safety").unwrap(),
                4_096,
            )
            .unwrap()],
            vec![CapacityBudgetTerm::new(id("inside"), key("expanded"), 8_192).unwrap()],
        )
        .unwrap();
        assert!(matches!(
            ResourceGroupBuilder::new(super::super::ResourceGroupId::new("edge").unwrap())
                .member(id("inside"))
                .safety_capacity_envelope(envelope)
                .build(),
            Err(super::super::ResourceGroupError::UnknownSafetyReservationMember { .. })
        ));
    }
}
