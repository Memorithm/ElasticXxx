//! BE14e fail-closed Boolean screening for fixed-width representation candidates.
//!
//! This is a planning-only bridge around the existing representation contract.
//! It does **not** encode, reinterpret, recompute, validate trusted-boundary
//! attestations, or mutate a [`VersionFrontier`](elastic_core::VersionFrontier).
//! A `True` Boolean result means only that a candidate survived an explicitly
//! declared fixed-width precision floor and the current structural/capability
//! snapshot. The existing [`RepresentationTransition::validate`](elastic_core::RepresentationTransition::validate)
//! boundary remains authoritative before any actuation.
//!
//! The numeric source is the custom observation
//! `representation-required-precision-bits`, with unit
//! `declared-bits-per-scalar`. The quantity is deliberately a declaration-level
//! bit width, not a claim about numerical error, model quality, entropy, or
//! effective information content. Variable-width/codebook representations must
//! use another qualified contract instead of fabricating a bit width here.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use elastic_core::resource::{DimensionId, ObservationSignalId, RepresentationalDeclaration};
use elastic_core::{
    BoolExpr, BooleanGuard, CapabilitySet, FreshnessSnapshot, GuardFactSource, GuardScope,
    GuardedResourceSpec, ObservationEpoch, PlannerEpoch, PredicateKey, PredicateRegistry,
    RepresentationId, RepresentationState, ResourceGeneration, TransitionMechanism, TruthValue,
};
use elastic_eir::{lower_guarded, EirGuardedResource};
use serde::{Deserialize, Serialize};

use crate::{
    BooleanGuardPreplanner, FactResourceBinding, FactSnapshot, FactSourceId, ObservationSnapshot,
    PredicateEvaluationInput, PredicateEvaluator,
};

/// Namespace for BE14e representation/precision predicates.
pub const REPRESENTATION_PRECISION_PREDICATE_NAMESPACE: &str = "elastic.representation-precision";
/// Stable predicate: a fixed-width candidate satisfies the declared precision floor.
pub const REPRESENTATION_PRECISION_FLOOR_PREDICATE_NAME: &str =
    "declared-precision-floor-satisfied";
/// Custom numeric observation supplying the required fixed-width precision floor.
pub const REPRESENTATION_PRECISION_FLOOR_SIGNAL_NAME: &str =
    "representation-required-precision-bits";
/// Unit of both the source observation and candidate fixed-width metadata.
pub const REPRESENTATION_PRECISION_SOURCE_UNIT: &str = "declared-bits-per-scalar";
/// Freshness envelope for the explicit precision-floor observation.
pub const REPRESENTATION_PRECISION_MAX_AGE: Duration = Duration::from_secs(1);
/// Bounded candidate set for one deterministic preplanning pass.
pub const MAX_REPRESENTATION_PRECISION_CANDIDATES: usize = 64;
const MAX_EXACT_F64_INTEGER: f64 = 9_007_199_254_740_992.0;
const MAX_CANDIDATE_ID_BYTES: usize = 128;

/// Stable BE14e predicate key.
pub fn representation_precision_floor_predicate_key() -> PredicateKey {
    PredicateKey::new(
        REPRESENTATION_PRECISION_PREDICATE_NAMESPACE,
        REPRESENTATION_PRECISION_FLOOR_PREDICATE_NAME,
    )
    .expect("static BE14e PredicateKey is valid")
}

/// Explicit custom observation signal for the fixed-width precision floor.
pub fn representation_precision_floor_signal() -> ObservationSignalId {
    ObservationSignalId::custom(REPRESENTATION_PRECISION_FLOOR_SIGNAL_NAME)
        .expect("static BE14e observation signal is valid")
}

/// One explicitly declared fixed-width candidate considered by the BE14e preplanner.
///
/// `declared_precision_bits` is policy metadata. It is not a measured error bound
/// and must not be interpreted as proof of physical or scientific fidelity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationPrecisionCandidateV1 {
    candidate_id: String,
    preference_rank: u32,
    target: RepresentationId,
    target_schema_version: u32,
    mechanism: TransitionMechanism,
    declared_precision_bits: u16,
}

