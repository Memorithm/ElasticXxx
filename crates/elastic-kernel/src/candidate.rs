//! The kernel-candidate contract: one admissible realization of a logical
//! kernel resource.
//!
//! A [`KernelCandidate`] couples the *logical* identity of a computation (a
//! [`LogicalResourceId`], stable across realizations) with one concrete
//! *realization* ([`RealizationIdentity`]) and everything the Elastic layer
//! needs to judge it: capability requirements, the semantic contract it
//! upholds, and objective evidence.
//!
//! Evidence discipline: measured facts, static estimates, and absence of
//! knowledge are different types. A guessed latency can never inhabit
//! [`Evidence::Measured`]; a proven architectural count (such as an explicit
//! static load model) lives in [`Evidence::StaticEstimate`] and is never
//! presented as a measurement.

use std::collections::BTreeMap;
use std::fmt;

use elastic_core::{ContractId, LogicalResourceId, ObjectiveId};
use elastic_eir::Fingerprint;

use crate::requirements::KernelRequirements;

/// Canonical namespace tag for candidate fingerprints.
pub(crate) const CANDIDATE_FINGERPRINT_DOMAIN: &str = "elastic-kernel/candidate/v1";

/// Identity of one concrete realization of a logical kernel resource.
///
/// Examples of what adapters may encode: `"portable-q4-tiled"`,
/// `"subgroup-q4"`. The identity must be stable for the same implementation;
/// changing the implementation changes the identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RealizationIdentity(String);

impl RealizationIdentity {
    /// Create a realization identity from non-blank text.
    ///
    /// # Errors
    ///
    /// Returns [`RealizationIdentityError::Empty`] when the text is empty or
    /// blank.
    pub fn new(text: impl Into<String>) -> Result<Self, RealizationIdentityError> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(RealizationIdentityError::Empty);
        }
        Ok(Self(text))
    }

    /// The canonical text of this identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RealizationIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Errors produced while constructing realization identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RealizationIdentityError {
    /// The supplied text was empty or blank.
    Empty,
}

impl fmt::Display for RealizationIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "realization identity must be non-empty"),
        }
    }
}

impl std::error::Error for RealizationIdentityError {}

/// Physical units carried by objective evidence.
///
/// Units keep distinct physical quantities incomparable by construction:
/// nanoseconds never mix with bytes. Within one unit, magnitudes compare
/// directly under the objective's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvidenceUnit {
    /// Duration in nanoseconds.
    Nanoseconds,
    /// Memory footprint in bytes.
    Bytes,
    /// Throughput in operations per second.
    OperationsPerSecond,
    /// Energy in millijoules.
    Millijoules,
    /// Dimensionless score where no physical unit applies (for example a
    /// stability rating).
    Dimensionless,
}

impl EvidenceUnit {
    /// Stable canonical name used in records and fingerprints.
    #[must_use]
    pub const fn canonical(self) -> &'static str {
        match self {
            Self::Nanoseconds => "ns",
            Self::Bytes => "bytes",
            Self::OperationsPerSecond => "ops/s",
            Self::Millijoules => "mJ",
            Self::Dimensionless => "dimensionless",
        }
    }
}

impl fmt::Display for EvidenceUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical())
    }
}

/// Objective evidence attached to one candidate.
///
/// The three variants encode three epistemic states; planners must never
/// merge them into a single untyped score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    /// Genuinely measured on hardware under a recorded protocol. Never used
    /// for guessed or projected values.
    Measured(MeasuredQuantity),
    /// Output of a proven static model over declared architecture (for
    /// example, an explicit logical K/V staging-load count). Not a claim
    /// about wall-clock behavior.
    StaticEstimate(StaticQuantity),
    /// No comparable evidence exists for this candidate and objective.
    Unknown,
}

impl Evidence {
    /// Evidence tier used for deterministic comparison.
    ///
    /// Measured evidence outranks static estimates: when any candidate
    /// carries measurements for an objective, candidates without
    /// measurements are not compared against them on that objective.
    #[must_use]
    pub const fn tier(self) -> EvidenceTier {
        match self {
            Self::Measured(_) => EvidenceTier::Measured,
            Self::StaticEstimate(_) => EvidenceTier::Static,
            Self::Unknown => EvidenceTier::Unknown,
        }
    }

    /// The quantity magnitude and unit, if this evidence carries one.
    #[must_use]
    pub const fn quantity(&self) -> Option<(u64, EvidenceUnit)> {
        match self {
            Self::Measured(quantity) => Some((quantity.magnitude, quantity.unit)),
            Self::StaticEstimate(quantity) => Some((quantity.magnitude, quantity.unit)),
            Self::Unknown => None,
        }
    }

