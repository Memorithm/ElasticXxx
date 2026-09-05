//! Durable evidence for one completed adaptive model-execution cycle.
//!
//! This module records what the runtime actually observed, planned, validated,
//! actuated, verified, committed, or rolled back. The artifact is bound to the
//! exact model-execution controller contracts and can be revalidated offline.
//! Offline validation is read-only evidence inspection; it never authorizes a
//! new physical transition.

use std::time::Duration;

use elastic_adapters::{
    model_execution_current_profile_rank_signal, model_execution_profile_dimension,
    ModelExecutionProfileSetV1, ModelExecutionProfileV1,
};
use elastic_core::TransitionMechanism;
use elastic_eir::PlanOutcome;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    EvidenceCommand, EvidenceEnvelope, EvidenceEvent, EvidenceEventKind, ForecastCycleResult,
    ForecastStatus, ModelExecutionControllerContractsV1, ObservationSnapshot, RuntimeError,
    RuntimeEventKind, VerificationResult,
};

/// Versioned durable evidence kind for one model-execution control cycle.
pub const MODEL_EXECUTION_CYCLE_EVIDENCE_V1: &str =
    "elastic.model-execution.cycle-evidence@1.0.0";
/// JSON media type for [`MODEL_EXECUTION_CYCLE_EVIDENCE_V1`].
pub const MODEL_EXECUTION_CYCLE_EVIDENCE_MEDIA_TYPE_V1: &str =
    "application/vnd.elastic.model-execution-cycle-evidence.v1+json";

/// One finite planner-facing signal captured in deterministic context order.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionSignalEvidenceV1 {
    pub signal: String,
    pub value: f64,
}

/// One runtime observation represented relative to its containing snapshot.
///
/// Monotonic `Instant` values are process-local and therefore not serialized.
/// `age_nanos` preserves how old the observation was when the runtime completed
/// that observation snapshot.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionObservationEvidenceV1 {
    pub source: String,
    pub signal: String,
    pub value: Option<f64>,
    pub valid: bool,
    pub unsupported_reason: Option<String>,
    pub age_nanos: u64,
}

/// One ordered observation snapshot from the trusted runtime cycle.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionObservationSnapshotEvidenceV1 {
    pub all_signals_valid: bool,
    pub observations: Vec<ModelExecutionObservationEvidenceV1>,
}

/// Forecast status persisted without serializing runtime-internal enum layout.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ModelExecutionForecastStatusEvidenceV1 {
    Available,
    Unsupported,
    Inconclusive,
}

/// Auditable forecast metadata used before planning.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionForecastEvidenceV1 {
    pub status: ModelExecutionForecastStatusEvidenceV1,
    pub method: String,
    pub horizon_nanos: u64,
    pub confidence: Option<f64>,
    pub detail: Option<String>,
}

/// Complete correlated profile identity persisted with a selected candidate.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionSelectedProfileEvidenceV1 {
    pub profile_id: String,
    pub preference_rank: u32,
    pub active_experts: u32,
    pub expert_width_bps: u16,
    pub activation_budget_bps: u16,
}

/// Honest planner outcome persisted for offline inspection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ModelExecutionPlanOutcomeEvidenceV1 {
    Candidate {
        mechanism: String,
        dimension: String,
        profile: ModelExecutionSelectedProfileEvidenceV1,
    },
    InsufficientEvidence { detail: String },
    Unsupported,
    NoCandidate,
}

/// One trusted invariant check emitted during runtime validation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionInvariantEvidenceV1 {
    pub invariant: String,
    pub holds: bool,
    pub detail: Option<String>,
}

/// Planner and trusted-validation evidence for the cycle.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionPlanEvidenceV1 {
    pub context: Vec<ModelExecutionSignalEvidenceV1>,
    pub outcome: ModelExecutionPlanOutcomeEvidenceV1,
    pub reasoning: String,
    pub validated: bool,
    pub invariant_checks: Vec<ModelExecutionInvariantEvidenceV1>,
}

/// Prepared physical actuation identity, when the cycle reached that stage.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionActuationEvidenceV1 {
    pub adapter_name: String,
    pub target_profile_rank: u32,
}

