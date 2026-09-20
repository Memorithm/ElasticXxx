//! ELANG7 composition of representation-precision admission and KV transition intent.
//!
//! This module proves structural coherence between an actually selected BE14e
//! representation/precision candidate and an existing validated-shape
//! [`KvTransitionPlan`]. It does not make either input trusted, does not perform
//! capacity admission, and does not authorize physical KV mutation.

use crate::{
    KeyEncodingPipeline, KeyTransformScope, KvCacheCompatibility, KvRecoverySource,
    KvTransitionPlan,
};
use elastic_core::{DimensionId, TargetContract, TransitionMechanism};
use elastic_eir::Fingerprint;
use elastic_runtime::{
    representation_precision_floor_predicate_key, representation_precision_floor_signal,
    BooleanRepresentationPrecisionOutcomeV1, BooleanRepresentationPrecisionReportV2, DecisionTrace,
    RepresentationPrecisionCandidateV1, REPRESENTATION_PRECISION_SOURCE_UNIT,
};
use std::fmt;

/// Versioned identity of the representation-precision/KV composition contract.
pub const REPRESENTATION_PRECISION_KV_BINDING_V1: &str =
    "elastic.kv.representation-precision-binding@1.0.0";

/// Structurally coherent representation admission plus KV transition plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationPrecisionKvBindingV1 {
    report: BooleanRepresentationPrecisionReportV2,
    candidate: RepresentationPrecisionCandidateV1,
    kv_plan: KvTransitionPlan,
    fingerprint: Fingerprint,
}

impl RepresentationPrecisionKvBindingV1 {
    /// Bind one actually-selected BE14e candidate to one KV transition plan.
    ///
    /// The source representation, derived target including epoch semantics,
    /// mechanism, selected report evidence, predicate/signal/unit metadata and
    /// strict decision trace must all agree. This remains composition evidence;
    /// trusted capabilities/attestations and the physical KV backend still own
    /// validation immediately before actuation.
    pub fn new(
        candidate: RepresentationPrecisionCandidateV1,
        report: BooleanRepresentationPrecisionReportV2,
        kv_plan: KvTransitionPlan,
    ) -> Result<Self, RepresentationPrecisionKvBindingError> {
        validate_report_schema(&report)?;
        validate_selected_outcome(&candidate, &report)?;
        validate_source(&report, &kv_plan)?;
        validate_candidate_evidence(&candidate, &report)?;
        validate_trace(&candidate, &report)?;
        validate_target(&candidate, &kv_plan)?;

        let selected_trace = report
            .candidate_traces
            .iter()
            .find(|trace| trace.candidate_id == candidate.candidate_id())
            .and_then(|trace| trace.decision_trace_json.as_deref())
            .expect("validated selected candidate trace is present");
        let fingerprint = binding_fingerprint(&candidate, &report, &kv_plan, selected_trace);
        Ok(Self {
            report,
            candidate,
            kv_plan,
            fingerprint,
        })
    }

    /// Exact BE14e durable report used by the binding.
    #[must_use]
    pub const fn report(&self) -> &BooleanRepresentationPrecisionReportV2 {
        &self.report
    }

    /// Fixed-width candidate selected by the report.
    #[must_use]
    pub const fn candidate(&self) -> &RepresentationPrecisionCandidateV1 {
        &self.candidate
    }

    /// KV transition plan structurally coherent with the selected candidate.
    #[must_use]
    pub const fn kv_plan(&self) -> &KvTransitionPlan {
        &self.kv_plan
    }

