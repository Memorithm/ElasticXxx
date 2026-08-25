//! Realization lifecycle: the PLAN → VALIDATE → ACT → VERIFY → COMMIT /
//! ROLLBACK state machine for selected kernel realizations.
//!
//! A selection recommendation is not an execution. Between planning and a
//! committed realization sit correctness qualification, activation (for
//! example, pipeline compilation), and verification. Every stage can fail or
//! be rolled back, and a failed shader compile or parity check must never
//! become a committed realization.
//!
//! The type-state encoding below makes illegal sequences unrepresentable:
//! each stage is its own type, advancement requires that stage's named
//! attestations, and only the verified stage can commit. This deliberately
//! models *kernel-realization* switching as its own lifecycle rather than
//! forcing it into the data-transition taxonomy (`Reinterpret`, `Reencode`,
//! `Recompute`): swapping an executable implementation transforms no stored
//! data, so none of those mechanisms describes it. A future core extension
//! may add a dedicated mechanism; until then this lifecycle is the honest
//! vocabulary.

use std::fmt;
use std::marker::PhantomData;

use elastic_core::ContractId;
use elastic_eir::Fingerprint;

use crate::candidate::{KernelCandidate, RealizationIdentity};

/// Canonical namespace tag for committed-realization fingerprints.
pub(crate) const COMMITTED_FINGERPRINT_DOMAIN: &str = "elastic-kernel/committed/v1";

/// Claim that one stage's obligation was discharged by a trusted boundary.
///
/// Like `elastic-core::TransitionAttestations`, these are claims, not proofs
/// authenticated by Elastic. The private fields force call sites to name
/// every trust decision through the provided constructors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageAttestations {
    validation_ok: bool,
    activation_ok: bool,
    verification_ok: bool,
}

impl StageAttestations {
    /// No positive attestations.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            validation_ok: false,
            activation_ok: false,
            verification_ok: false,
        }
    }

    /// Attest that the candidate passed correctness qualification against
    /// the deterministic oracle.
    #[must_use]
    pub const fn attesting_validation(mut self) -> Self {
        self.validation_ok = true;
        self
    }

    /// Attest that the realization was activated on the target boundary
    /// (for example, compiled and a pipeline created successfully).
    #[must_use]
    pub const fn attesting_activation(mut self) -> Self {
        self.activation_ok = true;
        self
    }

    /// Attest that post-activation verification passed on real execution.
    #[must_use]
    pub const fn attesting_verification(mut self) -> Self {
        self.verification_ok = true;
        self
    }
}

/// Why a stage refused to advance.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StageRejection {
    /// The required attestation for this stage was not supplied.
    MissingAttestation {
        /// Canonical name of the missing attestation.
        required: &'static str,
    },
    /// A caller-supplied reason for rolling back or rejecting.
    ExplicitReason(String),
}

impl fmt::Display for StageRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAttestation { required } => {
                write!(f, "stage rejected: missing `{required}` attestation")
            }
            Self::ExplicitReason(reason) => write!(f, "stage rolled back: {reason}"),
        }
    }
}

impl std::error::Error for StageRejection {}

/// Terminal record of a rolled-back proposal.
///
/// Rollback is a first-class outcome: it records where the attempt stopped
/// and why, so evidence systems can distinguish "never activated" from
/// "activated but not verified" from "verified then withdrawn".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RolledBackRealization {
    realization: RealizationIdentity,
    stopped_at: &'static str,
    reason: String,
}

impl RolledBackRealization {
    /// Realization identity that was rolled back.
    #[must_use]
    pub fn realization(&self) -> &RealizationIdentity {
        &self.realization
    }

    /// Stage name where the rollback happened (`proposed`, `validated`,
    /// `activated`, or `verified`).
    #[must_use]
    pub const fn stopped_at(&self) -> &'static str {
        self.stopped_at
    }

    /// Caller-supplied rollback reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Record of one blocked stage transition.
