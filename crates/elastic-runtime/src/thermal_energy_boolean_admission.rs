//! BE14h source-bound thermal/energy eligibility over real observation signals.
//!
//! This module is planning-only. It combines two explicit predicates:
//! `thermal-margin >= minimum_margin` and `energy-rate <= maximum_power`.
//! Both predicates require exact observation provenance, freshness, and
//! bit-identical planner/observation values. `False` and `Unknown` prune the
//! declared transition. `True` only establishes eligibility; it never
//! authorizes a clock, fan, power-mode, scheduler, or device mutation.

use std::time::{Duration, Instant};

use elastic_core::resource::{
    CapabilityRequirement, DimensionId, ObservationSignalId, ResourceSpec,
};
use elastic_core::{
    BoolExpr, BooleanGuard, FreshnessSnapshot, GuardFactSource, GuardScope, GuardedResourceSpec,
    ObservationEpoch, PlannerEpoch, PredicateKey, PredicateRegistry, ResourceGeneration,
    TransitionMechanism, TruthValue,
};
use elastic_eir::{lower_guarded, EirGuardedResource, PlanningContext, TransitionCandidate};
use serde::{Deserialize, Serialize};

use crate::{
    capture_decision_trace, BooleanGuardPreplanner, CurrentStateForecaster, FactResourceBinding,
    FactSnapshot, FactSourceId, Forecaster, ObservationSnapshot, ObservationSource,
    ObservationThresholdPredicate, PredicateEvaluationInput, PredicateEvaluator,
    ThresholdComparison, ENERGY_RATE_SOURCE_UNIT, THERMAL_MARGIN_SOURCE_UNIT,
};

/// Predicate namespace for BE14h thermal/energy policy.
pub const THERMAL_ENERGY_PREDICATE_NAMESPACE: &str = "elastic.thermal-energy";
/// Stable thermal-margin predicate name.
pub const THERMAL_MARGIN_SUFFICIENT_PREDICATE_NAME: &str = "thermal-margin-sufficient";
/// Stable direct-power predicate name.
pub const ENERGY_RATE_WITHIN_BUDGET_PREDICATE_NAME: &str = "energy-rate-within-budget";
/// Default freshness envelope for live host telemetry.
pub const THERMAL_ENERGY_MAX_AGE: Duration = Duration::from_secs(1);

/// Stable key for `thermal-margin >= configured minimum`.
pub fn thermal_margin_sufficient_predicate_key() -> PredicateKey {
    PredicateKey::new(
        THERMAL_ENERGY_PREDICATE_NAMESPACE,
        THERMAL_MARGIN_SUFFICIENT_PREDICATE_NAME,
    )
    .expect("static BE14h thermal predicate key is valid")
}

/// Stable key for `energy-rate <= configured maximum`.
pub fn energy_rate_within_budget_predicate_key() -> PredicateKey {
    PredicateKey::new(
        THERMAL_ENERGY_PREDICATE_NAMESPACE,
        ENERGY_RATE_WITHIN_BUDGET_PREDICATE_NAME,
    )
    .expect("static BE14h energy predicate key is valid")
}

/// Overall BE14h eligibility state.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BooleanThermalEnergyStatusV1 {
    /// Both source-bound predicates are `True` and the declared transition survived pruning.
    Eligible,
    /// At least one predicate is conclusively `False`.
    Rejected,
    /// Required evidence remains `Unknown`.
    InsufficientEvidence,
}

/// Explanatory, non-authoritative evidence for one thermal/energy decision.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BooleanThermalEnergyEvidenceV1 {
    pub schema_version: u16,
    pub thermal_predicate_key: String,
    pub energy_predicate_key: String,
    pub thermal_source: String,
    pub energy_source: String,
    pub thermal_source_unit: String,
    pub energy_source_unit: String,
    pub minimum_thermal_margin_celsius: f64,
    pub maximum_energy_rate_watts: f64,
    pub thermal_truth: String,
    pub energy_truth: String,
    pub combined_truth: String,
    pub forecast_method: String,
    pub forecast_horizon_milliseconds: u64,
    pub forecast_confidence_claimed: bool,
    pub decision_trace_json: String,
}