impl RepresentationPrecisionCandidateV1 {
    /// Construct one bounded fixed-width representation candidate.
    pub fn new(
        candidate_id: impl Into<String>,
        preference_rank: u32,
        target: RepresentationId,
        target_schema_version: u32,
        mechanism: TransitionMechanism,
        declared_precision_bits: u16,
    ) -> Result<Self, String> {
        let candidate_id = candidate_id.into();
        if candidate_id.trim().is_empty() {
            return Err("BE14e candidate id must not be blank".into());
        }
        if candidate_id.len() > MAX_CANDIDATE_ID_BYTES {
            return Err(format!(
                "BE14e candidate id exceeds {MAX_CANDIDATE_ID_BYTES} bytes"
            ));
        }
        if candidate_id.trim() != candidate_id {
            return Err("BE14e candidate id must not contain leading/trailing whitespace".into());
        }
        if declared_precision_bits == 0 {
            return Err("BE14e declared precision bits must be non-zero".into());
        }
        Ok(Self {
            candidate_id,
            preference_rank,
            target,
            target_schema_version,
            mechanism,
            declared_precision_bits,
        })
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub const fn preference_rank(&self) -> u32 {
        self.preference_rank
    }

    #[must_use]
    pub const fn mechanism(&self) -> TransitionMechanism {
        self.mechanism
    }

    #[must_use]
    pub const fn declared_precision_bits(&self) -> u16 {
        self.declared_precision_bits
    }

    #[must_use]
    pub const fn target_schema_version(&self) -> u32 {
        self.target_schema_version
    }

    #[must_use]
    pub const fn target(&self) -> &RepresentationId {
        &self.target
    }
}

/// Durable evidence for one screened representation candidate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanRepresentationPrecisionCandidateEvidenceV1 {
    pub candidate_id: String,
    pub preference_rank: u32,
    pub target_representation: String,
    pub target_schema_version: u32,
    pub mechanism: String,
    pub declared_precision_bits: u16,
    pub predicate_key: String,
    pub source_signal: String,
    pub source_unit: String,
    pub declaration_supports_target: bool,
    pub capability_supports_target: bool,
    pub truth: String,
    pub reason: String,
}

/// Fail-closed BE14e preplanning outcome.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BooleanRepresentationPrecisionOutcomeV1 {
    /// First provider-preferred candidate with complete `True` evidence.
    Selected {
        candidate_id: String,
        preference_rank: u32,
    },
    /// Every candidate was conclusively ineligible.
    NoCandidate,
    /// A potentially preferred candidate lacked usable evidence.
    InsufficientEvidence {
        candidate_id: String,
        preference_rank: u32,
    },
}

/// Durable planning-only BE14e screening report.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanRepresentationPrecisionReportV1 {
    pub schema_version: u16,
    pub current_representation: String,
    pub current_schema_version: u32,
    pub current_epoch: u64,
    pub source_signal: String,
    pub source_unit: String,
    pub observation_epoch: u64,
    pub resource_generation: u64,
    pub outcome: BooleanRepresentationPrecisionOutcomeV1,
    pub candidates: Vec<BooleanRepresentationPrecisionCandidateEvidenceV1>,
}

/// Planning-only BE14e preplanner over an existing representational declaration.
///
/// Construction compiles one Boolean guard per fixed-width candidate. Screening
/// is deterministic by ascending `preference_rank`; an `Unknown` candidate
/// blocks later candidates because silently skipping an unresolved, preferred
/// precision contract could change application/scientific semantics.
#[derive(Debug)]
pub struct BooleanRepresentationPrecisionPreplannerV1 {
    declaration: RepresentationalDeclaration,
    candidates: Vec<RepresentationPrecisionCandidateV1>,
    guarded: Vec<EirGuardedResource>,
}

