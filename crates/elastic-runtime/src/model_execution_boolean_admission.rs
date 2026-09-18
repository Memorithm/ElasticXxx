//! BE14c Boolean eligibility for correlated model-execution profiles.
//!
//! The stable predicate in this module answers one question only: does the
//! current fresh resource envelope satisfy at least one published policy rule?
//! It never chooses a rule or profile. On `True`, the existing adaptive planner
//! still selects among correlated profiles and `TransactionalModelExecution`
//! remains authoritative for validation, actuation, verification, and rollback.

use std::time::Instant;

use elastic_adapters::{
    model_execution_current_profile_rank_signal, model_execution_profile_dimension,
    ModelExecutionAdaptivePlannerV1, ModelExecutionEnvelopePolicyV1, ModelExecutionProfileSetV1,
};
use elastic_core::resource::ObservationSignalId;
use elastic_core::{
    BoolExpr, BooleanGuard, FreshnessSnapshot, GuardFactSource, GuardScope, GuardedResourceSpec,
    ObservationEpoch, PlannerEpoch, PredicateKey, PredicateRegistry, ResourceGeneration,
    TransitionMechanism, TruthValue,
};
use elastic_eir::{lower_guarded, EirGuardedResource, PlanningContext};
use serde::{Deserialize, Serialize};

use crate::{
    capture_decision_trace, BooleanGuardPreplanner, CadenceConfig, CurrentStateForecaster,
    ExecutionModeConfig, FactResourceBinding, FactSnapshot, FactSourceId, Forecaster,
    ModelExecutionControllerV1, ModelExecutionProfileBackendV1, ModelExecutionResourceTelemetryV1,
    Observation, ObservationSnapshot, Observer, PredicateEvaluationInput, PredicateEvaluator,
    RuntimeError,
};

/// Stable namespace of the BE14c resource-envelope predicate.
pub const MODEL_EXECUTION_ENVELOPE_PREDICATE_NAMESPACE: &str = "elastic.model-execution";
/// Stable local name of the BE14c resource-envelope predicate.
pub const MODEL_EXECUTION_ENVELOPE_PREDICATE_NAME: &str = "resource-envelope-available";
/// Unit of the generic utilization signal.
pub const MODEL_EXECUTION_UTILIZATION_SOURCE_UNIT: &str = "fraction";

/// Stable key used by the BE14c model-execution guard.
pub fn model_execution_envelope_predicate_key() -> PredicateKey {
    PredicateKey::new(
        MODEL_EXECUTION_ENVELOPE_PREDICATE_NAMESPACE,
        MODEL_EXECUTION_ENVELOPE_PREDICATE_NAME,
    )
    .expect("static BE14c PredicateKey is valid")
}

/// Durable explanatory Boolean evidence for one profile cycle.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanModelExecutionProfileEvidenceV1 {
    pub schema_version: u16,
    pub predicate_key: String,
    pub free_capacity_signal: String,
    pub free_capacity_unit: String,
    pub utilization_signal: String,
    pub utilization_unit: String,
    pub truth: String,
    pub forecast_method: String,
    pub forecast_horizon_milliseconds: u64,
    pub forecast_confidence_claimed: bool,
    pub decision_trace_json: String,
}

/// One BE14c gated profile result.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanModelExecutionProfileReportV1 {
    pub schema_version: u16,
    pub guard: BooleanModelExecutionProfileEvidenceV1,
    pub status: String,
    pub reason: String,
    pub previous_profile_rank: Option<u32>,
    pub final_profile_rank: Option<u32>,
    pub committed: Option<bool>,
    pub rolled_back: Option<bool>,
    pub verification: Option<String>,
    pub events: Vec<String>,
    pub model_cycle_evidence_json: Option<String>,
}

struct EnvelopeAvailablePredicate<'a> {
    key: PredicateKey,
    planner: &'a ModelExecutionAdaptivePlannerV1,
}