///
/// The failed proposal is consumed by the failing transition; this record
/// carries the identity and stage needed to file a
/// [`RolledBackRealization`] without keeping the proposal alive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageFailure {
    realization: RealizationIdentity,
    stopped_at: &'static str,
    rejection: StageRejection,
}

impl StageFailure {
    fn from_proposal<S: Stage>(proposal: &RealizationProposal<S>, required: &'static str) -> Self {
        Self {
            realization: proposal.candidate.realization().clone(),
            stopped_at: S::NAME,
            rejection: StageRejection::MissingAttestation { required },
        }
    }

    /// Realization identity whose transition was blocked.
    #[must_use]
    pub fn realization(&self) -> &RealizationIdentity {
        &self.realization
    }

    /// Stage where the attempt stopped.
    #[must_use]
    pub const fn stopped_at(&self) -> &'static str {
        self.stopped_at
    }

    /// The rejection itself.
    #[must_use]
    pub const fn rejection(&self) -> &StageRejection {
        &self.rejection
    }

    /// File the rollback with an explicit reason.
    #[must_use]
    pub fn rollback_record(self, reason: impl Into<String>) -> RolledBackRealization {
        RolledBackRealization {
            realization: self.realization,
            stopped_at: self.stopped_at,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for StageFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "realization `{}` stopped at `{}`: {}",
            self.realization, self.stopped_at, self.rejection
        )
    }
}

impl std::error::Error for StageFailure {}

/// A selected candidate entering the lifecycle at the proposed stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealizationProposal<S> {
    candidate: KernelCandidate,
    selection_fingerprint: Fingerprint,
    _stage: PhantomData<S>,
}

/// Lifecycle stages, each carrying its canonical name for evidence records.
pub trait Stage {
    /// Canonical stage name (`proposed`, `validated`, `activated`,
    /// `verified`).
    const NAME: &'static str;
}

/// Proposed: planned but not yet validated.
#[derive(Clone, Copy, Debug)]
pub struct Proposed;
/// Validated: correctness qualified; awaiting activation.
#[derive(Clone, Copy, Debug)]
pub struct Validated;
/// Activated: realized on the boundary; awaiting verification.
#[derive(Clone, Copy, Debug)]
pub struct Activated;
/// Verified: execution verified; eligible to commit.
#[derive(Clone, Copy, Debug)]
pub struct Verified;

impl Stage for Proposed {
    const NAME: &'static str = "proposed";
}
impl Stage for Validated {
    const NAME: &'static str = "validated";
}
impl Stage for Activated {
    const NAME: &'static str = "activated";
}
impl Stage for Verified {
    const NAME: &'static str = "verified";
}

impl<S: Stage> RealizationProposal<S> {
    /// Abandon the proposal from any stage with an explicit reason.
    pub fn rollback(self, reason: impl Into<String>) -> RolledBackRealization {
        RolledBackRealization {
            realization: self.candidate.realization().clone(),
            stopped_at: S::NAME,
            reason: reason.into(),
        }
    }

    fn failure(&self, required: &'static str) -> StageFailure {
        StageFailure::from_proposal(self, required)
    }
}

impl RealizationProposal<Proposed> {
    /// Enter the lifecycle with a planner-selected candidate.
    #[must_use]
    pub fn start(candidate: KernelCandidate, selection_fingerprint: Fingerprint) -> Self {
        Self {
            candidate,
            selection_fingerprint,
            _stage: PhantomData,
        }
    }

    /// Validate the proposal: correctness qualification and any structural
    /// pre-activation checks happen before this call succeeds.
    ///
    /// # Errors
    ///
    /// Returns a [`StageFailure`] naming the missing attestation when
    /// `attestations` does not carry the validation attestation. The failed
    /// proposal is consumed; the failure carries everything needed to record
    /// the rollback.
    pub fn validate(
        self,
        attestations: StageAttestations,
    ) -> Result<RealizationProposal<Validated>, StageFailure> {
        if !attestations.validation_ok {
            return Err(self.failure("validation"));
        }
        Ok(RealizationProposal {
            candidate: self.candidate,
            selection_fingerprint: self.selection_fingerprint,
            _stage: PhantomData,
        })
    }
}