impl BooleanRepresentationPrecisionPreplannerV1 {
    /// Bind the preplanner to an existing representational declaration and an
    /// explicitly bounded candidate policy.
    pub fn new(
        declaration: RepresentationalDeclaration,
        mut candidates: Vec<RepresentationPrecisionCandidateV1>,
    ) -> Result<Self, String> {
        if candidates.is_empty() {
            return Err("BE14e requires at least one representation candidate".into());
        }
        if candidates.len() > MAX_REPRESENTATION_PRECISION_CANDIDATES {
            return Err(format!(
                "BE14e candidate count exceeds {MAX_REPRESENTATION_PRECISION_CANDIDATES}"
            ));
        }
        let signal = representation_precision_floor_signal();
        if !declaration
            .spec()
            .observed_signals()
            .iter()
            .any(|declared| declared == &signal)
        {
            return Err(format!(
                "BE14e declaration must explicitly observe {REPRESENTATION_PRECISION_FLOOR_SIGNAL_NAME}"
            ));
        }

        candidates.sort_by_key(RepresentationPrecisionCandidateV1::preference_rank);
        let mut ids = BTreeSet::new();
        let mut ranks = BTreeSet::new();
        for candidate in &candidates {
            if !ids.insert(candidate.candidate_id.clone()) {
                return Err(format!(
                    "duplicate BE14e candidate id {:?}",
                    candidate.candidate_id
                ));
            }
            if !ranks.insert(candidate.preference_rank) {
                return Err(format!(
                    "duplicate BE14e preference rank {}",
                    candidate.preference_rank
                ));
            }
        }

        let mut guarded = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            let key = representation_precision_floor_predicate_key();
            let registry =
                PredicateRegistry::from_keys([key.clone()]).map_err(|error| error.to_string())?;
            let predicate_id = registry
                .id(&key)
                .expect("BE14e registry contains its fixed predicate");
            let guard = BooleanGuard::new(
                GuardScope::Transition {
                    mechanism: candidate.mechanism,
                    dimension: DimensionId::REPRESENTATION,
                },
                registry,
                BoolExpr::atom(predicate_id),
            )
            .map_err(|error| error.to_string())?;
            let guarded_spec = GuardedResourceSpec::new(declaration.spec().clone(), vec![guard])
                .map_err(|error| error.to_string())?;
            guarded.push(lower_guarded(&guarded_spec).map_err(|error| error.to_string())?);
        }