/// Post-action verification persisted as an explicit status.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ModelExecutionVerificationEvidenceV1 {
    Pass,
    Fail { detail: String },
    Inconclusive { detail: String },
}

/// Rollback evidence when a prepared transaction did not commit.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionRollbackEvidenceV1 {
    pub rationale: String,
    pub invariants_restored: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ModelExecutionCycleEvidenceWireV1 {
    evidence_kind: String,
    resource_id: String,
    provider_id: String,
    model_revision: String,
    capability_fingerprint: String,
    profile_set_fingerprint: String,
    policy_fingerprint: String,
    forecast: Option<ModelExecutionForecastEvidenceV1>,
    observation_snapshots: Vec<ModelExecutionObservationSnapshotEvidenceV1>,
    initial_profile_rank: Option<u32>,
    plan: Option<ModelExecutionPlanEvidenceV1>,
    actuation: Option<ModelExecutionActuationEvidenceV1>,
    verification: Option<ModelExecutionVerificationEvidenceV1>,
    committed: bool,
    commit_rationale: Option<String>,
    rolled_back: bool,
    rollback: Option<ModelExecutionRollbackEvidenceV1>,
    final_profile_rank: u32,
    events: Vec<EvidenceEvent>,
}

/// Fully validated durable evidence for one completed model-execution cycle.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelExecutionCycleEvidenceV1 {
    wire: ModelExecutionCycleEvidenceWireV1,
}

impl ModelExecutionCycleEvidenceV1 {
    /// Capture one completed forecast-aware model-execution cycle.
    ///
    /// # Errors
    ///
    /// Fails closed when runtime data cannot be represented durably, the cycle
    /// contains a candidate outside the exact correlated profile set, the final
    /// physical rank is unpublished, or the resulting evidence is internally
    /// inconsistent with the supplied controller contracts.
    pub fn capture(
        contracts: &ModelExecutionControllerContractsV1,
        resource_id: impl Into<String>,
        result: &ForecastCycleResult,
        final_profile_rank: u32,
    ) -> Result<Self, RuntimeError> {
        let profiles = contracts.profiles();
        require_profile_rank(profiles, final_profile_rank)?;

        let observation_snapshots = result
            .transaction
            .observations
            .iter()
            .map(capture_snapshot)
            .collect::<Result<Vec<_>, _>>()?;
        let initial_profile_rank = initial_profile_rank(&result.transaction.observations)?;
        if let Some(rank) = initial_profile_rank {
            require_profile_rank(profiles, rank)?;
        }

        let forecast = result
            .forecast
            .as_ref()
            .map(capture_forecast)
            .transpose()?;
        let plan = result
            .transaction
            .plan
            .as_ref()
            .map(|plan| capture_plan(plan, profiles))
            .transpose()?;
        let actuation = result
            .transaction
            .actuation
            .as_ref()
            .map(|actuation| {
                let raw = actuation.target.ok_or_else(|| {
                    RuntimeError::validation(
                        "model cycle evidence cannot persist actuation without target rank",
                    )
                })?;
                let target_profile_rank = u32::try_from(raw).map_err(|_| {
                    RuntimeError::validation(
                        "model cycle evidence actuation target rank does not fit u32",
                    )
                })?;
                require_profile_rank(profiles, target_profile_rank)?;
                Ok(ModelExecutionActuationEvidenceV1 {
                    adapter_name: actuation.adapter_name.clone(),
                    target_profile_rank,
                })
            })
            .transpose()?;
        let verification = result
            .transaction
            .verification
            .as_ref()
            .map(capture_verification);
        let committed = result.transaction.commit.is_some();
        let commit_rationale = result
            .transaction
            .commit
            .as_ref()
            .map(|commit| commit.rationale.clone());
        let rolled_back = result.transaction.rollback.is_some();
        let rollback = result.transaction.rollback.as_ref().map(|rollback| {
            ModelExecutionRollbackEvidenceV1 {
                rationale: rollback.rationale.clone(),
                invariants_restored: rollback.invariants_restored,
            }
        });
        let events = result
            .events()
            .map(|event| EvidenceEvent {
                kind: evidence_event_kind(&event.kind),
                details: event.details.clone(),
            })
            .collect();

        let wire = ModelExecutionCycleEvidenceWireV1 {
            evidence_kind: MODEL_EXECUTION_CYCLE_EVIDENCE_V1.to_owned(),
            resource_id: resource_id.into(),
            provider_id: profiles.provider_id().to_owned(),
            model_revision: profiles.model_revision().to_owned(),
            capability_fingerprint: profiles.capability_fingerprint().to_string(),
            profile_set_fingerprint: profiles.fingerprint().to_string(),
            policy_fingerprint: contracts.policy().fingerprint().to_string(),
            forecast,
            observation_snapshots,
            initial_profile_rank,
            plan,
            actuation,
            verification,
            committed,
            commit_rationale,
            rolled_back,
            rollback,
            final_profile_rank,
            events,
        };
        validate_wire(&wire, contracts)?;
        Ok(Self { wire })
    }