impl EnvelopeAvailablePredicate<'_> {
    fn signal_is_bound(input: &PredicateEvaluationInput<'_>, signal: ObservationSignalId) -> bool {
        let Some(observation) = input.observations().get(signal.clone()) else {
            return false;
        };
        if !observation.is_valid() || !observation.value().is_finite() {
            return false;
        }
        if input
            .now()
            .checked_duration_since(*observation.timestamp())
            .is_none()
        {
            return false;
        }
        let Some(context_value) = input.planning_context().get(signal) else {
            return false;
        };
        context_value.is_finite() && context_value.to_bits() == observation.value().to_bits()
    }
}

impl PredicateEvaluator for EnvelopeAvailablePredicate<'_> {
    fn key(&self) -> &PredicateKey {
        &self.key
    }

    fn evaluate(&self, input: &PredicateEvaluationInput<'_>) -> TruthValue {
        if !Self::signal_is_bound(input, ObservationSignalId::FREE_CAPACITY)
            || !Self::signal_is_bound(input, ObservationSignalId::UTILIZATION)
        {
            return TruthValue::Unknown;
        }

        let snapshot = match self
            .planner
            .resource_snapshot_from_context(input.planning_context())
        {
            Ok(snapshot) => snapshot,
            Err(_) => return TruthValue::Unknown,
        };
        if self
            .planner
            .policy()
            .rules()
            .iter()
            .any(|rule| rule.matches(&snapshot))
        {
            TruthValue::True
        } else {
            TruthValue::False
        }
    }
}

/// Current-state BE14c controller.
///
/// One physical observation is captured per cycle. The guard and, on `True`,
/// the existing forecast/planner/runtime path consume that same captured data.
pub struct BooleanModelExecutionProfileControllerV1<B, T> {
    inner: ModelExecutionControllerV1<B, T, CurrentStateForecaster>,
    guarded_resource: EirGuardedResource,
    next_observation_epoch: u64,
    resource_generation: u64,
}