/// Planning-only BE14h report.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BooleanThermalEnergyReportV1 {
    pub schema_version: u16,
    pub status: BooleanThermalEnergyStatusV1,
    pub reason: String,
    pub evidence: BooleanThermalEnergyEvidenceV1,
}

struct SourceBoundThresholdPredicate {
    inner: ObservationThresholdPredicate,
    signal: ObservationSignalId,
    expected_source: ObservationSource,
}

impl SourceBoundThresholdPredicate {
    fn new(
        key: PredicateKey,
        signal: ObservationSignalId,
        expected_source: ObservationSource,
        comparison: ThresholdComparison,
        threshold: f64,
        max_age: Duration,
    ) -> Result<Self, String> {
        let inner =
            ObservationThresholdPredicate::new(key, signal.clone(), comparison, threshold, max_age)
                .map_err(|error| error.to_string())?;
        Ok(Self {
            inner,
            signal,
            expected_source,
        })
    }
}

impl PredicateEvaluator for SourceBoundThresholdPredicate {
    fn key(&self) -> &PredicateKey {
        self.inner.key()
    }

    fn evaluate(&self, input: &PredicateEvaluationInput<'_>) -> TruthValue {
        let mut matching = input.observations().iter().filter(|observation| {
            observation.signal() == &self.signal && observation.source() == &self.expected_source
        });
        let Some(observation) = matching.next() else {
            return TruthValue::Unknown;
        };
        if matching.next().is_some() || !observation.is_valid() || !observation.value().is_finite()
        {
            return TruthValue::Unknown;
        }
        let Some(context_value) = input.planning_context().get(self.signal.clone()) else {
            return TruthValue::Unknown;
        };
        if !context_value.is_finite() || context_value.to_bits() != observation.value().to_bits() {
            return TruthValue::Unknown;
        }
        let filtered_observations =
            ObservationSnapshot::new(input.observations().timestamp, vec![observation.clone()]);
        let filtered_input = PredicateEvaluationInput::new(
            input.planning_context(),
            &filtered_observations,
            input.now(),
        );
        self.inner.evaluate(&filtered_input)
    }
}

/// Planning-only BE14h preplanner for one already-declared resource transition.
pub struct BooleanThermalEnergyPreplannerV1 {
    guarded_resource: EirGuardedResource,
    mechanism: TransitionMechanism,
    dimension: DimensionId,
    minimum_thermal_margin_celsius: f64,
    maximum_energy_rate_watts: f64,
    thermal_source: ObservationSource,
    energy_source: ObservationSource,
    max_age: Duration,
}