    /// Deterministic non-cryptographic structural identity of the composition.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

fn validate_report_schema(
    report: &BooleanRepresentationPrecisionReportV2,
) -> Result<(), RepresentationPrecisionKvBindingError> {
    if report.schema_version != 2 || report.planning.schema_version != 1 {
        return Err(RepresentationPrecisionKvBindingError::ReportSchema {
            outer: report.schema_version,
            planning: report.planning.schema_version,
        });
    }
    let expected_signal = representation_precision_floor_signal();
    if report.planning.source_signal != expected_signal.as_str()
        || report.planning.source_unit != REPRESENTATION_PRECISION_SOURCE_UNIT
    {
        return Err(RepresentationPrecisionKvBindingError::ReportSourceMetadata);
    }
    Ok(())
}

fn validate_selected_outcome(
    candidate: &RepresentationPrecisionCandidateV1,
    report: &BooleanRepresentationPrecisionReportV2,
) -> Result<(), RepresentationPrecisionKvBindingError> {
    match &report.planning.outcome {
        BooleanRepresentationPrecisionOutcomeV1::Selected {
            candidate_id,
            preference_rank,
        } if candidate_id == candidate.candidate_id()
            && *preference_rank == candidate.preference_rank() =>
        {
            Ok(())
        }
        BooleanRepresentationPrecisionOutcomeV1::Selected {
            candidate_id,
            preference_rank,
        } => Err(
            RepresentationPrecisionKvBindingError::SelectedCandidateMismatch {
                expected_id: candidate.candidate_id().to_owned(),
                expected_rank: candidate.preference_rank(),
                observed_id: candidate_id.clone(),
                observed_rank: *preference_rank,
            },
        ),
        _ => Err(
            RepresentationPrecisionKvBindingError::CandidateNotSelected {
                candidate_id: candidate.candidate_id().to_owned(),
            },
        ),
    }
}

fn validate_source(
    report: &BooleanRepresentationPrecisionReportV2,
    plan: &KvTransitionPlan,
) -> Result<(), RepresentationPrecisionKvBindingError> {
    let source = &plan.representation.from;
    if report.planning.current_representation != source.id.as_str()
        || report.planning.current_schema_version != source.schema_version
        || report.planning.current_epoch != source.epoch.get()
    {
        return Err(RepresentationPrecisionKvBindingError::SourceMismatch {
            report: format!(
                "{} v{} @e{}",
                report.planning.current_representation,
                report.planning.current_schema_version,
                report.planning.current_epoch
            ),
            plan: source.to_string(),
        });
    }
    Ok(())
}

fn validate_candidate_evidence(
    candidate: &RepresentationPrecisionCandidateV1,
    report: &BooleanRepresentationPrecisionReportV2,
) -> Result<(), RepresentationPrecisionKvBindingError> {
    let matching = report
        .planning
        .candidates
        .iter()
        .filter(|evidence| evidence.candidate_id == candidate.candidate_id())
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(
            RepresentationPrecisionKvBindingError::CandidateEvidenceCount {
                candidate_id: candidate.candidate_id().to_owned(),
                count: matching.len(),
            },
        );
    }
    let evidence = matching[0];
    let expected_signal = representation_precision_floor_signal();
    let expected_predicate = representation_precision_floor_predicate_key();
    if evidence.preference_rank != candidate.preference_rank()
        || evidence.target_representation != candidate.target().as_str()
        || evidence.target_schema_version != candidate.target_schema_version()
        || evidence.mechanism != mechanism_name(candidate.mechanism())
        || evidence.declared_precision_bits != candidate.declared_precision_bits()
        || evidence.predicate_key != expected_predicate.to_string()
        || evidence.source_signal != expected_signal.as_str()
        || evidence.source_unit != REPRESENTATION_PRECISION_SOURCE_UNIT
        || !evidence.declaration_supports_target
        || !evidence.capability_supports_target
        || evidence.truth != "true"
    {
        return Err(
            RepresentationPrecisionKvBindingError::CandidateEvidenceDrift {
                candidate_id: candidate.candidate_id().to_owned(),
            },
        );
    }
    Ok(())
}

fn validate_trace(
    candidate: &RepresentationPrecisionCandidateV1,
    report: &BooleanRepresentationPrecisionReportV2,
) -> Result<(), RepresentationPrecisionKvBindingError> {
    let matching = report
        .candidate_traces
        .iter()
        .filter(|trace| trace.candidate_id == candidate.candidate_id())
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(RepresentationPrecisionKvBindingError::CandidateTraceCount {
            candidate_id: candidate.candidate_id().to_owned(),
            count: matching.len(),
        });
    }
    let trace = matching[0];
    if trace.preference_rank != candidate.preference_rank() {
        return Err(RepresentationPrecisionKvBindingError::CandidateTraceDrift {
            candidate_id: candidate.candidate_id().to_owned(),
        });
    }
    let json = trace.decision_trace_json.as_deref().ok_or_else(|| {
        RepresentationPrecisionKvBindingError::MissingDecisionTrace {
            candidate_id: candidate.candidate_id().to_owned(),
        }
    })?;
    let decoded = DecisionTrace::from_bounded_json(json.as_bytes()).map_err(|error| {
        RepresentationPrecisionKvBindingError::InvalidDecisionTrace {
            candidate_id: candidate.candidate_id().to_owned(),
            detail: error.to_string(),
        }
    })?;
    let selected = decoded.selected().ok_or_else(|| {
        RepresentationPrecisionKvBindingError::TraceDidNotSelectCandidate {
            candidate_id: candidate.candidate_id().to_owned(),
        }
    })?;
    if selected.mechanism() != candidate.mechanism()
        || selected.dimension() != &DimensionId::REPRESENTATION
    {
        return Err(RepresentationPrecisionKvBindingError::CandidateTraceDrift {
            candidate_id: candidate.candidate_id().to_owned(),
        });
    }
    Ok(())
}