    /// Parse bounded generic runtime evidence and revalidate it against the exact
    /// current controller contracts. This method performs no actuation.
    pub fn from_runtime_evidence(
        evidence: &EvidenceEnvelope,
        contracts: &ModelExecutionControllerContractsV1,
    ) -> Result<Self, RuntimeError> {
        if evidence.command != EvidenceCommand::Run {
            return Err(RuntimeError::validation(format!(
                "model cycle evidence requires command=run, got {:?}",
                evidence.command
            )));
        }
        let mut value = evidence
            .to_value()
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        let object = value.as_object_mut().ok_or_else(|| {
            RuntimeError::validation("model cycle runtime evidence root is not an object")
        })?;
        object.remove("evidence_schema");
        object.remove("command");
        let wire: ModelExecutionCycleEvidenceWireV1 = serde_json::from_value(value)
            .map_err(|error| RuntimeError::validation(format!("invalid model cycle evidence: {error}")))?;
        validate_wire(&wire, contracts)?;
        Ok(Self { wire })
    }

    /// Parse a bounded JSON runtime-evidence document and revalidate its model
    /// cycle payload against the exact current controller contracts.
    pub fn from_json(
        json: &[u8],
        contracts: &ModelExecutionControllerContractsV1,
    ) -> Result<Self, RuntimeError> {
        let evidence = EvidenceEnvelope::from_slice(json)
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        Self::from_runtime_evidence(&evidence, contracts)
    }

    /// Convert this model-specific artifact to the shared bounded
    /// `elastic-runtime-evidence-v1` envelope.
    pub fn to_runtime_evidence(&self) -> Result<EvidenceEnvelope, RuntimeError> {
        let mut value = serde_json::to_value(&self.wire)
            .map_err(|error| RuntimeError::validation(error.to_string()))?;
        let object = value.as_object_mut().ok_or_else(|| {
            RuntimeError::validation("model cycle evidence did not serialize as an object")
        })?;
        object.insert("command".to_owned(), Value::String("run".to_owned()));
        EvidenceEnvelope::capture(value)
            .map_err(|error| RuntimeError::validation(error.to_string()))
    }

    /// Serialize this artifact through the shared bounded runtime evidence
    /// contract.
    pub fn to_pretty_json(&self) -> Result<String, RuntimeError> {
        self.to_runtime_evidence()?
            .to_pretty_json()
            .map_err(|error| RuntimeError::validation(error.to_string()))
    }

    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.wire.resource_id
    }

    #[must_use]
    pub const fn initial_profile_rank(&self) -> Option<u32> {
        self.wire.initial_profile_rank
    }

    #[must_use]
    pub const fn final_profile_rank(&self) -> u32 {
        self.wire.final_profile_rank
    }

    #[must_use]
    pub const fn committed(&self) -> bool {
        self.wire.committed
    }

    #[must_use]
    pub const fn rolled_back(&self) -> bool {
        self.wire.rolled_back
    }

    #[must_use]
    pub fn plan(&self) -> Option<&ModelExecutionPlanEvidenceV1> {
        self.wire.plan.as_ref()
    }

    #[must_use]
    pub fn observations(&self) -> &[ModelExecutionObservationSnapshotEvidenceV1] {
        &self.wire.observation_snapshots
    }
}