    /// Deterministic fingerprint contribution of this evidence value.
    #[must_use]
    pub fn fingerprint_part(self) -> Fingerprint {
        let mut fp = Fingerprint::EMPTY.text("evidence");
        fp = match self {
            Self::Measured(quantity) => fp
                .text("measured")
                .number(quantity.magnitude)
                .text(quantity.unit.canonical())
                .number(u64::from(quantity.protocol_version))
                .number(u64::from(quantity.samples)),
            Self::StaticEstimate(quantity) => fp
                .text("static")
                .number(quantity.magnitude)
                .text(quantity.unit.canonical()),
            Self::Unknown => fp.text("unknown"),
        };
        fp
    }
}

/// A measurement produced on hardware under a recorded protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredQuantity {
    /// Non-negative magnitude in `unit`.
    pub magnitude: u64,
    /// Physical unit of `magnitude`.
    pub unit: EvidenceUnit,
    /// Version of the benchmark protocol that produced this measurement.
    pub protocol_version: u32,
    /// Number of protocol samples summarized by `magnitude`.
    pub samples: u32,
}

/// A statically derived estimate with a proven derivation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticQuantity {
    /// Non-negative magnitude in `unit`.
    pub magnitude: u64,
    /// Physical (or architectural) unit of `magnitude`.
    pub unit: EvidenceUnit,
}

/// Reliability tier of evidence, ordered by how much a planner may trust it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvidenceTier {
    /// Nothing available.
    Unknown,
    /// Static, provably-derived estimate.
    Static,
    /// Hardware measurement under protocol.
    Measured,
}

/// Per-objective evidence map for one candidate.
///
/// Objectives absent from the map mean [`Evidence::Unknown`] for that
/// objective; insertion order is irrelevant because the backing store is
/// ordered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectiveEvidence {
    entries: BTreeMap<ObjectiveId, Evidence>,
}

impl ObjectiveEvidence {
    /// An empty evidence set (every objective unknown).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach evidence to one objective, replacing any previous entry.
    pub fn attach(&mut self, objective: ObjectiveId, evidence: Evidence) {
        self.entries.insert(objective, evidence);
    }

    /// Builder-style [`ObjectiveEvidence::attach`].
    #[must_use]
    pub fn with(mut self, objective: ObjectiveId, evidence: Evidence) -> Self {
        self.attach(objective, evidence);
        self
    }

    /// The evidence recorded for one objective (`Evidence::Unknown` when
    /// absent).
    #[must_use]
    pub fn get(&self, objective: &ObjectiveId) -> Evidence {
        self.entries
            .get(objective)
            .copied()
            .unwrap_or(Evidence::Unknown)
    }

    /// Recorded `(objective, evidence)` pairs in canonical objective order.
    pub fn iter(&self) -> impl Iterator<Item = (&ObjectiveId, &Evidence)> {
        self.entries.iter()
    }

    /// Deterministic fingerprint contribution of the whole evidence set.
    #[must_use]
    pub fn fingerprint_part(&self) -> Fingerprint {
        let mut fp = Fingerprint::EMPTY
            .text("objective-evidence")
            .number(self.entries.len() as u64);
        for (objective, evidence) in &self.entries {
            fp = fp.text(objective.as_str());
            fp = fp.number(evidence.fingerprint_part().bits());
        }
        fp
    }
}

/// One candidate realization offered to planning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelCandidate {
    logical_resource_id: LogicalResourceId,
    realization: RealizationIdentity,
    schema_version: u32,
    requirements: KernelRequirements,
    contract: ContractId,
    evidence: ObjectiveEvidence,
}

impl KernelCandidate {
    /// Assemble and validate one candidate.
    ///
    /// # Errors
    ///
    /// Returns the underlying construction error when the realization
    /// identity is blank or the requirement record is internally
    /// inconsistent.
    pub fn new(
        logical_resource_id: LogicalResourceId,
        realization: RealizationIdentity,
        schema_version: u32,
        requirements: KernelRequirements,
        contract: ContractId,
        evidence: ObjectiveEvidence,
    ) -> Result<Self, CandidateError> {
        requirements.validate()?;
        Ok(Self {
            logical_resource_id,
            realization,
            schema_version,
            requirements,
            contract,
            evidence,
        })
    }

    /// Logical resource this candidate realizes.
    #[must_use]
    pub fn logical_resource_id(&self) -> &LogicalResourceId {
        &self.logical_resource_id
    }