fn validate_target(
    candidate: &RepresentationPrecisionCandidateV1,
    plan: &KvTransitionPlan,
) -> Result<(), RepresentationPrecisionKvBindingError> {
    if plan.representation.mechanism != candidate.mechanism() {
        return Err(RepresentationPrecisionKvBindingError::MechanismMismatch {
            candidate: candidate.mechanism(),
            plan: plan.representation.mechanism,
        });
    }
    let source = &plan.representation.from;
    let contract = if source.id == *candidate.target()
        && source.schema_version == candidate.target_schema_version()
    {
        TargetContract::Same
    } else {
        TargetContract::New {
            id: candidate.target().clone(),
            schema_version: candidate.target_schema_version(),
        }
    };
    let expected = source
        .derive_target(contract, candidate.mechanism())
        .map_err(|error| {
            RepresentationPrecisionKvBindingError::TargetDerivation(error.to_string())
        })?;
    if expected != plan.representation.to {
        return Err(RepresentationPrecisionKvBindingError::TargetMismatch {
            expected: expected.to_string(),
            observed: plan.representation.to.to_string(),
        });
    }
    Ok(())
}

fn binding_fingerprint(
    candidate: &RepresentationPrecisionCandidateV1,
    report: &BooleanRepresentationPrecisionReportV2,
    plan: &KvTransitionPlan,
    selected_trace: &str,
) -> Fingerprint {
    Fingerprint::EMPTY
        .text(REPRESENTATION_PRECISION_KV_BINDING_V1)
        .text(candidate.candidate_id())
        .number(u64::from(candidate.preference_rank()))
        .number(u64::from(candidate.declared_precision_bits()))
        .text(report.planning.current_representation.as_str())
        .number(u64::from(report.planning.current_schema_version))
        .number(report.planning.current_epoch)
        .text(plan.representation.to.id.as_str())
        .number(u64::from(plan.representation.to.schema_version))
        .number(plan.representation.to.epoch.get())
        .text(mechanism_name(plan.representation.mechanism))
        .text(key_scope_name(plan.target_key_transform_scope))
        .text(pipeline_name(plan.target_key_encoding_pipeline))
        .text(recovery_name(plan.target_recovery_source))
        .text(compatibility_name(plan.compatibility))
        .text(selected_trace)
}

const fn mechanism_name(value: TransitionMechanism) -> &'static str {
    match value {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

const fn key_scope_name(value: KeyTransformScope) -> &'static str {
    match value {
        KeyTransformScope::Raw => "raw",
        KeyTransformScope::TokenStable => "token-stable",
        KeyTransformScope::QueryDependent => "query-dependent",
    }
}

const fn pipeline_name(value: KeyEncodingPipeline) -> &'static str {
    match value {
        KeyEncodingPipeline::Raw => "raw",
        KeyEncodingPipeline::TransformThenCodec => "transform-then-codec",
        KeyEncodingPipeline::CodecThenTransform => "codec-then-transform",
        KeyEncodingPipeline::FusedDeclared => "fused-declared",
    }
}

const fn recovery_name(value: KvRecoverySource) -> &'static str {
    match value {
        KvRecoverySource::None => "none",
        KvRecoverySource::StoredCanonicalRaw => "stored-canonical-raw",
        KvRecoverySource::ModelRecompute => "model-recompute",
        KvRecoverySource::ExternalStableSource => "external-stable-source",
    }
}

const fn compatibility_name(value: KvCacheCompatibility) -> &'static str {
    match value {
        KvCacheCompatibility::CacheInvariant => "cache-invariant",
        KvCacheCompatibility::EpochReencodable => "epoch-reencodable",
        KvCacheCompatibility::RecomputableOnly => "recomputable-only",
        KvCacheCompatibility::QueryDependent => "query-dependent",
    }
}