impl RealizationProposal<Validated> {
    /// Activate the realization on the target boundary.
    ///
    /// # Errors
    ///
    /// Returns [`StageRejection::MissingAttestation`] unless `attestations`
    /// carries the activation attestation (for example: pipeline compiled).
    pub fn activate(
        self,
        attestations: StageAttestations,
    ) -> Result<RealizationProposal<Activated>, StageFailure> {
        if !attestations.activation_ok {
            return Err(self.failure("activation"));
        }
        Ok(RealizationProposal {
            candidate: self.candidate,
            selection_fingerprint: self.selection_fingerprint,
            _stage: PhantomData,
        })
    }
}

impl RealizationProposal<Activated> {
    /// Verify behavior of the activated realization.
    ///
    /// # Errors
    ///
    /// Returns [`StageRejection::MissingAttestation`] unless `attestations`
    /// carries the verification attestation (for example: device-side
    /// parity check passed).
    pub fn verify(
        self,
        attestations: StageAttestations,
    ) -> Result<RealizationProposal<Verified>, StageFailure> {
        if !attestations.verification_ok {
            return Err(self.failure("verification"));
        }
        Ok(RealizationProposal {
            candidate: self.candidate,
            selection_fingerprint: self.selection_fingerprint,
            _stage: PhantomData,
        })
    }
}

/// A realization whose transition completed successfully.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedRealization {
    logical_resource_id: elastic_core::LogicalResourceId,
    realization: RealizationIdentity,
    schema_version: u32,
    contract: ContractId,
    selection_fingerprint: Fingerprint,
    fingerprint: Fingerprint,
}

impl CommittedRealization {
    /// Logical resource now served by this realization.
    #[must_use]
    pub fn logical_resource_id(&self) -> &elastic_core::LogicalResourceId {
        &self.logical_resource_id
    }

    /// Committed realization identity.
    #[must_use]
    pub fn realization(&self) -> &RealizationIdentity {
        &self.realization
    }

    /// Schema version of the committed description.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Contract upheld by the committed realization.
    #[must_use]
    pub const fn contract(&self) -> &ContractId {
        &self.contract
    }

    /// Selection-record fingerprint that led here.
    #[must_use]
    pub const fn selection_fingerprint(&self) -> Fingerprint {
        self.selection_fingerprint
    }

    /// Structural fingerprint over the commitment itself.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

impl RealizationProposal<Verified> {
    /// Commit the verified realization.
    #[must_use]
    pub fn commit(self) -> CommittedRealization {
        let logical = self.candidate.logical_resource_id().clone();
        let realization = self.candidate.realization().clone();
        let schema_version = self.candidate.schema_version();
        let contract = self.candidate.contract().clone();
        let fingerprint = Fingerprint::EMPTY
            .text(COMMITTED_FINGERPRINT_DOMAIN)
            .text(logical.as_str())
            .text(realization.as_str())
            .number(u64::from(schema_version))
            .text(contract.as_str())
            .number(self.selection_fingerprint.bits());
        CommittedRealization {
            logical_resource_id: logical,
            realization,
            schema_version,
            contract,
            selection_fingerprint: self.selection_fingerprint,
            fingerprint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::{
        Evidence, EvidenceUnit, KernelCandidate, ObjectiveEvidence, StaticQuantity,
    };
    use crate::requirements::KernelRequirements;
    use elastic_core::ObjectiveId;

    fn requirements() -> KernelRequirements {
        KernelRequirements {
            invocations_per_workgroup: 64,
            invocations_per_axis: [64, 1, 1],
            workgroup_storage_bytes: 1024,
            bind_groups: 2,
            max_storage_buffer_binding_bytes: 4096,
            subgroup_min_width: None,
            shader_f16: crate::requirements::FeatureRequirement::NotRequired,
            matrix_ops: crate::requirements::FeatureRequirement::NotRequired,
        }
    }

    fn candidate(realization: &str) -> KernelCandidate {
        KernelCandidate::new(
            elastic_core::LogicalResourceId::new("attention#42").expect("valid"),
            RealizationIdentity::new(realization).expect("valid"),
            1,
            requirements(),
            ContractId::new("attention-forward-v1").expect("valid"),
            ObjectiveEvidence::new().with(
                ObjectiveId::builtin(elastic_core::BuiltinObjective::Latency),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 10,
                    unit: EvidenceUnit::Nanoseconds,
                }),
            ),
        )
        .expect("fixture valid")
    }