impl BooleanThermalEnergyPreplannerV1 {
    /// Bind thermal/energy eligibility to one declared transition.
    ///
    /// Thresholds are policy inputs supplied by the application/operator.
    /// ElasticXxx does not claim that any particular value is a safe thermal or
    /// energy limit. Negative minimum thermal margins and negative power budgets
    /// are rejected fail-closed.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spec: ResourceSpec,
        mechanism: TransitionMechanism,
        dimension: DimensionId,
        minimum_thermal_margin_celsius: f64,
        maximum_energy_rate_watts: f64,
        thermal_source: ObservationSource,
        energy_source: ObservationSource,
        max_age: Duration,
    ) -> Result<Self, String> {
        if !minimum_thermal_margin_celsius.is_finite() || minimum_thermal_margin_celsius < 0.0 {
            return Err("BE14h minimum thermal margin must be finite and non-negative".into());
        }
        if !maximum_energy_rate_watts.is_finite() || maximum_energy_rate_watts < 0.0 {
            return Err("BE14h maximum energy rate must be finite and non-negative".into());
        }
        if max_age.is_zero() {
            return Err("BE14h observation freshness bound must be non-zero".into());
        }
        if !spec.admits(mechanism, &dimension) {
            return Err("BE14h resource does not admit the selected transition".into());
        }
        let capability = CapabilityRequirement::new(mechanism, dimension.clone());
        if !spec.requires_capability(&capability) {
            return Err("BE14h selected transition lacks a matching capability requirement".into());
        }
        for signal in [
            ObservationSignalId::THERMAL_MARGIN,
            ObservationSignalId::ENERGY_RATE,
        ] {
            if !spec.observed_signals().contains(&signal) {
                return Err(format!(
                    "BE14h resource must declare observation signal {}",
                    signal.as_str()
                ));
            }
        }

        let thermal_key = thermal_margin_sufficient_predicate_key();
        let energy_key = energy_rate_within_budget_predicate_key();
        let registry = PredicateRegistry::from_keys([thermal_key.clone(), energy_key.clone()])
            .map_err(|error| error.to_string())?;
        let thermal_id = registry
            .id(&thermal_key)
            .expect("registry contains BE14h thermal predicate");
        let energy_id = registry
            .id(&energy_key)
            .expect("registry contains BE14h energy predicate");
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism,
                dimension: dimension.clone(),
            },
            registry,
            BoolExpr::all([BoolExpr::atom(thermal_id), BoolExpr::atom(energy_id)]),
        )
        .map_err(|error| error.to_string())?;
        let guarded_resource = lower_guarded(
            &GuardedResourceSpec::new(spec, vec![guard]).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;

        Ok(Self {
            guarded_resource,
            mechanism,
            dimension,
            minimum_thermal_margin_celsius,
            maximum_energy_rate_watts,
            thermal_source,
            energy_source,
            max_age,
        })
    }

    /// Evaluate source-bound current observations and capture a durable decision trace.
    ///
    /// `observation_epoch` and `resource_generation` are trusted provenance inputs
    /// supplied by the caller that owns the observation/resource lifecycle. This
    /// preplanner never synthesizes either identity from local call order.
    ///
    /// This method performs no actuation and grants no validation authority.
    pub fn evaluate(
        &self,
        planning_context: &PlanningContext,
        observations: &ObservationSnapshot,
        now: Instant,
        observation_epoch: ObservationEpoch,
        resource_generation: ResourceGeneration,
    ) -> Result<BooleanThermalEnergyReportV1, String> {
        let forecast = CurrentStateForecaster
            .forecast(observations, planning_context)
            .map_err(|error| error.to_string())?;
        let forecast_context = forecast.planning_context().ok_or_else(|| {
            "BE14h current-state forecast produced no planning context".to_owned()
        })?;
        let input = PredicateEvaluationInput::new(forecast_context, observations, now);
        let thermal_key = thermal_margin_sufficient_predicate_key();
        let energy_key = energy_rate_within_budget_predicate_key();
        let thermal = SourceBoundThresholdPredicate::new(
            thermal_key.clone(),
            ObservationSignalId::THERMAL_MARGIN,
            self.thermal_source.clone(),
            ThresholdComparison::GreaterOrEqual,
            self.minimum_thermal_margin_celsius,
            self.max_age,
        )?;
        let energy = SourceBoundThresholdPredicate::new(
            energy_key.clone(),
            ObservationSignalId::ENERGY_RATE,
            self.energy_source.clone(),
            ThresholdComparison::LessOrEqual,
            self.maximum_energy_rate_watts,
            self.max_age,
        )?;
        let facts = FactSnapshot::derive(
            FactSourceId::new("elastic-runtime:be14h-thermal-energy")
                .map_err(|error| error.to_string())?,
            observation_epoch,
            Some(FactResourceBinding::new(
                self.guarded_resource.resource().identity().clone(),
                resource_generation,
            )),
            &input,
            &[
                &thermal as &dyn PredicateEvaluator,
                &energy as &dyn PredicateEvaluator,
            ],
        )
        .map_err(|error| error.to_string())?;
        let freshness = FreshnessSnapshot::new(
            PlannerEpoch::new(observation_epoch.get()),
            observation_epoch,
        )
        .with_resource_generation(
            self.guarded_resource.resource().identity().clone(),
            resource_generation,
        );
        let thermal_truth = facts.truth(&thermal_key);
        let energy_truth = facts.truth(&energy_key);
        let combined_truth = thermal_truth.kleene_and(energy_truth);
        let pruning = BooleanGuardPreplanner
            .prune(&self.guarded_resource, &facts, &freshness)
            .map_err(|error| error.to_string())?;
        let declared_candidate = self
            .guarded_resource
            .resource()
            .transitions()
            .iter()
            .find(|entry| {
                entry.transition().mechanism() == self.mechanism
                    && entry.transition().dimension() == &self.dimension
            })
            .map(TransitionCandidate::from_admitted);
        let selected_for_trace = match combined_truth {
            TruthValue::True if pruning.contains_eligible(self.mechanism, &self.dimension) => {
                declared_candidate.as_ref()
            }
            TruthValue::True => {
                return Err(
                    "BE14h guard was true but the declared transition was not eligible".into(),
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

        let (status, reason) = match combined_truth {
            TruthValue::True => (
                BooleanThermalEnergyStatusV1::Eligible,
                "thermal-energy-eligible",
            ),
            TruthValue::False => (
                BooleanThermalEnergyStatusV1::Rejected,
                "thermal-energy-rejected",
            ),
            TruthValue::Unknown => (
                BooleanThermalEnergyStatusV1::InsufficientEvidence,
                "thermal-energy-evidence-unknown",
            ),
        };

        Ok(BooleanThermalEnergyReportV1 {
            schema_version: 1,
            status,
            reason: reason.to_owned(),
            evidence: BooleanThermalEnergyEvidenceV1 {
                schema_version: 1,
                thermal_predicate_key: thermal_key.to_string(),
                energy_predicate_key: energy_key.to_string(),
                thermal_source: self.thermal_source.to_string(),
                energy_source: self.energy_source.to_string(),
                thermal_source_unit: THERMAL_MARGIN_SOURCE_UNIT.to_owned(),
                energy_source_unit: ENERGY_RATE_SOURCE_UNIT.to_owned(),
                minimum_thermal_margin_celsius: self.minimum_thermal_margin_celsius,
                maximum_energy_rate_watts: self.maximum_energy_rate_watts,
                thermal_truth: truth_text(thermal_truth).to_owned(),
                energy_truth: truth_text(energy_truth).to_owned(),
                combined_truth: truth_text(combined_truth).to_owned(),
                forecast_method: forecast.method.clone(),
                forecast_horizon_milliseconds: u64::try_from(forecast.horizon.as_millis())
                    .map_err(|_| "BE14h forecast horizon exceeds u64 milliseconds".to_owned())?,
                forecast_confidence_claimed: forecast.confidence.is_some(),
                decision_trace_json,
            },
        })
    }
}

fn truth_text(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecisionTrace, Observation};
    use elastic_core::resource::{AdmissibleTransition, LogicalResourceId, ResourceClassId};

    fn spec() -> ResourceSpec {
        ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("be14h-policy-test").unwrap(),
        )
        .allow(DimensionId::ENERGY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
        ))
        .observe(ObservationSignalId::THERMAL_MARGIN)
        .observe(ObservationSignalId::ENERGY_RATE)
        .build()
        .unwrap()
    }

    fn sources() -> (ObservationSource, ObservationSource) {
        (
            ObservationSource::host("test:thermal"),
            ObservationSource::host("test:power"),
        )
    }

    fn input(
        thermal: Option<(ObservationSource, f64)>,
        energy: Option<(ObservationSource, f64)>,
        observed_at: Instant,
    ) -> (PlanningContext, ObservationSnapshot) {
        let mut context = PlanningContext::new();
        let mut observations = Vec::new();
        if let Some((source, value)) = thermal {
            if value.is_finite() {
                context = context.observe(ObservationSignalId::THERMAL_MARGIN, value);
                observations.push(Observation::from_source(
                    source,
                    ObservationSignalId::THERMAL_MARGIN,
                    value,
                    observed_at,
                ));
            } else {
                observations.push(Observation::unsupported_from_source(
                    source,
                    ObservationSignalId::THERMAL_MARGIN,
                    observed_at,
                    "thermal unavailable",
                ));
            }
        }
        if let Some((source, value)) = energy {
            if value.is_finite() {
                context = context.observe(ObservationSignalId::ENERGY_RATE, value);
                observations.push(Observation::from_source(
                    source,
                    ObservationSignalId::ENERGY_RATE,
                    value,
                    observed_at,
                ));
            } else {
                observations.push(Observation::unsupported_from_source(
                    source,
                    ObservationSignalId::ENERGY_RATE,
                    observed_at,
                    "power unavailable",
                ));
            }
        }
        (context, ObservationSnapshot::new(observed_at, observations))
    }

    fn planner() -> BooleanThermalEnergyPreplannerV1 {
        let (thermal, energy) = sources();
        BooleanThermalEnergyPreplannerV1::new(
            spec(),
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
            10.0,
            50.0,
            thermal,
            energy,
            THERMAL_ENERGY_MAX_AGE,
        )
        .unwrap()
    }

    #[test]
    fn true_policy_selects_declared_candidate_and_captures_trace() {
        let now = Instant::now();
        let (thermal_source, energy_source) = sources();
        let (context, observations) = input(
            Some((thermal_source, 20.0)),
            Some((energy_source, 40.0)),
            now,
        );
        let report = planner()
            .evaluate(
                &context,
                &observations,
                now,
                ObservationEpoch::new(7),
                ResourceGeneration::new(3),
            )
            .unwrap();
        assert_eq!(report.status, BooleanThermalEnergyStatusV1::Eligible);
        assert_eq!(report.evidence.thermal_truth, "true");
        assert_eq!(report.evidence.energy_truth, "true");
        assert_eq!(report.evidence.combined_truth, "true");
        assert_eq!(report.evidence.forecast_method, "current-state");
        assert_eq!(report.evidence.forecast_horizon_milliseconds, 0);
        assert!(!report.evidence.forecast_confidence_claimed);
        let trace =
            DecisionTrace::from_bounded_json(report.evidence.decision_trace_json.as_bytes())
                .unwrap();
        assert!(trace.selected().is_some());
        assert_eq!(trace.observation_epoch(), ObservationEpoch::new(7));
        assert_eq!(trace.resource_generation(), ResourceGeneration::new(3));
    }

    #[test]
    fn false_policy_prunes_candidate() {
        let now = Instant::now();
        let (thermal_source, energy_source) = sources();
        let (context, observations) = input(
            Some((thermal_source, 5.0)),
            Some((energy_source, 40.0)),
            now,
        );
        let report = planner()
            .evaluate(
                &context,
                &observations,
                now,
                ObservationEpoch::new(7),
                ResourceGeneration::new(3),
            )
            .unwrap();
        assert_eq!(report.status, BooleanThermalEnergyStatusV1::Rejected);
        assert_eq!(report.evidence.thermal_truth, "false");
        assert_eq!(report.evidence.combined_truth, "false");
        let trace =
            DecisionTrace::from_bounded_json(report.evidence.decision_trace_json.as_bytes())
                .unwrap();
        assert!(trace.selected().is_none());
    }

    #[test]
    fn missing_or_foreign_source_stays_unknown() {
        let now = Instant::now();
        let (thermal_source, _energy_source) = sources();
        let (context, observations) = input(
            Some((thermal_source, 20.0)),
            Some((ObservationSource::host("foreign:power"), 40.0)),
            now,
        );
        let report = planner()
            .evaluate(
                &context,
                &observations,
                now,
                ObservationEpoch::new(7),
                ResourceGeneration::new(3),
            )
            .unwrap();
        assert_eq!(
            report.status,
            BooleanThermalEnergyStatusV1::InsufficientEvidence
        );
        assert_eq!(report.evidence.energy_truth, "unknown");
        assert_eq!(report.evidence.combined_truth, "unknown");
    }

    #[test]
    fn earlier_foreign_unsupported_observation_does_not_hide_bound_source() {
        let now = Instant::now();
        let (thermal_source, energy_source) = sources();
        let context = PlanningContext::new()
            .observe(ObservationSignalId::THERMAL_MARGIN, 20.0)
            .observe(ObservationSignalId::ENERGY_RATE, 40.0);
        let observations = ObservationSnapshot::new(
            now,
            vec![
                Observation::unsupported_from_source(
                    ObservationSource::host("foreign:power"),
                    ObservationSignalId::ENERGY_RATE,
                    now,
                    "foreign provider unavailable",
                ),
                Observation::from_source(
                    thermal_source,
                    ObservationSignalId::THERMAL_MARGIN,
                    20.0,
                    now,
                ),
                Observation::from_source(
                    energy_source,
                    ObservationSignalId::ENERGY_RATE,
                    40.0,
                    now,
                ),
            ],
        );

        let report = planner()
            .evaluate(
                &context,
                &observations,
                now,
                ObservationEpoch::new(7),
                ResourceGeneration::new(3),
            )
            .unwrap();
        assert_eq!(report.status, BooleanThermalEnergyStatusV1::Eligible);
        assert_eq!(report.evidence.energy_truth, "true");
    }

    #[test]
    fn ungrounded_declared_transition_is_rejected_at_construction() {
        let ungrounded = ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("be14h-ungrounded").unwrap(),
        )
        .allow(DimensionId::ENERGY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
        ))
        .observe(ObservationSignalId::THERMAL_MARGIN)
        .observe(ObservationSignalId::ENERGY_RATE)
        .build()
        .unwrap();
        let (thermal_source, energy_source) = sources();

        assert!(BooleanThermalEnergyPreplannerV1::new(
            ungrounded,
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
            10.0,
            50.0,
            thermal_source,
            energy_source,
            THERMAL_ENERGY_MAX_AGE,
        )
        .is_err());
    }

    #[test]
    fn stale_source_stays_unknown() {
        let now = Instant::now();
        let old = now.checked_sub(Duration::from_secs(2)).unwrap();
        let (thermal_source, energy_source) = sources();
        let (context, observations) = input(
            Some((thermal_source, 20.0)),
            Some((energy_source, 40.0)),
            old,
        );
        let report = planner()
            .evaluate(
                &context,
                &observations,
                now,
                ObservationEpoch::new(7),
                ResourceGeneration::new(3),
            )
            .unwrap();
        assert_eq!(
            report.status,
            BooleanThermalEnergyStatusV1::InsufficientEvidence
        );
        assert_eq!(report.evidence.combined_truth, "unknown");
    }

    #[test]
    fn numeric_context_drift_stays_unknown() {
        let now = Instant::now();
        let (thermal_source, energy_source) = sources();
        let (_, observations) = input(
            Some((thermal_source, 20.0)),
            Some((energy_source, 40.0)),
            now,
        );
        let context = PlanningContext::new()
            .observe(ObservationSignalId::THERMAL_MARGIN, 20.0)
            .observe(ObservationSignalId::ENERGY_RATE, 39.0);
        let report = planner()
            .evaluate(
                &context,
                &observations,
                now,
                ObservationEpoch::new(7),
                ResourceGeneration::new(3),
            )
            .unwrap();
        assert_eq!(
            report.status,
            BooleanThermalEnergyStatusV1::InsufficientEvidence
        );
        assert_eq!(report.evidence.energy_truth, "unknown");
    }

    #[test]
    fn invalid_policy_thresholds_fail_closed() {
        let (thermal, energy) = sources();
        for (margin, power) in [(f64::NAN, 50.0), (-1.0, 50.0), (10.0, -1.0)] {
            assert!(BooleanThermalEnergyPreplannerV1::new(
                spec(),
                TransitionMechanism::Reinterpret,
                DimensionId::ENERGY,
                margin,
                power,
                thermal.clone(),
                energy.clone(),
                THERMAL_ENERGY_MAX_AGE,
            )
            .is_err());
        }
    }
}