    /// Concrete realization identity.
    #[must_use]
    pub fn realization(&self) -> &RealizationIdentity {
        &self.realization
    }

    /// Schema version of the realization description.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Capability requirements of this realization.
    #[must_use]
    pub const fn requirements(&self) -> &KernelRequirements {
        &self.requirements
    }

    /// Semantic contract this realization upholds.
    #[must_use]
    pub const fn contract(&self) -> &ContractId {
        &self.contract
    }

    /// Objective evidence carried by this candidate.
    #[must_use]
    pub const fn evidence(&self) -> &ObjectiveEvidence {
        &self.evidence
    }

    /// Deterministic structural fingerprint of the full candidate.
    #[must_use]
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::EMPTY
            .text(CANDIDATE_FINGERPRINT_DOMAIN)
            .text(self.logical_resource_id.as_str())
            .text(self.realization.as_str())
            .number(u64::from(self.schema_version))
            .number(self.requirements.fingerprint().bits())
            .text(self.contract.as_str())
            .number(self.evidence.fingerprint_part().bits())
    }
}

/// Errors produced while assembling kernel candidates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CandidateError {
    /// The requirement record failed internal validation.
    InvalidRequirements(crate::requirements::RequirementsError),
}

impl From<crate::requirements::RequirementsError> for CandidateError {
    fn from(error: crate::requirements::RequirementsError) -> Self {
        Self::InvalidRequirements(error)
    }
}

impl fmt::Display for CandidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequirements(error) => write!(f, "invalid requirements: {error}"),
        }
    }
}

impl std::error::Error for CandidateError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requirements::FeatureRequirement;

    fn latency_objective() -> ObjectiveId {
        ObjectiveId::builtin(elastic_core::BuiltinObjective::Latency)
    }

    fn requirements_fixture() -> KernelRequirements {
        KernelRequirements {
            invocations_per_workgroup: 64,
            invocations_per_axis: [64, 1, 1],
            workgroup_storage_bytes: 1024,
            bind_groups: 2,
            max_storage_buffer_binding_bytes: 4096,
            subgroup_min_width: None,
            shader_f16: FeatureRequirement::NotRequired,
            matrix_ops: FeatureRequirement::NotRequired,
        }
    }

    fn candidate_fixture(realization: &str) -> KernelCandidate {
        KernelCandidate::new(
            LogicalResourceId::new("attention#42").expect("valid id"),
            RealizationIdentity::new(realization).expect("valid id"),
            1,
            requirements_fixture(),
            ContractId::new("attention-forward-v1").expect("valid id"),
            ObjectiveEvidence::new().with(
                latency_objective(),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 100,
                    unit: EvidenceUnit::Nanoseconds,
                }),
            ),
        )
        .expect("fixture is valid")
    }

    #[test]
    fn blank_realization_identity_is_rejected() {
        assert_eq!(
            RealizationIdentity::new("  "),
            Err(RealizationIdentityError::Empty)
        );
    }

    #[test]
    fn evidence_tiers_are_totally_ordered_by_trustworthiness() {
        let unknown = Evidence::Unknown.tier();
        let statik = Evidence::StaticEstimate(StaticQuantity {
            magnitude: 1,
            unit: EvidenceUnit::Nanoseconds,
        })
        .tier();
        let measured = Evidence::Measured(MeasuredQuantity {
            magnitude: 1,
            unit: EvidenceUnit::Nanoseconds,
            protocol_version: 1,
            samples: 9,
        })
        .tier();
        assert!(unknown < statik);
        assert!(statik < measured);
    }

    #[test]
    fn missing_evidence_entries_read_as_unknown() {
        let evidence = ObjectiveEvidence::new();
        assert_eq!(evidence.get(&latency_objective()), Evidence::Unknown);
    }

    #[test]
    fn candidate_fingerprints_are_identity_sensitive_and_deterministic() {
        let baseline = candidate_fixture("portable-q4");
        assert_eq!(baseline.fingerprint(), baseline.fingerprint());

        let other_realization = candidate_fixture("subgroup-q4");
        assert_ne!(baseline.fingerprint(), other_realization.fingerprint());

        let more_evidence = KernelCandidate::new(
            LogicalResourceId::new("attention#42").expect("valid id"),
            RealizationIdentity::new("portable-q4").expect("valid id"),
            1,
            requirements_fixture(),
            ContractId::new("attention-forward-v1").expect("valid id"),
            ObjectiveEvidence::new(),
        )
        .expect("valid");
        assert_ne!(baseline.fingerprint(), more_evidence.fingerprint());
    }
}