fn capture_snapshot(
    snapshot: &ObservationSnapshot,
) -> Result<ModelExecutionObservationSnapshotEvidenceV1, RuntimeError> {
    let observations = snapshot
        .observations
        .iter()
        .map(|observation| {
            let age = snapshot
                .timestamp
                .checked_duration_since(*observation.timestamp())
                .ok_or_else(|| {
                    RuntimeError::observation(format!(
                        "observation {} from {} is timestamped after its runtime snapshot",
                        observation.signal(),
                        observation.source()
                    ))
                })?;
            let age_nanos = duration_nanos(age, "observation age")?;
            let value = if observation.is_valid() {
                if !observation.value().is_finite() {
                    return Err(RuntimeError::observation(format!(
                        "valid observation {} from {} is non-finite",
                        observation.signal(),
                        observation.source()
                    )));
                }
                Some(observation.value())
            } else {
                None
            };
            Ok(ModelExecutionObservationEvidenceV1 {
                source: observation.source().to_string(),
                signal: observation.signal().to_string(),
                value,
                valid: observation.is_valid(),
                unsupported_reason: observation.unsupported_reason().map(ToOwned::to_owned),
                age_nanos,
            })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    Ok(ModelExecutionObservationSnapshotEvidenceV1 {
        all_signals_valid: snapshot.all_signals_valid,
        observations,
    })
}

fn capture_forecast(
    forecast: &crate::Forecast,
) -> Result<ModelExecutionForecastEvidenceV1, RuntimeError> {
    if forecast.confidence.is_some_and(|value| !value.is_finite()) {
        return Err(RuntimeError::validation(
            "model cycle forecast confidence must be finite when present",
        ));
    }
    Ok(ModelExecutionForecastEvidenceV1 {
        status: match forecast.status {
            ForecastStatus::Available => ModelExecutionForecastStatusEvidenceV1::Available,
            ForecastStatus::Unsupported => ModelExecutionForecastStatusEvidenceV1::Unsupported,
            ForecastStatus::Inconclusive => ModelExecutionForecastStatusEvidenceV1::Inconclusive,
        },
        method: forecast.method.clone(),
        horizon_nanos: duration_nanos(forecast.horizon, "forecast horizon")?,
        confidence: forecast.confidence,
        detail: forecast.detail.clone(),
    })
}

fn capture_plan(
    validated: &crate::ValidatedPlan,
    profiles: &ModelExecutionProfileSetV1,
) -> Result<ModelExecutionPlanEvidenceV1, RuntimeError> {
    let context = validated
        .plan
        .context
        .iter()
        .map(|(signal, value)| {
            if !value.is_finite() {
                return Err(RuntimeError::validation(format!(
                    "model cycle plan context signal {signal} is non-finite"
                )));
            }
            Ok(ModelExecutionSignalEvidenceV1 {
                signal: signal.to_string(),
                value,
            })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

    let outcome = match &validated.plan.outcome {
        PlanOutcome::Candidate(candidate) => {
            if candidate.dimension() != &model_execution_profile_dimension() {
                return Err(RuntimeError::validation(format!(
                    "model cycle evidence candidate dimension {} is not {}",
                    candidate.dimension(),
                    model_execution_profile_dimension()
                )));
            }
            let raw = candidate.magnitude().ok_or_else(|| {
                RuntimeError::validation("model cycle candidate has no target profile rank")
            })?;
            let rank = u32::try_from(raw).map_err(|_| {
                RuntimeError::validation("model cycle candidate target rank does not fit u32")
            })?;
            let profile = require_profile_rank(profiles, rank)?;
            ModelExecutionPlanOutcomeEvidenceV1::Candidate {
                mechanism: mechanism_name(candidate.mechanism()).to_owned(),
                dimension: candidate.dimension().to_string(),
                profile: profile_evidence(profile),
            }
        }
        PlanOutcome::InsufficientEvidence { detail } => {
            ModelExecutionPlanOutcomeEvidenceV1::InsufficientEvidence {
                detail: detail.clone(),
            }
        }
        PlanOutcome::Unsupported => ModelExecutionPlanOutcomeEvidenceV1::Unsupported,
        PlanOutcome::NoCandidate => ModelExecutionPlanOutcomeEvidenceV1::NoCandidate,
    };

    let invariant_checks = validated
        .invariant_checks
        .iter()
        .map(|check| ModelExecutionInvariantEvidenceV1 {
            invariant: check.invariant.to_string(),
            holds: check.holds,
            detail: check.detail.clone(),
        })
        .collect();

    Ok(ModelExecutionPlanEvidenceV1 {
        context,
        outcome,
        reasoning: validated.plan.reasoning.clone(),
        validated: validated.validated,
        invariant_checks,
    })
}

fn capture_verification(result: &VerificationResult) -> ModelExecutionVerificationEvidenceV1 {
    match result {
        VerificationResult::Pass => ModelExecutionVerificationEvidenceV1::Pass,
        VerificationResult::Fail { detail } => ModelExecutionVerificationEvidenceV1::Fail {
            detail: detail.clone(),
        },
        VerificationResult::Inconclusive { detail } => {
            ModelExecutionVerificationEvidenceV1::Inconclusive {
                detail: detail.clone(),
            }
        }
    }
}

fn initial_profile_rank(snapshots: &[ObservationSnapshot]) -> Result<Option<u32>, RuntimeError> {
    let signal = model_execution_current_profile_rank_signal();
    for snapshot in snapshots {
        for observation in &snapshot.observations {
            if observation.signal() == &signal && observation.is_valid() {
                let value = observation.value();
                if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > u32::MAX as f64
                {
                    return Err(RuntimeError::observation(format!(
                        "model current-profile observation is not an exact u32 rank: {value}"
                    )));
                }
                return Ok(Some(value as u32));
            }
        }
    }
    Ok(None)
}

fn profile_evidence(profile: &ModelExecutionProfileV1) -> ModelExecutionSelectedProfileEvidenceV1 {
    ModelExecutionSelectedProfileEvidenceV1 {
        profile_id: profile.profile_id().to_owned(),
        preference_rank: profile.preference_rank(),
        active_experts: profile.active_experts(),
        expert_width_bps: profile.expert_width_bps(),
        activation_budget_bps: profile.activation_budget_bps(),
    }
}

fn require_profile_rank(
    profiles: &ModelExecutionProfileSetV1,
    rank: u32,
) -> Result<&ModelExecutionProfileV1, RuntimeError> {
    profiles
        .profiles()
        .iter()
        .find(|profile| profile.preference_rank() == rank)
        .ok_or_else(|| {
            RuntimeError::validation(format!(
                "model cycle evidence references unpublished profile rank {rank}"
            ))
        })
}

fn duration_nanos(duration: Duration, name: &str) -> Result<u64, RuntimeError> {
    u64::try_from(duration.as_nanos()).map_err(|_| {
        RuntimeError::validation(format!("{name} exceeds durable u64 nanosecond range"))
    })
}

fn mechanism_name(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

fn evidence_event_kind(kind: &RuntimeEventKind) -> EvidenceEventKind {
    match kind {
        RuntimeEventKind::ObservationCollected => EvidenceEventKind::ObservationCollected,
        RuntimeEventKind::ForecastGenerated => EvidenceEventKind::ForecastGenerated,
        RuntimeEventKind::PlanSelected => EvidenceEventKind::PlanSelected,
        RuntimeEventKind::PlanRejected => EvidenceEventKind::PlanRejected,
        RuntimeEventKind::InvariantChecked => EvidenceEventKind::InvariantChecked,
        RuntimeEventKind::PlanValidated => EvidenceEventKind::PlanValidated,
        RuntimeEventKind::ActuationPrepared => EvidenceEventKind::ActuationPrepared,
        RuntimeEventKind::ActuationApplied => EvidenceEventKind::ActuationApplied,
        RuntimeEventKind::VerificationPerformed => EvidenceEventKind::VerificationPerformed,
        RuntimeEventKind::CommitExecuted => EvidenceEventKind::CommitExecuted,
        RuntimeEventKind::RollbackExecuted => EvidenceEventKind::RollbackExecuted,
        RuntimeEventKind::CycleStarted => EvidenceEventKind::CycleStarted,
        RuntimeEventKind::CycleCompleted => EvidenceEventKind::CycleCompleted,
        RuntimeEventKind::ControlLoopStarted => EvidenceEventKind::ControlLoopStarted,
        RuntimeEventKind::ControlLoopStopped => EvidenceEventKind::ControlLoopStopped,
        RuntimeEventKind::CancellationObserved => EvidenceEventKind::CancellationObserved,
        RuntimeEventKind::ErrorEncountered => EvidenceEventKind::ErrorEncountered,
    }
}

fn validate_wire(
    wire: &ModelExecutionCycleEvidenceWireV1,
    contracts: &ModelExecutionControllerContractsV1,
) -> Result<(), RuntimeError> {
    if wire.evidence_kind != MODEL_EXECUTION_CYCLE_EVIDENCE_V1 {
        return Err(RuntimeError::validation(format!(
            "unsupported model cycle evidence kind {:?}",
            wire.evidence_kind
        )));
    }
    let profiles = contracts.profiles();
    if wire.provider_id != profiles.provider_id() {
        return Err(RuntimeError::validation("model cycle provider identity mismatch"));
    }
    if wire.model_revision != profiles.model_revision() {
        return Err(RuntimeError::validation("model cycle revision identity mismatch"));
    }
    if wire.capability_fingerprint != profiles.capability_fingerprint().to_string() {
        return Err(RuntimeError::validation(
            "model cycle capability fingerprint mismatch",
        ));
    }
    if wire.profile_set_fingerprint != profiles.fingerprint().to_string() {
        return Err(RuntimeError::validation(
            "model cycle profile-set fingerprint mismatch",
        ));
    }
    if wire.policy_fingerprint != contracts.policy().fingerprint().to_string() {
        return Err(RuntimeError::validation(
            "model cycle policy fingerprint mismatch",
        ));
    }
    require_profile_rank(profiles, wire.final_profile_rank)?;
    if let Some(rank) = wire.initial_profile_rank {
        require_profile_rank(profiles, rank)?;
    }
    if wire.committed && wire.rolled_back {
        return Err(RuntimeError::validation(
            "model cycle cannot be both committed and rolled back",
        ));
    }
    if wire.committed != wire.commit_rationale.is_some() {
        return Err(RuntimeError::validation(
            "model cycle commit flag and rationale are inconsistent",
        ));
    }
    if wire.rolled_back != wire.rollback.is_some() {
        return Err(RuntimeError::validation(
            "model cycle rollback flag and payload are inconsistent",
        ));
    }
    if wire
        .rollback
        .as_ref()
        .is_some_and(|rollback| !rollback.invariants_restored)
    {
        return Err(RuntimeError::validation(
            "durable completed model cycle cannot claim an unrestored rollback",
        ));
    }

    for snapshot in &wire.observation_snapshots {
        for observation in &snapshot.observations {
            match (observation.valid, observation.value) {
                (true, Some(value)) if value.is_finite() => {}
                (true, _) => {
                    return Err(RuntimeError::validation(
                        "valid model cycle observation requires one finite value",
                    ));
                }
                (false, None) => {}
                (false, Some(_)) => {
                    return Err(RuntimeError::validation(
                        "unsupported model cycle observation must not persist a numeric value",
                    ));
                }
            }
        }
    }

    let candidate = wire.plan.as_ref().and_then(|plan| match &plan.outcome {
        ModelExecutionPlanOutcomeEvidenceV1::Candidate { profile, .. } => Some(profile),
        _ => None,
    });
    if let Some(candidate) = candidate {
        let profile = require_profile_rank(profiles, candidate.preference_rank)?;
        if candidate != &profile_evidence(profile) {
            return Err(RuntimeError::validation(
                "model cycle selected profile tuple does not match current qualified profile set",
            ));
        }
    }
    if let Some(actuation) = &wire.actuation {
        require_profile_rank(profiles, actuation.target_profile_rank)?;
        if candidate.is_none_or(|profile| profile.preference_rank != actuation.target_profile_rank) {
            return Err(RuntimeError::validation(
                "model cycle actuation target does not match selected profile",
            ));
        }
    }
    if wire.committed {
        let actuation = wire.actuation.as_ref().ok_or_else(|| {
            RuntimeError::validation("committed model cycle requires actuation evidence")
        })?;
        if wire.verification != Some(ModelExecutionVerificationEvidenceV1::Pass) {
            return Err(RuntimeError::validation(
                "committed model cycle requires passing verification evidence",
            ));
        }
        if wire.final_profile_rank != actuation.target_profile_rank {
            return Err(RuntimeError::validation(
                "committed model cycle final rank does not match actuation target",
            ));
        }
    }
    if wire.rolled_back
        && wire
            .initial_profile_rank
            .is_some_and(|rank| rank != wire.final_profile_rank)
    {
        return Err(RuntimeError::validation(
            "rolled-back model cycle final rank does not match observed initial rank",
        ));
    }
    if wire
        .plan
        .as_ref()
        .into_iter()
        .flat_map(|plan| plan.context.iter())
        .any(|entry| !entry.value.is_finite())
    {
        return Err(RuntimeError::validation(
            "model cycle plan context contains a non-finite value",
        ));
    }
    if wire
        .forecast
        .as_ref()
        .and_then(|forecast| forecast.confidence)
        .is_some_and(|value| !value.is_finite())
    {
        return Err(RuntimeError::validation(
            "model cycle forecast confidence is non-finite",
        ));
    }

    // Reuse the generic evidence contract as a second structural/transaction
    // consistency gate, including CommitExecuted/RollbackExecuted event checks.
    let native = ModelExecutionCycleEvidenceV1 { wire: wire.clone() };
    native.to_runtime_evidence()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_adapters::{
        ModelExecutionCapabilitiesV1, ModelExecutionEnvelopePolicyV1,
        ModelExecutionEnvelopeRuleV1, ModelExecutionProfileEnvelopeV1,
    };

    fn contracts() -> ModelExecutionControllerContractsV1 {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let profiles = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
            ],
        )
        .unwrap();
        let policy = ModelExecutionEnvelopePolicyV1::new(
            &profiles,
            "bytes",
            vec![
                ModelExecutionEnvelopeRuleV1::new(
                    "full",
                    0,
                    8_000,
                    7_000,
                    ModelExecutionProfileEnvelopeV1::new(4, 10_000, 10_000).unwrap(),
                )
                .unwrap(),
                ModelExecutionEnvelopeRuleV1::new(
                    "balanced",
                    10,
                    0,
                    10_000,
                    ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap();
        ModelExecutionControllerContractsV1::new(profiles, policy).unwrap()
    }

    #[test]
    fn strict_replay_rejects_foreign_contract_identity() {
        let left = contracts();
        let capabilities = ModelExecutionCapabilitiesV1::new(
            "foreign-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let foreign_profiles = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
            ],
        )
        .unwrap();
        let foreign_policy = ModelExecutionEnvelopePolicyV1::new(
            &foreign_profiles,
            "bytes",
            vec![ModelExecutionEnvelopeRuleV1::new(
                "balanced",
                0,
                0,
                10_000,
                ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
            )
            .unwrap()],
        )
        .unwrap();
        let right =
            ModelExecutionControllerContractsV1::new(foreign_profiles, foreign_policy).unwrap();

        let wire = ModelExecutionCycleEvidenceWireV1 {
            evidence_kind: MODEL_EXECUTION_CYCLE_EVIDENCE_V1.to_owned(),
            resource_id: "model-runtime".to_owned(),
            provider_id: left.profiles().provider_id().to_owned(),
            model_revision: left.profiles().model_revision().to_owned(),
            capability_fingerprint: left.profiles().capability_fingerprint().to_string(),
            profile_set_fingerprint: left.profiles().fingerprint().to_string(),
            policy_fingerprint: left.policy().fingerprint().to_string(),
            forecast: None,
            observation_snapshots: Vec::new(),
            initial_profile_rank: Some(0),
            plan: None,
            actuation: None,
            verification: None,
            committed: false,
            commit_rationale: None,
            rolled_back: false,
            rollback: None,
            final_profile_rank: 0,
            events: Vec::new(),
        };

        assert!(validate_wire(&wire, &left).is_ok());
        assert!(validate_wire(&wire, &right).is_err());
    }
}