        Ok(Self {
            declaration,
            candidates,
            guarded,
        })
    }

    /// Screen fixed-width candidates without staging, validation, or actuation.
    #[allow(clippy::too_many_arguments)]
    pub fn screen(
        &self,
        current: &RepresentationState,
        capabilities: &CapabilitySet,
        planning_context: &elastic_eir::PlanningContext,
        observations: &ObservationSnapshot,
        now: Instant,
        observation_epoch: ObservationEpoch,
        resource_generation: ResourceGeneration,
    ) -> Result<BooleanRepresentationPrecisionReportV1, String> {
        if !self.declaration.supports(current) {
            return Err("current representation is outside the BE14e declaration".into());
        }

        let key = representation_precision_floor_predicate_key();
        let signal = representation_precision_floor_signal();
        let input = PredicateEvaluationInput::new(planning_context, observations, now);
        let mut evidence = Vec::with_capacity(self.candidates.len());
        let mut selected = None;
        let mut first_unknown = None;

        for (candidate, guarded) in self.candidates.iter().zip(&self.guarded) {
            let target = self
                .declaration
                .derive_target(
                    current,
                    candidate.target(),
                    candidate.target_schema_version,
                    candidate.mechanism,
                )
                .ok();
            let declaration_supports_target = target.is_some();
            let capability_supports_target = target
                .as_ref()
                .is_some_and(|target| capabilities.supports(target));

            let (truth, reason) = if !declaration_supports_target {
                (
                    TruthValue::False,
                    "target-not-declared-or-mechanism-not-admitted",
                )
            } else if !capability_supports_target {
                (
                    TruthValue::False,
                    "target-not-in-trusted-capability-snapshot",
                )
            } else {
                let evaluator = RequiredPrecisionBitsPredicate {
                    key: key.clone(),
                    signal: signal.clone(),
                    candidate_bits: candidate.declared_precision_bits,
                    max_age: REPRESENTATION_PRECISION_MAX_AGE,
                };
                let facts = FactSnapshot::derive(
                    FactSourceId::new("elastic-runtime:be14e-representation-precision")
                        .map_err(|error| error.to_string())?,
                    observation_epoch,
                    Some(FactResourceBinding::new(
                        guarded.resource().identity().clone(),
                        resource_generation,
                    )),
                    &input,
                    &[&evaluator as &dyn PredicateEvaluator],
                )
                .map_err(|error| error.to_string())?;
                let freshness = FreshnessSnapshot::new(
                    PlannerEpoch::new(observation_epoch.get()),
                    observation_epoch,
                )
                .with_resource_generation(
                    guarded.resource().identity().clone(),
                    resource_generation,
                );
                let pruning = BooleanGuardPreplanner
                    .prune(guarded, &facts, &freshness)
                    .map_err(|error| error.to_string())?;
                let truth = facts.truth(&key);
                let eligible =
                    pruning.contains_eligible(candidate.mechanism, &DimensionId::REPRESENTATION);
                let reason = match (truth, eligible) {
                    (TruthValue::True, true) => "precision-floor-satisfied",
                    (TruthValue::True, false) => {
                        return Err(
                            "BE14e guard was true but the declared representation transition was not eligible"
                                .into(),
                        )
                    }
                    (TruthValue::False, _) => "precision-floor-not-satisfied",
                    (TruthValue::Unknown, _) => "precision-floor-evidence-unavailable",
                };
                (truth, reason)
            };

            evidence.push(BooleanRepresentationPrecisionCandidateEvidenceV1 {
                candidate_id: candidate.candidate_id.clone(),
                preference_rank: candidate.preference_rank,
                target_representation: candidate.target.as_str().to_owned(),
                target_schema_version: candidate.target_schema_version,
                mechanism: mechanism_name(candidate.mechanism).to_owned(),
                declared_precision_bits: candidate.declared_precision_bits,
                predicate_key: key.to_string(),
                source_signal: signal.as_str().to_owned(),
                source_unit: REPRESENTATION_PRECISION_SOURCE_UNIT.to_owned(),
                declaration_supports_target,
                capability_supports_target,
                truth: truth_text(truth).to_owned(),
                reason: reason.to_owned(),
            });

            match truth {
                TruthValue::False => {}
                TruthValue::Unknown => {
                    first_unknown =
                        Some((candidate.candidate_id.clone(), candidate.preference_rank));
                    break;
                }
                TruthValue::True => {
                    selected = Some((candidate.candidate_id.clone(), candidate.preference_rank));
                    break;
                }
            }
        }

        let outcome = if let Some((candidate_id, preference_rank)) = first_unknown {
            BooleanRepresentationPrecisionOutcomeV1::InsufficientEvidence {
                candidate_id,
                preference_rank,
            }
        } else if let Some((candidate_id, preference_rank)) = selected {
            BooleanRepresentationPrecisionOutcomeV1::Selected {
                candidate_id,
                preference_rank,
            }
        } else {
            BooleanRepresentationPrecisionOutcomeV1::NoCandidate
        };

        Ok(BooleanRepresentationPrecisionReportV1 {
            schema_version: 1,
            current_representation: current.id.as_str().to_owned(),
            current_schema_version: current.schema_version,
            current_epoch: current.epoch.get(),
            source_signal: signal.as_str().to_owned(),
            source_unit: REPRESENTATION_PRECISION_SOURCE_UNIT.to_owned(),
            observation_epoch: observation_epoch.get(),
            resource_generation: resource_generation.get(),
            outcome,
            candidates: evidence,
        })
    }

    /// Resolve a selected report back to a structural transition candidate.
    ///
    /// The returned transition is still unvalidated. Callers must use the
    /// existing trusted `RepresentationTransition::validate` path immediately
    /// before actuation with current capabilities and attestations.
    pub fn selected_transition(
        &self,
        current: &RepresentationState,
        report: &BooleanRepresentationPrecisionReportV1,
    ) -> Result<Option<elastic_core::RepresentationTransition>, String> {
        let BooleanRepresentationPrecisionOutcomeV1::Selected { candidate_id, .. } =
            &report.outcome
        else {
            return Ok(None);
        };
        let candidate = self
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id() == candidate_id)
            .ok_or_else(|| "BE14e report selected an unknown candidate".to_owned())?;
        let target = self
            .declaration
            .derive_target(
                current,
                candidate.target(),
                candidate.target_schema_version,
                candidate.mechanism,
            )
            .map_err(|error| error.to_string())?;
        Ok(Some(elastic_core::RepresentationTransition {
            from: current.clone(),
            to: target,
            mechanism: candidate.mechanism,
        }))
    }
}

