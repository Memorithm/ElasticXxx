//! BE14b Boolean eligibility for live concurrency-width adaptation.
//!
//! The stable `elastic.concurrency::target-holds-active-permits` predicate is a
//! fail-closed precheck over the live `active-permits` signal, measured in
//! permits. It only establishes whether `active_permits <= target_width` at the
//! observation instant. `False` and `Unknown` stop before numeric planning and
//! actuation. `True` still flows through the existing trusted transactional
//! concurrency adapter, which revalidates live-holder and width invariants
//! immediately before actuation, verifies the applied width, and commits or
//! rolls back through the ordinary runtime state machine.
//!
//! The Boolean trace is explanatory evidence only. It cannot authorize a resize
//! and it never replaces trusted adapter validation.

use std::time::{Duration, Instant};

use elastic_adapters::ConcurrencyPermits;
use elastic_core::resource::DimensionId;
use elastic_core::{
    BoolExpr, BooleanGuard, FreshnessSnapshot, GuardFactSource, GuardScope, GuardedResourceSpec,
    ObservationEpoch, PlannerEpoch, PredicateKey, PredicateRegistry, ResourceGeneration,
    TransitionMechanism, TruthValue,
};
use elastic_eir::{
    lower_guarded, EirGuardedResource, EirResource, PlanOutcome, TransitionCandidate,
    TransitionPlanner,
};
use serde::{Deserialize, Serialize};

use crate::{
    active_permits_signal, capture_decision_trace, BooleanGuardPreplanner, CurrentStateForecaster,
    CycleAttempt, FactResourceBinding, FactSnapshot, FactSourceId, Forecaster, ObservationSnapshot,
    ObservationThresholdPredicate, Observer, PlannerConfig, PredicateEvaluationInput,
    PredicateEvaluator, Runtime, RuntimeConfig, RuntimeMode, ThresholdComparison,
    TransactionalConcurrency,
};

/// Namespace of the stable BE14b concurrency predicate.
pub const CONCURRENCY_HEADROOM_PREDICATE_NAMESPACE: &str = "elastic.concurrency";
/// Name of the stable BE14b concurrency predicate.
pub const CONCURRENCY_HEADROOM_PREDICATE_NAME: &str = "target-holds-active-permits";
/// Unit of the source signal and target threshold.
pub const CONCURRENCY_HEADROOM_SOURCE_UNIT: &str = "permits";
/// Freshness envelope for the in-process live observer used by this precheck.
pub const CONCURRENCY_HEADROOM_MAX_AGE: Duration = Duration::from_secs(1);

/// Stable key for `active_permits <= requested_target_width`.
pub fn concurrency_headroom_predicate_key() -> PredicateKey {
    PredicateKey::new(
        CONCURRENCY_HEADROOM_PREDICATE_NAMESPACE,
        CONCURRENCY_HEADROOM_PREDICATE_NAME,
    )
    .expect("static BE14b PredicateKey is valid")
}

/// Durable explanatory evidence for one concurrency resize attempt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanConcurrencyEvidenceV1 {
    pub schema_version: u16,
    pub predicate_key: String,
    pub source_signal: String,
    pub source_unit: String,
    pub target_width: u32,
    pub truth: String,
    pub forecast_method: String,
    pub forecast_horizon_milliseconds: u64,
    pub forecast_confidence_claimed: bool,
    pub decision_trace_json: String,
}

/// Authoritative runtime outcome for one requested concurrency width.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencyResizeReportV1 {
    pub schema_version: u16,
    pub target_width: u32,
    pub previous_width: u32,
    pub final_width: u32,
    pub status: String,
    pub reason: String,
    pub committed: Option<bool>,
    pub rolled_back: Option<bool>,
    pub verification: Option<String>,
    pub events: Vec<String>,
}

/// BE14b report combining Boolean precheck evidence with trusted runtime state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanConcurrencyResizeReportV1 {
    pub schema_version: u16,
    pub guard: BooleanConcurrencyEvidenceV1,
    pub resize: ConcurrencyResizeReportV1,
}