/// Fail-closed structural composition errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepresentationPrecisionKvBindingError {
    ReportSchema {
        outer: u16,
        planning: u16,
    },
    ReportSourceMetadata,
    CandidateNotSelected {
        candidate_id: String,
    },
    SelectedCandidateMismatch {
        expected_id: String,
        expected_rank: u32,
        observed_id: String,
        observed_rank: u32,
    },
    SourceMismatch {
        report: String,
        plan: String,
    },
    CandidateEvidenceCount {
        candidate_id: String,
        count: usize,
    },
    CandidateEvidenceDrift {
        candidate_id: String,
    },
    CandidateTraceCount {
        candidate_id: String,
        count: usize,
    },
    CandidateTraceDrift {
        candidate_id: String,
    },
    MissingDecisionTrace {
        candidate_id: String,
    },
    InvalidDecisionTrace {
        candidate_id: String,
        detail: String,
    },
    TraceDidNotSelectCandidate {
        candidate_id: String,
    },
    MechanismMismatch {
        candidate: TransitionMechanism,
        plan: TransitionMechanism,
    },
    TargetDerivation(String),
    TargetMismatch {
        expected: String,
        observed: String,
    },
}

impl fmt::Display for RepresentationPrecisionKvBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReportSchema { outer, planning } => write!(
                f,
                "representation/KV binding requires report v2 with planning v1, observed outer={outer} planning={planning}"
            ),
            Self::ReportSourceMetadata => f.write_str(
                "representation/KV binding report precision source signal/unit drifted",
            ),
            Self::CandidateNotSelected { candidate_id } => write!(
                f,
                "representation/KV binding candidate {candidate_id:?} was not selected by the BE14e report"
            ),
            Self::SelectedCandidateMismatch { expected_id, expected_rank, observed_id, observed_rank } => write!(
                f,
                "representation/KV selected candidate mismatch: expected {expected_id:?}@{expected_rank}, observed {observed_id:?}@{observed_rank}"
            ),
            Self::SourceMismatch { report, plan } => write!(
                f,
                "representation/KV source mismatch: report={report}, KV plan={plan}"
            ),
            Self::CandidateEvidenceCount { candidate_id, count } => write!(
                f,
                "representation/KV report contains {count} evidence entries for selected candidate {candidate_id:?}"
            ),
            Self::CandidateEvidenceDrift { candidate_id } => write!(
                f,
                "representation/KV evidence for selected candidate {candidate_id:?} drifted"
            ),
            Self::CandidateTraceCount { candidate_id, count } => write!(
                f,
                "representation/KV report contains {count} trace entries for selected candidate {candidate_id:?}"
            ),
            Self::CandidateTraceDrift { candidate_id } => write!(
                f,
                "representation/KV trace binding for selected candidate {candidate_id:?} drifted"
            ),
            Self::MissingDecisionTrace { candidate_id } => write!(
                f,
                "representation/KV selected candidate {candidate_id:?} has no DecisionTrace"
            ),
            Self::InvalidDecisionTrace { candidate_id, detail } => write!(
                f,
                "representation/KV selected candidate {candidate_id:?} has invalid DecisionTrace: {detail}"
            ),
            Self::TraceDidNotSelectCandidate { candidate_id } => write!(
                f,
                "representation/KV DecisionTrace did not select candidate {candidate_id:?}"
            ),
            Self::MechanismMismatch { candidate, plan } => write!(
                f,
                "representation/KV mechanism mismatch: candidate={candidate:?}, KV plan={plan:?}"
            ),
            Self::TargetDerivation(detail) => write!(f, "representation/KV target derivation failed: {detail}"),
            Self::TargetMismatch { expected, observed } => write!(
                f,
                "representation/KV target mismatch: expected {expected}, observed {observed}"
            ),
        }
    }
}