#[derive(Clone, Debug)]
struct RequiredPrecisionBitsPredicate {
    key: PredicateKey,
    signal: ObservationSignalId,
    candidate_bits: u16,
    max_age: Duration,
}

impl PredicateEvaluator for RequiredPrecisionBitsPredicate {
    fn key(&self) -> &PredicateKey {
        &self.key
    }

    fn evaluate(&self, input: &PredicateEvaluationInput<'_>) -> TruthValue {
        let Some(observation) = input.observations().get(self.signal.clone()) else {
            return TruthValue::Unknown;
        };
        if !observation.is_valid() || !observation.value().is_finite() {
            return TruthValue::Unknown;
        }
        let Some(age) = input.now().checked_duration_since(*observation.timestamp()) else {
            return TruthValue::Unknown;
        };
        if age > self.max_age {
            return TruthValue::Unknown;
        }
        let Some(value) = input.planning_context().get(self.signal.clone()) else {
            return TruthValue::Unknown;
        };
        if !value.is_finite() || observation.value().to_bits() != value.to_bits() {
            return TruthValue::Unknown;
        }
        if !(1.0..=MAX_EXACT_F64_INTEGER).contains(&value) || value.fract() != 0.0 {
            return TruthValue::Unknown;
        }
        if value <= f64::from(self.candidate_bits) {
            TruthValue::True
        } else {
            TruthValue::False
        }
    }
}

const fn truth_text(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
}