struct TargetWidth(u32);

impl TransitionPlanner for TargetWidth {
    fn propose_transition(&self, resource: &EirResource) -> PlanOutcome {
        match resource.transitions().iter().find(|entry| {
            entry.transition().mechanism() == TransitionMechanism::Reinterpret
                && entry.transition().dimension() == &DimensionId::CONCURRENCY
                && entry.capability_grounded()
        }) {
            Some(entry) => PlanOutcome::Candidate(
                TransitionCandidate::from_admitted(entry).with_magnitude(u64::from(self.0)),
            ),
            None => PlanOutcome::Unsupported,
        }
    }
}

/// Boolean-gated controller for a live transactional concurrency ledger.
///
/// The controller owns no scheduler. Callers enforce the resulting permit width
/// through the shared [`TransactionalConcurrency`] handle returned by
/// [`Self::permits`].
pub struct BooleanConcurrencyResizeControllerV1 {
    permits: TransactionalConcurrency,
    runtime: Runtime,
    resource: EirResource,
    guarded_resource: EirGuardedResource,
    max_width: u32,
    next_observation_epoch: u64,
    resource_generation: u64,
}

impl BooleanConcurrencyResizeControllerV1 {
    /// Construct the controller without performing work or changing width.
    pub fn new(id: &str, max_width: u32, initial_width: u32) -> Result<Self, String> {
        if !(1..=256).contains(&max_width) || initial_width == 0 || initial_width > max_width {
            return Err("invalid BE14b concurrency width bounds".into());
        }
        let declaration = ConcurrencyPermits::new(id, max_width as usize, initial_width as usize)
            .map_err(|error| error.to_string())?;
        let resource = declaration.ir().clone();
        let runtime = Runtime::new(RuntimeConfig {
            resource_spec: declaration.spec().clone(),
            ir_resource: resource.clone(),
            planner_config: PlannerConfig::None,
            mode: RuntimeMode::Apply,
            dry_run: false,
            max_cycles: 1,
            ..RuntimeConfig::default()
        });
        let permits = TransactionalConcurrency::new(id, max_width as usize, initial_width as usize)
            .map_err(|error| error.to_string())?;

        let predicate = concurrency_headroom_predicate_key();
        let registry =
            PredicateRegistry::from_keys([predicate.clone()]).map_err(|error| error.to_string())?;
        let predicate_id = registry
            .id(&predicate)
            .expect("registry contains BE14b predicate");
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CONCURRENCY,
            },
            registry,
            BoolExpr::atom(predicate_id),
        )
        .map_err(|error| error.to_string())?;
        let guarded_resource = lower_guarded(
            &GuardedResourceSpec::new(declaration.spec().clone(), vec![guard])
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;

        Ok(Self {
            permits,
            runtime,
            resource,
            guarded_resource,
            max_width,
            next_observation_epoch: 1,
            resource_generation: 1,
        })
    }

    /// Shared live permit handle used by the embedding executor.
    pub fn permits(&self) -> TransactionalConcurrency {
        self.permits.clone()
    }

    /// Evaluate the Boolean precheck, then delegate any `True` candidate to the
    /// existing trusted runtime transaction.
    pub fn resize(
        &mut self,
        target_width: u32,
    ) -> Result<BooleanConcurrencyResizeReportV1, String> {
        if target_width == 0 || target_width > self.max_width {
            return Err("requested BE14b concurrency width is outside immutable bounds".into());
        }
        let previous_width = u32::try_from(self.permits.width().map_err(|e| e.to_string())?)
            .map_err(|_| "current concurrency width does not fit u32".to_owned())?;
        let epoch = self.next_epoch()?;
        let generation = ResourceGeneration::new(self.resource_generation);
        let (context, observed) = self.permits.observe();
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, observed);
        let forecast = CurrentStateForecaster
            .forecast(&observations, &context)
            .map_err(|error| error.to_string())?;
        let forecast_context = forecast.planning_context().ok_or_else(|| {
            "BE14b current-state forecast produced no planning context".to_owned()
        })?;
        let input = PredicateEvaluationInput::new(forecast_context, &observations, now);
        let predicate_key = concurrency_headroom_predicate_key();
        let evaluator = ObservationThresholdPredicate::new(
            predicate_key.clone(),
            active_permits_signal(),
            ThresholdComparison::LessOrEqual,
            f64::from(target_width),
            CONCURRENCY_HEADROOM_MAX_AGE,
        )
        .map_err(|error| error.to_string())?;
        let facts = FactSnapshot::derive(
            FactSourceId::new("elastic-runtime:be14b-concurrency-headroom")
                .map_err(|error| error.to_string())?,
            epoch,
            Some(FactResourceBinding::new(
                self.guarded_resource.resource().identity().clone(),
                generation,
            )),
            &input,
            &[&evaluator as &dyn PredicateEvaluator],
        )
        .map_err(|error| error.to_string())?;
        let freshness = FreshnessSnapshot::new(PlannerEpoch::new(epoch.get()), epoch)
            .with_resource_generation(
                self.guarded_resource.resource().identity().clone(),
                generation,
            );
        let truth = facts.truth(&predicate_key);
        let pruning = BooleanGuardPreplanner
            .prune(&self.guarded_resource, &facts, &freshness)
            .map_err(|error| error.to_string())?;
        let declared_candidate = self
            .guarded_resource
            .resource()
            .transitions()
            .iter()
            .find(|entry| {
                entry.transition().mechanism() == TransitionMechanism::Reinterpret
                    && entry.transition().dimension() == &DimensionId::CONCURRENCY
            })
            .map(|entry| {
                TransitionCandidate::from_admitted(entry).with_magnitude(u64::from(target_width))
            });
        let selected_for_trace = match truth {
            TruthValue::True
                if pruning.contains_eligible(
                    TransitionMechanism::Reinterpret,
                    &DimensionId::CONCURRENCY,
                ) =>
            {
                declared_candidate.as_ref()
            }
            TruthValue::True => {
                return Err(
                    "BE14b guard was true but concurrency transition was not eligible".into(),
                )
            }
            TruthValue::False | TruthValue::Unknown => None,
        };
        let trace = capture_decision_trace(
            &self.guarded_resource,
            &facts,
            &freshness,
            selected_for_trace,
        )
        .map_err(|error| error.to_string())?;
        let decision_trace_json = trace.to_bounded_json().map_err(|error| error.to_string())?;

        let resize = match truth {
            TruthValue::True => self.run_trusted_resize(target_width, previous_width)?,
            TruthValue::False => blocked_resize(
                target_width,
                previous_width,
                "boolean-concurrency-headroom-false",
            ),
            TruthValue::Unknown => blocked_resize(
                target_width,
                previous_width,
                "boolean-concurrency-headroom-unknown",
            ),
        };

        if resize.committed == Some(true) && resize.final_width != resize.previous_width {
            self.resource_generation = self
                .resource_generation
                .checked_add(1)
                .ok_or_else(|| "BE14b resource generation exhausted".to_owned())?;
        }

        Ok(BooleanConcurrencyResizeReportV1 {
            schema_version: 1,
            guard: BooleanConcurrencyEvidenceV1 {
                schema_version: 1,
                predicate_key: predicate_key.to_string(),
                source_signal: active_permits_signal().as_str().to_owned(),
                source_unit: CONCURRENCY_HEADROOM_SOURCE_UNIT.to_owned(),
                target_width,
                truth: truth_text(truth).to_owned(),
                forecast_method: forecast.method.clone(),
                forecast_horizon_milliseconds: u64::try_from(forecast.horizon.as_millis())
                    .map_err(|_| "BE14b forecast horizon exceeds u64 milliseconds".to_owned())?,
                forecast_confidence_claimed: forecast.confidence.is_some(),
                decision_trace_json,
            },
            resize,
        })
    }

    fn run_trusted_resize(
        &mut self,
        target_width: u32,
        previous_width: u32,
    ) -> Result<ConcurrencyResizeReportV1, String> {
        let observer = self.permits.clone();
        let attempt = self.runtime.cycle_attempt(
            &self.resource,
            &TargetWidth(target_width),
            &observer,
            &mut self.permits,
        );
        let final_width = u32::try_from(self.permits.width().map_err(|e| e.to_string())?)
            .map_err(|_| "final concurrency width does not fit u32".to_owned())?;
        match attempt {
            CycleAttempt::Completed(cycle) => {
                let committed = cycle.commit.is_some();
                if committed && final_width != target_width {
                    return Err("committed BE14b concurrency width disagrees with target".into());
                }
                Ok(ConcurrencyResizeReportV1 {
                    schema_version: 1,
                    target_width,
                    previous_width,
                    final_width,
                    status: if committed { "resized" } else { "rejected" }.into(),
                    reason: if committed {
                        "verified-concurrency-width"
                    } else {
                        "runtime-did-not-commit"
                    }
                    .into(),
                    committed: Some(committed),
                    rolled_back: Some(cycle.rollback.is_some()),
                    verification: cycle.verification.as_ref().map(|v| format!("{v:?}")),
                    events: cycle
                        .events
                        .iter()
                        .map(|event| format!("{:?}: {}", event.kind, event.details))
                        .collect(),
                })
            }
            CycleAttempt::Failed(failure) => Ok(ConcurrencyResizeReportV1 {
                schema_version: 1,
                target_width,
                previous_width,
                final_width,
                status: "rejected".into(),
                reason: format!("runtime-failure: {}", failure.error),
                committed: None,
                rolled_back: None,
                verification: None,
                events: failure
                    .events
                    .iter()
                    .map(|event| format!("{:?}: {}", event.kind, event.details))
                    .collect(),
            }),
        }
    }

    fn next_epoch(&mut self) -> Result<ObservationEpoch, String> {
        let epoch = ObservationEpoch::new(self.next_observation_epoch);
        self.next_observation_epoch = self
            .next_observation_epoch
            .checked_add(1)
            .ok_or_else(|| "BE14b observation epoch exhausted".to_owned())?;
        Ok(epoch)
    }
}