impl<B, T> BooleanModelExecutionProfileControllerV1<B, T>
where
    B: ModelExecutionProfileBackendV1,
    T: ModelExecutionResourceTelemetryV1,
{
    /// Assemble one current-state Boolean-gated model-execution controller.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        resource_id: &str,
        profiles: ModelExecutionProfileSetV1,
        policy: ModelExecutionEnvelopePolicyV1,
        backend: B,
        telemetry: T,
        cadence: CadenceConfig,
        mode: ExecutionModeConfig,
    ) -> Result<Self, RuntimeError> {
        let predicate = model_execution_envelope_predicate_key();
        let registry = PredicateRegistry::from_keys([predicate.clone()])
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let predicate_id = registry
            .id(&predicate)
            .expect("registry contains the BE14c predicate");
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: model_execution_profile_dimension(),
            },
            registry,
            BoolExpr::atom(predicate_id),
        )
        .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let guarded_spec = GuardedResourceSpec::new(
            profiles
                .atomic_resource_spec(resource_id)
                .map_err(|error| RuntimeError::configuration(error.to_string()))?,
            vec![guard],
        )
        .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let guarded_resource = lower_guarded(&guarded_spec)
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;

        let inner = ModelExecutionControllerV1::current_state(
            resource_id,
            profiles,
            policy,
            backend,
            telemetry,
            cadence,
            mode,
        )?;
        Ok(Self {
            inner,
            guarded_resource,
            next_observation_epoch: 1,
            resource_generation: 1,
        })
    }

    /// Borrow the existing authoritative controller.
    #[must_use]
    pub const fn inner(&self) -> &ModelExecutionControllerV1<B, T, CurrentStateForecaster> {
        &self.inner
    }

    /// Current physical profile rank from the bound backend.
    pub fn current_profile_rank(&self) -> Result<u32, RuntimeError> {
        self.inner.current_profile_rank()
    }

    /// Execute one guarded current-state profile cycle.
    pub fn cycle(&mut self) -> Result<BooleanModelExecutionProfileReportV1, RuntimeError> {
        let (current, observations) = self.inner.observer().observe();
        self.cycle_from_observations(current, observations, Instant::now())
    }

    fn cycle_from_observations(
        &mut self,
        current: PlanningContext,
        observations: Vec<Observation>,
        now: Instant,
    ) -> Result<BooleanModelExecutionProfileReportV1, RuntimeError> {
        let observation_snapshot = ObservationSnapshot::new(now, observations.clone());
        let forecast = CurrentStateForecaster.forecast(&observation_snapshot, &current)?;
        let forecast_context = forecast.planning_context().ok_or_else(|| {
            RuntimeError::planning("BE14c current-state forecast produced no planner context")
        })?;

        let epoch = self.next_epoch()?;
        let generation = ResourceGeneration::new(self.resource_generation);
        let key = model_execution_envelope_predicate_key();
        let evaluator = EnvelopeAvailablePredicate {
            key: key.clone(),
            planner: self.inner.planner(),
        };
        let input = PredicateEvaluationInput::new(forecast_context, &observation_snapshot, now);
        let facts = FactSnapshot::derive(
            FactSourceId::new("elastic-runtime:be14c-model-envelope")
                .map_err(|error| RuntimeError::planning(error.to_string()))?,
            epoch,
            Some(FactResourceBinding::new(
                self.guarded_resource.resource().identity().clone(),
                generation,
            )),
            &input,
            &[&evaluator as &dyn PredicateEvaluator],
        )
        .map_err(|error| RuntimeError::planning(error.to_string()))?;
        let freshness = FreshnessSnapshot::new(PlannerEpoch::new(epoch.get()), epoch)
            .with_resource_generation(
                self.guarded_resource.resource().identity().clone(),
                generation,
            );
        let truth = facts.truth(&key);
        let pruning = BooleanGuardPreplanner
            .prune(&self.guarded_resource, &facts, &freshness)
            .map_err(|error| RuntimeError::planning(error.to_string()))?;

        let eligible = pruning.contains_eligible(
            TransitionMechanism::Reinterpret,
            &model_execution_profile_dimension(),
        );
        match truth {
            TruthValue::True if eligible => {
                self.execute_true_cycle(current, observations, &forecast, &facts, &freshness)
            }
            TruthValue::True => Err(RuntimeError::planning(
                "BE14c guard was true but the atomic profile transition was not eligible",
            )),
            TruthValue::False | TruthValue::Unknown => {
                self.blocked_report(truth, &forecast, &facts, &freshness, &current)
            }
        }
    }

    fn execute_true_cycle(
        &mut self,
        current: PlanningContext,
        observations: Vec<Observation>,
        forecast: &crate::Forecast,
        facts: &FactSnapshot,
        freshness: &FreshnessSnapshot,
    ) -> Result<BooleanModelExecutionProfileReportV1, RuntimeError> {
        let previous_profile_rank = observed_profile_rank(&current);
        let (result, model_evidence) = self
            .inner
            .cycle_from_observations_with_evidence(current, observations)?;

        let selected = result
            .transaction
            .plan
            .as_ref()
            .and_then(|validated| validated.plan.candidate());
        let trace = capture_decision_trace(&self.guarded_resource, facts, freshness, selected)
            .map_err(|error| RuntimeError::planning(error.to_string()))?;
        let decision_trace_json = trace
            .to_bounded_json()
            .map_err(|error| RuntimeError::planning(error.to_string()))?;

        let committed = result.transaction.commit.is_some();
        let rolled_back = result.transaction.rollback.is_some();
        let final_profile_rank = model_evidence.final_profile_rank();
        if committed && previous_profile_rank != Some(final_profile_rank) {
            self.resource_generation = self
                .resource_generation
                .checked_add(1)
                .ok_or_else(|| RuntimeError::commit("BE14c resource generation exhausted"))?;
        }

        let (status, reason) = if committed {
            ("committed", "verified-model-profile-commit")
        } else if rolled_back {
            ("rolled-back", "verified-model-profile-rollback")
        } else {
            ("no-change", "trusted-runtime-no-profile-change")
        };
        Ok(BooleanModelExecutionProfileReportV1 {
            schema_version: 1,
            guard: self.guard_evidence(TruthValue::True, forecast, decision_trace_json)?,
            status: status.to_owned(),
            reason: reason.to_owned(),
            previous_profile_rank,
            final_profile_rank: Some(final_profile_rank),
            committed: Some(committed),
            rolled_back: Some(rolled_back),
            verification: result
                .transaction
                .verification
                .as_ref()
                .map(|value| format!("{value:?}")),
            events: result
                .events()
                .map(|event| format!("{:?}: {}", event.kind, event.details))
                .collect(),
            model_cycle_evidence_json: Some(model_evidence.to_pretty_json()?),
        })
    }

    fn blocked_report(
        &self,
        truth: TruthValue,
        forecast: &crate::Forecast,
        facts: &FactSnapshot,
        freshness: &FreshnessSnapshot,
        current: &PlanningContext,
    ) -> Result<BooleanModelExecutionProfileReportV1, RuntimeError> {
        let trace = capture_decision_trace(&self.guarded_resource, facts, freshness, None)
            .map_err(|error| RuntimeError::planning(error.to_string()))?;
        let decision_trace_json = trace
            .to_bounded_json()
            .map_err(|error| RuntimeError::planning(error.to_string()))?;
        let rank = observed_profile_rank(current);
        let reason = match truth {
            TruthValue::False => "boolean-model-envelope-false",
            TruthValue::Unknown => "boolean-model-envelope-unknown",
            TruthValue::True => "boolean-model-envelope-internal-error",
        };
        Ok(BooleanModelExecutionProfileReportV1 {
            schema_version: 1,
            guard: self.guard_evidence(truth, forecast, decision_trace_json)?,
            status: "rejected".to_owned(),
            reason: reason.to_owned(),
            previous_profile_rank: rank,
            final_profile_rank: rank,
            committed: Some(false),
            rolled_back: Some(false),
            verification: None,
            events: Vec::new(),
            model_cycle_evidence_json: None,
        })
    }

    fn guard_evidence(
        &self,
        truth: TruthValue,
        forecast: &crate::Forecast,
        decision_trace_json: String,
    ) -> Result<BooleanModelExecutionProfileEvidenceV1, RuntimeError> {
        Ok(BooleanModelExecutionProfileEvidenceV1 {
            schema_version: 1,
            predicate_key: model_execution_envelope_predicate_key().to_string(),
            free_capacity_signal: ObservationSignalId::FREE_CAPACITY.as_str().to_owned(),
            free_capacity_unit: self.inner.planner().capacity_unit().to_owned(),
            utilization_signal: ObservationSignalId::UTILIZATION.as_str().to_owned(),
            utilization_unit: MODEL_EXECUTION_UTILIZATION_SOURCE_UNIT.to_owned(),
            truth: truth_text(truth).to_owned(),
            forecast_method: forecast.method.clone(),
            forecast_horizon_milliseconds: u64::try_from(forecast.horizon.as_millis()).map_err(
                |_| RuntimeError::planning("BE14c forecast horizon exceeds u64 milliseconds"),
            )?,
            forecast_confidence_claimed: forecast.confidence.is_some(),
            decision_trace_json,
        })
    }

    fn next_epoch(&mut self) -> Result<ObservationEpoch, RuntimeError> {
        let epoch = ObservationEpoch::new(self.next_observation_epoch);
        self.next_observation_epoch = self
            .next_observation_epoch
            .checked_add(1)
            .ok_or_else(|| RuntimeError::planning("BE14c observation epoch exhausted"))?;
        Ok(epoch)
    }
}

fn observed_profile_rank(context: &PlanningContext) -> Option<u32> {
    let value = context.get(model_execution_current_profile_rank_signal())?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > f64::from(u32::MAX) {
        return None;
    }
    Some(value as u32)
}

const fn truth_text(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
}