const fn mechanism_name(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, ContractId, Invariant, InvariantKind,
        LogicalResourceId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::{RepresentationEpoch, TransitionAttestations, TransitionError};
    use elastic_eir::PlanningContext;

    use crate::{Observation, ObservationSource};

    fn declaration() -> RepresentationalDeclaration {
        let spec = ResourceSpec::builder(
            ResourceClassId::REPRESENTATIONAL,
            LogicalResourceId::new("be14e-fixture").unwrap(),
        )
        .allow(DimensionId::REPRESENTATION)
        .observe(representation_precision_floor_signal())
        .preserve(
            Invariant::new(InvariantKind::UpholdContract(
                ContractId::new("be14e.semantic-contract").unwrap(),
            ))
            .along(DimensionId::REPRESENTATION),
        )
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .build()
        .unwrap();
        RepresentationalDeclaration::new(
            spec,
            [
                (RepresentationId::new("tensor.fp16").unwrap(), 1),
                (RepresentationId::new("tensor.int8").unwrap(), 1),
                (RepresentationId::new("tensor.int4").unwrap(), 1),
            ],
        )
        .unwrap()
    }

    fn current() -> RepresentationState {
        RepresentationState::new(
            RepresentationId::new("tensor.fp16").unwrap(),
            1,
            RepresentationEpoch::new(7),
        )
    }

    fn candidate(id: &str, rank: u32, bits: u16) -> RepresentationPrecisionCandidateV1 {
        RepresentationPrecisionCandidateV1::new(
            id,
            rank,
            RepresentationId::new(id).unwrap(),
            1,
            TransitionMechanism::Reencode,
            bits,
        )
        .unwrap()
    }

    fn capabilities() -> CapabilitySet {
        let mut capabilities = CapabilitySet::new();
        for id in ["tensor.fp16", "tensor.int8", "tensor.int4"] {
            capabilities.insert(RepresentationId::new(id).unwrap(), 1);
        }
        capabilities
    }

    fn evidence(now: Instant, required_bits: f64) -> (PlanningContext, ObservationSnapshot) {
        let signal = representation_precision_floor_signal();
        (
            PlanningContext::new().observe(signal.clone(), required_bits),
            ObservationSnapshot::new(
                now,
                vec![Observation::from_source(
                    ObservationSource::runtime("be14e-test"),
                    signal,
                    required_bits,
                    now,
                )],
            ),
        )
    }

    fn screen(
        preplanner: &BooleanRepresentationPrecisionPreplannerV1,
        context: &PlanningContext,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> BooleanRepresentationPrecisionReportV1 {
        preplanner
            .screen(
                &current(),
                &capabilities(),
                context,
                observations,
                now,
                ObservationEpoch::new(11),
                ResourceGeneration::new(3),
            )
            .unwrap()
    }

    #[test]
    fn fresh_fixed_width_floor_selects_first_true_candidate_after_false_pruning() {
        let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
            declaration(),
            vec![
                candidate("tensor.int4", 0, 4),
                candidate("tensor.int8", 10, 8),
            ],
        )
        .unwrap();
        let now = Instant::now();
        let (context, observations) = evidence(now, 8.0);
        let report = screen(&preplanner, &context, &observations, now);
        assert_eq!(report.candidates[0].truth, "false");
        assert_eq!(report.candidates[1].truth, "true");
        assert_eq!(
            report.outcome,
            BooleanRepresentationPrecisionOutcomeV1::Selected {
                candidate_id: "tensor.int8".into(),
                preference_rank: 10,
            }
        );
    }

    #[test]
    fn all_fixed_width_candidates_below_floor_are_false() {
        let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
            declaration(),
            vec![
                candidate("tensor.int4", 0, 4),
                candidate("tensor.int8", 10, 8),
            ],
        )
        .unwrap();
        let now = Instant::now();
        let (context, observations) = evidence(now, 16.0);
        let report = screen(&preplanner, &context, &observations, now);
        assert_eq!(
            report.outcome,
            BooleanRepresentationPrecisionOutcomeV1::NoCandidate
        );
        assert!(report.candidates.iter().all(|entry| entry.truth == "false"));
    }

    #[test]
    fn missing_stale_invalid_and_context_mismatch_are_unknown() {
        let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
            declaration(),
            vec![candidate("tensor.int8", 0, 8)],
        )
        .unwrap();
        let now = Instant::now();
        let signal = representation_precision_floor_signal();

        let missing = screen(
            &preplanner,
            &PlanningContext::new(),
            &ObservationSnapshot::new(now, vec![]),
            now,
        );
        assert!(matches!(
            missing.outcome,
            BooleanRepresentationPrecisionOutcomeV1::InsufficientEvidence { .. }
        ));

        let old = now.checked_sub(Duration::from_secs(2)).unwrap();
        let stale = screen(
            &preplanner,
            &PlanningContext::new().observe(signal.clone(), 8.0),
            &ObservationSnapshot::new(
                now,
                vec![Observation::from_source(
                    ObservationSource::runtime("be14e-test"),
                    signal.clone(),
                    8.0,
                    old,
                )],
            ),
            now,
        );
        assert!(matches!(
            stale.outcome,
            BooleanRepresentationPrecisionOutcomeV1::InsufficientEvidence { .. }
        ));

        let invalid = screen(
            &preplanner,
            &PlanningContext::new().observe(signal.clone(), 8.5),
            &ObservationSnapshot::new(
                now,
                vec![Observation::from_source(
                    ObservationSource::runtime("be14e-test"),
                    signal.clone(),
                    8.5,
                    now,
                )],
            ),
            now,
        );
        assert!(matches!(
            invalid.outcome,
            BooleanRepresentationPrecisionOutcomeV1::InsufficientEvidence { .. }
        ));

        let mismatch = screen(
            &preplanner,
            &PlanningContext::new().observe(signal.clone(), 8.0),
            &ObservationSnapshot::new(
                now,
                vec![Observation::from_source(
                    ObservationSource::runtime("be14e-test"),
                    signal,
                    4.0,
                    now,
                )],
            ),
            now,
        );
        assert!(matches!(
            mismatch.outcome,
            BooleanRepresentationPrecisionOutcomeV1::InsufficientEvidence { .. }
        ));
    }

    #[test]
    fn unknown_preferred_candidate_blocks_later_candidate() {
        let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
            declaration(),
            vec![
                candidate("tensor.int8", 0, 8),
                candidate("tensor.fp16", 10, 16),
            ],
        )
        .unwrap();
        let now = Instant::now();
        let report = screen(
            &preplanner,
            &PlanningContext::new(),
            &ObservationSnapshot::new(now, vec![]),
            now,
        );
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(
            report.outcome,
            BooleanRepresentationPrecisionOutcomeV1::InsufficientEvidence {
                candidate_id: "tensor.int8".into(),
                preference_rank: 0,
            }
        );
    }

    #[test]
    fn trusted_capability_absence_is_conclusive_false() {
        let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
            declaration(),
            vec![candidate("tensor.int8", 0, 8)],
        )
        .unwrap();
        let now = Instant::now();
        let (context, observations) = evidence(now, 4.0);
        let report = preplanner
            .screen(
                &current(),
                &CapabilitySet::new(),
                &context,
                &observations,
                now,
                ObservationEpoch::new(1),
                ResourceGeneration::new(1),
            )
            .unwrap();
        assert_eq!(
            report.outcome,
            BooleanRepresentationPrecisionOutcomeV1::NoCandidate
        );
        assert_eq!(report.candidates[0].truth, "false");
        assert!(!report.candidates[0].capability_supports_target);
    }

    #[test]
    fn boolean_true_never_replaces_trusted_transition_validation() {
        let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
            declaration(),
            vec![candidate("tensor.int8", 0, 8)],
        )
        .unwrap();
        let now = Instant::now();
        let (context, observations) = evidence(now, 8.0);
        let report = screen(&preplanner, &context, &observations, now);
        let transition = preplanner
            .selected_transition(&current(), &report)
            .unwrap()
            .expect("Boolean preplanning selected a structural candidate");
        assert_eq!(
            transition.validate(&capabilities(), TransitionAttestations::default()),
            Err(TransitionError::MissingReencoderAttestation)
        );
        transition
            .validate(
                &capabilities(),
                TransitionAttestations::default().attest_reencoder_available(),
            )
            .unwrap();
    }

    #[test]
    fn report_roundtrip_is_deterministic_explanatory_data() {
        let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
            declaration(),
            vec![candidate("tensor.int8", 0, 8)],
        )
        .unwrap();
        let now = Instant::now();
        let (context, observations) = evidence(now, 8.0);
        let report = screen(&preplanner, &context, &observations, now);
        let encoded = serde_json::to_string(&report).unwrap();
        let decoded: BooleanRepresentationPrecisionReportV1 =
            serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, report);
        assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
    }

    #[test]
    fn declaration_must_explicitly_name_the_numeric_source_signal() {
        let spec = ResourceSpec::builder(
            ResourceClassId::REPRESENTATIONAL,
            LogicalResourceId::new("missing-signal").unwrap(),
        )
        .allow(DimensionId::REPRESENTATION)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .build()
        .unwrap();
        let declaration = RepresentationalDeclaration::new(
            spec,
            [(RepresentationId::new("tensor.int8").unwrap(), 1)],
        )
        .unwrap();
        assert!(BooleanRepresentationPrecisionPreplannerV1::new(
            declaration,
            vec![candidate("tensor.int8", 0, 8)]
        )
        .unwrap_err()
        .contains(REPRESENTATION_PRECISION_FLOOR_SIGNAL_NAME));
    }
}