fn blocked_resize(target_width: u32, width: u32, reason: &str) -> ConcurrencyResizeReportV1 {
    ConcurrencyResizeReportV1 {
        schema_version: 1,
        target_width,
        previous_width: width,
        final_width: width,
        status: "rejected".into(),
        reason: reason.into(),
        committed: Some(false),
        rolled_back: Some(false),
        verification: None,
        events: Vec::new(),
    }
}

const fn truth_text(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Observation, ObservationSource};
    use elastic_eir::PlanningContext;

    fn derive_test_truth(
        target: u32,
        context: &PlanningContext,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> TruthValue {
        let evaluator = ObservationThresholdPredicate::new(
            concurrency_headroom_predicate_key(),
            active_permits_signal(),
            ThresholdComparison::LessOrEqual,
            f64::from(target),
            CONCURRENCY_HEADROOM_MAX_AGE,
        )
        .unwrap();
        evaluator.evaluate(&PredicateEvaluationInput::new(context, observations, now))
    }

    #[test]
    fn true_guard_executes_full_trusted_cycle_and_matches_unguarded_runtime() {
        let mut guarded = BooleanConcurrencyResizeControllerV1::new("workers", 8, 4).unwrap();
        let report = guarded.resize(6).unwrap();
        assert_eq!(report.guard.truth, "true");
        assert_eq!(report.guard.forecast_method, "current-state");
        assert_eq!(report.guard.forecast_horizon_milliseconds, 0);
        assert!(!report.guard.forecast_confidence_claimed);
        assert_eq!(report.resize.committed, Some(true));
        assert_eq!(report.resize.final_width, 6);
        assert!(report.resize.verification.is_some());
        assert!(!report.resize.events.is_empty());
        let decoded =
            crate::DecisionTrace::from_bounded_json(report.guard.decision_trace_json.as_bytes())
                .unwrap();
        assert!(decoded.selected().is_some());

        let baseline = TransactionalConcurrency::new("baseline", 8, 4).unwrap();
        let resource = baseline.ir().unwrap();
        let declaration = ConcurrencyPermits::new("baseline", 8, 4).unwrap();
        let runtime = Runtime::new(RuntimeConfig {
            resource_spec: declaration.spec().clone(),
            ir_resource: resource.clone(),
            planner_config: PlannerConfig::None,
            mode: RuntimeMode::Apply,
            dry_run: false,
            max_cycles: 1,
            ..RuntimeConfig::default()
        });
        let observer = baseline.clone();
        let mut actuator = baseline.clone();
        let cycle = runtime
            .cycle(&resource, &TargetWidth(6), &observer, &mut actuator)
            .unwrap();
        assert!(cycle.commit.is_some());
        assert_eq!(
            baseline.width().unwrap(),
            guarded.permits().width().unwrap()
        );
    }

    #[test]
    fn false_guard_blocks_before_planning_when_target_would_strand_holders() {
        let mut guarded = BooleanConcurrencyResizeControllerV1::new("workers", 8, 4).unwrap();
        let permits = guarded.permits();
        permits.acquire().unwrap();
        permits.acquire().unwrap();
        permits.acquire().unwrap();
        let report = guarded.resize(2).unwrap();
        assert_eq!(report.guard.truth, "false");
        assert_eq!(report.resize.reason, "boolean-concurrency-headroom-false");
        assert_eq!(report.resize.final_width, 4);
        assert_eq!(report.resize.committed, Some(false));
        assert!(report.resize.events.is_empty());
        permits.release().unwrap();
        permits.release().unwrap();
        permits.release().unwrap();
    }

    #[test]
    fn missing_invalid_and_stale_evidence_are_unknown() {
        let now = Instant::now();
        let empty = ObservationSnapshot::new(now, Vec::new());
        assert_eq!(
            derive_test_truth(2, &PlanningContext::new(), &empty, now),
            TruthValue::Unknown
        );

        let unsupported = ObservationSnapshot::new(
            now,
            vec![Observation::unsupported_from_source(
                ObservationSource::runtime("test-provider"),
                active_permits_signal(),
                now,
                "counter unavailable",
            )],
        );
        assert_eq!(
            derive_test_truth(2, &PlanningContext::new(), &unsupported, now),
            TruthValue::Unknown
        );

        let old = now.checked_sub(Duration::from_secs(2)).unwrap();
        let stale = ObservationSnapshot::new(
            now,
            vec![Observation::from_source(
                ObservationSource::runtime("test-provider"),
                active_permits_signal(),
                1.0,
                old,
            )],
        );
        let stale_context = PlanningContext::new().observe(active_permits_signal(), 1.0);
        assert_eq!(
            derive_test_truth(2, &stale_context, &stale, now),
            TruthValue::Unknown
        );
    }

    #[test]
    fn trusted_validation_remains_authoritative_after_true_precheck() {
        let mut guarded = BooleanConcurrencyResizeControllerV1::new("workers", 8, 4).unwrap();
        let permits = guarded.permits();
        permits.acquire().unwrap();
        permits.acquire().unwrap();
        let first = guarded.resize(2).unwrap();
        assert_eq!(first.guard.truth, "true");
        assert_eq!(first.resize.committed, Some(true));
        assert_eq!(first.resize.final_width, 2);
        assert!(permits.acquire().is_err());
        permits.release().unwrap();
        permits.release().unwrap();
    }

    #[test]
    fn out_of_bounds_target_fails_before_observation_or_actuation() {
        let mut guarded = BooleanConcurrencyResizeControllerV1::new("workers", 8, 4).unwrap();
        assert!(guarded.resize(0).is_err());
        assert!(guarded.resize(9).is_err());
        assert_eq!(guarded.permits().width().unwrap(), 4);
    }
}