impl std::error::Error for RepresentationPrecisionKvBindingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KvPageDescriptor, KvPrecision, KvResidency, KvTargetMaterialization};
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, LogicalResourceId,
        RepresentationalDeclaration, ResourceClassId, ResourceSpec,
    };
    use elastic_core::{
        CapabilitySet, ObservationEpoch, RepresentationEpoch, RepresentationId,
        RepresentationState, ResourceGeneration, TransitionAttestations,
    };
    use elastic_eir::PlanningContext;
    use elastic_runtime::{
        representation_precision_floor_signal, BooleanRepresentationPrecisionPreplannerV1,
        Observation, ObservationSnapshot, ObservationSource,
    };
    use std::time::Instant;

    fn fixture() -> (
        RepresentationPrecisionCandidateV1,
        BooleanRepresentationPrecisionReportV2,
        KvTransitionPlan,
    ) {
        let spec = ResourceSpec::builder(
            ResourceClassId::REPRESENTATIONAL,
            LogicalResourceId::new("kv-composition").unwrap(),
        )
        .allow(DimensionId::REPRESENTATION)
        .observe(representation_precision_floor_signal())
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
        let declaration = RepresentationalDeclaration::new(
            spec,
            [
                (RepresentationId::new("tensor.fp16").unwrap(), 1),
                (RepresentationId::new("tensor.int8").unwrap(), 1),
            ],
        )
        .unwrap();
        let candidate = RepresentationPrecisionCandidateV1::new(
            "int8",
            0,
            RepresentationId::new("tensor.int8").unwrap(),
            1,
            TransitionMechanism::Reencode,
            8,
        )
        .unwrap();
        let preplanner =
            BooleanRepresentationPrecisionPreplannerV1::new(declaration, vec![candidate.clone()])
                .unwrap();
        let current = RepresentationState::new(
            RepresentationId::new("tensor.fp16").unwrap(),
            1,
            RepresentationEpoch::new(4),
        );
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(RepresentationId::new("tensor.int8").unwrap(), 1);
        let now = Instant::now();
        let signal = representation_precision_floor_signal();
        let context = PlanningContext::new().observe(signal.clone(), 8.0);
        let observations = ObservationSnapshot::new(
            now,
            vec![Observation::from_source(
                ObservationSource::runtime("elang7d-test"),
                signal,
                8.0,
                now,
            )],
        );
        let report = preplanner
            .screen_with_trace(
                &current,
                &capabilities,
                &context,
                &observations,
                now,
                ObservationEpoch::new(1),
                ResourceGeneration::new(1),
            )
            .unwrap();
        let target = current
            .derive_target(
                TargetContract::New {
                    id: RepresentationId::new("tensor.int8").unwrap(),
                    schema_version: 1,
                },
                TransitionMechanism::Reencode,
            )
            .unwrap();
        let page = KvPageDescriptor {
            page: crate::KvPageId::new(1),
            representation: current,
            precision: KvPrecision::F16,
            residency: KvResidency::Accelerator,
            key_transform_scope: KeyTransformScope::Raw,
            key_encoding_pipeline: KeyEncodingPipeline::Raw,
            recovery_source: KvRecoverySource::StoredCanonicalRaw,
        };
        let plan = page
            .validate_reusable_representation_change(
                target,
                TransitionMechanism::Reencode,
                &capabilities,
                TransitionAttestations::none().attest_reencoder_available(),
                KvTargetMaterialization::new(
                    KeyTransformScope::Raw,
                    KeyEncodingPipeline::Raw,
                    KvRecoverySource::StoredCanonicalRaw,
                ),
            )
            .unwrap();
        (candidate, report, plan)
    }

    #[test]
    fn actual_selected_be14e_report_binds_exact_kv_transition() {
        let (candidate, report, plan) = fixture();
        let binding = RepresentationPrecisionKvBindingV1::new(candidate, report, plan).unwrap();
        assert_eq!(
            binding.kv_plan().representation.to.id.as_str(),
            "tensor.int8"
        );
        assert_eq!(binding.kv_plan().representation.to.epoch.get(), 5);
    }

    #[test]
    fn target_epoch_or_contract_drift_fails_closed() {
        let (candidate, report, mut plan) = fixture();
        plan.representation.to.epoch = RepresentationEpoch::new(99);
        assert!(matches!(
            RepresentationPrecisionKvBindingV1::new(candidate, report, plan),
            Err(RepresentationPrecisionKvBindingError::TargetMismatch { .. })
        ));
    }

    #[test]
    fn unselected_or_trace_less_candidate_fails_closed() {
        let (candidate, mut report, plan) = fixture();
        report.planning.outcome = BooleanRepresentationPrecisionOutcomeV1::NoCandidate;
        assert!(matches!(
            RepresentationPrecisionKvBindingV1::new(candidate.clone(), report, plan.clone()),
            Err(RepresentationPrecisionKvBindingError::CandidateNotSelected { .. })
        ));

        let (_, mut report, _) = fixture();
        report.candidate_traces[0].decision_trace_json = None;
        assert!(matches!(
            RepresentationPrecisionKvBindingV1::new(candidate, report, plan),
            Err(RepresentationPrecisionKvBindingError::MissingDecisionTrace { .. })
        ));
    }
}