    fn proposal() -> RealizationProposal<Proposed> {
        RealizationProposal::start(
            candidate("portable-q4"),
            Fingerprint::EMPTY.text("selection"),
        )
    }

    #[test]
    fn full_lifecycle_commits_only_after_every_attestation() {
        let none = StageAttestations::none();
        assert!(proposal().validate(none).is_err());

        let validated = proposal()
            .validate(none.attesting_validation())
            .expect("validation attested");
        assert!(validated.clone().activate(none).is_err());

        let activated = validated
            .activate(none.attesting_activation())
            .expect("activation attested");
        assert!(activated.clone().verify(none).is_err());

        let verified = activated
            .verify(none.attesting_verification())
            .expect("verification attested");
        let committed = verified.commit();
        assert_eq!(committed.realization().as_str(), "portable-q4");
        assert_eq!(committed.fingerprint(), committed.fingerprint());
    }

    #[test]
    fn failed_stages_produce_rollback_records() {
        let failure = proposal()
            .validate(StageAttestations::none())
            .expect_err("missing attestation");
        assert_eq!(
            *failure.rejection(),
            StageRejection::MissingAttestation {
                required: "validation"
            }
        );
        let rolled_back = failure.rollback_record("parity check failed");
        assert_eq!(rolled_back.stopped_at(), "proposed");
        assert_eq!(rolled_back.reason(), "parity check failed");
    }

    #[test]
    fn rollback_records_the_stage_it_stopped_at() {
        let validated = proposal()
            .validate(StageAttestations::none().attesting_validation())
            .expect("validated");
        assert_eq!(
            validated
                .clone()
                .rollback("pipeline compile failed")
                .stopped_at(),
            "validated"
        );

        let activation_failure = validated
            .activate(StageAttestations::none())
            .expect_err("missing activation attestation");
        assert_eq!(activation_failure.stopped_at(), "validated");
        assert_eq!(
            activation_failure.rollback_record("compile error").reason(),
            "compile error"
        );

        let activated = proposal()
            .validate(StageAttestations::none().attesting_validation())
            .expect("validated")
            .activate(StageAttestations::none().attesting_activation())
            .expect("activated");
        assert_eq!(
            activated.clone().rollback("device hang").stopped_at(),
            "activated"
        );

        let verification_failure = activated
            .verify(StageAttestations::none())
            .expect_err("missing verification attestation");
        assert_eq!(verification_failure.stopped_at(), "activated");
    }

    #[test]
    fn commitments_from_different_selections_differ() {
        let left = proposal()
            .validate(StageAttestations::none().attesting_validation())
            .expect("validated")
            .activate(StageAttestations::none().attesting_activation())
            .expect("activated")
            .verify(StageAttestations::none().attesting_verification())
            .expect("verified")
            .commit();

        let other = RealizationProposal::start(
            candidate("subgroup-q4"),
            Fingerprint::EMPTY.text("other-selection"),
        )
        .validate(StageAttestations::none().attesting_validation())
        .expect("validated")
        .activate(StageAttestations::none().attesting_activation())
        .expect("activated")
        .verify(StageAttestations::none().attesting_verification())
        .expect("verified")
        .commit();
        assert_ne!(left.fingerprint(), other.fingerprint());
    }
}
